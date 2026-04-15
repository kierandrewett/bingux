use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use bingux_common::error::{BinguxError, Result};

// ── Top-level config ──────────────────────────────────────────────────

/// The full declarative home environment configuration (`home.toml`).
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct HomeConfig {
    pub user: Option<UserSection>,
    pub packages: Option<PackagesSection>,
    pub repos: Option<Vec<RepoEntry>>,
    pub mounts: Option<MountsSection>,
    pub permissions: Option<HashMap<String, PermissionSection>>,
    pub dotfiles: Option<DotfilesSection>,
    pub env: Option<HashMap<String, String>>,
    pub shell: Option<ShellSection>,
    pub services: Option<ServicesSection>,
    pub dconf: Option<HashMap<String, String>>,
}

// ── Sub-sections ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct UserSection {
    pub name: Option<String>,
    pub shell: Option<String>,
    pub editor: Option<String>,
    pub terminal: Option<String>,
}

/// Dotfiles configuration — can specify a git repo to clone and/or
/// individual file mappings (source -> target relative to `$HOME`).
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct DotfilesSection {
    /// Git repository URL for dotfiles.
    pub repo: Option<String>,
    /// Target directory to clone into (relative to `$HOME`, defaults to `.dotfiles`).
    #[serde(default = "default_dotfiles_target")]
    pub target: String,
    /// Individual file mappings: source (relative to config dir) -> target (relative to `$HOME`).
    #[serde(default)]
    pub links: HashMap<String, String>,
}

fn default_dotfiles_target() -> String {
    ".dotfiles".to_string()
}

/// Shell RC configuration — lines to append to the user's shell RC file.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ShellSection {
    /// Lines to add to .bashrc / .zshrc (depending on `[user].shell`).
    #[serde(default)]
    pub rc: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct PackagesSection {
    #[serde(default)]
    pub keep: Vec<String>,
    #[serde(default)]
    pub rm: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RepoEntry {
    pub scope: String,
    pub url: String,
    #[serde(default)]
    pub trusted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct MountsSection {
    #[serde(default)]
    pub global: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct PermissionSection {
    #[serde(default)]
    pub allow: Vec<String>,
    #[serde(default)]
    pub deny: Vec<String>,
    #[serde(default)]
    pub mounts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ServicesSection {
    #[serde(default)]
    pub enable: Vec<String>,
}

// ── Implementation ────────────────────────────────────────────────────

impl HomeConfig {
    /// Load a `HomeConfig` from a TOML file on disk.
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).map_err(|e| BinguxError::Config {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;
        Self::load_str(&content)
    }

    /// Parse a `HomeConfig` from a TOML string.
    pub fn load_str(content: &str) -> Result<Self> {
        let config: Self = toml::from_str(content)?;
        Ok(config)
    }

    /// Serialize this config and write it to a TOML file.
    pub fn save(&self, path: &Path) -> Result<()> {
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Add a package to `[packages].keep`. Creates the section if needed.
    pub fn add_package(&mut self, name: &str) {
        let packages = self.packages.get_or_insert_with(PackagesSection::default);
        if !packages.keep.iter().any(|p| p == name) {
            packages.keep.push(name.to_string());
        }
    }

    /// Remove a package from `[packages].keep`. Returns `true` if it was present.
    pub fn remove_package(&mut self, name: &str) -> bool {
        if let Some(ref mut packages) = self.packages {
            if let Some(pos) = packages.keep.iter().position(|p| p == name) {
                packages.keep.remove(pos);
                return true;
            }
        }
        false
    }

    /// Check whether a package is in `[packages].keep`.
    pub fn has_package(&self, name: &str) -> bool {
        self.packages
            .as_ref()
            .is_some_and(|p| p.keep.iter().any(|pkg| pkg == name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL_TOML: &str = r#"
[user]
name = "kieran"
shell = "zsh"
editor = "nvim"
terminal = "ghostty"

[packages]
keep = [
    "zsh", "starship", "@ghostty.ghostty", "tmux",
    "neovim",
    "firefox@128.0.1", "@brave.brave-browser",
    "git", "rust", "nodejs@^20", "python@~3.12",
    "ripgrep", "fd", "bat", "jq", "delta", "lazygit",
    "spotify", "mpv", "discord",
]
rm = ["gnome-console", "epiphany"]

[[repos]]
scope = "brave"
url = "https://packages.brave.com/bingux"
trusted = true

[mounts]
global = [
    "~/Downloads:list",
    "~/Documents:list",
    "~/Pictures:list",
]

[permissions.firefox]
allow = ["gpu", "audio", "display", "net:outbound", "clipboard", "notifications", "dbus:session"]
deny = ["camera"]
mounts = ["~/Downloads:list,w", "~/.mozilla:rw"]

[permissions.neovim]
allow = ["display", "clipboard"]
mounts = ["~/src:rw", "~/Documents:rw", "~/.config/nvim:rw", "~/.ssh:list,deny(w)"]

[dotfiles]
repo = "https://github.com/kieran/dotfiles"
target = ".dotfiles"

[dotfiles.links]
"nvim/" = ".config/nvim"
"zsh/.zshrc" = ".zshrc"
"git/config" = ".gitconfig"

[env]
EDITOR = "nvim"
PAGER = "bat --paging=always"

[shell]
rc = [
    'alias ll="ls -la"',
    'export PATH="$HOME/.local/bin:$PATH"',
]

[services]
enable = ["syncthing", "ssh-agent"]

[dconf]
"org.gnome.desktop.interface.color-scheme" = "prefer-dark"
"org.gnome.desktop.interface.gtk-theme" = "adw-gtk3-dark"
"#;

    #[test]
    fn parse_full_home_toml() {
        let config = HomeConfig::load_str(FULL_TOML).unwrap();

        // user
        let user = config.user.as_ref().unwrap();
        assert_eq!(user.name.as_deref(), Some("kieran"));
        assert_eq!(user.shell.as_deref(), Some("zsh"));
        assert_eq!(user.editor.as_deref(), Some("nvim"));
        assert_eq!(user.terminal.as_deref(), Some("ghostty"));

        // packages
        let packages = config.packages.as_ref().unwrap();
        assert!(packages.keep.contains(&"firefox@128.0.1".to_string()));
        assert_eq!(packages.rm, vec!["gnome-console", "epiphany"]);

        // repos
        let repos = config.repos.as_ref().unwrap();
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].scope, "brave");
        assert!(repos[0].trusted);

        // mounts
        let mounts = config.mounts.as_ref().unwrap();
        assert_eq!(mounts.global.len(), 3);

        // permissions
        let perms = config.permissions.as_ref().unwrap();
        assert!(perms.contains_key("firefox"));
        assert!(perms["firefox"].deny.contains(&"camera".to_string()));
        assert!(perms.contains_key("neovim"));

        // dotfiles
        let dotfiles = config.dotfiles.as_ref().unwrap();
        assert_eq!(
            dotfiles.repo.as_deref(),
            Some("https://github.com/kieran/dotfiles")
        );
        assert_eq!(dotfiles.target, ".dotfiles");
        assert_eq!(dotfiles.links.get("zsh/.zshrc").unwrap(), ".zshrc");

        // env
        let env = config.env.as_ref().unwrap();
        assert_eq!(env.get("EDITOR").unwrap(), "nvim");

        // shell
        let shell = config.shell.as_ref().unwrap();
        assert_eq!(shell.rc.len(), 2);
        assert!(shell.rc[0].contains("alias ll"));

        // services
        let services = config.services.as_ref().unwrap();
        assert!(services.enable.contains(&"syncthing".to_string()));

        // dconf
        let dconf = config.dconf.as_ref().unwrap();
        assert_eq!(
            dconf
                .get("org.gnome.desktop.interface.color-scheme")
                .unwrap(),
            "prefer-dark"
        );
    }

    #[test]
    fn parse_minimal_home_toml() {
        let toml = r#"
[packages]
keep = ["firefox", "git"]
"#;
        let config = HomeConfig::load_str(toml).unwrap();
        assert!(config.user.is_none());
        assert!(config.dotfiles.is_none());
        assert!(config.shell.is_none());
        assert_eq!(config.packages.as_ref().unwrap().keep.len(), 2);
    }

    #[test]
    fn add_remove_has_package() {
        let mut config = HomeConfig::default();
        assert!(!config.has_package("vim"));

        config.add_package("vim");
        assert!(config.has_package("vim"));

        // Duplicate add is a no-op
        config.add_package("vim");
        assert_eq!(config.packages.as_ref().unwrap().keep.len(), 1);

        assert!(config.remove_package("vim"));
        assert!(!config.has_package("vim"));

        // Removing non-existent returns false
        assert!(!config.remove_package("vim"));
    }

    #[test]
    fn toml_roundtrip() {
        let original = HomeConfig::load_str(FULL_TOML).unwrap();
        let serialized = toml::to_string_pretty(&original).unwrap();
        let reloaded = HomeConfig::load_str(&serialized).unwrap();
        assert_eq!(original, reloaded);
    }

    #[test]
    fn load_and_save_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("home.toml");

        let mut config = HomeConfig::default();
        config.add_package("git");
        config
            .env
            .get_or_insert_with(HashMap::new)
            .insert("EDITOR".into(), "vim".into());

        config.save(&path).unwrap();
        let loaded = HomeConfig::load(&path).unwrap();
        assert_eq!(config, loaded);
    }
}
