//! Seals a note to an X25519 recipient, then opens it, then scans a batch
//! that includes the sealed note among strangers' envelopes.
//!
//! Run with `cargo run -p sealring --example seal_open --features x25519`.

fn main() {
    use std::convert::Infallible;

    use rand_chacha::ChaCha20Rng;
    use rand_core::SeedableRng;
    use sealring::{
        Domain,
        Recipient,
        Scanner,
        X25519,
        open,
        seal,
    };
    use x25519_dalek::{
        PublicKey,
        StaticSecret,
    };

    struct WalletDomain;

    impl Domain for WalletDomain {
        // this codec accepts every note; a domain with a real format names
        // its own rejection type here.
        type Error = Infallible;
        type Note = Vec<u8>;

        const DOMAIN_TAG: &'static str = "sealring-example/v1";

        fn encode_note(note: &Self::Note, out: &mut Vec<u8>) -> Result<(), Self::Error> {
            out.extend_from_slice(note);
            Ok(())
        }

        fn decode_note(bytes: &[u8]) -> Result<Self::Note, Self::Error> {
            Ok(bytes.to_vec())
        }
    }

    // fixed seed for reproducible example output; a real wallet draws its
    // ephemeral secrets from an OS CSPRNG.
    let mut rng = ChaCha20Rng::seed_from_u64(42);

    // one value carries the keypair, so the public key the sender seals to
    // and the one the KDF binds on open cannot drift apart.
    let me = Recipient::<X25519>::new(StaticSecret::random_from_rng(&mut rng));

    let note = b"pay alice 5 units".to_vec();
    let aad = b"block-height:1000";

    let envelope =
        seal::<X25519, WalletDomain>(me.public_key(), &note, aad, &mut rng).unwrap();
    println!("sealed {} bytes", envelope.as_bytes().len());

    let opened = open::<X25519, WalletDomain, _>(&me, &envelope, aad).unwrap();
    assert_eq!(opened, Some(note.clone()));
    println!("opened: {:?}", opened.unwrap());

    // a wallet scans a mixed batch instead of opening each envelope by hand.
    let stranger_sk = StaticSecret::random_from_rng(&mut rng);
    let stranger_pk = PublicKey::from(&stranger_sk);
    let not_mine = seal::<X25519, WalletDomain>(
        &stranger_pk,
        &b"pay bob 1 unit".to_vec(),
        aad,
        &mut rng,
    )
    .unwrap();

    let batch = vec![&not_mine, &envelope];
    let mut scanner = Scanner::<X25519, WalletDomain>::new(me);
    for (index, result) in scanner.scan(batch, aad) {
        println!("batch[{index}] mine: {:?}", result.unwrap());
    }
}
