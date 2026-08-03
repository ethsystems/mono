#![cfg(feature = "test-helpers")]

use oring::{
    SealedNote,
    open,
    seal,
    test_util::{
        MockKem,
        TestDomain,
    },
};
use proptest::prelude::*;
use rand_chacha::ChaCha20Rng;
use rand_core::{
    Rng,
    SeedableRng,
};

proptest! {
    #[test]
    fn mockkem_round_trips(
        seed in any::<u64>(),
        note in prop::collection::vec(any::<u8>(), 0..256),
        aad in prop::collection::vec(any::<u8>(), 0..64),
    ) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let mut recipient = [0u8; 32];
        rng.fill_bytes(&mut recipient);

        let envelope =
            seal::<MockKem, TestDomain>(&recipient, &note, &aad, &mut rng).unwrap();
        let opened =
            open::<MockKem, TestDomain, _>(&recipient, &recipient, &envelope, &aad)
                .unwrap();
        prop_assert_eq!(opened, Some(note));
    }

    #[test]
    fn mockkem_bit_flip_never_opens(
        seed in any::<u64>(),
        note in prop::collection::vec(any::<u8>(), 1..256),
        aad in prop::collection::vec(any::<u8>(), 0..64),
        flip_index in any::<usize>(),
    ) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let mut recipient = [0u8; 32];
        rng.fill_bytes(&mut recipient);

        let envelope =
            seal::<MockKem, TestDomain>(&recipient, &note, &aad, &mut rng).unwrap();
        let mut bytes = envelope.as_bytes().to_vec();
        let idx = flip_index % bytes.len();
        bytes[idx] ^= 0x01;

        if let Ok(parsed) = SealedNote::<MockKem, Vec<u8>>::parse(bytes) {
            let result =
                open::<MockKem, TestDomain, _>(&recipient, &recipient, &parsed, &aad);
            prop_assert!(!matches!(result, Ok(Some(_))));
        }
    }

    #[test]
    fn mockkem_wrong_key_never_opens(
        seed in any::<u64>(),
        note in prop::collection::vec(any::<u8>(), 0..256),
        aad in prop::collection::vec(any::<u8>(), 0..64),
    ) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let mut recipient = [0u8; 32];
        let mut stranger = [0u8; 32];
        rng.fill_bytes(&mut recipient);
        rng.fill_bytes(&mut stranger);
        prop_assume!(recipient != stranger);

        let envelope =
            seal::<MockKem, TestDomain>(&recipient, &note, &aad, &mut rng).unwrap();
        let result = open::<MockKem, TestDomain, _>(&stranger, &stranger, &envelope, &aad);
        prop_assert!(matches!(result, Ok(None)));
    }
}

#[cfg(feature = "k256")]
mod k256_roundtrip {
    use k256::{
        SecretKey,
        elliptic_curve::Generate,
    };
    use oring::{
        K256,
        SealedNote,
        open,
        seal,
        test_util::TestDomain,
    };
    use proptest::prelude::*;
    use rand_chacha::ChaCha20Rng;
    use rand_core::SeedableRng;

    proptest! {
        #[test]
        fn round_trips(
            seed in any::<u64>(),
            note in prop::collection::vec(any::<u8>(), 0..256),
            aad in prop::collection::vec(any::<u8>(), 0..64),
        ) {
            let mut rng = ChaCha20Rng::seed_from_u64(seed);
            let sk = SecretKey::generate_from_rng(&mut rng);
            let pk = sk.public_key();

            let envelope = seal::<K256, TestDomain>(&pk, &note, &aad, &mut rng).unwrap();
            let opened = open::<K256, TestDomain, _>(&sk, &pk, &envelope, &aad).unwrap();
            prop_assert_eq!(opened, Some(note));
        }

        #[test]
        fn bit_flip_never_opens(
            seed in any::<u64>(),
            note in prop::collection::vec(any::<u8>(), 1..256),
            aad in prop::collection::vec(any::<u8>(), 0..64),
            flip_index in any::<usize>(),
        ) {
            let mut rng = ChaCha20Rng::seed_from_u64(seed);
            let sk = SecretKey::generate_from_rng(&mut rng);
            let pk = sk.public_key();

            let envelope = seal::<K256, TestDomain>(&pk, &note, &aad, &mut rng).unwrap();
            let mut bytes = envelope.as_bytes().to_vec();
            let idx = flip_index % bytes.len();
            bytes[idx] ^= 0x01;

            if let Ok(parsed) = SealedNote::<K256, Vec<u8>>::parse(bytes) {
                let result = open::<K256, TestDomain, _>(&sk, &pk, &parsed, &aad);
                prop_assert!(!matches!(result, Ok(Some(_))));
            }
        }

        #[test]
        fn wrong_key_never_opens(
            seed in any::<u64>(),
            note in prop::collection::vec(any::<u8>(), 0..256),
            aad in prop::collection::vec(any::<u8>(), 0..64),
        ) {
            let mut rng = ChaCha20Rng::seed_from_u64(seed);
            let sk_a = SecretKey::generate_from_rng(&mut rng);
            let pk_a = sk_a.public_key();
            let sk_b = SecretKey::generate_from_rng(&mut rng);
            let pk_b = sk_b.public_key();

            let envelope = seal::<K256, TestDomain>(&pk_a, &note, &aad, &mut rng).unwrap();
            let result = open::<K256, TestDomain, _>(&sk_b, &pk_b, &envelope, &aad);
            prop_assert!(matches!(result, Ok(None)));
        }
    }
}

#[cfg(feature = "x25519")]
mod x25519_roundtrip {
    use oring::{
        SealedNote,
        X25519,
        open,
        seal,
        test_util::TestDomain,
    };
    use proptest::prelude::*;
    use rand_chacha::ChaCha20Rng;
    use rand_core::SeedableRng;
    use x25519_dalek::{
        PublicKey,
        StaticSecret,
    };

    proptest! {
        #[test]
        fn round_trips(
            seed in any::<u64>(),
            note in prop::collection::vec(any::<u8>(), 0..256),
            aad in prop::collection::vec(any::<u8>(), 0..64),
        ) {
            let mut rng = ChaCha20Rng::seed_from_u64(seed);
            let sk = StaticSecret::random_from_rng(&mut rng);
            let pk = PublicKey::from(&sk);

            let envelope = seal::<X25519, TestDomain>(&pk, &note, &aad, &mut rng).unwrap();
            let opened = open::<X25519, TestDomain, _>(&sk, &pk, &envelope, &aad).unwrap();
            prop_assert_eq!(opened, Some(note));
        }

        #[test]
        fn bit_flip_never_opens(
            seed in any::<u64>(),
            note in prop::collection::vec(any::<u8>(), 1..256),
            aad in prop::collection::vec(any::<u8>(), 0..64),
            flip_index in any::<usize>(),
        ) {
            let mut rng = ChaCha20Rng::seed_from_u64(seed);
            let sk = StaticSecret::random_from_rng(&mut rng);
            let pk = PublicKey::from(&sk);

            let envelope = seal::<X25519, TestDomain>(&pk, &note, &aad, &mut rng).unwrap();
            let mut bytes = envelope.as_bytes().to_vec();
            let idx = flip_index % bytes.len();
            bytes[idx] ^= 0x01;

            if let Ok(parsed) = SealedNote::<X25519, Vec<u8>>::parse(bytes) {
                let result = open::<X25519, TestDomain, _>(&sk, &pk, &parsed, &aad);
                prop_assert!(!matches!(result, Ok(Some(_))));
            }
        }

        #[test]
        fn wrong_key_never_opens(
            seed in any::<u64>(),
            note in prop::collection::vec(any::<u8>(), 0..256),
            aad in prop::collection::vec(any::<u8>(), 0..64),
        ) {
            let mut rng = ChaCha20Rng::seed_from_u64(seed);
            let sk_a = StaticSecret::random_from_rng(&mut rng);
            let pk_a = PublicKey::from(&sk_a);
            let sk_b = StaticSecret::random_from_rng(&mut rng);
            let pk_b = PublicKey::from(&sk_b);

            let envelope = seal::<X25519, TestDomain>(&pk_a, &note, &aad, &mut rng).unwrap();
            let result = open::<X25519, TestDomain, _>(&sk_b, &pk_b, &envelope, &aad);
            prop_assert!(matches!(result, Ok(None)));
        }
    }
}

#[cfg(feature = "grumpkin")]
mod grumpkin_roundtrip {
    use ark_ec::{
        AffineRepr,
        CurveGroup,
    };
    use ark_ff::PrimeField;
    use ark_grumpkin::{
        Affine,
        Fr,
    };
    use oring::{
        Grumpkin,
        SealedNote,
        open,
        seal,
        test_util::TestDomain,
    };
    use proptest::prelude::*;
    use rand_chacha::ChaCha20Rng;
    use rand_core::{
        CryptoRng,
        SeedableRng,
    };

    fn keypair(rng: &mut impl CryptoRng) -> (Fr, Affine) {
        let mut bytes = [0u8; 64];
        rng.fill_bytes(&mut bytes);
        let sk = Fr::from_le_bytes_mod_order(&bytes);
        let pk = (Affine::generator() * sk).into_affine();
        (sk, pk)
    }

    proptest! {
        #[test]
        fn round_trips(
            seed in any::<u64>(),
            note in prop::collection::vec(any::<u8>(), 0..256),
            aad in prop::collection::vec(any::<u8>(), 0..64),
        ) {
            let mut rng = ChaCha20Rng::seed_from_u64(seed);
            let (sk, pk) = keypair(&mut rng);

            let envelope = seal::<Grumpkin, TestDomain>(&pk, &note, &aad, &mut rng).unwrap();
            let opened = open::<Grumpkin, TestDomain, _>(&sk, &pk, &envelope, &aad).unwrap();
            prop_assert_eq!(opened, Some(note));
        }

        #[test]
        fn bit_flip_never_opens(
            seed in any::<u64>(),
            note in prop::collection::vec(any::<u8>(), 1..256),
            aad in prop::collection::vec(any::<u8>(), 0..64),
            flip_index in any::<usize>(),
        ) {
            let mut rng = ChaCha20Rng::seed_from_u64(seed);
            let (sk, pk) = keypair(&mut rng);

            let envelope = seal::<Grumpkin, TestDomain>(&pk, &note, &aad, &mut rng).unwrap();
            let mut bytes = envelope.as_bytes().to_vec();
            let idx = flip_index % bytes.len();
            bytes[idx] ^= 0x01;

            if let Ok(parsed) = SealedNote::<Grumpkin, Vec<u8>>::parse(bytes) {
                let result = open::<Grumpkin, TestDomain, _>(&sk, &pk, &parsed, &aad);
                prop_assert!(!matches!(result, Ok(Some(_))));
            }
        }

        #[test]
        fn wrong_key_never_opens(
            seed in any::<u64>(),
            note in prop::collection::vec(any::<u8>(), 0..256),
            aad in prop::collection::vec(any::<u8>(), 0..64),
        ) {
            let mut rng = ChaCha20Rng::seed_from_u64(seed);
            let (_sk_a, pk_a) = keypair(&mut rng);
            let (sk_b, pk_b) = keypair(&mut rng);

            let envelope = seal::<Grumpkin, TestDomain>(&pk_a, &note, &aad, &mut rng).unwrap();
            let result = open::<Grumpkin, TestDomain, _>(&sk_b, &pk_b, &envelope, &aad);
            prop_assert!(matches!(result, Ok(None)));
        }
    }
}
