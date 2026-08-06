/// Total order over chain events: block number, then log index within the block.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Position {
    /// Block number the event belongs to.
    pub block: u64,
    /// Index of the log within its block.
    pub log_index: u64,
}

impl Position {
    /// Builds a position from a block number and its log index.
    pub const fn new(block: u64, log_index: u64) -> Self {
        Self { block, log_index }
    }
}

/// A block identified by number and hash; the hash commits to the full ancestry.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BlockRef {
    /// Block number.
    pub number: u64,
    /// Block hash, committing to the full ancestry.
    pub hash: [u8; 32],
}

#[cfg(test)]
mod tests {
    use super::Position;

    #[test]
    fn position_order_is_block_then_log_index() {
        // given positions across two blocks with mixed log indices
        let mut positions = [
            Position::new(2, 0),
            Position::new(1, 5),
            Position::new(1, 1),
            Position::new(2, 3),
        ];
        // when sorted
        positions.sort();
        // then block dominates and log_index breaks ties within a block
        assert_eq!(
            positions,
            [
                Position::new(1, 1),
                Position::new(1, 5),
                Position::new(2, 0),
                Position::new(2, 3),
            ]
        );
    }
}
