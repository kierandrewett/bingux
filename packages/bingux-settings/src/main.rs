//! Bingux Settings: a Freya application for the Bingux desktop shell.
//!
//! Usage: `bingux-settings [PANEL[/SUBPAGE]]`. `PANEL` is one of the names
//! the shell passes (`network`, `bluetooth`, `sound`, `power`, `users`,
//! `background`, `display`, `desktop`, `search`) or a page name; unknown
//! names open the first page. `SUBPAGE` is `default` or an application's
//! desktop id, under `applications`.

mod app;
mod apps;
mod pages;
mod passwd;
mod routes;
mod search_results;
mod settings_index;

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
        println!("usage: bingux-settings [PANEL[/SUBPAGE]]");
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
    // `PANEL/SUBPAGE` opens a subpage: `applications/default`, or
    // `applications/` followed by an application's desktop id.
    let (panel_name, subpage_name) = panel.split_once('/').unwrap_or((panel.as_str(), ""));
    let initial = PageId::from_panel(panel_name).unwrap_or(PageId::ALL[0]);
    let subpage = (!subpage_name.is_empty())
        .then(|| routes::Route::from_name(subpage_name))
        .flatten();
    let (width, height) = theme::WINDOW_SIZE;
    let (min_width, min_height) = theme::WINDOW_MIN_SIZE;
    // Gnoblin draws the window's corners, border and shadow, so the window is
    // opaque and square.
    launch(
        theme::with_font(LaunchConfig::new())
            .with_plugin(freya::borderless::BorderlessPlugin::new())
            .with_window(
                WindowConfig::new(move || app::App {
                    initial,
                    subpage: subpage.clone(),
                    list_first: panel.is_empty(),
                })
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
    use bingux_ui::style::{Paint, Tw, bg, h_full, w_full};
    let pages: Vec<PageId> = if only.is_empty() {
        PageId::ALL.to_vec()
    } else {
        only.iter()
            .filter_map(|name| PageId::from_panel(name))
            .collect()
    };
    // BINGUX_SETTINGS_SNAPSHOT_SEARCH="text" starts the window with that text
    // in the search box, so the search results can be captured. See
    // `search_results::initial_query`.
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
    // BINGUX_SETTINGS_SNAPSHOT_SUBPAGE="default" or an application's desktop id
    // opens that subpage instead of the panel's main page, for example to
    // capture an application's page.
    let subpage = std::env::var("BINGUX_SETTINGS_SNAPSHOT_SUBPAGE")
        .ok()
        .and_then(|name| routes::Route::from_name(&name));
    for page in pages {
        let subpage = subpage.clone();
        let path = format!("{dir}/{}.png", page.panel_name());
        let mut shot = bingux_ui::snapshot::Snapshot::new(
            move || {
                let tokens = theme::load_tokens();
                Element::from(
                    rect()
                        .tw(&tokens, &[w_full(), h_full(), bg(Paint::Bg)])
                        .child(app::App {
                            initial: page,
                            subpage: subpage.clone(),
                            list_first: std::env::var_os("BINGUX_SETTINGS_SNAPSHOT_LIST").is_some(),
                        }),
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
        // BINGUX_SETTINGS_SNAPSHOT_PRESCROLL="x,y,dy" scrolls before the clicks,
        // to bring a control that sits below the fold within reach.
        if let Some([x, y, dy]) = snapshot_numbers::<3>("BINGUX_SETTINGS_SNAPSHOT_PRESCROLL") {
            shot.scroll(x, y, dy)
                .settle(std::time::Duration::from_millis(500));
        }
        for (x, y) in &clicks {
            shot.click(*x, *y)
                .settle(std::time::Duration::from_millis(1000));
        }
        // BINGUX_SETTINGS_SNAPSHOT_RIGHT_CLICK="x,y" right-clicks there, to
        // open a context menu, before the capture.
        if let Some([x, y]) = snapshot_numbers::<2>("BINGUX_SETTINGS_SNAPSHOT_RIGHT_CLICK") {
            shot.right_click(x, y)
                .settle(std::time::Duration::from_millis(1000));
        }
        // BINGUX_SETTINGS_SNAPSHOT_SCROLL="x,y,dy" scrolls the wheel by dy over
        // (x, y), and BINGUX_SETTINGS_SNAPSHOT_HOVER="x,y" then moves the
        // pointer there, before the capture.
        if let Some([x, y, dy]) = snapshot_numbers::<3>("BINGUX_SETTINGS_SNAPSHOT_SCROLL") {
            shot.scroll(x, y, dy)
                .settle(std::time::Duration::from_millis(500));
        }
        if let Some([x, y]) = snapshot_numbers::<2>("BINGUX_SETTINGS_SNAPSHOT_HOVER") {
            shot.hover(x, y)
                .settle(std::time::Duration::from_millis(500));
        }
        shot.save(&path);
        println!("[snapshot] wrote {path}");
    }
}

/// `N` comma-separated numbers from an environment variable, if it holds them.
#[cfg(feature = "snapshot")]
fn snapshot_numbers<const N: usize>(name: &str) -> Option<[f32; N]> {
    let text = std::env::var(name).ok()?;
    let numbers: Vec<f32> = text
        .split(',')
        .filter_map(|part| part.trim().parse().ok())
        .collect();
    numbers.try_into().ok()
}
