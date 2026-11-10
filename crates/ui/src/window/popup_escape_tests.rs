//! Escape closes a popup even when a field inside it has the keyboard. The
//! calendar's event editor is a popup whose title field is autofocused: typing
//! then Escape must close the popup.

use std::time::Duration;

use freya::prelude::*;

use crate::controls::button::action_button;
use crate::controls::text_field::text_field;
use crate::snapshot::Snapshot;
use crate::theme::provide_tokens;
use crate::window::{Window, WindowHost, WindowParent, popup, use_window};

const TITLE: &str = "Event";
const OPEN: &str = "Open form";

#[derive(PartialEq)]
struct Root;

impl Component for Root {
    fn render(&self) -> impl IntoElement {
        let _ = provide_tokens();
        WindowHost::new(Page)
    }
}

#[derive(PartialEq)]
struct Page;

impl Component for Page {
    fn render(&self) -> impl IntoElement {
        let window = use_window();
        rect().expanded().child(action_button(OPEN, move |_: ()| {
            if let Window::Main(main) = window {
                main.open(popup(TITLE, Body));
            }
        }))
    }
}

#[derive(PartialEq)]
struct Body;

impl Component for Body {
    fn render(&self) -> impl IntoElement {
        let value = use_state(String::new);
        text_field(value, |_: String| {}).autofocus()
    }
}

/// Whether the popup's title is drawn.
fn form_open(shot: &mut Snapshot) -> bool {
    !shot
        .runner()
        .find_many(|_, element| {
            Label::try_downcast(element).and_then(|label| (label.text == TITLE).then_some(()))
        })
        .is_empty()
}

#[test]
fn one_escape_closes_a_popup_whose_autofocused_field_was_typed_in() {
    let mut shot = Snapshot::new(|| Root.into_element(), (800.0, 600.0));
    shot.settle(Duration::from_millis(900));
    assert!(shot.click_text(OPEN), "the button opened the popup");
    shot.settle(Duration::from_millis(900));
    assert!(form_open(&mut shot), "the popup is up");
    shot.key(Key::Character("x".to_owned()));
    shot.settle(Duration::from_millis(200));
    shot.key(Key::Named(NamedKey::Escape));
    shot.settle(Duration::from_millis(1500));
    assert!(!form_open(&mut shot), "one Escape closes the popup");
}
