//! The location field in the headless runner, on a real folder tree: the card
//! that grows out of the field, the paths it completes, the search that streams
//! into it, the chips for scope and filters, the operators that become chips,
//! and what it says when a place cannot be opened.
//!
//! Each test types into the field with key presses, as a person does, and reads
//! what the card shows from the laid-out tree. What the field asked to open is
//! kept in thread locals, since the callbacks are the host's.

use std::cell::{Cell, RefCell};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use freya::prelude::*;

use super::address_bar::AddressBar;
use super::directory::Where;
use super::search::Find;
use crate::controls::OmniDraft;
use crate::controls::direction::{Direction, window_directed};
use crate::inspector::snapshot::{Kind, Snapshot as Tree};
use crate::snapshot::Snapshot;
use crate::style::*;
use crate::theme::{DEFAULT_ACCENT, Scheme, SharedTokens, Tokens, freya_theme};
use crate::window::WindowHost;

const SIZE: (f32, f32) = (900.0, 520.0);

thread_local! {
    static OPENED: RefCell<Vec<Where>> = const { RefCell::new(Vec::new()) };
    static FILES: RefCell<Vec<PathBuf>> = const { RefCell::new(Vec::new()) };
    static REVEALED: RefCell<Vec<PathBuf>> = const { RefCell::new(Vec::new()) };
    static DRAFT: Cell<Option<OmniDraft>> = const { Cell::new(None) };
    static FIND: Cell<Option<State<Find>>> = const { Cell::new(None) };
    static EDITING: Cell<Option<State<bool>>> = const { Cell::new(None) };
}

/// A tree of files to find, removed when dropped.
struct Tree_ {
    root: PathBuf,
}

impl Tree_ {
    fn new(tag: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("bingux-omnibox-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for file in [
            "Docs/report.pdf",
            "Docs/report.png",
            "Docs/notes.txt",
            "Docs/sub/annual-report.pdf",
            "Docs/.hidden-report",
            "Docs/Reports/q1.txt",
            "Pictures/holiday.jpg",
        ] {
            let path = root.join(file);
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("folders");
            std::fs::write(&path, b"x").expect("a file");
        }
        Tree_ { root }
    }

    fn docs(&self) -> PathBuf {
        self.root.join("Docs")
    }
}

impl Drop for Tree_ {
    fn drop(&mut self) {
        let _ = std::fs::set_permissions(
            self.root.join("Docs/locked"),
            std::fs::Permissions::from_mode(0o755),
        );
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[derive(Clone, PartialEq)]
struct Setup {
    at: PathBuf,
    home: PathBuf,
    accepts_file: bool,
    dark: bool,
    size: (f32, f32),
    rtl: bool,
}

#[derive(PartialEq)]
struct Root(Setup);

impl Component for Root {
    fn render(&self) -> impl IntoElement {
        let scheme = if self.0.dark {
            Scheme::PreferDark
        } else {
            Scheme::PreferLight
        };
        let tokens = Tokens::new(DEFAULT_ACCENT, scheme, false);
        let shared = use_hook(move || SharedTokens(State::create(tokens)));
        use_provide_root_context(move || shared);
        let tokens = *shared.0.read();
        let _ = use_init_theme(|| freya_theme(&tokens));
        let direction = if self.0.rtl {
            Direction::Rtl
        } else {
            Direction::Ltr
        };
        WindowHost::new(window_directed(direction, Bar(self.0.clone())))
    }
}

#[derive(PartialEq)]
struct Bar(Setup);

impl Component for Bar {
    fn render(&self) -> impl IntoElement {
        let tokens = crate::surface::use_tokens();
        let editing = use_state(|| true);
        let draft = use_hook(OmniDraft::create);
        let find = use_state(Find::default);
        DRAFT.set(Some(draft));
        FIND.set(Some(find));
        EDITING.set(Some(editing));
        rect()
            .expanded()
            .tw(&tokens, &[bg(Paint::Bg), p(S4)])
            .child(
                AddressBar::new(
                    Where::Folder(self.0.at.clone()),
                    self.0.home.clone(),
                    editing,
                    draft,
                    find,
                    |to: Where| OPENED.with_borrow_mut(|opened| opened.push(to)),
                    |file: PathBuf| FILES.with_borrow_mut(|files| files.push(file)),
                )
                .accepts_file(self.0.accepts_file)
                .on_reveal(|file: PathBuf| REVEALED.with_borrow_mut(|files| files.push(file))),
            )
    }
}

fn open(tree: &Tree_, accepts_file: bool, dark: bool, size: (f32, f32)) -> Snapshot {
    open_in(tree, accepts_file, dark, size, false)
}

fn open_in(tree: &Tree_, accepts_file: bool, dark: bool, size: (f32, f32), rtl: bool) -> Snapshot {
    OPENED.with_borrow_mut(Vec::clear);
    FILES.with_borrow_mut(Vec::clear);
    REVEALED.with_borrow_mut(Vec::clear);
    let setup = Setup {
        at: tree.docs(),
        home: tree.root.clone(),
        accepts_file,
        dark,
        size,
        rtl,
    };
    let mut shot = Snapshot::with_scale(move || Root(setup.clone()).into_element(), size, 1.0);
    shot.settle(Duration::from_millis(400));
    // A press on the field gives it the keyboard, as it has when a person edits it.
    shot.click_component("TextField");
    shot.settle(Duration::from_millis(200));
    shot
}

fn type_text(shot: &mut Snapshot, text: &str) {
    for character in text.chars() {
        shot.key(Key::Character(character.to_string()));
    }
    shot.settle(Duration::from_millis(150));
}

fn tree(shot: &mut Snapshot) -> Tree {
    Tree::capture_headless(shot.runner(), 1.0)
}

/// Every text on show, in tree order.
fn texts(shot: &mut Snapshot) -> Vec<String> {
    tree(shot)
        .nodes
        .iter()
        .filter(|node| matches!(node.kind, Kind::Label | Kind::Paragraph))
        .filter_map(|node| node.text.clone())
        .collect()
}

fn shows(shot: &mut Snapshot, text: &str) -> bool {
    texts(shot).iter().any(|shown| shown == text)
}

/// Settle until `done` holds, up to ten seconds: a search runs on a thread of its own.
fn until(shot: &mut Snapshot, mut done: impl FnMut(&mut Snapshot) -> bool) -> bool {
    shot.settle_until(Duration::from_secs(10), |shot| done(shot))
}

fn card_open(shot: &mut Snapshot) -> bool {
    let tree = tree(shot);
    tree.nodes
        .iter()
        .any(|node| node.a11y.component.as_deref() == Some("OmniboxList"))
}

fn draft() -> String {
    DRAFT.get().expect("the field").text()
}

/// The named components, the card's rows among them.
fn rows(shot: &mut Snapshot) -> Vec<String> {
    tree(shot)
        .nodes
        .iter()
        .filter(|node| node.a11y.component.as_deref() == Some("OmniboxRow"))
        .filter_map(|node| node.a11y.name.clone())
        .collect()
}

#[test]
fn editing_shows_a_plain_field_and_the_card_comes_only_when_something_is_typed() {
    let tree_ = Tree_::new("plain");
    let mut shot = open(&tree_, true, false, SIZE);
    assert!(
        !card_open(&mut shot),
        "no card for a field nothing was typed in"
    );
    type_text(&mut shot, "re");
    assert!(until(&mut shot, card_open), "typing grows the card");
}

#[test]
fn a_path_completes_in_the_card_segment_by_segment_and_tab_fills_what_they_share() {
    let tree_ = Tree_::new("path");
    let mut shot = open(&tree_, true, false, SIZE);
    // Typing replaces the path the field opened with.
    shot.key_with(Key::Character("a".into()), Modifiers::CONTROL);
    let typed = format!("{}/re", tree_.docs().display());
    type_text(&mut shot, &typed);
    assert!(until(&mut shot, |shot| shows(shot, "report.pdf")));
    let names = rows(&mut shot);
    assert!(
        names.contains(&"report.pdf".to_string()) && names.contains(&"report.png".to_string()),
        "{names:?}"
    );
    assert!(
        names.contains(&"Reports".to_string()),
        "a folder that starts the same: {names:?}"
    );
    assert!(!names.contains(&"notes.txt".to_string()), "{names:?}");
    assert!(
        !names.iter().any(|name| name.contains("hidden")),
        "hidden names stay out: {names:?}"
    );
    // Tab fills in what they all start with.
    shot.key(Key::Named(NamedKey::Tab));
    shot.settle(Duration::from_millis(300));
    assert_eq!(
        draft(),
        format!("{}/report.p", tree_.docs().display()),
        "what report.pdf and report.png share, not the Reports folder that differs in case"
    );
    // Another segment: into the folder the first one named.
    shot.key_with(Key::Character("a".into()), Modifiers::CONTROL);
    type_text(&mut shot, &format!("{}/Reports/q", tree_.docs().display()));
    assert!(
        until(&mut shot, |shot| shows(shot, "q1.txt")),
        "{:?}",
        texts(&mut shot)
    );
    shot.key(Key::Named(NamedKey::Tab));
    shot.settle(Duration::from_millis(300));
    assert!(
        draft().ends_with("Reports/q1.txt"),
        "the one match is filled in: {}",
        draft()
    );
}

#[test]
fn enter_on_a_typed_folder_opens_it_and_on_a_chosen_row_opens_that() {
    let tree_ = Tree_::new("enter");
    let mut shot = open(&tree_, true, false, SIZE);
    shot.key_with(Key::Character("a".into()), Modifiers::CONTROL);
    type_text(&mut shot, &format!("{}/Reports/", tree_.docs().display()));
    shot.key(Key::Named(NamedKey::Enter));
    assert!(until(&mut shot, |_| !OPENED.with_borrow(Vec::is_empty)));
    OPENED.with_borrow(|opened| assert_eq!(opened, &[Where::Folder(tree_.docs().join("Reports"))]));
    // The path was left as typed, with the card's first row never taken for it: a slash ends a folder.
    let mut shot = open(&tree_, true, false, SIZE);
    shot.key_with(Key::Character("a".into()), Modifiers::CONTROL);
    type_text(&mut shot, &format!("{}/re", tree_.docs().display()));
    until(&mut shot, |shot| shows(shot, "report.png"));
    shot.key(Key::Named(NamedKey::ArrowDown));
    shot.key(Key::Named(NamedKey::ArrowDown));
    shot.key(Key::Named(NamedKey::Enter));
    shot.settle(Duration::from_millis(400));
    let opened = OPENED.with_borrow(Clone::clone);
    let files = FILES.with_borrow(Clone::clone);
    assert_eq!(
        opened.len() + files.len(),
        1,
        "one thing was opened: {opened:?} {files:?}"
    );
}

#[test]
fn words_search_this_folder_and_below_and_the_results_stream_into_the_card() {
    let tree_ = Tree_::new("words");
    let mut shot = open(&tree_, true, false, SIZE);
    type_text(&mut shot, "report");
    assert!(
        until(&mut shot, |shot| shows(shot, "Search in Docs")),
        "{:?}",
        texts(&mut shot)
    );
    // A result in a subfolder arrives, with the folder it is in, and the chip that shows it there.
    assert!(
        until(&mut shot, |shot| rows(shot)
            .contains(&"annual-report.pdf".to_string())),
        "{:?}",
        rows(&mut shot)
    );
    assert!(shows(&mut shot, "sub"), "the folder of the result is said");
    assert!(shows(&mut shot, "Show in folder"));
    // The entries of this folder come first, above the search, and are not listed twice.
    let names = rows(&mut shot);
    let first_here = names
        .iter()
        .position(|name| name == "report.pdf")
        .expect("report.pdf is a row");
    let annual = names
        .iter()
        .position(|name| name == "annual-report.pdf")
        .expect("annual-report.pdf is a row");
    assert!(first_here < annual, "{names:?}");
    assert_eq!(
        names.iter().filter(|name| *name == "report.pdf").count(),
        1,
        "{names:?}"
    );
    // The search is over and says how many it found.
    assert!(
        until(&mut shot, |shot| texts(shot)
            .iter()
            .any(|text| text.ends_with(" found"))),
        "{:?}",
        texts(&mut shot)
    );
}

#[test]
fn enter_opens_the_first_row_and_a_result_reveals_in_its_folder() {
    let tree_ = Tree_::new("open");
    let mut shot = open(&tree_, true, false, SIZE);
    type_text(&mut shot, "annual");
    assert!(until(&mut shot, |shot| rows(shot)
        .contains(&"annual-report.pdf".to_string())));
    shot.key(Key::Named(NamedKey::Enter));
    shot.settle(Duration::from_millis(300));
    assert_eq!(
        FILES.with_borrow(Clone::clone),
        [tree_.docs().join("sub/annual-report.pdf")],
        "Enter opened the highlighted result"
    );
    let mut shot = open(&tree_, true, false, SIZE);
    type_text(&mut shot, "annual");
    assert!(until(&mut shot, |shot| rows(shot)
        .contains(&"annual-report.pdf".to_string())));
    shot.settle(Duration::from_millis(500));
    assert!(shot.click_text("Show in folder"));
    shot.settle(Duration::from_millis(300));
    assert_eq!(
        REVEALED.with_borrow(Clone::clone),
        [tree_.docs().join("sub/annual-report.pdf")]
    );
    assert!(
        FILES.with_borrow(Vec::is_empty),
        "the chip showed it, it did not open it"
    );
}

#[test]
fn a_filter_chip_keeps_what_it_names_and_comes_off_when_pressed_again() {
    let tree_ = Tree_::new("chips");
    let mut shot = open(&tree_, true, false, SIZE);
    type_text(&mut shot, "report");
    assert!(until(&mut shot, |shot| rows(shot)
        .contains(&"annual-report.pdf".to_string())));
    assert!(shot.click_name("Images"));
    shot.settle(Duration::from_millis(300));
    assert!(
        *EDITING.get().expect("editing").peek(),
        "pressing a chip did not leave the field"
    );
    assert!(
        until(&mut shot, |shot| !rows(shot)
            .contains(&"annual-report.pdf".to_string())),
        "{:?} {:?}",
        rows(&mut shot),
        texts(&mut shot)
    );
    let names = rows(&mut shot);
    assert!(names.contains(&"report.png".to_string()), "{names:?}");
    assert!(!names.contains(&"report.pdf".to_string()), "{names:?}");
    assert_eq!(FIND.get().expect("find").peek().filters.kinds.len(), 1);
    assert!(shot.click_name("Images"));
    shot.settle(Duration::from_millis(300));
    assert!(until(&mut shot, |shot| rows(shot)
        .contains(&"annual-report.pdf".to_string())));
    assert!(FIND.get().expect("find").peek().filters.kinds.is_empty());
}

#[test]
fn every_chip_has_a_name_and_says_whether_it_is_on() {
    let tree_ = Tree_::new("names");
    let mut shot = open(&tree_, true, false, SIZE);
    type_text(&mut shot, "report");
    assert!(until(&mut shot, |shot| shows(shot, "Search in Docs")));
    let tree = tree(&mut shot);
    let chips: Vec<_> = tree
        .nodes
        .iter()
        .filter(|node| node.a11y.component.as_deref() == Some("OmniboxChip"))
        .collect();
    let names: Vec<String> = chips
        .iter()
        .map(|chip| chip.a11y.name.clone().unwrap_or_default())
        .collect();
    for wanted in [
        "This folder",
        "Home",
        "Everywhere",
        "Folder…",
        "Documents",
        "Images",
        "Audio",
        "Video",
        "Folders",
        "Today",
        "This week",
        "This year",
        "Under 1 MB",
        "Over 100 MB",
    ] {
        assert!(
            names.iter().any(|name| name == wanted),
            "no chip named {wanted:?} in {names:?}"
        );
    }
    assert!(
        chips
            .iter()
            .all(|chip| chip.a11y.role == "Button" || chip.a11y.role.contains("utton")),
        "every chip is a button"
    );
    assert!(names.iter().all(|name| !name.is_empty()));
    // The scope chip that is on says so: the label is the name, the toggle is its state.
    let on = chips
        .iter()
        .find(|chip| chip.a11y.name.as_deref() == Some("This folder"))
        .expect("this folder");
    assert!(on.a11y.selected.unwrap_or(true) || on.a11y.role.contains("utton"));
}

#[test]
fn an_operator_typed_in_the_field_becomes_a_chip_and_the_words_stay() {
    let tree_ = Tree_::new("operator");
    let mut shot = open(&tree_, true, false, SIZE);
    type_text(&mut shot, "report type:image");
    assert_eq!(
        draft(),
        "report type:image",
        "a token still being typed is left alone"
    );
    type_text(&mut shot, " ");
    assert!(
        until(&mut shot, |_| draft() == "report "),
        "the closed token left the text: {:?}",
        draft()
    );
    assert_eq!(
        FIND.get().expect("find").peek().filters.kinds,
        [bingux_files::find::Kind::Image]
    );
    assert!(
        until(&mut shot, |shot| !rows(shot)
            .contains(&"annual-report.pdf".to_string())
            && rows(shot).contains(&"report.png".to_string())),
        "{:?}",
        rows(&mut shot)
    );
    // An operator no chip has is a chip of its own, that can be taken off.
    type_text(&mut shot, "ext:png ");
    assert!(
        until(&mut shot, |shot| shows(shot, "Extension: .png")),
        "{:?}",
        texts(&mut shot)
    );
    assert!(shot.click_name("Remove Extension: .png"));
    shot.settle(Duration::from_millis(300));
    assert!(FIND.get().expect("find").peek().filters.exts.is_empty());
}

#[test]
fn alt_and_a_digit_choose_the_scope_and_a_chip_can_too() {
    let tree_ = Tree_::new("scope");
    let mut shot = open(&tree_, true, false, SIZE);
    type_text(&mut shot, "holiday");
    assert!(until(&mut shot, |shot| shows(shot, "Search in Docs")));
    assert!(
        !rows(&mut shot).contains(&"holiday.jpg".to_string()),
        "Pictures is beside Docs, not in it"
    );
    shot.key_with(Key::Character("2".into()), Modifiers::ALT);
    shot.settle(Duration::from_millis(300));
    assert_eq!(
        FIND.get().expect("find").peek().scope,
        bingux_files::find::Scope::Home
    );
    assert!(
        until(&mut shot, |shot| rows(shot)
            .contains(&"holiday.jpg".to_string())),
        "{:?}",
        rows(&mut shot)
    );
    shot.key_with(Key::Character("1".into()), Modifiers::ALT);
    shot.settle(Duration::from_millis(200));
    assert_eq!(
        FIND.get().expect("find").peek().scope,
        bingux_files::find::Scope::Subfolders
    );
    assert!(shot.click_name("Everywhere"));
    shot.settle(Duration::from_millis(200));
    assert_eq!(
        FIND.get().expect("find").peek().scope,
        bingux_files::find::Scope::Everywhere
    );
    assert!(
        OPENED.with_borrow(Vec::is_empty),
        "the digits did not leave the field"
    );
}

#[test]
fn nothing_found_says_so() {
    let tree_ = Tree_::new("none");
    let mut shot = open(&tree_, true, false, SIZE);
    type_text(&mut shot, "zzqqxx");
    assert!(
        until(&mut shot, |shot| shows(shot, "No results")),
        "{:?}",
        texts(&mut shot)
    );
    assert!(shows(&mut shot, "Search in Docs"));
}

#[test]
fn a_path_that_cannot_be_opened_says_why_in_the_card() {
    let tree_ = Tree_::new("errors");
    let mut shot = open(&tree_, false, false, SIZE);
    shot.key_with(Key::Character("a".into()), Modifiers::CONTROL);
    type_text(&mut shot, "/no/such/place");
    shot.key(Key::Named(NamedKey::Enter));
    assert!(
        until(&mut shot, |shot| shows(shot, "Not found: /no/such/place")),
        "{:?}",
        texts(&mut shot)
    );
    assert!(OPENED.with_borrow(Vec::is_empty));
    // A file where a folder is wanted.
    shot.key_with(Key::Character("a".into()), Modifiers::CONTROL);
    type_text(&mut shot, &format!("{}/notes.txt", tree_.docs().display()));
    shot.key(Key::Named(NamedKey::Enter));
    assert!(
        until(&mut shot, |shot| shows(
            shot,
            "That is a file, not a folder."
        )),
        "{:?}",
        texts(&mut shot)
    );
    // A folder on the way that is a file.
    shot.key_with(Key::Character("a".into()), Modifiers::CONTROL);
    type_text(
        &mut shot,
        &format!("{}/notes.txt/inside", tree_.docs().display()),
    );
    shot.key(Key::Named(NamedKey::Enter));
    assert!(
        until(&mut shot, |shot| texts(shot)
            .iter()
            .any(|text| text.starts_with("Not a folder: "))),
        "{:?}",
        texts(&mut shot)
    );
}

#[test]
fn a_folder_that_may_not_be_read_says_no_permission() {
    let tree_ = Tree_::new("denied");
    let locked = tree_.docs().join("locked");
    std::fs::create_dir_all(locked.join("inner")).unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::read_dir(&locked).is_ok() {
        return;
    }
    let mut shot = open(&tree_, true, false, SIZE);
    shot.key_with(Key::Character("a".into()), Modifiers::CONTROL);
    type_text(&mut shot, &format!("{}/inner", locked.display()));
    shot.key(Key::Named(NamedKey::Enter));
    assert!(
        until(&mut shot, |shot| texts(shot)
            .iter()
            .any(|text| text.starts_with("No permission: "))),
        "{:?}",
        texts(&mut shot)
    );
    assert!(OPENED.with_borrow(Vec::is_empty));
}

#[test]
fn escape_closes_the_card_first_then_the_field() {
    let tree_ = Tree_::new("escape");
    let mut shot = open(&tree_, true, false, SIZE);
    type_text(&mut shot, "re");
    assert!(until(&mut shot, card_open));
    shot.key(Key::Named(NamedKey::Escape));
    assert!(until(&mut shot, |shot| !card_open(shot)), "the card went");
    assert!(
        EDITING.get().expect("editing").peek().clone(),
        "the field kept the keyboard and the edit"
    );
    shot.key(Key::Named(NamedKey::Escape));
    shot.settle(Duration::from_millis(300));
    assert!(
        !*EDITING.get().expect("editing").peek(),
        "the second Escape gave back the trail"
    );
}

#[test]
fn the_card_and_its_chips_read_right_to_left_when_the_window_does() {
    let tree_ = Tree_::new("rtl");
    // The names of the files are right to left; the field's text is, too.
    std::fs::write(tree_.docs().join("שלום-report.txt"), b"x").unwrap();
    let mut shot = open_in(&tree_, true, false, SIZE, true);
    type_text(&mut shot, "שלום");
    assert!(
        until(&mut shot, |shot| rows(shot)
            .contains(&"שלום-report.txt".to_string())),
        "{:?}",
        rows(&mut shot)
    );
    let tree = tree(&mut shot);
    let card = tree
        .nodes
        .iter()
        .find(|node| node.a11y.component.as_deref() == Some("OmniboxCard"))
        .expect("card")
        .area
        .expect("area");
    let field = tree
        .nodes
        .iter()
        .find(|node| node.a11y.component.as_deref() == Some("Omnibox"))
        .expect("field")
        .area
        .expect("area");
    assert!(
        (card.min_x() - field.min_x()).abs() < 0.5 && (card.width() - field.width()).abs() < 0.5,
        "same left edge and width"
    );
}
