#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
#[cfg(feature = "std")]
use std::vec::Vec;

/// The consumer's contribution to the suite: a domain tag, a note codec, and
/// a commitment check.
pub trait Domain {
    /// Application-level note type this domain encodes and decodes.
    type Note;

    /// Failure the note codec reports. A codec that cannot fail names
    /// [`core::convert::Infallible`].
    ///
    /// The suite discards the value at its boundary, reporting
    /// [`SealError`](crate::SealError) or
    /// [`OpenError::NoteDecode`](crate::OpenError::NoteDecode), so the type
    /// is free to carry whatever detail the domain itself finds useful.
    type Error;

    /// Domain separation tag mixed into the KDF info. Must be non-empty.
    const DOMAIN_TAG: &'static str;

    /// Encodes `note`, appending its bytes to `out`.
    ///
    /// Returns `Err` when `note` has no encoding, which fails the seal. `out`
    /// is wiped afterward, so a partial encoding left behind on failure never
    /// escapes.
    fn encode_note(note: &Self::Note, out: &mut Vec<u8>) -> Result<(), Self::Error>;

    /// Decodes a note from `bytes`, or `Err` if the bytes are not a valid
    /// note.
    fn decode_note(bytes: &[u8]) -> Result<Self::Note, Self::Error>;

    /// Re-derives the note commitment and compares it against `aad`.
    ///
    /// Defaulted to always succeed so simple domains stay simple.
    fn verify(note: &Self::Note, aad: &[u8]) -> bool {
        let _ = (note, aad);
        true
    }
}
