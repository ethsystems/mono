#![cfg_attr(feature = "concurrent", allow(unused_mut))]

use proptest::prelude::*;
use rotortree::{
    Hash,
    LeanIMT,
    test_util::XorHasher,
};

fn leaves_strategy(max: usize) -> impl Strategy<Value = Vec<Hash>> {
    prop::collection::vec(prop::array::uniform32(any::<u8>()), 1..=max)
}

#[crabtime::function]
fn gen_root_deterministic(n_values: Vec<usize>) {
    for n in n_values {
        crabtime::output! {
            proptest! {
                #[test]
                fn root_deterministic_n{{n}}(
                    leaves in leaves_strategy(100)
                ) {
                    let mut t1 =
                        LeanIMT::<XorHasher, {{n}}, 32>::new(XorHasher);
                    let mut t2 =
                        LeanIMT::<XorHasher, {{n}}, 32>::new(XorHasher);
                    for &leaf in &leaves {
                        t1.insert(leaf).unwrap();
                        t2.insert(leaf).unwrap();
                    }
                    prop_assert_eq!(t1.root(), t2.root());
                }
            }
        }
    }
}

gen_root_deterministic!([2, 3, 4]);

#[crabtime::function]
fn gen_proof_round_trip(n_values: Vec<usize>) {
    for n in n_values {
        crabtime::output! {
            proptest! {
                #[test]
                fn proof_round_trip_n{{n}}(
                    leaves in leaves_strategy(100)
                ) {
                    let mut tree =
                        LeanIMT::<XorHasher, {{n}}, 32>::new(XorHasher);
                    for &leaf in &leaves {
                        tree.insert(leaf).unwrap();
                    }
                    let snap = tree.snapshot();
                    for i in 0..leaves.len() as u64 {
                        let proof = snap.generate_proof(i).unwrap();
                        prop_assert!(proof.verify(&XorHasher).unwrap());
                    }
                }
            }
        }
    }
}

gen_proof_round_trip!([2, 3, 4]);

#[crabtime::function]
fn gen_insert_many_equivalence(n_values: Vec<usize>) {
    for n in n_values {
        crabtime::output! {
            proptest! {
                #[test]
                fn insert_many_equivalence_n{{n}}(
                    leaves in leaves_strategy(200)
                ) {
                    let mut seq =
                        LeanIMT::<XorHasher, {{n}}, 32>::new(XorHasher);
                    for &leaf in &leaves {
                        seq.insert(leaf).unwrap();
                    }

                    let mut batch =
                        LeanIMT::<XorHasher, {{n}}, 32>::new(XorHasher);
                    batch.insert_many(&leaves).unwrap();

                    prop_assert_eq!(seq.root(), batch.root());
                }
            }
        }
    }
}

gen_insert_many_equivalence!([2, 3, 4]);

#[crabtime::function]
fn gen_consistency_proof(n_values: Vec<usize>) {
    for n in n_values {
        crabtime::output! {
            proptest! {
                #[test]
                fn consistency_proof_n{{n}}(
                    leaves in leaves_strategy(100)
                ) {
                    let mut tree =
                        LeanIMT::<XorHasher, {{n}}, 32>::new(XorHasher);
                    let mut roots = Vec::new();
                    let mut sizes = Vec::new();
                    for &leaf in &leaves {
                        tree.insert(leaf).unwrap();
                        let snap = tree.snapshot();
                        roots.push(snap.root().unwrap());
                        sizes.push(snap.size());
                    }
                    let final_snap = tree.snapshot();
                    for i in 0..leaves.len() {
                        let proof = final_snap
                            .generate_consistency_proof(sizes[i], roots[i])
                            .unwrap();
                        prop_assert!(
                            proof.verify(&XorHasher).unwrap(),
                            "n{{n}} consistency failed for size {} -> {}",
                            sizes[i],
                            final_snap.size()
                        );
                    }
                }
            }
        }
    }
}

gen_consistency_proof!([2, 3, 4]);

#[crabtime::function]
fn gen_consistency_proof_update(n_values: Vec<usize>) {
    for n in n_values {
        crabtime::output! {
            proptest! {
                #[test]
                fn consistency_proof_update_n{{n}}(
                    leaves in leaves_strategy(50)
                ) {
                    let mut tree =
                        LeanIMT::<XorHasher, {{n}}, 32>::new(XorHasher);
                    let mut snaps = Vec::new();
                    for &leaf in &leaves {
                        tree.insert(leaf).unwrap();
                        let snap = tree.snapshot();
                        snaps.push((snap.size(), snap.root().unwrap(), snap));
                    }
                    let last = snaps.len() - 1;
                    for i in 0..snaps.len() {
                        let cp = snaps[last].2
                            .generate_consistency_proof(snaps[i].0, snaps[i].1)
                            .unwrap();
                        let old_ip = snaps[i].2.generate_proof(0).unwrap();
                        if i == last {
                            let err = cp.update_inclusion_proof(&old_ip, &XorHasher).unwrap_err();
                            prop_assert_eq!(err, rotortree::TreeError::NoUpdateNeeded);
                            continue;
                        }
                        let updated = cp.update_inclusion_proof(&old_ip, &XorHasher).unwrap();
                        let fresh = snaps[last].2.generate_proof(0).unwrap();
                        prop_assert_eq!(updated, fresh,
                            "n{{n}} update mismatch: size {} -> {}", snaps[i].0, snaps[last].0);
                    }
                }
            }
        }
    }
}

gen_consistency_proof_update!([2, 3, 4]);

proptest! {
    #[test]
    fn insert_many_incremental_binary(
        first in leaves_strategy(50),
        second in leaves_strategy(50),
    ) {
        let mut seq =
            LeanIMT::<XorHasher, 2, 32>::new(XorHasher);
        for &leaf in first.iter().chain(second.iter()) {
            seq.insert(leaf).unwrap();
        }

        let mut mixed =
            LeanIMT::<XorHasher, 2, 32>::new(XorHasher);
        for &leaf in &first {
            mixed.insert(leaf).unwrap();
        }
        mixed.insert_many(&second).unwrap();

        prop_assert_eq!(seq.root(), mixed.root());
    }
}

#[cfg(feature = "blake3")]
proptest! {
    #[test]
    fn proof_round_trip_blake3_n2(
        leaves in leaves_strategy(100)
    ) {
        let hasher = rotortree::Blake3Hasher;
        let th = rotortree::Blake3Hasher;
        let mut tree = LeanIMT::<rotortree::Blake3Hasher, 2, 32>::new(hasher);
        for &leaf in &leaves {
            tree.insert(leaf).unwrap();
        }
        let snap = tree.snapshot();
        for i in 0..leaves.len() as u64 {
            let proof = snap.generate_proof(i).unwrap();
            prop_assert!(proof.verify(&th).unwrap());
        }
    }
}
