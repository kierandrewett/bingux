use clap::Parser;

// We need to reference the args module from the bsys crate.
// Since it's a binary crate we re-declare the types inline via a path include,
// or we test via command-line parsing directly. The simplest approach for a
// binary crate is to pull in the args module as a path dependency.

#[path = "../src/args.rs"]
mod args;

use args::{Cli, Command, HomeCommand, RepoAction};

fn parse(args: &[&str]) -> Command {
    let cli = Cli::try_parse_from(args).expect("failed to parse");
    cli.command
}

#[test]
fn add_volatile() {
    let cmd = parse(&["bsys", "add", "nginx"]);
    assert!(matches!(cmd, Command::Add { ref package, keep: false } if package == "nginx"));
}

#[test]
fn add_keep() {
    let cmd = parse(&["bsys", "add", "--keep", "linux"]);
    assert!(matches!(cmd, Command::Add { ref package, keep: true } if package == "linux"));
}

#[test]
fn build_recipe() {
    let cmd = parse(&["bsys", "build", "firefox"]);
    assert!(matches!(cmd, Command::Build { ref recipe } if recipe == "firefox"));
}

#[test]
fn gc_dry_run() {
    let cmd = parse(&["bsys", "gc", "--dry-run"]);
    assert!(matches!(cmd, Command::Gc { dry_run: true }));
}

#[test]
fn diff_generations() {
    let cmd = parse(&["bsys", "diff", "41", "42"]);
    assert!(matches!(cmd, Command::Diff { gen1: 41, gen2: 42 }));
}

#[test]
fn export_all() {
    let cmd = parse(&["bsys", "export", "--all"]);
    assert!(matches!(cmd, Command::Export { package: None, all: true, .. }));
}

#[test]
fn export_index() {
    let cmd = parse(&["bsys", "export", "--index", "/path"]);
    if let Command::Export { index, .. } = cmd {
        assert_eq!(index.unwrap().to_str().unwrap(), "/path");
    } else {
        panic!("expected Export");
    }
}

#[test]
fn unkeep_force() {
    let cmd = parse(&["bsys", "unkeep", "linux", "--force"]);
    assert!(matches!(cmd, Command::Unkeep { ref package, force: true } if package == "linux"));
}

#[test]
fn grant_multiple_perms() {
    let cmd = parse(&["bsys", "grant", "nginx", "net:listen", "net:outbound"]);
    if let Command::Grant { package, perms } = cmd {
        assert_eq!(package, "nginx");
        assert_eq!(perms, vec!["net:listen", "net:outbound"]);
    } else {
        panic!("expected Grant");
    }
}

#[test]
fn home_apply() {
    let cmd = parse(&["bsys", "home", "apply"]);
    assert!(matches!(cmd, Command::Home { action: HomeCommand::Apply { path: None } }));
}

#[test]
fn repo_sync() {
    let cmd = parse(&["bsys", "repo", "sync"]);
    assert!(matches!(cmd, Command::Repo { action: RepoAction::Sync }));
}
