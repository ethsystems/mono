//! Feature-gated `Kem` adapters for concrete curves.

#[cfg(feature = "k256")]
#[cfg_attr(docsrs, doc(cfg(feature = "k256")))]
pub mod k256;

#[cfg(feature = "x25519")]
#[cfg_attr(docsrs, doc(cfg(feature = "x25519")))]
pub mod x25519;

#[cfg(feature = "grumpkin")]
#[cfg_attr(docsrs, doc(cfg(feature = "grumpkin")))]
pub mod grumpkin;
