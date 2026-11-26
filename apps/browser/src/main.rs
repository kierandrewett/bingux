//! Bingux Browser: the web browser of the Bingux desktop.
//!
//! The window is built from bingux-ui and nothing else; the pages are drawn by
//! the Vivarium engine (an embeddable Gecko) behind bingux-ui's web view control. A
//! second launch hands its address to the running window of its profile, which
//! opens it in a new tab. `--private` starts a window of its own whose tabs keep
//! nothing. Each profile is a process of its own (`docs/browser/profiles-and-store.md`).

mod address;
mod browser;
mod engine;
mod locale;
mod model;
mod profile;
#[cfg(feature = "snapshot")]
mod snapshot_mode;
mod target;
mod ui;
mod zoom;

use std::ffi::OsString;
use std::os::unix::process::CommandExt;

use bingux_ui::prelude::Application;

use crate::browser::{Options, Profile};
use crate::engine::Engine;
use crate::profile::open::{Opened, open, open_private};
use crate::profile::places::{Folders, Places};
use crate::ui::{App, Gate, GateWindow, Handoff};

const USAGE: &str = "Usage: bingux-browser [--private] [--profile NAME] [ADDRESS | WORDS | FILE]...\n\
\n\
Opens ADDRESS in a new tab of the browser window, starting it if it is not\n\
running. WORDS are searched for with the search engine of the settings. A FILE\n\
is a path starting with /, ./, ../ or ~/. With no argument a new tab opens.\n\
\n\
--profile NAME opens the profile called NAME (or with that id) instead of the\n\
default one. Each profile keeps its own history, cookies and settings.\n\
\n\
--private starts a separate window whose tabs keep nothing: no history, no\n\
cookies and no site data. It is a process of its own.\n";

/// The environment variable that makes a copy of the browser a private window.
pub const PRIVATE: &str = "BINGUX_BROWSER_PRIVATE";
/// The environment variable that names the profile a copy opens (`--profile`).
pub const PROFILE: &str = "BINGUX_BROWSER_OPEN_PROFILE";

/// The browser's own options, taken off the command line; bingux-ui's launch
/// reads the rest (the addresses).
#[derive(Debug, Default, PartialEq)]
struct OwnOptions {
    private: bool,
    profile: Option<String>,
    rest: Vec<OsString>,
}

fn own_options(arguments: &[OsString]) -> Result<OwnOptions, String> {
    let mut own = OwnOptions::default();
    let mut rest = arguments.iter();
    while let Some(argument) = rest.next() {
        let text = argument.to_string_lossy();
        if text == "--" {
            own.rest.push(argument.clone());
            own.rest.extend(rest.cloned());
            break;
        } else if text == "--private" {
            own.private = true;
        } else if text == "--profile" {
            let name = rest.next().ok_or("--profile needs the name of a profile")?;
            own.profile = Some(name.to_string_lossy().into_owned());
        } else if let Some(name) = text.strip_prefix("--profile=") {
            own.profile = Some(name.to_string());
        } else {
            own.rest.push(argument.clone());
        }
    }
    Ok(own)
}

/// Starts this program again in place of this process: in `profile` when one
/// is named, with the addresses of this launch. bingux-ui's launch does not
/// know the browser's own options, so they travel in the environment.
pub fn relaunch(profile: Option<&str>) -> ! {
    let arguments: Vec<OsString> = std::env::args_os().skip(1).collect();
    restart(false, profile, &arguments)
}

fn restart(private: bool, profile: Option<&str>, arguments: &[OsString]) -> ! {
    match std::env::current_exe() {
        Ok(program) => {
            let mut command = std::process::Command::new(program);
            if private {
                command.env(PRIVATE, "1");
            }
            if let Some(profile) = profile {
                command.env(PROFILE, profile);
            }
            let error = command.args(arguments).exec();
            eprintln!("bingux-browser: could not start again: {error}");
        }
        Err(error) => eprintln!("bingux-browser: could not find this program: {error}"),
    }
    std::process::exit(1);
}

fn main() {
    let arguments: Vec<OsString> = std::env::args_os().skip(1).collect();
    locale::register();
    #[cfg(feature = "snapshot")]
    if arguments.first().is_some_and(|first| first == "--snapshot") {
        snapshot_mode::run(&arguments[1..]);
        return;
    }
    let own = match own_options(&arguments) {
        Ok(own) => own,
        Err(message) => {
            eprintln!("bingux-browser: {message}\n{USAGE}");
            std::process::exit(2);
        }
    };
    if own.private || own.profile.is_some() {
        // A private window is a copy of its own with a private application id,
        // so it is not the running window's second launch.
        let private = own.private || std::env::var_os(PRIVATE).is_some();
        let profile = own.profile.or_else(|| std::env::var(PROFILE).ok());
        restart(private, profile.as_deref(), &own.rest);
    }
    let private = std::env::var_os(PRIVATE).is_some();
    let asked = std::env::var(PROFILE).ok().filter(|name| !name.is_empty());
    let places = Places::from_environment();
    let opened = if private {
        open_private(&places, asked.as_deref())
    } else {
        open(&places, asked.as_deref())
    };
    match opened {
        Opened::Profile(mut opened) => {
            if let Some(cache) = opened.stale_cache.take() {
                // The old engine cache goes on a thread of its own: it can be large.
                std::thread::spawn(move || {
                    let _ = profile::files::remove_tree(&cache);
                });
            }
            let engine = start_or_exit(Some(&opened.folders));
            let settings_keeping = profile::store::keeping_of(opened.settings_problem.as_ref());
            let (store, settings_keeping) = match profile::store::Store::open(
                &opened.folders,
                settings_keeping.clone(),
                bingux_ui::search::service(),
            ) {
                Ok(store) => (Some(store), settings_keeping),
                Err(error) => (
                    None,
                    model::keeping::Keeping::NotSaved(model::keeping::NotSaved::Failed(
                        error.to_string(),
                    )),
                ),
            };
            let profile = Profile {
                id: opened
                    .entry
                    .as_ref()
                    .map(|entry| entry.id.as_str().to_string()),
                name: opened.entry.as_ref().map(|entry| entry.name.clone()),
                settings: opened.settings,
                settings_keeping,
                store,
            };
            // The lock is held until the process ends.
            let _lock = opened.lock;
            run_browser(&opened.app_id, engine, Options { private: false }, profile);
        }
        Opened::Private { settings, from } => {
            let engine = start_or_exit(None);
            let profile = Profile::private(from.map(|id| id.as_str().to_string()), settings);
            run_browser(
                "bingux-browser-private",
                engine,
                Options { private: true },
                profile,
            );
        }
        Opened::Choose(entries) => {
            let profiles = entries
                .into_iter()
                .map(|entry| (entry.id.as_str().to_string(), entry.name))
                .collect();
            run_gate("bingux-browser", Gate::Choose(profiles));
        }
        Opened::Problem(state) => {
            // A profile another window has open: its application id, so this
            // launch hands its addresses to that window instead of showing this.
            let id = match &state {
                profile::open::ProfileState::InUse { app_id, .. } => app_id.clone(),
                _ => "bingux-browser".to_string(),
            };
            run_gate(&id, Gate::Problem(state));
        }
    }
}

fn application(id: &str) -> Application {
    Application::new(id)
        .title("Browser")
        .size(1100.0, 760.0)
        .borderless()
        .normalise(target::normalise)
        .about(bingux_ui::app_info!(
            name = "Browser",
            description = "Browse the web",
            licence_text = include_str!("../../../COPYING"),
        ))
        .usage(USAGE)
}

fn run_browser(id: &str, engine: Engine, options: Options, profile: Profile) {
    let profile = Handoff::new(profile);
    application(id).run(move || App {
        engine: engine.clone(),
        options,
        start: Vec::new(),
        profile: profile.clone(),
    });
}

fn run_gate(id: &str, gate: Gate) {
    application(id).run(move || GateWindow { gate: gate.clone() });
}

fn start_or_exit(folders: Option<&Folders>) -> Engine {
    match start_engine(folders) {
        Ok(engine) => engine,
        Err(message) => {
            eprintln!("bingux-browser: {message}");
            std::process::exit(1);
        }
    }
}

/// Start the engine in `folders`, or in a throwaway profile for a private window.
#[cfg(not(any(feature = "vivarium", feature = "dynamic-load")))]
fn start_engine(_folders: Option<&Folders>) -> Result<Engine, String> {
    Err(
        "this build has no engine. Build it with the `vivarium` feature, against a Vivarium SDK."
            .to_string(),
    )
}

#[cfg(any(feature = "vivarium", feature = "dynamic-load"))]
fn start_engine(folders: Option<&Folders>) -> Result<Engine, String> {
    use bingux_ui::web_view::engine::{Config, EngineFolders, start};
    let folders = match folders {
        None => EngineFolders::Private,
        Some(folders) => EngineFolders::Profile {
            profile: folders.engine.clone(),
            cache: folders.cache.clone(),
            crash: folders.crash.clone(),
            runtime: folders.runtime.clone(),
        },
    };
    // An engine that does not start is not a reason to exit: the window opens and says why
    // (a profile folder that is not safe, a sandbox it cannot reach), and offers to try again.
    Ok(Engine(start(Config::from_environment(folders))))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn own(arguments: &[&str]) -> OwnOptions {
        let arguments: Vec<OsString> = arguments.iter().map(OsString::from).collect();
        own_options(&arguments).unwrap()
    }

    #[test]
    fn the_browsers_own_options_come_off_the_command_line() {
        let parsed = own(&["--profile", "Work", "example.com", "--private"]);
        assert!(parsed.private);
        assert_eq!(parsed.profile.as_deref(), Some("Work"));
        assert_eq!(parsed.rest, [OsString::from("example.com")]);
        assert_eq!(own(&["--profile=Study"]).profile.as_deref(), Some("Study"));
        let after = own(&["--", "--profile", "x"]);
        assert_eq!(after.profile, None, "after -- everything is an address");
        assert_eq!(after.rest.len(), 3);
        let arguments = [OsString::from("--profile")];
        assert!(own_options(&arguments).is_err());
    }
}
