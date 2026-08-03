use core::{
    fmt,
    marker::PhantomData,
};

use crate::{
    error::ParseError,
    kem::Kem,
};

#[cfg(all(any(feature = "serde", feature = "wincode"), not(feature = "std")))]
use alloc::vec::Vec;
#[cfg(all(any(feature = "serde", feature = "wincode"), feature = "std"))]
use std::vec::Vec;

/// Maximum ciphertext length accepted by [`SealedNote::parse`].
pub const MAX_CT_LEN: usize = 64 * 1024;

/// Wire-format version byte for suite v1.
pub(crate) const VERSION: u8 = 1;

/// Bytes spent on the fixed version and kem-id header.
pub(crate) const HEADER_LEN: usize = 2;

/// Byte length of the key-commitment tag.
pub(crate) const COMMIT_LEN: usize = 32;

/// Byte length of the ChaCha20-Poly1305 authentication tag carried at the
/// end of `ct`.
pub(crate) const AEAD_TAG_LEN: usize = 16;

/// Suite v1 wire-format envelope: version, kem id, epk, commit, ciphertext.
///
/// Wraps its canonical bytes instead of exposing plain fields: one value
/// equals exactly one byte string, so the envelope hashes and re-emits
/// consistently. `PhantomData<fn() -> K>` ties a parsed envelope to the Kem
/// that parsed it.
pub struct SealedNote<K, B> {
    bytes: B,
    _kem: PhantomData<fn() -> K>,
}

impl<K: Kem, B: AsRef<[u8]>> SealedNote<K, B> {
    /// Parses `bytes` into a sealed note.
    ///
    /// Validates the version byte, `K::KEM_ID`, and buffer length before
    /// trusting any offset; the ciphertext length is bounded by
    /// [`MAX_CT_LEN`].
    pub fn parse(bytes: B) -> Result<Self, ParseError<B>> {
        let min_len = HEADER_LEN + K::EPK_LEN + COMMIT_LEN + AEAD_TAG_LEN;
        let len = bytes.as_ref().len();
        if len < min_len {
            return Err(ParseError::too_short(bytes, min_len, len));
        }

        let version = bytes.as_ref()[0];
        if version != VERSION {
            return Err(ParseError::wrong_version(bytes, VERSION, version));
        }

        let kem_id = bytes.as_ref()[1];
        if kem_id != K::KEM_ID {
            return Err(ParseError::wrong_kem_id(bytes, K::KEM_ID, kem_id));
        }

        let ct_len = len - (HEADER_LEN + K::EPK_LEN + COMMIT_LEN);
        if ct_len > MAX_CT_LEN {
            return Err(ParseError::ct_too_long(bytes, MAX_CT_LEN, ct_len));
        }

        Ok(Self {
            bytes,
            _kem: PhantomData,
        })
    }

    /// Wire-format version byte.
    pub fn version(&self) -> u8 {
        self.bytes.as_ref()[0]
    }

    /// KEM id byte, equal to `K::KEM_ID` for a successfully parsed envelope.
    pub fn kem_id(&self) -> u8 {
        self.bytes.as_ref()[1]
    }

    /// Ephemeral public key bytes.
    pub fn epk(&self) -> &[u8] {
        &self.bytes.as_ref()[HEADER_LEN..HEADER_LEN + K::EPK_LEN]
    }

    /// Key-commitment tag.
    pub fn commit(&self) -> &[u8; COMMIT_LEN] {
        let start = HEADER_LEN + K::EPK_LEN;
        self.bytes.as_ref()[start..]
            .first_chunk()
            .expect("parse validated the commit slice length")
    }

    /// Note ciphertext, including the trailing AEAD tag.
    pub fn ct(&self) -> &[u8] {
        let start = HEADER_LEN + K::EPK_LEN + COMMIT_LEN;
        &self.bytes.as_ref()[start..]
    }

    /// Returns the parsed input verbatim.
    pub fn as_bytes(&self) -> &[u8] {
        self.bytes.as_ref()
    }
}

impl<K, B: Clone> Clone for SealedNote<K, B> {
    fn clone(&self) -> Self {
        Self {
            bytes: self.bytes.clone(),
            _kem: PhantomData,
        }
    }
}

impl<K, B: AsRef<[u8]>> fmt::Debug for SealedNote<K, B> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SealedNote")
            .field("len", &self.bytes.as_ref().len())
            .finish()
    }
}

impl<K, B: AsRef<[u8]>> PartialEq for SealedNote<K, B> {
    fn eq(&self, other: &Self) -> bool {
        self.bytes.as_ref() == other.bytes.as_ref()
    }
}

impl<K, B: AsRef<[u8]>> Eq for SealedNote<K, B> {}

#[cfg(feature = "serde")]
#[cfg_attr(docsrs, doc(cfg(feature = "serde")))]
impl<K: Kem, B: AsRef<[u8]>> serde::Serialize for SealedNote<K, B> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(self.as_bytes())
    }
}

/// Largest byte string [`SealedNote::parse`] accepts for `K`: the fixed
/// header, epk, and commit, plus the [`MAX_CT_LEN`] ciphertext bound.
#[cfg(feature = "serde")]
const fn max_envelope_len<K: Kem>() -> usize {
    HEADER_LEN + K::EPK_LEN + COMMIT_LEN + MAX_CT_LEN
}

/// Accepts both encodings a self-describing format may use for a byte
/// string: a dedicated byte-string type and a sequence of `u8`.
///
/// Carries `K` so the sequence arm can bound how far it trusts a
/// self-reported length.
#[cfg(feature = "serde")]
struct ByteStringVisitor<K>(PhantomData<fn() -> K>);

#[cfg(feature = "serde")]
impl<'de, K: Kem> serde::de::Visitor<'de> for ByteStringVisitor<K> {
    type Value = Vec<u8>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a sealed note byte string")
    }

    fn visit_bytes<E: serde::de::Error>(self, v: &[u8]) -> Result<Self::Value, E> {
        Ok(v.to_vec())
    }

    fn visit_byte_buf<E: serde::de::Error>(self, v: Vec<u8>) -> Result<Self::Value, E> {
        Ok(v)
    }

    fn visit_seq<A: serde::de::SeqAccess<'de>>(
        self,
        mut seq: A,
    ) -> Result<Self::Value, A::Error> {
        // The hint is the sequence's own claim about its length, so it is
        // attacker-controlled. Capping the reserve at the largest envelope
        // `parse` would accept keeps a lying hint running out of input
        // rather than out of memory; an honest longer sequence still grows
        // the buffer as its bytes arrive.
        let reserve = seq.size_hint().unwrap_or(0).min(max_envelope_len::<K>());
        let mut out = Vec::with_capacity(reserve);
        while let Some(byte) = seq.next_element::<u8>()? {
            out.push(byte);
        }
        Ok(out)
    }
}

#[cfg(feature = "serde")]
#[cfg_attr(docsrs, doc(cfg(feature = "serde")))]
impl<'de, K: Kem> serde::Deserialize<'de> for SealedNote<K, Vec<u8>> {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Self, D::Error> {
        let bytes =
            deserializer.deserialize_bytes(ByteStringVisitor::<K>(PhantomData))?;
        Self::parse(bytes).map_err(serde::de::Error::custom)
    }
}

// SAFETY: TYPE_META is left at its default `Dynamic`, which carries no
// safety obligation.
#[cfg(feature = "wincode")]
#[cfg_attr(docsrs, doc(cfg(feature = "wincode")))]
unsafe impl<K: Kem, B: AsRef<[u8]>, C: wincode::config::Config> wincode::SchemaWrite<C>
    for SealedNote<K, B>
{
    type Src = Self;

    fn size_of(value: &Self::Src) -> wincode::WriteResult<usize> {
        <[u8] as wincode::SchemaWrite<C>>::size_of(value.as_bytes())
    }

    fn write(
        writer: impl wincode::io::Writer,
        value: &Self::Src,
    ) -> wincode::WriteResult<()> {
        <[u8] as wincode::SchemaWrite<C>>::write(writer, value.as_bytes())
    }
}

// SAFETY: TYPE_META is left at its default `Dynamic`, which carries no
// safety obligation.
#[cfg(feature = "wincode")]
#[cfg_attr(docsrs, doc(cfg(feature = "wincode")))]
unsafe impl<'de, K: Kem, C: wincode::config::Config> wincode::SchemaRead<'de, C>
    for SealedNote<K, Vec<u8>>
{
    type Dst = Self;

    fn read(
        reader: impl wincode::io::Reader<'de>,
        dst: &mut core::mem::MaybeUninit<Self::Dst>,
    ) -> wincode::ReadResult<()> {
        let bytes = <Vec<u8> as wincode::SchemaRead<'de, C>>::get(reader)?;
        let note = Self::parse(bytes)
            .map_err(|_| wincode::ReadError::Custom("invalid oring envelope"))?;
        dst.write(note);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[cfg(not(feature = "std"))]
    use alloc::{
        vec,
        vec::Vec,
    };
    #[cfg(feature = "std")]
    use std::{
        vec,
        vec::Vec,
    };

    use super::*;
    use crate::test_util::MockKem;

    const MIN_LEN: usize = HEADER_LEN + MockKem::EPK_LEN + COMMIT_LEN + AEAD_TAG_LEN;

    fn valid_bytes(ct_len: usize) -> Vec<u8> {
        let mut bytes =
            Vec::with_capacity(HEADER_LEN + MockKem::EPK_LEN + COMMIT_LEN + ct_len);
        bytes.push(VERSION);
        bytes.push(MockKem::KEM_ID);
        bytes.extend(vec![0xAAu8; MockKem::EPK_LEN]);
        bytes.extend(vec![0xBBu8; COMMIT_LEN]);
        bytes.extend(vec![0xCCu8; ct_len]);
        bytes
    }

    #[test]
    fn parse_round_trip_accessors() {
        let bytes = valid_bytes(AEAD_TAG_LEN + 4);
        let note = SealedNote::<MockKem, Vec<u8>>::parse(bytes).unwrap();
        assert_eq!(note.version(), VERSION);
        assert_eq!(note.kem_id(), MockKem::KEM_ID);
        assert_eq!(note.epk(), &[0xAAu8; MockKem::EPK_LEN]);
        assert_eq!(note.commit(), &[0xBBu8; COMMIT_LEN]);
        assert_eq!(note.ct(), &[0xCCu8; AEAD_TAG_LEN + 4][..]);
    }

    #[test]
    fn wrong_version_fails() {
        let mut bytes = valid_bytes(AEAD_TAG_LEN);
        bytes[0] = VERSION.wrapping_add(1);
        assert!(SealedNote::<MockKem, Vec<u8>>::parse(bytes).is_err());
    }

    #[test]
    fn wrong_kem_id_fails() {
        let mut bytes = valid_bytes(AEAD_TAG_LEN);
        bytes[1] = MockKem::KEM_ID.wrapping_add(1);
        assert!(SealedNote::<MockKem, Vec<u8>>::parse(bytes).is_err());
    }

    #[test]
    fn too_short_fails() {
        let bytes = vec![0u8; MIN_LEN - 1];
        assert!(SealedNote::<MockKem, Vec<u8>>::parse(bytes).is_err());
    }

    #[test]
    fn ct_too_long_fails() {
        let bytes = valid_bytes(MAX_CT_LEN + 1);
        assert!(SealedNote::<MockKem, Vec<u8>>::parse(bytes).is_err());
    }

    #[test]
    fn ct_at_max_len_succeeds() {
        let bytes = valid_bytes(MAX_CT_LEN);
        assert!(SealedNote::<MockKem, Vec<u8>>::parse(bytes).is_ok());
    }

    #[test]
    fn parse_error_into_inner_returns_input() {
        let bytes = vec![0u8; MIN_LEN - 1];
        let Err(err) = SealedNote::<MockKem, Vec<u8>>::parse(bytes.clone()) else {
            panic!("parse of a too-short buffer must fail");
        };
        assert_eq!(err.into_inner(), bytes);
    }

    #[test]
    fn as_bytes_returns_input_verbatim() {
        let bytes = valid_bytes(AEAD_TAG_LEN);
        let note = SealedNote::<MockKem, Vec<u8>>::parse(bytes.clone()).unwrap();
        assert_eq!(note.as_bytes(), bytes.as_slice());
    }

    #[test]
    fn clone_preserves_bytes_and_compares_equal() {
        let bytes = valid_bytes(AEAD_TAG_LEN);
        let note = SealedNote::<MockKem, Vec<u8>>::parse(bytes.clone()).unwrap();
        let cloned = note.clone();
        assert_eq!(cloned.as_bytes(), bytes.as_slice());
        assert_eq!(note, cloned);
    }

    #[test]
    fn parses_from_owned_and_borrowed_storage() {
        let bytes = valid_bytes(AEAD_TAG_LEN);
        let owned = SealedNote::<MockKem, Vec<u8>>::parse(bytes.clone()).unwrap();
        let borrowed = SealedNote::<MockKem, &[u8]>::parse(bytes.as_slice()).unwrap();
        assert_eq!(owned.as_bytes(), borrowed.as_bytes());
    }
}
