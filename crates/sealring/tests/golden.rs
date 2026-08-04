#![cfg(feature = "test-helpers")]

use sealring::{
    seal,
    test_util::{
        MockKem,
        TestDomain,
    },
};
use rand_chacha::ChaCha20Rng;
use rand_core::SeedableRng;

/// Note bytes shared by every adapter's golden vector.
const NOTE: [u8; 4] = [1, 2, 3, 4];

/// AAD bytes shared by every adapter's golden vector.
const AAD: &[u8] = b"golden-aad";

fn decode_hex(hex: &str) -> Vec<u8> {
    let hex = hex.trim();
    (0..hex.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&hex[i..i + 2], 16).expect("golden vector is valid hex")
        })
        .collect()
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Frozen suite v1 wire format for MockKem, seeded from `ChaCha20Rng::seed_from_u64(0)`.
/// A mismatch is a format break.
#[test]
fn mockkem_matches_frozen_vector() {
    let recipient = [0x42u8; 32];
    let mut rng = ChaCha20Rng::seed_from_u64(0);

    let envelope =
        seal::<MockKem, TestDomain>(&recipient, &NOTE.to_vec(), AAD, &mut rng).unwrap();
    let expected = decode_hex(include_str!("golden/mockkem.hex"));
    assert_eq!(envelope.as_bytes(), expected.as_slice());
}

#[cfg(feature = "k256")]
mod golden_k256 {
    use k256::{
        SecretKey,
        elliptic_curve::Generate,
    };
    use sealring::{
        K256,
        seal,
        test_util::TestDomain,
    };
    use rand_chacha::ChaCha20Rng;
    use rand_core::SeedableRng;

    use super::{
        AAD,
        NOTE,
        decode_hex,
    };

    /// Frozen suite v1 wire format for k256. A mismatch is a format break.
    #[test]
    fn matches_frozen_vector() {
        let mut rng = ChaCha20Rng::seed_from_u64(100);
        let sk = SecretKey::generate_from_rng(&mut rng);
        let pk = sk.public_key();

        let envelope =
            seal::<K256, TestDomain>(&pk, &NOTE.to_vec(), AAD, &mut rng).unwrap();
        let expected = decode_hex(include_str!("golden/k256.hex"));
        assert_eq!(envelope.as_bytes(), expected.as_slice());
    }
}

#[cfg(feature = "x25519")]
mod golden_x25519 {
    use sealring::{
        X25519,
        seal,
        test_util::TestDomain,
    };
    use rand_chacha::ChaCha20Rng;
    use rand_core::SeedableRng;
    use x25519_dalek::{
        PublicKey,
        StaticSecret,
    };

    use super::{
        AAD,
        NOTE,
        decode_hex,
    };

    /// Frozen suite v1 wire format for x25519. A mismatch is a format break.
    #[test]
    fn matches_frozen_vector() {
        let mut rng = ChaCha20Rng::seed_from_u64(200);
        let sk = StaticSecret::random_from_rng(&mut rng);
        let pk = PublicKey::from(&sk);

        let envelope =
            seal::<X25519, TestDomain>(&pk, &NOTE.to_vec(), AAD, &mut rng).unwrap();
        let expected = decode_hex(include_str!("golden/x25519.hex"));
        assert_eq!(envelope.as_bytes(), expected.as_slice());
    }
}

#[cfg(feature = "grumpkin")]
mod golden_grumpkin {
    use ark_ec::{
        AffineRepr,
        CurveGroup,
    };
    use ark_ff::PrimeField;
    use ark_grumpkin::{
        Affine,
        Fr,
    };
    use sealring::{
        Grumpkin,
        seal,
        test_util::TestDomain,
    };
    use rand_chacha::ChaCha20Rng;
    use rand_core::{
        CryptoRng,
        SeedableRng,
    };

    use super::{
        AAD,
        NOTE,
        decode_hex,
    };

    fn keypair(rng: &mut impl CryptoRng) -> (Fr, Affine) {
        let mut bytes = [0u8; 64];
        rng.fill_bytes(&mut bytes);
        let sk = Fr::from_le_bytes_mod_order(&bytes);
        let pk = (Affine::generator() * sk).into_affine();
        (sk, pk)
    }

    /// Frozen suite v1 wire format for grumpkin. A mismatch is a format break.
    #[test]
    fn matches_frozen_vector() {
        let mut rng = ChaCha20Rng::seed_from_u64(300);
        let (_sk, pk) = keypair(&mut rng);

        let envelope =
            seal::<Grumpkin, TestDomain>(&pk, &NOTE.to_vec(), AAD, &mut rng).unwrap();
        let expected = decode_hex(include_str!("golden/grumpkin.hex"));
        assert_eq!(envelope.as_bytes(), expected.as_slice());
    }
}

/// Prints the current suite v1 wire-format hex for every adapter enabled in
/// this build. Run with the target adapter's feature on, for example
/// `cargo nextest run -p sealring --features k256,test-helpers,std --
/// --ignored golden_regen`, and paste the printed hex into
/// `tests/golden/<adapter>.hex`.
#[test]
#[ignore]
fn golden_regen() {
    let recipient = [0x42u8; 32];
    let mut rng = ChaCha20Rng::seed_from_u64(0);
    let envelope =
        seal::<MockKem, TestDomain>(&recipient, &NOTE.to_vec(), AAD, &mut rng).unwrap();
    println!("mockkem {}", encode_hex(envelope.as_bytes()));

    #[cfg(feature = "k256")]
    {
        use k256::{
            SecretKey,
            elliptic_curve::Generate,
        };
        use sealring::K256;

        let mut rng = ChaCha20Rng::seed_from_u64(100);
        let sk = SecretKey::generate_from_rng(&mut rng);
        let pk = sk.public_key();
        let envelope =
            seal::<K256, TestDomain>(&pk, &NOTE.to_vec(), AAD, &mut rng).unwrap();
        println!("k256 {}", encode_hex(envelope.as_bytes()));
    }

    #[cfg(feature = "x25519")]
    {
        use sealring::X25519;
        use x25519_dalek::{
            PublicKey,
            StaticSecret,
        };

        let mut rng = ChaCha20Rng::seed_from_u64(200);
        let sk = StaticSecret::random_from_rng(&mut rng);
        let pk = PublicKey::from(&sk);
        let envelope =
            seal::<X25519, TestDomain>(&pk, &NOTE.to_vec(), AAD, &mut rng).unwrap();
        println!("x25519 {}", encode_hex(envelope.as_bytes()));
    }

    #[cfg(feature = "grumpkin")]
    {
        use ark_ec::{
            AffineRepr,
            CurveGroup,
        };
        use ark_ff::PrimeField;
        use ark_grumpkin::{
            Affine,
            Fr,
        };
        use sealring::Grumpkin;
        use rand_core::Rng;

        let mut rng = ChaCha20Rng::seed_from_u64(300);
        let mut sk_bytes = [0u8; 64];
        rng.fill_bytes(&mut sk_bytes);
        let sk = Fr::from_le_bytes_mod_order(&sk_bytes);
        let pk = (Affine::generator() * sk).into_affine();
        let envelope =
            seal::<Grumpkin, TestDomain>(&pk, &NOTE.to_vec(), AAD, &mut rng).unwrap();
        println!("grumpkin {}", encode_hex(envelope.as_bytes()));
    }
}
