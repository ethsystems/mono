use std::hint::black_box;

use criterion::{
    Criterion,
    Throughput,
    criterion_group,
    criterion_main,
};
use sealring::{
    Grumpkin,
    K256,
    Kem,
    Scanner,
    X25519,
    open,
    test_util::TestDomain,
};

mod common;
use common::{
    AAD,
    GenKeypair,
    NOTE_LEN,
    bench_recipient,
    build_envelopes,
};

const COUNTS: [usize; 2] = [1_024, 16_384];
const HIT_RATES: [f64; 2] = [0.0, 0.01];

/// Grumpkin decapsulates through arkworks generic short-Weierstrass
/// arithmetic at roughly 3.3x the per-envelope cost of the other two
/// adapters, which puts a 16384-envelope scan at about 1.1 s per iteration,
/// so it scans the small batch only.
const GRUMPKIN_COUNTS: [usize; 1] = [1_024];

/// Note sizes for the note-size sweep, spanning the 48-byte baseline up to
/// the largest note that stays under the crate's `MAX_CT_LEN` once the
/// 16-byte AEAD tag is added.
const NOTE_LENS: [usize; 4] = [48, 1_024, 16_384, 65_000];

/// Batch size for the note-size sweep. Holding it at the small count keeps
/// the largest cell at roughly 66 MB of envelopes.
const NOTE_LEN_COUNT: usize = 1_024;

/// Hit rates for the note-size sweep. A 0% batch stops at the commit compare
/// and a 100% batch decrypts every envelope, so the pair separates the AEAD
/// cost from the KEM cost.
const NOTE_LEN_HIT_RATES: [f64; 2] = [0.0, 1.0];

// Sync is only exercised by the parallel path below, which shares the
// scanner's secret key across tasks; every shipped adapter holds its key and
// shared secret in a plain byte or field-element struct, and satisfies it
// trivially.
fn bench_scan_adapter<K>(c: &mut Criterion, adapter: &str, counts: &[usize])
where
    K: Kem + GenKeypair,
    K::SecretKey: Sync,
{
    for n in counts.iter().copied() {
        let mut group =
            c.benchmark_group(format!("sealring::scan/adapter={adapter} n={n}"));
        group.throughput(Throughput::Elements(n as u64));

        for hit_rate in HIT_RATES {
            let hit_pct = (hit_rate * 100.0).round() as u32;

            let naive = bench_recipient::<K>();
            let scan = bench_recipient::<K>();
            let envelopes = build_envelopes::<K, TestDomain>(
                naive.public_key(),
                n,
                hit_rate,
                NOTE_LEN,
            );
            let refs: Vec<_> = envelopes.iter().collect();

            group.bench_function(format!("method=naive hit={hit_pct}%"), |b| {
                b.iter(|| {
                    for envelope in black_box(&refs).iter().copied() {
                        let _ = black_box(open::<K, TestDomain, _>(
                            &naive,
                            envelope,
                            black_box(AAD),
                        ));
                    }
                });
            });

            let mut scanner = Scanner::<K, TestDomain>::new(scan);
            group.bench_function(format!("method=chunked hit={hit_pct}%"), |b| {
                b.iter(|| {
                    black_box(
                        scanner
                            .scan(black_box(&refs).iter().copied(), black_box(AAD))
                            .count(),
                    );
                });
            });

            #[cfg(feature = "parallel")]
            {
                group.bench_function(
                    format!("method=chunked_parallel hit={hit_pct}%"),
                    |b| {
                        b.iter(|| {
                            black_box(
                                scanner
                                    .scan_parallel(
                                        black_box(&refs).iter().copied(),
                                        black_box(AAD),
                                    )
                                    .count(),
                            );
                        });
                    },
                );
            }
        }

        group.finish();
    }
}

fn bench_scan_x25519(c: &mut Criterion) {
    bench_scan_adapter::<X25519>(c, "x25519", &COUNTS);
}

fn bench_scan_k256(c: &mut Criterion) {
    bench_scan_adapter::<K256>(c, "k256", &COUNTS);
}

fn bench_scan_grumpkin(c: &mut Criterion) {
    bench_scan_adapter::<Grumpkin>(c, "grumpkin", &GRUMPKIN_COUNTS);
}

/// Sweeps note size against hit rate on one adapter, locating the size at
/// which the AEAD pass costs as much as the decapsulation that precedes it.
///
/// A missed envelope stops at the commit compare, so the 0% row prices the
/// KEM alone; the 100% row adds one decryption per envelope over the same
/// batch.
fn bench_scan_note_len(c: &mut Criterion) {
    for note_len in NOTE_LENS {
        let mut group = c.benchmark_group(format!(
            "sealring::scan/adapter=x25519 n={NOTE_LEN_COUNT} note_len={note_len}"
        ));
        group.throughput(Throughput::Elements(NOTE_LEN_COUNT as u64));

        for hit_rate in NOTE_LEN_HIT_RATES {
            let hit_pct = (hit_rate * 100.0).round() as u32;

            let me = bench_recipient::<X25519>();
            let envelopes = build_envelopes::<X25519, TestDomain>(
                me.public_key(),
                NOTE_LEN_COUNT,
                hit_rate,
                note_len,
            );
            let refs: Vec<_> = envelopes.iter().collect();

            let mut scanner = Scanner::<X25519, TestDomain>::new(me);
            group.bench_function(format!("method=chunked hit={hit_pct}%"), |b| {
                b.iter(|| {
                    black_box(
                        scanner
                            .scan(black_box(&refs).iter().copied(), black_box(AAD))
                            .count(),
                    );
                });
            });

            #[cfg(feature = "parallel")]
            {
                group.bench_function(
                    format!("method=chunked_parallel hit={hit_pct}%"),
                    |b| {
                        b.iter(|| {
                            black_box(
                                scanner
                                    .scan_parallel(
                                        black_box(&refs).iter().copied(),
                                        black_box(AAD),
                                    )
                                    .count(),
                            );
                        });
                    },
                );
            }
        }

        group.finish();
    }
}

criterion_group!(
    benches,
    bench_scan_x25519,
    bench_scan_k256,
    bench_scan_grumpkin,
    bench_scan_note_len
);
criterion_main!(benches);
