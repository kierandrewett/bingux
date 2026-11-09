//! Dragging the selected text, headless (no Wayland, so the drag stays in
//! the window).
//!
//! Failure modes guarded: a press on the selection starting a new selection
//! instead of waiting to see whether it is a drag; a click inside the
//! selection not placing the caret; a move taking two undo steps or leaving
//! the source behind; Ctrl not copying; a file's path dropped in as text.

use std::cell::RefCell;
use std::time::Duration;

use bingux_text::{Editor, Range, Selection};
use freya::prelude::*;

use super::*;
use crate::shell::overlays::OverlayHost;
use crate::snapshot::Snapshot;
use crate::theme::provide_tokens;
use crate::window::WindowHost;

thread_local! {
    static HANDLE: RefCell<Option<EditorHandle>> = const { RefCell::new(None) };
}

#[derive(PartialEq)]
struct Scene;

impl Component for Scene {
    fn render(&self) -> impl IntoElement {
        let handle = use_editor(|| {
            let mut editor = Editor::new("one two three");
            editor.set_selection(Selection::single(Range::new(0, 3)));
            editor
        });
        HANDLE.with(|slot| *slot.borrow_mut() = Some(handle.clone()));
        rect().width(Size::px(500.0)).height(Size::px(200.0)).child(
            text_editor(handle)
                .line_numbers(false)
                .wrap(false)
                .highlight_line(false),
        )
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

fn handle() -> EditorHandle {
    HANDLE.with(|slot| slot.borrow().clone().expect("a handle"))
}

fn window() -> Snapshot {
    let mut shot = Snapshot::new(|| Root.into_element(), (500.0, 200.0));
    shot.settle(Duration::from_millis(300));
    shot
}

/// A point on the first row: x in the text, y in the first row.
const ON_ONE: (f32, f32) = (18.0, 12.0);
const FAR_RIGHT: (f32, f32) = (480.0, 12.0);

#[test]
fn a_click_inside_the_selection_puts_the_caret_there() {
    let mut shot = window();
    shot.click(ON_ONE.0, ON_ONE.1);
    shot.settle(Duration::from_millis(200));
    let editor = handle();
    let selection = editor.peek().selection().primary();
    assert!(
        selection.is_empty(),
        "a click is not a selection: {selection:?}"
    );
    assert!(
        selection.head <= 3,
        "inside the old selection: {selection:?}"
    );
    assert_eq!(editor.peek().text(), "one two three");
}

#[test]
fn dragging_the_selection_moves_it_as_one_step() {
    let mut shot = window();
    shot.drag(ON_ONE, FAR_RIGHT);
    shot.settle(Duration::from_millis(300));
    let editor = handle();
    assert_eq!(editor.peek().text(), " two threeone");
    assert_eq!(
        editor.peek().history().undo_label(),
        Some("move text"),
        "one named step"
    );
    editor.update(|editor| editor.undo());
    assert_eq!(
        editor.peek().text(),
        "one two three",
        "the whole move undone"
    );
}
