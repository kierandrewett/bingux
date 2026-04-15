use std::collections::HashSet;
use std::path::PathBuf;

use crate::output;

fn default_store_root() -> PathBuf {
    std::env::var("BPKG_STORE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/system/packages"))
}

fn default_config_path() -> PathBuf {
    std::env::var("BSYS_CONFIG_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/system/config/system.toml"))
}

/// Garbage collect the package store.
/// Removes packages not referenced by system.toml or any user's home.toml.
pub fn run(dry_run: bool) {
    let store_root = default_store_root();
    let config_path = default_config_path();

    // Collect all referenced package names
    let mut referenced: HashSet<String> = HashSet::new();

    // From system.toml
    if let Ok(content) = std::fs::read_to_string(&config_path) {
        if let Ok(config) = bsys_config::parse_system_config_str(&content) {
            for pkg in &config.packages.keep {
                // Extract base name (strip @version)
                let base = pkg.split('@').next().unwrap_or(pkg);
                referenced.insert(base.to_string());
            }
        }
    }

    // Scan for user home.toml files (in /users/*/...)
    // For now, also check BPKG_HOME_TOML env
    // Also scan user home.toml if available
    if let Ok(home_toml) = std::env::var("BPKG_HOME_TOML") {
        if let Ok(content) = std::fs::read_to_string(&home_toml) {
            // Simple parse: look for package names in keep = [...]
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with('"') && trimmed.ends_with('"') {
                    let name = trimmed.trim_matches('"').trim_matches(',');
                    let base = name.split('@').next().unwrap_or(name);
                    if !base.is_empty() {
                        referenced.insert(base.to_string());
                    }
                }
            }
        }
    }

    output::status("gc", &format!("{} packages referenced in configs", referenced.len()));

    // List all packages in store
    if let Ok(store) = bpkg_store::PackageStore::new(store_root.clone()) {
        let installed = store.list();
        let mut unreferenced = Vec::new();
        let mut kept = 0;

        for pkg_id in &installed {
            if referenced.contains(&pkg_id.name) {
                kept += 1;
            } else {
                unreferenced.push(pkg_id.clone());
            }
        }

        if unreferenced.is_empty() {
            output::status("gc", "nothing to collect — all packages are referenced");
            return;
        }

        output::status("gc", &format!(
            "{} kept, {} unreferenced",
            kept, unreferenced.len()
        ));

        for pkg_id in &unreferenced {
            let pkg_dir = store_root.join(pkg_id.dir_name());
            let size = dir_size(&pkg_dir);

            if dry_run {
                output::status("gc", &format!(
                    "would remove {} ({})",
                    pkg_id, human_size(size)
                ));
            } else {
                match store.remove(pkg_id) {
                    Ok(()) => output::status("gc", &format!("removed {} ({})", pkg_id, human_size(size))),
                    Err(e) => output::status("error", &format!("failed to remove {}: {e}", pkg_id)),
                }
            }
        }

        if !dry_run {
            output::status("gc", &format!("freed space from {} packages", unreferenced.len()));
        }
    } else {
        output::status("error", "failed to open store");
    }
}

fn dir_size(path: &PathBuf) -> u64 {
    walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

fn human_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes}B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1}K", bytes as f64 / 1024.0)
    } else {
        format!("{:.1}M", bytes as f64 / (1024.0 * 1024.0))
    }
}
