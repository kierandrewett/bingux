use anyhow::Result;

use crate::output;

/// Promote a volatile package to persistent.
pub fn run_keep(package: &str) -> Result<()> {
    output::print_spinner(&format!("Promoting {package} to persistent..."));

    // TODO: move from volatile.toml to kept list
    // TODO: recompose profile

    output::print_success(&format!("{package} is now persistent (kept)"));
    Ok(())
}

/// Demote a persistent package to volatile.
pub fn run_unkeep(package: &str) -> Result<()> {
    output::print_spinner(&format!("Demoting {package} to volatile..."));

    // TODO: move from kept list to volatile.toml
    // TODO: recompose profile

    output::print_success(&format!(
        "{package} is now volatile (will disappear on reboot)"
    ));
    Ok(())
}
