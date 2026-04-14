use std::path::PathBuf;

use crate::dispatch::DispatchEntry;

/// Given a dispatch entry, resolve the full path to the actual binary.
pub fn resolve_binary(entry: &DispatchEntry) -> PathBuf {
    PathBuf::from("/system/packages")
        .join(&entry.package)
        .join(&entry.binary)
}

/// For explicit @version: resolve directly from the package store.
pub fn resolve_versioned(name: &str, version: &str, arch: &str) -> PathBuf {
    PathBuf::from("/system/packages")
        .join(format!("{}-{}-{}", name, version, arch))
        .join("bin")
        .join(name)
}
