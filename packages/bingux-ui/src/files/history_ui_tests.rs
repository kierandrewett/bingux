//! Back, Forward, Up and their menus in the file view, under real keys and a
//! real pointer in the headless runner, with more than one tab.
//!
//! What a tab's history holds is checked by `history` and `history_fuzz`
//! without a window. These tests hold the window to it: the keys and mouse
//! buttons that walk the history, what the menus list for each tab, a folder
//! that is gone, a search or a refresh (which are not moves) and a typed path
//! that is not there.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use freya::prelude::*;
use freya_core::elements::label::Label;
use freya_core::elements::paragraph::Paragraph;
use freya_testing::prelude::{MouseEventName, PlatformEvent};
use torin::prelude::Area;

use super::history::Trail;
use super::{Browse, TabMemory, Where, browse};
use crate::shell::OverlayHost;
use crate::snapshot::Snapshot;
use crate::style::*;
use crate::theme::provide_tokens;

const SIZE: (f32, f32) = (1000.0, 600.0);

/// A folder tree in the temporary directory:
///
/// ```text
/// root/alpha/one  root/alpha/two  root/alpha/a1.txt
/// root/beta/inner root/beta/b1.txt
/// root/gamma
/// ```
struct Tree {
    root: PathBuf,
}

impl Tree {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!("bingux-history-ui-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for dir in ["alpha/one", "alpha/two", "beta/inner", "gamma"] {
            std::fs::create_dir_all(root.join(dir)).expect("a folder in the temporary directory");
        }
        std::fs::write(root.join("alpha/a1.txt"), "a").expect("a file");
        std::fs::write(root.join("beta/b1.txt"), "b").expect("a file");
        Tree { root }
    }

    fn at(&self, below: &str) -> Where {
        Where::Folder(if below.is_empty() { self.root.clone() } else { self.root.join(below) })
    }

    fn name(&self) -> String {
        self.root.file_name().expect("a name").to_string_lossy().into_owned()
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The folders tabs were asked to open in a tab of their own, in order.
#[derive(Clone, Default)]
struct Opened(Rc<RefCell<Vec<PathBuf>>>);

impl PartialEq for Opened {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// The window: the body of the tab on show, and Ctrl+Tab for the next tab.
#[derive(PartialEq)]
struct Window {
    tabs: Vec<TabMemory>,
    opened: Opened,
}

impl Component for Window {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        let mut active = use_state(|| 0_usize);
        let count = self.tabs.len();
        let memory = self.tabs[active().min(count - 1)].clone();
        let opened = self.opened.clone();
        rect()
            .tw(&tokens, &[w_full(), h_full(), flex_col(), bg(Paint::Bg)])
            .on_global_key_down(move |event: Event<KeyboardEventData>| {
                if event.modifiers.contains(Modifiers::CONTROL) && event.key == Key::Named(NamedKey::Tab) {
                    event.stop_propagation();
                    let next = (*active.peek() + 1) % count;
                    active.set(next);
                }
            })
            .child(OverlayHost::new(
                rect().tw(&tokens, &[w_full(), h_flex(), flex_col()]).child(browse(
                    Browse::new(memory).on_new_tab(EventHandler::new(move |path: PathBuf| opened.0.borrow_mut().push(path))),
                )),
            ))
    }
}

fn open(tabs: Vec<TabMemory>) -> (Snapshot, Opened) {
    let opened = Opened::default();
    let shared = opened.clone();
    let mut shot = Snapshot::new(move || Window { tabs: tabs.clone(), opened: shared.clone() }.into_element(), SIZE);
    shot.settle(Duration::from_millis(1200));
    (shot, opened)
}

fn rest(shot: &mut Snapshot) {
    shot.settle(Duration::from_millis(400));
}

fn trail(memory: &TabMemory) -> Trail {
    Trail::of(&memory.model())
}

/// A trail as the names of its folders, `root` for the tree's root.
fn names(tree: &Tree, trail: &Trail) -> (Vec<String>, String, Vec<String>) {
    let name = |at: &Where| match at {
        Where::Folder(path) => match path.strip_prefix(&tree.root) {
            Ok(rest) if rest.as_os_str().is_empty() => "root".to_string(),
            Ok(rest) => rest.to_string_lossy().into_owned(),
            Err(_) => path.to_string_lossy().into_owned(),
        },
        Where::Recent => "recent".to_string(),
    };
    (trail.back.iter().map(name).collect(), name(&trail.current), trail.forward.iter().map(name).collect())
}

fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(|text| text.to_string()).collect()
}

/// Select the row called `name` and open it with Enter.
fn enter(shot: &mut Snapshot, name: &str) {
    assert!(shot.click_text(name), "a row called {name}");
    shot.key(Key::Named(NamedKey::Enter));
    rest(shot);
}

fn alt(shot: &mut Snapshot, key: NamedKey) {
    shot.key_with(Key::Named(key), Modifiers::ALT);
    rest(shot);
}

fn named(shot: &mut Snapshot, class: &'static str) -> Vec<Area> {
    shot.runner().find_many(move |node, element| {
        (element.accessibility().builder.class_name() == Some(class)).then(|| node.layout().area)
    })
}

fn centre(area: &Area) -> (f32, f32) {
    (area.min_x() + area.width() / 2.0, area.min_y() + area.height() / 2.0)
}

/// The toolbar's icon buttons, left to right: Back, Forward, Up, Refresh, …
fn toolbar(shot: &mut Snapshot) -> Vec<Area> {
    let mut buttons = named(shot, "IconButton");
    let top = buttons.iter().map(Area::min_y).fold(f32::MAX, f32::min);
    buttons.retain(|button| (button.min_y() - top).abs() < 4.0);
    buttons.sort_by(|a, b| a.min_x().total_cmp(&b.min_x()));
    buttons
}

/// A press of a mouse button at a point, and its release.
fn mouse(shot: &mut Snapshot, at: (f32, f32), button: MouseButton) {
    let cursor = CursorPoint::new(at.0 as f64, at.1 as f64);
    for name in [MouseEventName::MouseDown, MouseEventName::MouseUp] {
        shot.runner().send_event(PlatformEvent::Mouse { name, cursor, button: Some(button) });
        shot.runner().sync_and_update();
    }
    rest(shot);
}

/// The rows of the menu that is open, top to bottom: their text and where.
fn menu(shot: &mut Snapshot) -> Vec<(String, Area)> {
    let rows = named(shot, "MenuRow");
    let labels: Vec<(String, Area)> = shot.runner().find_many(|node, element| {
        let label = Label::try_downcast(element)?;
        Some((label.text.to_string(), node.layout().area))
    });
    let mut found: Vec<(String, Area)> = rows
        .into_iter()
        .filter_map(|row| {
            // The overlay is drawn last: of the labels under a row, the menu's is the last.
            let label = labels.iter().rfind(|(_, area)| {
                area.min_x() >= row.min_x() - 0.5
                    && area.max_x() <= row.max_x() + 0.5
                    && area.min_y() >= row.min_y() - 0.5
                    && area.max_y() <= row.max_y() + 0.5
            })?;
            Some((label.0.clone(), row))
        })
        .collect();
    found.sort_by(|a, b| a.1.min_y().total_cmp(&b.1.min_y()));
    found
}

/// The text of every label and paragraph that is drawn.
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

#[test]
fn alt_left_alt_right_alt_up_and_backspace_walk_the_history() {
    let tree = Tree::new("keys");
    let memory = TabMemory::new(tree.at(""));
    let (mut shot, _) = open(vec![memory.clone()]);
    enter(&mut shot, "alpha");
    enter(&mut shot, "one");
    assert_eq!(names(&tree, &trail(&memory)), (strings(&["root", "alpha"]), "alpha/one".into(), vec![]));

    alt(&mut shot, NamedKey::ArrowLeft);
    assert_eq!(names(&tree, &trail(&memory)), (strings(&["root"]), "alpha".into(), strings(&["alpha/one"])), "Alt+Left");
    alt(&mut shot, NamedKey::ArrowRight);
    assert_eq!(names(&tree, &trail(&memory)), (strings(&["root", "alpha"]), "alpha/one".into(), vec![]), "Alt+Right");

    alt(&mut shot, NamedKey::ArrowUp);
    assert_eq!(
        names(&tree, &trail(&memory)),
        (strings(&["root", "alpha", "alpha/one"]), "alpha".into(), vec![]),
        "Alt+Up is a move: an entry, and Back returns to where it was pressed"
    );
    shot.key(Key::Named(NamedKey::Backspace));
    rest(&mut shot);
    assert_eq!(
        names(&tree, &trail(&memory)),
        (strings(&["root", "alpha", "alpha/one", "alpha"]), "root".into(), vec![]),
        "Backspace goes to the parent folder"
    );
    alt(&mut shot, NamedKey::ArrowLeft);
    assert_eq!(names(&tree, &trail(&memory)).1, "alpha", "Back after Up returns to the child");
    alt(&mut shot, NamedKey::ArrowLeft);
    alt(&mut shot, NamedKey::ArrowLeft);
    alt(&mut shot, NamedKey::ArrowLeft);
    alt(&mut shot, NamedKey::ArrowLeft);
    assert_eq!(names(&tree, &trail(&memory)).0, Vec::<String>::new(), "Back stops at the first entry");
    alt(&mut shot, NamedKey::ArrowLeft);
    assert_eq!(names(&tree, &trail(&memory)).1, "root", "Back with nothing behind does nothing");

    // Going somewhere from the root, with entries ahead, forgets them.
    assert_eq!(names(&tree, &trail(&memory)).2, strings(&["alpha", "alpha/one", "alpha", "root"]));
    enter(&mut shot, "gamma");
    assert_eq!(names(&tree, &trail(&memory)), (strings(&["root"]), "gamma".into(), vec![]));
}

#[test]
fn the_mouse_back_and_forward_buttons_walk_the_history() {
    let tree = Tree::new("mouse");
    let memory = TabMemory::new(tree.at(""));
    let (mut shot, _) = open(vec![memory.clone()]);
    enter(&mut shot, "beta");
    enter(&mut shot, "inner");
    mouse(&mut shot, (500.0, 300.0), MouseButton::Back);
    assert_eq!(names(&tree, &trail(&memory)).1, "beta", "the Back button of the mouse");
    mouse(&mut shot, (500.0, 300.0), MouseButton::Back);
    assert_eq!(names(&tree, &trail(&memory)).1, "root");
    mouse(&mut shot, (500.0, 300.0), MouseButton::Back);
    assert_eq!(names(&tree, &trail(&memory)).1, "root", "nothing behind");
    mouse(&mut shot, (500.0, 300.0), MouseButton::Forward);
    mouse(&mut shot, (500.0, 300.0), MouseButton::Forward);
    assert_eq!(names(&tree, &trail(&memory)), (strings(&["root", "beta"]), "beta/inner".into(), vec![]));
    mouse(&mut shot, (500.0, 300.0), MouseButton::Forward);
    assert_eq!(names(&tree, &trail(&memory)).1, "beta/inner", "nothing ahead");
}

#[test]
fn the_toolbar_buttons_and_their_menus_belong_to_the_tab_on_show() {
    let tree = Tree::new("menus");
    let (first, second) = (TabMemory::new(tree.at("")), TabMemory::new(tree.at("")));
    let (mut shot, opened) = open(vec![first.clone(), second.clone()]);
    enter(&mut shot, "alpha");
    enter(&mut shot, "one");
    shot.key_with(Key::Named(NamedKey::Tab), Modifiers::CONTROL);
    rest(&mut shot);
    enter(&mut shot, "beta");
    enter(&mut shot, "inner");
    assert_eq!(names(&tree, &trail(&first)).1, "alpha/one");
    assert_eq!(names(&tree, &trail(&second)).1, "beta/inner");

    // The second tab's Back menu: nearest first, plain text.
    let buttons = toolbar(&mut shot);
    assert!(buttons.len() >= 3, "Back, Forward and Up are there: {buttons:?}");
    let (back, forward) = (centre(&buttons[0]), centre(&buttons[1]));
    shot.right_click(back.0, back.1);
    rest(&mut shot);
    let rows = menu(&mut shot);
    let texts: Vec<&str> = rows.iter().map(|(text, _)| text.as_str()).collect();
    assert_eq!(
        texts,
        ["beta", tree.name().as_str()],
        "the Back menu of the second tab: second at {:?}, buttons {buttons:?}, labels {:?}",
        names(&tree, &trail(&second)),
        labels(&mut shot)
    );
    shot.key(Key::Named(NamedKey::Escape));
    rest(&mut shot);
    assert!(menu(&mut shot).is_empty(), "Escape closes the menu");

    // The Forward menu is empty until there is somewhere forward.
    shot.right_click(forward.0, forward.1);
    rest(&mut shot);
    assert!(menu(&mut shot).is_empty(), "nothing forward, no menu");

    // The first tab's menu is its own.
    shot.key_with(Key::Named(NamedKey::Tab), Modifiers::CONTROL);
    rest(&mut shot);
    shot.right_click(back.0, back.1);
    rest(&mut shot);
    let rows = menu(&mut shot);
    let texts: Vec<&str> = rows.iter().map(|(text, _)| text.as_str()).collect();
    assert_eq!(texts, ["alpha", tree.name().as_str()], "the Back menu of the first tab");

    // A choice crosses the entries between: one move, and Forward lists them.
    let root_row = rows[1].1;
    let at = centre(&root_row);
    shot.click(at.0, at.1);
    rest(&mut shot);
    assert_eq!(
        names(&tree, &trail(&first)),
        (vec![], "root".into(), strings(&["alpha", "alpha/one"])),
        "choosing the oldest row goes there in one move"
    );
    assert_eq!(names(&tree, &trail(&second)).1, "beta/inner", "the other tab did not move");
    shot.right_click(forward.0, forward.1);
    rest(&mut shot);
    let texts: Vec<String> = menu(&mut shot).into_iter().map(|(text, _)| text).collect();
    assert_eq!(texts, strings(&["alpha", "one"]), "the Forward menu, nearest first");
    shot.key(Key::Named(NamedKey::Escape));
    rest(&mut shot);

    // A middle click on a row opens that folder in a tab and leaves this history alone.
    shot.right_click(forward.0, forward.1);
    rest(&mut shot);
    let rows = menu(&mut shot);
    let one = centre(&rows[1].1);
    mouse(&mut shot, one, MouseButton::Middle);
    assert_eq!(opened.0.borrow().clone(), vec![tree.root.join("alpha/one")], "opened in a new tab");
    assert_eq!(names(&tree, &trail(&first)), (vec![], "root".into(), strings(&["alpha", "alpha/one"])), "no move");
    assert!(menu(&mut shot).is_empty(), "the menu closed");
}

#[test]
fn a_refresh_and_a_search_are_not_moves() {
    let tree = Tree::new("refresh");
    let memory = TabMemory::new(tree.at(""));
    let (mut shot, _) = open(vec![memory.clone()]);
    enter(&mut shot, "alpha");
    alt(&mut shot, NamedKey::ArrowLeft);
    let before = names(&tree, &trail(&memory));
    assert_eq!(before, (vec![], "root".into(), strings(&["alpha"])));

    shot.key(Key::Named(NamedKey::F5));
    rest(&mut shot);
    shot.key_with(Key::Character("r".into()), Modifiers::CONTROL);
    rest(&mut shot);
    assert_eq!(names(&tree, &trail(&memory)), before, "F5 and Ctrl+R read the folder again and nothing else");

    shot.key_with(Key::Character("f".into()), Modifiers::CONTROL);
    for letter in ["a", "l"] {
        shot.key(Key::Character(letter.into()));
    }
    rest(&mut shot);
    assert_eq!(names(&tree, &trail(&memory)), before, "searching adds no entry and keeps Forward");
    shot.key(Key::Named(NamedKey::Escape));
    rest(&mut shot);
    assert_eq!(names(&tree, &trail(&memory)), before, "ending the search adds no entry");
}

#[test]
fn a_folder_that_is_gone_stays_in_the_history_and_the_view_says_so() {
    let tree = Tree::new("gone");
    let memory = TabMemory::new(tree.at(""));
    let (mut shot, _) = open(vec![memory.clone()]);
    enter(&mut shot, "alpha");
    enter(&mut shot, "one");
    alt(&mut shot, NamedKey::ArrowLeft);
    std::fs::remove_dir_all(tree.root.join("alpha/one")).expect("the folder is removed");
    alt(&mut shot, NamedKey::ArrowRight);
    shot.settle(Duration::from_millis(900));
    assert_eq!(names(&tree, &trail(&memory)).1, "alpha/one", "Forward went to the entry that was kept");
    let found = labels(&mut shot);
    assert!(shown(&mut shot, "Folder not found"), "the view says the folder is not there: {found:?}");
    // The toolbar still works from it, and Up goes by the name. (The error
    // page has no focused row, so Alt+Left and Alt+Up do not reach the view
    // from it: a known gap, left unfixed.)
    let buttons = toolbar(&mut shot);
    let (back, up) = (centre(&buttons[0]), centre(&buttons[2]));
    shot.click(up.0, up.1);
    shot.settle(Duration::from_millis(600));
    assert_eq!(names(&tree, &trail(&memory)).1, "alpha");
    assert!(!shown(&mut shot, "Folder not found"));
    shot.click(back.0, back.1);
    shot.settle(Duration::from_millis(600));
    assert_eq!(names(&tree, &trail(&memory)).1, "alpha/one", "Back to the missing folder is allowed too");
    shot.click(back.0, back.1);
    shot.settle(Duration::from_millis(600));
    assert_eq!(names(&tree, &trail(&memory)).1, "alpha");
}

#[test]
fn a_typed_path_that_is_not_there_adds_nothing() {
    let tree = Tree::new("typed");
    let memory = TabMemory::new(tree.at(""));
    let (mut shot, _) = open(vec![memory.clone()]);
    enter(&mut shot, "alpha");
    let before = names(&tree, &trail(&memory));
    shot.key_with(Key::Character("l".into()), Modifiers::CONTROL);
    rest(&mut shot);
    shot.key_with(Key::Character("a".into()), Modifiers::CONTROL);
    for letter in "/no-such-folder-here".chars() {
        shot.key(Key::Character(letter.to_string()));
    }
    shot.key(Key::Named(NamedKey::Enter));
    shot.settle(Duration::from_millis(900));
    assert_eq!(names(&tree, &trail(&memory)), before, "an address that is not there is not a place");
    assert!(shown(&mut shot, "Not found"), "and the bar says why");
}

#[test]
fn a_duplicated_tab_has_its_own_copy_of_the_history() {
    let tree = Tree::new("twin");
    let original = TabMemory::new(tree.at(""));
    let (mut shot, _) = open(vec![original.clone()]);
    enter(&mut shot, "alpha");
    enter(&mut shot, "one");
    let copy = original.duplicate();
    assert_eq!(trail(&copy), trail(&original));
    // Moving in the original leaves the copy where it was.
    alt(&mut shot, NamedKey::ArrowLeft);
    assert_eq!(names(&tree, &trail(&original)).1, "alpha");
    assert_eq!(names(&tree, &trail(&copy)).1, "alpha/one");
    assert_eq!(names(&tree, &trail(&copy)).0, strings(&["root", "alpha"]));
}
