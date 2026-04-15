use std::collections::HashMap;
use std::path::PathBuf;

use bingux_common::error::{BinguxError, Result};

use crate::delta::{DotfileLink, DotfilesRepoDelta, HomeDelta};

/// Summary of actions taken during an apply operation.
#[derive(Debug, Clone, Default)]
pub struct ApplySummary {
    pub packages_added: usize,
    pub packages_removed: usize,
    pub dotfiles_linked: usize,
    pub dotfiles_backed_up: usize,
    pub dotfiles_repo_cloned: bool,
    pub dotfiles_repo_updated: bool,
    pub env_vars_set: usize,
    pub shell_rc_written: bool,
    pub services_changed: usize,
    pub dconf_applied: usize,
}

/// Result of syncing a dotfiles repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DotfilesRepoAction {
    Cloned,
    Updated,
}

/// Engine that applies a [`HomeDelta`] to the filesystem.
pub struct ApplyEngine {
    home_dir: PathBuf,
    config_dir: PathBuf,
    backup_dir: PathBuf,
}

impl ApplyEngine {
    /// Create a new `ApplyEngine`.
    ///
    /// `backup_dir` is placed at `<config_dir>/backups`.
    pub fn new(home_dir: PathBuf, config_dir: PathBuf) -> Self {
        let backup_dir = config_dir.join("backups");
        Self {
            home_dir,
            config_dir,
            backup_dir,
        }
    }

    /// Apply the full delta. Returns a summary of actions taken.
    ///
    /// Package installation/removal is **not** performed here — that is the
    /// responsibility of the `bpkg` CLI which orchestrates the package
    /// manager.  This engine handles dotfiles, env, shell RC, dconf, and
    /// services.
    pub fn apply(&self, delta: &HomeDelta) -> Result<ApplySummary> {
        let mut summary = ApplySummary::default();

        // Record package counts (actual install/remove is done by bpkg).
        summary.packages_added = delta.packages_to_add.len();
        summary.packages_removed = delta.packages_to_remove.len();

        // Dotfiles repo
        if let Some(ref repo_delta) = delta.dotfiles_repo {
            let result = self.sync_dotfiles_repo(repo_delta)?;
            summary.dotfiles_repo_cloned = result == DotfilesRepoAction::Cloned;
            summary.dotfiles_repo_updated = result == DotfilesRepoAction::Updated;
        }

        // Dotfile symlinks
        let linked = self.link_dotfiles(&delta.dotfiles_to_link, &delta.dotfiles_to_backup)?;
        summary.dotfiles_linked = delta.dotfiles_to_link.len();
        summary.dotfiles_backed_up = linked.iter().filter(|m| m.starts_with("backed up")).count();

        // Environment
        if !delta.env_changes.is_empty() {
            self.generate_env_sh(&delta.env_changes)?;
            summary.env_vars_set = delta.env_changes.len();
        }

        // Shell RC
        if !delta.shell_rc.is_empty() {
            self.generate_shell_rc(&delta.shell_rc, delta.shell_name.as_deref())?;
            summary.shell_rc_written = true;
        }

        // dconf
        if !delta.dconf_changes.is_empty() {
            let applied = self.apply_dconf(&delta.dconf_changes)?;
            summary.dconf_applied = applied.len();
        }

        // Services
        if !delta.services_to_enable.is_empty() || !delta.services_to_disable.is_empty() {
            let managed =
                self.manage_services(&delta.services_to_enable, &delta.services_to_disable)?;
            summary.services_changed = managed.len();
        }

        Ok(summary)
    }

    /// Create symlinks for dotfiles. Backs up existing files first.
    ///
    /// Returns a list of human-readable messages describing what was done.
    pub fn link_dotfiles(
        &self,
        links: &[DotfileLink],
        backups: &[PathBuf],
    ) -> Result<Vec<String>> {
        let mut messages = Vec::new();

        // Ensure backup dir exists if needed.
        if !backups.is_empty() {
            std::fs::create_dir_all(&self.backup_dir)?;
        }

        // Back up existing files.
        for target_rel in backups {
            let target_abs = self.home_dir.join(target_rel);
            if target_abs.exists() {
                let backup_path = self.backup_dir.join(
                    target_rel
                        .file_name()
                        .unwrap_or(target_rel.as_os_str()),
                );
                std::fs::rename(&target_abs, &backup_path)?;
                messages.push(format!(
                    "backed up {} -> {}",
                    target_abs.display(),
                    backup_path.display()
                ));
            }
        }

        // Create symlinks.
        for link in links {
            let source_abs = self.config_dir.join(&link.source);
            let target_abs = self.home_dir.join(&link.target);

            // Ensure parent directory exists.
            if let Some(parent) = target_abs.parent() {
                std::fs::create_dir_all(parent)?;
            }

            // Remove existing symlink if present.
            if target_abs.is_symlink() {
                std::fs::remove_file(&target_abs)?;
            }

            #[cfg(unix)]
            std::os::unix::fs::symlink(&source_abs, &target_abs).map_err(|e| {
                BinguxError::Config {
                    path: target_abs.clone(),
                    message: format!("failed to create symlink: {e}"),
                }
            })?;

            #[cfg(not(unix))]
            {
                return Err(BinguxError::Config {
                    path: target_abs.clone(),
                    message: "symlinks not supported on this platform".into(),
                });
            }

            messages.push(format!(
                "linked {} -> {}",
                source_abs.display(),
                target_abs.display()
            ));
        }

        Ok(messages)
    }

    /// Generate `~/.config/bingux/env.sh` from environment variables.
    ///
    /// The generated file is meant to be sourced by the user's shell profile.
    /// Returns the path to the generated file.
    pub fn generate_env_sh(&self, env: &HashMap<String, String>) -> Result<PathBuf> {
        let env_dir = self.home_dir.join(".config").join("bingux");
        std::fs::create_dir_all(&env_dir)?;

        let env_path = env_dir.join("env.sh");
        let mut content = String::from("# Generated by bpkg-home — do not edit manually.\n");
        let mut keys: Vec<&String> = env.keys().collect();
        keys.sort();
        for key in keys {
            let value = &env[key];
            // Shell-quote the value with single quotes, escaping embedded single quotes.
            let escaped = value.replace('\'', "'\\''");
            content.push_str(&format!("export {key}='{escaped}'\n"));
        }

        std::fs::write(&env_path, &content)?;
        Ok(env_path)
    }

    /// Clone or update a dotfiles git repository.
    ///
    /// Returns whether a clone or update was performed.
    pub fn sync_dotfiles_repo(&self, repo: &DotfilesRepoDelta) -> Result<DotfilesRepoAction> {
        if repo.already_cloned {
            // Pull latest changes.
            let status = std::process::Command::new("git")
                .args(["pull", "--ff-only"])
                .current_dir(&repo.target)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .status()
                .map_err(|e| BinguxError::Config {
                    path: repo.target.clone(),
                    message: format!("failed to run git pull: {e}"),
                })?;
            if !status.success() {
                return Err(BinguxError::Config {
                    path: repo.target.clone(),
                    message: "git pull failed (non-zero exit)".into(),
                });
            }
            Ok(DotfilesRepoAction::Updated)
        } else {
            // Clone the repo.
            if let Some(parent) = repo.target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let status = std::process::Command::new("git")
                .args(["clone", &repo.url])
                .arg(&repo.target)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .status()
                .map_err(|e| BinguxError::Config {
                    path: repo.target.clone(),
                    message: format!("failed to run git clone: {e}"),
                })?;
            if !status.success() {
                return Err(BinguxError::Config {
                    path: repo.target.clone(),
                    message: "git clone failed (non-zero exit)".into(),
                });
            }
            Ok(DotfilesRepoAction::Cloned)
        }
    }

    /// Generate a shell RC snippet file that gets sourced by the user's
    /// shell profile.
    ///
    /// Writes `~/.config/bingux/shell.rc` which should be sourced from
    /// `.bashrc` / `.zshrc`. Also writes a source-line into the actual
    /// RC file if it is not already present.
    pub fn generate_shell_rc(
        &self,
        lines: &[String],
        shell_name: Option<&str>,
    ) -> Result<PathBuf> {
        let bingux_dir = self.home_dir.join(".config").join("bingux");
        std::fs::create_dir_all(&bingux_dir)?;

        let rc_path = bingux_dir.join("shell.rc");
        let mut content = String::from("# Generated by bpkg-home — do not edit manually.\n");
        for line in lines {
            content.push_str(line);
            content.push('\n');
        }
        std::fs::write(&rc_path, &content)?;

        // Ensure the user's actual shell RC file sources our snippet.
        let shell = shell_name.unwrap_or("bash");
        let rc_file = match shell {
            "zsh" => ".zshrc",
            "fish" => ".config/fish/config.fish",
            _ => ".bashrc",
        };
        let user_rc = self.home_dir.join(rc_file);
        let source_line = format!(
            "[ -f \"$HOME/.config/bingux/shell.rc\" ] && . \"$HOME/.config/bingux/shell.rc\""
        );

        // Read existing content to check if source line is already present.
        let existing = std::fs::read_to_string(&user_rc).unwrap_or_default();
        if !existing.contains(".config/bingux/shell.rc") {
            if let Some(parent) = user_rc.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut new_content = existing;
            if !new_content.is_empty() && !new_content.ends_with('\n') {
                new_content.push('\n');
            }
            new_content.push_str(&format!("\n# Bingux home environment\n{source_line}\n"));
            std::fs::write(&user_rc, &new_content)?;
        }

        Ok(rc_path)
    }

    /// Apply dconf settings by writing gsettings commands.
    ///
    /// Returns a list of key=value pairs that were applied.
    ///
    /// In a real implementation this would call `dconf write` or `gsettings set`.
    /// For now we write a script that can be executed.
    pub fn apply_dconf(&self, settings: &HashMap<String, String>) -> Result<Vec<String>> {
        let dconf_dir = self.home_dir.join(".config").join("bingux");
        std::fs::create_dir_all(&dconf_dir)?;

        let script_path = dconf_dir.join("dconf-apply.sh");
        let mut content =
            String::from("#!/bin/sh\n# Generated by bpkg-home — do not edit manually.\n");

        let mut applied = Vec::new();
        let mut keys: Vec<&String> = settings.keys().collect();
        keys.sort();
        for key in keys {
            let value = &settings[key];
            // Convert dotted key to dconf path:
            // "org.gnome.desktop.interface.color-scheme" →
            // schema "org.gnome.desktop.interface", key "color-scheme"
            if let Some(dot_pos) = key.rfind('.') {
                let schema = &key[..dot_pos];
                let dconf_key = &key[dot_pos + 1..];
                let escaped = value.replace('\'', "'\\''");
                content.push_str(&format!(
                    "gsettings set {schema} {dconf_key} '{escaped}'\n"
                ));
                applied.push(format!("{key}={value}"));
            }
        }

        std::fs::write(&script_path, &content)?;
        Ok(applied)
    }

    /// Enable/disable systemd user services.
    ///
    /// Returns a list of actions taken. In a real implementation this would
    /// call `systemctl --user enable/disable`. For now we record the intent.
    pub fn manage_services(
        &self,
        enable: &[String],
        disable: &[String],
    ) -> Result<Vec<String>> {
        let mut actions = Vec::new();
        for svc in enable {
            actions.push(format!("enable {svc}"));
        }
        for svc in disable {
            actions.push(format!("disable {svc}"));
        }
        Ok(actions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::Path;

    fn make_engine(dir: &Path) -> ApplyEngine {
        let home = dir.join("home");
        let cfg = dir.join("cfg");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&cfg).unwrap();
        ApplyEngine::new(home, cfg)
    }

    #[test]
    fn link_dotfiles_creates_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let engine = make_engine(dir.path());

        // Create the source file.
        let source_dir = engine.config_dir.join("zsh");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::write(source_dir.join(".zshrc"), "# zsh config").unwrap();

        let links = vec![DotfileLink {
            source: PathBuf::from("zsh/.zshrc"),
            target: PathBuf::from(".zshrc"),
        }];

        let msgs = engine.link_dotfiles(&links, &[]).unwrap();
        assert_eq!(msgs.len(), 1);
        assert!(msgs[0].contains("linked"));

        let target = engine.home_dir.join(".zshrc");
        assert!(target.is_symlink());
        let link_dest = std::fs::read_link(&target).unwrap();
        assert_eq!(link_dest, engine.config_dir.join("zsh/.zshrc"));
    }

    #[test]
    fn link_dotfiles_backs_up_existing() {
        let dir = tempfile::tempdir().unwrap();
        let engine = make_engine(dir.path());

        // Create existing file at target.
        std::fs::write(engine.home_dir.join(".zshrc"), "# old").unwrap();

        // Create source.
        let source_dir = engine.config_dir.join("zsh");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::write(source_dir.join(".zshrc"), "# new").unwrap();

        let links = vec![DotfileLink {
            source: PathBuf::from("zsh/.zshrc"),
            target: PathBuf::from(".zshrc"),
        }];
        let backups = vec![PathBuf::from(".zshrc")];

        let msgs = engine.link_dotfiles(&links, &backups).unwrap();
        assert!(msgs.iter().any(|m| m.starts_with("backed up")));
        assert!(msgs.iter().any(|m| m.starts_with("linked")));

        // Backup file should exist.
        let backup = engine.backup_dir.join(".zshrc");
        assert!(backup.exists());
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), "# old");

        // Target should be a symlink.
        assert!(engine.home_dir.join(".zshrc").is_symlink());
    }

    #[test]
    fn generate_env_sh_format() {
        let dir = tempfile::tempdir().unwrap();
        let engine = make_engine(dir.path());

        let mut env = HashMap::new();
        env.insert("EDITOR".into(), "nvim".into());
        env.insert("PAGER".into(), "bat --paging=always".into());

        let path = engine.generate_env_sh(&env).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();

        assert!(content.contains("# Generated by bpkg-home"));
        assert!(content.contains("export EDITOR='nvim'"));
        assert!(content.contains("export PAGER='bat --paging=always'"));
    }

    #[test]
    fn generate_env_sh_escapes_single_quotes() {
        let dir = tempfile::tempdir().unwrap();
        let engine = make_engine(dir.path());

        let mut env = HashMap::new();
        env.insert("GREETING".into(), "it's a test".into());

        let path = engine.generate_env_sh(&env).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();

        assert!(content.contains(r"export GREETING='it'\''s a test'"));
    }

    #[test]
    fn apply_dconf_generates_script() {
        let dir = tempfile::tempdir().unwrap();
        let engine = make_engine(dir.path());

        let mut settings = HashMap::new();
        settings.insert(
            "org.gnome.desktop.interface.color-scheme".into(),
            "prefer-dark".into(),
        );

        let applied = engine.apply_dconf(&settings).unwrap();
        assert_eq!(applied.len(), 1);

        let script = engine
            .home_dir
            .join(".config/bingux/dconf-apply.sh");
        let content = std::fs::read_to_string(&script).unwrap();
        assert!(content.contains(
            "gsettings set org.gnome.desktop.interface color-scheme 'prefer-dark'"
        ));
    }

    #[test]
    fn manage_services_records_actions() {
        let dir = tempfile::tempdir().unwrap();
        let engine = make_engine(dir.path());

        let actions = engine
            .manage_services(
                &["syncthing".into(), "ssh-agent".into()],
                &["tracker-miner".into()],
            )
            .unwrap();

        assert_eq!(
            actions,
            vec!["enable syncthing", "enable ssh-agent", "disable tracker-miner"]
        );
    }

    #[test]
    fn generate_shell_rc_creates_snippet() {
        let dir = tempfile::tempdir().unwrap();
        let engine = make_engine(dir.path());

        let lines = vec![
            r#"alias ll="ls -la""#.to_string(),
            r#"export PATH="$HOME/.local/bin:$PATH""#.to_string(),
        ];

        let path = engine.generate_shell_rc(&lines, Some("bash")).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();

        assert!(content.contains("# Generated by bpkg-home"));
        assert!(content.contains(r#"alias ll="ls -la""#));
        assert!(content.contains(r#"export PATH="$HOME/.local/bin:$PATH""#));
    }

    #[test]
    fn generate_shell_rc_sources_from_bashrc() {
        let dir = tempfile::tempdir().unwrap();
        let engine = make_engine(dir.path());

        let lines = vec!["alias ll='ls -la'".to_string()];
        engine.generate_shell_rc(&lines, Some("bash")).unwrap();

        let bashrc = std::fs::read_to_string(engine.home_dir.join(".bashrc")).unwrap();
        assert!(bashrc.contains(".config/bingux/shell.rc"));
    }

    #[test]
    fn generate_shell_rc_sources_from_zshrc() {
        let dir = tempfile::tempdir().unwrap();
        let engine = make_engine(dir.path());

        let lines = vec!["alias ll='ls -la'".to_string()];
        engine.generate_shell_rc(&lines, Some("zsh")).unwrap();

        let zshrc = std::fs::read_to_string(engine.home_dir.join(".zshrc")).unwrap();
        assert!(zshrc.contains(".config/bingux/shell.rc"));
    }

    #[test]
    fn generate_shell_rc_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let engine = make_engine(dir.path());

        let lines = vec!["alias ll='ls -la'".to_string()];
        engine.generate_shell_rc(&lines, Some("bash")).unwrap();
        engine.generate_shell_rc(&lines, Some("bash")).unwrap();

        let bashrc = std::fs::read_to_string(engine.home_dir.join(".bashrc")).unwrap();
        // The "# Bingux home environment" marker should appear exactly once.
        let count = bashrc.matches("# Bingux home environment").count();
        assert_eq!(count, 1);
    }
}
