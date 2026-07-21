use std::{
    path::PathBuf,
    time::Duration,
};

use super::checkpoint::{
    CheckpointPolicy,
    TieringConfig,
};

/// Configuration for opening a `RotorTree`
pub struct RotorTreeConfig {
    /// Directory path where the WAL and data files are stored
    pub path: PathBuf,
    /// Controls when WAL entries are fsynced to disk
    pub flush_policy: FlushPolicy,
    /// Controls when checkpoints are triggered
    pub checkpoint_policy: CheckpointPolicy,
    /// Controls which tree levels are kept in memory vs mmap'd
    pub tiering: TieringConfig,
    /// Recompute Merkle root on recovery to detect corruption beyond CRC
    pub verify_checkpoint: bool,
}

/// Controls when buffered WAL entries are fsynced to disk
pub enum FlushPolicy {
    /// Fsync on a periodic interval (default: 10ms)
    Interval(Duration),
    /// Caller controls flushing via `flush()`
    Manual,
}

impl Default for FlushPolicy {
    fn default() -> Self {
        Self::Interval(Duration::from_millis(10))
    }
}
