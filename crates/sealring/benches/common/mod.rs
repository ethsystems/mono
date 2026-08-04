// Shared by every bench target, so each one leaves part of it unused.
#![allow(dead_code)]

use ark_ff::PrimeField;
use ark_grumpkin::Fr;
use k256::elliptic_curve::Generate;
use sealring::{
    Domain,
    Grumpkin,
    K256,
    Kem,
    Recipient,
    SealedNote,
    X25519,
    seal,
};
use rand_chacha::ChaCha20Rng;
use rand_core::{
    Rng,
    SeedableRng,
};
use x25519_dalek::StaticSecret;

/// AAD bound into every benchmark envelope.
pub const AAD: &[u8] = b"sealring-bench-aad";

/// Baseline note length, used by every benchmark that holds note size fixed.
pub const NOTE_LEN: usize = 48;

const KEY_SEED: u64 = 4_242;
const ENVELOPE_SEED: u64 = 9_009;

/// Deterministic secret-key generation per adapter. The matching public key
/// comes from `Kem::derive_pk`, so a bench never pairs one up by hand.
pub trait GenKeypair: Kem {
    fn secret_key(rng: &mut ChaCha20Rng) -> Self::SecretKey;
}

impl GenKeypair for K256 {
    fn secret_key(rng: &mut ChaCha20Rng) -> Self::SecretKey {
        k256::SecretKey::generate_from_rng(rng)
    }
}

impl GenKeypair for X25519 {
    fn secret_key(rng: &mut ChaCha20Rng) -> Self::SecretKey {
        StaticSecret::random_from_rng(rng)
    }
}

// Draws the scalar the way the adapter draws its ephemeral scalar: 64 bytes
// from the caller's rng, reduced modulo the scalar field order. The rng stays
// on this side of arkworks, so the bench sees the same key distribution the
// adapter produces.
impl GenKeypair for Grumpkin {
    fn secret_key(rng: &mut ChaCha20Rng) -> Self::SecretKey {
        let mut bytes = [0u8; 64];
        rng.fill_bytes(&mut bytes);
        Fr::from_le_bytes_mod_order(&bytes)
    }
}

/// Builds a recipient from a fixed seed, so two calls yield two independent
/// values holding identical key material.
pub fn bench_recipient<K: GenKeypair>() -> Recipient<K> {
    Recipient::new(K::secret_key(&mut ChaCha20Rng::seed_from_u64(KEY_SEED)))
}

/// Builds `count` envelopes of `note_len`-byte notes under domain `D`. Every
/// `1 / hit_rate`th envelope is addressed to `pk`; the rest go to a fresh
/// stranger key each.
pub fn build_envelopes<K: GenKeypair, D: Domain<Note = Vec<u8>>>(
    pk: &K::PublicKey,
    count: usize,
    hit_rate: f64,
    note_len: usize,
) -> Vec<SealedNote<K, Vec<u8>>> {
    let mut rng = ChaCha20Rng::seed_from_u64(ENVELOPE_SEED);
    let stride = if hit_rate > 0.0 {
        (1.0 / hit_rate).round() as usize
    } else {
        0
    };

    (0..count)
        .map(|i| {
            let note = vec![(i % 256) as u8; note_len];
            if stride != 0 && i % stride == 0 {
                seal::<K, D>(pk, &note, AAD, &mut rng).unwrap()
            } else {
                let stranger = K::derive_pk(&K::secret_key(&mut rng));
                seal::<K, D>(&stranger, &note, AAD, &mut rng).unwrap()
            }
        })
        .collect()
}
