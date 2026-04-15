use std::path::PathBuf;

use anyhow::Result;

use bpkg_store::PackageStore;

use crate::output;

fn default_store_root() -> PathBuf {
    std::env::var("BPKG_STORE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/system/packages"))
}

/// Upgrade user packages.
///
/// If `package` is Some, show info about that package's current version.
/// If `all` is true, show all packages and their versions.
pub fn run(package: Option<&str>, all: bool) -> Result<()> {
    let store_root = default_store_root();

    if all {
        output::print_spinner("Checking all user packages...");

        if let Ok(store) = PackageStore::new(store_root) {
            let packages = store.list();
            if packages.is_empty() {
                output::print_warning("No packages installed");
                return Ok(());
            }

            let mut upgradable = 0;
            for pkg_id in &packages {
                let versions = store.query(&pkg_id.name);
                if versions.len() > 1 {
                    let latest = versions.last().unwrap();
                    if latest.version != pkg_id.version {
                        output::print_success(&format!(
                            "{}: {} → {} (upgrade available)",
                            pkg_id.name, pkg_id.version, latest.version
                        ));
                        upgradable += 1;
                    }
                }
            }

            if upgradable == 0 {
                output::print_success("All packages are at their latest versions");
            } else {
                output::print_success(&format!("{upgradable} packages have upgrades available"));
            }
        } else {
            output::print_error("Failed to open store");
        }
    } else if let Some(pkg) = package {
        output::print_spinner(&format!("Checking {pkg}..."));

        if let Ok(store) = PackageStore::new(store_root) {
            let versions = store.query(pkg);
            match versions.len() {
                0 => output::print_warning(&format!("{pkg} is not installed")),
                1 => {
                    output::print_success(&format!(
                        "{pkg} {} — latest version installed",
                        versions[0].version
                    ));
                }
                n => {
                    output::print_success(&format!("{pkg} has {n} versions installed:"));
                    for v in &versions {
                        println!("  {} {}", v.name, v.version);
                    }
                }
            }
        } else {
            output::print_error("Failed to open store");
        }
    } else {
        output::print_warning("Specify a package or use --all to check for upgrades");
    }
    Ok(())
}
