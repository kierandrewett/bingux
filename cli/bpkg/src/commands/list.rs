use std::path::PathBuf;

use anyhow::Result;

use bpkg_store::PackageStore;

use crate::output::{self, PackageListEntry, PackageStatus};

fn default_store_root() -> PathBuf {
    std::env::var("BPKG_STORE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/system/packages"))
}

fn default_home_toml() -> PathBuf {
    std::env::var("BPKG_HOME_TOML")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
            PathBuf::from(home).join(".config/bingux/config/home.toml")
        })
}

/// List all packages in the store with their volatility status.
pub fn run() -> Result<()> {
    let root = default_store_root();
    let store = PackageStore::new(root)?;
    let ids = store.list();

    if ids.is_empty() {
        println!("No packages installed.");
        return Ok(());
    }

    // Load home.toml to determine kept/volatile/pinned status
    let home_path = default_home_toml();
    let home_content = if home_path.exists() {
        std::fs::read_to_string(&home_path).unwrap_or_default()
    } else {
        String::new()
    };

    // Simple check: does the package name appear in the keep list?
    let is_kept = |name: &str| -> bool {
        home_content.contains(&format!("\"{name}\""))
            || home_content.contains(&format!("\"{name}@"))
    };
    let pin_version = |name: &str| -> Option<String> {
        // Look for "name@version" pattern
        let pattern = format!("\"{name}@");
        if let Some(start) = home_content.find(&pattern) {
            let after = &home_content[start + pattern.len()..];
            if let Some(end) = after.find('"') {
                return Some(after[..end].to_string());
            }
        }
        None
    };

    let packages: Vec<PackageListEntry> = ids
        .iter()
        .map(|id| {
            let status = if let Some(ver) = pin_version(&id.name) {
                PackageStatus::Pinned(ver)
            } else if is_kept(&id.name) {
                PackageStatus::Kept
            } else {
                PackageStatus::Volatile
            };

            PackageListEntry {
                name: id.name.clone(),
                version: id.version.clone(),
                status,
            }
        })
        .collect();

    output::print_package_list(&packages);
    Ok(())
}
