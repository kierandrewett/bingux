mod args;
mod commands;
mod output;

use args::{Cli, Command};
use clap::Parser;

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Command::Add { package, keep } => commands::add::run(package, *keep),
        Command::Rm { package } => commands::remove::run(package),
        Command::Keep { package } => commands::keep::run_keep(package),
        Command::Unkeep { package, force } => commands::keep::run_unkeep(package, *force),
        Command::Build { recipe } => commands::build::run(recipe),
        Command::Upgrade { package, all } => {
            commands::compose::upgrade(package.as_deref(), *all);
        }
        Command::Apply => commands::compose::apply(),
        Command::Rollback { generation } => commands::compose::rollback(*generation),
        Command::History => commands::compose::history(),
        Command::Diff { gen1, gen2 } => commands::compose::diff(*gen1, *gen2),
        Command::List => commands::list::run(),
        Command::Info { package } => commands::list::info(package),
        Command::Grant { package, perms } => commands::permissions::grant(package, perms),
        Command::Revoke { package, perms } => commands::permissions::revoke(package, perms),
        Command::Gc { dry_run } => commands::gc::run(*dry_run),
        Command::Export {
            package,
            all,
            index,
        } => commands::export::run(package.as_deref(), *all, index.as_deref()),
        Command::Repo { action } => commands::repo::run(action),
        Command::Home { action } => commands::home::run(action),
    }
}
