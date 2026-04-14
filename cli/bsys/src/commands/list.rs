use std::path::PathBuf;

use crate::output;

use bpkg_store::PackageStore;

fn default_store_root() -> PathBuf {
    std::env::var("BSYS_PACKAGES_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/system/packages"))
}

pub fn run() {
    let root = default_store_root();
    match PackageStore::new(root) {
        Ok(store) => {
            let ids = store.list();
            if ids.is_empty() {
                output::status("list", "no system packages installed");
            } else {
                for id in &ids {
                    output::status("list", &format!("{} {}", id.name, id.version));
                }
            }
        }
        Err(e) => {
            output::status("error", &format!("failed to open package store: {e}"));
        }
    }
}

pub fn info(package: &str) {
    let root = default_store_root();
    match PackageStore::new(root) {
        Ok(store) => {
            let installed = store.query(package);
            if let Some(id) = installed.first() {
                match store.manifest(id) {
                    Ok(manifest) => {
                        output::status("info", &format!(
                            "{} {} — {}",
                            manifest.package.name,
                            manifest.package.version,
                            manifest.package.description
                        ));
                    }
                    Err(e) => {
                        output::status("error", &format!("failed to read manifest: {e}"));
                    }
                }
            } else {
                output::status("info", &format!("{package} is not installed"));
            }
        }
        Err(e) => {
            output::status("error", &format!("failed to open package store: {e}"));
        }
    }
}
