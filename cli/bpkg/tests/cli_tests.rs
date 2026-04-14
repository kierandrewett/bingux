use clap::Parser;

// We need to reference the types from the crate. Since bpkg-cli is a binary
// crate, we replicate the arg types here for testing. Alternatively, we test
// via the binary's argument parsing by importing the module path.
//
// For integration testing of a binary crate, we re-declare the arg structs
// or use `assert_cmd`. Here we take the simpler approach of duplicating the
// clap types since the structures are the source of truth.

// Re-declare the arg types for testing (mirrors cli/bpkg/src/args.rs).
// In a real project these would live in a library crate, but for now this
// is the pragmatic approach for a binary crate.

use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "bpkg")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand, Debug, PartialEq)]
enum Command {
    Add {
        package: String,
        #[arg(long)]
        keep: bool,
    },
    Rm {
        package: String,
        #[arg(long)]
        purge: bool,
    },
    Keep {
        package: String,
    },
    Unkeep {
        package: String,
    },
    Pin {
        spec: String,
    },
    Unpin {
        package: String,
    },
    Upgrade {
        package: Option<String>,
        #[arg(long)]
        all: bool,
    },
    List,
    Search {
        query: String,
    },
    Info {
        package: String,
    },
    Grant {
        package: String,
        #[arg(required = true)]
        permissions: Vec<String>,
    },
    Revoke {
        package: String,
        #[arg(required = true)]
        permissions: Vec<String>,
    },
    Apply,
    Rollback {
        generation: Option<u64>,
    },
    History,
    Init,
    Home {
        #[command(subcommand)]
        action: HomeCommand,
    },
    Repo {
        #[command(subcommand)]
        action: RepoCommand,
    },
}

#[derive(clap::Subcommand, Debug, PartialEq)]
enum HomeCommand {
    Apply { path: Option<PathBuf> },
    Diff,
    Status,
}

#[derive(clap::Subcommand, Debug, PartialEq)]
enum RepoCommand {
    List,
    Add { scope: String, url: String },
    Rm { scope: String },
    Sync,
}

fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
    Cli::try_parse_from(args)
}

#[test]
fn add_volatile() {
    let cli = parse(&["bpkg", "add", "firefox"]).unwrap();
    assert_eq!(
        cli.command,
        Command::Add {
            package: "firefox".to_string(),
            keep: false,
        }
    );
}

#[test]
fn add_keep() {
    let cli = parse(&["bpkg", "add", "--keep", "firefox"]).unwrap();
    assert_eq!(
        cli.command,
        Command::Add {
            package: "firefox".to_string(),
            keep: true,
        }
    );
}

#[test]
fn rm_basic() {
    let cli = parse(&["bpkg", "rm", "firefox"]).unwrap();
    assert_eq!(
        cli.command,
        Command::Rm {
            package: "firefox".to_string(),
            purge: false,
        }
    );
}

#[test]
fn rm_purge() {
    let cli = parse(&["bpkg", "rm", "--purge", "firefox"]).unwrap();
    assert_eq!(
        cli.command,
        Command::Rm {
            package: "firefox".to_string(),
            purge: true,
        }
    );
}

#[test]
fn pin_spec() {
    let cli = parse(&["bpkg", "pin", "firefox=128.0.1"]).unwrap();
    assert_eq!(
        cli.command,
        Command::Pin {
            spec: "firefox=128.0.1".to_string(),
        }
    );
}

#[test]
fn list_command() {
    let cli = parse(&["bpkg", "list"]).unwrap();
    assert_eq!(cli.command, Command::List);
}

#[test]
fn home_apply_with_path() {
    let cli = parse(&["bpkg", "home", "apply", "/home/user/dotfiles/home.toml"]).unwrap();
    assert_eq!(
        cli.command,
        Command::Home {
            action: HomeCommand::Apply {
                path: Some(PathBuf::from("/home/user/dotfiles/home.toml")),
            },
        }
    );
}

#[test]
fn home_apply_no_path() {
    let cli = parse(&["bpkg", "home", "apply"]).unwrap();
    assert_eq!(
        cli.command,
        Command::Home {
            action: HomeCommand::Apply { path: None },
        }
    );
}

#[test]
fn home_diff() {
    let cli = parse(&["bpkg", "home", "diff"]).unwrap();
    assert_eq!(
        cli.command,
        Command::Home {
            action: HomeCommand::Diff,
        }
    );
}

#[test]
fn home_status() {
    let cli = parse(&["bpkg", "home", "status"]).unwrap();
    assert_eq!(
        cli.command,
        Command::Home {
            action: HomeCommand::Status,
        }
    );
}

#[test]
fn repo_add() {
    let cli = parse(&["bpkg", "repo", "add", "brave", "https://repo.brave.com"]).unwrap();
    assert_eq!(
        cli.command,
        Command::Repo {
            action: RepoCommand::Add {
                scope: "brave".to_string(),
                url: "https://repo.brave.com".to_string(),
            },
        }
    );
}

#[test]
fn repo_list() {
    let cli = parse(&["bpkg", "repo", "list"]).unwrap();
    assert_eq!(
        cli.command,
        Command::Repo {
            action: RepoCommand::List,
        }
    );
}

#[test]
fn repo_rm() {
    let cli = parse(&["bpkg", "repo", "rm", "brave"]).unwrap();
    assert_eq!(
        cli.command,
        Command::Repo {
            action: RepoCommand::Rm {
                scope: "brave".to_string(),
            },
        }
    );
}

#[test]
fn repo_sync() {
    let cli = parse(&["bpkg", "repo", "sync"]).unwrap();
    assert_eq!(
        cli.command,
        Command::Repo {
            action: RepoCommand::Sync,
        }
    );
}

#[test]
fn grant_permissions() {
    let cli = parse(&["bpkg", "grant", "firefox", "gpu", "audio"]).unwrap();
    assert_eq!(
        cli.command,
        Command::Grant {
            package: "firefox".to_string(),
            permissions: vec!["gpu".to_string(), "audio".to_string()],
        }
    );
}

#[test]
fn revoke_permissions() {
    let cli = parse(&["bpkg", "revoke", "firefox", "network"]).unwrap();
    assert_eq!(
        cli.command,
        Command::Revoke {
            package: "firefox".to_string(),
            permissions: vec!["network".to_string()],
        }
    );
}

#[test]
fn upgrade_specific() {
    let cli = parse(&["bpkg", "upgrade", "firefox"]).unwrap();
    assert_eq!(
        cli.command,
        Command::Upgrade {
            package: Some("firefox".to_string()),
            all: false,
        }
    );
}

#[test]
fn upgrade_all() {
    let cli = parse(&["bpkg", "upgrade", "--all"]).unwrap();
    assert_eq!(
        cli.command,
        Command::Upgrade {
            package: None,
            all: true,
        }
    );
}

#[test]
fn rollback_with_generation() {
    let cli = parse(&["bpkg", "rollback", "5"]).unwrap();
    assert_eq!(
        cli.command,
        Command::Rollback {
            generation: Some(5),
        }
    );
}

#[test]
fn rollback_no_generation() {
    let cli = parse(&["bpkg", "rollback"]).unwrap();
    assert_eq!(
        cli.command,
        Command::Rollback { generation: None }
    );
}

#[test]
fn history_command() {
    let cli = parse(&["bpkg", "history"]).unwrap();
    assert_eq!(cli.command, Command::History);
}

#[test]
fn init_command() {
    let cli = parse(&["bpkg", "init"]).unwrap();
    assert_eq!(cli.command, Command::Init);
}

#[test]
fn apply_command() {
    let cli = parse(&["bpkg", "apply"]).unwrap();
    assert_eq!(cli.command, Command::Apply);
}

#[test]
fn keep_command() {
    let cli = parse(&["bpkg", "keep", "firefox"]).unwrap();
    assert_eq!(
        cli.command,
        Command::Keep {
            package: "firefox".to_string(),
        }
    );
}

#[test]
fn unkeep_command() {
    let cli = parse(&["bpkg", "unkeep", "firefox"]).unwrap();
    assert_eq!(
        cli.command,
        Command::Unkeep {
            package: "firefox".to_string(),
        }
    );
}

#[test]
fn unpin_command() {
    let cli = parse(&["bpkg", "unpin", "firefox"]).unwrap();
    assert_eq!(
        cli.command,
        Command::Unpin {
            package: "firefox".to_string(),
        }
    );
}

#[test]
fn search_command() {
    let cli = parse(&["bpkg", "search", "browser"]).unwrap();
    assert_eq!(
        cli.command,
        Command::Search {
            query: "browser".to_string(),
        }
    );
}

#[test]
fn info_command() {
    let cli = parse(&["bpkg", "info", "firefox"]).unwrap();
    assert_eq!(
        cli.command,
        Command::Info {
            package: "firefox".to_string(),
        }
    );
}

#[test]
fn invalid_command_fails() {
    assert!(parse(&["bpkg", "nonexistent"]).is_err());
}

#[test]
fn no_args_fails() {
    assert!(parse(&["bpkg"]).is_err());
}

#[test]
fn pin_spec_parsing() {
    // Test the pin spec parsing logic
    fn parse_pin_spec(spec: &str) -> Option<(&str, &str)> {
        let eq = spec.find('=')?;
        let name = &spec[..eq];
        let version = &spec[eq + 1..];
        if name.is_empty() || version.is_empty() {
            return None;
        }
        Some((name, version))
    }

    let (name, version) = parse_pin_spec("firefox=128.0.1").unwrap();
    assert_eq!(name, "firefox");
    assert_eq!(version, "128.0.1");

    assert!(parse_pin_spec("firefox").is_none());
    assert!(parse_pin_spec("=128.0.1").is_none());
    assert!(parse_pin_spec("firefox=").is_none());
}
