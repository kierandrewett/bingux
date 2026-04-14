//! Error types for bingux-gated.

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum GatedError {
    #[error("permission file parse error in {path}: {message}")]
    PermissionParse { path: PathBuf, message: String },

    #[error("permission serialization error: {0}")]
    PermissionSerialize(String),

    #[error("unknown syscall: {0}")]
    UnknownSyscall(i64),

    #[error("pid {pid} not found in registry")]
    PidNotFound { pid: u32 },

    #[error("prompt cancelled or failed: {0}")]
    PromptFailed(String),

    #[error("process memory read failed for pid {pid}: {message}")]
    ProcessMemoryRead { pid: u32, message: String },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, GatedError>;
