//! Bingux Settings: a Freya application for the Bingux desktop shell.
//!
//! Every page and subpage is a location (see `routes`), a URI such as
//! `bingux-settings://network/wifi/saved`. The window opens on the location
//! given on the command line (or by `xdg-open`), and a second launch hands its
//! location to the running window and exits.

mod app;
mod apps;
#[cfg(all(test, feature = "snapshot"))]
mod header_rest_tests;
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
    if std::env::args().nth(1).as_deref() == Some("--search-provider") {
        search_provider();
        return;
    }
    // No argument opens the window at its root location: the list of pages on
    // a narrow window, the first page on a wide one. A location opens that
    // page or subpage, with Back leading to the pages above it.
    Application::new("bingux-settings")
        .routes(routes::table())
        .title("Settings")
        .borderless()
        .about(bingux_ui::app_info!(
            description = "Change Bingux and system settings",
            licence_text = include_str!("../../../COPYING"),
        ))
        .usage(usage())
        .run(|| app::App);
}

/// `bingux-settings --search-provider`: no window. Registers the search
/// indexes and serves them on the app search socket until killed, so Spotlight
/// (or `socat`) can search Settings without it being open. An activation prints
/// the address instead of opening a window.
fn search_provider() {
    use bingux_ui::search;
    settings_index::register();
    search::wait_loaded(std::time::Duration::from_secs(30));
    let config = search::ExternalConfig {
        app_id: "bingux-settings".into(),
        app_version: env!("CARGO_PKG_VERSION").into(),
    };
    match search::serve_external_with(config, |uri| {
        println!("activate {uri}");
        true
    }) {
        Ok(path) => println!("listening {}", path.display()),
        Err(error) => {
            eprintln!("bingux-settings: cannot serve search: {error}");
            std::process::exit(1);
        }
    }
    loop {
        std::thread::park();
    }
}

/// The text `--help` prints: the locations the window understands.
fn usage() -> String {
    let panels: Vec<&str> = PageId::ALL.iter().map(|id| id.panel_name()).collect();
    let mut text = format!(
        "Usage: bingux-settings [LOCATION]\n\
         \n\
         LOCATION is a URI, bingux-settings://PATH[?QUERY][#ROW], or the bare PATH.\n\
         Path segments go into the hierarchy, a query holds one-off state, and the\n\
         fragment names a row on the page to scroll to. Opening a location while\n\
         Settings runs raises the running window on it; a second launch exits at\n\
         once. `xdg-open bingux-settings://network/proxy` does the same.\n\
         \n\
         bingux-settings --search-provider\n\
         \x20   serve the search indexes without a window (see docs/bingux-ui.md, Search)\n\
         \n\
         Pages: {}\n\
         \n\
         Subpages:\n\
         \x20 network/wifi                     Wi-Fi networks (also: wifi)\n\
         \x20 network/wifi/saved               saved Wi-Fi networks\n\
         \x20 network/wifi/network/<ssid>      one Wi-Fi network (below saved/<ssid> from the saved list)\n\
         \x20 network/hotspot                  Wi-Fi hotspot\n\
         \x20 network/ethernet/<device>        one wired device and the profile it uses (also: wired/<device>)\n\
         \x20 network/ethernet/<device>/profile\n\
         \x20                                  the profiles of a wired device\n\
         \x20 network/ethernet/<device>/profile/<name>\n\
         \x20                                  a wired profile that is not in use\n\
         \x20 network/vpn/<name>               one VPN (also: wired/vpn/<name>)\n\
         \x20 network/add-vpn[/<kind>]         add a VPN\n\
         \x20 network/mobile                   mobile network (also: mobile, wwan, cellular)\n\
         \x20 network/mobile/advanced          carrier, access points, SIM and modem\n\
         \x20 network/proxy                    proxy\n\
         \x20 network/usage                    data usage\n\
         \x20 network/details/<connection>     opens the connection's advanced page\n\
         \x20 <connection page>/advanced       the connection's editors and technical details\n\
         \x20 <connection page>/advanced/<s>   ipv4, ipv6, dns, security, identity, proxy, options\n\
         \x20 <connection page>/usage          the connection's data usage\n\
         \x20 <vpn page>/tunnel                a VPN's tunnel\n\
         \x20 bluetooth/<mac>                  one Bluetooth device\n\
         \x20 display/<index>                  one display, from 0\n\
         \x20 keyboard/shortcuts               keyboard shortcuts\n\
         \x20 keyboard/input/<kind>/<id>       one input source\n\
         \x20 datetime/timezone                time zone list\n\
         \x20 background/font                  installed fonts\n\
         \x20 sound/applications               volume of each application\n\
         \x20 sound/effects                    sound effects\n\
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
             \x20 bingux-settings --snapshot DIR [LOCATION,...]\n\
             \x20                                  render pages to DIR/<location>.png\n",
        );
    }
    text
}

/// Render every page headlessly after its backend has had 4 s to answer.
/// Pages read the real host (nmcli, bluetoothctl, gsettings, bingux-config),
/// so the images show what the machine running this reports.
#[cfg(feature = "snapshot")]
fn snapshot_pages(dir: &str, only: &[String]) {
    use bingux_ui::prelude::Location;
    // Each name is a location, as on the command line: `network`,
    // `network/wifi/saved`. BINGUX_SETTINGS_SNAPSHOT_ADDRESS="network/wifi/saved"
    // renders that one location (it overrides the list), so a page can be
    // captured without a list.
    let addresses: Vec<Location> =
        if let Ok(text) = std::env::var("BINGUX_SETTINGS_SNAPSHOT_ADDRESS") {
            match Location::parse_in(routes::SCHEME, &text) {
                Some(location) => vec![location],
                None => {
                    eprintln!("[snapshot] not a location: {text}");
                    return;
                }
            }
        } else if only.is_empty() {
            PageId::ALL.iter().map(|id| id.location()).collect()
        } else {
            only.iter()
                .filter_map(|name| {
                    let location = Location::parse_in(routes::SCHEME, name);
                    if location.is_none() {
                        eprintln!("[snapshot] not a location: {name}");
                    }
                    location
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
        // The root address is the list of pages; a page address is that page.
        let list = std::env::var_os("BINGUX_SETTINGS_SNAPSHOT_LIST").is_some();
        let page = if list {
            bingux_ui::prelude::Location::new(routes::SCHEME)
        } else {
            address.clone()
        };
        // BINGUX_SETTINGS_SNAPSHOT_TIMING=N mounts the page N times and prints
        // what the mount frame cost on the UI thread: no window, no slide, no
        // GPU, so the figure is the render, diff, layout and text work alone.
        if let Some(rounds) = std::env::var("BINGUX_SETTINGS_SNAPSHOT_TIMING")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
        {
            snapshot_timing(&address, &page, size, rounds);
            continue;
        }
        let mut shot = bingux_ui::snapshot::Snapshot::new(snapshot_app(page), size);
        // BINGUX_SETTINGS_SNAPSHOT_FRAMES=DIR records every frame of the run,
        // from the first load on, to DIR/<page>/ as NNNNNN.png plus
        // frames.jsonl. Used by scripts/frame-walk.py to find layout shifts.
        if let Some(frames) = std::env::var_os("BINGUX_SETTINGS_SNAPSHOT_FRAMES") {
            shot.record_frames(std::path::Path::new(&frames).join(snapshot_name(&address)));
        }
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
        // BINGUX_SETTINGS_SNAPSHOT_TOUCH="x,y" taps there with one finger, and
        // BINGUX_SETTINGS_SNAPSHOT_LONG_PRESS="x,y" rests a finger there past the
        // long-press threshold, before the capture.
        if let Some([x, y]) = snapshot_numbers::<2>("BINGUX_SETTINGS_SNAPSHOT_TOUCH") {
            shot.touch(x, y)
                .settle(std::time::Duration::from_millis(500));
        }
        if let Some([x, y]) = snapshot_numbers::<2>("BINGUX_SETTINGS_SNAPSHOT_LONG_PRESS") {
            shot.long_press(x, y)
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
        // "shift+tab", "f6" and "shift+f6" drive the focus cycle.
        if let Ok(keys) = std::env::var("BINGUX_SETTINGS_SNAPSHOT_KEYS") {
            for name in keys
                .split(',')
                .map(str::trim)
                .filter(|name| !name.is_empty())
            {
                let (modifiers, name) = match name.strip_prefix("shift+") {
                    Some(rest) => (freya::prelude::Modifiers::SHIFT, rest),
                    None => (freya::prelude::Modifiers::default(), name),
                };
                let key = match name {
                    "escape" => freya::prelude::Key::Named(freya::prelude::NamedKey::Escape),
                    "return" => freya::prelude::Key::Named(freya::prelude::NamedKey::Enter),
                    "tab" => freya::prelude::Key::Named(freya::prelude::NamedKey::Tab),
                    "f6" => freya::prelude::Key::Named(freya::prelude::NamedKey::F6),
                    other => {
                        eprintln!("[snapshot] unknown key: {other}");
                        continue;
                    }
                };
                shot.key_with(key, modifiers)
                    .settle(std::time::Duration::from_millis(1000));
            }
        }
        // BINGUX_SETTINGS_SNAPSHOT_DRAG="x1,y1,x2,y2" drags from the first point
        // to the second, after the clicks and before the capture, for a slider
        // or a drag handle.
        if let Some([x1, y1, x2, y2]) = snapshot_numbers::<4>("BINGUX_SETTINGS_SNAPSHOT_DRAG") {
            shot.drag((x1, y1), (x2, y2))
                .settle(std::time::Duration::from_millis(500));
        }
        // BINGUX_SETTINGS_SNAPSHOT_SCRIPT runs a walk of commands after the
        // knobs above, e.g. "click-text Wi-Fi; wait 400; key escape; address
        // wifi". It is either the commands or "@path" to a file of them. See
        // `Snapshot::run_script` for the commands.
        if let Ok(script) = std::env::var("BINGUX_SETTINGS_SNAPSHOT_SCRIPT") {
            let script = match script.strip_prefix('@') {
                Some(file) => system::read_to_string(file).unwrap_or_else(|error| {
                    eprintln!("[snapshot] cannot read {file}: {error}");
                    String::new()
                }),
                None => script,
            };
            if let Err(error) = shot.run_script(&script, routes::SCHEME) {
                eprintln!("[snapshot] script stopped: {error}");
            }
        }
        shot.save(&path);
        println!("[snapshot] wrote {path}");
    }
}

/// The window root a snapshot mounts for `page`: the app on the page's route.
#[cfg(feature = "snapshot")]
fn snapshot_app(page: bingux_ui::prelude::Location) -> impl Fn() -> freya::prelude::Element {
    use bingux_ui::style::{Paint, Tw, bg, h_full, w_full};
    use freya::prelude::*;
    move || {
        let tokens = theme::load_tokens();
        bingux_ui::nav::provide_router(std::sync::Arc::new(routes::table()), &page);
        Element::from(
            rect()
                .tw(&tokens, &[w_full(), h_full(), bg(Paint::Bg)])
                .child(app::App),
        )
    }
}

/// Opens `address` from a settled page `rounds` times and prints the
/// milliseconds the UI thread spent on the first update after it (the mount:
/// components, diff, layout, text), the first one apart (cold caches) and the
/// median and best of the rest. The page it comes from is the Power page.
#[cfg(feature = "snapshot")]
fn snapshot_timing(
    address: &bingux_ui::prelude::Location,
    page: &bingux_ui::prelude::Location,
    size: (f32, f32),
    rounds: usize,
) {
    use std::time::{Duration, Instant};
    let home = PageId::Power.location();
    let mut shot = bingux_ui::snapshot::Snapshot::new(snapshot_app(home.clone()), size);
    shot.settle(Duration::from_millis(600));
    let mut times: Vec<f64> = Vec::new();
    for _ in 0..rounds.max(2) {
        let started = Instant::now();
        shot.navigate(page.clone());
        times.push(started.elapsed().as_secs_f64() * 1000.0);
        shot.settle(Duration::from_millis(400));
        shot.navigate(home.clone()).settle(Duration::from_millis(400));
    }
    let first = times.remove(0);
    times.sort_by(f64::total_cmp);
    println!(
        "[timing] {:28} first {first:7.1} ms  median {:7.1} ms  best {:7.1} ms",
        snapshot_name(address),
        times[times.len() / 2],
        times[0],
    );
}

/// The file name of a location's snapshot: its segments joined by `_`, so
/// `network` is `network.png` and `network/wifi/saved` is `network_wifi_saved.png`.
#[cfg(feature = "snapshot")]
fn snapshot_name(location: &bingux_ui::prelude::Location) -> String {
    if location.segments().is_empty() {
        return "root".to_string();
    }
    location.segments().join("_")
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
