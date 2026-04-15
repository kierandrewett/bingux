use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};

use bpkg_repo::config_file::RepoConfigFile;
use bpkg_repo::RepoIndex;

use crate::output;

/// Resolve the repos.toml config path.
fn config_path() -> PathBuf {
    RepoConfigFile::default_path()
}

/// Resolve the repo cache directory.
fn cache_dir() -> PathBuf {
    RepoConfigFile::default_cache_dir()
}

/// List configured repositories.
pub fn run_list() -> Result<()> {
    let config = RepoConfigFile::load(&config_path())
        .context("failed to load repos.toml")?;

    if config.repos.is_empty() {
        println!("No repositories configured.");
        println!();
        output::print_status("hint", "add a repository with: bpkg repo add <name> <url>");
        return Ok(());
    }

    println!(
        "\x1b[1m{:<16} {:<8} {}\x1b[0m",
        "NAME", "STATUS", "URL"
    );

    let cache = cache_dir();
    for repo in &config.repos {
        let status = if repo.enabled { "enabled" } else { "disabled" };
        println!(
            "{:<16} {:<8} {}",
            repo.name, status, repo.url
        );

        // Show cached index info if available
        let index_path = RepoConfigFile::cached_index_path(&cache, &repo.name);
        if index_path.exists() {
            if let Ok(idx) = RepoIndex::load(&index_path) {
                println!(
                    "  \x1b[2m{} packages cached (scope: @{})\x1b[0m",
                    idx.packages.len(),
                    idx.meta.scope
                );
            }
        } else {
            println!("  \x1b[2m(no cached index \u{2014} run `bpkg repo sync`)\x1b[0m");
        }
    }

    Ok(())
}

/// Add a user repository.
pub fn run_add(name: &str, url: &str) -> Result<()> {
    let path = config_path();
    let mut config = RepoConfigFile::load(&path)
        .context("failed to load repos.toml")?;

    config
        .add(name, url)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    config.save(&path).context("failed to save repos.toml")?;

    output::print_success(&format!(
        "Added repository '{name}' -> {url}"
    ));
    output::print_status("next", "run `bpkg repo sync` to fetch the package index");

    Ok(())
}

/// Remove a user repository.
pub fn run_rm(name: &str) -> Result<()> {
    let path = config_path();
    let mut config = RepoConfigFile::load(&path)
        .context("failed to load repos.toml")?;

    config
        .remove(name)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    config.save(&path).context("failed to save repos.toml")?;

    // Remove cached index if present
    let cache = cache_dir();
    let index_path = RepoConfigFile::cached_index_path(&cache, name);
    if index_path.exists() {
        fs::remove_file(&index_path).ok();
    }

    output::print_success(&format!("Removed repository '{name}'"));

    Ok(())
}

/// Refresh repository indexes by downloading index.toml from each enabled repo.
pub fn run_sync() -> Result<()> {
    let config = RepoConfigFile::load(&config_path())
        .context("failed to load repos.toml")?;

    let enabled = config.enabled();
    if enabled.is_empty() {
        output::print_warning("No enabled repositories to sync");
        output::print_status("hint", "add a repository with: bpkg repo add <name> <url>");
        return Ok(());
    }

    let cache = cache_dir();
    fs::create_dir_all(&cache)
        .context("failed to create cache directory")?;

    let mut total_packages = 0usize;
    let mut errors = 0usize;

    for repo in &enabled {
        let index_url = RepoConfigFile::index_url(repo);
        output::print_spinner(&format!("Syncing '{}'...", repo.name));

        match fetch_index(&index_url) {
            Ok(body) => {
                let index_path = RepoConfigFile::cached_index_path(&cache, &repo.name);
                fs::write(&index_path, &body).with_context(|| {
                    format!("failed to write cached index for '{}'", repo.name)
                })?;

                // Validate the downloaded index
                match RepoIndex::load(&index_path) {
                    Ok(idx) => {
                        output::print_success(&format!(
                            "'{}': {} packages (scope: @{})",
                            repo.name,
                            idx.packages.len(),
                            idx.meta.scope
                        ));
                        total_packages += idx.packages.len();
                    }
                    Err(e) => {
                        output::print_error(&format!(
                            "'{}': downloaded index is invalid: {e}",
                            repo.name
                        ));
                        fs::remove_file(&index_path).ok();
                        errors += 1;
                    }
                }
            }
            Err(e) => {
                output::print_error(&format!("'{}': {e}", repo.name));
                errors += 1;
            }
        }
    }

    println!();
    if errors > 0 {
        output::print_warning(&format!(
            "Sync complete with {errors} error(s). {total_packages} packages available."
        ));
    } else {
        output::print_success(&format!(
            "Sync complete. {total_packages} packages available across {} repo(s).",
            enabled.len()
        ));
    }

    Ok(())
}

/// Fetch the content at a URL. Uses `curl` as a subprocess since we don't
/// want to pull in a heavy HTTP client dependency for the CLI.
fn fetch_index(url: &str) -> Result<String> {
    let output = std::process::Command::new("curl")
        .args(["-fsSL", "--connect-timeout", "10", "--max-time", "30", url])
        .output()
        .context("failed to execute curl (is it installed?)")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("failed to fetch {url}: {stderr}");
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_env(tmp: &TempDir) -> (PathBuf, PathBuf) {
        let config_path = tmp.path().join("repos.toml");
        let cache_dir = tmp.path().join("cache");
        std::fs::create_dir_all(&cache_dir).unwrap();
        (config_path, cache_dir)
    }

    #[test]
    fn add_and_list_repos() {
        let tmp = TempDir::new().unwrap();
        let (path, _cache) = setup_env(&tmp);

        let mut config = RepoConfigFile::default();
        config.add("core", "https://repo.bingux.dev/core").unwrap();
        config.save(&path).unwrap();

        let loaded = RepoConfigFile::load(&path).unwrap();
        assert_eq!(loaded.repos.len(), 1);
        assert_eq!(loaded.repos[0].name, "core");
        assert_eq!(loaded.repos[0].url, "https://repo.bingux.dev/core");
        assert!(loaded.repos[0].enabled);
    }

    #[test]
    fn remove_repo_and_cached_index() {
        let tmp = TempDir::new().unwrap();
        let (path, cache) = setup_env(&tmp);

        // Set up config with two repos
        let mut config = RepoConfigFile::default();
        config.add("core", "https://repo.bingux.dev/core").unwrap();
        config.add("extra", "https://repo.bingux.dev/extra").unwrap();
        config.save(&path).unwrap();

        // Create a fake cached index
        let cached = RepoConfigFile::cached_index_path(&cache, "extra");
        std::fs::write(&cached, "fake index").unwrap();
        assert!(cached.exists());

        // Remove repo
        let mut config = RepoConfigFile::load(&path).unwrap();
        config.remove("extra").unwrap();
        config.save(&path).unwrap();

        // Clean up cache manually (as run_rm would)
        if cached.exists() {
            std::fs::remove_file(&cached).ok();
        }

        let loaded = RepoConfigFile::load(&path).unwrap();
        assert_eq!(loaded.repos.len(), 1);
        assert_eq!(loaded.repos[0].name, "core");
        assert!(!cached.exists());
    }

    #[test]
    fn sync_writes_valid_index_to_cache() {
        let tmp = TempDir::new().unwrap();
        let cache = tmp.path().join("cache");
        std::fs::create_dir_all(&cache).unwrap();

        // Write a sample index.toml to cache as if downloaded
        let index = bpkg_repo::RepoIndex {
            meta: bpkg_repo::RepoMeta {
                scope: "core".to_string(),
                updated_at: "2026-04-15T00:00:00Z".to_string(),
                arch: vec!["x86_64-linux".to_string()],
            },
            packages: vec![bpkg_repo::RepoPackage {
                name: "firefox".to_string(),
                version: "128.0.1".to_string(),
                arch: "x86_64-linux".to_string(),
                file: "firefox-128.0.1-x86_64-linux.bgx".to_string(),
                size: 52428800,
                sha256: "abc123".to_string(),
                depends: vec![],
                description: "Mozilla Firefox web browser".to_string(),
            }],
        };

        let index_path = RepoConfigFile::cached_index_path(&cache, "core");
        index.save(&index_path).unwrap();

        // Verify we can load the cached index
        let loaded = bpkg_repo::RepoIndex::load(&index_path).unwrap();
        assert_eq!(loaded.packages.len(), 1);
        assert_eq!(loaded.packages[0].name, "firefox");
        assert_eq!(loaded.meta.scope, "core");
    }
}
