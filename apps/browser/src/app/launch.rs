//! The start of the browser: which profile opens, which engine starts, which window runs.

use std::ffi::OsString;
use std::os::unix::process::CommandExt;

use bingux_ui::prelude::{Application, Message, i18n_id, tr};

use super::CommandLine;
use crate::browser::{Options, Profile};
use crate::engine::Engine;
use crate::profiles::open::{Opened, Waiting};
use crate::profiles::places::{Folders, Places};
use crate::profiles::{Gate, GateWindow};
use crate::store;
use crate::ui::{App, Handoff};

/// The launch of the browser.
pub struct Launch;

impl Launch {
    /// Starts this program again in place of this process: in `profile` when one
    /// is named, with the addresses of this launch. bingux-ui's launch does not
    /// know the browser's own options, so they travel in the environment.
    pub fn relaunch(profile: Option<&str>) -> ! {
        let arguments: Vec<OsString> = std::env::args_os().skip(1).collect();
        Self::restart(false, profile, &arguments)
    }

    fn restart(private: bool, profile: Option<&str>, arguments: &[OsString]) -> ! {
        match std::env::current_exe() {
            Ok(program) => {
                let mut command = std::process::Command::new(program);
                if private {
                    command.env(CommandLine::PRIVATE, "1");
                }
                if let Some(profile) = profile {
                    command.env(CommandLine::PROFILE, profile);
                }
                let error = command.args(arguments).exec();
                eprintln!("bingux-browser: could not start again: {error}");
            }
            Err(error) => eprintln!("bingux-browser: could not find this program: {error}"),
        }
        std::process::exit(1);
    }

    pub fn run(arguments: &[OsString]) {
        let own = match CommandLine::parse(arguments) {
            Ok(own) => own,
            Err(message) => {
                eprintln!("bingux-browser: {message}\n{}", CommandLine::USAGE);
                std::process::exit(2);
            }
        };
        if own.private || own.profile.is_some() {
            // A private window is a copy of its own with a private application id,
            // so it is not the running window's second launch.
            let private = own.private || std::env::var_os(CommandLine::PRIVATE).is_some();
            let profile = own
                .profile
                .or_else(|| std::env::var(CommandLine::PROFILE).ok());
            Self::restart(private, profile.as_deref(), &own.rest);
        }
        let private = std::env::var_os(CommandLine::PRIVATE).is_some();
        let asked = std::env::var(CommandLine::PROFILE)
            .ok()
            .filter(|name| !name.is_empty());
        let places = Places::from_environment();
        let opened = if private {
            Opened::private(&places, asked.as_deref())
        } else {
            // A profile another copy holds: hand this launch to that copy, or
            // wait for the profile while that copy closes.
            let mut waiting = Waiting::new(Waiting::PATIENCE, |app_id: &str| {
                Self::application(app_id).hand_off()
            });
            Opened::start(&places, asked.as_deref(), &mut waiting)
        };
        match opened {
            // The copy with the profile open took this launch.
            Opened::HandedOff => {}
            Opened::Profile(mut opened) => {
                if let Some(cache) = opened.stale_cache.take() {
                    // The old engine cache goes on a thread of its own: it can be large.
                    std::thread::spawn(move || {
                        let _ = store::files::remove_tree(&cache);
                    });
                }
                let engine = Self::start_or_exit(Some(&opened.folders));
                let settings_keeping =
                    store::keeping::Keeping::of_settings(opened.settings_problem.as_ref());
                let (store, settings_keeping) = match store::Store::open(
                    &opened.folders,
                    settings_keeping.clone(),
                    bingux_ui::search::service(),
                ) {
                    Ok(store) => (Some(store), settings_keeping),
                    Err(error) => (
                        None,
                        store::keeping::Keeping::NotSaved(store::keeping::NotSaved::Failed(
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
                Self::run_browser(&opened.app_id, engine, Options { private: false }, profile);
            }
            Opened::Private { settings, from } => {
                let engine = Self::start_or_exit(None);
                let profile = Profile::private(from.map(|id| id.as_str().to_string()), settings);
                Self::run_browser(
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
                Self::run_gate("bingux-browser", Gate::Choose(profiles));
            }
            Opened::Problem(state) => {
                // A profile another window has open: its application id, so this
                // launch hands its addresses to that window instead of showing this.
                let id = match &state {
                    crate::profiles::open::ProfileState::InUse { app_id, .. } => app_id.clone(),
                    _ => "bingux-browser".to_string(),
                };
                Self::run_gate(&id, Gate::Problem(state));
            }
        }
    }

    fn application(id: &str) -> Application {
        Application::new(id)
            .title(Message::new(i18n_id!("browser-title")))
            .size(1100.0, 760.0)
            .borderless()
            .normalise(crate::target::Target::normalise)
            .about(bingux_ui::app_info!(
                description = tr!("browser-description"),
                licence_text = include_str!("../../../../COPYING"),
            ))
            .usage(CommandLine::USAGE)
    }

    fn run_browser(id: &str, engine: Engine, options: Options, profile: Profile) {
        let profile = Handoff::new(profile);
        Self::application(id).run(move || App {
            engine: engine.clone(),
            options,
            start: Vec::new(),
            profile: profile.clone(),
        });
    }

    fn run_gate(id: &str, gate: Gate) {
        Self::application(id).run(move || GateWindow { gate: gate.clone() });
    }

    fn start_or_exit(folders: Option<&Folders>) -> Engine {
        match Engine::start(folders) {
            Ok(engine) => engine,
            Err(message) => {
                eprintln!("bingux-browser: {message}");
                std::process::exit(1);
            }
        }
    }
}
