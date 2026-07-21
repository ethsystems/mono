use crate::{
    Hash,
    HashState,
    Hasher,
};

pub struct XorState {
    hash: [u8; 32],
    pos: usize,
}

impl HashState for XorState {
    fn update(&mut self, data: &[u8]) {
        for &b in data {
            self.hash[self.pos % 32] ^= b;
            self.pos += 1;
        }
    }

    fn finalize(self) -> Hash {
        self.hash
    }
}

#[derive(Clone)]
pub struct XorHasher;

impl Hasher for XorHasher {
    type State = XorState;

    fn new_state(&self) -> Self::State {
        XorState {
            hash: [0u8; 32],
            pos: 0,
        }
    }
}

pub fn leaf(n: u32) -> Hash {
    let mut h = [0u8; 32];
    h[..4].copy_from_slice(&n.to_le_bytes());
    h
}
