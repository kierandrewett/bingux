use std::collections::HashMap;
use std::path::Path;

use crate::config::{HomeConfig, PackagesSection, UserSection};

/// Generate a `HomeConfig` from the current system state.
///
/// This is the "init" command — it inspects what is currently installed and
/// produces a `home.toml` that captures the current environment so the user
/// can start managing it declaratively.
///
/// - `current_packages`: names of currently installed "kept" packages.
/// - `home_dir`: the user's `$HOME` (used to detect shell, editor, etc.).
pub fn generate_home_toml(current_packages: &[String], _home_dir: &Path) -> HomeConfig {
    let mut config = HomeConfig::default();

    // ── User section ──────────────────────────────────────────────────
    let mut user = UserSection::default();

    // Detect shell from $SHELL env var.
    if let Ok(shell) = std::env::var("SHELL") {
        if let Some(name) = Path::new(&shell).file_name() {
            user.shell = Some(name.to_string_lossy().into_owned());
        }
    }

    // Detect editor from $EDITOR or $VISUAL.
    if let Ok(editor) = std::env::var("EDITOR").or(std::env::var("VISUAL")) {
        if let Some(name) = Path::new(&editor).file_name() {
            user.editor = Some(name.to_string_lossy().into_owned());
        }
    }

    if user.shell.is_some() || user.editor.is_some() {
        config.user = Some(user);
    }

    // ── Packages section ──────────────────────────────────────────────
    if !current_packages.is_empty() {
        config.packages = Some(PackagesSection {
            keep: current_packages.to_vec(),
            rm: Vec::new(),
        });
    }

    // ── Environment ───────────────────────────────────────────────────
    // Capture common environment variables that users typically set.
    let interesting_vars = ["EDITOR", "VISUAL", "PAGER", "BROWSER", "TERMINAL"];
    let mut env_map = HashMap::new();
    for var in &interesting_vars {
        if let Ok(val) = std::env::var(var) {
            env_map.insert((*var).to_string(), val);
        }
    }
    if !env_map.is_empty() {
        config.env = Some(env_map);
    }

    config
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_current_packages() {
        let packages = vec![
            "firefox".to_string(),
            "git".to_string(),
            "ripgrep".to_string(),
        ];
        let config = generate_home_toml(&packages, Path::new("/home/test"));

        let pkgs = config.packages.as_ref().unwrap();
        assert_eq!(pkgs.keep, packages);
        assert!(pkgs.rm.is_empty());
    }

    #[test]
    fn empty_packages_produces_no_section() {
        let config = generate_home_toml(&[], Path::new("/home/test"));
        assert!(config.packages.is_none());
    }

    #[test]
    fn generated_config_is_valid_toml() {
        let packages = vec!["git".to_string(), "vim".to_string()];
        let config = generate_home_toml(&packages, Path::new("/home/test"));

        // Should round-trip through TOML without error.
        let serialized = toml::to_string_pretty(&config).unwrap();
        let reloaded = HomeConfig::load_str(&serialized).unwrap();
        assert_eq!(
            config.packages.as_ref().unwrap().keep,
            reloaded.packages.as_ref().unwrap().keep,
        );
    }
}
