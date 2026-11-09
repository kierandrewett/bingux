//! Bingux Calculator: the calculator of the Bingux desktop shell.
//!
//! Basic, Scientific, Programmer and Convert modes on one engine
//! (`bingux-calc`) that computes exactly: `0.1 + 0.2` is `0.3`, and `100!`
//! has all its digits. This crate is the window: the display, the keys of each
//! mode and the history. Everything it draws comes from `bingux-ui`.

mod app;
mod display;
mod history;
mod model;
mod modes;
mod pads;
mod rates_fetch;
#[cfg(feature = "snapshot")]
mod snapshot_mode;

use bingux_ui::prelude::Application;

const USAGE: &str = "Usage: bingux-calculator [--help]\n\
\n\
Opens the calculator. A second launch raises the window that is open.\n\
\n\
The history, the mode and the exchange rates are kept under\n\
$XDG_STATE_HOME and $XDG_CACHE_HOME in a folder named bingux-calculator.\n";

fn main() {
    #[cfg(feature = "snapshot")]
    if std::env::args().nth(1).as_deref() == Some("--snapshot") {
        let dir = std::env::args()
            .nth(2)
            .unwrap_or_else(|| "/tmp/bingux-calculator-snapshots".to_string());
        snapshot_mode::run(&dir);
        return;
    }
    let _ = model::INITIAL.set(model::Initial::load());
    Application::new("bingux-calculator")
        .title("Calculator")
        .size(880.0, 660.0)
        .borderless()
        .about(bingux_ui::app_info!(
            description = "Calculate, convert units and currencies, and work in binary",
            licence_text = include_str!("../../../COPYING"),
        ))
        .usage(USAGE)
        .run(|| app::App);
}
