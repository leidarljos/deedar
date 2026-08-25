use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidId(String),
    InvalidKind(String),
    InvalidUrl(String),
    InvalidMessageId(String),
    InvalidBody(&'static str),
    NotFound(String),
    Frozen(String),
    Tombstoned(String),
    Cycle(String),
    Evidence(String),
    Io(String),
    Decode(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidId(s) => write!(f, "invalid deed id: {s}"),
            Self::InvalidKind(s) => write!(f, "invalid kind: {s}"),
            Self::InvalidUrl(s) => write!(f, "invalid url: {s}"),
            Self::InvalidMessageId(s) => write!(f, "invalid message-id: {s}"),
            Self::InvalidBody(s) => write!(f, "invalid body: {s}"),
            Self::NotFound(s) => write!(f, "deed not found: {s}"),
            Self::Frozen(s) => write!(f, "deed frozen: {s}"),
            Self::Tombstoned(s) => write!(f, "deed tombstoned: {s}"),
            Self::Cycle(s) => write!(f, "source trail cycle at {s}"),
            Self::Evidence(s) => write!(f, "evidence: {s}"),
            Self::Io(s) => write!(f, "store io: {s}"),
            Self::Decode(s) => write!(f, "store decode: {s}"),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
