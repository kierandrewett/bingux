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

use std::time::Duration;

use bingux_ui::prelude::*;
use bingux_ui::snapshot::Snapshot;
use bingux_ui::style::{Paint, Tw, bg, h_full, w_full};
use freya::prelude::*;

use crate::app;

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
