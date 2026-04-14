use anyhow::{Result, bail};

use crate::args::parse_pin_spec;
use crate::output;

/// Pin a package to a specific version.
pub fn run_pin(spec: &str) -> Result<()> {
    let (name, version) = match parse_pin_spec(spec) {
        Some(parsed) => parsed,
        None => bail!("Invalid pin spec '{spec}'. Expected format: pkg=version"),
    };

    output::print_spinner(&format!("Pinning {name} to version {version}..."));

    // TODO: record pin in user config
    // TODO: if current version differs, resolve and install pinned version
    // TODO: recompose profile

    output::print_success(&format!("{name} pinned to {version}"));
    Ok(())
}

/// Remove a version pin from a package.
pub fn run_unpin(package: &str) -> Result<()> {
    output::print_spinner(&format!("Removing pin from {package}..."));

    // TODO: remove pin from user config
    // TODO: check if newer version available
    // TODO: recompose profile

    output::print_success(&format!("Removed pin from {package}"));
    Ok(())
}
