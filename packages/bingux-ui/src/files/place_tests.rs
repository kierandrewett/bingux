//! The sidebar's row menus in the headless runner: a right click, Shift+F10
//! and Alt+Enter on a place, and Properties opening the dialog of that folder.

use std::path::PathBuf;
use std::time::Duration;

use bingux_files::places::Sources;
use bingux_files::Places;
use freya::prelude::*;

use super::directory::Where;
use super::places_view::PlacesSidebar;
use crate::shell::OverlayHost;
use crate::snapshot::Snapshot;
use crate::theme::provide_tokens;
use crate::window::WindowHost;

const SIZE: (f32, f32) = (360.0, 640.0);

/// A home with Documents in it and a bookmark of `Projects` called `Work`.
fn fixture(tag: &str) -> Places {
    let home: PathBuf = std::env::temp_dir().join(format!("bingux-place-menu-{}-{tag}", std::process::id()));
    for folder in ["Documents", "Projects"] {
        std::fs::create_dir_all(home.join(folder)).unwrap();
    }
    Places::build(
        &Sources {
            home: home.clone(),
            user_dirs: Some("XDG_DOCUMENTS_DIR=\"$HOME/Documents\"\n".to_owned()),
            mounts: None,
            bookmarks: Some(format!("file://{} Work\n", home.join("Projects").display())),
        },
        |path| path.is_dir(),
    )
}

#[derive(PartialEq)]
struct Scene {
    places: Places,
}

impl Component for Scene {
    fn render(&self) -> impl IntoElement {
        let _ = provide_tokens();
        WindowHost::new(OverlayHost::new(
            PlacesSidebar::new(self.places.clone(), Where::Recent, |_: Where| {})
                .on_open_new_tab(|_: Where| {})
                .on_empty_trash(|_: ()| {}),
        ))
    }
}

fn start(tag: &str) -> Snapshot {
    let places = fixture(tag);
    let mut shot = Snapshot::new(move || Scene { places: places.clone() }.into(), SIZE);
    shot.settle(Duration::from_millis(400));
    shot
}

fn labels(shot: &mut Snapshot) -> Vec<String> {
    shot.runner()
        .find_many(|_, element| Label::try_downcast(element).map(|label| label.text.to_string()))
}

/// What the elements say they are, which is where a dialog keeps its title.
fn titles(shot: &mut Snapshot) -> Vec<String> {
    shot.runner().find_many(|_, element| {
        let a11y = element.accessibility();
        a11y.builder.label().map(str::to_owned)
    })
}

fn centre_of(shot: &mut Snapshot, text: &str) -> (f32, f32) {
    let found = shot.runner().find_many(|node, element| {
        let label = Label::try_downcast(element)?;
        (label.text == text).then(|| {
            let area = node.layout().area;
            (area.min_x() + area.width() / 2.0, area.min_y() + area.height() / 2.0)
        })
    });
    *found.first().unwrap_or_else(|| panic!("no label {text}"))
}

fn right_click(shot: &mut Snapshot, text: &str) {
    let (x, y) = centre_of(shot, text);
    shot.right_click(x, y);
    shot.settle(Duration::from_millis(300));
}

fn has(labels: &[String], text: &str) -> bool {
    labels.iter().any(|label| label == text)
}

#[test]
fn a_user_folders_menu_has_open_new_tab_copy_path_and_properties() {
    let mut shot = start("folder");
    right_click(&mut shot, "Documents");
    let shown = labels(&mut shot);
    for wanted in ["Open", "Open in new tab", "Copy path", "Properties", "Alt+Enter"] {
        assert!(has(&shown, wanted), "{wanted} in {shown:?}");
    }
    assert!(!has(&shown, "Rename…") && !has(&shown, "Remove from bookmarks"), "{shown:?}");
}

#[test]
fn recent_has_no_properties() {
    let mut shot = start("recent");
    right_click(&mut shot, "Recent");
    let shown = labels(&mut shot);
    assert!(has(&shown, "Open"), "{shown:?}");
    assert!(!has(&shown, "Properties") && !has(&shown, "Copy path"), "{shown:?}");
}

#[test]
fn properties_on_a_place_opens_the_dialog_of_that_folder() {
    let mut shot = start("properties");
    right_click(&mut shot, "Documents");
    assert!(shot.click_text("Properties"));
    shot.settle(Duration::from_millis(600));
    let shown = titles(&mut shot);
    assert!(has(&shown, "Properties of Documents"), "{shown:?}");
}

#[test]
fn a_bookmark_is_called_by_its_folder_in_properties_and_has_rename_and_remove() {
    let mut shot = start("bookmark");
    right_click(&mut shot, "Work");
    let shown = labels(&mut shot);
    for wanted in ["Rename…", "Remove from bookmarks", "Copy path", "Properties"] {
        assert!(has(&shown, wanted), "{wanted} in {shown:?}");
    }
    assert!(shot.click_text("Properties"));
    shot.settle(Duration::from_millis(600));
    let shown = titles(&mut shot);
    assert!(has(&shown, "Properties of Projects"), "{shown:?}");
}

#[test]
fn the_menu_key_chord_and_alt_enter_work_on_the_focused_row() {
    let mut shot = start("keys");
    // A click on a row focuses it (and opens it, which here goes nowhere).
    let (x, y) = centre_of(&mut shot, "Work");
    shot.click(x, y);
    shot.settle(Duration::from_millis(200));
    shot.key_with(Key::Named(NamedKey::F10), Modifiers::SHIFT);
    shot.settle(Duration::from_millis(300));
    let shown = labels(&mut shot);
    assert!(has(&shown, "Rename…") && has(&shown, "Properties"), "{shown:?}");
    shot.key(Key::Named(NamedKey::Escape));
    shot.settle(Duration::from_millis(300));

    shot.key_with(Key::Named(NamedKey::Enter), Modifiers::ALT);
    shot.settle(Duration::from_millis(600));
    let shown = titles(&mut shot);
    assert!(has(&shown, "Properties of Projects"), "{shown:?}");
}
