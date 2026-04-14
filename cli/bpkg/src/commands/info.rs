use std::path::PathBuf;

use anyhow::Result;

use bpkg_store::PackageStore;

use crate::output;

/// Default store root when no override is provided.
fn default_store_root() -> PathBuf {
    std::env::var("BPKG_STORE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/system/packages"))
}

/// Show detailed information about a package.
pub fn run(package: &str) -> Result<()> {
    output::print_spinner(&format!("Looking up {package}..."));

    let root = default_store_root();
    let store = PackageStore::new(root)?;

    // Find all installed versions of this package.
    let installed = store.query(package);

    if let Some(id) = installed.first() {
        let manifest = store.manifest(id)?;
        output::print_package_info(
            &manifest.package.name,
            &manifest.package.version,
            &manifest.package.description,
            &manifest.package.license,
            &manifest.package.scope,
            &manifest.dependencies.runtime,
            &manifest.exports.binaries,
        );
    } else {
        output::print_warning(&format!("{package} is not installed"));
        output::print_package_info(
            package,
            "(not installed)",
            "",
            "",
            "unknown",
            &[],
            &[],
        );
    }
    Ok(())
}
