//! The main menu, pressed in a window over the engine's test double: its rows in the
//! order of the design, what each one does, and what a row of a feature that is not
//! built does (nothing, and says why).

use bingux_ui::snapshot::Snapshot;
use freya::prelude::*;

use crate::browser::UiRequest;
use crate::browser::testing::browser;
use crate::snapshot::{self, Rig, ctx, wait};
use crate::tabs::TabKind;

fn window() -> (Snapshot, Rig) {
    snapshot::window(&["example.com"])
}

/// Open the main menu with a click on its button.
fn open(shot: &mut Snapshot) {
    assert!(shot.click_name("Main menu"), "the main menu button");
    wait(shot, 500);
}

/// Whether a label with exactly `text` is drawn.
fn shows(shot: &mut Snapshot, text: &str) -> bool {
    !tops(shot, text).is_empty()
}

/// The top edge of each label that reads `text`.
fn tops(shot: &mut Snapshot, text: &str) -> Vec<f32> {
    shot.runner().find_many(|node, element| {
        let label = Label::try_downcast(element)?;
        (label.text == text).then(|| node.layout().area.min_y())
    })
}

/// Whether a label that starts with `text` is drawn.
fn says(shot: &mut Snapshot, text: &str) -> bool {
    !shot
        .runner()
        .find_many(|_, element| {
            Label::try_downcast(element).filter(|label| label.text.starts_with(text))
        })
        .is_empty()
}

fn zoom() -> u32 {
    ctx().browser.peek().active().expect("a tab").zoom.percent()
}

fn tab_count() -> usize {
    ctx().browser.peek().tabs.len()
}

fn active_is(check: impl Fn(&crate::tabs::Tab) -> bool) -> bool {
    ctx().browser.peek().active().is_some_and(check)
}

#[test]
fn the_rows_are_the_designs_in_the_designs_order() {
    let (mut shot, _rig) = window();
    open(&mut shot);
    let order = [
        "Zoom",
        "New tab",
        "New window",
        "New private window",
        "Profile",
        "History",
        "Bookmarks",
        "Downloads",
        "Passwords",
        "Find in page",
        "Save page as PDF",
        "Share",
        "More tools",
        "Settings",
    ];
    let mut last = f32::MIN;
    for title in order {
        let at = tops(&mut shot, title);
        assert_eq!(at.len(), 1, "one row reads {title}");
        assert!(at[0] > last, "{title} comes after the row before it");
        last = at[0];
    }
}

#[test]
fn the_zoom_row_steps_the_zoom_and_leaves_the_menu_open() {
    let (mut shot, _rig) = window();
    open(&mut shot);
    assert_eq!(zoom(), 100);
    assert!(shows(&mut shot, "100%"));
    assert!(shot.click_name("Zoom in (Ctrl+=)"));
    wait(&mut shot, 300);
    assert!(shot.click_name("Zoom in (Ctrl+=)"));
    wait(&mut shot, 300);
    assert_eq!(zoom(), 120);
    assert!(
        shows(&mut shot, "120%"),
        "the level is live in the open menu"
    );
    assert!(shows(&mut shot, "New tab"), "the menu stayed open");
    assert!(shot.click_name("Zoom out (Ctrl+-)"));
    wait(&mut shot, 300);
    assert_eq!(zoom(), 110);
    // The toolbar's zoom chip reads the same: the menu's level is the higher of the two.
    let level = shot.runner().find_many(|node, element| {
        let label = Label::try_downcast(element)?;
        (label.text == "110%").then(|| Area::center(&node.layout().area))
    });
    let level = level
        .into_iter()
        .min_by(|a, b| a.y.total_cmp(&b.y))
        .expect("the level in the menu");
    shot.click(level.x, level.y);
    wait(&mut shot, 300);
    assert_eq!(zoom(), 100);
}

#[test]
fn the_full_screen_button_closes_the_menu_and_goes_full_screen() {
    let (mut shot, _rig) = window();
    open(&mut shot);
    assert!(shot.click_name("Full screen (F11)"));
    wait(&mut shot, 400);
    assert!(ctx().browser.peek().fullscreen);
    assert!(!shows(&mut shot, "New tab"), "the menu closed");
}

#[test]
fn the_keyboard_reaches_the_zoom_buttons_and_runs_them() {
    let (mut shot, _rig) = window();
    open(&mut shot);
    // The first row is Zoom: its first button is Zoom out, then the level, then Zoom in.
    shot.key(Key::Named(NamedKey::ArrowDown));
    shot.key(Key::Named(NamedKey::ArrowRight));
    shot.key(Key::Named(NamedKey::ArrowRight));
    shot.key(Key::Named(NamedKey::Enter));
    wait(&mut shot, 300);
    assert_eq!(zoom(), 110, "Enter on Zoom in");
    shot.key(Key::Named(NamedKey::ArrowLeft));
    shot.key(Key::Named(NamedKey::Enter));
    wait(&mut shot, 300);
    assert_eq!(zoom(), 100, "Left to the level, Enter resets");
    assert!(shows(&mut shot, "New tab"), "the menu is still open");
}

#[test]
fn the_last_row_is_reachable_by_the_keyboard() {
    let (mut shot, _rig) = window();
    open(&mut shot);
    shot.key(Key::Named(NamedKey::End));
    shot.key(Key::Named(NamedKey::Enter));
    wait(&mut shot, 400);
    assert!(
        says(&mut shot, "About"),
        "End reaches the last row, About, and Enter opens it"
    );
}

#[test]
fn new_tab_opens_a_tab() {
    let (mut shot, _rig) = window();
    let before = tab_count();
    open(&mut shot);
    assert!(shot.click_text("New tab"));
    wait(&mut shot, 400);
    assert_eq!(tab_count(), before + 1);
}

#[test]
fn a_row_of_a_feature_that_is_not_built_does_nothing_and_says_why() {
    for (title, reason) in [
        ("New window", "A profile has one window for now"),
        ("Passwords", "Passwords are not built yet"),
    ] {
        let (mut shot, _rig) = window();
        let before = tab_count();
        open(&mut shot);
        assert!(!says(&mut shot, reason), "{title}: no reason until asked");
        assert!(shot.click_text(title), "{title} is a row");
        wait(&mut shot, 300);
        assert_eq!(tab_count(), before, "{title} does nothing");
        assert!(
            shows(&mut shot, "Zoom"),
            "{title}: the menu stays open on a dead row"
        );
        // The press put its row's tip down: the pointer leaves and comes back to rest.
        shot.hover(2.0, 2.0);
        assert!(shot.hover_text(title));
        wait(&mut shot, 900);
        assert!(says(&mut shot, reason), "{title} says why: {reason}");
    }
}

#[test]
fn bookmarks_is_a_dead_submenu_with_its_reason() {
    let (mut shot, _rig) = window();
    open(&mut shot);
    assert!(shot.click_text("Bookmarks"));
    wait(&mut shot, 300);
    shot.hover(2.0, 2.0);
    assert!(shot.hover_text("Bookmarks"));
    wait(&mut shot, 900);
    assert!(says(&mut shot, "Bookmarks are not built yet"));
}

#[test]
fn history_lists_the_closed_tabs_and_a_row_reopens_its_tab() {
    let (mut shot, _rig) = window();
    {
        let mut browser = ctx().browser;
        browser
            .write()
            .open_tab("https://one.example.net/", crate::tabs::Place::End, true);
    }
    wait(&mut shot, 300);
    let key = ctx().browser.peek().tabs.active_key();
    {
        let mut browser = ctx().browser;
        browser.write().close_tab(key);
    }
    wait(&mut shot, 300);
    let tabs = tab_count();
    open(&mut shot);
    assert!(shot.click_text("History"));
    wait(&mut shot, 300);
    assert!(shows(&mut shot, "Show all history"));
    let closed = ctx()
        .browser
        .peek()
        .closed
        .last()
        .cloned()
        .expect("a closed tab");
    let label = if closed.title.trim().is_empty() {
        "one.example.net".to_string()
    } else {
        closed.title
    };
    assert!(shows(&mut shot, &label), "the closed tab is a row: {label}");
    assert!(shot.click_text(&label));
    wait(&mut shot, 400);
    assert_eq!(tab_count(), tabs + 1);
    assert!(ctx().browser.peek().closed.is_empty(), "it left the list");
}

#[test]
fn history_with_nothing_closed_says_so() {
    let (mut shot, _rig) = window();
    open(&mut shot);
    assert!(shot.click_text("History"));
    wait(&mut shot, 300);
    assert!(shows(&mut shot, "No recently closed tabs"));
}

#[test]
fn the_profile_row_names_this_windows_profile_and_lists_the_others() {
    let (mut shot, _rig) = window();
    {
        let mut browser = ctx().browser;
        let mut guard = browser.write();
        guard.profile = Some(("p1".into(), "Personal".into()));
        guard.profiles = vec![
            ("p1".into(), "Personal".into()),
            ("p2".into(), "Work".into()),
        ];
    }
    wait(&mut shot, 300);
    open(&mut shot);
    assert!(
        shows(&mut shot, "Personal"),
        "the profile is the row's detail"
    );
    assert!(shot.click_text("Profile"));
    wait(&mut shot, 300);
    assert!(shows(&mut shot, "Work"), "the other profile is a row");
    assert!(shows(&mut shot, "New profile…"));
}

#[test]
fn choosing_another_profile_asks_for_its_window_and_this_one_asks_for_nothing() {
    let (mut browser, _double) = browser();
    browser.profile = Some(("p1".into(), "Personal".into()));
    browser.open_profile("p1");
    assert_eq!(browser.take_ui(), []);
    browser.open_profile("p2");
    assert_eq!(browser.take_ui(), [UiRequest::OpenProfile("p2".into())]);
}

#[test]
fn clear_history_is_dead_when_there_is_no_history() {
    let (mut shot, _rig) = window();
    open(&mut shot);
    assert!(shot.click_text("History"));
    wait(&mut shot, 300);
    // A window with no profile keeps no history: nothing to clear, nothing to ask.
    assert!(shot.click_text("Clear history…"));
    wait(&mut shot, 300);
    assert!(!shows(&mut shot, "Clear browsing history?"));
    let (mut browser, _double) = browser();
    browser.run(crate::commands::Command::ClearHistory);
    assert_eq!(browser.take_ui(), []);
}

#[test]
fn share_copies_the_address() {
    let (mut shot, _rig) = window();
    open(&mut shot);
    assert!(shot.click_text("Share"));
    wait(&mut shot, 300);
    assert!(shows(&mut shot, "More apps…"));
    assert!(shot.click_text("Copy link"));
    wait(&mut shot, 500);
    // The clipboard of a headless run may refuse: either way the person is told.
    assert!(
        shows(&mut shot, "Page address copied")
            || shows(&mut shot, "Could not copy the page address"),
        "a toast says what happened"
    );
}

#[test]
fn more_tools_view_source_opens_the_source_in_a_tab() {
    let (mut shot, _rig) = window();
    let before = tab_count();
    open(&mut shot);
    assert!(shot.click_text("More tools"));
    wait(&mut shot, 300);
    assert!(shows(&mut shot, "Save page as…"));
    assert!(shot.click_text("View page source"));
    wait(&mut shot, 400);
    assert_eq!(tab_count(), before + 1);
    assert!(active_is(|tab| tab.url.starts_with("view-source:")));
}

#[test]
fn settings_opens_the_settings_tab() {
    let (mut shot, _rig) = window();
    open(&mut shot);
    assert!(shot.click_text("Settings"));
    wait(&mut shot, 400);
    assert!(active_is(|tab| tab.kind == TabKind::Settings));
}

#[test]
fn help_opens_the_keyboard_shortcuts() {
    let (mut shot, _rig) = window();
    open(&mut shot);
    assert!(shot.click_text("Keyboard shortcuts"));
    wait(&mut shot, 600);
    assert!(!shows(&mut shot, "More tools"), "the main menu closed");
    assert!(
        shows(&mut shot, "Keyboard shortcuts"),
        "and the sheet that is titled so is up"
    );
}

mod phone {
    use super::*;

    fn window() -> (Snapshot, Rig) {
        snapshot::window_of(&["example.com"], (390.0, 800.0))
    }

    #[test]
    fn the_button_opens_a_sheet_with_the_designs_tiles_and_rows() {
        let (mut shot, _rig) = window();
        assert!(shot.click_name("Main menu"));
        wait(&mut shot, 600);
        for text in [
            "Forward",
            "Copy link",
            "Bookmark",
            "Find in page",
            "History",
            "Bookmarks",
            "Reading list",
            "Downloads",
            "Translate page",
            "Desktop site",
            "Extensions",
            "Settings",
        ] {
            assert!(shot.hover_text(text), "the sheet has {text}");
        }
        assert!(
            shot.hover_text("The reading list is not built yet"),
            "a dead row says why under its name, as a finger has no tooltip"
        );
    }

    #[test]
    fn a_row_does_its_work_and_closes_the_sheet() {
        let (mut shot, _rig) = window();
        assert!(shot.click_name("Main menu"));
        wait(&mut shot, 600);
        assert!(shot.click_text("Downloads"));
        wait(&mut shot, 500);
        assert!(
            ctx().downloads_popover.is_open(),
            "the list of downloads opened"
        );
        assert!(!shot.hover_text("Reading list"), "the sheet closed");
    }
}
