/// Error type for the repository subsystem.
#[derive(Debug, thiserror::Error)]
pub enum RepoError {
    #[error("archive creation failed: {0}")]
    ArchiveCreate(String),

    #[error("archive extraction failed: {0}")]
    ArchiveExtract(String),

    #[error("archive verification failed: {0}")]
    ArchiveVerify(String),

    #[error("index parse error: {0}")]
    IndexParse(String),

    #[error("package not found: {0}")]
    PackageNotFound(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML deserialization error: {0}")]
    TomlDe(#[from] toml::de::Error),

    #[error("TOML serialization error: {0}")]
    TomlSer(#[from] toml::ser::Error),
}
