use std::path::PathBuf;

/// Top-level error type for the Bingux system.
#[derive(Debug, thiserror::Error)]
pub enum BinguxError {
    // ── Package ID parsing ─────────────────────────────────────

    #[error("invalid package ID: {0}")]
    InvalidPackageId(String),

    #[error("invalid scope: {0}")]
    InvalidScope(String),

    #[error("invalid version: {0}")]
    InvalidVersion(String),

    #[error("invalid architecture: {0}")]
    InvalidArch(String),

    // ── Recipe parsing ─────────────────────────────────────────

    #[error("recipe parse error in {path}: {message}")]
    RecipeParse { path: PathBuf, message: String },

    #[error("recipe validation error: {0}")]
    RecipeValidation(String),

    // ── Package store ──────────────────────────────────────────

    #[error("package not found: {0}")]
    PackageNotFound(String),

    #[error("package already exists: {0}")]
    PackageAlreadyExists(String),

    #[error("manifest error in {package}: {message}")]
    Manifest { package: String, message: String },

    #[error("file integrity check failed for {path}: expected {expected}, got {actual}")]
    IntegrityCheckFailed {
        path: PathBuf,
        expected: String,
        actual: String,
    },

    // ── Dependency resolution ──────────────────────────────────

    #[error("unresolved dependency: {package} requires {dependency}")]
    UnresolvedDependency { package: String, dependency: String },

    #[error("dependency cycle detected: {0}")]
    DependencyCycle(String),

    #[error("conflict: {0} and {1} both export {2}")]
    ExportConflict(String, String, String),

    #[error("library not found: {library} (needed by {binary})")]
    LibraryNotFound { library: String, binary: String },

    // ── Patchelf ───────────────────────────────────────────────

    #[error("ELF parse error for {path}: {message}")]
    ElfParse { path: PathBuf, message: String },

    #[error("patchelf failed for {path}: {message}")]
    PatchelfFailed { path: PathBuf, message: String },

    // ── Build ──────────────────────────────────────────────────

    #[error("build failed for {package}: {message}")]
    BuildFailed { package: String, message: String },

    #[error("source fetch failed for {url}: {message}")]
    FetchFailed { url: String, message: String },

    #[error("checksum mismatch for {path}: expected {expected}, got {actual}")]
    ChecksumMismatch {
        path: PathBuf,
        expected: String,
        actual: String,
    },

    // ── Sandbox / permissions ──────────────────────────────────

    #[error("sandbox creation failed: {0}")]
    SandboxCreation(String),

    #[error("permission denied: {package} requires {permission}")]
    PermissionDenied {
        package: String,
        permission: String,
    },

    // ── Configuration ──────────────────────────────────────────

    #[error("config error in {path}: {message}")]
    Config { path: PathBuf, message: String },

    // ── Composition ────────────────────────────────────────────

    #[error("generation error: {0}")]
    Generation(String),

    // ── Generic wrappers ───────────────────────────────────────

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML deserialization error: {0}")]
    TomlDeserialize(#[from] toml::de::Error),

    #[error("TOML serialization error: {0}")]
    TomlSerialize(#[from] toml::ser::Error),
}

pub type Result<T> = std::result::Result<T, BinguxError>;
