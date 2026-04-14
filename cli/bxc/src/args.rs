use clap::{Parser, Subcommand};

/// bxc — BinguX sandbox runtime CLI
#[derive(Parser, Debug)]
#[command(name = "bxc", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug, PartialEq)]
pub enum Command {
    /// Run a package in a sandbox
    Run {
        /// Package name (optionally with @version)
        package: String,
        /// Extra arguments passed to the sandboxed process
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Open an interactive shell in a package sandbox
    Shell {
        /// Package name
        package: String,
    },
    /// Show sandbox configuration for a package
    Inspect {
        /// Package name
        package: String,
    },
    /// Show or manage permissions for a package
    Perms {
        /// Package name
        package: String,
        /// Reset all permissions to defaults
        #[arg(long)]
        reset: bool,
    },
    /// List running sandboxed processes
    Ps,
    /// List per-package home contents
    Ls {
        /// Package name
        package: String,
    },
    /// Show the computed mount set for a package sandbox
    Mounts {
        /// Package name
        package: String,
    },
}
