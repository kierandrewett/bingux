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

const USAGE: &str = "Usage: bingux-calculator [--mode MODE]\n\
\n\
Opens the calculator. MODE is basic, scientific, programmer or convert.\n\
A second launch raises the window that is open, in the mode it names.\n\
\n\
The history, the mode, the window size and the exchange rates are kept under\n\
$XDG_STATE_HOME and $XDG_CACHE_HOME in a folder named bingux-calculator.\n";

fn main() {
    #[cfg(feature = "snapshot")]
    if std::env::args().nth(1).as_deref() == Some("--snapshot") {
        snapshot_mode::run(std::env::args().skip(2));
        return;
    }
    let initial = model::Initial::load();
    let (width, height) = initial
        .saved
        .settings
        .window
        .map_or((880.0, 660.0), |(w, h)| (w as f32, h as f32));
    let _ = model::INITIAL.set(initial);
    Application::new("bingux-calculator")
        .title("Calculator")
        .routes(app::routes())
        .address_option("--mode")
        .size(width, height)
        .borderless()
        .about(bingux_ui::app_info!(
            description = "Calculate, convert units and currencies, and work in binary.",
            licence_text = include_str!("../../../COPYING"),
        ))
        .usage(USAGE)
        .run(|| app::App);
}
