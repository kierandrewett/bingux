use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::RepoError;

/// A single repository entry in `repos.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RepoEntry {
    /// Human-readable name for this repository (e.g. "core", "community").
    pub name: String,
    /// Base URL of the repository.
    pub url: String,
    /// Whether this repository is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// The on-disk `repos.toml` config file.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RepoConfigFile {
    /// List of configured repositories.
    #[serde(default)]
    pub repos: Vec<RepoEntry>,
}

impl RepoConfigFile {
    /// Default system path for the repo config.
    pub fn default_path() -> PathBuf {
        std::env::var("BPKG_REPOS_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/system/config/repos.toml"))
    }

    /// Default path for the local index cache directory.
    pub fn default_cache_dir() -> PathBuf {
        std::env::var("BPKG_CACHE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
                PathBuf::from(home).join(".cache/bingux/repos")
            })
    }

    /// Load `repos.toml` from a path. Returns an empty config if the file
    /// does not exist.
    pub fn load(path: &Path) -> Result<Self, RepoError> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let contents = fs::read_to_string(path).map_err(|e| {
            RepoError::Io(e)
        })?;
        let config: RepoConfigFile = toml::from_str(&contents)?;
        Ok(config)
    }

    /// Write this config back to disk, creating parent directories if needed.
    pub fn save(&self, path: &Path) -> Result<(), RepoError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let contents = toml::to_string_pretty(self)?;
        fs::write(path, contents)?;
        Ok(())
    }

    /// Add a new repository. Errors if a repo with the same name already exists.
    pub fn add(&mut self, name: &str, url: &str) -> Result<(), RepoError> {
        if self.repos.iter().any(|r| r.name == name) {
            return Err(RepoError::RepoAlreadyExists(name.to_string()));
        }
        self.repos.push(RepoEntry {
            name: name.to_string(),
            url: url.to_string(),
            enabled: true,
        });
        Ok(())
    }

    /// Remove a repository by name. Errors if not found.
    pub fn remove(&mut self, name: &str) -> Result<RepoEntry, RepoError> {
        let idx = self
            .repos
            .iter()
            .position(|r| r.name == name)
            .ok_or_else(|| RepoError::RepoNotFound(name.to_string()))?;
        Ok(self.repos.remove(idx))
    }

    /// Find a repo by name.
    pub fn find(&self, name: &str) -> Option<&RepoEntry> {
        self.repos.iter().find(|r| r.name == name)
    }

    /// List all enabled repositories.
    pub fn enabled(&self) -> Vec<&RepoEntry> {
        self.repos.iter().filter(|r| r.enabled).collect()
    }

    /// Build the URL for a repo's `index.toml`.
    pub fn index_url(repo: &RepoEntry) -> String {
        let base = repo.url.trim_end_matches('/');
        format!("{base}/index.toml")
    }

    /// Path to the cached index file for a given repo name.
    pub fn cached_index_path(cache_dir: &Path, repo_name: &str) -> PathBuf {
        cache_dir.join(format!("{repo_name}.index.toml"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn roundtrip_empty_config() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("repos.toml");

        let config = RepoConfigFile::default();
        config.save(&path).unwrap();

        let loaded = RepoConfigFile::load(&path).unwrap();
        assert!(loaded.repos.is_empty());
    }

    #[test]
    fn add_and_save_repo() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("repos.toml");

        let mut config = RepoConfigFile::default();
        config.add("core", "https://repo.bingux.dev/core").unwrap();
        config.add("community", "https://repo.bingux.dev/community").unwrap();
        config.save(&path).unwrap();

        let loaded = RepoConfigFile::load(&path).unwrap();
        assert_eq!(loaded.repos.len(), 2);
        assert_eq!(loaded.repos[0].name, "core");
        assert_eq!(loaded.repos[0].url, "https://repo.bingux.dev/core");
        assert!(loaded.repos[0].enabled);
        assert_eq!(loaded.repos[1].name, "community");
    }

    #[test]
    fn add_duplicate_fails() {
        let mut config = RepoConfigFile::default();
        config.add("core", "https://repo.bingux.dev/core").unwrap();
        let err = config.add("core", "https://other.dev/core").unwrap_err();
        assert!(matches!(err, RepoError::RepoAlreadyExists(_)));
    }

    #[test]
    fn remove_repo() {
        let mut config = RepoConfigFile::default();
        config.add("core", "https://repo.bingux.dev/core").unwrap();
        config.add("community", "https://repo.bingux.dev/community").unwrap();

        let removed = config.remove("core").unwrap();
        assert_eq!(removed.name, "core");
        assert_eq!(config.repos.len(), 1);
        assert_eq!(config.repos[0].name, "community");
    }

    #[test]
    fn remove_nonexistent_fails() {
        let mut config = RepoConfigFile::default();
        let err = config.remove("nonexistent").unwrap_err();
        assert!(matches!(err, RepoError::RepoNotFound(_)));
    }

    #[test]
    fn find_repo() {
        let mut config = RepoConfigFile::default();
        config.add("core", "https://repo.bingux.dev/core").unwrap();

        assert!(config.find("core").is_some());
        assert!(config.find("missing").is_none());
    }

    #[test]
    fn enabled_filters_disabled() {
        let mut config = RepoConfigFile::default();
        config.add("core", "https://repo.bingux.dev/core").unwrap();
        config.add("test", "https://repo.bingux.dev/test").unwrap();
        config.repos[1].enabled = false;

        let enabled = config.enabled();
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].name, "core");
    }

    #[test]
    fn load_missing_file_returns_empty() {
        let config = RepoConfigFile::load(Path::new("/nonexistent/repos.toml")).unwrap();
        assert!(config.repos.is_empty());
    }

    #[test]
    fn index_url_construction() {
        let entry = RepoEntry {
            name: "core".to_string(),
            url: "https://repo.bingux.dev/core".to_string(),
            enabled: true,
        };
        assert_eq!(
            RepoConfigFile::index_url(&entry),
            "https://repo.bingux.dev/core/index.toml"
        );

        let entry_trailing = RepoEntry {
            name: "core".to_string(),
            url: "https://repo.bingux.dev/core/".to_string(),
            enabled: true,
        };
        assert_eq!(
            RepoConfigFile::index_url(&entry_trailing),
            "https://repo.bingux.dev/core/index.toml"
        );
    }

    #[test]
    fn cached_index_path_construction() {
        let cache = Path::new("/tmp/cache");
        assert_eq!(
            RepoConfigFile::cached_index_path(cache, "core"),
            PathBuf::from("/tmp/cache/core.index.toml")
        );
    }

    #[test]
    fn toml_format_matches_spec() {
        let mut config = RepoConfigFile::default();
        config.add("core", "https://repo.bingux.dev/core").unwrap();

        let toml_str = toml::to_string_pretty(&config).unwrap();
        // Should contain [[repos]] table array syntax
        assert!(toml_str.contains("[[repos]]"));
        assert!(toml_str.contains("name = \"core\""));
        assert!(toml_str.contains("url = \"https://repo.bingux.dev/core\""));
        assert!(toml_str.contains("enabled = true"));
    }
}
