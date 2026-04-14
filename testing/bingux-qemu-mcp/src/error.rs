use std::path::PathBuf;

/// Errors produced by the QEMU MCP server.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("QEMU process failed to start: {0}")]
    QemuLaunchFailed(String),

    #[error("QEMU process exited unexpectedly (vm_id={vm_id})")]
    QemuExited { vm_id: String },

    #[error("VM not found: {0}")]
    VmNotFound(String),

    #[error("QMP connection failed for socket {path}: {source}")]
    QmpConnectionFailed {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("QMP command `{command}` failed: {detail}")]
    QmpCommandFailed { command: String, detail: String },

    #[error("QMP protocol error: {0}")]
    QmpProtocol(String),

    #[error("Serial connection failed for socket {path}: {source}")]
    SerialConnectionFailed {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Serial read timed out after {timeout_secs}s waiting for pattern `{pattern}`")]
    SerialTimeout {
        pattern: String,
        timeout_secs: u64,
    },

    #[error("Screenshot capture failed: {0}")]
    ScreenshotFailed(String),

    #[error("Image conversion failed: {0}")]
    ImageConversion(String),

    #[error("MCP protocol error: {0}")]
    McpProtocol(String),

    #[error("Invalid tool arguments: {0}")]
    InvalidArguments(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
