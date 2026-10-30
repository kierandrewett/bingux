//! The crate's single error type.

use std::{fmt, io, path::PathBuf};

/// Everything that can go wrong while recording or reading usage data.
#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Sqlite(rusqlite::Error),
    /// The database file does not exist (the recorder has never run).
    NotFound(PathBuf),
    /// The database was written by an older schema and has not been
    /// migrated yet; only the recorder (or [`crate::Settings`]) migrates.
    Outdated {
        found: i64,
        expected: i64,
    },
    /// The database was written by a newer version of this crate.
    TooNew {
        found: i64,
        supported: i64,
    },
    /// A scope names an interface or connection that is not in the data.
    UnknownScope(String),
    /// The data source cannot answer this query (vnStat has no
    /// per-connection or per-minute history).
    Unsupported(&'static str),
    /// A caller-supplied value is out of range.
    Invalid(String),
    /// An external command (`nmcli`, `vnstat`) failed or printed
    /// something unparseable.
    Command(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(error) => write!(f, "{error}"),
            Error::Sqlite(error) => write!(f, "database: {error}"),
            Error::NotFound(path) => write!(f, "no usage database at {}", path.display()),
            Error::Outdated { found, expected } => write!(
                f,
                "usage database has schema version {found}, expected {expected}"
            ),
            Error::TooNew { found, supported } => write!(
                f,
                "usage database has schema version {found}, this build supports up to {supported}"
            ),
            Error::UnknownScope(name) => write!(f, "no usage data for {name}"),
            Error::Unsupported(what) => write!(f, "not supported by this data source: {what}"),
            Error::Invalid(what) => write!(f, "{what}"),
            Error::Command(what) => write!(f, "{what}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(error) => Some(error),
            Error::Sqlite(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Error::Io(error)
    }
}

impl From<rusqlite::Error> for Error {
    fn from(error: rusqlite::Error) -> Self {
        Error::Sqlite(error)
    }
}
