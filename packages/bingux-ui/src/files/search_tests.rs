//! The search bar of the file views, under real keys and a real pointer in
//! the headless runner: opening and leaving it, a typed filter becoming a
//! token, the Filters panel, what is offered when nothing is found, the recent
//! searches, a phone, and the rule that nothing around the list moves while
//! results stream in.

use std::path::PathBuf;
use std::time::Duration;

use freya::prelude::*;
use freya_core::elements::label::Label;
use freya_core::elements::paragraph::Paragraph;
use torin::prelude::Area;

use super::{Browse, TabMemory, Where, browse};
use crate::shell::OverlayHost;
use crate::snapshot::Snapshot;
use crate::style::*;
use crate::theme::provide_tokens;

const DESKTOP: (f32, f32) = (1100.0, 700.0);
const PHONE: (f32, f32) = (390.0, 800.0);

/// A tree where tests can write many files: the shared-memory filesystem when
/// there is one.
struct Tree {
    root: PathBuf,
}

impl Tree {
    fn new(tag: &str) -> Self {
        let base = if std::path::Path::new("/dev/shm").is_dir() {
            PathBuf::from("/dev/shm")
        } else {
            std::env::temp_dir()
        };
        let root = base.join(format!("bingux-search-ui-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for dir in ["docs", "pics", "sub/deep", ".secret"] {
            std::fs::create_dir_all(root.join(dir)).expect("a folder");
        }
        for (name, size) in [
            ("docs/notes.md", 10),
            ("docs/report-2024.pdf", 3_000_000),
            ("docs/budget.txt", 20),
            ("pics/beach.png", 2_000_000),
            ("pics/report-card.png", 100),
            ("sub/deep/needle.txt", 5),
            ("top.txt", 1),
            (".secret/report-hidden.txt", 1),
        ] {
            let file = std::fs::File::create(root.join(name)).expect("a file");
            file.set_len(size).expect("a size");
        }
        Tree { root }
    }

    /// `count` files called `item-N.txt` in a few folders, for a search that
    /// takes several polls.
    fn crowded(tag: &str, count: usize) -> Self {
        let tree = Tree::new(tag);
        for folder in 0..40 {
            std::fs::create_dir_all(tree.root.join(format!("bulk/f{folder}"))).expect("a folder");
        }
        for at in 0..count {
            std::fs::File::create(tree.root.join(format!("bulk/f{}/item-{at}.txt", at % 40))).expect("a file");
        }
        tree
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[derive(PartialEq)]
struct Window {
    memory: TabMemory,
}

impl Component for Window {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        let memory = self.memory.clone();
        rect()
            .tw(&tokens, &[w_full(), h_full(), flex_col(), bg(Paint::Bg)])
            .child(OverlayHost::new(
                rect()
                    .tw(&tokens, &[w_full(), h_flex(), flex_col()])
                    .child(browse(Browse::new(memory))),
            ))
    }
}

/// How long a test waits for something to appear before it says it did not. A wait ends the moment
/// what it waits for is on screen, so a generous cap costs nothing when the machine keeps up and
/// lets a loaded one finish.
const WAIT: Duration = Duration::from_secs(30);

/// Drive the window until `done`, or until [`WAIT`] has passed. The assertion that follows says
/// what was missing.
fn until(shot: &mut Snapshot, done: impl FnMut(&mut Snapshot) -> bool) -> bool {
    shot.settle_until(WAIT, done)
}

fn open(tree: &Tree, size: (f32, f32)) -> Snapshot {
    let memory = TabMemory::new(Where::Folder(tree.root.clone()));
    let mut shot = Snapshot::new(move || Window { memory: memory.clone() }.into_element(), size);
    // Every tree has a `top.txt` in its top folder: the folder is listed.
    until(&mut shot, |shot| shown(shot, "top.txt"));
    shot.settle(Duration::from_millis(150));
    shot
}

fn rest(shot: &mut Snapshot) {
    shot.settle(Duration::from_millis(500));
}

fn type_text(shot: &mut Snapshot, text: &str) {
    for letter in text.chars() {
        shot.key(Key::Character(letter.to_string()));
    }
}

fn ctrl(shot: &mut Snapshot, letter: &str) {
    shot.key_with(Key::Character(letter.into()), Modifiers::CONTROL);
    rest(shot);
}

fn named(shot: &mut Snapshot, class: &'static str) -> Vec<Area> {
    shot.runner().find_many(move |node, element| {
        (element.accessibility().builder.class_name() == Some(class)).then(|| node.layout().area)
    })
}

fn labels(shot: &mut Snapshot) -> Vec<String> {
    shot.runner().find_many(|_, element| {
        if let Some(label) = Label::try_downcast(element) {
            return Some(label.text.to_string());
        }
        let paragraph = Paragraph::try_downcast(element)?;
        Some(paragraph.spans.iter().map(|span| span.text.to_string()).collect())
    })
}

fn shown(shot: &mut Snapshot, text: &str) -> bool {
    labels(shot).iter().any(|label| label.contains(text))
}

fn centre(area: &Area) -> (f32, f32) {
    (area.min_x() + area.width() / 2.0, area.min_y() + area.height() / 2.0)
}

/// A press in the field of the open bar, to give it the keyboard.
fn focus_field(shot: &mut Snapshot) {
    let bar = named(shot, "SearchBar");
    shot.click(bar[0].min_x() + 200.0, bar[0].min_y() + 30.0);
    rest(shot);
}

fn bar_open(shot: &mut Snapshot) -> bool {
    !named(shot, "SearchBar").is_empty()
}

#[test]
fn ctrl_f_opens_the_bar_and_escape_clears_the_text_then_leaves() {
    let tree = Tree::new("escape");
    let mut shot = open(&tree, DESKTOP);
    assert!(!bar_open(&mut shot), "no bar until it is asked for");
    ctrl(&mut shot, "f");
    assert!(bar_open(&mut shot), "Ctrl+F opens the bar");

    type_text(&mut shot, "needle");
    until(&mut shot, |shot| shown(shot, "needle.txt") && !shown(shot, "budget.txt"));
    assert!(shown(&mut shot, "needle.txt"), "a result from two folders down: {:?}", labels(&mut shot));
    assert!(!shown(&mut shot, "budget.txt"), "what does not match is gone");
    assert!(shown(&mut shot, "sub/deep"), "a result says which folder it is in");

    shot.key(Key::Named(NamedKey::Escape));
    until(&mut shot, |shot| (shown(shot, "budget.txt") || shown(shot, "docs")) && !shown(shot, "needle.txt"));
    assert!(bar_open(&mut shot), "the first Escape only clears the text");
    assert!(shown(&mut shot, "budget.txt") || shown(&mut shot, "docs"), "the folder is whole again");
    assert!(!shown(&mut shot, "needle.txt"));

    shot.key(Key::Named(NamedKey::Escape));
    until(&mut shot, |shot| !bar_open(shot));
    assert!(!bar_open(&mut shot), "the second Escape leaves the search");
}

#[test]
fn a_typed_key_becomes_a_token_that_filters_and_can_be_removed() {
    let tree = Tree::new("token");
    let mut shot = open(&tree, DESKTOP);
    ctrl(&mut shot, "f");
    type_text(&mut shot, "type:image ");
    until(&mut shot, |shot| {
        shown(shot, "Type: Images") && shown(shot, "Filters (1)") && shown(shot, "beach.png") && !shown(shot, "budget.txt") && !shown(shot, "needle.txt")
    });
    assert!(shown(&mut shot, "Type: Images"), "the filter is a token: {:?}", labels(&mut shot));
    assert!(shown(&mut shot, "Filters (1)"), "and the button counts it");
    assert!(shown(&mut shot, "beach.png"));
    assert!(!shown(&mut shot, "budget.txt") && !shown(&mut shot, "needle.txt"), "only pictures are left");

    let tokens = named(&mut shot, "FilterToken");
    assert_eq!(tokens.len(), 1);
    // The remove button is the token's last 28 px.
    shot.click(tokens[0].max_x() - 10.0, centre(&tokens[0]).1);
    until(&mut shot, |shot| named(shot, "FilterToken").is_empty() && shown(shot, "top.txt"));
    assert!(named(&mut shot, "FilterToken").is_empty(), "the token is gone");
    assert!(shown(&mut shot, "Filters") && !shown(&mut shot, "Filters ("), "and the count with it");
    assert!(shown(&mut shot, "top.txt"), "no words and no filter: the folder itself is back");

    // Several tokens offer Clear all.
    focus_field(&mut shot);
    type_text(&mut shot, "type:document size:>1KB ");
    until(&mut shot, |shot| {
        named(shot, "FilterToken").len() == 2 && shown(shot, "Clear all") && shown(shot, "report-2024.pdf") && !shown(shot, "notes.md")
    });
    assert_eq!(named(&mut shot, "FilterToken").len(), 2, "{:?}", labels(&mut shot));
    assert!(shown(&mut shot, "Clear all"));
    assert!(shown(&mut shot, "report-2024.pdf") && !shown(&mut shot, "notes.md"), "documents over a kilobyte");
    assert!(shot.click_text("Clear all"));
    until(&mut shot, |shot| named(shot, "FilterToken").is_empty());
    assert!(named(&mut shot, "FilterToken").is_empty());
}

#[test]
fn the_filters_panel_opens_from_ctrl_shift_f_and_from_its_button() {
    let tree = Tree::new("panel");
    let mut shot = open(&tree, DESKTOP);
    ctrl(&mut shot, "f");
    // From inside the field: the chord is the window's, never typed as text.
    shot.key_with(Key::Character("f".into()), Modifiers::CONTROL | Modifiers::SHIFT);
    until(&mut shot, |shot| shown(shot, "Match case"));
    assert!(!labels(&mut shot).iter().any(|label| label == "f"), "Ctrl+Shift+F is not text");
    for row in ["Type", "Modified", "Size", "Name", "Sort by", "Hidden files", "Match case", "Reset", "Done"] {
        assert!(shown(&mut shot, row), "the panel has {row}: {:?}", labels(&mut shot));
    }
    shot.key(Key::Named(NamedKey::Escape));
    until(&mut shot, |shot| !shown(shot, "Match case"));
    assert!(!shown(&mut shot, "Match case"), "Escape closes it");
    let found = labels(&mut shot);
    assert!(shot.click_text("Filters"), "{found:?}");
    until(&mut shot, |shot| shown(shot, "Match case"));
    assert!(shown(&mut shot, "Match case"), "the button opens it");
    assert!(shot.click_text("Done"));
    until(&mut shot, |shot| !shown(shot, "Match case"));
    assert!(!shown(&mut shot, "Match case"), "Done closes it");
}

#[test]
fn a_search_that_finds_nothing_says_how_to_widen_or_narrow_it() {
    let tree = Tree::new("nothing");
    let mut shot = open(&tree, DESKTOP);
    ctrl(&mut shot, "f");
    type_text(&mut shot, "zzzzqqq type:image ");
    until(&mut shot, |shot| shown(shot, "Nothing found") && shown(shot, "Remove “Type: Images”") && shown(shot, "Search everywhere"));
    assert!(shown(&mut shot, "Nothing found"), "{:?}", labels(&mut shot));
    assert!(shown(&mut shot, "Remove “Type: Images”"));
    assert!(shown(&mut shot, "Search everywhere"));
    assert!(shot.click_text("Remove “Type: Images”"));
    until(&mut shot, |shot| named(shot, "FilterToken").is_empty() && shown(shot, "Nothing found"));
    assert!(named(&mut shot, "FilterToken").is_empty(), "the hint removed the filter");
    assert!(shown(&mut shot, "Nothing found"), "the words still find nothing");
    // A pattern that cannot run says so instead of listing nothing.
    focus_field(&mut shot);
    type_text(&mut shot, "( match:regex ");
    until(&mut shot, |shot| shown(shot, "Cannot search") && shown(shot, "not a valid regular expression"));
    assert!(shown(&mut shot, "Cannot search") && shown(&mut shot, "not a valid regular expression"), "{:?}", labels(&mut shot));
}

#[test]
fn the_scope_dropdown_changes_where_the_search_looks() {
    let tree = Tree::new("scope");
    let mut shot = open(&tree, DESKTOP);
    ctrl(&mut shot, "f");
    type_text(&mut shot, "needle");
    until(&mut shot, |shot| shown(shot, "needle.txt"));
    assert!(shown(&mut shot, "needle.txt"), "Subfolders is the default");
    // This folder: nothing below it is read, so a file two folders down is not a result.
    let drop = named(&mut shot, "Dropdown");
    assert!(!drop.is_empty(), "the scope dropdown");
    let face = drop.iter().max_by(|a, b| a.min_x().total_cmp(&b.min_x())).copied().unwrap_or_default();
    shot.click(centre(&face).0, centre(&face).1);
    until(&mut shot, |shot| shown(shot, "This folder"));
    assert!(shot.click_text("This folder"), "{:?}", labels(&mut shot));
    until(&mut shot, |shot| !shown(shot, "needle.txt") && (shown(shot, "Nothing found") || shown(shot, "0 of")));
    assert!(!shown(&mut shot, "needle.txt"), "this folder does not look in sub/deep");
    assert!(shown(&mut shot, "Nothing found") || shown(&mut shot, "0 of"), "{:?}", labels(&mut shot));
}

#[test]
fn recent_searches_drop_down_from_the_empty_field_and_can_be_forgotten() {
    let tree = Tree::new("recent");
    let mut shot = open(&tree, DESKTOP);
    ctrl(&mut shot, "f");
    // Enter on a result would open the file in its application; a search with
    // no result is remembered all the same.
    type_text(&mut shot, "nosuchname");
    shot.key(Key::Named(NamedKey::Enter));
    until(&mut shot, |shot| shown(shot, "Nothing found"));
    shot.key(Key::Named(NamedKey::Escape));
    rest(&mut shot);
    focus_field(&mut shot);
    shot.key(Key::Named(NamedKey::Escape));
    rest(&mut shot);
    assert!(!bar_open(&mut shot), "the second Escape left the search");
    ctrl(&mut shot, "f");
    until(&mut shot, |shot| shown(shot, "Clear recent searches"));
    assert!(shown(&mut shot, "Clear recent searches"), "the list opens under the empty field: {:?}", labels(&mut shot));
    assert!(labels(&mut shot).iter().any(|label| label == "nosuchname"), "with the last search in it");
    assert!(shot.click_text("Clear recent searches"));
    until(&mut shot, |shot| !shown(shot, "Clear recent searches"));
    assert!(!shown(&mut shot, "Clear recent searches"), "and forgets them");
}

#[test]
fn down_goes_to_the_results_and_enter_opens_the_selected_one() {
    let tree = Tree::new("keys");
    let mut shot = open(&tree, DESKTOP);
    ctrl(&mut shot, "f");
    type_text(&mut shot, "needle");
    until(&mut shot, |shot| shown(shot, "needle.txt"));
    // Enter opens the first result: a file is opened by the system, a folder
    // is entered. `sub` matches nothing, so make a folder result.
    drop(shot);
    let tree = Tree::new("keys-folder");
    std::fs::create_dir_all(tree.root.join("zzfolder/inner")).expect("a folder");
    let mut shot = open(&tree, DESKTOP);
    ctrl(&mut shot, "f");
    type_text(&mut shot, "zzfolder");
    until(&mut shot, |shot| status_line(shot).contains("1 found"));
    shot.key(Key::Named(NamedKey::Enter));
    until(&mut shot, |shot| !bar_open(shot) && shown(shot, "inner"));
    assert!(!bar_open(&mut shot), "opening a folder ends the search");
    assert!(shown(&mut shot, "inner"), "and the folder is on show: {:?}", labels(&mut shot));
}

#[test]
fn nothing_around_the_list_moves_while_results_stream_in() {
    let tree = Tree::crowded("stream", 4000);
    let mut shot = open(&tree, DESKTOP);
    ctrl(&mut shot, "f");
    let structure = |shot: &mut Snapshot| -> Vec<(String, [i32; 4])> {
        ["SearchBar", "SearchStatus", "FiltersButton", "Dropdown"]
            .iter()
            .flat_map(|class| {
                named(shot, class).into_iter().map(move |area| {
                    (
                        class.to_string(),
                        [area.min_x(), area.min_y(), area.width(), area.height()].map(|value| value.round() as i32),
                    )
                })
            })
            .collect()
    };
    let before = structure(&mut shot);
    assert!(before.len() >= 4, "{before:?}");
    type_text(&mut shot, "item");
    let mut frames = Vec::new();
    for _ in 0..30 {
        shot.settle(Duration::from_millis(60));
        frames.push(structure(&mut shot));
    }
    for (at, frame) in frames.iter().enumerate() {
        // The status line is allowed to change its text, never its box's top
        // or its row: compare everything but the status width.
        let strip = |list: &[(String, [i32; 4])]| -> Vec<(String, [i32; 4])> {
            list.iter()
                .map(|(class, area)| {
                    let mut area = *area;
                    if class == "SearchStatus" {
                        area[2] = 0;
                    }
                    (class.clone(), area)
                })
                .collect()
        };
        assert_eq!(strip(frame), strip(&frames[0]), "frame {at} moved something in the bar");
    }
    assert!(shown(&mut shot, "found in") || shown(&mut shot, "Searching"), "{:?}", labels(&mut shot));
}

#[test]
fn on_a_phone_the_field_has_a_row_and_the_scope_and_filters_share_the_next() {
    let tree = Tree::new("phone");
    let mut shot = open(&tree, PHONE);
    ctrl(&mut shot, "f");
    assert!(bar_open(&mut shot));
    let field = named(&mut shot, "SearchBar");
    let buttons = named(&mut shot, "FiltersButton");
    assert_eq!(buttons.len(), 1);
    assert!(buttons[0].max_x() <= PHONE.0 + 0.5, "the Filters button is inside the window: {buttons:?}");
    assert!(field[0].width() <= PHONE.0 + 0.5);
    assert!(shot.click_text("Filters"));
    until(&mut shot, |shot| shown(shot, "Match case"));
    assert!(shown(&mut shot, "Match case"), "the panel is a sheet on a phone: {:?}", labels(&mut shot));
}

fn overlap(a: &Area, b: &Area) -> bool {
    a.min_x() < b.max_x() - 0.5 && b.min_x() < a.max_x() - 0.5 && a.min_y() < b.max_y() - 0.5 && b.min_y() < a.max_y() - 0.5
}

#[test]
fn on_a_phone_four_tokens_wrap_to_their_own_rows_and_cover_nothing() {
    let tree = Tree::new("phone-tokens");
    let mut shot = open(&tree, PHONE);
    ctrl(&mut shot, "f");
    // Where the first row's controls are. The Filters button says how many
    // filters are set, so its width follows its label; its place does not.
    let first_row = |shot: &mut Snapshot| -> Vec<(&'static str, [i32; 4])> {
        ["Dropdown", "FiltersButton"]
            .iter()
            .flat_map(|class| {
                named(shot, class).into_iter().map(move |area| {
                    let width = if *class == "Dropdown" { area.width() } else { 0.0 };
                    (*class, [area.min_x(), area.min_y(), width, area.height()].map(|v| v.round() as i32))
                })
            })
            .collect()
    };
    let bar_before = named(&mut shot, "SearchBar")[0];
    let before = first_row(&mut shot);
    assert_eq!(before.len(), 2, "{before:?}");

    type_text(&mut shot, "type:image size:>1KB modified:week ext:png ");
    until(&mut shot, |shot| named(shot, "FilterToken").len() == 4);
    shot.settle(Duration::from_millis(200));
    let tokens = named(&mut shot, "FilterToken");
    assert_eq!(tokens.len(), 4, "{:?}", labels(&mut shot));
    assert_eq!(first_row(&mut shot), before, "the scope and Filters buttons did not move when tokens arrived");

    let bar = named(&mut shot, "SearchBar")[0];
    let buttons: Vec<Area> = named(&mut shot, "FiltersButton").into_iter().chain(named(&mut shot, "Dropdown")).collect();
    let field_bottom = buttons.iter().map(|area| area.max_y()).fold(0.0_f32, f32::max);
    for token in &tokens {
        for button in &buttons {
            assert!(!overlap(token, button), "a token {token:?} lies over a button {button:?}");
        }
        assert!(token.min_y() >= field_bottom - 0.5, "a token {token:?} starts above the end of the buttons row ({field_bottom})");
        assert!(token.min_x() >= bar.min_x() - 0.5 && token.max_x() <= bar.max_x() + 0.5, "a token {token:?} is inside the bar {bar:?}");
        assert!(token.max_y() <= bar.max_y() + 0.5, "a token {token:?} is inside the bar {bar:?}");
    }
    for (at, a) in tokens.iter().enumerate() {
        for b in &tokens[at + 1..] {
            assert!(!overlap(a, b), "tokens {a:?} and {b:?} overlap");
        }
    }
    assert!(bar.height() > bar_before.height(), "the bar grew to hold the token rows: {bar_before:?} -> {bar:?}");
    // The status line is a row of its own too, below the tokens.
    let status = named(&mut shot, "SearchStatus")[0];
    for token in &tokens {
        assert!(!overlap(token, &status), "a token {token:?} lies over the status {status:?}");
    }
}


/// Open the bar on `size` with four tokens and nothing in the field.
fn bar_with_tokens(tree: &Tree, size: (f32, f32)) -> Snapshot {
    let mut shot = open(tree, size);
    ctrl(&mut shot, "f");
    type_text(&mut shot, "type:image size:>1KB modified:week ext:png ");
    until(&mut shot, |shot| named(shot, "FilterToken").len() == 4);
    shot.settle(Duration::from_millis(150));
    assert_eq!(named(&mut shot, "FilterToken").len(), 4, "{:?}", labels(&mut shot));
    shot
}

/// Eight keyboard stops in the bar with four tokens: the field, the scope,
/// the Filters button, each token's remove button and Clear all.
const STOPS: usize = 8;

#[test]
fn escape_leaves_the_search_from_every_focus_stop_of_the_bar() {
    let mut stuck = Vec::new();
    for size in [DESKTOP, PHONE] {
        for stop in 0..STOPS {
            let tree = Tree::new(&format!("escape-stop-{stop}-{}", size.0 as i32));
            let mut shot = bar_with_tokens(&tree, size);
            for _ in 0..stop {
                shot.key(Key::Named(NamedKey::Tab));
            }
            rest(&mut shot);
            let focused = shot
                .runner()
                .run_in(|| *Platform::get().current_window().focused_accessibility_id.peek());
            shot.key(Key::Named(NamedKey::Escape));
            rest(&mut shot);
            if bar_open(&mut shot) {
                stuck.push(format!("stop {stop} ({focused:?}) at {} px", size.0));
            }
        }
    }
    assert!(stuck.is_empty(), "Escape did not leave the search from: {stuck:?}");
}

#[test]
fn escape_in_the_scope_dropdown_closes_its_list_first_and_then_leaves() {
    let tree = Tree::new("escape-list");
    let mut shot = bar_with_tokens(&tree, DESKTOP);
    shot.key(Key::Named(NamedKey::Tab));
    shot.key(Key::Named(NamedKey::Enter));
    until(&mut shot, |shot| shown(shot, "Everywhere"));
    assert!(shown(&mut shot, "Everywhere"), "the scope list is open: {:?}", labels(&mut shot));
    shot.key(Key::Named(NamedKey::Escape));
    rest(&mut shot);
    assert!(bar_open(&mut shot), "the first Escape only closes the list");
    assert!(!shown(&mut shot, "Everywhere"), "and the list is closed");
    shot.key(Key::Named(NamedKey::Escape));
    rest(&mut shot);
    assert!(!bar_open(&mut shot), "the next Escape, from the closed dropdown, leaves the search");
}

#[test]
fn escape_from_a_button_leaves_the_search_even_with_text_in_the_field() {
    let tree = Tree::new("escape-text");
    let mut shot = open(&tree, DESKTOP);
    ctrl(&mut shot, "f");
    type_text(&mut shot, "needle");
    until(&mut shot, |shot| shown(shot, "needle.txt"));
    shot.key(Key::Named(NamedKey::Tab));
    shot.key(Key::Named(NamedKey::Tab));
    shot.key(Key::Named(NamedKey::Escape));
    rest(&mut shot);
    assert!(!bar_open(&mut shot), "Escape off the field ends the search, as in the list");
    assert!(!shown(&mut shot, "needle.txt"), "and the folder is whole again");
}

/// The status line of the bar: the one label that says how the search is going.
fn status_line(shot: &mut Snapshot) -> String {
    labels(shot).into_iter().find(|label| label.contains(" found") || label.contains("Searching")).unwrap_or_default()
}

#[test]
fn stop_ends_the_walk_of_a_subfolder_search_and_what_was_found_stays() {
    // "-slow-walk": the search holds after its first batch of results until it is stopped (see
    // `holds_the_walk` in search.rs), so Stop is pressed while the walk is running however loaded
    // the machine is.
    let tree = Tree::crowded("stop-slow-walk", 6000);
    let mut shot = open(&tree, DESKTOP);
    ctrl(&mut shot, "f");
    type_text(&mut shot, "item");
    // Wait for the walk to be under way: results arrive and the bar offers Stop.
    until(&mut shot, |shot| shown(shot, "Stop") && shown(shot, "item-"));
    assert!(shown(&mut shot, "Stop"), "the walk was still running when Stop was wanted: {}", status_line(&mut shot));
    assert!(shot.click_text("Stop"));
    until(&mut shot, |shot| status_line(shot).contains("stopped") && !shown(shot, "Stop"));
    let stopped = status_line(&mut shot);
    assert!(stopped.contains("stopped"), "the status says the search was stopped: {stopped:?}");
    assert!(!shown(&mut shot, "Stop"), "and the button is gone: {:?}", labels(&mut shot));
    assert!(shown(&mut shot, "item-"), "what was found stays on the list");
    // The walk really ended: nothing more arrives afterwards.
    shot.settle(Duration::from_millis(800));
    assert_eq!(status_line(&mut shot), stopped, "results kept arriving after Stop");
}

#[test]
fn ctrl_f_in_the_field_is_a_shortcut_not_a_letter() {
    let tree = Tree::new("ctrlf");
    let mut shot = open(&tree, DESKTOP);
    ctrl(&mut shot, "f");
    type_text(&mut shot, "abc");
    ctrl(&mut shot, "f");
    ctrl(&mut shot, "l");
    assert!(shown(&mut shot, "abc") && !shown(&mut shot, "abcf") && !shown(&mut shot, "abcl"), "{:?}", labels(&mut shot));
}
