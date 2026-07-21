use rotortree::Hash;

pub fn generate_leaves(count: usize) -> Vec<Hash> {
    (0..count)
        .map(|i| {
            let mut leaf = [0u8; 32];
            leaf[..8].copy_from_slice(&(i as u64).to_le_bytes());
            leaf
        })
        .collect()
}
