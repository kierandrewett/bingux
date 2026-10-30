//! `bingux-taxonomy --snapshot DIR [PAGE,PAGE...]`: every page to PNG.
//!
//! No compositor is needed: the app runs in `freya-testing`'s software
//! renderer through `bingux_ui::snapshot`. For each page it writes
//!
//! * `<category>_<page>.png`, at desktop size and as tall as the page is,
//! * `<category>_<page>.phone.png`, at 390 x 800 (the phone preview size),
//! * `<category>_<page>.<entry>-<state>.png` for every state whose spec
//!   carries a script (a menu open, a dialog up), taken at desktop size.
//!
//! The environment of `Env::initial` (`BINGUX_TAXONOMY_THEME` and the rest)
//! picks the theme, accent, density and scale, and `BINGUX_SNAPSHOT_SCALE`
//! the render scale and `BINGUX_SNAPSHOT_SETTLE_MS` the settle time (900 ms), so a check can render the same pages in any combination
//! and diff the files.

use std::time::Duration;

use bingux_ui::nav::provide_router;
use bingux_ui::prelude::*;
use bingux_ui::snapshot::Snapshot;
use bingux_ui::theme::PHONE_PREVIEW_SIZE;
use freya::prelude::*;

use crate::app::{self, location_of};
use crate::model::{Body, PageId, pages, slug};

/// Width of the desktop capture: the sidebar and a full 880 px page column.
const DESKTOP_WIDTH: f32 = 1320.0;
/// The window height the first pass measures the page with.
const PROBE_HEIGHT: f32 = 800.0;
/// The tallest capture, so a runaway page cannot fill the disk.
const MAX_HEIGHT: f32 = 12000.0;

/// How long a page settles before it is captured: 900 ms, or
/// `BINGUX_SNAPSHOT_SETTLE_MS` on a busy machine, where the icons of a long
/// page are still being rasterised on their workers after the default.
fn settle_time() -> Duration {
    let ms = std::env::var("BINGUX_SNAPSHOT_SETTLE_MS").ok().and_then(|raw| raw.parse().ok());
    Duration::from_millis(ms.unwrap_or(900))
}

pub fn run(dir: &str, only: &[String]) {
    let selected: Vec<PageId> = PageId::all()
        .into_iter()
        .filter(|page| {
            only.is_empty()
                || only.iter().any(|name| {
                    let [category, title] = page.segments();
                    name == &format!("{category}/{title}")
                        || name == &format!("{category}_{title}")
                        || name == &category
                })
        })
        .collect();
    if selected.is_empty() {
        eprintln!("[snapshot] no page matches {only:?}");
        std::process::exit(2);
    }
    for page in selected {
        // BINGUX_SNAPSHOT_SCRIPT="..." walks the page instead of capturing it:
        // the script of `Snapshot::run_script` runs at desktop size after the
        // page has settled (`{out}` is DIR/<page> in a `save`), with every
        // frame recorded to BINGUX_SNAPSHOT_FRAMES/<page> when that is set.
        // `scripts/number-roll-check.py` and `scripts/loop-check.py` use it.
        if let Ok(script) = std::env::var("BINGUX_SNAPSHOT_SCRIPT") {
            let name = file_stem(page);
            let mut shot = open(page, (DESKTOP_WIDTH, PROBE_HEIGHT * 2.0));
            if let Some(frames) = std::env::var_os("BINGUX_SNAPSHOT_FRAMES") {
                shot.record_frames(std::path::Path::new(&frames).join(&name));
            }
            shot.settle(settle_time());
            let script = script.replace("{out}", &format!("{dir}/{name}"));
            if let Err(error) = shot.run_script(&script, app::SCHEME) {
                eprintln!("[snapshot] {name}: script stopped: {error}");
            }
            println!("[snapshot] walked {name}");
            continue;
        }
        let name = file_stem(page);
        let height = measured_height(page);
        let path = format!("{dir}/{name}.png");
        let mut shot = open(page, (DESKTOP_WIDTH, height));
        shot.settle(settle_time()).save(&path);
        println!("[snapshot] wrote {path}");

        let path = format!("{dir}/{name}.phone.png");
        let mut shot = open(page, PHONE_PREVIEW_SIZE);
        shot.settle(settle_time()).save(&path);
        println!("[snapshot] wrote {path}");

        for entry in &page.info().entries {
            let Body::States(specs) = entry.body else {
                continue;
            };
            for spec in specs.iter().filter(|spec| !spec.script.is_empty()) {
                let path = format!("{dir}/{name}.{}-{}.png", slug(entry.name), slug(spec.label));
                let mut shot = open(page, (DESKTOP_WIDTH, PROBE_HEIGHT * 2.0));
                shot.settle(settle_time());
                let script = spec.script.replace("{out}", &path);
                if let Err(error) = shot.run_script(&script, app::SCHEME) {
                    eprintln!("[snapshot] {path}: script stopped: {error}");
                }
                println!("[snapshot] wrote {path}");
            }
        }
    }
}

/// `controls_buttons`: the file stem of a page's capture.
fn file_stem(page: PageId) -> String {
    page.segments().join("_")
}

/// The app at a page's address, in a window of `size`.
pub fn open(page: PageId, size: (f32, f32)) -> Snapshot {
    let location = location_of(page);
    Snapshot::new(
        move || {
            let tokens = bingux_ui::theme::load_tokens();
            provide_router(std::sync::Arc::new(app::table()), &location);
            Element::from(
                rect()
                    .tw(&tokens, &[w_full(), h_full(), bg(Paint::Bg)])
                    .child(app::App),
            )
        },
        size,
    )
}

/// How tall the page is: lay it out once and take the lowest label.
fn measured_height(page: PageId) -> f32 {
    let mut probe = open(page, (DESKTOP_WIDTH, PROBE_HEIGHT));
    probe.settle(settle_time());
    let bottom = probe
        .runner()
        .find_many(|node, element| {
            Label::try_downcast(element)?;
            Some(node.layout().area.max_y())
        })
        .into_iter()
        .fold(PROBE_HEIGHT, f32::max);
    (bottom + 120.0).min(MAX_HEIGHT)
}
