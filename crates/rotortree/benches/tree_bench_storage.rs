use criterion::{
    BatchSize,
    BenchmarkId,
    Throughput,
    criterion_group,
    criterion_main,
};
use rotortree::{
    Blake3Hasher,
    CheckpointPolicy,
    FlushPolicy,
    RotorTree,
    RotorTreeConfig,
};

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
                                || {
                                    let dir = tempfile::tempdir().unwrap();
                                    let config = RotorTreeConfig {
                                        path: dir.path().to_path_buf(),
                                        flush_policy: FlushPolicy::Manual,
                                        checkpoint_policy: Default::default(),
                                        tiering: Default::default(),
                                        verify_checkpoint: true,
                                    };
                                    let tree = RotorTree::<Blake3Hasher, {{n}}, 32>::open(Blake3Hasher, config).unwrap();
                                    (tree, dir)
                                },
                                |(tree, _dir)| {
                                    for &leaf in leaves {
                                        std::hint::black_box(tree.insert(leaf).unwrap());
                                    }
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
                                || {
                                    let dir = tempfile::tempdir().unwrap();
                                    let config = RotorTreeConfig {
                                        path: dir.path().to_path_buf(),
                                        flush_policy: FlushPolicy::Manual,
                                        checkpoint_policy: Default::default(),
                                        tiering: Default::default(),
                                        verify_checkpoint: true,
                                    };
                                    let tree = RotorTree::<Blake3Hasher, {{n}}, 32>::open(Blake3Hasher, config).unwrap();
                                    #[cfg(feature = "parallel")]
                                    rayon::broadcast(|_| {});
                                    (tree, dir)
                                },
                                |(tree, _dir)| {
                                    std::hint::black_box(tree.insert_many(leaves).unwrap());
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
fn bench_flush(n_values: Vec<usize>) {
    for n in n_values {
        crabtime::output! {
            fn flush_n{{n}}(c: &mut criterion::Criterion) {
                let mut group = c.benchmark_group(concat!("flush/n", {{n}}));
                for count in [1_000usize, 10_000, 100_000] {
                    let leaves = generate_leaves(count);
                    group.throughput(Throughput::Elements(count as u64));
                    group.bench_with_input(
                        BenchmarkId::from_parameter(count),
                        &leaves,
                        |b, leaves| {
                            b.iter_batched(
                                || {
                                    let dir = tempfile::tempdir().unwrap();
                                    let config = RotorTreeConfig {
                                        path: dir.path().to_path_buf(),
                                        flush_policy: FlushPolicy::Manual,
                                        checkpoint_policy: Default::default(),
                                        tiering: Default::default(),
                                        verify_checkpoint: true,
                                    };
                                    let tree = RotorTree::<Blake3Hasher, {{n}}, 32>::open(Blake3Hasher, config).unwrap();
                                    tree.insert_many(leaves).unwrap();
                                    (tree, dir)
                                },
                                |(tree, _dir)| {
                                    std::hint::black_box(tree.flush().unwrap());
                                    tree.close().unwrap();
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
fn bench_open_recover(n_values: Vec<usize>) {
    for n in n_values {
        crabtime::output! {
            fn open_recover_n{{n}}(c: &mut criterion::Criterion) {
                let mut group = c.benchmark_group(concat!("open_recover/n", {{n}}));
                for count in [1_000usize, 10_000, 100_000] {
                    let leaves = generate_leaves(count);
                    let dir = tempfile::tempdir().unwrap();
                    let path = dir.path().to_path_buf();
                    {
                        let config = RotorTreeConfig {
                            path: path.clone(),
                            flush_policy: FlushPolicy::Manual,
                            checkpoint_policy: Default::default(),
                            tiering: Default::default(),
                            verify_checkpoint: true,
                        };
                        let tree = RotorTree::<Blake3Hasher, {{n}}, 32>::open(Blake3Hasher, config).unwrap();
                        tree.insert_many(&leaves).unwrap();
                        tree.flush().unwrap();
                        tree.close().unwrap();
                    }
                    group.throughput(Throughput::Elements(count as u64));
                    group.bench_function(BenchmarkId::from_parameter(count), |b| {
                        b.iter_batched(
                            || path.clone(),
                            |path| {
                                let config = RotorTreeConfig {
                                    path,
                                    flush_policy: FlushPolicy::Manual,
                                    checkpoint_policy: Default::default(),
                                    tiering: Default::default(),
                                    verify_checkpoint: true,
                                };
                                let tree = RotorTree::<Blake3Hasher, {{n}}, 32>::open(Blake3Hasher, config).unwrap();
                                std::hint::black_box(tree.root());
                                tree.close().unwrap();
                            },
                            BatchSize::PerIteration,
                        );
                    });
                }
                group.finish();
            }
        }
    }
}

#[crabtime::function]
fn bench_mixed_workload(n_values: Vec<usize>) {
    for n in n_values {
        crabtime::output! {
            fn mixed_workload_n{{n}}(c: &mut criterion::Criterion) {
                let mut group = c.benchmark_group(concat!("mixed_workload/n", {{n}}));
                for tick in [100usize, 1_000, 10_000, 100_000] {
                    let prepop_leaves = generate_leaves(10_000);
                    let tick_leaves = generate_leaves(tick);
                    let th = Blake3Hasher;
                    group.throughput(Throughput::Elements(tick as u64));
                    group.bench_with_input(
                        BenchmarkId::from_parameter(tick),
                        &(prepop_leaves.clone(), tick_leaves.clone()),
                        |b, (prepop_leaves, tick_leaves)| {
                            b.iter_batched(
                                || {
                                    let dir = tempfile::tempdir().unwrap();
                                    let config = RotorTreeConfig {
                                        path: dir.path().to_path_buf(),
                                        flush_policy: FlushPolicy::Manual,
                                        checkpoint_policy: Default::default(),
                                        tiering: Default::default(),
                                        verify_checkpoint: true,
                                    };
                                    let tree = RotorTree::<Blake3Hasher, {{n}}, 32>::open(Blake3Hasher, config).unwrap();
                                    tree.insert_many(prepop_leaves).unwrap();
                                    tree.flush().unwrap();
                                    (tree, dir)
                                },
                                |(tree, _dir)| {
                                    let (root, _token) = tree.insert_many(tick_leaves).unwrap();
                                    std::hint::black_box(root);

                                    std::hint::black_box(tree.root());

                                    let snap = tree.snapshot();
                                    let proof_index = snap.size() / 2;
                                    let proof = snap.generate_proof(proof_index).unwrap();
                                    std::hint::black_box(&proof);

                                    std::hint::black_box(proof.verify(&th).unwrap());
                                    tree.close().unwrap();
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
fn bench_sustained_checkpoint(n_values: Vec<usize>) {
    for n in n_values {
        crabtime::output! {
            fn sustained_checkpoint_n{{n}}(c: &mut criterion::Criterion) {
                let mut group = c.benchmark_group(concat!("sustained_checkpoint/n", {{n}}));
                for count in [100_000usize, 1_000_000] {
                    for freq in [5usize, 25, 100, 500] {
                        let leaves = generate_leaves(count);
                        let th = Blake3Hasher;
                        group.throughput(Throughput::Elements(count as u64));
                        group.bench_with_input(
                            BenchmarkId::new(format!("every{freq}"), count),
                            &leaves,
                            |b, leaves| {
                                b.iter_batched_ref(
                                    || {
                                        let dir = tempfile::tempdir().unwrap();
                                        let config = RotorTreeConfig {
                                            path: dir.path().to_path_buf(),
                                            flush_policy: FlushPolicy::Manual,
                                            checkpoint_policy: CheckpointPolicy::EveryNEntries(freq as u64),
                                            tiering: Default::default(),
                                            verify_checkpoint: true,
                                        };
                                        let tree = RotorTree::<Blake3Hasher, {{n}}, 32>::open(Blake3Hasher, config).unwrap();
                                        (tree, dir)
                                    },
                                    |(tree, _dir)| {
                                        for chunk in leaves.chunks(10_000) {
                                            std::hint::black_box(tree.insert_many(chunk).unwrap());

                                            let snap = tree.snapshot();
                                            let size = snap.size();
                                            let proof = snap.generate_proof(0).unwrap();
                                            std::hint::black_box(proof.verify(&th).unwrap());
                                            let proof = snap.generate_proof(size / 2).unwrap();
                                            std::hint::black_box(proof.verify(&th).unwrap());
                                            let proof = snap.generate_proof(size - 1).unwrap();
                                            std::hint::black_box(proof.verify(&th).unwrap());
                                        }
                                    },
                                    BatchSize::PerIteration,
                                );
                            },
                        );
                    }
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
            bench_flush!([{{n}}]);
            bench_open_recover!([{{n}}]);
            bench_mixed_workload!([{{n}}]);
            bench_sustained_checkpoint!([{{n}}]);

            criterion_group!(
                benches_n{{n}},
                insert_single_n{{n}},
                insert_many_n{{n}},
                flush_n{{n}},
                open_recover_n{{n}},
                mixed_workload_n{{n}},
                sustained_checkpoint_n{{n}}
            );
        }
    }
}

define_harness!([2, 4, 8, 16]);
criterion_main!(benches_n2, benches_n4, benches_n8, benches_n16);
