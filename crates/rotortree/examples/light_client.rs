#![cfg_attr(feature = "concurrent", allow(unused_mut))]

//! Full-node / light-client consistency proof demo.
//!
//! Two threads communicate over `mpsc` channels:
//!
//! - **Full node**: persists batches to a `RotorTree` (WAL-backed) and sends
//!   consistency proofs after each batch.
//! - **Light client**: bootstraps once with the initial leaves, then drops the
//!   tree. Subsequent batches are verified using *only* the consistency proof:
//!   no leaves are transferred, and the tracked inclusion proof is updated
//!   purely from consistency data.
//!
//! ```sh
//! cargo run --example light_client --features storage,blake3 --release
//! ```

use rotortree::{
    Blake3Hasher,
    CheckpointPolicy,
    ConsistencyProof,
    FlushPolicy,
    Hash,
    LeanIMT,
    NaryProof,
    RotorTree,
    RotorTreeConfig,
    TieringConfig,
};
use std::{
    mem,
    path::PathBuf,
    sync::mpsc,
    thread,
    time::Instant,
};

const N: usize = 4;
const MAX_DEPTH: usize = 14;
const BATCH_SIZE: u64 = 10_000;
const NUM_BATCHES: usize = 5;
const TRACKED_LEAF: u64 = 812;
const DB_PATH: &str = ".db";

enum NodeMessage {
    Bootstrap {
        leaves: Vec<Hash>,
    },
    Update {
        consistency_proof: ConsistencyProof<N, MAX_DEPTH>,
    },
    Shutdown,
}

enum ClientMessage {
    InclusionProof(NaryProof<N, MAX_DEPTH>),
}

fn generate_leaves(start: u64, count: u64) -> Vec<Hash> {
    (start..start + count)
        .map(|i| *blake3::hash(&i.to_le_bytes()).as_bytes())
        .collect()
}

fn full_node(tx: mpsc::Sender<NodeMessage>, rx: mpsc::Receiver<ClientMessage>) {
    let hasher = Blake3Hasher;
    let th = Blake3Hasher;
    let _ = std::fs::remove_dir_all(DB_PATH);
    let config = RotorTreeConfig {
        path: PathBuf::from(DB_PATH),
        flush_policy: FlushPolicy::Manual,
        checkpoint_policy: CheckpointPolicy::OnClose,
        tiering: TieringConfig::default(),
        verify_checkpoint: false,
    };
    let tree = RotorTree::<Blake3Hasher, N, MAX_DEPTH>::open(hasher, config)
        .expect("failed to open tree");

    let mut inserted: u64 = 0;

    for batch in 0..NUM_BATCHES {
        let leaves = generate_leaves(inserted, BATCH_SIZE);

        if batch == 0 {
            let (_root, _token) = tree.insert_many(&leaves).expect("insert_many failed");
            inserted += BATCH_SIZE;

            let leaf_bytes = leaves.len() * mem::size_of::<Hash>();
            println!("  [full node] batch 0: sending {leaf_bytes} bytes (raw leaves)");
            tx.send(NodeMessage::Bootstrap { leaves }).unwrap();

            let ClientMessage::InclusionProof(proof) = rx.recv().unwrap();
            let snap = tree.snapshot();
            assert!(proof.verify(&th).unwrap());
            assert_eq!(proof.root, snap.root().unwrap());
            println!("  [full node] batch 0: client proof verified");
        } else {
            let old_snap = tree.snapshot();
            let old_size = old_snap.size();
            let old_root = old_snap.root().unwrap();

            let (_root, _token) = tree.insert_many(&leaves).expect("insert_many failed");
            inserted += BATCH_SIZE;

            let new_snap = tree.snapshot();
            let consistency_proof = new_snap
                .generate_consistency_proof(old_size, old_root)
                .expect("generate_consistency_proof failed");

            let proof_bytes = mem::size_of_val(&consistency_proof);
            let leaf_bytes = BATCH_SIZE as usize * mem::size_of::<Hash>();
            println!(
                "  [full node] batch {batch}: sending {proof_bytes} bytes \
                 (consistency proof, vs {leaf_bytes} bytes for raw leaves)"
            );
            tx.send(NodeMessage::Update { consistency_proof }).unwrap();

            let ClientMessage::InclusionProof(proof) = rx.recv().unwrap();
            assert!(proof.verify(&th).unwrap());
            assert_eq!(proof.root, new_snap.root().unwrap());
            println!("  [full node] batch {batch}: client proof verified");
        }
    }

    tx.send(NodeMessage::Shutdown).unwrap();
    tree.close().expect("close failed");
    let _ = std::fs::remove_dir_all(DB_PATH);
}

fn light_client(tx: mpsc::Sender<ClientMessage>, rx: mpsc::Receiver<NodeMessage>) {
    let hasher = Blake3Hasher;
    let th = Blake3Hasher;
    let mut tracked_proof: Option<NaryProof<N, MAX_DEPTH>> = None;
    let mut current_root: Option<Hash> = None;

    loop {
        match rx.recv().unwrap() {
            NodeMessage::Bootstrap { leaves } => {
                let mut tree = LeanIMT::<Blake3Hasher, N, MAX_DEPTH>::new(hasher);
                tree.insert_many(&leaves).expect("insert_many failed");
                let snap = tree.snapshot();
                let proof = snap
                    .generate_proof(TRACKED_LEAF)
                    .expect("generate_proof failed");
                assert!(proof.verify(&th).unwrap());

                current_root = snap.root();
                println!(
                    "  [light client] bootstrap: {} leaves, proof OK",
                    leaves.len()
                );
                let send_proof = snap
                    .generate_proof(TRACKED_LEAF)
                    .expect("generate_proof failed");
                tracked_proof = Some(proof);
                tx.send(ClientMessage::InclusionProof(send_proof)).unwrap();
            }

            NodeMessage::Update { consistency_proof } => {
                assert!(
                    consistency_proof
                        .verify_transition(&th, current_root.unwrap())
                        .unwrap()
                );

                let old_proof = tracked_proof.as_ref().expect("tracked proof must exist");
                let updated_proof = consistency_proof
                    .update_inclusion_proof(old_proof, &th)
                    .expect("update_inclusion_proof failed");

                assert!(updated_proof.verify(&th).unwrap());
                assert_eq!(updated_proof.root, consistency_proof.new_root);

                current_root = Some(consistency_proof.new_root);
                println!(
                    "  [light client] update: consistency OK, proof updated \
                     (old_size={}, new_size={})",
                    consistency_proof.old_size, consistency_proof.new_size,
                );
                let send_proof = consistency_proof
                    .update_inclusion_proof(old_proof, &th)
                    .expect("update_inclusion_proof failed");
                tracked_proof = Some(updated_proof);
                tx.send(ClientMessage::InclusionProof(send_proof)).unwrap();
            }

            NodeMessage::Shutdown => {
                println!("  [light client] shutdown");
                break;
            }
        }
    }
}

fn main() {
    println!(
        "light_client: N={N}, MAX_DEPTH={MAX_DEPTH}, \
         {NUM_BATCHES} batches x {BATCH_SIZE} leaves, tracked leaf #{TRACKED_LEAF}"
    );
    let start = Instant::now();

    let (node_tx, client_rx) = mpsc::channel();
    let (client_tx, node_rx) = mpsc::channel();

    let node_handle = thread::spawn(move || full_node(node_tx, node_rx));
    let client_handle = thread::spawn(move || light_client(client_tx, client_rx));

    node_handle.join().expect("full node panicked");
    client_handle.join().expect("light client panicked");

    println!("done in {:.2?}", start.elapsed());
}
