use std::path::PathBuf;

use crate::output;

fn default_store_root() -> PathBuf {
    std::env::var("BPKG_STORE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/system/packages"))
}

/// Open an interactive shell inside a package's sandbox.
/// The shell runs with the per-package home and the package's files visible.
pub fn run(package: &str) {
    let store_root = default_store_root();

    // Find the package in the store
    if let Ok(store) = bpkg_store::PackageStore::new(store_root) {
        let versions = store.query(package);
        if let Some(pkg_id) = versions.into_iter().next() {
            if let Some(pkg_dir) = store.get(&pkg_id) {
                output::status("shell", &format!("entering sandbox for {pkg_id}"));
                output::status("shell", &format!("package dir: {}", pkg_dir.display()));

                // In a full implementation, this would:
                // 1. Create a mount namespace
                // 2. Mount the per-package home
                // 3. Mount /system/packages (ro)
                // 4. Install seccomp filter
                // 5. exec /bin/sh

                // For now, set up environment and exec a shell with package in PATH
                let pkg_bin = pkg_dir.join("bin");
                let current_path = std::env::var("PATH").unwrap_or_default();
                let new_path = format!("{}:{}", pkg_bin.display(), current_path);

                output::status("shell", &format!("PATH includes {}", pkg_bin.display()));
                output::status("shell", "sandbox features not yet active (requires root for namespaces)");

                // Actually exec the shell with the package in PATH
                let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
                let err = std::process::Command::new(&shell)
                    .env("PATH", &new_path)
                    .env("BINGUX_SANDBOX", package)
                    .env("PS1", &format!("[bxc:{package}] \\$ "))
                    .status();

                match err {
                    Ok(status) => {
                        output::status("shell", &format!("shell exited with {}", status));
                    }
                    Err(e) => {
                        output::status("error", &format!("failed to launch shell: {e}"));
                    }
                }
            } else {
                output::status("error", &format!("{pkg_id} exists but directory not found"));
            }
        } else {
            output::status("error", &format!("{package} not found in store"));
        }
    } else {
        output::status("error", "failed to open package store");
    }
}
