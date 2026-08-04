//! Per-operation costs behind a scan, so a whole-scan delta can be attributed
//! rather than guessed at.
//!
//! `op=decap` prices the KEM alone and `op=open_miss` prices the KEM plus
//! everything the crate itself does for a missed envelope, so the gap between
//! the two is the suite v1 KDF and commit compare. `op=decap_batch` prices the
//! batched KEM against the scalar one over the same epks.

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
    SCAN_CHUNK,
    X25519,
    open,
    test_util::TestDomain,
};
use rand_chacha::ChaCha20Rng;
use rand_core::SeedableRng;

mod common;
use common::{
    AAD,
    GenKeypair,
    NOTE_LEN,
    bench_recipient,
    build_envelopes,
};

/// Envelopes each probe runs over. One chunk, so the batched probe measures
/// exactly the batch a scan hands to the adapter.
const PROBE_COUNT: usize = SCAN_CHUNK;

fn bench_ops<K>(c: &mut Criterion, adapter: &str)
where
    K: Kem + GenKeypair,
{
    let me = bench_recipient::<K>();
    // Addressed to strangers, so every envelope decapsulates and then stops
    // at the commit compare: the miss path a scan spends nearly all its time on.
    let envelopes =
        build_envelopes::<K, TestDomain>(me.public_key(), PROBE_COUNT, 0.0, NOTE_LEN);
    let epks: Vec<&[u8]> = envelopes.iter().map(|envelope| envelope.epk()).collect();

    let mut group = c.benchmark_group(format!("oring::micro/adapter={adapter}"));
    group.throughput(Throughput::Elements(PROBE_COUNT as u64));

    group.bench_function("op=decap", |b| {
        b.iter(|| {
            for epk in black_box(&epks).iter().copied() {
                black_box(K::decap(black_box(me.secret_key()), epk));
            }
        });
    });

    group.bench_function("op=decap_batch", |b| {
        let mut out: Vec<Option<K::SharedSecret>> =
            (0..PROBE_COUNT).map(|_| None).collect();
        b.iter(|| {
            K::decap_batch(
                black_box(me.secret_key()),
                black_box(&epks),
                black_box(&mut out),
            );
        });
    });

    // The sealing half: an ephemeral keypair plus the same shared-secret
    // agreement `op=decap` prices, so the gap between them is the keygen.
    group.bench_function("op=encap", |b| {
        let pk = me.public_key();
        let mut rng = ChaCha20Rng::seed_from_u64(1);
        b.iter(|| {
            for _ in 0..PROBE_COUNT {
                black_box(K::encap(black_box(&mut rng), black_box(pk)));
            }
        });
    });

    group.bench_function("op=open_miss", |b| {
        b.iter(|| {
            for envelope in black_box(&envelopes) {
                let _ = black_box(open::<K, TestDomain, _>(
                    black_box(&me),
                    envelope,
                    black_box(AAD),
                ));
            }
        });
    });

    group.finish();
}

fn bench_micro_x25519(c: &mut Criterion) {
    bench_ops::<X25519>(c, "x25519");
}

fn bench_micro_k256(c: &mut Criterion) {
    bench_ops::<K256>(c, "k256");
}

fn bench_micro_grumpkin(c: &mut Criterion) {
    bench_ops::<Grumpkin>(c, "grumpkin");
}

criterion_group!(
    benches,
    bench_micro_x25519,
    bench_micro_k256,
    bench_micro_grumpkin
);
criterion_main!(benches);
