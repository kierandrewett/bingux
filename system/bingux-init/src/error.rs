#[derive(Debug, thiserror::Error)]
pub enum InitError {
    #[error("mount failed: {target}: {device}")]
    MountFailed { device: String, target: String },
    #[error("config not found: {0}")]
    ConfigNotFound(String),
    #[error("directory creation failed: {0}")]
    DirectoryCreationFailed(String),
    #[error("symlink creation failed: {target} -> {link}")]
    SymlinkFailed { target: String, link: String },
    #[error("switch_root failed: {0}")]
    SwitchRootFailed(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
