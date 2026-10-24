//! Bingux Settings: a Freya application for the Bingux desktop shell.
//!
//! Every page and subpage has an address (see `routes`), such as
//! `bingux-settings://wifi/saved`. The window opens on the address given on
//! the command line, and a second launch hands its address to the running
//! window and exits.

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

use bingux_ui::prelude::Application;

use crate::pages::PageId;

fn main() {
    #[cfg(feature = "snapshot")]
    if std::env::args().nth(1).as_deref() == Some("--snapshot") {
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
    // No argument opens the window on its first page, narrow windows showing
    // the list of pages; an address opens that page or subpage.
    let list_first = std::env::args().nth(1).is_none();
    Application::new("bingux-settings")
        .scheme(routes::SCHEME)
        .title("Settings")
        .borderless()
        .usage(usage())
        .run(move || app::App { list_first });
}

/// The text `--help` prints: the address forms the window understands.
fn usage() -> String {
    let panels: Vec<&str> = PageId::ALL.iter().map(|id| id.panel_name()).collect();
    let mut text = format!(
        "Usage: bingux-settings [ADDRESS]\n\
         \n\
         ADDRESS is bingux-settings://PATH, or the bare PATH. Opening an address\n\
         while Settings runs raises the running window on it; a second launch\n\
         exits at once.\n\
         \n\
         Pages: {}\n\
         \n\
         Subpages:\n\
         \x20 wifi/saved                       saved Wi-Fi networks\n\
         \x20 wifi/network/<ssid>              one Wi-Fi network\n\
         \x20 wired/<device>                   one wired device\n\
         \x20 wired/vpn/<name>                 one VPN profile\n\
         \x20 bluetooth/<mac>                  one Bluetooth device\n\
         \x20 display/<index>                  one display, from 0\n\
         \x20 keyboard/shortcuts               keyboard shortcuts\n\
         \x20 keyboard/input/<kind>/<id>       one input source\n\
         \x20 datetime/timezone                time zone list\n\
         \x20 background/font                  installed fonts\n\
         \x20 users/<path>                     one user account\n\
         \x20 applications/default             default applications\n\
         \x20 applications/default/<kind>      one kind of file or link\n\
         \x20 applications/<desktop-id>        one application\n",
        panels.join(", ")
    );
    if cfg!(feature = "snapshot") {
        text.push_str(
            "\n\
             Snapshot (feature snapshot):\n\
             \x20 bingux-settings --snapshot DIR [ADDRESS,...]\n\
             \x20                                  render pages to DIR/<address>.png\n",
        );
    }
    text
}

/// Render every page headlessly after its backend has had 4 s to answer.
/// Pages read the real host (nmcli, bluetoothctl, gsettings, bingux-config),
/// so the images show what the machine running this reports.
#[cfg(feature = "snapshot")]
fn snapshot_pages(dir: &str, only: &[String]) {
    use bingux_ui::app::provide_address;
    use bingux_ui::prelude::Address;
    use bingux_ui::style::{Paint, Tw, bg, h_full, w_full};
    use freya::prelude::*;
    // Each name is an address, as on the command line: `wifi`, `wifi/saved`.
    // BINGUX_SETTINGS_SNAPSHOT_ADDRESS="wifi/saved" renders that one address
    // (it overrides the list), so a page can be captured without a list.
    let addresses: Vec<Address> = if let Ok(text) = std::env::var("BINGUX_SETTINGS_SNAPSHOT_ADDRESS")
    {
        match Address::parse_in(routes::SCHEME, &text) {
            Some(address) => vec![address],
            None => {
                eprintln!("[snapshot] not an address: {text}");
                return;
            }
        }
    } else if only.is_empty() {
        PageId::ALL.iter().map(|id| id.address()).collect()
    } else {
        only.iter()
            .filter_map(|name| {
                let address = Address::parse_in(routes::SCHEME, name);
                if address.is_none() {
                    eprintln!("[snapshot] not an address: {name}");
                }
                address
            })
            .collect()
    };
    // BINGUX_SETTINGS_SNAPSHOT_SEARCH="text" starts the window with that text
    // in the search box, so the search results can be captured. See
    // `search_results::initial_query`.
    // BINGUX_SETTINGS_SNAPSHOT_CLICK="x,y[;x,y...]" clicks those points
    // after the page has loaded, before the capture, so a write path can be
    // driven and checked from a script. Each click waits
    // BINGUX_SETTINGS_SNAPSHOT_CLICK_SETTLE_MS before the next step.
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
    for address in addresses {
        let path = format!("{dir}/{}.png", snapshot_name(&address));
        let list_first = std::env::var_os("BINGUX_SETTINGS_SNAPSHOT_LIST").is_some();
        let page = address.clone();
        let mut shot = bingux_ui::snapshot::Snapshot::new(
            move || {
                let tokens = theme::load_tokens();
                provide_address(page.clone());
                Element::from(
                    rect()
                        .tw(&tokens, &[w_full(), h_full(), bg(Paint::Bg)])
                        .child(app::App { list_first }),
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
        // BINGUX_SETTINGS_SNAPSHOT_CLICK_SETTLE_MS sets how long to wait after
        // each click (default 1000). Set it to 60, 120 or 220 to capture a
        // popup part-way through its open or close animation.
        let click_settle_ms = std::env::var("BINGUX_SETTINGS_SNAPSHOT_CLICK_SETTLE_MS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(1000);
        for (x, y) in &clicks {
            shot.click(*x, *y)
                .settle(std::time::Duration::from_millis(click_settle_ms));
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
        // BINGUX_SETTINGS_SNAPSHOT_KEYS="escape,escape" presses those keys, in
        // order, after the clicks, to close a popup or a menu from a script.
        if let Ok(keys) = std::env::var("BINGUX_SETTINGS_SNAPSHOT_KEYS") {
            for name in keys.split(',').map(str::trim).filter(|name| !name.is_empty()) {
                let key = match name {
                    "escape" => freya::prelude::Key::Named(freya::prelude::NamedKey::Escape),
                    "return" => freya::prelude::Key::Named(freya::prelude::NamedKey::Enter),
                    "tab" => freya::prelude::Key::Named(freya::prelude::NamedKey::Tab),
                    other => {
                        eprintln!("[snapshot] unknown key: {other}");
                        continue;
                    }
                };
                shot.key(key).settle(std::time::Duration::from_millis(1000));
            }
        }
        // BINGUX_SETTINGS_SNAPSHOT_DRAG="x1,y1,x2,y2" drags from the first point
        // to the second, after the clicks and before the capture, for a slider
        // or a drag handle.
        if let Some([x1, y1, x2, y2]) = snapshot_numbers::<4>("BINGUX_SETTINGS_SNAPSHOT_DRAG") {
            shot.drag((x1, y1), (x2, y2))
                .settle(std::time::Duration::from_millis(500));
        }
        shot.save(&path);
        println!("[snapshot] wrote {path}");
    }
}

/// The file name of an address's snapshot: its segments joined by `_`, so
/// `wifi` is `wifi.png` and `wifi/saved` is `wifi_saved.png`.
#[cfg(feature = "snapshot")]
fn snapshot_name(address: &bingux_ui::prelude::Address) -> String {
    if address.segments().is_empty() {
        return "root".to_string();
    }
    address.segments().join("_")
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
