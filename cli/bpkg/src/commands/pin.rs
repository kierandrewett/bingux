use std::path::PathBuf;

use anyhow::{Result, bail};

use bpkg_home::HomeConfig;

use crate::args::parse_pin_spec;
use crate::output;

fn default_home_toml() -> PathBuf {
    std::env::var("BPKG_HOME_TOML")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
            PathBuf::from(home).join(".config/bingux/config/home.toml")
        })
}

/// Pin a package to a specific version.
/// Adds `pkg@version` to home.toml [packages].keep — pins are always persistent.
pub fn run_pin(spec: &str) -> Result<()> {
    let (name, version) = match parse_pin_spec(spec) {
        Some(parsed) => parsed,
        None => bail!("Invalid pin spec '{spec}'. Expected format: pkg=version"),
    };

    output::print_spinner(&format!("Pinning {name} to version {version}..."));

    let home_path = default_home_toml();
    let mut config = if home_path.exists() {
        HomeConfig::load(&home_path)?
    } else {
        if let Some(parent) = home_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        HomeConfig::default()
    };

    // Remove unpinned version if present
    config.remove_package(&name);

    // Add the pinned version
    let pinned = format!("{name}@{version}");
    config.add_package(&pinned);
    config.save(&home_path)?;

    output::print_success(&format!("{name} pinned to {version}"));
    Ok(())
}

/// Remove a version pin from a package.
pub fn run_unpin(package: &str) -> Result<()> {
    output::print_spinner(&format!("Removing pin from {package}..."));

    let home_path = default_home_toml();
    if !home_path.exists() {
        output::print_warning(&format!("{package} is not pinned"));
        return Ok(());
    }

    let mut config = HomeConfig::load(&home_path)?;

    // Check if any pinned version exists (has_package checks exact match,
    // so we need to check the raw config for @version entries)
    let pinned_name = format!("{package}@");
    let has_pin = config.has_package(package) || {
        // Read the file to check for @version entries
        let content = std::fs::read_to_string(&home_path).unwrap_or_default();
        content.contains(&pinned_name)
    };

    if !has_pin {
        output::print_warning(&format!("{package} is not in home.toml"));
        return Ok(());
    }

    // Remove any @version entry by reading and rewriting
    let content = std::fs::read_to_string(&home_path)?;
    let mut new_lines: Vec<String> = Vec::new();
    for line in content.lines() {
        if line.contains(&format!("\"{package}@")) {
            // Skip pinned entries
            continue;
        }
        new_lines.push(line.to_string());
    }
    std::fs::write(&home_path, new_lines.join("\n"))?;

    // Reload and add the unpinned version
    let mut config = HomeConfig::load(&home_path)?;
    config.add_package(package);
    config.save(&home_path)?;

    output::print_success(&format!("Removed pin from {package} — will use system default"));
    Ok(())
}
