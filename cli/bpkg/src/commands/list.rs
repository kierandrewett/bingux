use std::path::PathBuf;

use anyhow::Result;

use bpkg_store::PackageStore;

use crate::output::{self, PackageListEntry, PackageStatus};

/// Default store root when no override is provided.
fn default_store_root() -> PathBuf {
    std::env::var("BPKG_STORE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/system/packages"))
}

/// List all user-installed packages with their status.
pub fn run() -> Result<()> {
    let root = default_store_root();
    let store = PackageStore::new(root)?;
    let ids = store.list();

    if ids.is_empty() {
        println!("No packages installed.");
        return Ok(());
    }

    let packages: Vec<PackageListEntry> = ids
        .iter()
        .map(|id| PackageListEntry {
            name: id.name.clone(),
            version: id.version.clone(),
            // TODO: read kept/volatile/pinned status from user profile state
            status: PackageStatus::Kept,
        })
        .collect();

    output::print_package_list(&packages);
    Ok(())
}
