#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
#[cfg(feature = "std")]
use std::vec::Vec;

use chacha20poly1305::{
    ChaCha20Poly1305,
    Key,
    KeyInit,
    Nonce,
    aead::{
        Aead,
        Payload,
    },
};
use hkdf::Hkdf;
use rand_core::CryptoRng;
use sha2::Sha256;
use subtle::ConstantTimeEq;
use zeroize::Zeroize;

use crate::{
    domain::Domain,
    envelope::{
        COMMIT_LEN,
        HEADER_LEN,
        SealedNote,
        VERSION,
    },
    error::{
        OpenError,
        SealError,
    },
    kem::Kem,
    recipient::Recipient,
};

/// Domain-separation salt for the suite v1 HKDF-SHA256 extract step.
pub(crate) const SALT: &[u8] = b"sealring/v1";

/// Byte length of the suite v1 symmetric key.
pub(crate) const KEY_LEN: usize = 32;

/// Byte length of the suite v1 AEAD nonce.
pub(crate) const NONCE_LEN: usize = 12;

/// Byte length of the HKDF-Expand output: key, nonce, and commit tag.
pub(crate) const DERIVED_LEN: usize = KEY_LEN + NONCE_LEN + COMMIT_LEN;

/// Byte length of the u16 big-endian length prefix in front of each KDF
/// info field.
pub(crate) const LP_PREFIX_LEN: usize = 2;

/// Number of length-prefixed fields in the KDF info string.
pub(crate) const INFO_FIELD_COUNT: usize = 5;

/// Bytes the KDF info string spends on length prefixes plus the
/// single-byte version and kem-id fields.
pub(crate) const INFO_FIXED_LEN: usize = INFO_FIELD_COUNT * LP_PREFIX_LEN + HEADER_LEN;

/// Key, nonce, and commit tag produced by HKDF-Expand. Zeroized on drop.
pub(crate) struct Derived {
    pub(crate) key: [u8; KEY_LEN],
    pub(crate) nonce: [u8; NONCE_LEN],
    pub(crate) commit: [u8; COMMIT_LEN],
}

impl Drop for Derived {
    fn drop(&mut self) {
        self.key.zeroize();
        self.nonce.zeroize();
        self.commit.zeroize();
    }
}

/// Appends `field` to `out` as a u16 big-endian length prefix followed by
/// its bytes.
fn push_lp(out: &mut Vec<u8>, field: &[u8]) {
    let len = u16::try_from(field.len()).expect("kdf info field longer than u16::MAX");
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(field);
}

/// Compile-time suite v1 invariants every entry point shares: the domain
/// tag separates domains, and each variable-length KDF info field fits the
/// u16 prefix [`push_lp`] writes for it.
///
/// Invoke as `const { suite_asserts::<K, D>() }`, which evaluates per
/// instantiation and so fails to compile for an unusable suite.
pub(crate) const fn suite_asserts<K: Kem, D: Domain>() {
    assert!(!D::DOMAIN_TAG.is_empty(), "domain tag must not be empty");
    assert!(
        D::DOMAIN_TAG.len() <= u16::MAX as usize,
        "domain tag must fit its u16 kdf info length prefix"
    );
    assert!(
        K::EPK_LEN <= u16::MAX as usize,
        "epk must fit its u16 kdf info length prefix"
    );
}

/// Appends the HKDF-Expand info string to `out`: `lp(version) lp(kem_id)
/// lp(tag) lp(epk) lp(pk_r)`.
pub(crate) fn write_kdf_info(
    out: &mut Vec<u8>,
    version: u8,
    kem_id: u8,
    tag: &str,
    epk: &[u8],
    pk_r: &[u8],
) {
    push_lp(out, &[version]);
    push_lp(out, &[kem_id]);
    push_lp(out, tag.as_bytes());
    push_lp(out, epk);
    push_lp(out, pk_r);
}

#[cfg(test)]
pub(crate) fn kdf_info_for_test(
    kem_id: u8,
    tag: &str,
    epk: &[u8],
    pk_r: &[u8],
) -> Vec<u8> {
    let mut info = Vec::new();
    write_kdf_info(&mut info, VERSION, kem_id, tag, epk, pk_r);
    info
}

/// Runs HKDF-SHA256 extract-then-expand over `shared` under `info` and
/// splits the output into key, nonce, and commit tag.
pub(crate) fn expand(shared: &[u8], info: &[u8]) -> Derived {
    let hk = Hkdf::<Sha256>::new(Some(SALT), shared);
    let mut okm = [0u8; DERIVED_LEN];
    hk.expand(info, &mut okm)
        .expect("suite v1 derived length is within the HKDF-SHA256 output bound");

    let mut derived = Derived {
        key: [0u8; KEY_LEN],
        nonce: [0u8; NONCE_LEN],
        commit: [0u8; COMMIT_LEN],
    };
    derived.key.copy_from_slice(&okm[..KEY_LEN]);
    derived
        .nonce
        .copy_from_slice(&okm[KEY_LEN..KEY_LEN + NONCE_LEN]);
    derived.commit.copy_from_slice(&okm[KEY_LEN + NONCE_LEN..]);
    okm.zeroize();
    derived
}

/// Builds the suite v1 info string and derives from `shared`, zeroizing
/// `shared` once the expand step has produced the key, nonce, and commit
/// tag.
fn derive<S: Zeroize + AsRef<[u8]>>(
    mut shared: S,
    kem_id: u8,
    tag: &str,
    epk: &[u8],
    pk_r: &[u8],
) -> Derived {
    let mut info_scratch =
        Vec::with_capacity(INFO_FIXED_LEN + tag.len() + epk.len() + pk_r.len());
    write_kdf_info(&mut info_scratch, VERSION, kem_id, tag, epk, pk_r);
    let derived = expand(shared.as_ref(), &info_scratch);
    shared.zeroize();
    derived
}

/// Builds a ChaCha20-Poly1305 cipher from `key`, wiping the key copy the
/// construction consumes.
pub(crate) fn cipher_from(key: &[u8; KEY_LEN]) -> ChaCha20Poly1305 {
    let mut key_block = Key::from(*key);
    let sealer = ChaCha20Poly1305::new(&key_block);
    key_block.as_mut_slice().zeroize();
    sealer
}

/// Constant-time test for an all-zero byte string.
fn is_all_zero(bytes: &[u8]) -> bool {
    let mut residue = 0u8;
    for octet in bytes {
        residue |= octet;
    }
    residue.ct_eq(&0).into()
}

/// Seals `note` to `pk` under domain `D`, binding `aad` into the AEAD.
///
/// Returns `Err(SealError)` when encapsulation produces an all-zero shared
/// secret, which a low-order or identity recipient key yields.
pub fn seal<K: Kem, D: Domain>(
    pk: &K::PublicKey,
    note: &D::Note,
    aad: &[u8],
    rng: &mut impl CryptoRng,
) -> Result<SealedNote<K, Vec<u8>>, SealError> {
    const { suite_asserts::<K, D>() };

    let (epk, mut shared) = K::encap(rng, pk);
    if is_all_zero(shared.as_ref()) {
        shared.zeroize();
        return Err(SealError);
    }

    let pk_r_encoded = K::encode_pk(pk);
    let derived = derive(
        shared,
        K::KEM_ID,
        D::DOMAIN_TAG,
        epk.as_ref(),
        pk_r_encoded.as_ref(),
    );

    let mut note_bytes = Vec::new();
    if D::encode_note(note, &mut note_bytes).is_err() {
        note_bytes.zeroize();
        return Err(SealError);
    }
    let cipher = cipher_from(&derived.key);
    let encrypted = cipher.encrypt(
        &Nonce::from(derived.nonce),
        Payload {
            msg: &note_bytes,
            aad,
        },
    );
    note_bytes.zeroize();
    let ct = encrypted.map_err(|_| SealError)?;

    let mut bytes = Vec::with_capacity(HEADER_LEN + K::EPK_LEN + COMMIT_LEN + ct.len());
    bytes.push(VERSION);
    bytes.push(K::KEM_ID);
    bytes.extend_from_slice(epk.as_ref());
    bytes.extend_from_slice(&derived.commit);
    bytes.extend_from_slice(&ct);

    SealedNote::parse(bytes).map_err(|_| SealError)
}

/// Opens `envelope` for `recipient`, whose public key reproduces the KDF
/// binding, and binds `aad` into the AEAD.
///
/// Returns `Ok(None)` when the commit does not match this key: the envelope
/// was not addressed to it, which is not an error. Returns `Err` when the
/// commit matches but AEAD decryption, note decoding, or `Domain::verify`
/// fails: an authenticated envelope that is wrong.
///
/// Opening more than one envelope reuses one [`Recipient`], which holds the
/// encoded public key this derivation needs.
pub fn open<K: Kem, D: Domain, B: AsRef<[u8]>>(
    recipient: &Recipient<K>,
    envelope: &SealedNote<K, B>,
    aad: &[u8],
) -> Result<Option<D::Note>, OpenError> {
    const { suite_asserts::<K, D>() };

    let Some(shared) = K::decap(recipient.secret_key(), envelope.epk()) else {
        return Ok(None);
    };

    let derived = derive(
        shared,
        K::KEM_ID,
        D::DOMAIN_TAG,
        envelope.epk(),
        recipient.pk_encoded(),
    );

    let commit_matches: bool = derived
        .commit
        .as_slice()
        .ct_eq(envelope.commit().as_slice())
        .into();
    if !commit_matches {
        return Ok(None);
    }

    let cipher = cipher_from(&derived.key);
    let mut plaintext = cipher
        .decrypt(
            &Nonce::from(derived.nonce),
            Payload {
                msg: envelope.ct(),
                aad,
            },
        )
        .map_err(|_| OpenError::Aead)?;

    let decoded = D::decode_note(&plaintext);
    plaintext.zeroize();
    let note = decoded.map_err(|_| OpenError::NoteDecode)?;

    if !D::verify(&note, aad) {
        return Err(OpenError::Verify);
    }

    Ok(Some(note))
}

#[cfg(test)]
mod tests {
    use core::convert::Infallible;

    #[cfg(not(feature = "std"))]
    use alloc::{
        vec,
        vec::Vec,
    };
    #[cfg(feature = "std")]
    use std::{
        vec,
        vec::Vec,
    };

    use rand_chacha::ChaCha20Rng;
    use rand_core::SeedableRng;

    use super::*;
    use crate::test_util::{
        MockKem,
        TestDomain,
    };

    struct OtherDomain;

    impl Domain for OtherDomain {
        type Error = Infallible;
        type Note = Vec<u8>;

        const DOMAIN_TAG: &'static str = "sealring-test-other";

        fn encode_note(note: &Self::Note, out: &mut Vec<u8>) -> Result<(), Self::Error> {
            out.extend_from_slice(note);
            Ok(())
        }

        fn decode_note(bytes: &[u8]) -> Result<Self::Note, Self::Error> {
            Ok(bytes.to_vec())
        }
    }

    /// Domain whose codec rejects: encoding fails for an empty note, and
    /// decoding fails for any note whose first byte is not `0xA5`.
    struct PickyDomain;

    #[derive(Debug, PartialEq, Eq)]
    struct NoteRejected;

    impl Domain for PickyDomain {
        type Error = NoteRejected;
        type Note = Vec<u8>;

        const DOMAIN_TAG: &'static str = "sealring-test-picky";

        fn encode_note(note: &Self::Note, out: &mut Vec<u8>) -> Result<(), Self::Error> {
            // Push before the check so a rejection leaves a partial encoding
            // behind, which `seal` must wipe rather than encrypt.
            out.extend_from_slice(note);
            if note.is_empty() {
                return Err(NoteRejected);
            }
            Ok(())
        }

        fn decode_note(bytes: &[u8]) -> Result<Self::Note, Self::Error> {
            match bytes.first() {
                Some(0xA5) => Ok(bytes.to_vec()),
                _ => Err(NoteRejected),
            }
        }
    }

    /// `PickyDomain`'s tag with a codec that accepts anything, so a note
    /// sealed here reaches `PickyDomain`'s decoder with the commit matching.
    struct PermissiveDomain;

    impl Domain for PermissiveDomain {
        type Error = Infallible;
        type Note = Vec<u8>;

        const DOMAIN_TAG: &'static str = PickyDomain::DOMAIN_TAG;

        fn encode_note(note: &Self::Note, out: &mut Vec<u8>) -> Result<(), Self::Error> {
            out.extend_from_slice(note);
            Ok(())
        }

        fn decode_note(bytes: &[u8]) -> Result<Self::Note, Self::Error> {
            Ok(bytes.to_vec())
        }
    }

    #[test]
    fn seal_then_open_round_trips() {
        let me = Recipient::<MockKem>::new([5u8; 32]);
        let note = vec![1u8, 2, 3, 4, 5];
        let aad = b"context";
        let mut rng = ChaCha20Rng::seed_from_u64(7);

        let envelope =
            seal::<MockKem, TestDomain>(me.public_key(), &note, aad, &mut rng).unwrap();
        let opened = open::<MockKem, TestDomain, _>(&me, &envelope, aad).unwrap();

        assert_eq!(opened, Some(note));
    }

    #[test]
    fn open_with_wrong_sk_returns_none_not_error() {
        let recipient_a = [1u8; 32];
        let recipient_b = Recipient::<MockKem>::new([2u8; 32]);
        let note = vec![5u8, 6, 7];
        let aad = b"aad";
        let mut rng = ChaCha20Rng::seed_from_u64(2);

        let envelope =
            seal::<MockKem, TestDomain>(&recipient_a, &note, aad, &mut rng).unwrap();
        let result = open::<MockKem, TestDomain, _>(&recipient_b, &envelope, aad);

        assert!(matches!(result, Ok(None)));
    }

    #[test]
    fn any_single_byte_flip_never_opens() {
        let me = Recipient::<MockKem>::new([33u8; 32]);
        let note = vec![9u8, 8, 7, 6];
        let aad = b"aad";
        let mut rng = ChaCha20Rng::seed_from_u64(4);
        let envelope =
            seal::<MockKem, TestDomain>(me.public_key(), &note, aad, &mut rng).unwrap();
        let original = envelope.as_bytes().to_vec();

        for i in 0..original.len() {
            let mut flipped = original.clone();
            flipped[i] ^= 0x01;
            if let Ok(parsed) = SealedNote::<MockKem, Vec<u8>>::parse(flipped) {
                let result = open::<MockKem, TestDomain, _>(&me, &parsed, aad);
                assert!(
                    !matches!(result, Ok(Some(_))),
                    "byte {i} flip must not open"
                );
            }
        }
    }

    #[test]
    fn different_recipients_produce_unopenable_cross_envelope() {
        let recipient_a = [11u8; 32];
        let recipient_b = Recipient::<MockKem>::new([22u8; 32]);
        let note = vec![1u8, 2, 3];
        let aad = b"aad";
        let mut rng = ChaCha20Rng::seed_from_u64(3);

        let envelope_a =
            seal::<MockKem, TestDomain>(&recipient_a, &note, aad, &mut rng).unwrap();
        let envelope_b =
            seal::<MockKem, TestDomain>(recipient_b.public_key(), &note, aad, &mut rng)
                .unwrap();

        assert_ne!(envelope_a.as_bytes(), envelope_b.as_bytes());
        let result = open::<MockKem, TestDomain, _>(&recipient_b, &envelope_a, aad);
        assert!(matches!(result, Ok(None)));
    }

    #[test]
    fn cross_domain_tag_yields_nothing() {
        let me = Recipient::<MockKem>::new([9u8; 32]);
        let note = vec![10u8, 20, 30];
        let aad = b"aad";
        let mut rng = ChaCha20Rng::seed_from_u64(1);

        let envelope =
            seal::<MockKem, TestDomain>(me.public_key(), &note, aad, &mut rng).unwrap();
        let result = open::<MockKem, OtherDomain, _>(&me, &envelope, aad);

        assert!(matches!(result, Ok(None)));
    }

    #[test]
    fn encode_rejection_fails_the_seal() {
        let recipient = [13u8; 32];
        let mut rng = ChaCha20Rng::seed_from_u64(13);

        let rejected =
            seal::<MockKem, PickyDomain>(&recipient, &Vec::new(), b"aad", &mut rng);
        assert_eq!(rejected.err(), Some(SealError));

        let accepted =
            seal::<MockKem, PickyDomain>(&recipient, &vec![0xA5, 1], b"aad", &mut rng);
        assert!(accepted.is_ok());
    }

    #[test]
    fn decode_rejection_surfaces_as_note_decode() {
        let me = Recipient::<MockKem>::new([17u8; 32]);
        let aad = b"aad";
        let mut rng = ChaCha20Rng::seed_from_u64(17);

        // Sealed under a domain that encodes anything, opened under one whose
        // decoder rejects the plaintext: same tag, so the commit still matches.
        let envelope = seal::<MockKem, PermissiveDomain>(
            me.public_key(),
            &vec![0x00, 1],
            aad,
            &mut rng,
        )
        .unwrap();
        let result = open::<MockKem, PickyDomain, _>(&me, &envelope, aad);
        assert_eq!(result, Err(OpenError::NoteDecode));

        let accepted = seal::<MockKem, PermissiveDomain>(
            me.public_key(),
            &vec![0xA5, 1],
            aad,
            &mut rng,
        )
        .unwrap();
        let opened = open::<MockKem, PickyDomain, _>(&me, &accepted, aad);
        assert_eq!(opened, Ok(Some(vec![0xA5, 1])));
    }

    #[test]
    fn deterministic_derivation_matches_golden_vector() {
        let recipient = [0x42u8; 32];
        let note = vec![1u8, 2, 3, 4];
        let aad = b"golden-aad";
        let mut rng = ChaCha20Rng::seed_from_u64(0);

        let envelope =
            seal::<MockKem, TestDomain>(&recipient, &note, aad, &mut rng).unwrap();

        // Frozen wire format for MockKem suite v1: a mismatch is a format break.
        #[rustfmt::skip]
        const EXPECTED: &[u8] = &[
            1, 255, 178, 247, 245, 129, 214, 222, 60, 6, 168, 34, 253, 110, 126, 130, 101, 251,
            192, 15, 132, 1, 105, 106, 91, 220, 52, 245, 166, 210, 255, 63, 146, 47, 232, 123,
            147, 237, 191, 7, 21, 77, 211, 14, 231, 235, 141, 90, 24, 140, 87, 32, 55, 137,
            153, 234, 185, 252, 230, 216, 24, 18, 50, 29, 124, 123, 73, 227, 90, 166, 201, 106,
            179, 159, 168, 59, 165, 25, 112, 163, 182, 17, 197, 129, 107, 190,
        ];
        assert_eq!(envelope.as_bytes(), EXPECTED);
    }

    #[test]
    fn kdf_info_layout_is_length_prefixed() {
        let info = kdf_info_for_test(0xAB, "example-tag", &[1, 2, 3], &[4, 5, 6, 7]);

        let mut expected = Vec::new();
        expected.extend_from_slice(&1u16.to_be_bytes());
        expected.push(VERSION);
        expected.extend_from_slice(&1u16.to_be_bytes());
        expected.push(0xAB);
        expected.extend_from_slice(&11u16.to_be_bytes());
        expected.extend_from_slice(b"example-tag");
        expected.extend_from_slice(&3u16.to_be_bytes());
        expected.extend_from_slice(&[1, 2, 3]);
        expected.extend_from_slice(&4u16.to_be_bytes());
        expected.extend_from_slice(&[4, 5, 6, 7]);

        assert_eq!(info, expected);
    }
}
