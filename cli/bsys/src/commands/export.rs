use std::path::{Path, PathBuf};

use crate::output;

fn default_store_root() -> PathBuf {
    std::env::var("BPKG_STORE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/system/packages"))
}

fn default_export_dir() -> PathBuf {
    std::env::var("BSYS_EXPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/system/exports"))
}

pub fn run(package: Option<&str>, all: bool, index: Option<&Path>) {
    let store_root = default_store_root();
    let export_dir = default_export_dir();
    let _ = std::fs::create_dir_all(&export_dir);

    if let Some(dir) = index {
        // Generate index.toml from a directory of .bgx files
        match bpkg_repo::RepoIndex::generate_from_directory(dir, "bingux") {
            Ok(idx) => {
                let index_path = dir.join("index.toml");
                match idx.save(&index_path) {
                    Ok(()) => output::status("export", &format!(
                        "index.toml generated with {} packages at {}",
                        idx.packages.len(), index_path.display()
                    )),
                    Err(e) => output::status("error", &format!("failed to write index: {e}")),
                }
            }
            Err(e) => output::status("error", &format!("failed to generate index: {e}")),
        }
    } else if all {
        // Export all packages
        if let Ok(store) = bpkg_store::PackageStore::new(store_root.clone()) {
            let packages = store.list();
            let mut exported = 0;
            for pkg_id in &packages {
                if let Some(pkg_dir) = store.get(pkg_id) {
                    let bgx_path = export_dir.join(pkg_id.bgx_filename());
                    match bpkg_repo::archive::create_bgx(&pkg_dir, &bgx_path) {
                        Ok(()) => {
                            let size = std::fs::metadata(&bgx_path).map(|m| m.len()).unwrap_or(0);
                            output::status("export", &format!(
                                "{} → {} ({})",
                                pkg_id, bgx_path.display(), human_size(size)
                            ));
                            exported += 1;
                        }
                        Err(e) => output::status("error", &format!("failed to export {pkg_id}: {e}")),
                    }
                }
            }
            output::status("export", &format!("{exported} packages exported to {}", export_dir.display()));
        } else {
            output::status("error", "failed to open store");
        }
    } else if let Some(pkg_name) = package {
        // Export a single package
        if let Ok(store) = bpkg_store::PackageStore::new(store_root) {
            let versions = store.query(pkg_name);
            if let Some(pkg_id) = versions.into_iter().next() {
                if let Some(pkg_dir) = store.get(&pkg_id) {
                    let bgx_path = export_dir.join(pkg_id.bgx_filename());
                    match bpkg_repo::archive::create_bgx(&pkg_dir, &bgx_path) {
                        Ok(()) => {
                            let size = std::fs::metadata(&bgx_path).map(|m| m.len()).unwrap_or(0);
                            output::status("export", &format!(
                                "{} → {} ({})",
                                pkg_id, bgx_path.display(), human_size(size)
                            ));
                        }
                        Err(e) => output::status("error", &format!("failed to export: {e}")),
                    }
                }
            } else {
                output::status("error", &format!("{pkg_name} not found in store"));
            }
        }
    } else {
        output::status("export", "usage: bsys export <pkg> | --all | --index <dir>");
    }
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
