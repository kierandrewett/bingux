use anyhow::Result;

use crate::output;

/// Pre-grant permissions to a package.
pub fn run_grant(package: &str, permissions: &[String]) -> Result<()> {
    let perms = permissions.join(", ");
    output::print_spinner(&format!("Granting [{perms}] to {package}..."));

    // TODO: write to ~/.config/bingux/permissions/<pkg>.toml
    // TODO: these pre-grants mean the sandbox won't prompt at runtime

    output::print_success(&format!("Granted [{perms}] to {package}"));
    Ok(())
}

/// Revoke permissions from a package.
pub fn run_revoke(package: &str, permissions: &[String]) -> Result<()> {
    let perms = permissions.join(", ");
    output::print_spinner(&format!("Revoking [{perms}] from {package}..."));

    // TODO: update ~/.config/bingux/permissions/<pkg>.toml
    // TODO: next launch will prompt again for these permissions

    output::print_success(&format!("Revoked [{perms}] from {package}"));
    Ok(())
}
