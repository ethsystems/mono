#![cfg(all(
    feature = "test-helpers",
    feature = "k256",
    feature = "x25519",
    feature = "grumpkin"
))]

use ark_ff::PrimeField;
use ark_grumpkin::Fr;
use k256::elliptic_curve::Generate;
use rand_chacha::ChaCha20Rng;
use rand_core::{
    CryptoRng,
    SeedableRng,
};
use sealring::{
    Grumpkin,
    K256,
    Kem,
    Recipient,
    X25519,
    open,
    seal,
    test_util::{
        MockKem,
        TestDomain,
    },
};
use x25519_dalek::StaticSecret;

/// Draws a fresh secret key for the matrix test, one impl per adapter under
/// test. The matching public key comes from `Kem::derive_pk`, so no impl
/// here restates how a keypair pairs up.
trait MatrixKem: Kem {
    fn matrix_sk(rng: &mut impl CryptoRng) -> Self::SecretKey;
}

impl MatrixKem for MockKem {
    fn matrix_sk(rng: &mut impl CryptoRng) -> Self::SecretKey {
        let mut sk = [0u8; 32];
        rng.fill_bytes(&mut sk);
        sk
    }
}

impl MatrixKem for K256 {
    fn matrix_sk(rng: &mut impl CryptoRng) -> Self::SecretKey {
        k256::SecretKey::generate_from_rng(rng)
    }
}

impl MatrixKem for X25519 {
    fn matrix_sk(rng: &mut impl CryptoRng) -> Self::SecretKey {
        StaticSecret::random_from_rng(rng)
    }
}

impl MatrixKem for Grumpkin {
    fn matrix_sk(rng: &mut impl CryptoRng) -> Self::SecretKey {
        let mut bytes = [0u8; 64];
        rng.fill_bytes(&mut bytes);
        Fr::from_le_bytes_mod_order(&bytes)
    }
}

#[crabtime::function]
fn gen_matrix_round_trip(kems: Vec<String>) {
    for kem in kems {
        let suffix = kem.to_lowercase();
        crabtime::output! {
            #[test]
            fn round_trip_{{suffix}}() {
                let mut rng = ChaCha20Rng::seed_from_u64(1);
                let me = Recipient::<{{kem}}>::new({{kem}}::matrix_sk(&mut rng));
                let note = vec![1u8, 2, 3, 4, 5];
                let aad = b"matrix-aad";

                let envelope =
                    seal::<{{kem}}, TestDomain>(me.public_key(), &note, aad, &mut rng)
                        .unwrap();
                let opened =
                    open::<{{kem}}, TestDomain, _>(&me, &envelope, aad).unwrap();

                assert_eq!(opened, Some(note));
            }
        }
    }
}

gen_matrix_round_trip!(["MockKem", "K256", "X25519", "Grumpkin"]);

#[crabtime::function]
fn gen_matrix_wrong_key(kems: Vec<String>) {
    for kem in kems {
        let suffix = kem.to_lowercase();
        crabtime::output! {
            #[test]
            fn wrong_key_{{suffix}}() {
                let mut rng = ChaCha20Rng::seed_from_u64(2);
                let stranger = Recipient::<{{kem}}>::new({{kem}}::matrix_sk(&mut rng));
                let me = Recipient::<{{kem}}>::new({{kem}}::matrix_sk(&mut rng));
                let note = vec![9u8, 8, 7];
                let aad = b"matrix-wrong-key-aad";

                let envelope = seal::<{{kem}}, TestDomain>(
                    stranger.public_key(),
                    &note,
                    aad,
                    &mut rng,
                )
                .unwrap();
                let result = open::<{{kem}}, TestDomain, _>(&me, &envelope, aad);

                assert!(matches!(result, Ok(None)));
            }
        }
    }
}

gen_matrix_wrong_key!(["MockKem", "K256", "X25519", "Grumpkin"]);
