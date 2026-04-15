use std::path::Path;

use crate::config::HomeConfig;

/// Drift detected for a single package.
#[derive(Debug, Clone, PartialEq)]
pub enum PackageDrift {
    /// Package is declared in home.toml but not currently installed.
    NotInstalled(String),
    /// Package is installed but not declared in home.toml.
    NotInConfig(String),
    /// Package is installed with a different version than declared.
    VersionMismatch {
        name: String,
        declared: String,
        installed: String,
    },
}

/// Drift detected for a single dotfile.
#[derive(Debug, Clone, PartialEq)]
pub enum DotfileDrift {
    /// Dotfile is declared but no symlink exists at the target.
    NotLinked { source: String, target: String },
    /// Symlink exists but points to a different location.
    Modified { target: String },
    /// Symlink exists but the source file is missing.
    BrokenLink { target: String },
}

/// Drift detected for a service.
#[derive(Debug, Clone, PartialEq)]
pub enum ServiceDrift {
    /// Service is declared as enabled but is not running/enabled.
    NotEnabled(String),
}

/// Drift detected for the shell RC configuration.
#[derive(Debug, Clone, PartialEq)]
pub enum ShellDrift {
    /// The managed shell.rc file does not exist yet.
    NotGenerated,
    /// The shell.rc content does not match the declared lines.
    OutOfDate,
    /// The user's shell RC file does not source our snippet.
    NotSourced { rc_file: String },
}

/// Drift detected for the dotfiles repository.
#[derive(Debug, Clone, PartialEq)]
pub enum DotfilesRepoDrift {
    /// The repo has not been cloned yet.
    NotCloned { url: String, target: String },
}

/// Overall status of the home environment relative to `home.toml`.
#[derive(Debug, Clone, Default)]
pub struct HomeStatus {
    pub package_drift: Vec<PackageDrift>,
    pub dotfile_drift: Vec<DotfileDrift>,
    pub shell_drift: Vec<ShellDrift>,
    pub dotfiles_repo_drift: Vec<DotfilesRepoDrift>,
    pub service_drift: Vec<ServiceDrift>,
}

impl HomeStatus {
    /// Returns `true` if everything is in sync — no drift detected.
    pub fn is_clean(&self) -> bool {
        self.package_drift.is_empty()
            && self.dotfile_drift.is_empty()
            && self.shell_drift.is_empty()
            && self.dotfiles_repo_drift.is_empty()
            && self.service_drift.is_empty()
    }
}

/// Compute the status (drift) between the desired `home.toml` config and the
/// current system state.
///
/// - `current_packages`: list of currently installed "kept" package names (may
///   include version, e.g. `"firefox"` or `"firefox@128.0.1"`).
pub fn compute_status(
    config: &HomeConfig,
    config_dir: &Path,
    home_dir: &Path,
    current_packages: &[String],
) -> HomeStatus {
    let mut status = HomeStatus::default();

    // ── Package drift ─────────────────────────────────────────────────
    if let Some(ref packages) = config.packages {
        let current_base_names: Vec<&str> = current_packages
            .iter()
            .map(|p| package_base_name(p))
            .collect();

        let declared_base_names: Vec<&str> = packages
            .keep
            .iter()
            .map(|p| package_base_name(p))
            .collect();

        // Declared but not installed.
        for spec in &packages.keep {
            let base = package_base_name(spec);
            if !current_base_names.contains(&base) {
                status
                    .package_drift
                    .push(PackageDrift::NotInstalled(spec.clone()));
            }
        }

        // Installed but not declared.
        for pkg in current_packages {
            let base = package_base_name(pkg);
            if !declared_base_names.contains(&base) {
                status
                    .package_drift
                    .push(PackageDrift::NotInConfig(pkg.clone()));
            }
        }
    }

    // ── Dotfile drift ─────────────────────────────────────────────────
    if let Some(ref dotfiles) = config.dotfiles {
        // Check dotfiles repo.
        if let Some(ref repo_url) = dotfiles.repo {
            let target_dir = home_dir.join(&dotfiles.target);
            if !target_dir.join(".git").is_dir() {
                status.dotfiles_repo_drift.push(DotfilesRepoDrift::NotCloned {
                    url: repo_url.clone(),
                    target: dotfiles.target.clone(),
                });
            }
        }

        // Check individual file links.
        for (source_rel, target_rel) in &dotfiles.links {
            let target_abs = home_dir.join(target_rel);
            let source_abs = config_dir.join(source_rel);

            if target_abs.is_symlink() {
                match std::fs::read_link(&target_abs) {
                    Ok(link_dest) => {
                        if !source_abs.exists() {
                            status.dotfile_drift.push(DotfileDrift::BrokenLink {
                                target: target_rel.clone(),
                            });
                        } else if link_dest != source_abs {
                            status.dotfile_drift.push(DotfileDrift::Modified {
                                target: target_rel.clone(),
                            });
                        }
                        // else: symlink is correct, no drift.
                    }
                    Err(_) => {
                        status.dotfile_drift.push(DotfileDrift::BrokenLink {
                            target: target_rel.clone(),
                        });
                    }
                }
            } else {
                status.dotfile_drift.push(DotfileDrift::NotLinked {
                    source: source_rel.clone(),
                    target: target_rel.clone(),
                });
            }
        }
    }

    // ── Shell RC drift ───────────────────────────────────────────────
    if let Some(ref shell) = config.shell {
        if !shell.rc.is_empty() {
            let bingux_rc = home_dir.join(".config/bingux/shell.rc");
            if !bingux_rc.exists() {
                status.shell_drift.push(ShellDrift::NotGenerated);
            } else {
                // Check if content matches.
                let existing = std::fs::read_to_string(&bingux_rc).unwrap_or_default();
                let mut expected = String::from("# Generated by bpkg-home — do not edit manually.\n");
                for line in &shell.rc {
                    expected.push_str(line);
                    expected.push('\n');
                }
                if existing != expected {
                    status.shell_drift.push(ShellDrift::OutOfDate);
                }
            }

            // Check if the user's RC file sources our snippet.
            let shell_name = config
                .user
                .as_ref()
                .and_then(|u| u.shell.as_deref())
                .unwrap_or("bash");
            let rc_file = match shell_name {
                "zsh" => ".zshrc",
                "fish" => ".config/fish/config.fish",
                _ => ".bashrc",
            };
            let user_rc = home_dir.join(rc_file);
            let content = std::fs::read_to_string(&user_rc).unwrap_or_default();
            if !content.contains(".config/bingux/shell.rc") {
                status.shell_drift.push(ShellDrift::NotSourced {
                    rc_file: rc_file.to_string(),
                });
            }
        }
    }

    status
}

/// Extract the base package name, stripping any `@version` suffix.
/// Scoped packages (starting with `@`) keep their scope intact.
fn package_base_name(spec: &str) -> &str {
    if let Some(pos) = spec[1..].find('@') {
        &spec[..pos + 1]
    } else {
        spec
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::HomeConfig;

    #[test]
    fn detects_not_installed_packages() {
        let config = HomeConfig::load_str(
            r#"
[packages]
keep = ["firefox", "git", "ripgrep"]
"#,
        )
        .unwrap();

        let current = vec!["git".to_string()];
        let status = compute_status(&config, Path::new("/cfg"), Path::new("/home/u"), &current);

        let not_installed: Vec<_> = status
            .package_drift
            .iter()
            .filter_map(|d| match d {
                PackageDrift::NotInstalled(name) => Some(name.as_str()),
                _ => None,
            })
            .collect();

        assert_eq!(not_installed, vec!["firefox", "ripgrep"]);
    }

    #[test]
    fn detects_not_in_config_packages() {
        let config = HomeConfig::load_str(
            r#"
[packages]
keep = ["git"]
"#,
        )
        .unwrap();

        let current = vec!["git".to_string(), "vim".to_string()];
        let status = compute_status(&config, Path::new("/cfg"), Path::new("/home/u"), &current);

        let extra: Vec<_> = status
            .package_drift
            .iter()
            .filter_map(|d| match d {
                PackageDrift::NotInConfig(name) => Some(name.as_str()),
                _ => None,
            })
            .collect();

        assert_eq!(extra, vec!["vim"]);
    }

    #[test]
    fn detects_not_linked_dotfile() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let cfg = dir.path().join("cfg");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&cfg).unwrap();

        let config = HomeConfig::load_str(
            r#"
[dotfiles]
[dotfiles.links]
"zsh/.zshrc" = ".zshrc"
"#,
        )
        .unwrap();

        let status = compute_status(&config, &cfg, &home, &[]);
        assert_eq!(status.dotfile_drift.len(), 1);
        assert!(matches!(
            &status.dotfile_drift[0],
            DotfileDrift::NotLinked { source, target }
            if source == "zsh/.zshrc" && target == ".zshrc"
        ));
    }

    #[test]
    fn detects_broken_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let cfg = dir.path().join("cfg");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&cfg).unwrap();

        // Create a symlink to a non-existent source.
        let target = home.join(".zshrc");
        let source = cfg.join("zsh/.zshrc");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&source, &target).unwrap();

        let config = HomeConfig::load_str(
            r#"
[dotfiles]
[dotfiles.links]
"zsh/.zshrc" = ".zshrc"
"#,
        )
        .unwrap();

        let status = compute_status(&config, &cfg, &home, &[]);
        assert_eq!(status.dotfile_drift.len(), 1);
        assert!(matches!(
            &status.dotfile_drift[0],
            DotfileDrift::BrokenLink { target } if target == ".zshrc"
        ));
    }

    #[test]
    fn clean_status_when_everything_matches() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let cfg = dir.path().join("cfg");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&cfg).unwrap();

        // Create source and a correct symlink.
        let source = cfg.join("zsh/.zshrc");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(&source, "# config").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&source, home.join(".zshrc")).unwrap();

        let config = HomeConfig::load_str(
            r#"
[packages]
keep = ["git"]

[dotfiles]
[dotfiles.links]
"zsh/.zshrc" = ".zshrc"
"#,
        )
        .unwrap();

        let current = vec!["git".to_string()];
        let status = compute_status(&config, &cfg, &home, &current);
        assert!(status.is_clean());
    }

    #[test]
    fn detects_dotfiles_repo_not_cloned() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let cfg = dir.path().join("cfg");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&cfg).unwrap();

        let config = HomeConfig::load_str(
            r#"
[dotfiles]
repo = "https://github.com/kieran/dotfiles"
"#,
        )
        .unwrap();

        let status = compute_status(&config, &cfg, &home, &[]);
        assert_eq!(status.dotfiles_repo_drift.len(), 1);
        assert!(matches!(
            &status.dotfiles_repo_drift[0],
            DotfilesRepoDrift::NotCloned { url, .. }
            if url == "https://github.com/kieran/dotfiles"
        ));
    }

    #[test]
    fn detects_shell_rc_not_generated() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let cfg = dir.path().join("cfg");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&cfg).unwrap();

        let config = HomeConfig::load_str(
            r#"
[user]
shell = "bash"

[shell]
rc = ['alias ll="ls -la"']
"#,
        )
        .unwrap();

        let status = compute_status(&config, &cfg, &home, &[]);
        assert!(status.shell_drift.iter().any(|d| matches!(d, ShellDrift::NotGenerated)));
        assert!(status.shell_drift.iter().any(|d| matches!(d, ShellDrift::NotSourced { .. })));
    }
}
