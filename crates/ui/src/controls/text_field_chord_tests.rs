//! A chord typed into a text field inserts nothing, so a Ctrl, Super or Alt
//! shortcut the window handles (Calculator's inspector key, Ctrl+Alt+Shift+I)
//! cannot put a letter into the field. A plain key still types, and AltGr
//! symbols (Ctrl+Alt+Q gives `@` on German layouts) still type.

use std::time::Duration;

use freya::prelude::*;
use freya_core::element::ElementExt;

use super::text_field;
use crate::shell::overlays::OverlayHost;
use crate::snapshot::Snapshot;
use crate::style::*;
use crate::surface::use_tokens;
use crate::theme::provide_tokens;
use crate::window::WindowHost;

/// The window root: it provides the tokens the window host reads.
#[derive(PartialEq)]
struct Root;

impl Component for Root {
    fn render(&self) -> impl IntoElement {
        let _ = provide_tokens();
        WindowHost::new(OverlayHost::new(Page))
    }
}

#[derive(PartialEq)]
struct Page;

impl Component for Page {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let value = use_state(|| "hello".to_string());
        rect()
            .tw(&tokens, &[flex_col(), p(S4), w_full(), h_full()])
            .child(text_field(value, |_: String| {}))
    }
}

const FIELD: (f32, f32) = (120.0, 40.0);

/// A window with the field focused and the caret at the end of "hello".
fn focused_field() -> Snapshot {
    let mut shot = Snapshot::new(|| Root.into_element(), (800.0, 600.0));
    shot.settle(Duration::from_millis(300));
    shot.click(FIELD.0, FIELD.1);
    shot.settle(Duration::from_millis(200));
    shot.key(Key::Named(NamedKey::End));
    shot.settle(Duration::from_millis(200));
    shot
}

/// Whether the field shows exactly `value`.
fn shows(shot: &mut Snapshot, value: &str) -> bool {
    !shot
        .runner()
        .find_many(|_, element| (text_of(element)? == value).then_some(()))
        .is_empty()
}

fn text_of(element: &dyn ElementExt) -> Option<String> {
    if let Some(label) = Label::try_downcast(element) {
        return Some(label.text.to_string());
    }
    Paragraph::try_downcast(element).map(|paragraph| {
        paragraph
            .spans
            .iter()
            .map(|span| span.text.to_string())
            .collect()
    })
}

fn type_chord(shot: &mut Snapshot, key: &str, modifiers: Modifiers) {
    shot.key_with(Key::Character(key.to_owned()), modifiers);
    shot.settle(Duration::from_millis(200));
}

#[test]
fn a_plain_letter_still_types() {
    let mut shot = focused_field();
    type_chord(&mut shot, "i", Modifiers::empty());
    assert!(shows(&mut shot, "helloi"), "plain i did not type");
}

#[test]
fn ctrl_alt_shift_i_types_nothing() {
    let mut shot = focused_field();
    type_chord(
        &mut shot,
        "I",
        Modifiers::CONTROL | Modifiers::ALT | Modifiers::SHIFT,
    );
    assert!(shows(&mut shot, "hello"), "Ctrl+Alt+Shift+I typed text");
}

#[test]
fn super_i_types_nothing() {
    let mut shot = focused_field();
    type_chord(&mut shot, "i", Modifiers::META);
    assert!(shows(&mut shot, "hello"), "Super+I typed text");
}

#[test]
fn alt_i_types_nothing() {
    let mut shot = focused_field();
    type_chord(&mut shot, "i", Modifiers::ALT);
    assert!(shows(&mut shot, "hello"), "Alt+I typed text");
}

#[test]
fn ctrl_a_selects_and_types_nothing() {
    let mut shot = focused_field();
    type_chord(&mut shot, "a", Modifiers::CONTROL);
    assert!(shows(&mut shot, "hello"), "Ctrl+A changed the text");
}

#[test]
fn ctrl_shift_f_types_nothing() {
    let mut shot = focused_field();
    type_chord(&mut shot, "F", Modifiers::CONTROL | Modifiers::SHIFT);
    assert!(shows(&mut shot, "hello"), "Ctrl+Shift+F typed text");
}

#[test]
fn altgr_symbol_still_types() {
    let mut shot = focused_field();
    type_chord(&mut shot, "@", Modifiers::CONTROL | Modifiers::ALT);
    assert!(shows(&mut shot, "hello@"), "AltGr @ did not type");
}
