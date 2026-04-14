use std::path::PathBuf;

use crate::output;

use bingux_gated::permissions::{PackagePermissions, PermissionDb};

fn default_perms_dir() -> PathBuf {
    std::env::var("BXC_PERMS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
            PathBuf::from(home).join(".config/bingux/permissions")
        })
}

pub fn run(package: &str, reset: bool) {
    let perms_dir = default_perms_dir();
    let user = std::env::var("USER").unwrap_or_else(|_| "unknown".into());

    if reset {
        // Reset permissions by saving an empty set.
        let db = PermissionDb::new(&user, perms_dir);
        let empty = PackagePermissions::new_empty(package);
        match db.save(package, &empty) {
            Ok(()) => output::status("perms", &format!("reset all permissions for {package}")),
            Err(e) => output::status("error", &format!("failed to reset permissions: {e}")),
        }
    } else {
        // Show current permissions.
        let mut db = PermissionDb::new(&user, perms_dir);
        match db.load(package) {
            Ok(perms) => {
                if perms.capabilities.is_empty() && perms.mounts.is_empty() && perms.files.is_empty() {
                    output::status("perms", &format!("no permissions recorded for {package}"));
                    return;
                }

                if !perms.capabilities.is_empty() {
                    output::status("perms", "capabilities:");
                    for (cap, grant) in &perms.capabilities {
                        output::status("perms", &format!("  {cap}: {grant:?}"));
                    }
                }
                if !perms.mounts.is_empty() {
                    output::status("perms", "mounts:");
                    for (path, perm) in &perms.mounts {
                        output::status("perms", &format!("  {path}: {perm}"));
                    }
                }
                if !perms.files.is_empty() {
                    output::status("perms", "files:");
                    for (path, perm) in &perms.files {
                        output::status("perms", &format!("  {path}: {perm}"));
                    }
                }
            }
            Err(e) => {
                output::status("error", &format!("failed to load permissions: {e}"));
            }
        }
    }
}
