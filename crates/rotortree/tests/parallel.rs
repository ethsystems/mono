#![cfg(feature = "parallel")]
#![cfg_attr(feature = "concurrent", allow(unused_mut))]

use rotortree::{
    Hash,
    LeanIMT,
    test_util::XorHasher,
};

fn make_leaves(count: usize) -> Vec<Hash> {
    (0..count)
        .map(|i| {
            let mut h = [0u8; 32];
            let bytes = (i as u64).to_le_bytes();
            h[..8].copy_from_slice(&bytes);
            h
        })
        .collect()
}

#[test]
fn parallel_insert_many_matches_sequential_binary() {
    let leaves = make_leaves(1000);

    let mut seq = LeanIMT::<XorHasher, 2, 32>::new(XorHasher);
    for &l in &leaves {
        seq.insert(l).unwrap();
    }

    let mut batch = LeanIMT::<XorHasher, 2, 32>::new(XorHasher);
    batch.insert_many(&leaves).unwrap();

    assert_eq!(seq.root(), batch.root());
    assert_eq!(seq.size(), batch.size());
}

#[test]
fn parallel_insert_many_matches_sequential_ternary() {
    let leaves = make_leaves(1000);

    let mut seq = LeanIMT::<XorHasher, 3, 32>::new(XorHasher);
    for &l in &leaves {
        seq.insert(l).unwrap();
    }

    let mut batch = LeanIMT::<XorHasher, 3, 32>::new(XorHasher);
    batch.insert_many(&leaves).unwrap();

    assert_eq!(seq.root(), batch.root());
}

#[test]
fn parallel_insert_many_matches_sequential_quaternary() {
    let leaves = make_leaves(1000);

    let mut seq = LeanIMT::<XorHasher, 4, 32>::new(XorHasher);
    for &l in &leaves {
        seq.insert(l).unwrap();
    }

    let mut batch = LeanIMT::<XorHasher, 4, 32>::new(XorHasher);
    batch.insert_many(&leaves).unwrap();

    assert_eq!(seq.root(), batch.root());
}

#[test]
fn parallel_large_batch_proofs() {
    let leaves = make_leaves(2000);

    let mut tree = LeanIMT::<XorHasher, 2, 32>::new(XorHasher);
    tree.insert_many(&leaves).unwrap();

    let snap = tree.snapshot();
    for i in 0..2000u64 {
        let proof = snap.generate_proof(i).unwrap();
        assert!(proof.verify(&XorHasher).unwrap());
    }
}

#[test]
fn parallel_insert_many_incremental() {
    let leaves = make_leaves(1000);

    let mut seq = LeanIMT::<XorHasher, 2, 32>::new(XorHasher);
    for &l in &leaves {
        seq.insert(l).unwrap();
    }

    let mut mixed = LeanIMT::<XorHasher, 2, 32>::new(XorHasher);
    for &l in &leaves[..100] {
        mixed.insert(l).unwrap();
    }
    mixed.insert_many(&leaves[100..]).unwrap();

    assert_eq!(seq.root(), mixed.root());
}
