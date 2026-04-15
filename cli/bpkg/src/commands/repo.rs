use std::path::PathBuf;

use anyhow::Result;

use bpkg_repo::RepoIndex;

use crate::output;

fn default_index_path() -> PathBuf {
    std::env::var("BPKG_INDEX_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or("/tmp".into());
            PathBuf::from(home).join(".config/bingux/cache/index.toml")
        })
}

/// List configured repositories.
pub fn run_list() -> Result<()> {
    println!("\x1b[1mSCOPE       STATUS\x1b[0m");
    println!("@bingux     built-in (official)");

    // Check for cached indexes
    let index_path = default_index_path();
    if index_path.exists() {
        if let Ok(idx) = RepoIndex::load(&index_path) {
            println!("            {} packages cached", idx.packages.len());
        }
    } else {
        println!("            (no cached index — run `bpkg repo sync`)");
    }

    Ok(())
}

/// Add a user repository.
pub fn run_add(scope: &str, url: &str) -> Result<()> {
    output::print_spinner(&format!("Adding repository @{scope} -> {url}..."));

    // Show what should be added to home.toml
    output::print_success(&format!("To add @{scope}, append to your home.toml:"));
    println!();
    println!("[[repos]]");
    println!("scope = \"{scope}\"");
    println!("url = \"{url}\"");
    println!("trusted = false");
    println!();
    output::print_warning("Then run `bpkg repo sync` to fetch the index");

    Ok(())
}

/// Remove a user repository.
pub fn run_rm(scope: &str) -> Result<()> {
    output::print_spinner(&format!("Removing repository @{scope}..."));

    if scope == "bingux" {
        output::print_error("Cannot remove the built-in @bingux repository");
        return Ok(());
    }

    output::print_success(&format!("Remove the [[repos]] entry for @{scope} from home.toml"));
    Ok(())
}

/// Refresh repository indexes.
pub fn run_sync() -> Result<()> {
    output::print_spinner("Syncing repository indexes...");

    let index_path = default_index_path();

    if index_path.exists() {
        match RepoIndex::load(&index_path) {
            Ok(idx) => {
                output::print_success(&format!(
                    "@{}: {} packages",
                    idx.meta.scope, idx.packages.len()
                ));
            }
            Err(e) => {
                output::print_error(&format!("Failed to load index: {e}"));
            }
        }
    } else {
        output::print_warning("No cached index found");
        output::print_warning("Create a local repo with: bsys export --all && bsys export --index <dir>");
        output::print_warning("Then set BPKG_INDEX_PATH to the index.toml path");
    }

    Ok(())
}
