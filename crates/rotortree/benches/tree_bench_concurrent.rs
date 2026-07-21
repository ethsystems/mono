use criterion::{
    BatchSize,
    BenchmarkId,
    Throughput,
    criterion_group,
    criterion_main,
};
use rotortree::{
    Blake3Hasher,
    LeanIMT,
};
use std::sync::Arc;

mod common;
use common::generate_leaves;

#[crabtime::function]
fn bench_insert_single(n_values: Vec<usize>) {
    for n in n_values {
        crabtime::output! {
            fn insert_single_n{{n}}(c: &mut criterion::Criterion) {
                let mut group = c.benchmark_group(concat!("insert_single/n", {{n}}));
                for count in [1_000usize, 10_000, 100_000] {
                    let leaves = generate_leaves(count);
                    group.throughput(Throughput::Elements(count as u64));
                    group.bench_with_input(
                        BenchmarkId::from_parameter(count),
                        &leaves,
                        |b, leaves| {
                            b.iter_batched_ref(
                                || LeanIMT::<Blake3Hasher, {{n}}, 32>::new(Blake3Hasher),
                                |tree| {
                                    for &leaf in leaves {
                                        std::hint::black_box(tree.insert(leaf).unwrap());
                                    }
                                },
                                BatchSize::LargeInput,
                            );
                        },
                    );
                }
                group.finish();
            }
        }
    }
}

#[crabtime::function]
fn bench_insert_many(n_values: Vec<usize>) {
    for n in n_values {
        crabtime::output! {
            fn insert_many_n{{n}}(c: &mut criterion::Criterion) {
                let mut group = c.benchmark_group(concat!("insert_many/n", {{n}}));
                for count in [1_000usize, 10_000, 100_000, 1_000_000] {
                    let leaves = generate_leaves(count);
                    group.throughput(Throughput::Elements(count as u64));
                    group.bench_with_input(
                        BenchmarkId::from_parameter(count),
                        &leaves,
                        |b, leaves| {
                            b.iter_batched_ref(
                                || LeanIMT::<Blake3Hasher, {{n}}, 32>::new(Blake3Hasher),
                                |tree| {
                                    std::hint::black_box(tree.insert_many(leaves).unwrap());
                                },
                                BatchSize::LargeInput,
                            );
                        },
                    );
                }
                group.finish();
            }
        }
    }
}

#[crabtime::function]
fn bench_insert_incremental(n_values: Vec<usize>) {
    for n in n_values {
        crabtime::output! {
            fn insert_incremental_n{{n}}(c: &mut criterion::Criterion) {
                let mut group = c.benchmark_group(concat!("insert_incremental/n", {{n}}));
                for count in [10_000usize, 100_000, 1_000_000] {
                    let all_leaves = generate_leaves(count);
                    let half = count / 2;
                    let (first_half, second_half) = all_leaves.split_at(half);
                    let first_half = first_half.to_vec();
                    let second_half = second_half.to_vec();
                    group.throughput(Throughput::Elements(second_half.len() as u64));
                    group.bench_with_input(
                        BenchmarkId::from_parameter(count),
                        &(first_half.clone(), second_half.clone()),
                        |b, (first_half, second_half)| {
                            b.iter_batched_ref(
                                || {
                                    let tree = LeanIMT::<Blake3Hasher, {{n}}, 32>::new(Blake3Hasher);
                                    tree.insert_many(first_half).unwrap();
                                    tree
                                },
                                |tree| {
                                    std::hint::black_box(tree.insert_many(second_half).unwrap());
                                },
                                BatchSize::LargeInput,
                            );
                        },
                    );
                }
                group.finish();
            }
        }
    }
}

#[crabtime::function]
fn bench_generate_proof(n_values: Vec<usize>) {
    for n in n_values {
        crabtime::output! {
            fn generate_proof_n{{n}}(c: &mut criterion::Criterion) {
                let mut group = c.benchmark_group(concat!("generate_proof/n", {{n}}));
                for count in [1_000usize, 10_000, 100_000, 1_000_000] {
                    let leaves = generate_leaves(count);
                    let tree = LeanIMT::<Blake3Hasher, {{n}}, 32>::new(Blake3Hasher);
                    tree.insert_many(&leaves).unwrap();
                    let snap = tree.snapshot();
                    let mid = (count / 2) as u64;
                    group.bench_function(BenchmarkId::from_parameter(count), |b| {
                        b.iter(|| {
                            std::hint::black_box(snap.generate_proof(mid).unwrap());
                        });
                    });
                }
                group.finish();
            }
        }
    }
}

#[crabtime::function]
fn bench_verify_proof(n_values: Vec<usize>) {
    for n in n_values {
        crabtime::output! {
            fn verify_proof_n{{n}}(c: &mut criterion::Criterion) {
                let mut group = c.benchmark_group(concat!("verify_proof/n", {{n}}));
                for count in [1_000usize, 10_000, 100_000, 1_000_000] {
                    let leaves = generate_leaves(count);
                    let tree = LeanIMT::<Blake3Hasher, {{n}}, 32>::new(Blake3Hasher);
                    tree.insert_many(&leaves).unwrap();
                    let snap = tree.snapshot();
                    let proof = snap.generate_proof(0).unwrap();
                    let th = Blake3Hasher;
                    group.bench_function(BenchmarkId::from_parameter(count), |b| {
                        b.iter(|| {
                            std::hint::black_box(proof.verify(&th).unwrap());
                        });
                    });
                }
                group.finish();
            }
        }
    }
}

#[crabtime::function]
fn bench_snapshot(n_values: Vec<usize>) {
    for n in n_values {
        crabtime::output! {
            fn snapshot_n{{n}}(c: &mut criterion::Criterion) {
                let mut group = c.benchmark_group(concat!("snapshot/n", {{n}}));
                for count in [1_000usize, 10_000, 100_000, 1_000_000] {
                    let leaves = generate_leaves(count);
                    let tree = LeanIMT::<Blake3Hasher, {{n}}, 32>::new(Blake3Hasher);
                    tree.insert_many(&leaves).unwrap();
                    group.bench_function(BenchmarkId::from_parameter(count), |b| {
                        b.iter(|| {
                            std::hint::black_box(tree.snapshot());
                        });
                    });
                }
                group.finish();
            }
        }
    }
}

#[crabtime::function]
fn bench_concurrent_contention(n_values: Vec<usize>) {
    for n in n_values {
        crabtime::output! {
            fn concurrent_contention_n{{n}}(c: &mut criterion::Criterion) {
                let mut group = c.benchmark_group(concat!("concurrent_contention/n", {{n}}));
                for count in [10_000usize, 100_000] {
                    let leaves = generate_leaves(count);
                    group.throughput(Throughput::Elements(count as u64));
                    group.bench_with_input(
                        BenchmarkId::from_parameter(count),
                        &leaves,
                        |b, leaves| {
                            b.iter_batched(
                                || Arc::new(LeanIMT::<Blake3Hasher, {{n}}, 32>::new(Blake3Hasher)),
                                |tree| {
                                    std::thread::scope(|s| {
                                        for _ in 0..4 {
                                            let tree = Arc::clone(&tree);
                                            s.spawn(move || {
                                                loop {
                                                    let snap = tree.snapshot();
                                                    let size = snap.size();
                                                    if size > 0 {
                                                        let _ = std::hint::black_box(
                                                            snap.generate_proof(size / 2)
                                                        );
                                                    }
                                                    if size >= count as u64 {
                                                        break;
                                                    }
                                                }
                                            });
                                        }
                                        tree.insert_many(leaves).unwrap();
                                    });
                                },
                                BatchSize::PerIteration,
                            );
                        },
                    );
                }
                group.finish();
            }
        }
    }
}

#[crabtime::function]
fn define_harness(n_values: Vec<usize>) {
    for n in n_values {
        crabtime::output! {
            bench_insert_single!([{{n}}]);
            bench_insert_many!([{{n}}]);
            bench_insert_incremental!([{{n}}]);
            bench_generate_proof!([{{n}}]);
            bench_verify_proof!([{{n}}]);
            bench_snapshot!([{{n}}]);
            bench_concurrent_contention!([{{n}}]);

            criterion_group!(
                benches_n{{n}},
                insert_single_n{{n}},
                insert_many_n{{n}},
                insert_incremental_n{{n}},
                generate_proof_n{{n}},
                verify_proof_n{{n}},
                snapshot_n{{n}},
                concurrent_contention_n{{n}}
            );
        }
    }
}

define_harness!([2, 4, 8, 16]);
criterion_main!(benches_n2, benches_n4, benches_n8, benches_n16);
