//! How the browser starts another copy of itself.
//!
//! Failure this guards against: a window started with `--private` or
//! `--profile` keeps `BINGUX_BROWSER_PRIVATE` and `BINGUX_BROWSER_OPEN_PROFILE`
//! in its environment (its own restart put them there). A copy it starts
//! inherited them, so "open profile X" from a private window became a private
//! window, and what a window meant lived in a hidden place instead of in its
//! arguments.

use std::ffi::{OsStr, OsString};
use std::process::Command;

use super::{CommandLine, Launch};

/// What the environment of `command` is told: each variable it sets or removes.
fn told(command: &Command) -> Vec<(OsString, Option<OsString>)> {
    command
        .get_envs()
        .map(|(name, value)| (name.to_os_string(), value.map(OsStr::to_os_string)))
        .collect()
}

#[test]
fn a_copy_gets_what_it_means_from_its_arguments_and_not_from_its_parents_environment() {
    let command = Launch::copy_command("bingux-browser", vec!["--profile".into(), "work".into()]);
    assert_eq!(
        command.get_args().collect::<Vec<_>>(),
        ["--profile", "work"],
        "the arguments are the whole meaning"
    );
    let told = told(&command);
    for variable in [CommandLine::PRIVATE, CommandLine::PROFILE] {
        assert!(
            told.contains(&(OsString::from(variable), None)),
            "{variable} is removed, so a private parent does not make this window private"
        );
    }
}

#[test]
fn a_copy_sets_nothing_in_the_environment() {
    let command = Launch::copy_command("bingux-browser", vec!["--private".into()]);
    assert!(
        told(&command).iter().all(|(_, value)| value.is_none()),
        "a copy that is private says so with `--private`, which its own restart turns into the variable"
    );
}
