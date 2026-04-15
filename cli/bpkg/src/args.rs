use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "bpkg",
    about = "Bingux user package manager",
    version,
    arg_required_else_help = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug, PartialEq)]
pub enum Command {
    /// Install a package (volatile by default, --keep for persistent)
    Add {
        /// Package name or @scope.name
        package: String,
        /// Make the install persistent across reboots
        #[arg(long)]
        keep: bool,
    },
    /// Remove a package from the user profile
    Rm {
        /// Package name or @scope.name
        package: String,
        /// Also delete per-package state (sandboxed home, etc.)
        #[arg(long)]
        purge: bool,
    },
    /// Promote a volatile package to persistent
    Keep {
        /// Package name or @scope.name
        package: String,
    },
    /// Demote a persistent package to volatile
    Unkeep {
        /// Package name or @scope.name
        package: String,
    },
    /// Pin a package to a specific version (format: pkg=version)
    Pin {
        /// Package pin spec, e.g. "firefox=128.0.1"
        spec: String,
    },
    /// Remove a version pin from a package
    Unpin {
        /// Package name or @scope.name
        package: String,
    },
    /// Upgrade packages
    Upgrade {
        /// Specific package to upgrade (omit for interactive selection)
        package: Option<String>,
        /// Upgrade all user packages
        #[arg(long)]
        all: bool,
    },
    /// List installed user packages
    List,
    /// Search available packages
    Search {
        /// Search query
        query: String,
    },
    /// Show package details
    Info {
        /// Package name or @scope.name
        package: String,
    },
    /// Pre-grant permissions to a package
    Grant {
        /// Package name or @scope.name
        package: String,
        /// Permissions to grant (e.g. gpu audio network)
        #[arg(required = true)]
        permissions: Vec<String>,
    },
    /// Revoke permissions from a package
    Revoke {
        /// Package name or @scope.name
        package: String,
        /// Permissions to revoke
        #[arg(required = true)]
        permissions: Vec<String>,
    },
    /// Recompose the user profile from declared state
    Apply,
    /// Roll back the user profile to a previous generation
    Rollback {
        /// Generation number (defaults to previous)
        generation: Option<u64>,
    },
    /// List user profile generations
    History,
    /// First-time user profile setup
    Init,
    /// Manage home environment (home.toml)
    Home {
        #[command(subcommand)]
        action: HomeCommand,
    },
    /// Manage package repositories
    Repo {
        #[command(subcommand)]
        action: RepoCommand,
    },
}

#[derive(Subcommand, Debug, PartialEq)]
pub enum HomeCommand {
    /// Converge full environment to home.toml
    Apply {
        /// Path to home.toml (default: ~/.config/bingux/config/home.toml)
        path: Option<PathBuf>,
    },
    /// Show what would change
    Diff,
    /// Show current state vs declared
    Status,
}

#[derive(Subcommand, Debug, PartialEq)]
pub enum RepoCommand {
    /// List configured repositories
    List,
    /// Add a repository
    Add {
        /// Repository name (e.g. "core", "community")
        name: String,
        /// Repository URL (e.g. "https://repo.bingux.dev/core")
        url: String,
    },
    /// Remove a repository
    Rm {
        /// Repository name
        name: String,
    },
    /// Download and refresh repository indexes
    Sync,
}

/// Parse a pin spec like "firefox=128.0.1" into (name, version).
pub fn parse_pin_spec(spec: &str) -> Option<(&str, &str)> {
    let eq = spec.find('=')?;
    let name = &spec[..eq];
    let version = &spec[eq + 1..];
    if name.is_empty() || version.is_empty() {
        return None;
    }
    Some((name, version))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_pin_spec_valid() {
        let (name, version) = parse_pin_spec("firefox=128.0.1").unwrap();
        assert_eq!(name, "firefox");
        assert_eq!(version, "128.0.1");
    }

    #[test]
    fn parse_pin_spec_no_equals() {
        assert!(parse_pin_spec("firefox").is_none());
    }

    #[test]
    fn parse_pin_spec_empty_name() {
        assert!(parse_pin_spec("=128.0.1").is_none());
    }

    #[test]
    fn parse_pin_spec_empty_version() {
        assert!(parse_pin_spec("firefox=").is_none());
    }
}
