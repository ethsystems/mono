use core::fmt;

/// Errors produced by tree operations.
#[derive(Debug, PartialEq, Eq)]
pub enum TreeError {
    /// The tree would exceed its compile-time maximum depth.
    MaxDepthExceeded { max_depth: usize },
    /// The requested leaf index is outside `[0, size)`.
    IndexOutOfRange { index: u64, size: u64 },
    /// `insert_many` was called with an empty slice.
    EmptyBatch,
    /// The tree's `u64` size cannot be represented as `usize`
    CapacityExceeded,
    /// Math error
    MathError,
    /// Consistency proof depth mismatch
    InvalidProofDepth { expected: usize, actual: usize },
    /// The proof update is a no-op (old_size == new_size)
    NoUpdateNeeded,
}

impl fmt::Display for TreeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MaxDepthExceeded { max_depth } => {
                write!(
                    f,
                    "tree depth would exceed maximum of \
                     {max_depth}"
                )
            }
            Self::IndexOutOfRange { index, size } => {
                write!(
                    f,
                    "leaf index {index} out of range for tree \
                     of size {size}"
                )
            }
            Self::EmptyBatch => {
                write!(f, "insert_many called with empty batch")
            }
            Self::CapacityExceeded => {
                write!(
                    f,
                    "tree capacity exceeded (u64 to usize \
                     overflow)"
                )
            }
            Self::MathError => {
                write!(f, "math error")
            }
            Self::InvalidProofDepth { expected, actual } => {
                write!(
                    f,
                    "consistency proof depth {actual} does not match \
                     expected {expected}"
                )
            }
            Self::NoUpdateNeeded => {
                write!(f, "proof update is a no-op (same size)")
            }
        }
    }
}

impl core::error::Error for TreeError {}
