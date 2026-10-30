//! Helper commands for the Bingux shell.
//!
//! The shell's QML runs a helper as `bingux-helper <name> [arguments]` and
//! reads one line (or one JSON document) from its standard output. Each helper
//! prints a message the shell can show as it is and exits non-zero on failure.
//! This binary replaces the Python helpers in `shell/bingux`, one at a time;
//! see TODO.md ("Shell maintenance").

mod process_action;

use std::process::ExitCode;

const USAGE: &str = "usage: bingux-helper <helper> [arguments]\nhelpers: process-action";

fn main() -> ExitCode {
    let mut arguments = std::env::args().skip(1);
    let result = match arguments.next().as_deref() {
        Some("process-action") => process_action::run(arguments.collect()),
        Some("--help" | "-h") => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Some(other) => {
            eprintln!("bingux-helper: unknown helper {other:?}\n{USAGE}");
            return ExitCode::from(2);
        }
        None => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(message) => {
            println!("{message}");
            ExitCode::SUCCESS
        }
        // The shell shows this text to the user, so it goes to standard output.
        Err(message) => {
            println!("{message}");
            ExitCode::FAILURE
        }
    }
}
