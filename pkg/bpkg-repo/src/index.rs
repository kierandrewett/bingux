use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::archive::{sha256_file, verify_bgx};
use crate::error::RepoError;

/// Repository index: the `index.toml` that lists all packages in a repo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoIndex {
    pub meta: RepoMeta,
    pub packages: Vec<RepoPackage>,
}

/// Repository-level metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoMeta {
    /// The scope this repository provides (e.g. "bingux", "brave").
    pub scope: String,
    /// ISO-8601 timestamp of the last index generation.
    pub updated_at: String,
    /// Architectures served by this repository.
    pub arch: Vec<String>,
}

/// A single package entry in a repository index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoPackage {
    pub name: String,
    pub version: String,
    pub arch: String,
    /// Filename of the `.bgx` archive.
    pub file: String,
    /// Size of the `.bgx` file in bytes.
    pub size: u64,
    /// SHA-256 hex digest of the `.bgx` file.
    pub sha256: String,
    /// Runtime dependency names.
    #[serde(default)]
    pub depends: Vec<String>,
    #[serde(default)]
    pub description: String,
}

impl RepoIndex {
    /// Parse an `index.toml` file from disk.
    pub fn load(path: &Path) -> Result<Self, RepoError> {
        let contents = fs::read_to_string(path).map_err(|e| {
            RepoError::IndexParse(format!(
                "failed to read {}: {e}",
                path.display()
            ))
        })?;
        let index: RepoIndex = toml::from_str(&contents)?;
        Ok(index)
    }

    /// Serialize and write this index to an `index.toml` file.
    pub fn save(&self, path: &Path) -> Result<(), RepoError> {
        let contents = toml::to_string_pretty(self)?;
        fs::write(path, contents)?;
        Ok(())
    }

    /// Generate an index from a directory of `.bgx` files.
    ///
    /// Scans `dir` for all `*.bgx` files, verifies each one, and builds
    /// a complete index with the given scope.
    pub fn generate_from_directory(dir: &Path, scope: &str) -> Result<Self, RepoError> {
        let mut packages = Vec::new();
        let mut arches = std::collections::BTreeSet::new();

        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|e| e.to_str()) != Some("bgx") {
                continue;
            }

            let info = verify_bgx(&path)?;
            let sha256 = sha256_file(&path)?;
            let file_size = fs::metadata(&path)?.len();

            let filename = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            arches.insert(info.arch.clone());

            packages.push(RepoPackage {
                name: info.name,
                version: info.version,
                arch: info.arch,
                file: filename,
                size: file_size,
                sha256,
                depends: Vec::new(),
                description: info.description,
            });
        }

        // Sort packages by name then version for deterministic output
        packages.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.version.cmp(&b.version)));

        Ok(RepoIndex {
            meta: RepoMeta {
                scope: scope.to_string(),
                updated_at: String::new(),
                arch: arches.into_iter().collect(),
            },
            packages,
        })
    }

    /// Search packages by query string (matches name or description, case-insensitive).
    pub fn search(&self, query: &str) -> Vec<&RepoPackage> {
        let query_lower = query.to_lowercase();
        self.packages
            .iter()
            .filter(|p| {
                p.name.to_lowercase().contains(&query_lower)
                    || p.description.to_lowercase().contains(&query_lower)
            })
            .collect()
    }

    /// Find a specific package by name (returns the first match).
    pub fn find(&self, name: &str) -> Option<&RepoPackage> {
        self.packages.iter().find(|p| p.name == name)
    }

    /// Find a specific package by name and version.
    pub fn find_version(&self, name: &str, version: &str) -> Option<&RepoPackage> {
        self.packages
            .iter()
            .find(|p| p.name == name && p.version == version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::create_bgx;
    use tempfile::TempDir;

    /// Build a minimal index for testing without needing .bgx files.
    fn sample_index() -> RepoIndex {
        RepoIndex {
            meta: RepoMeta {
                scope: "bingux".to_string(),
                updated_at: "2026-04-14T00:00:00Z".to_string(),
                arch: vec!["x86_64-linux".to_string()],
            },
            packages: vec![
                RepoPackage {
                    name: "firefox".to_string(),
                    version: "128.0.1".to_string(),
                    arch: "x86_64-linux".to_string(),
                    file: "firefox-128.0.1-x86_64-linux.bgx".to_string(),
                    size: 100_000,
                    sha256: "abc123".to_string(),
                    depends: vec!["glibc".to_string()],
                    description: "Mozilla Firefox web browser".to_string(),
                },
                RepoPackage {
                    name: "ripgrep".to_string(),
                    version: "14.1".to_string(),
                    arch: "x86_64-linux".to_string(),
                    file: "ripgrep-14.1-x86_64-linux.bgx".to_string(),
                    size: 5_000,
                    sha256: "def456".to_string(),
                    depends: Vec::new(),
                    description: "Fast line-oriented search tool".to_string(),
                },
            ],
        }
    }

    #[test]
    fn index_roundtrip_toml() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("index.toml");

        let index = sample_index();
        index.save(&path).unwrap();

        let loaded = RepoIndex::load(&path).unwrap();
        assert_eq!(loaded.meta.scope, "bingux");
        assert_eq!(loaded.packages.len(), 2);
        assert_eq!(loaded.packages[0].name, "firefox");
        assert_eq!(loaded.packages[1].name, "ripgrep");
    }

    #[test]
    fn search_by_name_substring() {
        let index = sample_index();

        let results = index.search("fire");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "firefox");
    }

    #[test]
    fn search_by_description() {
        let index = sample_index();

        let results = index.search("search tool");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "ripgrep");
    }

    #[test]
    fn search_case_insensitive() {
        let index = sample_index();

        let results = index.search("FIREFOX");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "firefox");
    }

    #[test]
    fn find_by_name() {
        let index = sample_index();

        assert!(index.find("firefox").is_some());
        assert_eq!(index.find("firefox").unwrap().version, "128.0.1");
        assert!(index.find("nonexistent").is_none());
    }

    #[test]
    fn find_by_name_and_version() {
        let index = sample_index();

        assert!(index.find_version("firefox", "128.0.1").is_some());
        assert!(index.find_version("firefox", "999.0").is_none());
        assert!(index.find_version("nonexistent", "1.0").is_none());
    }

    /// Helper to create a mock package directory.
    fn make_test_package(
        tmp: &Path,
        name: &str,
        version: &str,
    ) -> std::path::PathBuf {
        let pkg_name = format!("{name}-{version}-x86_64-linux");
        let pkg = tmp.join(&pkg_name);
        let meta = pkg.join(".bpkg");
        std::fs::create_dir_all(&meta).unwrap();

        let manifest = format!(
            r#"[package]
name = "{name}"
version = "{version}"
arch = "x86_64-linux"
description = "Test {name}"
"#
        );
        std::fs::write(meta.join("manifest.toml"), manifest).unwrap();

        let bin = pkg.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join(name), "#!/bin/sh\necho hello\n").unwrap();

        pkg
    }

    #[test]
    fn generate_from_directory_with_two_bgx() {
        let tmp = TempDir::new().unwrap();
        let repo_dir = tmp.path().join("repo");
        std::fs::create_dir_all(&repo_dir).unwrap();

        // Create two packages and archive them into the repo dir
        let pkg1 = make_test_package(tmp.path(), "alpha", "1.0");
        let pkg2 = make_test_package(tmp.path(), "beta", "2.0");

        create_bgx(&pkg1, &repo_dir.join("alpha-1.0-x86_64-linux.bgx")).unwrap();
        create_bgx(&pkg2, &repo_dir.join("beta-2.0-x86_64-linux.bgx")).unwrap();

        let index = RepoIndex::generate_from_directory(&repo_dir, "test").unwrap();

        assert_eq!(index.meta.scope, "test");
        assert_eq!(index.meta.arch, vec!["x86_64-linux".to_string()]);
        assert_eq!(index.packages.len(), 2);

        // Sorted by name
        assert_eq!(index.packages[0].name, "alpha");
        assert_eq!(index.packages[1].name, "beta");

        // SHA-256 should be non-empty hex
        assert!(!index.packages[0].sha256.is_empty());
        assert!(index.packages[0].size > 0);
    }
}
