use anyhow::{Context, Result};

use bpkg_repo::config_file::RepoConfigFile;
use bpkg_repo::RepoIndex;

use crate::output;

/// Search available packages across all configured repository indexes.
pub fn run(query: &str) -> Result<()> {
    output::print_spinner(&format!("Searching for '{query}'..."));

    let config = RepoConfigFile::load(&RepoConfigFile::default_path())
        .context("failed to load repos.toml")?;

    let cache_dir = RepoConfigFile::default_cache_dir();

    if config.repos.is_empty() {
        output::print_warning("No repositories configured. Run `bpkg repo add <name> <url>` first.");
        return Ok(());
    }

    let mut results: Vec<(String, String, String, String)> = Vec::new();
    let mut any_index_found = false;

    for repo in config.enabled() {
        let index_path = RepoConfigFile::cached_index_path(&cache_dir, &repo.name);

        if !index_path.exists() {
            continue;
        }
        any_index_found = true;

        let index = match RepoIndex::load(&index_path) {
            Ok(idx) => idx,
            Err(e) => {
                output::print_warning(&format!(
                    "Skipping '{}': failed to load cached index: {e}",
                    repo.name
                ));
                continue;
            }
        };

        let matches = index.search(query);
        for pkg in matches {
            results.push((
                pkg.name.clone(),
                pkg.version.clone(),
                pkg.description.clone(),
                repo.name.clone(),
            ));
        }
    }

    if !any_index_found {
        output::print_warning("No cached repository indexes found. Run `bpkg repo sync` first.");
        return Ok(());
    }

    // Deduplicate by name, keeping the first occurrence (from the first repo
    // that provides it).
    results.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.3.cmp(&b.3)));
    results.dedup_by(|a, b| a.0 == b.0);

    if results.is_empty() {
        println!("No packages found matching '{query}'.");
        return Ok(());
    }

    // Format for display: include repo name in output
    let display_results: Vec<(String, String, String)> = results
        .iter()
        .map(|(name, version, desc, repo)| {
            (
                format!("{name} ({repo})"),
                version.clone(),
                desc.clone(),
            )
        })
        .collect();

    output::print_search_results(&display_results);
    Ok(())
}

#[cfg(test)]
mod tests {
    use bpkg_repo::{RepoIndex, RepoMeta, RepoPackage};
    use bpkg_repo::config_file::RepoConfigFile;
    use std::fs;
    use tempfile::TempDir;

    fn make_test_index(scope: &str, packages: Vec<RepoPackage>) -> RepoIndex {
        RepoIndex {
            meta: RepoMeta {
                scope: scope.to_string(),
                updated_at: "2026-04-15T00:00:00Z".to_string(),
                arch: vec!["x86_64-linux".to_string()],
            },
            packages,
        }
    }

    fn make_pkg(name: &str, version: &str, desc: &str) -> RepoPackage {
        RepoPackage {
            name: name.to_string(),
            version: version.to_string(),
            arch: "x86_64-linux".to_string(),
            file: format!("{name}-{version}-x86_64-linux.bgx"),
            size: 1000,
            sha256: "deadbeef".to_string(),
            depends: vec![],
            description: desc.to_string(),
        }
    }

    #[test]
    fn search_across_multiple_repos() {
        let tmp = TempDir::new().unwrap();
        let cache_dir = tmp.path().join("cache");
        fs::create_dir_all(&cache_dir).unwrap();

        // Create two repo indexes
        let core_index = make_test_index(
            "core",
            vec![
                make_pkg("firefox", "128.0.1", "Mozilla Firefox web browser"),
                make_pkg("ripgrep", "14.1", "Fast line-oriented search tool"),
            ],
        );
        let community_index = make_test_index(
            "community",
            vec![
                make_pkg("brave", "1.67", "Brave web browser"),
            ],
        );

        core_index
            .save(&RepoConfigFile::cached_index_path(&cache_dir, "core"))
            .unwrap();
        community_index
            .save(&RepoConfigFile::cached_index_path(&cache_dir, "community"))
            .unwrap();

        // Search the core index directly
        let results = core_index.search("browser");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "firefox");

        // Search community index
        let results = community_index.search("browser");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "brave");

        // Combined search across both
        let mut all_results: Vec<&RepoPackage> = Vec::new();
        all_results.extend(core_index.search("browser"));
        all_results.extend(community_index.search("browser"));
        assert_eq!(all_results.len(), 2);
    }

    #[test]
    fn search_no_results() {
        let index = make_test_index(
            "core",
            vec![make_pkg("firefox", "128.0.1", "Mozilla Firefox web browser")],
        );
        let results = index.search("nonexistent-query-xyz");
        assert!(results.is_empty());
    }

    #[test]
    fn search_case_insensitive() {
        let index = make_test_index(
            "core",
            vec![make_pkg("Firefox", "128.0.1", "Mozilla Firefox Web Browser")],
        );
        let results = index.search("firefox");
        assert_eq!(results.len(), 1);
    }
}
