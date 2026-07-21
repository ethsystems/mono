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

mod common;
use common::generate_leaves;

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
                                    let tree = LeanIMT::<Blake3Hasher, {{n}}, 32>::new(Blake3Hasher);
                                    rayon::broadcast(|_| {});
                                    tree
                                },
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
fn bench_insert_many_chunked_100(n_values: Vec<usize>) {
    for n in n_values {
        crabtime::output! {
            fn insert_many_chunked_100_n{{n}}(c: &mut criterion::Criterion) {
                let mut group = c.benchmark_group(concat!("insert_many_chunked_100/n", {{n}}));
                for count in [10_000usize, 100_000, 1_000_000] {
                    let leaves = generate_leaves(count);
                    group.throughput(Throughput::Elements(count as u64));
                    group.bench_with_input(
                        BenchmarkId::from_parameter(count),
                        &leaves,
                        |b, leaves| {
                            b.iter_batched_ref(
                                || {
                                    let tree = LeanIMT::<Blake3Hasher, {{n}}, 32>::new(Blake3Hasher);
                                    rayon::broadcast(|_| {});
                                    tree
                                },
                                |tree| {
                                    for chunk in leaves.chunks(100) {
                                        std::hint::black_box(tree.insert_many(chunk).unwrap());
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
fn bench_insert_many_chunked_1000(n_values: Vec<usize>) {
    for n in n_values {
        crabtime::output! {
            fn insert_many_chunked_1000_n{{n}}(c: &mut criterion::Criterion) {
                let mut group = c.benchmark_group(concat!("insert_many_chunked_1000/n", {{n}}));
                for count in [10_000usize, 100_000, 1_000_000] {
                    let leaves = generate_leaves(count);
                    group.throughput(Throughput::Elements(count as u64));
                    group.bench_with_input(
                        BenchmarkId::from_parameter(count),
                        &leaves,
                        |b, leaves| {
                            b.iter_batched_ref(
                                || {
                                    let tree = LeanIMT::<Blake3Hasher, {{n}}, 32>::new(Blake3Hasher);
                                    rayon::broadcast(|_| {});
                                    tree
                                },
                                |tree| {
                                    for chunk in leaves.chunks(1000) {
                                        std::hint::black_box(tree.insert_many(chunk).unwrap());
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
                                    let mut tree = LeanIMT::<Blake3Hasher, {{n}}, 32>::new(Blake3Hasher);
                                    tree.insert_many(first_half).unwrap();
                                    rayon::broadcast(|_| {});
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
fn define_harness(n_values: Vec<usize>) {
    for n in n_values {
        crabtime::output! {
            bench_insert_many!([{{n}}]);
            bench_insert_many_chunked_100!([{{n}}]);
            bench_insert_many_chunked_1000!([{{n}}]);
            bench_insert_incremental!([{{n}}]);

            criterion_group!(
                benches_n{{n}},
                insert_many_n{{n}},
                insert_many_chunked_100_n{{n}},
                insert_many_chunked_1000_n{{n}},
                insert_incremental_n{{n}}
            );
        }
    }
}

define_harness!([2, 4, 8, 16]);
criterion_main!(benches_n2, benches_n4, benches_n8, benches_n16);
