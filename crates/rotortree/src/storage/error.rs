use std::{
    fmt,
    sync::Arc,
};

use crate::TreeError;

#[derive(Debug)]
pub(crate) enum BackgroundError {
    FlushFailed(Arc<std::io::Error>),
    CheckpointFailed(String),
}

/// Errors from WAL and storage operations
#[derive(Debug)]
pub enum StorageError {
    /// An I/O error occurred
    Io(std::io::Error),
    /// CRC32C checksum mismatch at a given file offset
    CrcMismatch {
        offset: u64,
        expected: u32,
        actual: u32,
    },
    /// WAL file is corrupted at the given offset (not a tail truncation)
    WalCorrupted { offset: u64 },
    /// WAL file header has different N or MAX_DEPTH than expected
    ConfigMismatch {
        expected_n: u32,
        actual_n: u32,
        expected_max_depth: u32,
        actual_max_depth: u32,
    },
    /// Another process holds an exclusive lock on the WAL file
    FileLocked,
    /// The tree has been closed; no further operations are allowed
    Closed,
    /// A tree operation failed during WAL recovery
    Tree(TreeError),
    /// The background flush thread encountered an I/O error
    FlushFailed(Arc<std::io::Error>),
    /// The background checkpoint thread encountered an error
    CheckpointFailed(String),
    /// Math error
    MathError,
    /// Data corruption detected (e.g., root recomputation mismatch)
    DataCorruption { detail: String },
    /// Frame deserialization failed despite valid CRC (schema/version mismatch)
    SerdeFailed { path: String },
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "storage I/O error: {e}"),
            Self::CrcMismatch {
                offset,
                expected,
                actual,
            } => write!(
                f,
                "CRC mismatch at offset {offset}: expected {expected:#010x}, \
                 got {actual:#010x}"
            ),
            Self::WalCorrupted { offset } => {
                write!(f, "WAL corrupted at offset {offset}")
            }
            Self::ConfigMismatch {
                expected_n,
                actual_n,
                expected_max_depth,
                actual_max_depth,
            } => write!(
                f,
                "WAL config mismatch: expected N={expected_n}, \
                 MAX_DEPTH={expected_max_depth}; found N={actual_n}, \
                 MAX_DEPTH={actual_max_depth}"
            ),
            Self::FileLocked => {
                write!(f, "WAL file is locked by another process")
            }
            Self::Closed => write!(f, "tree has been closed"),
            Self::Tree(e) => write!(f, "tree error during recovery: {e}"),
            Self::FlushFailed(e) => {
                write!(f, "background flush failed: {e}")
            }
            Self::CheckpointFailed(detail) => {
                write!(f, "background checkpoint failed: {detail}")
            }
            Self::MathError => {
                write!(f, "math error")
            }
            Self::DataCorruption { detail } => {
                write!(f, "data corruption: {detail}")
            }
            Self::SerdeFailed { path } => {
                write!(f, "frame deserialization failed for {path}")
            }
        }
    }
}

impl std::error::Error for StorageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Tree(e) => Some(e),
            Self::FlushFailed(e) => Some(e.as_ref()),
            _ => None,
        }
    }
}

impl From<std::io::Error> for StorageError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// Unified error type for `RotorTree` operations
#[derive(Debug)]
pub enum RotorTreeError {
    /// A tree-level error (depth exceeded, capacity, etc.)
    Tree(TreeError),
    /// A storage-level error (I/O, corruption, etc.)
    Storage(StorageError),
}

impl fmt::Display for RotorTreeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tree(e) => write!(f, "{e}"),
            Self::Storage(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for RotorTreeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Tree(e) => Some(e),
            Self::Storage(e) => Some(e),
        }
    }
}

impl From<TreeError> for RotorTreeError {
    fn from(e: TreeError) -> Self {
        Self::Tree(e)
    }
}

impl From<StorageError> for RotorTreeError {
    fn from(e: StorageError) -> Self {
        Self::Storage(e)
    }
}

impl From<std::io::Error> for RotorTreeError {
    fn from(e: std::io::Error) -> Self {
        Self::Storage(StorageError::Io(e))
    }
}
