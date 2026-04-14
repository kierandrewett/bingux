use std::path::PathBuf;

use anyhow::Result;

use bpkg_repo::RepoIndex;

use crate::output;

/// Default path for the local repository index cache.
fn default_index_path() -> PathBuf {
    std::env::var("BPKG_INDEX_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs_home().join(".config/bingux/cache/index.toml")
        })
}

fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/root"))
}

/// Search available packages in configured repositories.
pub fn run(query: &str) -> Result<()> {
    output::print_spinner(&format!("Searching for '{query}'..."));

    let index_path = default_index_path();

    if !index_path.exists() {
        output::print_warning("No repository index found. Run `bpkg repo sync` first.");
        return Ok(());
    }

    let index = RepoIndex::load(&index_path)
        .map_err(|e| anyhow::anyhow!("failed to load repo index: {e}"))?;

    let matches = index.search(query);

    let results: Vec<(String, String, String)> = matches
        .iter()
        .map(|pkg| {
            (
                pkg.name.clone(),
                pkg.version.clone(),
                pkg.description.clone(),
            )
        })
        .collect();

    output::print_search_results(&results);
    Ok(())
}
