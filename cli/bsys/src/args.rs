use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// bsys — BinguX system manager
#[derive(Parser, Debug)]
#[command(name = "bsys", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug, PartialEq)]
pub enum Command {
    /// Install a system package (volatile by default)
    Add {
        /// Package name
        package: String,
        /// Persist across reboots
        #[arg(long)]
        keep: bool,
    },
    /// Remove a system package
    Rm {
        /// Package name
        package: String,
    },
    /// Promote a volatile package to persistent
    Keep {
        /// Package name
        package: String,
    },
    /// Demote a persistent package to volatile
    Unkeep {
        /// Package name
        package: String,
        /// Force even for boot_essential packages
        #[arg(long)]
        force: bool,
    },
    /// Build a package from a BPKGBUILD recipe
    Build {
        /// Recipe name
        recipe: String,
    },
    /// Upgrade system packages
    Upgrade {
        /// Specific package to upgrade (omit for interactive selection)
        package: Option<String>,
        /// Upgrade all packages
        #[arg(long)]
        all: bool,
    },
    /// Recompose the system profile
    Apply,
    /// Roll back to a previous system generation
    Rollback {
        /// Generation number (defaults to previous)
        generation: Option<u64>,
    },
    /// List system generations
    History,
    /// Diff two system generations
    Diff {
        /// First generation
        gen1: u64,
        /// Second generation
        gen2: u64,
    },
    /// List installed system packages
    List,
    /// Show details for a system package
    Info {
        /// Package name
        package: String,
    },
    /// Pre-grant permissions for a system service
    Grant {
        /// Package name
        package: String,
        /// Permissions to grant
        #[arg(required = true)]
        perms: Vec<String>,
    },
    /// Revoke permissions for a system service
    Revoke {
        /// Package name
        package: String,
        /// Permissions to revoke
        #[arg(required = true)]
        perms: Vec<String>,
    },
    /// Garbage collect the package store
    Gc {
        /// Only show what would be removed
        #[arg(long)]
        dry_run: bool,
    },
    /// Export packages as .bgx archives
    Export {
        /// Package to export (omit when using --all or --index)
        package: Option<String>,
        /// Export all packages
        #[arg(long)]
        all: bool,
        /// Generate index.toml in directory
        #[arg(long)]
        index: Option<PathBuf>,
    },
    /// Manage package repositories
    Repo {
        #[command(subcommand)]
        action: RepoAction,
    },
    /// System-level home configuration convergence
    Home {
        #[command(subcommand)]
        action: HomeCommand,
    },
}

#[derive(Subcommand, Debug, PartialEq)]
pub enum RepoAction {
    /// Add a repository
    Add {
        /// Repository URL or name
        repo: String,
    },
    /// Remove a repository
    Rm {
        /// Repository URL or name
        repo: String,
    },
    /// Sync all repositories
    Sync,
}

#[derive(Subcommand, Debug, PartialEq)]
pub enum HomeCommand {
    /// Apply home configuration
    Apply {
        /// Path to configuration (optional)
        path: Option<PathBuf>,
    },
}
