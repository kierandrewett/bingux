mod args;
mod commands;
mod output;

use anyhow::Result;
use clap::Parser;

use args::{Cli, Command, HomeCommand, RepoCommand};

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Add { ref package, keep } => {
            commands::add::run(package, keep)?;
        }
        Command::Rm { ref package, purge } => {
            commands::remove::run(package, purge)?;
        }
        Command::Keep { ref package } => {
            commands::keep::run_keep(package)?;
        }
        Command::Unkeep { ref package } => {
            commands::keep::run_unkeep(package)?;
        }
        Command::Pin { ref spec } => {
            commands::pin::run_pin(spec)?;
        }
        Command::Unpin { ref package } => {
            commands::pin::run_unpin(package)?;
        }
        Command::Upgrade {
            ref package, all, ..
        } => {
            commands::upgrade::run(package.as_deref(), all)?;
        }
        Command::List => {
            commands::list::run()?;
        }
        Command::Search { ref query } => {
            commands::search::run(query)?;
        }
        Command::Info { ref package } => {
            commands::info::run(package)?;
        }
        Command::Grant {
            ref package,
            ref permissions,
        } => {
            commands::permissions::run_grant(package, permissions)?;
        }
        Command::Revoke {
            ref package,
            ref permissions,
        } => {
            commands::permissions::run_revoke(package, permissions)?;
        }
        Command::Apply => {
            commands::profile::run_apply()?;
        }
        Command::Rollback { generation } => {
            commands::profile::run_rollback(generation)?;
        }
        Command::History => {
            commands::profile::run_history()?;
        }
        Command::Init => {
            commands::profile::run_init()?;
        }
        Command::Home { ref action } => match action {
            HomeCommand::Apply { path } => {
                commands::home::run_apply(path.as_ref())?;
            }
            HomeCommand::Diff => {
                commands::home::run_diff()?;
            }
            HomeCommand::Status => {
                commands::home::run_status()?;
            }
        },
        Command::Repo { ref action } => match action {
            RepoCommand::List => {
                commands::repo::run_list()?;
            }
            RepoCommand::Add { scope, url } => {
                commands::repo::run_add(scope, url)?;
            }
            RepoCommand::Rm { scope } => {
                commands::repo::run_rm(scope)?;
            }
            RepoCommand::Sync => {
                commands::repo::run_sync()?;
            }
        },
    }

    Ok(())
}
