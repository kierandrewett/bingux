//! Bingux Settings: a Freya application for the Bingux desktop shell.
//!
//! Usage: `bingux-settings [PANEL]`. `PANEL` is one of the names the shell
//! passes (`network`, `bluetooth`, `sound`, `power`, `users`, `background`,
//! `display`, `desktop`, `search`) or a page name; unknown names open the
//! first page.

mod app;
mod pages;

/// Pages were written against these module paths; they map onto bingux-ui.
pub(crate) use bingux_ui::icons;
pub(crate) use bingux_ui::runtime as system;
pub(crate) use bingux_ui::theme;
pub(crate) mod ui {
    pub use bingux_ui::prelude::*;
    pub use bingux_ui::surface::use_tokens;
}

use freya::prelude::*;

use crate::pages::PageId;

fn main() {
    let panel = std::env::args().nth(1).unwrap_or_default();
    if panel == "--help" || panel == "-h" {
        println!("usage: bingux-settings [PANEL]");
        println!(
            "panels: {}",
            PageId::ALL.map(|id| id.panel_name()).join(", ")
        );
        #[cfg(feature = "snapshot")]
        println!(
            "       bingux-settings --snapshot DIR [PANEL,PANEL]   render pages to DIR/<panel>.png"
        );
        return;
    }
    #[cfg(feature = "snapshot")]
    if panel == "--snapshot" {
        let dir = std::env::args()
            .nth(2)
            .unwrap_or_else(|| "/tmp/bingux-settings-snapshots".to_string());
        let only: Vec<String> = std::env::args()
            .nth(3)
            .map(|list| list.split(',').map(str::to_string).collect())
            .unwrap_or_default();
        snapshot_pages(&dir, &only);
        return;
    }
    let initial = PageId::from_panel(&panel).unwrap_or(PageId::ALL[0]);
    let (width, height) = theme::WINDOW_SIZE;
    let (min_width, min_height) = theme::WINDOW_MIN_SIZE;
    // Gnoblin draws the window's corners, border and shadow, so the window is
    // opaque and square.
    launch(
        theme::with_font(LaunchConfig::new())
            .with_plugin(freya::borderless::BorderlessPlugin::new())
            .with_window(
                WindowConfig::new(move || app::App { initial })
                    .with_title("Settings")
                    .with_app_id("bingux-settings")
                    .with_decorations(false)
                    .with_size(width as f64, height as f64)
                    .with_min_size(min_width as f64, min_height as f64),
            ),
    );
}

/// Render every page headlessly after its backend has had 4 s to answer.
/// Pages read the real host (nmcli, bluetoothctl, gsettings, bingux-config),
/// so the images show what the machine running this reports.
#[cfg(feature = "snapshot")]
fn snapshot_pages(dir: &str, only: &[String]) {
    let pages: Vec<PageId> = if only.is_empty() {
        PageId::ALL.to_vec()
    } else {
        only.iter()
            .filter_map(|name| PageId::from_panel(name))
            .collect()
    };
    // BINGUX_SETTINGS_SNAPSHOT_CLICK="x,y[;x,y...]" clicks those points
    // (one second apart) after the page has loaded, before the capture, so
    // a write path can be driven and checked from a script.
    let clicks: Vec<(f32, f32)> = std::env::var("BINGUX_SETTINGS_SNAPSHOT_CLICK")
        .ok()
        .map(|list| {
            list.split(';')
                .filter_map(|point| {
                    let (x, y) = point.split_once(',')?;
                    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
                })
                .collect()
        })
        .unwrap_or_default();
    // BINGUX_SETTINGS_SNAPSHOT_SIZE="WxH" captures at another window size,
    // for the overlay sidebar or for pages taller than the default window.
    let size = std::env::var("BINGUX_SETTINGS_SNAPSHOT_SIZE")
        .ok()
        .and_then(|text| {
            let (w, h) = text.split_once('x')?;
            Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
        })
        .unwrap_or(theme::WINDOW_SIZE);
    for page in pages {
        let path = format!("{dir}/{}.png", page.panel_name());
        let mut shot = bingux_ui::snapshot::Snapshot::new(
            move || {
                Element::from(
                    rect()
                        .expanded()
                        .background(Color::from_rgb(28, 28, 32))
                        .child(app::App { initial: page }),
                )
            },
            size,
        );
        // BINGUX_SETTINGS_SNAPSHOT_SETTLE_MS shortens the wait to capture a page
        // before its data has arrived (the skeleton).
        let settle_ms = std::env::var("BINGUX_SETTINGS_SNAPSHOT_SETTLE_MS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(4000);
        shot.settle(std::time::Duration::from_millis(settle_ms));
        for (x, y) in &clicks {
            shot.click(*x, *y)
                .settle(std::time::Duration::from_millis(1000));
        }
        shot.save(&path);
        println!("[snapshot] wrote {path}");
    }
}
