/// Summary information extracted from a `.bgx` archive.
#[derive(Debug, Clone)]
pub struct BgxInfo {
    /// Full package identifier (e.g. `firefox-128.0.1-x86_64-linux`).
    pub package_id: String,
    /// Package name.
    pub name: String,
    /// Package version.
    pub version: String,
    /// Target architecture.
    pub arch: String,
    /// Repository scope.
    pub scope: String,
    /// Human-readable description.
    pub description: String,
    /// Uncompressed size in bytes of the archive.
    pub size: u64,
}
