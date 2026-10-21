//! Read-only facts for the Bingux lock screen and login greeter.
//!
//! Every command prints one JSON document on stdout. The tool never prompts
//! and never handles credentials; PAM conversations stay in greetd or in the
//! lock client's PamContext.
//!
//! ```text
//! bingux-session users                    accounts that can own a graphical session
//! bingux-session sessions                 wayland-sessions entries, Gnoblin first
//! bingux-session biometrics USER SERVICE  PAM biometric methods that can serve USER
//! bingux-session remember USER SESSION    store the last successful login choice
//! bingux-session recall                   read the stored login choice
//! bingux-session greeter-background       Lua that paints the remembered account's wallpaper
//! bingux-session wallpaper                the calling user's wallpaper from bingux-config
//! bingux-session export                   publish the greeter snapshot and Gnoblin background
//! ```

mod accounts;
mod biometrics;
mod sessions;
mod snapshot;
mod state;

use std::{env, io::Write, process};

const USAGE: &str = "usage: bingux-session users | sessions | biometrics USER SERVICE | remember USER SESSION | recall | greeter-background | wallpaper | export";

fn main() {
    let arguments: Vec<String> = env::args().skip(1).collect();
    let words: Vec<&str> = arguments.iter().map(String::as_str).collect();
    let output = match words.as_slice() {
        ["users"] => serde_json::to_string(&accounts::Catalog::read()),
        ["sessions"] => serde_json::to_string(&sessions::Catalog::read()),
        ["biometrics", user, service] => serde_json::to_string(&biometrics::probe(user, service)),
        ["remember", user, session] => match state::remember(user, session) {
            Ok(choice) => serde_json::to_string(&choice),
            Err(error) => {
                eprintln!("bingux-session: could not store the login choice: {error}");
                process::exit(1);
            }
        },
        ["recall"] => serde_json::to_string(&state::recall()),
        ["greeter-background"] => {
            let catalog = accounts::Catalog::read();
            let remembered = state::recall().user;
            let account = catalog
                .users
                .iter()
                .find(|account| account.name == remembered && !account.wallpaper.is_empty())
                .or_else(|| {
                    catalog
                        .users
                        .iter()
                        .find(|account| !account.wallpaper.is_empty())
                });
            let wallpaper = account
                .map(|account| account.wallpaper.as_str())
                .unwrap_or("");
            print!("{}", snapshot::greeter_background_lua(wallpaper));
            return;
        }
        ["wallpaper"] => serde_json::to_string(
            &serde_json::json!({ "wallpaper": snapshot::configured_wallpaper() }),
        ),
        ["export"] => match accounts::current_user_name() {
            Some(user) => {
                let wallpaper = snapshot::configured_wallpaper();
                if let Err(error) =
                    snapshot::write_background_dropin(&snapshot::config_home(), &wallpaper)
                {
                    eprintln!(
                        "bingux-session: could not write the Gnoblin background drop-in: {error}"
                    );
                }
                match snapshot::export(&snapshot::root(), &user, &wallpaper) {
                    Ok(result) => serde_json::to_string(&result),
                    Err(error) => {
                        eprintln!("bingux-session: could not export the greeter snapshot: {error}");
                        process::exit(1);
                    }
                }
            }
            None => {
                eprintln!("bingux-session: the calling user is not in the password database");
                process::exit(1);
            }
        },
        _ => {
            eprintln!("{USAGE}");
            process::exit(2);
        }
    };
    let text = output.expect("serialising plain data cannot fail");
    let mut stdout = std::io::stdout().lock();
    if writeln!(stdout, "{text}").is_err() {
        process::exit(1);
    }
}
