use std::path::PathBuf;

use anyhow::Result;

use bpkg_home::HomeConfig;
use bpkg_repo::archive::{extract_bgx, verify_bgx};
use bpkg_repo::resolve::{InstallSource, parse_install_source};
use bpkg_store::PackageStore;

use crate::output;

/// Default path for the user's home.toml.
fn default_home_toml() -> PathBuf {
    std::env::var("BPKG_HOME_TOML")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
            PathBuf::from(home).join(".config/bingux/config/home.toml")
        })
}

fn default_store_root() -> PathBuf {
    std::env::var("BPKG_STORE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/system/packages"))
}

/// Install a package into the user profile.
///
/// Accepts three input types:
/// - Package name: `bpkg add firefox` → resolve from repo
/// - Scoped name: `bpkg add @brave.brave-browser` → resolve from specific repo
/// - File path: `bpkg add ./firefox.bgx` → install from local .bgx file
///
/// When `keep` is false, the package is volatile (disappears on reboot).
/// When `keep` is true, the package is persisted across reboots.
pub fn run(package: &str, keep: bool) -> Result<()> {
    let mode = if keep { "persistent" } else { "volatile" };
    let source = parse_install_source(package);

    match source {
        InstallSource::File(path) => {
            // Install from .bgx file
            install_from_bgx(&path, keep)?;
        }
        InstallSource::Name(name) | InstallSource::Scoped(_, name) => {
            output::print_spinner(&format!("Resolving {name}..."));
            output::print_spinner(&format!("Installing {name} ({mode})..."));

            if keep {
                let home_path = default_home_toml();
                let mut config = if home_path.exists() {
                    HomeConfig::load(&home_path)?
                } else {
                    if let Some(parent) = home_path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    HomeConfig::default()
                };

                config.add_package(&name);
                config.save(&home_path)?;
                output::print_success(&format!(
                    "Installed {name} ({mode}) — added to home.toml"
                ));
            } else {
                // Volatile install: package is already in the store,
                // just add a symlink to the session profile
                let session_dir = std::env::var("BPKG_SESSION_ROOT")
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|_| std::path::PathBuf::from("/run/bingux/session"));

                let session_bin = session_dir.join("bin");
                let _ = std::fs::create_dir_all(&session_bin);

                // Find the package in the store and link its binaries
                let store_root = default_store_root();
                if let Ok(store) = bpkg_store::PackageStore::new(store_root) {
                    let versions = store.query(&name);
                    if let Some(pkg_id) = versions.into_iter().next() {
                        if let Some(pkg_dir) = store.get(&pkg_id) {
                            let bin_dir = pkg_dir.join("bin");
                            if bin_dir.is_dir() {
                                if let Ok(entries) = std::fs::read_dir(&bin_dir) {
                                    for entry in entries.flatten() {
                                        let bin_name = entry.file_name();
                                        let link = session_bin.join(&bin_name);
                                        let target = format!(
                                            "/system/packages/{}/bin/{}",
                                            pkg_id.dir_name(),
                                            bin_name.to_string_lossy()
                                        );
                                        let _ = std::os::unix::fs::symlink(&target, &link);
                                    }
                                }
                            }
                        }
                        output::print_success(&format!("Installed {name} ({mode}) — session only"));
                        output::print_warning(
                            "Volatile install — disappears on reboot. Use `bpkg keep` to persist.",
                        );
                    } else {
                        output::print_warning(&format!("{name} not found in store"));
                    }
                } else {
                    output::print_success(&format!("Installed {name} ({mode})"));
                    output::print_warning("Volatile — disappears on reboot.");
                }
            }
        }
    }
    Ok(())
}

/// Install a package from a local .bgx archive file.
fn install_from_bgx(bgx_path: &PathBuf, keep: bool) -> Result<()> {
    output::print_spinner(&format!("Verifying {}...", bgx_path.display()));

    // Verify the archive first
    let info = verify_bgx(bgx_path).map_err(|e| anyhow::anyhow!("invalid .bgx: {e}"))?;
    output::print_spinner(&format!(
        "Installing {} {} from .bgx...",
        info.name, info.version
    ));

    // Extract to the store
    let store_root = default_store_root();
    let extracted_id =
        extract_bgx(bgx_path, &store_root).map_err(|e| anyhow::anyhow!("extract failed: {e}"))?;

    output::print_success(&format!(
        "Installed {} {} from .bgx → {}",
        info.name, info.version, extracted_id
    ));

    // If --keep, add to home.toml
    if keep {
        let home_path = default_home_toml();
        let mut config = if home_path.exists() {
            HomeConfig::load(&home_path)?
        } else {
            if let Some(parent) = home_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            HomeConfig::default()
        };
        config.add_package(&info.name);
        config.save(&home_path)?;
        output::print_success("Added to home.toml (persistent)");
    }

    Ok(())
}
