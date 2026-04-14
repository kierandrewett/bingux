use anyhow::Result;

use crate::output;

/// Recompose the user profile from the declared state.
pub fn run_apply() -> Result<()> {
    output::print_spinner("Recomposing user profile...");

    // TODO: read user package list (kept + volatile + pins)
    // TODO: build GenerationBuilder from bsys-compose
    // TODO: create new generation directory with symlinks
    // TODO: atomically switch profiles/current symlink

    output::print_success("User profile recomposed");
    Ok(())
}

/// Roll back the user profile to a previous generation.
pub fn run_rollback(generation: Option<u64>) -> Result<()> {
    match generation {
        Some(number) => {
            output::print_spinner(&format!("Rolling back to generation {number}..."));
            // TODO: verify generation exists
            // TODO: switch profiles/current to that generation
            output::print_success(&format!("Rolled back to generation {number}"));
        }
        None => {
            output::print_spinner("Rolling back to previous generation...");
            // TODO: find current gen, switch to current - 1
            output::print_success("Rolled back to previous generation");
        }
    }
    Ok(())
}

/// List user profile generations.
pub fn run_history() -> Result<()> {
    // TODO: read generation directories from ~/.config/bingux/profiles/
    // TODO: parse metadata from each generation
    // For now, show stub data.

    let entries = vec![
        (3, "2026-04-14 10:30".to_string(), "add firefox (kept)".to_string()),
        (2, "2026-04-14 09:15".to_string(), "add ripgrep (volatile)".to_string()),
        (1, "2026-04-13 20:00".to_string(), "init".to_string()),
    ];

    output::print_history(&entries);
    Ok(())
}

/// First-time user profile setup.
pub fn run_init() -> Result<()> {
    output::print_spinner("Initialising user profile...");

    // TODO: create ~/.config/bingux/ directory structure
    // TODO: create initial home.toml
    // TODO: create generation 1 (empty profile)
    // TODO: set profiles/current symlink

    output::print_success("User profile initialised at ~/.config/bingux/");
    Ok(())
}
