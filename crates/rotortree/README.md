# rotortree

A fast n-ary leanIMT implementation with persistence, for high-throughput append-only merkle trees.

<!-- ANCHOR: intro --> 
Most privacy protocols reuse general-purpose databases such as rocksdb or postgres, which may not suit high-throughput use cases; this is also why layerzero built [qmdb](https://arxiv.org/pdf/2501.05262). rotortree's design differs in that it focuses exclusively on append-only merkle trees and does not support updating leaves in place.

<!-- ANCHOR_END: intro --> 

This approach makes significant tradeoffs and is not intended for production use.

The tree design is inspired by [lean-imt](https://zkkit.org/leanimt-paper.pdf), based on work by cedoor and vivian at [PSE](https://pse.dev). This design allows for functional equivalents in zk DSLs and Solidity; the main deviation here is an n-ary leanIMT, which reduces tree depth while maintaining the same leaf count and allows on-disk storage blocks to group leaves together efficiently.

<!-- ANCHOR: design --> 

## Design decisions

- you should have a k-v database in tandem with this to ensure you don't insert the same nullifier twice. 
- you should constrain node values to the finite field you're using before insertion
- generic hasher, blake3 default
- batteries included for playing with different branching factors and max depths
- wal for persistence and recovery, with checkpointing to prevent unbounded wal growth
- [wincode](https://github.com/anza-xyz/wincode) & regular serde for compatibility with ecosystem (e.g the jolt prover needs serde)
- no_std by default, persistence requires std
- benchmarks driven and configured by divan + crabtime
- by default your tree lives in memory, but with the `storage` feature you can tier cold levels to mmap'd data files via `TieringConfig::pin_above_level`
  - with N=4, MAX_DEPTH=16, you can fit ~4.3B nullifiers in 41 GiB
  - with N=8, MAX_DEPTH=10, you can fit ~1B nullifiers in 37 GiB
  - which are quite feasible, but expensive. just use a new tree per generation and encode your nullifiers with the generation pls
  - in most cases, you just need the tree in memory without crash persistence (as long as there is a bootstrapping sync mechanism), just use the single threaded variant, its _MUCH_ better if you have a low number of insertions
  - writes go to the wal + memory, reads are always from memory or mmap. one cannot do an apples to apples comparison with other merkle tree dbs that read from disk on every query
- assumes that leaves are prehashed, partial support for rfc 6962 only for internal nodes
- few dependencies ~ 65 (active + transitive, excluding dev deps), zero deps for just the algorithm

<!-- ANCHOR_END: design --> 

<!-- ANCHOR: usage --> 

## Usage

### In-memory (no persistence) `default-features = false`

use `LeanIMT` 

```rust
use rotortree::{LeanIMT, Blake3Hasher};

// N=4 branching factor, MAX_DEPTH=20
let mut tree = LeanIMT::<Blake3Hasher, 4, 20>::new(Blake3Hasher);

// single insert
let leaf = [1u8; 32];
let root = tree.insert(leaf).unwrap();

// batch insert
let leaves: Vec<[u8; 32]> = (0..1000u32)
    .map(|i| {
        let mut h = [0u8; 32];
        h[..4].copy_from_slice(&i.to_le_bytes());
        h
    })
    .collect();
let root = tree.insert_many(&leaves).unwrap();

// proof generation & verification
let snap = tree.snapshot();
let proof = snap.generate_proof(0).unwrap();
assert!(proof.verify(&Blake3Hasher).unwrap());
```

optional feature flags for the in-memory mode:
- `concurrent`: switches to `&self` methods with internal `RwLock` (via `parking_lot`)
- `parallel`: enables rayon-parallelized `insert_many` for large batches (this works really well)
- `wincode`: adds wincode serde derives to proof types

### With WAL persistence (`storage` feature)

```rust
use rotortree::{
    Blake3Hasher, RotorTree, RotorTreeConfig,
    FlushPolicy, CheckpointPolicy, TieringConfig,
};
use std::path::PathBuf;

let config = RotorTreeConfig {
    path: PathBuf::from("/tmp/my-tree"),
    flush_policy: FlushPolicy::default(), // fsync every 10ms
    checkpoint_policy: CheckpointPolicy::default(), // manual
    tiering: TieringConfig::default(), // all in memory
    verify_checkpoint: true, // recompute root on recovery
};

// opens existing WAL or creates a new one
let tree = RotorTree::<Blake3Hasher, 4, 20>::open(Blake3Hasher, config).unwrap();

// insert: returns root + a durability token
let (root, token) = tree.insert([42u8; 32]).unwrap();
// token.wait() blocks until the entry is fsynced

// or insert + wait for fsync in one call
let root = tree.insert_durable([43u8; 32]).unwrap();

// batch insert
let leaves = vec![[1u8; 32]; 500];
let (root, token) = tree.insert_many(&leaves).unwrap();

// lock-free snapshot for proof generation (same as in-memory)
let snap = tree.snapshot();
let proof = snap.generate_proof(0).unwrap();
assert!(proof.verify(&Blake3Hasher).unwrap());

// explicit flush & close
tree.flush().unwrap();
tree.close().unwrap();
// (also flushes + releases file lock on drop)
```

`FlushPolicy` options:
- `Interval(Duration)`: background thread fsyncs periodically
- `Manual`: caller controls flushing via `tree.flush()` (works well if you're following a blockchain as the canonical source of state transitions)

`CheckpointPolicy` options (materializes wal state to data files, allowing wal truncation):
- `Manual`: caller calls `checkpoint()` explicitly
- `EveryNEntries(n)`: auto-checkpoint after every `n` wal entries
- `MemoryThreshold(bytes)`: auto-checkpoint when uncheckpointed memory exceeds threshold
- `OnClose`: checkpoint only on graceful close

`TieringConfig` controls which levels stay in memory vs get mmap'd after checkpoint:
- `pin_above_level`: levels below this value have their committed chunks mmap'd from data files after checkpoint. set to `usize::MAX` to mmap everything (default), `0` to keep everything in memory

### Provability

#### Jolt

```math
\text{execution\_cycles} \approx 2948.4 + 1832.9 \cdot N \cdot D + (899.5 + 2993.8N - 103.4N^2) \cdot d
```

```math
\text{where } N \in \{2, 4, 8, 16\}, \quad D = \text{max\_depth}, \quad d = \lceil \log_N(\text{num\_leaves}) \rceil
```

#### Noir (UltraHonk via bb)

```math
\text{expression\_width} = 68 + (217N - 100) \cdot d
```
```math
\text{circuit\_size} = 2962 + 30N + (551N^2 + 2629N - 2448) \cdot d
```

```math
\text{where } N \in \{2, 4, 8, 16\}, \quad d = \lceil \log_N(\text{num\_leaves}) \rceil
```

### Tuning

of course, you'll have to benchmark your unique workload to see if this database suits your use case and requirements. here are some constants and env vars you can play around with to alter behaviour:

compile-time constants (change in source and recompile):
- `CHUNK_SIZE` (default `128`): hashes per chunk for structural sharing. 128 × 32 bytes = 4 KB per chunk. affects snapshot copy cost and arc granularity
- `CHUNKS_PER_SEGMENT` (default `256`): chunks per immutable segment. controls how many chunks are frozen into a single arc slab 
- `PAR_CHUNK_SIZE` (default `64`, `parallel` feature): parents per rayon work unit. smaller values = more parallelism but more scheduling overhead
- `MAX_FRAME_PAYLOAD` (default `128 MB`): maximum wal/checkpoint frame payload size. change this if you insert_many more than 128 MB worth of leaves

runtime env vars:
- `ROTORTREE_PARALLEL_THRESHOLD` (default `1024`, `parallel` feature): minimum parent count before rayon parallelism kicks in

<!-- ANCHOR_END: usage --> 

## Development

### Prerequisites

- [cargo-hack](https://github.com/taiki-e/cargo-hack?tab=readme-ov-file#installation): to test all combinations of feature flags
- [cargo-nextest](https://nexte.st/): rust test runner

### Check

```
cargo hack check --feature-powerset
```

### Clippy

```
cargo hack clippy --feature-powerset
```

### Format

```
cargo +nightly fmt
```

## Testing

```
cargo hack nextest run --feature-powerset
```

## Benchmarks

```
cargo bench -- --list
```

there are some feature flagged benchmarks, refer to the [Cargo.toml entry](Cargo.toml) for more details

<!-- ANCHOR: devnote --> 

the pure in-memory benchmark (tree_bench_parallel) exhibits lesser variance in the benchmarks, and achieves peak throughput upto ~190M leaves/sec; single threaded by far has the best performance characteristic in terms of variance though, useful to keep in mind if that is a constraint; trading off performance for predictability under load.

<!-- ANCHOR_END: devnote --> 

There are more realistic benchmarks that simulate performance under load, i.e concurrent reads / proof generation + insertions

## Future work

1. optimize `ceil_log_n` by precomputing the table
2. run benchmarks in an isolated environment for better estimations
