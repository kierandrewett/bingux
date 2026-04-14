use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ShimError {
    #[error("binary not found in dispatch table: {0}")]
    BinaryNotFound(String),
    #[error("dispatch table not found at {0}")]
    DispatchTableNotFound(PathBuf),
    #[error("dispatch table parse error: {0}")]
    DispatchTableParse(String),
    #[error("package binary not found: {0}")]
    PackageBinaryNotFound(PathBuf),
    #[error("sandbox launch failed: {0}")]
    SandboxLaunch(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
