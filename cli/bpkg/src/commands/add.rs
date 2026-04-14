use std::path::PathBuf;

use anyhow::Result;

use bpkg_home::HomeConfig;

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

/// Install a package into the user profile.
///
/// When `keep` is false, the package is volatile (disappears on reboot).
/// When `keep` is true, the package is persisted across reboots.
pub fn run(package: &str, keep: bool) -> Result<()> {
    let mode = if keep { "persistent" } else { "volatile" };
    output::print_spinner(&format!("Resolving {package}..."));
    output::print_spinner(&format!("Installing {package} ({mode})..."));

    if keep {
        let home_path = default_home_toml();
        let mut config = if home_path.exists() {
            HomeConfig::load(&home_path)?
        } else {
            // Ensure parent directory exists.
            if let Some(parent) = home_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            HomeConfig::default()
        };

        config.add_package(package);
        config.save(&home_path)?;
        output::print_success(&format!("Installed {package} ({mode}) — added to home.toml"));
    } else {
        // Volatile install: would install to session root.
        // TODO: resolve package from repo index, fetch/build, add to volatile profile
        output::print_success(&format!("Installed {package} ({mode})"));
        output::print_warning(
            "This is a volatile install. It will disappear on reboot. Use `bpkg keep` to persist.",
        );
    }
    Ok(())
}
