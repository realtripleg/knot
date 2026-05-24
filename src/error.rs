//! Error type shared across the whole crate.

use std::fmt;
use std::io;

/// Crate-wide result alias so functions can just write `Result<T>`.
pub type Result<T> = std::result::Result<T, KnotError>;

/// Everything that can go wrong tying or untying a knot.
#[derive(Debug)]
pub enum KnotError {
    /// An underlying filesystem error (read/write/open).
    Io(io::Error),
    /// The file doesn't start with the `KNOT` magic bytes.
    BadMagic,
    /// The file's version byte is one this build doesn't understand.
    UnsupportedVersion(u8),
    /// The file ended before a complete header/payload could be read.
    Truncated,
    /// The stored filename wasn't valid UTF-8 (or was empty).
    BadFilename,
    /// The compressed payload is malformed; `&'static str` says how.
    Corrupt(&'static str),
    /// The untied bytes didn't match the checksum recorded at tie time.
    ChecksumMismatch { expected: u32, actual: u32 },
    /// The untied bytes weren't the length recorded at tie time.
    SizeMismatch { expected: u64, actual: u64 },
}

impl fmt::Display for KnotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KnotError::Io(e) => write!(f, "{e}"),
            KnotError::BadMagic => {
                write!(f, "this doesn't look like a .knot file (bad magic bytes)")
            }
            KnotError::UnsupportedVersion(v) => write!(
                f,
                "unsupported .knot version {v}; this build understands version {}",
                crate::format::VERSION
            ),
            KnotError::Truncated => {
                write!(f, "this knot is frayed: the file is truncated or incomplete")
            }
            KnotError::BadFilename => write!(f, "the stored filename is not valid UTF-8"),
            KnotError::Corrupt(what) => write!(f, "this knot is frayed: {what}"),
            KnotError::ChecksumMismatch { expected, actual } => write!(
                f,
                "this knot is frayed: checksum mismatch (expected {expected:#010x}, got {actual:#010x})"
            ),
            KnotError::SizeMismatch { expected, actual } => write!(
                f,
                "this knot is frayed: size mismatch (expected {expected} bytes, got {actual})"
            ),
        }
    }
}

impl std::error::Error for KnotError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            KnotError::Io(e) => Some(e),
            _ => None,
        }
    }
}

/// Lets `?` turn an `io::Error` into a `KnotError` automatically.
impl From<io::Error> for KnotError {
    fn from(e: io::Error) -> Self {
        KnotError::Io(e)
    }
}
