mod args;
mod commands;
mod output;

use args::{Cli, Command};
use clap::Parser;

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Command::Run { package, args } => commands::run::run(package, args),
        Command::Shell { package } => commands::shell::run(package),
        Command::Inspect { package } => commands::inspect::run(package),
        Command::Perms { package, reset } => commands::perms::run(package, *reset),
        Command::Ps => commands::ps::run(),
        Command::Ls { package } => commands::ls::run(package),
        Command::Mounts { package } => commands::mounts::run(package),
    }
}
