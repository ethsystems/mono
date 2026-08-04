use core::fmt;

/// Error returned when parsing bytes into a `SealedNote` fails.
///
/// Carries the rejected input so it is never silently dropped; the
/// `FromUtf8Error` pattern.
#[derive(Debug)]
pub struct ParseError<B> {
    input: B,
    kind: ParseErrorKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParseErrorKind {
    TooShort { expected: usize, actual: usize },
    WrongVersion { expected: u8, actual: u8 },
    WrongKemId { expected: u8, actual: u8 },
    CtTooLong { max: usize, actual: usize },
}

impl<B> ParseError<B> {
    pub(crate) fn too_short(input: B, expected: usize, actual: usize) -> Self {
        Self {
            input,
            kind: ParseErrorKind::TooShort { expected, actual },
        }
    }

    pub(crate) fn wrong_version(input: B, expected: u8, actual: u8) -> Self {
        Self {
            input,
            kind: ParseErrorKind::WrongVersion { expected, actual },
        }
    }

    pub(crate) fn wrong_kem_id(input: B, expected: u8, actual: u8) -> Self {
        Self {
            input,
            kind: ParseErrorKind::WrongKemId { expected, actual },
        }
    }

    pub(crate) fn ct_too_long(input: B, max: usize, actual: usize) -> Self {
        Self {
            input,
            kind: ParseErrorKind::CtTooLong { max, actual },
        }
    }

    /// Returns the input that failed to parse.
    pub fn into_inner(self) -> B {
        self.input
    }
}

impl<B> fmt::Display for ParseError<B> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            ParseErrorKind::TooShort { expected, actual } => {
                write!(
                    f,
                    "envelope too short: {actual} bytes, need at least {expected}"
                )
            }
            ParseErrorKind::WrongVersion { expected, actual } => {
                write!(
                    f,
                    "unexpected envelope version {actual}, expected {expected}"
                )
            }
            ParseErrorKind::WrongKemId { expected, actual } => {
                write!(f, "unexpected kem id {actual}, expected {expected}")
            }
            ParseErrorKind::CtTooLong { max, actual } => {
                write!(f, "ciphertext too long: {actual} bytes exceeds max {max}")
            }
        }
    }
}

impl<B: fmt::Debug> core::error::Error for ParseError<B> {}

/// Error returned when sealing a note fails.
///
/// Opaque by design: AEAD failure reasons are not distinguished, so a
/// caller cannot learn anything about why encryption failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SealError;

impl fmt::Display for SealError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "failed to seal note")
    }
}

impl core::error::Error for SealError {}

/// Error returned when a sealed note's commit matches the opening key but
/// the note still fails to open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenError {
    /// AEAD decryption or authentication failed.
    Aead,
    /// The domain's note decoder rejected the plaintext.
    NoteDecode,
    /// `Domain::verify` rejected the note against the AAD.
    Verify,
}

impl fmt::Display for OpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Aead => write!(f, "AEAD decryption failed after commit matched"),
            Self::NoteDecode => write!(f, "note decoding failed after commit matched"),
            Self::Verify => write!(f, "domain verification failed after commit matched"),
        }
    }
}

impl core::error::Error for OpenError {}

/// Error yielded by `Scanner::scan` for an envelope whose commit matched the
/// scanning key but which failed to open.
///
/// Commit mismatch is not an error and never produces this type: it means
/// the envelope was not addressed to the scanning key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Malformed(pub OpenError);

impl fmt::Display for Malformed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "malformed envelope: {}", self.0)
    }
}

impl core::error::Error for Malformed {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        Some(&self.0)
    }
}

#[cfg(test)]
mod tests {
    #[cfg(not(feature = "std"))]
    use alloc::{
        string::ToString,
        vec,
    };
    #[cfg(feature = "std")]
    use std::{
        string::ToString,
        vec,
    };

    use super::*;

    #[test]
    fn parse_error_into_inner_returns_input() {
        let input = vec![1u8, 2, 3];
        let err = ParseError::too_short(input.clone(), 10, 3);
        assert_eq!(err.into_inner(), input);
    }

    #[test]
    fn parse_error_display_too_short() {
        let err = ParseError::too_short(vec![0u8], 10, 1);
        assert_eq!(
            err.to_string(),
            "envelope too short: 1 bytes, need at least 10"
        );
    }

    #[test]
    fn parse_error_display_wrong_version() {
        let err = ParseError::wrong_version(vec![0u8], 1, 2);
        assert_eq!(err.to_string(), "unexpected envelope version 2, expected 1");
    }

    #[test]
    fn parse_error_display_wrong_kem_id() {
        let err = ParseError::wrong_kem_id(vec![0u8], 1, 2);
        assert_eq!(err.to_string(), "unexpected kem id 2, expected 1");
    }

    #[test]
    fn parse_error_display_ct_too_long() {
        let err = ParseError::ct_too_long(vec![0u8], 65536, 65537);
        assert_eq!(
            err.to_string(),
            "ciphertext too long: 65537 bytes exceeds max 65536"
        );
    }

    #[test]
    fn seal_error_display() {
        assert_eq!(SealError.to_string(), "failed to seal note");
    }

    #[test]
    fn open_error_display() {
        assert_eq!(
            OpenError::Aead.to_string(),
            "AEAD decryption failed after commit matched"
        );
        assert_eq!(
            OpenError::NoteDecode.to_string(),
            "note decoding failed after commit matched"
        );
        assert_eq!(
            OpenError::Verify.to_string(),
            "domain verification failed after commit matched"
        );
    }

    #[test]
    fn malformed_display_wraps_open_error() {
        let err = Malformed(OpenError::Aead);
        assert_eq!(
            err.to_string(),
            "malformed envelope: AEAD decryption failed after commit matched"
        );
    }
}
