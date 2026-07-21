use rotortree::{
    Blake3Hasher,
    CheckpointPolicy,
    FlushPolicy,
    RotorTree,
    RotorTreeConfig,
    TieringConfig,
};
use std::{
    env,
    fs::File,
    io::Write,
    path::PathBuf,
    time::Instant,
};

const N: usize = 4;
const MAX_DEPTH: usize = 14;
const DB_PATH: &str = ".db";

use tikv_jemallocator::Jemalloc;

#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

fn env_or<T: std::str::FromStr>(key: &str, default: T) -> T {
    env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn generate_leaves(start: u64, count: u64) -> Vec<[u8; 32]> {
    (start..start + count)
        .map(|i| {
            let mut h = [0u8; 32];
            h[..8].copy_from_slice(&i.to_le_bytes());
            h
        })
        .collect()
}

fn main() {
    let total_leaves: u64 = env_or("TOTAL_LEAVES", 100_000_000);
    let block_size: u64 = env_or("BLOCK_SIZE", 1_000_000);
    let proof_iters: usize = env_or("PROOF_ITERS", 100);
    let csv_path: String = env_or("CSV_PATH", "bulk_load.csv".to_string());

    let _ = std::fs::remove_dir_all(DB_PATH);

    let config = RotorTreeConfig {
        path: PathBuf::from(DB_PATH),
        flush_policy: FlushPolicy::Interval(std::time::Duration::from_millis(10)),
        checkpoint_policy: CheckpointPolicy::MemoryThreshold(256 * 1024 * 1024),
        tiering: TieringConfig::default(),
        verify_checkpoint: false,
    };
    let tree =
        RotorTree::<Blake3Hasher, N, MAX_DEPTH>::open(Blake3Hasher, config).unwrap();

    let mut csv = File::create(&csv_path).unwrap();
    writeln!(
        csv,
        "leaves,block_ins_per_sec,durable_ins_per_sec,proof_gen_ns,proof_verify_ns,depth"
    )
    .unwrap();

    let mut inserted: u64 = 0;

    while inserted < total_leaves {
        let this_block = block_size.min(total_leaves - inserted);
        let leaves = generate_leaves(inserted, this_block);

        let block_start = Instant::now();
        let (_root, token) = tree.insert_many(&leaves).unwrap();
        let insert_elapsed = block_start.elapsed();
        token.wait();
        let durable_elapsed = block_start.elapsed();

        inserted += this_block;

        let probe_index = inserted - this_block / 2;
        let snap = tree.snapshot();

        let mut gen_times = Vec::with_capacity(proof_iters);
        let mut verify_times = Vec::with_capacity(proof_iters);
        for _ in 0..proof_iters {
            let t = Instant::now();
            let proof = snap.generate_proof(probe_index).unwrap();
            gen_times.push(t.elapsed());
            let t = Instant::now();
            assert!(proof.verify(&Blake3Hasher).unwrap());
            verify_times.push(t.elapsed());
        }
        gen_times.sort();
        verify_times.sort();
        let proof_gen_ns = gen_times[gen_times.len() / 2].as_nanos();
        let proof_verify_ns = verify_times[verify_times.len() / 2].as_nanos();

        let block_ins = (this_block as f64 / insert_elapsed.as_secs_f64()) as u64;
        let durable_ins = (this_block as f64 / durable_elapsed.as_secs_f64()) as u64;

        writeln!(
            csv,
            "{inserted},{block_ins},{durable_ins},{proof_gen_ns},{proof_verify_ns},{}",
            tree.depth()
        )
        .unwrap();
    }

    tree.close().unwrap();
    std::fs::remove_dir_all(DB_PATH).unwrap();
}
