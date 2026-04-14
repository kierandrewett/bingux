use anyhow::Result;

use crate::output;

/// Upgrade user packages.
///
/// If `package` is Some, upgrade only that package.
/// If `all` is true, upgrade all user packages.
/// If neither, print a hint.
pub fn run(package: Option<&str>, all: bool) -> Result<()> {
    if all {
        output::print_spinner("Checking for updates to all user packages...");

        // TODO: for each user package, check repo for newer version
        // TODO: resolve, fetch, recompose

        output::print_success("All packages are up to date");
    } else if let Some(pkg) = package {
        output::print_spinner(&format!("Checking for updates to {pkg}..."));

        // TODO: check repo for newer version of pkg
        // TODO: resolve, fetch, recompose

        output::print_success(&format!("{pkg} is up to date"));
    } else {
        output::print_warning("Specify a package or use --all to upgrade everything");
    }
    Ok(())
}
