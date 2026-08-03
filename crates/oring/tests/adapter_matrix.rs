#![cfg(all(
    feature = "test-helpers",
    feature = "k256",
    feature = "x25519",
    feature = "grumpkin"
))]

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
    Grumpkin,
    K256,
    Kem,
    X25519,
    open,
    seal,
    test_util::{
        MockKem,
        TestDomain,
    },
};
use rand_chacha::ChaCha20Rng;
use rand_core::{
    CryptoRng,
    SeedableRng,
};
use x25519_dalek::{
    PublicKey as X25519PublicKey,
    StaticSecret,
};

/// Draws a fresh keypair for the matrix test, one impl per adapter under
/// test.
trait MatrixKem: Kem {
    fn matrix_keypair(rng: &mut impl CryptoRng) -> (Self::PublicKey, Self::SecretKey);
}

impl MatrixKem for MockKem {
    fn matrix_keypair(rng: &mut impl CryptoRng) -> (Self::PublicKey, Self::SecretKey) {
        let mut sk = [0u8; 32];
        rng.fill_bytes(&mut sk);
        (sk, sk)
    }
}

impl MatrixKem for K256 {
    fn matrix_keypair(rng: &mut impl CryptoRng) -> (Self::PublicKey, Self::SecretKey) {
        let sk = k256::SecretKey::generate_from_rng(rng);
        let pk = sk.public_key();
        (pk, sk)
    }
}

impl MatrixKem for X25519 {
    fn matrix_keypair(rng: &mut impl CryptoRng) -> (Self::PublicKey, Self::SecretKey) {
        let sk = StaticSecret::random_from_rng(rng);
        let pk = X25519PublicKey::from(&sk);
        (pk, sk)
    }
}

impl MatrixKem for Grumpkin {
    fn matrix_keypair(rng: &mut impl CryptoRng) -> (Self::PublicKey, Self::SecretKey) {
        let mut bytes = [0u8; 64];
        rng.fill_bytes(&mut bytes);
        let sk = Fr::from_le_bytes_mod_order(&bytes);
        let pk = (Affine::generator() * sk).into_affine();
        (pk, sk)
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
                let (pk, sk) = {{kem}}::matrix_keypair(&mut rng);
                let note = vec![1u8, 2, 3, 4, 5];
                let aad = b"matrix-aad";

                let envelope =
                    seal::<{{kem}}, TestDomain>(&pk, &note, aad, &mut rng).unwrap();
                let opened =
                    open::<{{kem}}, TestDomain, _>(&sk, &pk, &envelope, aad).unwrap();

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
                let (pk_a, _sk_a) = {{kem}}::matrix_keypair(&mut rng);
                let (pk_b, sk_b) = {{kem}}::matrix_keypair(&mut rng);
                let note = vec![9u8, 8, 7];
                let aad = b"matrix-wrong-key-aad";

                let envelope =
                    seal::<{{kem}}, TestDomain>(&pk_a, &note, aad, &mut rng).unwrap();
                let result =
                    open::<{{kem}}, TestDomain, _>(&sk_b, &pk_b, &envelope, aad);

                assert!(matches!(result, Ok(None)));
            }
        }
    }
}

gen_matrix_wrong_key!(["MockKem", "K256", "X25519", "Grumpkin"]);
