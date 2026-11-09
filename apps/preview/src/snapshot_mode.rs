//! `bingux-preview --snapshot DIR [FILE]`: the window to PNG without a compositor.
//!
//! The app runs in `freya-testing`'s software renderer through
//! `bingux_ui::snapshot`. It writes `preview.png` for FILE (an empty window
//! when none is given). `BINGUX_PREVIEW_SNAPSHOT_SIZE="WxH"` picks the window
//! size, `BINGUX_PREVIEW_SNAPSHOT_SETTLE_MS` how long to wait for the file to
//! load (default 2500), and `BINGUX_PREVIEW_SNAPSHOT_SCRIPT` one walk of
//! commands (see `Snapshot::run_script`; `@path` reads them from a file) that is
//! taken before the capture: the way a check opens the sidebar or a search.

use std::ffi::OsStr;
use std::sync::Arc;
use std::time::Duration;

use bingux_ui::nav::{Location, provide_router};
use bingux_ui::prelude::*;
use bingux_ui::snapshot::Snapshot;
use bingux_ui::style::{Paint, Tw, bg, h_full, w_full};
use freya::prelude::*;

use crate::{app, launch};

/// The window size when `BINGUX_PREVIEW_SNAPSHOT_SIZE` is not set.
const SIZE: (f32, f32) = (1100.0, 760.0);

pub fn run(dir: &str, file: Option<&str>) {
    let size = std::env::var("BINGUX_PREVIEW_SNAPSHOT_SIZE")
        .ok()
        .and_then(|text| {
            let (w, h) = text.split_once('x')?;
            Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
        })
        .unwrap_or(SIZE);
    let address = match file {
        Some(file) => launch::normalise(OsStr::new(file)),
        None => String::new(),
    };
    let location = if address.is_empty() {
        Location::rooted(launch::SCHEME)
    } else {
        Location::parse(&address).unwrap_or_else(|| Location::rooted(launch::SCHEME))
    };
    let routes = Arc::new(
        bingux_ui::nav::RouteTable::new(launch::SCHEME)
            .rooted()
            .mode(bingux_ui::nav::Mode::Linear)
            .root("*path"),
    );
    let mut shot = Snapshot::new(
        move || {
            let tokens = bingux_ui::theme::load_tokens();
            provide_router(routes.clone(), &location);
            Element::from(
                rect()
                    .tw(&tokens, &[w_full(), h_full(), bg(Paint::Bg)])
                    .child(app::App),
            )
        },
        size,
    );
    let settle_ms = std::env::var("BINGUX_PREVIEW_SNAPSHOT_SETTLE_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(2500);
    shot.settle(Duration::from_millis(settle_ms));
    if let Ok(script) = std::env::var("BINGUX_PREVIEW_SNAPSHOT_SCRIPT") {
        let script = match script.strip_prefix('@') {
            Some(path) => std::fs::read_to_string(path).unwrap_or_else(|error| {
                eprintln!("[snapshot] cannot read {path}: {error}");
                String::new()
            }),
            None => script,
        };
        if let Err(error) = shot.run_script(&script, "bingux-preview") {
            eprintln!("[snapshot] script stopped: {error}");
        }
    }
    let path = format!("{dir}/preview.png");
    shot.save(&path);
    println!("[snapshot] wrote {path}");
}
