//! Bingux Browser: the web browser of the Bingux desktop.
//!
//! The window is built from bingux-ui and nothing else; the pages are drawn by
//! the Vivarium engine (an embeddable Gecko) behind bingux-ui's web view control. A
//! second launch hands its address to the running window of its profile, which
//! opens it in a new tab. `--private` starts a window of its own whose tabs keep
//! nothing. Each profile is a process of its own (`docs/browser/profiles-and-store.md`).

mod address;
mod app;
mod browser;
mod commands;
mod downloads;
mod engine;
mod error_pages;
mod find;
mod first_run;
mod history;
mod locale;
mod menus;
mod navigation;
mod new_tab;
mod permissions;
mod private;
mod profiles;
mod prompts;
mod settings;
mod site_info;
#[cfg(feature = "snapshot")]
mod snapshot_mode;
mod store;
mod tabs;
mod target;
mod ui;
mod zoom;

use std::ffi::OsString;

fn main() {
    let arguments: Vec<OsString> = std::env::args_os().skip(1).collect();
    locale::register();
    #[cfg(feature = "snapshot")]
    if arguments.first().is_some_and(|first| first == "--snapshot") {
        snapshot_mode::run(&arguments[1..]);
        return;
    }
    app::Launch::run(&arguments);
}
