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
    ]
}
