use std::path::PathBuf;

use crate::args::RepoAction;
use crate::output;

fn default_config_path() -> PathBuf {
    std::env::var("BSYS_CONFIG_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/system/config/system.toml"))
}

pub fn run(action: &RepoAction) {
    match action {
        RepoAction::Add { repo } => {
            output::status("repo", &format!("adding repository: {repo}"));
            // For now, print the TOML that should be added to system.toml
            output::status("repo", "add the following to system.toml:");
            println!(r#"
[[repos]]
scope = "{}"
url = "{}"
trusted = false
"#, repo.split('/').last().unwrap_or(repo), repo);
        }
        RepoAction::Rm { repo } => {
            output::status("repo", &format!("removing repository: {repo}"));
            output::status("repo", "remove the [[repos]] entry from system.toml");
        }
        RepoAction::Sync => {
            output::status("repo", "syncing repository indexes...");

            // Check for local repo index
            let index_path = std::env::var("BPKG_INDEX_PATH")
                .map(PathBuf::from)
                .unwrap_or_else(|_| {
                    let home = std::env::var("HOME").unwrap_or("/tmp".into());
                    PathBuf::from(home).join(".config/bingux/cache/index.toml")
                });

            if index_path.exists() {
                match bpkg_repo::RepoIndex::load(&index_path) {
                    Ok(idx) => {
                        output::status("repo", &format!(
                            "loaded @{} index: {} packages",
                            idx.meta.scope, idx.packages.len()
                        ));
                    }
                    Err(e) => output::status("error", &format!("failed to load index: {e}")),
                }
            } else {
                output::status("repo", "no cached index found");
                output::status("repo", "run `bsys export --index <dir>` to create a local repo");
            }
        }
    }
}
