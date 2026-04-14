use std::path::PathBuf;

use bingux_common::paths::SystemPaths;

/// Configuration for the build pipeline.
#[derive(Debug, Clone)]
pub struct BuildConfig {
    /// Path to the BPKGBUILD recipe file.
    pub recipe_path: PathBuf,
    /// Package store root (e.g. `/system/packages/`).
    pub store_root: PathBuf,
    /// Temporary build directory for intermediate artifacts.
    pub work_dir: PathBuf,
    /// Directory for cached source downloads.
    pub source_cache: PathBuf,
    /// Target architecture (e.g. `x86_64-linux`).
    pub arch: String,
    /// Whether network fetching is allowed for source downloads.
    pub network_fetch: bool,
}

impl BuildConfig {
    /// Create a `BuildConfig` with sane defaults for the running system.
    pub fn default_for_system() -> Self {
        Self {
            recipe_path: PathBuf::new(),
            store_root: PathBuf::from(SystemPaths::PACKAGES),
            work_dir: PathBuf::from("/tmp/bpkg-build"),
            source_cache: PathBuf::from("/tmp/bpkg-sources"),
            arch: "x86_64-linux".to_string(),
            network_fetch: true,
        }
    }
}
