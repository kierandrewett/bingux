use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::index::{RepoIndex, RepoPackage};

/// Configuration for a single package repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoConfig {
    /// The scope this repository provides (e.g. "bingux", "brave").
    pub scope: String,
    /// Base URL of the repository (e.g. "https://repo.bingux.org/stable").
    pub url: String,
    /// Optional GPG/minisign public key for signature verification.
    pub signing_key: Option<String>,
    /// Priority: lower numbers are preferred when the same package exists
    /// in multiple repositories.
    pub priority: u32,
    /// Whether this repository is trusted (skips signature checks).
    pub trusted: bool,
}

/// Resolve a package name across multiple repositories, picking the
/// highest-priority (lowest `priority` number) repository that has it.
///
/// Returns a clone of the matching `RepoConfig` and `RepoPackage`.
pub fn resolve_package(
    repos: &[RepoConfig],
    indexes: &HashMap<String, RepoIndex>,
    name: &str,
) -> Option<(RepoConfig, RepoPackage)> {
    let mut best: Option<(u32, RepoConfig, RepoPackage)> = None;

    for repo in repos {
        if let Some(index) = indexes.get(&repo.scope) {
            if let Some(pkg) = index.find(name) {
                let dominated = match &best {
                    Some((prio, _, _)) => repo.priority < *prio,
                    None => true,
                };
                if dominated {
                    best = Some((repo.priority, repo.clone(), pkg.clone()));
                }
            }
        }
    }

    best.map(|(_, config, pkg)| (config, pkg))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::{RepoIndex, RepoMeta, RepoPackage};

    fn make_repo(scope: &str, priority: u32, packages: Vec<RepoPackage>) -> (RepoConfig, RepoIndex) {
        let config = RepoConfig {
            scope: scope.to_string(),
            url: format!("https://repo.example.com/{scope}"),
            signing_key: None,
            priority,
            trusted: true,
        };
        let index = RepoIndex {
            meta: RepoMeta {
                scope: scope.to_string(),
                updated_at: String::new(),
                arch: vec!["x86_64-linux".to_string()],
            },
            packages,
        };
        (config, index)
    }

    fn make_pkg(name: &str, version: &str) -> RepoPackage {
        RepoPackage {
            name: name.to_string(),
            version: version.to_string(),
            arch: "x86_64-linux".to_string(),
            file: format!("{name}-{version}-x86_64-linux.bgx"),
            size: 1000,
            sha256: "deadbeef".to_string(),
            depends: Vec::new(),
            description: format!("Package {name}"),
        }
    }

    #[test]
    fn resolve_picks_highest_priority() {
        let (cfg_a, idx_a) = make_repo("community", 100, vec![make_pkg("firefox", "127.0")]);
        let (cfg_b, idx_b) = make_repo("bingux", 10, vec![make_pkg("firefox", "128.0")]);

        let repos = vec![cfg_a, cfg_b];
        let mut indexes = HashMap::new();
        indexes.insert("community".to_string(), idx_a);
        indexes.insert("bingux".to_string(), idx_b);

        let (config, pkg) = resolve_package(&repos, &indexes, "firefox").unwrap();
        assert_eq!(config.scope, "bingux");
        assert_eq!(pkg.version, "128.0");
    }

    #[test]
    fn resolve_returns_none_for_missing() {
        let (cfg, idx) = make_repo("bingux", 10, vec![make_pkg("firefox", "128.0")]);
        let repos = vec![cfg];
        let mut indexes = HashMap::new();
        indexes.insert("bingux".to_string(), idx);

        assert!(resolve_package(&repos, &indexes, "nonexistent").is_none());
    }

    #[test]
    fn resolve_with_single_repo() {
        let (cfg, idx) = make_repo("bingux", 10, vec![make_pkg("ripgrep", "14.1")]);
        let repos = vec![cfg];
        let mut indexes = HashMap::new();
        indexes.insert("bingux".to_string(), idx);

        let (config, pkg) = resolve_package(&repos, &indexes, "ripgrep").unwrap();
        assert_eq!(config.scope, "bingux");
        assert_eq!(pkg.name, "ripgrep");
    }
}
