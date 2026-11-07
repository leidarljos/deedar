//! The one error type of the crate.

use std::fmt;

/// Why a store operation, a tile read or a note check did not go through.
#[derive(Debug)]
pub enum Error {
    /// The store could not be read or written.
    Io(std::io::Error),
    /// Bytes in the store do not agree with the tree they claim to be part of,
    /// or are not in the shape the layout gives them.
    Corrupt {
        /// The store path the bytes came from.
        path: String,
        /// What did not agree.
        reason: String,
    },
    /// Text (a note, a checkpoint, a key, a manifest) is not in its format.
    Format(String),
    /// Another writer holds the log; the path is the lock file.
    Locked(String),
    /// An entry longer than a bundle can length-prefix (65535 bytes).
    EntryTooLong(usize),
    /// A tile, hash or entry index that the tree of this size does not have.
    OutOfRange(String),
    /// A signature on a note did not check, or no trusted key signed it.
    Signature(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "store: {e}"),
            Self::Corrupt { path, reason } => write!(f, "{path}: {reason}"),
            Self::Format(what) => write!(f, "format: {what}"),
            Self::Locked(path) => write!(f, "another writer holds {path}"),
            Self::EntryTooLong(len) => write!(f, "entry of {len} bytes exceeds 65535"),
            Self::OutOfRange(what) => write!(f, "out of range: {what}"),
            Self::Signature(what) => write!(f, "signature: {what}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// `Result` with this crate's [`Error`].
pub type Result<T> = std::result::Result<T, Error>;
