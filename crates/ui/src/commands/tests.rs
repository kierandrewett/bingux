//! The router in a window, headless: a view's shortcut runs only while the
//! focus is in the view and beats the window's; a text field keeps its own
//! Ctrl+Z; the palette and the sheet open on their keys and run what is
//! picked; a dialog holds the keyboard.
//!
//! Failure modes guarded: a view command running with the focus elsewhere; a
//! window command running instead of the focused view's; the window's Undo
//! eating a text field's Ctrl+Z; a command running twice for one key; the
//! palette running a command before it has closed.

use std::cell::RefCell;
use std::time::Duration;

use freya::prelude::*;

use super::*;
use crate::controls::{action_button, text_field};
use crate::shell::overlays::OverlayHost;
use crate::snapshot::Snapshot;
use crate::theme::provide_tokens;
use crate::undo::{UndoStep, use_undo};
use crate::window::WindowHost;

thread_local! {
    static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn note(text: &str) {
    LOG.with(|log| log.borrow_mut().push(text.to_owned()));
}

fn take_log() -> Vec<String> {
    LOG.with(|log| std::mem::take(&mut *log.borrow_mut()))
}

#[derive(PartialEq)]
struct Scene;

impl Component for Scene {
    fn render(&self) -> impl IntoElement {
        let draft = use_state(String::new);
        let undo = use_undo();
        use_hook(move || {
            undo.push(UndoStep::new(
                "Rename",
                || note("undo rename"),
                || note("redo rename"),
            ));
        });
        use_window_commands(vec![
            Command::new("win.save", "Save", |_| note("window save")).shortcut("Ctrl+S"),
            Command::new("win.k", "Window K", |_| note("window k")).shortcut("Ctrl+K"),
            Command::new("win.off", "Off", |_| note("off ran"))
                .shortcut("Ctrl+J")
                .enabled(false),
        ]);
        let view = command_scope(
            "View",
            vec![Command::new("view.k", "View K", |_| note("view k")).shortcut("Ctrl+K")],
            rect()
                .child(action_button("Inside", |_| {}))
                .child(text_field(draft, |_: String| {}).placeholder("Field")),
        );
        rect()
            .expanded()
            .child(view)
            .child(action_button("Outside", |_| {}))
    }
}

#[derive(PartialEq)]
struct Root;

impl Component for Root {
    fn render(&self) -> impl IntoElement {
        let _tokens = provide_tokens();
        WindowHost::new(OverlayHost::new(Scene))
    }
}

fn window() -> Snapshot {
    take_log();
    let mut shot = Snapshot::new(|| Root.into_element(), (800.0, 600.0));
    shot.settle(Duration::from_millis(300));
    shot
}

fn chord(shot: &mut Snapshot, key: &str, modifiers: Modifiers) {
    shot.key_with(Key::Character(key.to_owned()), modifiers);
    shot.settle(Duration::from_millis(300));
}

fn has_class(shot: &mut Snapshot, name: &'static str) -> bool {
    !shot
        .runner()
        .find_many(|_, element| {
            (element.accessibility().builder.class_name() == Some(name)).then_some(())
        })
        .is_empty()
}

#[test]
fn a_views_shortcut_needs_the_focus_and_beats_the_windows() {
    let mut shot = window();
    chord(&mut shot, "k", Modifiers::CONTROL);
    assert_eq!(
        take_log(),
        ["window k"],
        "focus in no view: the window's command"
    );
    assert!(shot.click_text("Inside"));
    shot.settle(Duration::from_millis(200));
    chord(&mut shot, "k", Modifiers::CONTROL);
    assert_eq!(take_log(), ["view k"], "the focused view's command, once");
    chord(&mut shot, "s", Modifiers::CONTROL);
    assert_eq!(
        take_log(),
        ["window save"],
        "the window's commands still apply in a view"
    );
    assert!(shot.click_text("Outside"));
    shot.settle(Duration::from_millis(200));
    chord(&mut shot, "k", Modifiers::CONTROL);
    assert_eq!(take_log(), ["window k"], "focus left the view");
}

#[test]
fn a_disabled_command_takes_its_key_and_does_nothing() {
    let mut shot = window();
    chord(&mut shot, "j", Modifiers::CONTROL);
    assert!(take_log().is_empty());
}

#[test]
fn a_text_field_keeps_its_own_undo() {
    let mut shot = window();
    assert!(shot.click_text("Field"));
    shot.settle(Duration::from_millis(200));
    chord(&mut shot, "z", Modifiers::CONTROL);
    assert!(take_log().is_empty(), "Ctrl+Z in the field is the field's");
    assert!(shot.click_text("Inside"));
    shot.settle(Duration::from_millis(200));
    chord(&mut shot, "z", Modifiers::CONTROL);
    assert_eq!(take_log(), ["undo rename"]);
    chord(&mut shot, "Z", Modifiers::CONTROL | Modifiers::SHIFT);
    assert_eq!(take_log(), ["redo rename"]);
}

#[test]
fn the_palette_and_the_sheet_open_on_their_keys_and_hold_the_keyboard() {
    let mut shot = window();
    chord(&mut shot, "P", Modifiers::CONTROL | Modifiers::SHIFT);
    assert!(
        has_class(&mut shot, "CommandPalette"),
        "Ctrl+Shift+P opens the palette"
    );
    chord(&mut shot, "s", Modifiers::CONTROL);
    assert!(
        take_log().is_empty(),
        "a shortcut does not reach the window behind the palette"
    );
    // Type a query and run the match: it runs once the palette has gone.
    for letter in ["s", "a", "v"] {
        shot.key(Key::Character(letter.to_owned()));
    }
    shot.settle(Duration::from_millis(200));
    shot.key(Key::Named(NamedKey::Enter));
    shot.settle(Duration::from_millis(800));
    assert!(!has_class(&mut shot, "CommandPalette"));
    assert_eq!(take_log(), ["window save"]);

    chord(&mut shot, "?", Modifiers::CONTROL | Modifiers::SHIFT);
    assert!(
        has_class(&mut shot, "ShortcutsSheet"),
        "Ctrl+? opens the shortcuts sheet"
    );
    shot.key(Key::Named(NamedKey::Escape));
    shot.settle(Duration::from_millis(800));
    assert!(!has_class(&mut shot, "ShortcutsSheet"));
    chord(&mut shot, "/", Modifiers::CONTROL);
    assert!(has_class(&mut shot, "ShortcutsSheet"), "Ctrl+/ too");
}

// What a screen reader is told: the palette's field names the row the arrows
// reach, each row carries its shortcut and is named by its title alone, and
// Ctrl+Z says what it undid.

fn nodes_where<T>(
    shot: &mut Snapshot,
    pick: impl Fn(&accesskit::Node, Option<accesskit::NodeId>) -> Option<T>,
) -> Vec<T> {
    shot.runner().find_many(|_, element| {
        let data = element.accessibility();
        pick(&data.builder, data.a11y_id)
    })
}

#[test]
fn the_palette_field_names_the_highlighted_row_and_rows_carry_their_shortcut() {
    let mut shot = window();
    chord(&mut shot, "P", Modifiers::CONTROL | Modifiers::SHIFT);
    shot.key(Key::Named(NamedKey::ArrowDown));
    shot.settle(Duration::from_millis(200));
    let active: Vec<Option<accesskit::NodeId>> = nodes_where(&mut shot, |node, _| {
        (node.role() == accesskit::Role::SearchInput).then(|| node.active_descendant())
    });
    assert_eq!(active.len(), 1, "one palette field: {active:?}");
    let active = active[0].expect("the field names the highlighted row");
    let rows: Vec<(Option<accesskit::NodeId>, Option<String>, Option<String>)> =
        nodes_where(&mut shot, |node, id| {
            (node.role() == accesskit::Role::ListBoxOption).then(|| {
                (
                    id,
                    node.label().map(str::to_owned),
                    node.keyboard_shortcut().map(str::to_owned),
                )
            })
        });
    assert!(rows.iter().any(|(id, ..)| *id == Some(active)), "{rows:?}");
    let save = rows
        .iter()
        .find(|(_, label, _)| label.as_deref() == Some("Save"))
        .expect("a Save row");
    assert_eq!(save.2.as_deref(), Some("Ctrl+S"), "the shortcut is not part of the name");
}

#[test]
fn undo_from_the_keyboard_says_what_it_undid() {
    let mut shot = window();
    chord(&mut shot, "z", Modifiers::CONTROL);
    assert_eq!(take_log(), ["undo rename"]);
    let said: Vec<String> = nodes_where(&mut shot, |node, _| {
        (node.role() == accesskit::Role::Status && node.live() == Some(accesskit::Live::Polite))
            .then(|| node.label().unwrap_or_default().to_owned())
    });
    assert!(said.iter().any(|text| text == "Undid rename"), "{said:?}");
}
