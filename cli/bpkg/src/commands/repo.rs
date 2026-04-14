use anyhow::Result;

use crate::output;

/// List configured repositories.
pub fn run_list() -> Result<()> {
    // TODO: read repo config from user and system config

    println!("\x1b[1mSCOPE       URL\x1b[0m");
    println!("@bingux     (built-in)");
    println!("(no user repositories configured)");
    Ok(())
}

/// Add a user repository.
pub fn run_add(scope: &str, url: &str) -> Result<()> {
    output::print_spinner(&format!("Adding repository @{scope} -> {url}..."));

    // TODO: validate scope name
    // TODO: write to user repo config
    // TODO: fetch and validate repo index

    output::print_success(&format!("Added repository @{scope}"));
    Ok(())
}

/// Remove a user repository.
pub fn run_rm(scope: &str) -> Result<()> {
    output::print_spinner(&format!("Removing repository @{scope}..."));

    // TODO: remove from user repo config
    // TODO: optionally remove cached index

    output::print_success(&format!("Removed repository @{scope}"));
    Ok(())
}

/// Refresh repository indexes.
pub fn run_sync() -> Result<()> {
    output::print_spinner("Syncing repository indexes...");

    // TODO: for each configured repo, fetch latest index
    // TODO: update local cache

    output::print_success("Repository indexes up to date");
    Ok(())
}
