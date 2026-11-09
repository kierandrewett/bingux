//! Bingux Weather: the forecast app of the Bingux desktop shell.
//!
//! The data is Open-Meteo's, fetched and cached by `bingux-weather` (the
//! model, the units, the moon and the notices live there). The position is
//! `bingux_ui::location`'s. Everything on screen is `bingux-ui`: the window,
//! the lists, the charts, the dialogs. This crate is the pages: the locations
//! list, the forecast and its cards, and the sheet that adds a city.

mod app;
mod cards;
mod format;
mod glyph;
mod hero;
mod hourly;
mod hub;
mod load;
mod locations;
mod page;
mod search;
mod shell;
mod sky;
#[cfg(feature = "snapshot")]
mod snapshot_mode;
mod week;
mod words;

use bingux_ui::prelude::Application;

const USAGE: &str = "Usage: bingux-weather\n\
\n\
Opens the Weather window, or raises it when it is already open. The forecast\n\
is for the computer's position, found with the location service or, failing\n\
that, the network address, and for the cities that were added. The data is\n\
Open-Meteo's (https://open-meteo.com), licensed CC BY 4.0.\n";

fn main() {
    bingux_ui::i18n::register(bingux_ui::catalogue!("weather"));
    #[cfg(feature = "snapshot")]
    if std::env::args().nth(1).as_deref() == Some("--snapshot") {
        let dir = std::env::args()
            .nth(2)
            .unwrap_or_else(|| "/tmp/bingux-weather-snapshots".to_string());
        snapshot_mode::run(&dir);
        return;
    }
    #[cfg(feature = "snapshot")]
    if std::env::args().nth(1).as_deref() == Some("--sky-sheet") {
        let dir = std::env::args()
            .nth(2)
            .unwrap_or_else(|| "/tmp/bingux-weather-snapshots".to_string());
        snapshot_mode::sky_sheet(&dir);
        return;
    }
    Application::new("bingux-weather")
        .title(bingux_ui::prelude::tr!("weather-title"))
        .size(980.0, 760.0)
        .borderless()
        .about(bingux_ui::app_info!(
            description = bingux_ui::prelude::tr!("weather-description"),
            licence_text = include_str!("../../../COPYING"),
        ))
        .usage(USAGE)
        .run(|| app::App);
}

#[cfg(test)]
mod catalogue_tests {
    /// Every `tr!` of the app has its message, with the right variables, and
    /// the catalogue holds nothing the code does not use.
    #[test]
    fn the_catalogue_matches_the_code() {
        bingux_ui::i18n::check::crate_catalogue(env!("CARGO_MANIFEST_DIR"), "weather")
            .assert_clean();
    }
}
