use anyhow::Result;

use crate::output;

/// Show detailed information about a package.
pub fn run(package: &str) -> Result<()> {
    output::print_spinner(&format!("Looking up {package}..."));

    // TODO: look up package in store or repo index
    // TODO: if installed, read manifest from store
    // TODO: if not installed, fetch metadata from repo
    // For now, show stub data.

    output::print_package_info(
        package,
        "0.0.0",
        &format!("(stub info for {package})"),
        "unknown",
        "bingux",
        &[],
        &[],
    );
    Ok(())
}
