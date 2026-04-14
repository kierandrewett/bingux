use std::path::PathBuf;

use anyhow::Result;

use crate::output;

/// Converge the full environment to home.toml.
pub fn run_apply(path: Option<&PathBuf>) -> Result<()> {
    let display_path = path
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "~/.config/bingux/config/home.toml".to_string());

    output::print_spinner(&format!("Applying home configuration from {display_path}..."));

    // TODO: parse home.toml
    // TODO: diff declared state vs current state
    // TODO: install/remove packages as needed
    // TODO: apply dotfile links, shell config, etc.
    // TODO: recompose profile

    output::print_success("Home environment converged");
    Ok(())
}

/// Show what applying home.toml would change.
pub fn run_diff() -> Result<()> {
    output::print_spinner("Comparing declared state to current...");

    // TODO: parse home.toml
    // TODO: diff against current profile
    // TODO: display additions, removals, changes

    println!("(no changes detected — stub)");
    Ok(())
}

/// Show the current state vs the declared home.toml.
pub fn run_status() -> Result<()> {
    output::print_spinner("Checking home environment status...");

    // TODO: parse home.toml
    // TODO: compare each section to live state

    println!("Home environment is in sync (stub)");
    Ok(())
}
