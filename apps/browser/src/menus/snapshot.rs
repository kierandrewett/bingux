//! The snapshot scenarios of menus.

use crate::snapshot::{Rig, Scenario, ctx, scenario, wait};
use bingux_ui::snapshot::Snapshot;

/// The main menu, open, over a page: the rows of the design in its order.
fn menu(shot: &mut Snapshot, _: &Rig) {
    wait(shot, 500);
    shot.click_name("Main menu");
    wait(shot, 500);
}

/// The zoom row after two steps: the level follows the presses in the open menu.
fn menu_zoom(shot: &mut Snapshot, rig: &Rig) {
    menu(shot, rig);
    shot.click_name("Zoom in (Ctrl+=)");
    wait(shot, 300);
    shot.click_name("Zoom in (Ctrl+=)");
    wait(shot, 300);
}

/// The window is Personal, and Work and Study are the other profiles.
fn with_profiles() {
    let mut browser = ctx().browser;
    let mut guard = browser.write();
    guard.profile = Some(("personal".into(), "Personal".into()));
    guard.profiles = vec![
        ("personal".into(), "Personal".into()),
        ("work".into(), "Work".into()),
        ("study".into(), "Study".into()),
    ];
}

fn menu_profile(shot: &mut Snapshot, rig: &Rig) {
    wait(shot, 500);
    with_profiles();
    wait(shot, 300);
    menu(shot, rig);
    shot.click_text("Profile");
    wait(shot, 500);
}

/// History with two tabs closed, so "Recently closed" has rows.
fn menu_history(shot: &mut Snapshot, rig: &Rig) {
    wait(shot, 500);
    for address in [
        "https://peakline.org.uk/routes",
        "https://docs.example.net/guide",
    ] {
        let mut browser = ctx().browser;
        browser
            .write()
            .open_tab(address, crate::tabs::Place::End, true);
        wait(shot, 300);
        let key = ctx().browser.peek().tabs.active_key();
        let mut browser = ctx().browser;
        browser.write().close_tab(key);
        wait(shot, 300);
    }
    menu(shot, rig);
    shot.click_text("History");
    wait(shot, 500);
}

/// Share ›, open.
fn menu_share(shot: &mut Snapshot, rig: &Rig) {
    menu(shot, rig);
    shot.click_text("Share");
    wait(shot, 500);
}

/// More tools ›, open.
fn menu_tools(shot: &mut Snapshot, rig: &Rig) {
    menu(shot, rig);
    shot.click_text("More tools");
    wait(shot, 500);
}

/// A right click on the link of the double's page.
fn context(shot: &mut Snapshot, _: &Rig) {
    wait(shot, 500);
    let (x, y) = *ctx().chrome.page_origin.peek();
    shot.right_click(x + 100.0, y + 290.0);
    wait(shot, 600);
}

/// The scenarios of this feature.
pub(crate) fn scenarios() -> Vec<Scenario> {
    vec![
        scenario("menu", &["example.com"], menu),
        scenario("menu-zoom", &["example.com"], menu_zoom),
        scenario("menu-profile", &["example.com"], menu_profile),
        scenario("menu-history", &["example.com"], menu_history),
        scenario("menu-share", &["example.com"], menu_share),
        scenario("menu-tools", &["example.com"], menu_tools),
        scenario("context", &["example.com"], context),
    ]
}
