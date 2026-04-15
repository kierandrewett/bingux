use std::path::PathBuf;

use anyhow::Result;

use bpkg_home::HomeConfig;

use crate::output;

fn default_home_toml() -> PathBuf {
    std::env::var("BPKG_HOME_TOML")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
            PathBuf::from(home).join(".config/bingux/config/home.toml")
        })
}

/// Promote a volatile package to persistent.
/// Adds the package to home.toml so it survives reboots.
pub fn run_keep(package: &str) -> Result<()> {
    output::print_spinner(&format!("Promoting {package} to persistent..."));

    let home_path = default_home_toml();
    let mut config = if home_path.exists() {
        HomeConfig::load(&home_path)?
    } else {
        if let Some(parent) = home_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        HomeConfig::default()
    };

    if config.has_package(package) {
        output::print_warning(&format!("{package} is already persistent"));
        return Ok(());
    }

    config.add_package(package);
    config.save(&home_path)?;
    output::print_success(&format!("{package} promoted to persistent — added to home.toml"));
    Ok(())
}

/// Demote a persistent package to volatile.
/// Removes from home.toml — package stays available this session but disappears on reboot.
pub fn run_unkeep(package: &str) -> Result<()> {
    output::print_spinner(&format!("Demoting {package} to volatile..."));

    let home_path = default_home_toml();
    if !home_path.exists() {
        output::print_warning(&format!("{package} is not in home.toml"));
        return Ok(());
    }

    let mut config = HomeConfig::load(&home_path)?;

    if !config.has_package(package) {
        output::print_warning(&format!("{package} is not in home.toml (already volatile)"));
        return Ok(());
    }

    config.remove_package(package);
    config.save(&home_path)?;
    output::print_success(&format!(
        "{package} demoted to volatile — will disappear on reboot"
    ));
    Ok(())
}
