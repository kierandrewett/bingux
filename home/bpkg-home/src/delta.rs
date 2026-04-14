use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::config::{HomeConfig, PermissionSection};

/// A single dotfile symlink to create.
#[derive(Debug, Clone, PartialEq)]
pub struct DotfileLink {
    /// Source path, relative to the config directory.
    pub source: PathBuf,
    /// Target path, relative to `$HOME`.
    pub target: PathBuf,
}

/// The full set of changes needed to converge the home environment to the
/// desired state described in `home.toml`.
#[derive(Debug, Clone, Default)]
pub struct HomeDelta {
    pub packages_to_add: Vec<String>,
    pub packages_to_remove: Vec<String>,
    pub dotfiles_to_link: Vec<DotfileLink>,
    pub dotfiles_to_backup: Vec<PathBuf>,
    pub env_changes: HashMap<String, String>,
    pub services_to_enable: Vec<String>,
    pub services_to_disable: Vec<String>,
    pub dconf_changes: HashMap<String, String>,
    pub permissions_to_set: HashMap<String, PermissionSection>,
}

/// Compute the delta between the desired state (`home.toml`) and the current
/// system state.
///
/// - `config`: the parsed `HomeConfig`.
/// - `config_dir`: directory containing `home.toml` (dotfile sources are
///   resolved relative to this).
/// - `home_dir`: the user's `$HOME`.
/// - `current_packages`: names of packages currently installed as "kept".
pub fn compute_delta(
    config: &HomeConfig,
    config_dir: &Path,
    home_dir: &Path,
    current_packages: &[String],
) -> HomeDelta {
    let mut delta = HomeDelta::default();

    // ── Packages ──────────────────────────────────────────────────────
    if let Some(ref packages) = config.packages {
        // Packages to add: in keep list but not currently installed.
        // Strip version constraints for comparison (e.g. "firefox@128.0.1" → "firefox").
        for spec in &packages.keep {
            let base_name = package_base_name(spec);
            if !current_packages.iter().any(|p| package_base_name(p) == base_name) {
                delta.packages_to_add.push(spec.clone());
            }
        }

        // Packages to remove: in rm list AND currently installed.
        for spec in &packages.rm {
            let base_name = package_base_name(spec);
            if current_packages.iter().any(|p| package_base_name(p) == base_name) {
                delta.packages_to_remove.push(spec.clone());
            }
        }
    }

    // ── Dotfiles ──────────────────────────────────────────────────────
    if let Some(ref dotfiles) = config.dotfiles {
        for (source_rel, target_rel) in dotfiles {
            let source = PathBuf::from(source_rel);
            let target = PathBuf::from(target_rel);
            let target_abs = home_dir.join(&target);
            let source_abs = config_dir.join(&source);

            // Check if the target already points to our source.
            let needs_link = if target_abs.is_symlink() {
                match std::fs::read_link(&target_abs) {
                    Ok(link_target) => link_target != source_abs,
                    Err(_) => true,
                }
            } else {
                true
            };

            if needs_link {
                // If target exists and is NOT already our symlink, we need to
                // back it up before linking.
                if target_abs.exists() && !target_abs.is_symlink() {
                    delta.dotfiles_to_backup.push(target.clone());
                }
                delta.dotfiles_to_link.push(DotfileLink { source, target });
            }
        }
    }

    // ── Environment variables ─────────────────────────────────────────
    if let Some(ref env) = config.env {
        delta.env_changes = env.clone();
    }

    // ── Services ──────────────────────────────────────────────────────
    if let Some(ref services) = config.services {
        // For now we treat all listed services as needing to be enabled.
        // A more sophisticated implementation would query systemd state.
        delta.services_to_enable = services.enable.clone();
    }

    // ── dconf ─────────────────────────────────────────────────────────
    if let Some(ref dconf) = config.dconf {
        delta.dconf_changes = dconf.clone();
    }

    // ── Permissions ───────────────────────────────────────────────────
    if let Some(ref perms) = config.permissions {
        delta.permissions_to_set = perms.clone();
    }

    delta
}

/// Extract the base package name from a spec that may contain a version
/// constraint (e.g. `"firefox@128.0.1"` → `"firefox"`, `"@brave.brave-browser"` → `"@brave.brave-browser"`).
fn package_base_name(spec: &str) -> &str {
    // Scoped packages start with '@' (e.g. "@ghostty.ghostty").
    // The version separator is '@' that is NOT at position 0.
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
    fn package_to_add() {
        let config = HomeConfig::load_str(
            r#"
[packages]
keep = ["firefox", "git"]
"#,
        )
        .unwrap();

        let delta = compute_delta(&config, Path::new("/cfg"), Path::new("/home/u"), &[]);
        assert_eq!(delta.packages_to_add, vec!["firefox", "git"]);
        assert!(delta.packages_to_remove.is_empty());
    }

    #[test]
    fn package_already_installed_not_in_delta() {
        let config = HomeConfig::load_str(
            r#"
[packages]
keep = ["firefox", "git"]
"#,
        )
        .unwrap();

        let current = vec!["firefox".to_string()];
        let delta = compute_delta(&config, Path::new("/cfg"), Path::new("/home/u"), &current);
        assert_eq!(delta.packages_to_add, vec!["git"]);
    }

    #[test]
    fn package_to_remove() {
        let config = HomeConfig::load_str(
            r#"
[packages]
keep = []
rm = ["epiphany"]
"#,
        )
        .unwrap();

        let current = vec!["epiphany".to_string()];
        let delta = compute_delta(&config, Path::new("/cfg"), Path::new("/home/u"), &current);
        assert_eq!(delta.packages_to_remove, vec!["epiphany"]);
    }

    #[test]
    fn package_to_remove_not_installed() {
        let config = HomeConfig::load_str(
            r#"
[packages]
rm = ["epiphany"]
"#,
        )
        .unwrap();

        let delta = compute_delta(&config, Path::new("/cfg"), Path::new("/home/u"), &[]);
        assert!(delta.packages_to_remove.is_empty());
    }

    #[test]
    fn dotfile_to_link_not_yet_linked() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let cfg = dir.path().join("cfg");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&cfg).unwrap();

        let config = HomeConfig::load_str(
            r#"
[dotfiles]
"zsh/.zshrc" = ".zshrc"
"#,
        )
        .unwrap();

        let delta = compute_delta(&config, &cfg, &home, &[]);
        assert_eq!(delta.dotfiles_to_link.len(), 1);
        assert_eq!(
            delta.dotfiles_to_link[0],
            DotfileLink {
                source: PathBuf::from("zsh/.zshrc"),
                target: PathBuf::from(".zshrc"),
            }
        );
        assert!(delta.dotfiles_to_backup.is_empty());
    }

    #[test]
    fn dotfile_needs_backup_when_target_exists() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let cfg = dir.path().join("cfg");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&cfg).unwrap();

        // Create an existing regular file at the target location
        std::fs::write(home.join(".zshrc"), "# old config").unwrap();

        let config = HomeConfig::load_str(
            r#"
[dotfiles]
"zsh/.zshrc" = ".zshrc"
"#,
        )
        .unwrap();

        let delta = compute_delta(&config, &cfg, &home, &[]);
        assert_eq!(delta.dotfiles_to_link.len(), 1);
        assert_eq!(delta.dotfiles_to_backup, vec![PathBuf::from(".zshrc")]);
    }

    #[test]
    fn env_and_dconf_forwarded() {
        let config = HomeConfig::load_str(
            r#"
[env]
EDITOR = "nvim"

[dconf]
"org.gnome.desktop.interface.color-scheme" = "prefer-dark"
"#,
        )
        .unwrap();

        let delta = compute_delta(&config, Path::new("/cfg"), Path::new("/home/u"), &[]);
        assert_eq!(delta.env_changes.get("EDITOR").unwrap(), "nvim");
        assert_eq!(
            delta
                .dconf_changes
                .get("org.gnome.desktop.interface.color-scheme")
                .unwrap(),
            "prefer-dark"
        );
    }

    #[test]
    fn base_name_strips_version() {
        assert_eq!(package_base_name("firefox@128.0.1"), "firefox");
        assert_eq!(package_base_name("nodejs@^20"), "nodejs");
        assert_eq!(package_base_name("git"), "git");
        assert_eq!(
            package_base_name("@ghostty.ghostty"),
            "@ghostty.ghostty"
        );
        assert_eq!(
            package_base_name("@brave.brave-browser"),
            "@brave.brave-browser"
        );
    }
}
