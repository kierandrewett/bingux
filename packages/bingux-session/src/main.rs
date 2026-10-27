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
//! bingux-session publish [--dry-run]      hand the login-screen appearance to bsm (bsmctl)
//! bingux-session blur-wallpaper SOURCE TARGET  write a pre-blurred, dimmed copy (packaging)
//! ```

mod accounts;
mod biometrics;
mod publish;
mod sessions;
mod snapshot;
mod state;

use std::{env, io::Write, path::Path, process};

const USAGE: &str = "usage: bingux-session users | sessions | biometrics USER SERVICE | remember USER SESSION | recall | greeter-background | wallpaper | export | publish [--dry-run] | blur-wallpaper SOURCE TARGET";

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
            print!(
                "{}",
                snapshot::greeter_background_lua(&snapshot::greeter_image(wallpaper))
            );
            return;
        }
        ["wallpaper"] => serde_json::to_string(
            &serde_json::json!({ "wallpaper": snapshot::configured_wallpaper() }),
        ),
        ["publish"] => process::exit(publish::publish(&publish::resolve(&publish::System))),
        ["publish", "--dry-run"] => {
            let appearance = publish::resolve(&publish::System);
            serde_json::to_string(&serde_json::json!({
                "arguments": publish::bsmctl_arguments(&appearance),
                "appearance": appearance,
            }))
        }
        ["export"] => match accounts::current_user_name() {
            Some(user) => {
                // An account that keeps its pictures off the login screen
                // (privacy.show_personal_images) exports no wallpaper.
                let wallpaper = if snapshot::show_personal_images() {
                    snapshot::configured_wallpaper()
                } else {
                    String::new()
                };
                // The export writes the blurred twin that the drop-in names, so it
                // runs first. A failed export still leaves a drop-in for the last twin.
                let exported = snapshot::export(&snapshot::root(), &user, &wallpaper);
                let directory = snapshot::root().join(&user);
                if let Err(error) = snapshot::write_background_dropin(
                    &snapshot::config_home(),
                    &snapshot::handover_image(&directory),
                ) {
                    eprintln!(
                        "bingux-session: could not write the Gnoblin background drop-in: {error}"
                    );
                }
                match exported {
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
        ["blur-wallpaper", source, target] => match snapshot::blurred_format(Path::new(target)) {
            Some(format) => {
                match snapshot::write_blurred(Path::new(source), Path::new(target), format) {
                    Ok(()) => serde_json::to_string(target),
                    Err(error) => {
                        eprintln!("bingux-session: could not blur {source}: {error}");
                        process::exit(1);
                    }
                }
            }
            None => {
                eprintln!("bingux-session: blur-wallpaper TARGET must end in .jpg, .jpeg or .png");
                process::exit(2);
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
