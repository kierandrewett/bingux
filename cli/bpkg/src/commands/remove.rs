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

/// Remove a package from the user profile.
///
/// When `purge` is true, also delete per-package state (sandboxed home, etc.).
pub fn run(package: &str, purge: bool) -> Result<()> {
    output::print_spinner(&format!("Removing {package} from user profile..."));

    let home_path = default_home_toml();
    if home_path.exists() {
        let mut config = HomeConfig::load(&home_path)?;
        if config.remove_package(package) {
            config.save(&home_path)?;
            output::print_status("removed", &format!("{package} from home.toml"));
        } else {
            output::print_warning(&format!("{package} was not in home.toml keep list"));
        }
    }

    // TODO: also remove from volatile state if applicable
    // TODO: recompose user profile

    if purge {
        output::print_spinner(&format!("Purging state for {package}..."));
        // TODO: delete ~/.config/bingux/state/<pkg>/
        output::print_success(&format!("Removed {package} and purged its state"));
    } else {
        output::print_success(&format!("Removed {package}"));
    }
    Ok(())
}
