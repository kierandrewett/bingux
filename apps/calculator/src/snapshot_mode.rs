//! `bingux-calculator --snapshot DIR`: the window to PNG without a compositor.
//!
//! The app runs in `freya-testing`'s software renderer through
//! `bingux_ui::snapshot`. It writes one PNG for each mode (`basic.png`,
//! `scientific.png`, `programmer.png`, `convert.png`) and one of the history
//! after two finished calculations (`history.png`).
//! `BINGUX_CALCULATOR_SNAPSHOT_SIZE="WxH"` picks the window size, and
//! `BINGUX_CALCULATOR_SNAPSHOT_SCRIPT` replaces the scenes by one that is
//! taken after a script. Run it with `XDG_STATE_HOME` and `XDG_CACHE_HOME`
//! pointing at an empty folder, so nothing is read from or written to the
//! real ones, and `BINGUX_SETTINGS_SNAPSHOT_SCHEME=prefer-light` or
//! `prefer-dark` for the colour scheme.

use std::time::Duration;

use bingux_ui::prelude::*;
use bingux_ui::snapshot::Snapshot;
use bingux_ui::style::{Paint, Tw, bg, h_full, w_full};
use freya::prelude::*;

use crate::app;
use crate::model::{INITIAL, Initial};

const SIZE: (f32, f32) = (880.0, 660.0);

/// The scenes: a name and the walk that sets it up.
const SCENES: &[(&str, &str)] = &[
    ("basic", "type 1234.5*8+3; wait 300"),
    (
        "scientific",
        "click-text Scientific; wait 400; type sin(30)+2^10; wait 300",
    ),
    (
        "programmer",
        "click-text Programmer; wait 400; type 0xFF and 0x3C; wait 300",
    ),
    (
        "convert",
        "click-text Convert; wait 400; type 100; wait 300",
    ),
    (
        "history",
        "type 6*7; key return; wait 200; type 2^64; key return; wait 300",
    ),
];

fn capture(dir: &str, name: &str, script: &str, size: (f32, f32)) {
    let mut shot = Snapshot::new(
        || {
            let tokens = bingux_ui::theme::load_tokens();
            Element::from(
                rect()
                    .tw(&tokens, &[w_full(), h_full(), bg(Paint::Bg)])
                    .child(app::App),
            )
        },
        size,
    );
    shot.settle(Duration::from_millis(1500));
    if let Err(error) = shot.run_script(script, "bingux-calculator") {
        eprintln!("[snapshot] {name}: script stopped: {error}");
    }
    let path = format!("{dir}/{name}.png");
    shot.save(&path);
    println!("[snapshot] wrote {path}");
}

pub fn run(dir: &str) {
    let _ = INITIAL.set(Initial::clean());
    let size = std::env::var("BINGUX_CALCULATOR_SNAPSHOT_SIZE")
        .ok()
        .and_then(|text| {
            let (w, h) = text.split_once('x')?;
            Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
        })
        .unwrap_or(SIZE);
    if let Ok(script) = std::env::var("BINGUX_CALCULATOR_SNAPSHOT_SCRIPT") {
        capture(dir, "calculator", &script, size);
        return;
    }
    for (name, script) in SCENES {
        capture(dir, name, script, size);
    }
}
