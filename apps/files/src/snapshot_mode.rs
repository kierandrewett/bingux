//! `bingux-files --snapshot DIR [FOLDER]`: the window to PNG without a compositor.
//!
//! The app runs in `freya-testing`'s software renderer through
//! `bingux_ui::snapshot`, so the checks that read a capture (`scripts/tooltip-check.py`
//! hovers every named icon button of each) cover the Files window too. It
//! writes `files.png` for the folder (the home folder when none is given),
//! `files-tabs.png` after Ctrl+T has opened a second tab, so the tab strip has
//! its close buttons, and `files-preview.png` with the preview pane open.
//! `BINGUX_FILES_SNAPSHOT_SIZE="WxH"` picks the window size, and
//! `BINGUX_FILES_SNAPSHOT_SCRIPT` replaces the three captures by one that is
//! taken after a script (for a dialog, say).

use std::ffi::OsStr;
use std::sync::Arc;
use std::time::Duration;

use bingux_ui::nav::provide_router;
use bingux_ui::prelude::*;
use bingux_ui::snapshot::Snapshot;
use bingux_ui::style::{Paint, Tw, bg, h_full, w_full};
use freya::prelude::*;

use crate::app;

/// The window size when `BINGUX_FILES_SNAPSHOT_SIZE` is not set.
const SIZE: (f32, f32) = (1100.0, 760.0);

pub fn run(dir: &str, folder: Option<&str>) {
    let size = std::env::var("BINGUX_FILES_SNAPSHOT_SIZE")
        .ok()
        .and_then(|text| {
            let (w, h) = text.split_once('x')?;
            Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
        })
        .unwrap_or(SIZE);
    let address = bingux_ui::files::launch::normalise(OsStr::new(folder.unwrap_or("~")));
    let location = Location::parse(&address).unwrap_or_else(|| {
        eprintln!("[snapshot] not a location: {address}");
        std::process::exit(2);
    });
    let mut shot = Snapshot::new(
        move || {
            let tokens = bingux_ui::theme::load_tokens();
            provide_router(Arc::new(bingux_ui::files::folder_table()), &location);
            Element::from(
                rect()
                    .tw(&tokens, &[w_full(), h_full(), bg(Paint::Bg)])
                    .child(app::App),
            )
        },
        size,
    );
    // BINGUX_FILES_SNAPSHOT_SETTLE_MS shortens the wait (default 1500), so a
    // script can capture the first frames of the listing before it has landed
    // (`scripts/structure-stability.py`).
    let settle_ms = std::env::var("BINGUX_FILES_SNAPSHOT_SETTLE_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1500);
    shot.settle(Duration::from_millis(settle_ms));
    // BINGUX_FILES_SNAPSHOT_SCRIPT runs one walk of commands (see
    // `Snapshot::run_script`) and writes `files.png` after it, instead of the
    // three captures below: the way a check opens a dialog first. The value is
    // the commands or "@path" to a file of them.
    if let Ok(script) = std::env::var("BINGUX_FILES_SNAPSHOT_SCRIPT") {
        let script = match script.strip_prefix('@') {
            Some(file) => std::fs::read_to_string(file).unwrap_or_else(|error| {
                eprintln!("[snapshot] cannot read {file}: {error}");
                String::new()
            }),
            None => script,
        };
        if let Err(error) = shot.run_script(&script, "bingux-files") {
            eprintln!("[snapshot] script stopped: {error}");
        }
        let path = format!("{dir}/files.png");
        shot.save(&path);
        println!("[snapshot] wrote {path}");
        return;
    }
    for (name, script) in [
        ("files", ""),
        ("files-tabs", "key ctrl+t; wait 800"),
        ("files-preview", "key alt+p; wait 800"),
    ] {
        if let Err(error) = shot.run_script(script, "bingux-files") {
            eprintln!("[snapshot] {name}: script stopped: {error}");
        }
        let path = format!("{dir}/{name}.png");
        shot.save(&path);
        println!("[snapshot] wrote {path}");
    }
}
