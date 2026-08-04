use core::marker::PhantomData;

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
#[cfg(feature = "std")]
use std::vec::Vec;

use chacha20poly1305::{
    Nonce,
    aead::AeadInOut,
};
use subtle::ConstantTimeEq;
use zeroize::Zeroize;

use crate::{
    domain::Domain,
    envelope::{
        MAX_CT_LEN,
        SealedNote,
        VERSION,
    },
    error::{
        Malformed,
        OpenError,
    },
    kem::Kem,
    recipient::Recipient,
    seal::{
        INFO_FIXED_LEN,
        cipher_from,
        expand,
        suite_asserts,
        write_kdf_info,
    },
};

/// Fixed-size chunk used for batched trial decryption.
pub const SCAN_CHUNK: usize = 64;

/// Chunks each thread gets per parallel round. Bounds how much of the input
/// a parallel scan buffers, keeping it a streaming scan.
#[cfg(feature = "parallel")]
const CHUNKS_PER_THREAD: usize = 8;

/// Envelopes one parallel task claims, as `(index, outcome)` pairs.
#[cfg(feature = "parallel")]
type ChunkHits<D> = Vec<(usize, Result<<D as Domain>::Note, Malformed>)>;

/// Working memory one chunk needs, reused across the envelopes in it.
///
/// The scanner owns one set for the whole sequential scan; a parallel task
/// owns one for the duration of its chunk.
struct ScanBuffers<K: Kem> {
    out: [Option<K::SharedSecret>; SCAN_CHUNK],
    info: Vec<u8>,
    scratch: Vec<u8>,
}

impl<K: Kem> ScanBuffers<K> {
    /// Builds a set holding `info_cap` bytes of KDF info and `scratch_cap`
    /// bytes of plaintext scratch.
    fn new(info_cap: usize, scratch_cap: usize) -> Self {
        Self {
            out: core::array::from_fn(|_| None),
            info: Vec::with_capacity(info_cap),
            scratch: Vec::with_capacity(scratch_cap),
        }
    }
}

/// Batch trial-decryption over sealed notes.
///
/// Scans in fixed-size chunks: `Kem::decap_batch` per chunk, then commit
/// compare, then decrypt into reused scratch. A miss allocates nothing.
pub struct Scanner<K: Kem, D: Domain> {
    recipient: Recipient<K>,
    buffers: ScanBuffers<K>,
    _domain: PhantomData<fn() -> D>,
}

impl<K: Kem, D: Domain> Scanner<K, D> {
    /// Builds a scanner for `recipient`.
    ///
    /// Pre-reserves the scratch and KDF-info buffers so a scan never grows
    /// them.
    pub fn new(recipient: Recipient<K>) -> Self {
        const { suite_asserts::<K, D>() };

        let info_len = INFO_FIXED_LEN
            + D::DOMAIN_TAG.len()
            + K::EPK_LEN
            + recipient.pk_encoded().len();
        Self {
            recipient,
            buffers: ScanBuffers::new(info_len, MAX_CT_LEN),
            _domain: PhantomData,
        }
    }

    /// The recipient this scanner holds, whose public key senders seal to.
    pub fn recipient(&self) -> &Recipient<K> {
        &self.recipient
    }

    /// Scans `envelopes` under this scanner's key, binding `aad` into each
    /// AEAD.
    ///
    /// Yields `(index, Result)` per commit match, in input order; a mismatch
    /// is not mine and is skipped, so the run is shorter than the input. A
    /// match that fails afterward (AEAD, note decode, `Domain::verify`)
    /// yields `Err(Malformed)`. Runs to completion before returning.
    pub fn scan<'a, B, I>(
        &mut self,
        envelopes: I,
        aad: &[u8],
    ) -> impl Iterator<Item = (usize, Result<D::Note, Malformed>)> + use<'a, K, D, B, I>
    where
        K: 'a,
        B: AsRef<[u8]> + 'a,
        I: IntoIterator<Item = &'a SealedNote<K, B>>,
    {
        let mut results = Vec::new();
        let mut iter = envelopes.into_iter().enumerate();

        loop {
            let mut chunk: [Option<(usize, &'a SealedNote<K, B>)>; SCAN_CHUNK] =
                [None; SCAN_CHUNK];
            let mut chunk_len = 0usize;
            for slot in &mut chunk {
                let Some(item) = iter.next() else { break };
                *slot = Some(item);
                chunk_len += 1;
            }
            if chunk_len == 0 {
                break;
            }

            scan_chunk::<K, D, B>(
                self.recipient.secret_key(),
                self.recipient.pk_encoded(),
                aad,
                &chunk[..chunk_len],
                &mut self.buffers,
                &mut results,
            );

            if chunk_len < SCAN_CHUNK {
                break;
            }
        }

        results.into_iter()
    }

    /// Scans `envelopes` the same as [`Scanner::scan`], but runs whole
    /// chunks across the rayon thread pool.
    ///
    /// A task owns its chunk end to end: decapsulate, derive, decrypt.
    /// Decapsulation dominates a scan, so it belongs inside the parallel
    /// region. Each task keeps one `info`/`scratch` pair and reuses it
    /// across its chunk, mirroring the scanner's own reuse.
    ///
    /// The input is pulled one fixed window of chunks at a time, so a scan
    /// buffers a bounded number of envelopes whatever the batch size.
    /// `Sync`/`Send` bounds live only on this method.
    #[cfg(feature = "parallel")]
    #[cfg_attr(docsrs, doc(cfg(feature = "parallel")))]
    pub fn scan_parallel<'a, B, I>(
        &mut self,
        envelopes: I,
        aad: &[u8],
    ) -> impl Iterator<Item = (usize, Result<D::Note, Malformed>)> + use<'a, K, D, B, I>
    where
        K: 'a,
        K::SecretKey: Sync,
        D::Note: Send,
        B: AsRef<[u8]> + Sync + 'a,
        I: IntoIterator<Item = &'a SealedNote<K, B>>,
    {
        use rayon::prelude::*;

        let sk = self.recipient.secret_key();
        let pk_r = self.recipient.pk_encoded();
        let info_cap = self.buffers.info.capacity();

        // Sized against the pool this scan actually runs on, so the window
        // holds several chunks per thread whatever the machine is.
        let threads = rayon::current_num_threads().max(1);
        let tasks_per_round = threads * CHUNKS_PER_THREAD;
        let window_len = SCAN_CHUNK * tasks_per_round;

        let mut results = Vec::new();
        let mut iter = envelopes.into_iter().enumerate();
        let mut window: Vec<Option<(usize, &'a SealedNote<K, B>)>> =
            Vec::with_capacity(window_len);
        let mut per_chunk: Vec<ChunkHits<D>> = Vec::new();

        loop {
            window.clear();
            window.extend(iter.by_ref().take(window_len).map(Some));
            let len = window.len();
            if len == 0 {
                break;
            }

            let task_len = len.div_ceil(tasks_per_round).clamp(1, SCAN_CHUNK);

            window
                .par_chunks(task_len)
                .map(|chunk| {
                    // A task's scratch grows to fit the ciphertexts it
                    // actually meets, unlike the scanner's own pre-reserved
                    // buffer, since a task is discarded after one chunk.
                    let mut buffers = ScanBuffers::<K>::new(info_cap, 0);
                    let mut found = Vec::new();
                    scan_chunk::<K, D, B>(sk, pk_r, aad, chunk, &mut buffers, &mut found);
                    found
                })
                .collect_into_vec(&mut per_chunk);

            for found in per_chunk.drain(..) {
                results.extend(found);
            }

            if len < window_len {
                break;
            }
        }

        results.into_iter()
    }
}

/// Runs one chunk end to end: batch decapsulate, trial-decrypt each
/// envelope against the shared secret that came back, then wipe every
/// shared secret the batch produced.
///
/// Pushes an `(index, outcome)` pair onto `hits` for each envelope whose
/// commit matches, and leaves every out-slot `None` on return. `chunk` holds
/// at most [`SCAN_CHUNK`] entries, each of them `Some`.
fn scan_chunk<K, D, B>(
    sk: &K::SecretKey,
    pk_r: &[u8],
    aad: &[u8],
    chunk: &[Option<(usize, &SealedNote<K, B>)>],
    buffers: &mut ScanBuffers<K>,
    hits: &mut Vec<(usize, Result<D::Note, Malformed>)>,
) where
    K: Kem,
    D: Domain,
    B: AsRef<[u8]>,
{
    let len = chunk.len();
    debug_assert!(len <= SCAN_CHUNK, "a chunk fits the fixed-size buffers");

    let mut epks: [&[u8]; SCAN_CHUNK] = [&[]; SCAN_CHUNK];
    for (slot, item) in epks.iter_mut().zip(chunk) {
        *slot = item.expect("caller fills every slot it passes").1.epk();
    }

    K::decap_batch(sk, &epks[..len], &mut buffers.out[..len]);

    for (item, shared_slot) in chunk.iter().zip(&buffers.out[..len]) {
        let (index, envelope) = item.expect("caller fills every slot it passes");
        let outcome = match shared_slot {
            Some(shared) => process_envelope::<K, D, B>(
                shared.as_ref(),
                pk_r,
                envelope,
                aad,
                &mut buffers.info,
                &mut buffers.scratch,
            ),
            None => None,
        };
        if let Some(outcome) = outcome {
            hits.push((index, outcome));
        }
    }

    for slot in &mut buffers.out[..len] {
        if let Some(shared) = slot {
            shared.zeroize();
        }
        *slot = None;
    }
}

/// Derives the suite v1 key material for `envelope` from `shared`, then
/// decrypts and decodes the note if the commit matches.
///
/// Returns `None` on commit mismatch: not this scanner's note. Returns
/// `Some(Err(Malformed))` when the commit matches but decryption, decoding,
/// or domain verification fails.
fn process_envelope<K: Kem, D: Domain, B: AsRef<[u8]>>(
    shared: &[u8],
    pk_r: &[u8],
    envelope: &SealedNote<K, B>,
    aad: &[u8],
    info: &mut Vec<u8>,
    scratch: &mut Vec<u8>,
) -> Option<Result<D::Note, Malformed>> {
    info.clear();
    // `parse` rejects any other version or kem id, so a parsed envelope
    // carries exactly these two bytes; reading them back off the wire would
    // re-derive what the type already guarantees. Spelling them as the
    // constants also keeps this info string visibly identical to the one
    // `seal` and `open` build.
    write_kdf_info(
        info,
        VERSION,
        K::KEM_ID,
        D::DOMAIN_TAG,
        envelope.epk(),
        pk_r,
    );

    let derived = expand(shared, info);

    let commit_matches: bool = derived
        .commit
        .as_slice()
        .ct_eq(envelope.commit().as_slice())
        .into();
    if !commit_matches {
        return None;
    }

    scratch.clear();
    scratch.extend_from_slice(envelope.ct());
    let cipher = cipher_from(&derived.key);
    let decrypted = cipher.decrypt_in_place(&Nonce::from(derived.nonce), aad, scratch);

    let outcome = match decrypted {
        Err(_) => Err(Malformed(OpenError::Aead)),
        Ok(()) => match D::decode_note(scratch) {
            Err(_) => Err(Malformed(OpenError::NoteDecode)),
            Ok(note) if D::verify(&note, aad) => Ok(note),
            Ok(_) => Err(Malformed(OpenError::Verify)),
        },
    };

    scratch.as_mut_slice().zeroize();
    scratch.clear();
    Some(outcome)
}

#[cfg(test)]
mod tests {
    #[cfg(not(feature = "std"))]
    use alloc::vec;
    #[cfg(feature = "std")]
    use std::vec;

    use rand_chacha::ChaCha20Rng;
    use rand_core::SeedableRng;

    use super::*;
    use crate::{
        seal::seal,
        test_util::{
            MockKem,
            TestDomain,
        },
    };

    #[test]
    fn scan_yields_correct_results_for_mixed_batch() {
        let me = Recipient::<MockKem>::new([7u8; 32]);
        let stranger = [9u8; 32];
        let aad = b"scan-aad";
        let mut rng = ChaCha20Rng::seed_from_u64(11);

        let mine =
            seal::<MockKem, TestDomain>(me.public_key(), &vec![1, 2, 3], aad, &mut rng)
                .unwrap();
        let not_mine =
            seal::<MockKem, TestDomain>(&stranger, &vec![4, 5, 6], aad, &mut rng)
                .unwrap();

        let sealed_to_recipient =
            seal::<MockKem, TestDomain>(me.public_key(), &vec![7, 8, 9], aad, &mut rng)
                .unwrap();
        let mut malformed_bytes = sealed_to_recipient.as_bytes().to_vec();
        let last = malformed_bytes.len() - 1;
        malformed_bytes[last] ^= 0x01;
        let malformed = SealedNote::<MockKem, Vec<u8>>::parse(malformed_bytes).unwrap();

        let envelopes = vec![&mine, &not_mine, &malformed];

        let mut scanner = Scanner::<MockKem, TestDomain>::new(me);
        let results: Vec<_> = scanner.scan(envelopes, aad).collect();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, 0);
        assert_eq!(results[0].1, Ok(vec![1, 2, 3]));
        assert_eq!(results[1].0, 2);
        assert!(matches!(results[1].1, Err(Malformed(OpenError::Aead))));
    }

    /// Spans several windows so cross-window and cross-chunk ordering are
    /// both exercised, with hits placed in the first and last window.
    #[cfg(feature = "parallel")]
    #[test]
    fn scan_parallel_matches_scan_over_many_windows() {
        let recipient = [5u8; 32];
        let stranger = [6u8; 32];
        let aad = b"parallel-aad";
        let mut rng = ChaCha20Rng::seed_from_u64(31);

        let me = Recipient::<MockKem>::new(recipient);
        let window = SCAN_CHUNK * CHUNKS_PER_THREAD * rayon::current_num_threads().max(1);
        let count = window * 2 + SCAN_CHUNK + 7;
        let envelopes: Vec<_> = (0..count)
            .map(|i| {
                let to = if i % 5 == 0 { &recipient } else { &stranger };
                seal::<MockKem, TestDomain>(to, &vec![i as u8], aad, &mut rng).unwrap()
            })
            .collect();
        let refs: Vec<_> = envelopes.iter().collect();

        let mut scanner = Scanner::<MockKem, TestDomain>::new(me);
        let sequential: Vec<_> = scanner.scan(refs.iter().copied(), aad).collect();
        let parallel: Vec<_> = scanner.scan_parallel(refs.iter().copied(), aad).collect();

        assert_eq!(sequential, parallel);
        assert_eq!(parallel.len(), count.div_ceil(5));
        assert!(parallel.windows(2).all(|w| w[0].0 < w[1].0));
        assert_eq!(parallel[0].0, 0);
        assert_eq!(parallel.last().unwrap().0, (count - 1) / 5 * 5);
    }

    #[test]
    fn out_slots_are_none_after_a_chunk() {
        let recipient = [3u8; 32];
        let aad = b"hygiene-aad";
        let mut rng = ChaCha20Rng::seed_from_u64(21);

        let envelopes: Vec<_> = (0..4)
            .map(|i| {
                seal::<MockKem, TestDomain>(&recipient, &vec![i as u8], aad, &mut rng)
                    .unwrap()
            })
            .collect();
        let refs: Vec<_> = envelopes.iter().collect();

        let mut scanner = Scanner::<MockKem, TestDomain>::new(Recipient::new(recipient));
        let results: Vec<_> = scanner.scan(refs, aad).collect();

        assert_eq!(results.len(), 4);
        assert!(scanner.buffers.out.iter().all(Option::is_none));
    }

    #[test]
    fn scan_iterators_do_not_borrow_the_scanner() {
        let recipient = [6u8; 32];
        let aad = b"borrow-aad";
        let mut rng = ChaCha20Rng::seed_from_u64(41);
        let envelope =
            seal::<MockKem, TestDomain>(&recipient, &vec![1u8], aad, &mut rng).unwrap();
        let refs = [&envelope];

        let mut scanner = Scanner::<MockKem, TestDomain>::new(Recipient::new(recipient));
        let first = scanner.scan(refs.iter().copied(), aad);
        let second = scanner.scan(refs.iter().copied(), aad);

        assert_eq!(first.count(), 1);
        assert_eq!(second.count(), 1);
    }

    #[test]
    fn empty_batch_yields_nothing() {
        let recipient = [1u8; 32];
        let mut scanner = Scanner::<MockKem, TestDomain>::new(Recipient::new(recipient));
        let envelopes: Vec<&SealedNote<MockKem, Vec<u8>>> = Vec::new();
        let results: Vec<_> = scanner.scan(envelopes, b"aad").collect();
        assert!(results.is_empty());
    }

    #[test]
    fn scan_spans_multiple_chunks_in_order() {
        let recipient = [4u8; 32];
        let aad = b"chunk-aad";
        let mut rng = ChaCha20Rng::seed_from_u64(31);

        let count = SCAN_CHUNK + 5;
        let envelopes: Vec<_> = (0..count)
            .map(|i| {
                seal::<MockKem, TestDomain>(
                    &recipient,
                    &vec![(i % 256) as u8],
                    aad,
                    &mut rng,
                )
                .unwrap()
            })
            .collect();
        let refs: Vec<_> = envelopes.iter().collect();

        let mut scanner = Scanner::<MockKem, TestDomain>::new(Recipient::new(recipient));
        let results: Vec<_> = scanner.scan(refs, aad).collect();

        assert_eq!(results.len(), count);
        for (expected_index, (index, outcome)) in results.into_iter().enumerate() {
            assert_eq!(index, expected_index);
            assert_eq!(outcome, Ok(vec![(expected_index % 256) as u8]));
        }
    }
}
