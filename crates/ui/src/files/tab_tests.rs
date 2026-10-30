//! Showing a tab, or opening one, changes the file view and nothing around it.
//!
//! The Files window has one body for all its tabs. A tab keeps its own states
//! (where it is, what is selected, its sort and zoom, the path being typed) and
//! a switch is a change of which ones the body reads, in the pass that renders
//! the new tab. These tests mount the body for two or three tabs in the
//! headless runner and step it one pass at a time, the way the windowed app's
//! frames come, to hold that to account:
//!
//! - the toolbar, the places sidebar, the column header, the file view and the
//!   status pill are where they were in every pass after the switch, not only
//!   in the settled one;
//! - the first pass of a tab that was shown before has its rows, at the
//!   scroll position and with the selection it was left with;
//! - a tab opened on the folder on show is whole in its first pass, and one
//!   opened on another folder has the chrome the loaded view has, with only its
//!   rows still to come.
//!
//! The same is checked on the real window by `scripts/files-shift.py`.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use freya::prelude::*;
use freya_testing::prelude::{KeyboardEventName, PlatformEvent};
use torin::prelude::Area;

use super::directory::{forget_kept, remembered};
use super::{Browse, TabMemory, Where, browse};
use crate::shell::OverlayHost;
use crate::snapshot::Snapshot;
use crate::style::*;
use crate::theme::provide_tokens;

const SIZE: (f32, f32) = (1000.0, 600.0);
/// One pass of the runner, as the windowed app runs one per frame.
const FRAME: Duration = Duration::from_millis(16);
/// How many passes after an action are compared: half a second.
const PASSES: usize = 32;

/// The elements of the window that a tab must not move, by the name their
/// component gives them.
const CHROME: [&str; 9] = [
    "FileChooser",
    "AddressBar",
    "AddressCrumbs",
    "PlacesSidebar",
    "PlaceRow",
    "ColumnHeaders",
    "FileView",
    "IconButton",
    "IconMenuButton",
];

/// Folders of the sizes a tab can show: a few files, so no scrollbar, and many.
struct Folders {
    root: PathBuf,
}

impl Folders {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!("bingux-tab-tests-{}-{tag}", std::process::id()));
        for (name, count) in [("short", 6), ("long", 80), ("fresh", 40)] {
            let dir = root.join(name);
            std::fs::create_dir_all(&dir).expect("a folder in the temporary directory");
            for index in 0..count {
                std::fs::write(dir.join(format!("{name}-{index:03}.txt")), name).expect("a file");
            }
        }
        // Nothing of an earlier test stands in for a first listing.
        forget_kept();
        Folders { root }
    }

    fn at(&self, name: &str) -> Where {
        Where::Folder(self.root.join(name))
    }

    fn tab(&self, name: &str) -> TabMemory {
        TabMemory::new(self.at(name))
    }
}

impl Drop for Folders {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The window: the body of the tab that is on show, and Ctrl+Tab for the
/// next tab, as the Files app has it.
#[derive(PartialEq)]
struct Window {
    tabs: Vec<TabMemory>,
}

impl Component for Window {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        let mut active = use_state(|| 0_usize);
        let count = self.tabs.len();
        let memory = self.tabs[active().min(count - 1)].clone();
        rect()
            .tw(&tokens, &[w_full(), h_full(), flex_col(), bg(Paint::Bg)])
            // Global: a key reaches the window whatever has the focus, which a tab
            // switch moves (the focused row of the tab that went is gone).
            .on_global_key_down(move |event: Event<KeyboardEventData>| {
                if event.modifiers.contains(Modifiers::CONTROL) && event.key == Key::Named(NamedKey::Tab) {
                    event.stop_propagation();
                    let next = (*active.peek() + 1) % count;
                    active.set(next);
                }
            })
            .child(OverlayHost::new(
                rect()
                    .tw(&tokens, &[w_full(), h_flex(), flex_col()])
                    .child(browse(Browse::new(memory))),
            ))
    }
}

fn open(tabs: Vec<TabMemory>) -> Snapshot {
    let mut shot = Snapshot::new(move || Window { tabs: tabs.clone() }.into_element(), SIZE);
    shot.settle(Duration::from_millis(1200));
    shot
}

/// Ctrl+Tab, sent and not yet run: the passes that follow are the switch.
fn next_tab(shot: &mut Snapshot) {
    shot.runner().send_event(PlatformEvent::Keyboard {
        name: KeyboardEventName::KeyDown,
        key: Key::Named(NamedKey::Tab),
        code: Code::Unidentified,
        modifiers: Modifiers::CONTROL,
    });
}

/// One pass of the runner.
fn pass(shot: &mut Snapshot) {
    shot.runner().poll_n(FRAME, 1);
}

/// Where each named element of the window is, in order of the tree.
type Chrome = BTreeMap<&'static str, Vec<Area>>;

fn chrome(shot: &mut Snapshot) -> Chrome {
    let found: Vec<(&'static str, Area)> = shot.runner().find_many(|node, element| {
        let data = element.accessibility();
        let name = data.builder.class_name()?;
        let name = CHROME.iter().copied().find(|chrome| *chrome == name)?;
        Some((name, node.layout().area))
    });
    let mut all = Chrome::new();
    for (name, area) in found {
        all.entry(name).or_default().push(area);
    }
    all
}

/// The status pill: its right and bottom edges and its height. Its width is
/// its text, which is the tab's.
fn pill(shot: &mut Snapshot) -> Option<(f32, f32, f32)> {
    let pills: Vec<Area> = shot.runner().find_many(|node, element| {
        (element.accessibility().builder.class_name() == Some("StatusLine")).then(|| node.layout().area)
    });
    pills.first().map(|area| (area.max_x(), area.max_y(), area.height()))
}

/// The names of the files whose rows are drawn, with where each is: `(x, y,
/// name)`, in order from the top.
fn rows_at(shot: &mut Snapshot) -> Vec<(f32, f32, String)> {
    let mut rows: Vec<(f32, f32, String)> = shot.runner().find_many(|node, element| {
        let label = Label::try_downcast(element)?;
        let area = node.layout().area;
        label.text.ends_with(".txt").then(|| (area.min_x(), area.min_y(), label.text.to_string()))
    });
    rows.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.total_cmp(&b.0)));
    rows
}

/// The names of the files whose rows are drawn, in order. Only the name column
/// counts: the hover card of the row under the pointer repeats that row's name
/// (a label of its own, further right), and it is not a row.
fn rows(shot: &mut Snapshot) -> Vec<String> {
    let all = rows_at(shot);
    let column = all.iter().map(|row| row.0).fold(f32::MAX, f32::min);
    all.into_iter().filter(|row| (row.0 - column).abs() < 1.0).map(|(_, _, text)| text).collect()
}

/// Every element of `chrome` that is not where it was in `expected`.
fn moved(expected: &Chrome, now: &Chrome) -> Vec<String> {
    let mut moved = Vec::new();
    for name in CHROME {
        let (was, is) = (expected.get(name), now.get(name));
        if was != is {
            moved.push(format!("{name}: {was:?} became {is:?}"));
        }
    }
    moved
}

#[test]
fn switching_tabs_moves_nothing_around_the_file_view() {
    let folders = Folders::new("switch");
    let mut shot = open(vec![folders.tab("short"), folders.tab("long")]);
    // Both tabs have been on show before the one measured.
    next_tab(&mut shot);
    shot.settle(Duration::from_millis(900));
    next_tab(&mut shot);
    shot.settle(Duration::from_millis(900));
    let before = chrome(&mut shot);
    let pill_before = pill(&mut shot);
    let shown_before = rows(&mut shot);
    assert!(before["PlaceRow"].len() >= 4, "the sidebar lists the places: {before:?}");
    assert!(shown_before.iter().all(|row| row.starts_with("short-")), "the first tab shows its folder: {shown_before:?}");

    next_tab(&mut shot);
    let mut changed = Vec::new();
    for index in 0..PASSES {
        pass(&mut shot);
        let now = chrome(&mut shot);
        for finding in moved(&before, &now) {
            changed.push(format!("pass {index}: {finding}"));
        }
        let pill_now = pill(&mut shot);
        let same_corner = match (pill_before, pill_now) {
            (Some(was), Some(is)) => (was.0, was.1, was.2) == (is.0, is.1, is.2),
            _ => false,
        };
        if !same_corner {
            changed.push(format!("pass {index}: the status pill was {pill_before:?} and is {pill_now:?}"));
        }
    }
    assert!(changed.is_empty(), "persistent elements moved:\n{}", changed.join("\n"));
    let shown_after = rows(&mut shot);
    assert!(shown_after.iter().all(|row| row.starts_with("long-")), "the second tab shows its folder: {shown_after:?}");
}

#[test]
fn the_first_pass_of_a_tab_has_its_rows() {
    let folders = Folders::new("rows");
    let mut shot = open(vec![folders.tab("short"), folders.tab("long")]);
    next_tab(&mut shot);
    shot.settle(Duration::from_millis(900));
    let settled = rows(&mut shot);
    assert!(settled.len() > 8, "a screenful of rows of the long folder: {settled:?}");
    next_tab(&mut shot);
    shot.settle(Duration::from_millis(900));

    next_tab(&mut shot);
    pass(&mut shot);
    assert_eq!(rows(&mut shot), settled, "the first pass of a tab that was shown before has all its rows");
}

#[test]
fn a_tab_comes_back_scrolled_in_its_first_pass() {
    let folders = Folders::new("scroll");
    let mut shot = open(vec![folders.tab("long"), folders.tab("short")]);
    shot.scroll(500.0, 300.0, -900.0);
    shot.settle(Duration::from_millis(900));
    let scrolled = rows(&mut shot);
    assert_ne!(scrolled.first().map(String::as_str), Some("long-000.txt"), "the wheel moved the list: {scrolled:?}");
    next_tab(&mut shot);
    shot.settle(Duration::from_millis(900));

    next_tab(&mut shot);
    pass(&mut shot);
    // The rows of the place it was left at, each once.
    let first_pass = rows(&mut shot);
    assert_eq!(
        first_pass,
        scrolled,
        "the first pass of the tab has the rows it was left with, at the place it was left, not at the top (at {:?})",
        rows_at(&mut shot)
    );
    // And it stays there.
    for _ in 0..PASSES {
        pass(&mut shot);
        assert_eq!(rows(&mut shot), scrolled, "the list moved after its first pass (at {:?})", rows_at(&mut shot));
    }
}

#[test]
fn a_new_tab_on_the_folder_on_show_is_whole_in_its_first_pass() {
    let folders = Folders::new("same");
    let original = folders.tab("long");
    let copy = original.duplicate();
    let mut shot = open(vec![original, copy]);
    let before = chrome(&mut shot);
    let pill_before = pill(&mut shot);
    let settled = rows(&mut shot);
    assert!(settled.len() > 8, "a screenful of rows: {settled:?}");

    next_tab(&mut shot);
    let mut changed = Vec::new();
    for index in 0..PASSES {
        pass(&mut shot);
        for finding in moved(&before, &chrome(&mut shot)) {
            changed.push(format!("pass {index}: {finding}"));
        }
        if pill(&mut shot) != pill_before {
            changed.push(format!("pass {index}: the status pill was {pill_before:?} and is {:?}", pill(&mut shot)));
        }
        if rows(&mut shot) != settled {
            changed.push(format!("pass {index}: the rows are not the folder's rows"));
        }
    }
    assert!(changed.is_empty(), "the new tab differs from the loaded one:\n{}", changed.join("\n"));
}

#[test]
fn a_new_tab_on_another_folder_has_the_chrome_of_the_loaded_view() {
    let folders = Folders::new("other");
    let mut shot = open(vec![folders.tab("short"), folders.tab("fresh")]);
    let before = chrome(&mut shot);

    next_tab(&mut shot);
    let mut changed = Vec::new();
    let mut seen = Vec::new();
    for index in 0..PASSES {
        pass(&mut shot);
        let now = chrome(&mut shot);
        for finding in moved(&before, &now) {
            changed.push(format!("pass {index}: {finding}"));
        }
        seen.push(rows(&mut shot).len());
    }
    assert!(changed.is_empty(), "the chrome of the new tab moved while it loaded:\n{}", changed.join("\n"));
    // The rows come into the places the loaded view has them: once there,
    // they are never fewer than before.
    let mut most = 0;
    for (index, count) in seen.iter().enumerate() {
        assert!(*count >= most, "pass {index}: {count} rows after {most}: the list was emptied again: {seen:?}");
        most = most.max(*count);
    }
    assert!(most > 8, "the folder's rows came: {seen:?}");
}

/// The 20,000-file folder of the real app, made in the temporary directory.
const BIG_FILES: usize = 20_000;

#[test]
fn a_folder_left_while_it_loads_is_read_on_and_whole_when_the_tab_comes_back() {
    let folders = Folders::new("detach");
    let big = folders.root.join("big");
    std::fs::create_dir_all(&big).expect("a folder in the temporary directory");
    for index in 0..BIG_FILES {
        std::fs::write(big.join(format!("big-{index:05}.txt")), "").expect("a file");
    }
    let mut shot = open(vec![folders.tab("short"), folders.tab("big")]);

    // Show the big folder for one pass, then leave: it is not read yet.
    next_tab(&mut shot);
    pass(&mut shot);
    assert!(
        remembered(&folders.at("big")).is_none(),
        "the folder was read before the tab was left: this test needs a folder that takes longer"
    );
    next_tab(&mut shot);
    pass(&mut shot);
    let left_for = rows(&mut shot);
    assert!(left_for.iter().all(|row| row.starts_with("short-")), "the tab was left for the short folder: {left_for:?}");

    // The read goes on unseen and keeps the whole listing.
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    while remembered(&folders.at("big")).is_none() {
        assert!(std::time::Instant::now() < deadline, "the folder left half-read was never read to its end");
        shot.settle(Duration::from_millis(200));
    }
    let whole = remembered(&folders.at("big")).expect("kept");
    assert_eq!(whole.entries.len(), BIG_FILES, "the kept listing is the whole folder");

    // And the tab that comes back to it has its rows in its first pass.
    next_tab(&mut shot);
    pass(&mut shot);
    let first_pass = rows(&mut shot);
    assert!(first_pass.len() > 8, "the first pass of the tab has its rows: {first_pass:?}");
    assert!(first_pass.iter().all(|row| row.starts_with("big-")), "the rows are the big folder's: {first_pass:?}");
}

#[test]
fn what_a_closed_tab_kept_is_given_up_only_when_the_host_lets_go_of_the_tab() {
    use super::chooser::TabBank;

    let mut bank = TabBank::<u32>::default();
    let kept = TabMemory::new(Where::Recent);
    let closed = TabMemory::new(Where::Recent);
    assert_eq!(bank.states(kept.id(), Some(kept.downgrade()), || 1), 1);
    assert_eq!(bank.states(closed.id(), Some(closed.downgrade()), || 2), 2);
    // A tab shown again finds what it left, and `create` does not run.
    assert_eq!(bank.states(kept.id(), Some(kept.downgrade()), || unreachable!()), 1);
    assert!(bank.closed().is_empty(), "both tabs are still the host's");

    // The host keeps a closed tab for Reopen closed tab; letting go of it
    // is what frees its states.
    let kept_alive = closed.clone();
    drop(closed);
    assert!(bank.closed().is_empty(), "a tab the host can reopen keeps its states");
    drop(kept_alive);
    assert_eq!(bank.closed(), vec![2]);
    assert_eq!(bank.states(kept.id(), None, || unreachable!()), 1, "the open tab is untouched");
}

#[test]
fn the_dialog_has_one_bank_entry_that_never_closes() {
    use super::chooser::TabBank;

    let mut bank = TabBank::<u32>::default();
    assert_eq!(bank.states(0, None, || 7), 7);
    assert_eq!(bank.states(0, None, || unreachable!()), 7);
    assert!(bank.closed().is_empty());
}

#[test]
fn the_placeholder_rows_are_as_high_as_the_rows_they_stand_for() {
    use super::file_list::row_height;
    use super::skeleton::skeleton;

    /// The placeholder in a window that has the theme.
    #[derive(PartialEq)]
    struct Scene {
        px: u16,
        viewport: Area,
    }
    impl Component for Scene {
        fn render(&self) -> impl IntoElement {
            let tokens = provide_tokens();
            rect().tw(&tokens, &[w_full(), h_full()]).child(skeleton(self.px, self.viewport))
        }
    }

    for px in [16_u16, 24] {
        let viewport = Area::new((0.0, 0.0).into(), (900.0, 400.0).into());
        let mut shot = Snapshot::new(move || Scene { px, viewport }.into_element(), (900.0, 400.0));
        // The placeholder waits a moment before it draws, so a folder that
        // opens at once never flashes it.
        shot.settle(Duration::from_millis(400));
        let heights: Vec<f32> = shot.runner().find_many(|node, element| {
            (element.accessibility().builder.class_name() == Some("FileSkeleton")).then(|| {
                node.children().iter().map(|row| row.layout().area.height()).collect::<Vec<f32>>()
            })
        })
        .into_iter()
        .flatten()
        .collect();
        assert!(heights.len() >= 8, "{px} px: rows are drawn: {heights:?}");
        // Token sizes are read inside the app's runtime.
        let real = shot.runner().run_in(|| row_height(px));
        for height in &heights {
            assert!((height - real).abs() < 0.5, "{px} px: a row of {height} px, a real one is {real}");
        }
    }
}
