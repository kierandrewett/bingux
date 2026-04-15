use std::path::PathBuf;

use crate::output;
use crate::commands::keep::run_keep;

fn default_store_root() -> PathBuf {
    std::env::var("BPKG_STORE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/system/packages"))
}

/// Add a package to the system.
/// If --keep is specified, also add to system.toml for persistence.
pub fn run(package: &str, keep: bool) {
    let store_root = default_store_root();
    let mode = if keep { "persistent" } else { "volatile" };

    output::status("add", &format!("installing {package} ({mode})..."));

    // Check if the package is already in the store
    if let Ok(store) = bpkg_store::PackageStore::new(store_root) {
        let versions = store.query(package);
        if versions.is_empty() {
            output::status("add", &format!("{package} not in store — build it first with `bsys build`"));
            return;
        }

        let pkg_id = &versions[0];
        output::status("add", &format!("found {pkg_id} in store"));

        if keep {
            // Add to system.toml keep list
            run_keep(package);
        } else {
            output::status("add", &format!("{package} added as volatile (until reboot)"));
        }

        // Recompose the generation
        output::status("add", "run `bsys apply` to update the system generation");
    } else {
        output::status("error", "failed to open package store");
    }
}
