//! Stable machine-readable errors. The `code` strings are part of the wire
//! contract on D-Bus (error name `org.bingux.Session1.Error.<Code>`), the control
//! socket and `bsmctl --json`. Never reuse or renumber a code.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    Unauthorised,
    InvalidAccount,
    NoSession,
    AlreadyLocked,
    Busy,
    StaleGeneration,
    Cancelled,
    PamFailure,
    LogindFailure,
    OutputNotReady,
    TransitionInterrupted,
    Inhibited,
    ShutdownRejected,
    Timeout,
    HelperFailure,
    InvalidArgument,
    NotSupported,
    Internal,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unauthorised => "unauthorised",
            Self::InvalidAccount => "invalid_account",
            Self::NoSession => "no_session",
            Self::AlreadyLocked => "already_locked",
            Self::Busy => "busy",
            Self::StaleGeneration => "stale_generation",
            Self::Cancelled => "cancelled",
            Self::PamFailure => "pam_failure",
            Self::LogindFailure => "logind_failure",
            Self::OutputNotReady => "output_not_ready",
            Self::TransitionInterrupted => "transition_interrupted",
            Self::Inhibited => "inhibited",
            Self::ShutdownRejected => "shutdown_rejected",
            Self::Timeout => "timeout",
            Self::HelperFailure => "helper_failure",
            Self::InvalidArgument => "invalid_argument",
            Self::NotSupported => "not_supported",
            Self::Internal => "internal",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BsmError {
    pub code: ErrorCode,
    /// Human-readable detail for logs and UI. Never contains secrets.
    pub message: String,
}

impl BsmError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
}

impl std::fmt::Display for BsmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for BsmError {}
