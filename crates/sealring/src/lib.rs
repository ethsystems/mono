#![cfg_attr(feature = "docs", doc = include_utils::include_md!("README.md:intro"))]
#![cfg_attr(feature = "docs", doc = include_utils::include_md!("README.md:design"))]
#![cfg_attr(feature = "docs", doc = include_utils::include_md!("README.md:usage"))]
#![cfg_attr(not(test), deny(clippy::cast_possible_truncation))]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(unused_crate_dependencies)]
#![deny(warnings)]
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
#[cfg_attr(docsrs, doc(cfg(not(feature = "std"))))]
extern crate alloc;

// dev-only crates linked into the test harness build.
#[cfg(test)]
use {
    crabtime as _,
    criterion as _,
    proptest as _,
    rand_chacha as _,
};

mod domain;
mod envelope;
mod error;
mod kem;
mod recipient;
mod scan;
mod seal;

pub mod adapters;

#[cfg(any(test, feature = "test-helpers"))]
#[cfg_attr(docsrs, doc(cfg(feature = "test-helpers")))]
pub mod test_util;

pub use domain::Domain;
pub use envelope::{
    MAX_CT_LEN,
    SealedNote,
};
pub use error::{
    Malformed,
    OpenError,
    ParseError,
    SealError,
};
pub use kem::Kem;
pub use recipient::Recipient;
pub use scan::{
    SCAN_CHUNK,
    Scanner,
};
pub use seal::{
    open,
    seal,
};

#[cfg(feature = "k256")]
#[cfg_attr(docsrs, doc(cfg(feature = "k256")))]
pub use adapters::k256::K256;

#[cfg(feature = "x25519")]
#[cfg_attr(docsrs, doc(cfg(feature = "x25519")))]
pub use adapters::x25519::X25519;

#[cfg(feature = "grumpkin")]
#[cfg_attr(docsrs, doc(cfg(feature = "grumpkin")))]
pub use adapters::grumpkin::Grumpkin;
