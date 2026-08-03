// Shared by every bench target, so each one leaves part of it unused.
#![allow(dead_code)]

use ark_ec::{
    AffineRepr,
    CurveGroup,
};
use ark_ff::PrimeField;
use ark_grumpkin::{
    Affine,
    Fr,
};
use k256::elliptic_curve::Generate;
use oring::{
    Domain,
    Grumpkin,
    K256,
    Kem,
    SealedNote,
    X25519,
    seal,
};
use rand_chacha::ChaCha20Rng;
use rand_core::{
    Rng,
    SeedableRng,
};
use x25519_dalek::{
    PublicKey,
    StaticSecret,
};

/// AAD bound into every benchmark envelope.
pub const AAD: &[u8] = b"oring-bench-aad";

/// Baseline note length, used by every benchmark that holds note size fixed.
pub const NOTE_LEN: usize = 48;

const KEY_SEED: u64 = 4_242;
const ENVELOPE_SEED: u64 = 9_009;

/// Deterministic keypair generation per adapter, giving benchmarks a way
/// to obtain two `SecretKey` values that hold the same key material.
pub trait GenKeypair: Kem {
    fn keypair(rng: &mut ChaCha20Rng) -> (Self::PublicKey, Self::SecretKey);
}

impl GenKeypair for K256 {
    fn keypair(rng: &mut ChaCha20Rng) -> (Self::PublicKey, Self::SecretKey) {
        let sk = k256::SecretKey::generate_from_rng(rng);
        let pk = sk.public_key();
        (pk, sk)
    }
}

impl GenKeypair for X25519 {
    fn keypair(rng: &mut ChaCha20Rng) -> (Self::PublicKey, Self::SecretKey) {
        let sk = StaticSecret::random_from_rng(rng);
        let pk = PublicKey::from(&sk);
        (pk, sk)
    }
}

// Draws the scalar the way the adapter draws its ephemeral scalar: 64 bytes
// from the caller's rng, reduced modulo the scalar field order. The rng stays
// on this side of arkworks, so the bench sees the same key distribution the
// adapter produces.
impl GenKeypair for Grumpkin {
    fn keypair(rng: &mut ChaCha20Rng) -> (Self::PublicKey, Self::SecretKey) {
        let mut bytes = [0u8; 64];
        rng.fill_bytes(&mut bytes);
        let sk = Fr::from_le_bytes_mod_order(&bytes);
        let pk = (Affine::generator() * sk).into_affine();
        (pk, sk)
    }
}

/// Draws a recipient keypair from a fixed seed, so two calls yield two
/// independent `SecretKey` values holding identical key material.
pub fn recipient_keypair<K: GenKeypair>() -> (K::PublicKey, K::SecretKey) {
    K::keypair(&mut ChaCha20Rng::seed_from_u64(KEY_SEED))
}

/// Builds `count` envelopes of `note_len`-byte notes under domain `D`. Every
/// `1 / hit_rate`th envelope is addressed to `pk`; the rest go to a fresh
/// stranger key each.
pub fn build_envelopes<K: GenKeypair, D: Domain<Note = Vec<u8>>>(
    pk: &K::PublicKey,
    count: usize,
    hit_rate: f64,
    note_len: usize,
) -> Vec<SealedNote<K, Vec<u8>>>
where
    K::SharedSecret: AsRef<[u8]>,
{
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
                let (stranger, _) = K::keypair(&mut rng);
                seal::<K, D>(&stranger, &note, AAD, &mut rng).unwrap()
            }
        })
        .collect()
}
