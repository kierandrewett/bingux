use std::path::PathBuf;

use anyhow::Result;

use bpkg_home::{ApplyEngine, HomeConfig, compute_delta, compute_status};

use crate::output;

/// Default path for the user's home.toml.
fn default_home_toml() -> PathBuf {
    std::env::var("BPKG_HOME_TOML")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = home_dir();
            home.join(".config/bingux/config/home.toml")
        })
}

fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/root"))
}

/// Converge the full environment to home.toml.
pub fn run_apply(path: Option<&PathBuf>) -> Result<()> {
    let home_toml = path.cloned().unwrap_or_else(default_home_toml);
    let display_path = home_toml.display().to_string();

    output::print_spinner(&format!("Applying home configuration from {display_path}..."));

    let config = HomeConfig::load(&home_toml)?;
    let config_dir = home_toml.parent().unwrap_or_else(|| std::path::Path::new("."));
    let home = home_dir();

    // TODO: read actual current packages from profile state
    let current_packages: Vec<String> = Vec::new();

    let delta = compute_delta(&config, config_dir, &home, &current_packages);

    let engine = ApplyEngine::new(home.clone(), config_dir.to_path_buf());
    let summary = engine.apply(&delta)?;

    if summary.packages_added > 0 {
        output::print_status("packages", &format!("{} to add", summary.packages_added));
    }
    if summary.packages_removed > 0 {
        output::print_status("packages", &format!("{} to remove", summary.packages_removed));
    }
    if summary.dotfiles_linked > 0 {
        output::print_status("dotfiles", &format!("{} linked", summary.dotfiles_linked));
    }
    if summary.env_vars_set > 0 {
        output::print_status("env", &format!("{} variables set", summary.env_vars_set));
    }
    if summary.dconf_applied > 0 {
        output::print_status("dconf", &format!("{} settings applied", summary.dconf_applied));
    }

    output::print_success("Home environment converged");
    Ok(())
}

/// Show what applying home.toml would change.
pub fn run_diff() -> Result<()> {
    output::print_spinner("Comparing declared state to current...");

    let home_toml = default_home_toml();
    if !home_toml.exists() {
        output::print_warning("No home.toml found");
        return Ok(());
    }

    let config = HomeConfig::load(&home_toml)?;
    let config_dir = home_toml.parent().unwrap_or_else(|| std::path::Path::new("."));
    let home = home_dir();

    // TODO: read actual current packages from profile state
    let current_packages: Vec<String> = Vec::new();

    let delta = compute_delta(&config, config_dir, &home, &current_packages);

    let mut has_changes = false;

    if !delta.packages_to_add.is_empty() {
        has_changes = true;
        println!("Packages to add:");
        for pkg in &delta.packages_to_add {
            println!("  + {pkg}");
        }
    }
    if !delta.packages_to_remove.is_empty() {
        has_changes = true;
        println!("Packages to remove:");
        for pkg in &delta.packages_to_remove {
            println!("  - {pkg}");
        }
    }
    if !delta.dotfiles_to_link.is_empty() {
        has_changes = true;
        println!("Dotfiles to link:");
        for link in &delta.dotfiles_to_link {
            println!("  {} -> {}", link.source.display(), link.target.display());
        }
    }
    if !delta.env_changes.is_empty() {
        has_changes = true;
        println!("Environment variables:");
        for (key, val) in &delta.env_changes {
            println!("  {key}={val}");
        }
    }
    if !delta.services_to_enable.is_empty() {
        has_changes = true;
        println!("Services to enable:");
        for svc in &delta.services_to_enable {
            println!("  + {svc}");
        }
    }

    if !has_changes {
        println!("No changes detected.");
    }
    Ok(())
}

/// Show the current state vs the declared home.toml.
pub fn run_status() -> Result<()> {
    output::print_spinner("Checking home environment status...");

    let home_toml = default_home_toml();
    if !home_toml.exists() {
        output::print_warning("No home.toml found");
        return Ok(());
    }

    let config = HomeConfig::load(&home_toml)?;
    let config_dir = home_toml.parent().unwrap_or_else(|| std::path::Path::new("."));
    let home = home_dir();

    // TODO: read actual current packages from profile state
    let current_packages: Vec<String> = Vec::new();

    let status = compute_status(&config, config_dir, &home, &current_packages);

    if status.is_clean() {
        println!("Home environment is in sync.");
        return Ok(());
    }

    for drift in &status.package_drift {
        match drift {
            bpkg_home::PackageDrift::NotInstalled(pkg) => {
                println!("  package not installed: {pkg}");
            }
            bpkg_home::PackageDrift::NotInConfig(pkg) => {
                println!("  package not in config: {pkg}");
            }
            bpkg_home::PackageDrift::VersionMismatch { name, declared, installed } => {
                println!("  version mismatch: {name} (declared {declared}, installed {installed})");
            }
        }
    }

    for drift in &status.dotfile_drift {
        match drift {
            bpkg_home::DotfileDrift::NotLinked { source, target } => {
                println!("  dotfile not linked: {source} -> {target}");
            }
            bpkg_home::DotfileDrift::Modified { target } => {
                println!("  dotfile modified: {target}");
            }
            bpkg_home::DotfileDrift::BrokenLink { target } => {
                println!("  dotfile broken link: {target}");
            }
        }
    }

    for drift in &status.service_drift {
        match drift {
            bpkg_home::ServiceDrift::NotEnabled(svc) => {
                println!("  service not enabled: {svc}");
            }
        }
    }

    Ok(())
}
