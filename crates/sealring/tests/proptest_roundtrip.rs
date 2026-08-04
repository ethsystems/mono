#![cfg(feature = "test-helpers")]

use proptest::prelude::*;
use rand_chacha::ChaCha20Rng;
use rand_core::{
    Rng,
    SeedableRng,
};
use sealring::{
    Recipient,
    SealedNote,
    open,
    seal,
    test_util::{
        MockKem,
        TestDomain,
    },
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
        let me = Recipient::<MockKem>::new(recipient);

        let envelope =
            seal::<MockKem, TestDomain>(me.public_key(), &note, &aad, &mut rng).unwrap();
        let opened = open::<MockKem, TestDomain, _>(&me, &envelope, &aad).unwrap();
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
        let me = Recipient::<MockKem>::new(recipient);

        let envelope =
            seal::<MockKem, TestDomain>(me.public_key(), &note, &aad, &mut rng).unwrap();
        let mut bytes = envelope.as_bytes().to_vec();
        let idx = flip_index % bytes.len();
        bytes[idx] ^= 0x01;

        if let Ok(parsed) = SealedNote::<MockKem, Vec<u8>>::parse(bytes) {
            let result = open::<MockKem, TestDomain, _>(&me, &parsed, &aad);
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
        let result = open::<MockKem, TestDomain, _>(
            &Recipient::<MockKem>::new(stranger),
            &envelope,
            &aad,
        );
        prop_assert!(matches!(result, Ok(None)));
    }
}

#[cfg(feature = "k256")]
mod k256_roundtrip {
    use k256::{
        SecretKey,
        elliptic_curve::Generate,
    };
    use proptest::prelude::*;
    use rand_chacha::ChaCha20Rng;
    use rand_core::SeedableRng;
    use sealring::{
        K256,
        Recipient,
        SealedNote,
        open,
        seal,
        test_util::TestDomain,
    };

    proptest! {
        #[test]
        fn round_trips(
            seed in any::<u64>(),
            note in prop::collection::vec(any::<u8>(), 0..256),
            aad in prop::collection::vec(any::<u8>(), 0..64),
        ) {
            let mut rng = ChaCha20Rng::seed_from_u64(seed);
            let me = Recipient::<K256>::new(SecretKey::generate_from_rng(&mut rng));

            let envelope =
                seal::<K256, TestDomain>(me.public_key(), &note, &aad, &mut rng).unwrap();
            let opened = open::<K256, TestDomain, _>(&me, &envelope, &aad).unwrap();
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
            let me = Recipient::<K256>::new(SecretKey::generate_from_rng(&mut rng));

            let envelope =
                seal::<K256, TestDomain>(me.public_key(), &note, &aad, &mut rng).unwrap();
            let mut bytes = envelope.as_bytes().to_vec();
            let idx = flip_index % bytes.len();
            bytes[idx] ^= 0x01;

            if let Ok(parsed) = SealedNote::<K256, Vec<u8>>::parse(bytes) {
                let result = open::<K256, TestDomain, _>(&me, &parsed, &aad);
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
            let stranger = Recipient::<K256>::new(SecretKey::generate_from_rng(&mut rng));
            let me = Recipient::<K256>::new(SecretKey::generate_from_rng(&mut rng));

            let envelope =
                seal::<K256, TestDomain>(stranger.public_key(), &note, &aad, &mut rng)
                    .unwrap();
            let result = open::<K256, TestDomain, _>(&me, &envelope, &aad);
            prop_assert!(matches!(result, Ok(None)));
        }
    }
}

#[cfg(feature = "x25519")]
mod x25519_roundtrip {
    use proptest::prelude::*;
    use rand_chacha::ChaCha20Rng;
    use rand_core::SeedableRng;
    use sealring::{
        Recipient,
        SealedNote,
        X25519,
        open,
        seal,
        test_util::TestDomain,
    };
    use x25519_dalek::StaticSecret;

    proptest! {
        #[test]
        fn round_trips(
            seed in any::<u64>(),
            note in prop::collection::vec(any::<u8>(), 0..256),
            aad in prop::collection::vec(any::<u8>(), 0..64),
        ) {
            let mut rng = ChaCha20Rng::seed_from_u64(seed);
            let me = Recipient::<X25519>::new(StaticSecret::random_from_rng(&mut rng));

            let envelope =
                seal::<X25519, TestDomain>(me.public_key(), &note, &aad, &mut rng)
                    .unwrap();
            let opened = open::<X25519, TestDomain, _>(&me, &envelope, &aad).unwrap();
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
            let me = Recipient::<X25519>::new(StaticSecret::random_from_rng(&mut rng));

            let envelope =
                seal::<X25519, TestDomain>(me.public_key(), &note, &aad, &mut rng)
                    .unwrap();
            let mut bytes = envelope.as_bytes().to_vec();
            let idx = flip_index % bytes.len();
            bytes[idx] ^= 0x01;

            if let Ok(parsed) = SealedNote::<X25519, Vec<u8>>::parse(bytes) {
                let result = open::<X25519, TestDomain, _>(&me, &parsed, &aad);
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
            let stranger =
                Recipient::<X25519>::new(StaticSecret::random_from_rng(&mut rng));
            let me = Recipient::<X25519>::new(StaticSecret::random_from_rng(&mut rng));

            let envelope =
                seal::<X25519, TestDomain>(stranger.public_key(), &note, &aad, &mut rng)
                    .unwrap();
            let result = open::<X25519, TestDomain, _>(&me, &envelope, &aad);
            prop_assert!(matches!(result, Ok(None)));
        }
    }
}

#[cfg(feature = "grumpkin")]
mod grumpkin_roundtrip {
    use ark_ff::PrimeField;
    use ark_grumpkin::Fr;
    use proptest::prelude::*;
    use rand_chacha::ChaCha20Rng;
    use rand_core::{
        CryptoRng,
        SeedableRng,
    };
    use sealring::{
        Grumpkin,
        Recipient,
        SealedNote,
        open,
        seal,
        test_util::TestDomain,
    };

    /// Draws a scalar the way the adapter draws its ephemeral scalar: bytes
    /// from the caller's rng, reduced mod the scalar field order.
    fn secret_key(rng: &mut impl CryptoRng) -> Fr {
        let mut bytes = [0u8; 64];
        rng.fill_bytes(&mut bytes);
        Fr::from_le_bytes_mod_order(&bytes)
    }

    proptest! {
        #[test]
        fn round_trips(
            seed in any::<u64>(),
            note in prop::collection::vec(any::<u8>(), 0..256),
            aad in prop::collection::vec(any::<u8>(), 0..64),
        ) {
            let mut rng = ChaCha20Rng::seed_from_u64(seed);
            let me = Recipient::<Grumpkin>::new(secret_key(&mut rng));

            let envelope =
                seal::<Grumpkin, TestDomain>(me.public_key(), &note, &aad, &mut rng)
                    .unwrap();
            let opened = open::<Grumpkin, TestDomain, _>(&me, &envelope, &aad).unwrap();
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
            let me = Recipient::<Grumpkin>::new(secret_key(&mut rng));

            let envelope =
                seal::<Grumpkin, TestDomain>(me.public_key(), &note, &aad, &mut rng)
                    .unwrap();
            let mut bytes = envelope.as_bytes().to_vec();
            let idx = flip_index % bytes.len();
            bytes[idx] ^= 0x01;

            if let Ok(parsed) = SealedNote::<Grumpkin, Vec<u8>>::parse(bytes) {
                let result = open::<Grumpkin, TestDomain, _>(&me, &parsed, &aad);
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
            let stranger = Recipient::<Grumpkin>::new(secret_key(&mut rng));
            let me = Recipient::<Grumpkin>::new(secret_key(&mut rng));

            let envelope = seal::<Grumpkin, TestDomain>(
                stranger.public_key(),
                &note,
                &aad,
                &mut rng,
            )
            .unwrap();
            let result = open::<Grumpkin, TestDomain, _>(&me, &envelope, &aad);
            prop_assert!(matches!(result, Ok(None)));
        }
    }
}
