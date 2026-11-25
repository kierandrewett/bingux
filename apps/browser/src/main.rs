//! Bingux Browser: the web browser of the Bingux desktop.
//!
//! The window is built from bingux-ui and nothing else; the pages are drawn by
//! the Vivarium engine (an embeddable Gecko) behind `bingux-geckoview`. A
//! second launch hands its address to the running window, which opens it in a
//! new tab. `--private` starts a window of its own whose tabs keep nothing.

mod model;
#[cfg(feature = "snapshot")]
mod snapshot_mode;
mod target;
mod ui;

use std::os::unix::process::CommandExt;

use bingux_ui::prelude::Application;

use crate::model::browser::Options;
use crate::ui::{App, Engine};

const USAGE: &str = "Usage: bingux-browser [--private] [ADDRESS | WORDS | FILE]...\n\
\n\
Opens ADDRESS in a new tab of the browser window, starting it if it is not\n\
running. WORDS are searched for with the search engine of the settings. A FILE\n\
is a path starting with /, ./, ../ or ~/. With no argument a new tab opens.\n\
\n\
--private starts a separate window whose tabs keep nothing: no history, no\n\
cookies and no site data. It is a process of its own.\n";

/// The environment variable that makes a copy of the browser a private window.
pub const PRIVATE: &str = "BINGUX_BROWSER_PRIVATE";

fn main() {
    let mut arguments: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    #[cfg(feature = "snapshot")]
    if arguments.first().is_some_and(|first| first == "--snapshot") {
        snapshot_mode::run(&arguments[1..]);
        return;
    }
    let private = std::env::var_os(PRIVATE).is_some();
    if arguments.first().is_some_and(|first| first == "--private") && !private {
        // A private window is a copy of its own with a private application id,
        // so it is not the running window's second launch.
        arguments.remove(0);
        if let Ok(program) = std::env::current_exe() {
            let error = std::process::Command::new(program).env(PRIVATE, "1").args(arguments).exec();
            eprintln!("bingux-browser: could not start a private window: {error}");
        }
        std::process::exit(1);
    }
    let engine = match start_engine(private) {
        Ok(engine) => engine,
        Err(message) => {
            eprintln!("bingux-browser: {message}");
            std::process::exit(1);
        }
    };
    let options = Options { private };
    let id = if private { "bingux-browser-private" } else { "bingux-browser" };
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
        .run(move || App {
            engine: engine.clone(),
            options,
            start: Vec::new(),
        });
}

/// Start the engine, with the profile of this window.
#[cfg(not(feature = "vivarium"))]
fn start_engine(_private: bool) -> Result<Engine, String> {
    Err("this build has no engine. Build it with the `vivarium` feature, against a Vivarium SDK.".to_string())
}

#[cfg(feature = "vivarium")]
fn start_engine(private: bool) -> Result<Engine, String> {
    // An engine that does not start is not a reason to exit: the window opens and says why
    // (a profile folder that is not safe, a sandbox it cannot reach), and offers to try again.
    Ok(Engine(bingux_geckoview::engine::start(bingux_geckoview::engine::Config::from_environment(private))))
}
