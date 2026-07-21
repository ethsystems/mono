//! SIMD-batched parent hashing equivalence for the Blake3 hasher.
//!
//! The SIMD batch path in `_insert_many` (via `Blake3Hasher::hash_many_into`)
//! must produce a root byte-identical to feeding the same leaves through the
//! scalar one-at-a-time `insert`. The sequential path is the ground truth.

#![cfg(feature = "blake3")]
#![cfg_attr(feature = "concurrent", allow(unused_mut))]

use proptest::prelude::*;
use rotortree::{
    Blake3Hasher,
    Hash,
    LeanIMT,
};

fn leaf_at(seed: u64, i: usize) -> Hash {
    let mut h = [0u8; 32];
    let v = seed.wrapping_add(i as u64).to_le_bytes();
    h[..8].copy_from_slice(&v);
    h
}

macro_rules! batch_eq_seq {
    ($name:ident, $arity:literal) => {
        proptest! {
            #![proptest_config(ProptestConfig::with_cases(48))]
            #[test]
            fn $name(
                // counts straddling the 128-entry chunk boundary and the
                // 256*128 = 32768 chunk-of-chunks boundary, plus the lift and
                // single-element edges.
                n in prop::sample::select(vec![
                    1usize, 2, 127, 128, 129, 255, 256, 257, 512,
                    32767, 32768, 32769,
                ]),
                seed in any::<u64>(),
            ) {
                let leaves: Vec<Hash> = (0..n).map(|i| leaf_at(seed, i)).collect();

                let mut seq = LeanIMT::<Blake3Hasher, $arity, 32>::new(Blake3Hasher);
                for &l in &leaves {
                    seq.insert(l).unwrap();
                }

                let mut batch = LeanIMT::<Blake3Hasher, $arity, 32>::new(Blake3Hasher);
                batch.insert_many(&leaves).unwrap();

                prop_assert_eq!(seq.root(), batch.root(), "n={}", n);
                prop_assert_eq!(seq.size(), batch.size());
            }
        }
    };
}

batch_eq_seq!(insert_many_matches_sequential_blake3_n2, 2);
batch_eq_seq!(insert_many_matches_sequential_blake3_n4, 4);
batch_eq_seq!(insert_many_matches_sequential_blake3_n8, 8);
batch_eq_seq!(insert_many_matches_sequential_blake3_n16, 16);

/// Incremental: some leaves via single insert, the rest via batched
/// `insert_many`, must equal the all-sequential tree. Exercises a non-zero
/// `start_parent` and partial first group on the batched path.
macro_rules! incremental_eq {
    ($name:ident, $arity:literal) => {
        proptest! {
            #![proptest_config(ProptestConfig::with_cases(48))]
            #[test]
            fn $name(
                pre in prop::sample::select(vec![1usize, 3, 7, 128, 200]),
                post in prop::sample::select(vec![1usize, 5, 129, 300, 1000]),
                seed in any::<u64>(),
            ) {
                let total = pre + post;
                let leaves: Vec<Hash> = (0..total).map(|i| leaf_at(seed, i)).collect();

                let mut seq = LeanIMT::<Blake3Hasher, $arity, 32>::new(Blake3Hasher);
                for &l in &leaves {
                    seq.insert(l).unwrap();
                }

                let mut mixed = LeanIMT::<Blake3Hasher, $arity, 32>::new(Blake3Hasher);
                for &l in &leaves[..pre] {
                    mixed.insert(l).unwrap();
                }
                mixed.insert_many(&leaves[pre..]).unwrap();

                prop_assert_eq!(seq.root(), mixed.root(), "pre={} post={}", pre, post);
            }
        }
    };
}

incremental_eq!(incremental_matches_sequential_blake3_n2, 2);
incremental_eq!(incremental_matches_sequential_blake3_n4, 4);
incremental_eq!(incremental_matches_sequential_blake3_n8, 8);
incremental_eq!(incremental_matches_sequential_blake3_n16, 16);
