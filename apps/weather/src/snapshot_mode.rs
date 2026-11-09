//! `bingux-weather --snapshot DIR`: the window to PNG without a compositor.
//!
//! The app runs in `freya-testing`'s software renderer through
//! `bingux_ui::snapshot`. It reads the settings and the cache of the person who
//! runs it and asks the network like the window does, so the forecast is real;
//! `BINGUX_WEATHER_DIR` points it at a folder of its own. It writes
//! `weather.png` (the top of the page), `weather-details.png` and
//! `weather-bottom.png` (scrolled down). `BINGUX_WEATHER_SNAPSHOT_SIZE="WxH"`
//! picks the window size, `BINGUX_WEATHER_SNAPSHOT_SETTLE_MS` how long to wait
//! for the forecast (default 9000), and `BINGUX_WEATHER_SNAPSHOT_SCRIPT` (the
//! commands, or `@path` to a file of them) replaces the three captures by one
//! taken after a script.
//!
//! `bingux-weather --sky-sheet DIR` writes `sky-day.png`, `sky-twilight.png`
//! and `sky-night.png`: every condition's sky under white text, to look at the
//! art without waiting for the weather. `BINGUX_WEATHER_SKY_CELL="WxH"` sets
//! the size of a cell (the hero's own proportions on a desktop, or a phone's).

use std::time::Duration;

use bingux_ui::prelude::*;
use bingux_ui::snapshot::Snapshot;
use bingux_ui::style::*;
use freya::prelude::*;

use crate::app;
use crate::hero::Backdrop;
use crate::sky::Light;
use bingux_weather::codes::Sky;

/// The window size when `BINGUX_WEATHER_SNAPSHOT_SIZE` is not set.
const SIZE: (f32, f32) = (1100.0, 760.0);

pub fn run(dir: &str) {
    let size = std::env::var("BINGUX_WEATHER_SNAPSHOT_SIZE")
        .ok()
        .and_then(|text| {
            let (w, h) = text.split_once('x')?;
            Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
        })
        .unwrap_or(SIZE);
    let mut shot = Snapshot::new(
        move || {
            let tokens = bingux_ui::theme::load_tokens();
            Element::from(
                rect()
                    .tw(&tokens, &[w_full(), h_full(), bg(Paint::Bg)])
                    .child(app::App),
            )
        },
        size,
    );
    let settle_ms = std::env::var("BINGUX_WEATHER_SNAPSHOT_SETTLE_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(9000);
    shot.settle(Duration::from_millis(settle_ms));
    if let Ok(script) = std::env::var("BINGUX_WEATHER_SNAPSHOT_SCRIPT") {
        let script = match script.strip_prefix('@') {
            Some(file) => std::fs::read_to_string(file).unwrap_or_else(|error| {
                eprintln!("[snapshot] cannot read {file}: {error}");
                String::new()
            }),
            None => script,
        };
        if let Err(error) = shot.run_script(&script, "bingux-weather") {
            eprintln!("[snapshot] script stopped: {error}");
        }
        let path = format!("{dir}/weather.png");
        shot.save(&path);
        println!("[snapshot] wrote {path}");
        return;
    }
    let (x, y) = ((size.0 * 0.6) as i32, (size.1 * 0.6) as i32);
    for (name, script) in [
        ("weather", String::new()),
        ("weather-details", format!("scroll {x},{y},-1400; wait 600")),
        ("weather-bottom", format!("scroll {x},{y},-2600; wait 600")),
    ] {
        if let Err(error) = shot.run_script(&script, "bingux-weather") {
            eprintln!("[snapshot] {name}: script stopped: {error}");
        }
        let path = format!("{dir}/{name}.png");
        shot.save(&path);
        println!("[snapshot] wrote {path}");
    }
}

/// The cell of a sheet of skies, when `BINGUX_WEATHER_SKY_CELL` is not set.
const CELL: (f32, f32) = (330.0, 190.0);
const COLUMNS: usize = 4;

const SKIES: [(Sky, &str); 12] = [
    (Sky::Clear, "Clear"),
    (Sky::MostlyClear, "Mostly clear"),
    (Sky::PartlyCloudy, "Partly cloudy"),
    (Sky::Overcast, "Overcast"),
    (Sky::Fog, "Fog"),
    (Sky::Drizzle, "Drizzle"),
    (Sky::Rain, "Rain"),
    (Sky::HeavyRain, "Heavy rain"),
    (Sky::FreezingRain, "Freezing rain"),
    (Sky::Snow, "Snow"),
    (Sky::HeavySnow, "Heavy snow"),
    (Sky::Thunderstorm, "Thunderstorm"),
];

/// A sky with a hero's text over it, so the haze is judged where it matters.
#[derive(Clone, PartialEq)]
struct Cell {
    sky: Sky,
    light: Light,
    name: &'static str,
    size: (f32, f32),
}

impl Component for Cell {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let on_sky = |style: typography::TextStyle| style.in_ink(Ink::OnFloating);
        rect()
            .width(Size::px(self.size.0))
            .height(Size::px(self.size.1))
            .tw(&tokens, &[clip()])
            .child(Backdrop {
                sky: self.sky,
                light: self.light,
            })
            .child(
                // Freya paints by depth and a tie goes either way, so the text
                // is made deeper than the sky, as the hero does.
                rect().child(
                    rect()
                        .width(Size::px(self.size.0))
                        .height(Size::px(self.size.1))
                        .tw(
                            &tokens,
                            &[flex_col(), items(Align::Center), justify(Align::Center)],
                        )
                        .child(on_sky(typography::TITLE).label(&tokens, "Tokyo"))
                        .child(on_sky(typography::HERO).label_scaled(&tokens, "9°", 1.0))
                        .child(on_sky(typography::HEADING).label(&tokens, self.name))
                        .child(on_sky(typography::BODY).label(&tokens, "High 11°  Low 8°"))
                        .child(on_sky(typography::CAPTION).label(&tokens, "Feels like 8°")),
                ),
            )
    }
}

/// The window of a sheet: every sky in one light.
#[derive(Clone, PartialEq)]
struct Sheet {
    light: Light,
    cell: (f32, f32),
}

impl Component for Sheet {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        let mut grid = rect().tw(
            &tokens,
            &[
                w_full(),
                h_full(),
                bg(Paint::Bg),
                flex_col(),
                gap(S2),
                p(S2),
            ],
        );
        for row in SKIES.chunks(COLUMNS) {
            let mut line = rect().tw(&tokens, &[flex_row(), gap(S2)]);
            for (sky, name) in row {
                line = line.child(Cell {
                    sky: *sky,
                    light: self.light,
                    name,
                    size: self.cell,
                });
            }
            grid = grid.child(line);
        }
        grid
    }
}

/// `--sky-sheet DIR`.
pub fn sky_sheet(dir: &str) {
    let cell = std::env::var("BINGUX_WEATHER_SKY_CELL")
        .ok()
        .and_then(|text| {
            let (w, h) = text.split_once('x')?;
            Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
        })
        .unwrap_or(CELL);
    let gutter = 8.0;
    let rows = SKIES.len().div_ceil(COLUMNS);
    let size = (
        COLUMNS as f32 * (cell.0 + gutter) + gutter,
        rows as f32 * (cell.1 + gutter) + gutter,
    );
    for (file, light) in [
        ("sky-day", Light::Day),
        ("sky-twilight", Light::Twilight),
        ("sky-night", Light::Night),
    ] {
        let mut shot = Snapshot::new(move || Sheet { light, cell }.into_element(), size);
        shot.settle(Duration::from_millis(800));
        let path = format!("{dir}/{file}.png");
        shot.save(&path);
        println!("[snapshot] wrote {path}");
    }
}
