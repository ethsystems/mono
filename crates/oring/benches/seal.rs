use std::hint::black_box;

use criterion::{
    Criterion,
    Throughput,
    criterion_group,
    criterion_main,
};
use oring::{
    Grumpkin,
    K256,
    Kem,
    X25519,
    seal,
    test_util::TestDomain,
};
use rand_chacha::ChaCha20Rng;
use rand_core::SeedableRng;

mod common;
use common::{
    AAD,
    GenKeypair,
    bench_recipient,
};

/// Note sizes in bytes. The largest sits just under the 65536-byte
/// `MAX_CT_LEN` ceiling once the 16-byte AEAD tag is appended.
const NOTE_LENS: [usize; 4] = [48, 1_024, 16_384, 65_000];

/// Seed for the sender rng.
const SEAL_SEED: u64 = 1_337;

fn bench_seal_adapter<K>(c: &mut Criterion, adapter: &str)
where
    K: Kem + GenKeypair,
{
    let mut group = c.benchmark_group(format!("oring::seal/adapter={adapter}"));

    let me = bench_recipient::<K>();
    let pk = me.public_key();

    // ChaCha20 holds its seed for the whole run and its stream outlasts any
    // benchmark, so one instance hoisted here leaves each timed iteration
    // paying exactly the one fill its encapsulation asks for.
    let mut rng = ChaCha20Rng::seed_from_u64(SEAL_SEED);

    for len in NOTE_LENS {
        // A non-zero fill keeps the pages resident, so the timed loop reads
        // the buffer rather than faulting it in.
        let note = vec![0xA5u8; len];

        // A seal that fails returns before the AEAD, reporting a fast time
        // for work it never did.
        assert!(
            seal::<K, TestDomain>(pk, &note, AAD, &mut rng).is_ok(),
            "seal of a {len}-byte note must succeed under {adapter}"
        );

        group.throughput(Throughput::Bytes(len as u64));
        group.bench_function(format!("bytes={len}"), |b| {
            b.iter(|| {
                black_box(seal::<K, TestDomain>(
                    black_box(pk),
                    black_box(&note),
                    black_box(AAD),
                    &mut rng,
                ))
            });
        });
    }

    group.finish();
}

fn bench_seal_x25519(c: &mut Criterion) {
    bench_seal_adapter::<X25519>(c, "x25519");
}

fn bench_seal_k256(c: &mut Criterion) {
    bench_seal_adapter::<K256>(c, "k256");
}

fn bench_seal_grumpkin(c: &mut Criterion) {
    bench_seal_adapter::<Grumpkin>(c, "grumpkin");
}

criterion_group!(
    benches,
    bench_seal_x25519,
    bench_seal_k256,
    bench_seal_grumpkin
);
criterion_main!(benches);
