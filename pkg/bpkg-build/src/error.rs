use std::path::PathBuf;

/// Errors that can occur during the build pipeline.
#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error("recipe error: {0}")]
    Recipe(#[from] bpkg_recipe::RecipeError),

    #[error("store error: {0}")]
    Store(#[from] bingux_common::BinguxError),

    #[error("fetch failed for {url}: {message}")]
    FetchFailed { url: String, message: String },

    #[error("checksum mismatch for {path}: expected {expected}, got {actual}")]
    ChecksumMismatch {
        path: PathBuf,
        expected: String,
        actual: String,
    },

    #[error("build failed: exit code {code}\n{stderr}")]
    BuildFailed { code: i32, stderr: String },

    #[error("package step failed: exit code {code}\n{stderr}")]
    PackageFailed { code: i32, stderr: String },

    #[error("dependency not in store: {0}")]
    MissingDependency(String),

    #[error("extraction failed for {path}: {message}")]
    ExtractionFailed { path: PathBuf, message: String },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, BuildError>;
