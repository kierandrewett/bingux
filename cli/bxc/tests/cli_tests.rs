use clap::Parser;

#[path = "../src/args.rs"]
mod args;

use args::{Cli, Command};

fn parse(input: &[&str]) -> Command {
    let cli = Cli::try_parse_from(input).expect("failed to parse");
    cli.command
}

#[test]
fn run_simple() {
    let cmd = parse(&["bxc", "run", "firefox"]);
    assert!(matches!(cmd, Command::Run { ref package, ref args } if package == "firefox" && args.is_empty()));
}

#[test]
fn run_with_version() {
    let cmd = parse(&["bxc", "run", "firefox@128.0.1"]);
    assert!(matches!(cmd, Command::Run { ref package, .. } if package == "firefox@128.0.1"));
}

#[test]
fn run_with_extra_args() {
    let cmd = parse(&["bxc", "run", "firefox", "--", "--private-window"]);
    if let Command::Run { package, args } = cmd {
        assert_eq!(package, "firefox");
        assert_eq!(args, vec!["--private-window"]);
    } else {
        panic!("expected Run");
    }
}

#[test]
fn shell_command() {
    let cmd = parse(&["bxc", "shell", "neovim"]);
    assert!(matches!(cmd, Command::Shell { ref package } if package == "neovim"));
}

#[test]
fn perms_reset() {
    let cmd = parse(&["bxc", "perms", "firefox", "--reset"]);
    assert!(matches!(cmd, Command::Perms { ref package, reset: true } if package == "firefox"));
}

#[test]
fn ps_command() {
    let cmd = parse(&["bxc", "ps"]);
    assert!(matches!(cmd, Command::Ps));
}

#[test]
fn ls_command() {
    let cmd = parse(&["bxc", "ls", "firefox"]);
    assert!(matches!(cmd, Command::Ls { ref package } if package == "firefox"));
}

#[test]
fn mounts_command() {
    let cmd = parse(&["bxc", "mounts", "nginx"]);
    assert!(matches!(cmd, Command::Mounts { ref package } if package == "nginx"));
}
