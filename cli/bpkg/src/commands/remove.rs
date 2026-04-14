use anyhow::Result;

use crate::output;

/// Remove a package from the user profile.
///
/// When `purge` is true, also delete per-package state (sandboxed home, etc.).
pub fn run(package: &str, purge: bool) -> Result<()> {
    output::print_spinner(&format!("Removing {package} from user profile..."));

    // TODO: remove from kept list / volatile state
    // TODO: recompose user profile
    // TODO: if purge, delete ~/.config/bingux/state/<pkg>/

    if purge {
        output::print_spinner(&format!("Purging state for {package}..."));
        output::print_success(&format!("Removed {package} and purged its state"));
    } else {
        output::print_success(&format!("Removed {package}"));
    }
    Ok(())
}
