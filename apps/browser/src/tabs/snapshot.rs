//! The snapshot scenarios of tabs.

use crate::snapshot::{Rig, Scenario, ctx, scenario, wait};
use bingux_ui::snapshot::Snapshot;
use bingux_ui::web_view::TabEvent;

const TABS: &[&str] = &[
    "example.com",
    "news.bingux.dev",
    "video.example.net",
    "crash.example.net",
    "dialog.example.net",
    "docs.example.org/a-page-with-a-very-long-title-that-will-not-fit",
    "plain.example.net",
    "files.example.net",
];

/// Forty tabs, enough that the strip scrolls at the desktop size. The last repeats the
/// first, so the list has a duplicate to offer to close.
const FORTY: &[&str] = &[
    "example.com",
    "news.bingux.dev",
    "video.example.net",
    "docs.example.org",
    "files.example.net",
    "mail.example.com",
    "maps.example.org",
    "radio.example.net",
    "library.example.org",
    "cycles.example.com",
    "parkin.example.org",
    "recipes.example.net",
    "weather.example.com",
    "trains.example.org",
    "forum.example.net",
    "bakery.example.com",
    "tram.example.org",
    "bins.example.net",
    "hall.example.org",
    "coffee.example.com",
    "wall.example.net",
    "bikes.example.org",
    "guide.example.com",
    "notes.example.net",
    "crates.example.org",
    "lumen.example.dev",
    "text.example.dev",
    "shop.example.com",
    "events.example.org",
    "clubs.example.net",
    "pubs.example.com",
    "walks.example.org",
    "tools.example.net",
    "paper.example.com",
    "chess.example.org",
    "gigs.example.net",
    "kiln.example.com",
    "loom.example.org",
    "mill.example.net",
    "example.com",
];

/// The tab strip with every state a tab can be in.
fn tabs(shot: &mut Snapshot, rig: &Rig) {
    wait(shot, 400);
    let ctx = ctx();
    let (first, second, third) = (Rig::key_of(0), Rig::key_of(1), Rig::key_of(2));
    ctx.with(|browser| {
        browser.set_pinned(first, true);
        browser.set_pinned(second, true);
        browser.select(third);
    });
    rig.tell(
        2,
        TabEvent::Audio(bingux_ui::web_view::Audio {
            playing: true,
            muted: false,
        }),
    );
    rig.tell(
        7,
        TabEvent::Audio(bingux_ui::web_view::Audio {
            playing: true,
            muted: true,
        }),
    );
    wait(shot, 500);
}

/// Three tabs: they fit, so the strip has no arrows, and the list button is there all the same.
fn few(shot: &mut Snapshot, _: &Rig) {
    wait(shot, 500);
}

/// Forty tabs, two of them pinned, one playing sound and one muted, the first on show: the
/// strip scrolls, and the arrow that goes back is dimmed.
fn forty(shot: &mut Snapshot, rig: &Rig) {
    wait(shot, 500);
    let (first, second) = (Rig::key_of(0), Rig::key_of(1));
    ctx().with(|browser| {
        browser.set_pinned(first, true);
        browser.set_pinned(second, true);
    });
    rig.tell(
        1,
        TabEvent::Audio(bingux_ui::web_view::Audio {
            playing: true,
            muted: false,
        }),
    );
    rig.tell(
        6,
        TabEvent::Audio(bingux_ui::web_view::Audio {
            playing: true,
            muted: true,
        }),
    );
    wait(shot, 600);
}

/// The same, with the last tab on show: the strip is scrolled to its end and the arrow that goes
/// on is dimmed.
fn forty_end(shot: &mut Snapshot, rig: &Rig) {
    forty(shot, rig);
    let last = Rig::key_of(FORTY.len() - 1);
    ctx().with(|browser| browser.select(last));
    wait(shot, 800);
}

/// The list of every tab open: the selected tab is in the middle of it.
fn list_all(shot: &mut Snapshot, rig: &Rig) {
    forty(shot, rig);
    let selected = Rig::key_of(20);
    ctx().with(|browser| browser.select(selected));
    wait(shot, 600);
    shot.click_name("Search and manage tabs");
    wait(shot, 800);
}

fn tab_menu(shot: &mut Snapshot, rig: &Rig) {
    tabs(shot, rig);
    shot.right_click(300.0, 38.0);
    wait(shot, 500);
}

fn tab_list(shot: &mut Snapshot, _: &Rig) {
    wait(shot, 500);
    // A phone opens the list from the tab count; a desktop window has no such
    // button, so the sheet is opened by the same call the button makes.
    if !shot.click_text("3") {
        let at = ctx();
        shot.runner().run_in(|| crate::tabs::TabsSheet::open(at));
    }
    wait(shot, 1500);
}

fn popup_window(shot: &mut Snapshot, rig: &Rig) {
    wait(shot, 500);
    let opener = Rig::session(0);
    rig.double
        .open_window(opener, "https://news.bingux.dev/popup");
    wait(shot, 600);
}

/// The scenarios of this feature.
pub(crate) fn scenarios() -> Vec<Scenario> {
    vec![
        scenario("tabs", TABS, tabs),
        scenario("tab-menu", TABS, tab_menu),
        scenario(
            "tab-list",
            &["example.com", "news.bingux.dev", "video.example.net"],
            tab_list,
        ),
        scenario("popup", &["example.com"], popup_window),
        scenario(
            "tabs-few",
            &["example.com", "news.bingux.dev", "docs.example.org"],
            few,
        ),
        scenario("tabs-forty", FORTY, forty),
        scenario("tabs-forty-end", FORTY, forty_end),
        scenario("tab-list-all", FORTY, list_all),
        Scenario {
            private: true,
            ..scenario("tab-list-private", FORTY, list_all)
        },
    ]
}
