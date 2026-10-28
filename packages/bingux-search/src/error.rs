//! Errors, with the OpenSearch error `type` each maps to.

use std::fmt;

#[derive(Debug)]
pub enum Error {
    /// The schema is not usable (no searchable field, too many fields).
    Schema(String),
    /// The request is malformed or asks for something unsupported.
    /// `kind` is the OpenSearch error type, `reason` a sentence.
    Request { kind: &'static str, reason: String },
    /// An upsert would pass a configured limit. Nothing was changed.
    LimitExceeded(String),
    /// The index named does not exist (or is not visible to the caller).
    UnknownIndex(String),
    /// The caller cancelled the search.
    Cancelled,
    Io(std::io::Error),
    /// A snapshot file or message could not be read.
    Corrupt(String),
}

impl Error {
    pub fn request(reason: impl Into<String>) -> Self {
        Error::Request {
            kind: "illegal_argument_exception",
            reason: reason.into(),
        }
    }

    pub fn parsing(reason: impl Into<String>) -> Self {
        Error::Request {
            kind: "parsing_exception",
            reason: reason.into(),
        }
    }

    /// The OpenSearch error type.
    pub fn kind(&self) -> &'static str {
        match self {
            Error::Schema(_) => "illegal_argument_exception",
            Error::Request { kind, .. } => kind,
            Error::LimitExceeded(_) => "circuit_breaking_exception",
            Error::UnknownIndex(_) => "index_not_found_exception",
            Error::Cancelled => "task_cancelled_exception",
            Error::Io(_) => "io_exception",
            Error::Corrupt(_) => "corrupt_index_exception",
        }
    }

    /// The error as an OpenSearch error body.
    pub fn to_json(&self) -> serde_json::Value {
        let status = match self {
            Error::UnknownIndex(_) => 404,
            Error::Request { .. } | Error::Schema(_) => 400,
            Error::LimitExceeded(_) => 429,
            _ => 500,
        };
        serde_json::json!({
            "error": { "type": self.kind(), "reason": self.to_string() },
            "status": status,
        })
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Schema(s) | Error::LimitExceeded(s) | Error::Corrupt(s) => f.write_str(s),
            Error::Request { reason, .. } => f.write_str(reason),
            Error::UnknownIndex(name) => write!(f, "no such index [{name}]"),
            Error::Cancelled => f.write_str("search cancelled"),
            Error::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
