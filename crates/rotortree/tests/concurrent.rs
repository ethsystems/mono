#![cfg(feature = "concurrent")]

use std::{
    sync::Arc,
    thread,
};

use rotortree::{
    Hash,
    LeanIMT,
    test_util::{
        XorHasher,
        leaf,
    },
};

#[test]
fn concurrent_insert_single_thread() {
    let tree = LeanIMT::<XorHasher, 2, 32>::new(XorHasher);
    for i in 1..=10u32 {
        tree.insert(leaf(i)).unwrap();
    }
    assert_eq!(tree.size(), 10);

    let snap = tree.snapshot();
    for i in 0..10u64 {
        let proof = snap.generate_proof(i).unwrap();
        assert!(proof.verify(&XorHasher).unwrap());
    }
}

#[test]
fn concurrent_multi_thread_insert() {
    let tree = Arc::new(LeanIMT::<XorHasher, 2, 32>::new(XorHasher));
    let num_threads = 4;
    let leaves_per_thread = 50;

    let handles: Vec<_> = (0..num_threads)
        .map(|t| {
            let tree = Arc::clone(&tree);
            thread::spawn(move || {
                let base = (t * leaves_per_thread) as u8;
                for i in 0..leaves_per_thread as u8 {
                    let mut l = [0u8; 32];
                    l[0] = base.wrapping_add(i);
                    l[1] = t as u8;
                    tree.insert(l).unwrap();
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    let total = num_threads * leaves_per_thread;
    assert_eq!(tree.size(), total as u64);

    let snap = tree.snapshot();
    for i in 0..total as u64 {
        let proof = snap.generate_proof(i).unwrap();
        assert!(proof.verify(&XorHasher).unwrap());
    }
}

#[test]
fn concurrent_reader_writer() {
    let tree = Arc::new(LeanIMT::<XorHasher, 2, 32>::new(XorHasher));
    let num_inserts = 200u64;
    let num_readers = 3;

    let writer_tree = Arc::clone(&tree);
    let writer = thread::spawn(move || {
        for i in 0..num_inserts {
            let mut l = [0u8; 32];
            l[0] = i as u8;
            l[1] = (i >> 8) as u8;
            writer_tree.insert(l).unwrap();
        }
    });

    let readers: Vec<_> = (0..num_readers)
        .map(|_| {
            let tree = Arc::clone(&tree);
            thread::spawn(move || {
                for _ in 0..100 {
                    let snap = tree.snapshot();
                    let size = snap.size();
                    if size == 0 {
                        continue;
                    }
                    for i in 0..size {
                        let proof = snap.generate_proof(i).unwrap();
                        assert!(proof.verify(&XorHasher).unwrap());
                    }
                }
            })
        })
        .collect();

    writer.join().unwrap();
    for r in readers {
        r.join().unwrap();
    }

    assert_eq!(tree.size(), num_inserts);
}

#[test]
fn snapshot_isolation() {
    let tree = LeanIMT::<XorHasher, 2, 32>::new(XorHasher);
    for i in 1..=5u32 {
        tree.insert(leaf(i)).unwrap();
    }

    let snap = tree.snapshot();
    let snap_root = snap.root();
    let snap_size = snap.size();
    let snap_depth = snap.depth();

    // Insert more after taking the snapshot.
    for i in 6..=10u32 {
        tree.insert(leaf(i)).unwrap();
    }

    // The snapshot must be unchanged.
    assert_eq!(snap.root(), snap_root);
    assert_eq!(snap.size(), snap_size);
    assert_eq!(snap.depth(), snap_depth);

    // But the tree itself has advanced.
    assert_eq!(tree.size(), 10);
    assert_ne!(tree.root(), snap_root);
}

#[test]
fn concurrent_insert_many() {
    let tree = Arc::new(LeanIMT::<XorHasher, 2, 32>::new(XorHasher));
    let num_threads = 4;
    let batch_size = 25;

    let handles: Vec<_> = (0..num_threads)
        .map(|t| {
            let tree = Arc::clone(&tree);
            thread::spawn(move || {
                let batch: Vec<Hash> = (0..batch_size)
                    .map(|i| {
                        let mut l = [0u8; 32];
                        l[0] = i as u8;
                        l[1] = t as u8;
                        l
                    })
                    .collect();
                tree.insert_many(&batch).unwrap();
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    let total = (num_threads * batch_size) as u64;
    assert_eq!(tree.size(), total);

    let snap = tree.snapshot();
    for i in 0..total {
        let proof = snap.generate_proof(i).unwrap();
        assert!(proof.verify(&XorHasher).unwrap());
    }
}
