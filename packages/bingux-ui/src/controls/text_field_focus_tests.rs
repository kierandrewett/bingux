//! A text field keeps its focus through everything that happens to its own
//! context menu: it stays focused while the menu is up and after the menu
//! closes by an item, by Escape or by a click outside it. The click outside is
//! the menu's shield's, not a click on the page, so it must not blur the field.

use std::time::Duration;

use freya::prelude::*;

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

/// The accessibility id that has the keyboard focus.
fn focused(shot: &mut Snapshot) -> AccessibilityId {
    shot.runner()
        .run_in(|| *Platform::get().current_window().focused_accessibility_id.peek())
}

const FIELD: (f32, f32) = (120.0, 40.0);
const OUTSIDE: (f32, f32) = (700.0, 500.0);

/// A window with the field focused and its context menu open.
fn with_menu_open() -> (Snapshot, AccessibilityId) {
    let mut shot = Snapshot::new(|| Root.into_element(), (800.0, 600.0));
    shot.settle(Duration::from_millis(300));
    shot.click(FIELD.0, FIELD.1);
    shot.settle(Duration::from_millis(200));
    let field = focused(&mut shot);
    shot.right_click(FIELD.0, FIELD.1);
    shot.settle(Duration::from_millis(400));
    (shot, field)
}

#[test]
fn a_click_outside_that_dismisses_the_menu_keeps_the_field_focused() {
    let (mut shot, field) = with_menu_open();
    assert_ne!(
        field,
        freya_core::accessibility::tree::ACCESSIBILITY_ROOT_ID,
        "the click focused the field"
    );
    shot.click(OUTSIDE.0, OUTSIDE.1);
    shot.settle(Duration::from_millis(400));
    assert_eq!(focused(&mut shot), field, "the field lost its focus");
}

#[test]
fn a_slow_click_outside_keeps_the_field_focused_too() {
    let (mut shot, field) = with_menu_open();
    // The menu has gone before the button is released: the release lands on
    // the page, not on the menu's shield.
    shot.press(OUTSIDE);
    shot.settle(Duration::from_millis(400));
    shot.release(OUTSIDE);
    shot.settle(Duration::from_millis(400));
    assert_eq!(focused(&mut shot), field, "the field lost its focus");
}

#[test]
fn escape_keeps_the_field_focused() {
    let (mut shot, field) = with_menu_open();
    shot.key(Key::Named(NamedKey::Escape));
    shot.settle(Duration::from_millis(400));
    assert_eq!(focused(&mut shot), field, "the field lost its focus");
}

#[test]
fn a_click_on_the_page_with_no_menu_still_blurs_the_field() {
    let mut shot = Snapshot::new(|| Root.into_element(), (800.0, 600.0));
    shot.settle(Duration::from_millis(300));
    shot.click(FIELD.0, FIELD.1);
    shot.settle(Duration::from_millis(200));
    let field = focused(&mut shot);
    // The first release outside only ends the drag that the click in the field
    // began; the next one is a click outside.
    shot.click(OUTSIDE.0, OUTSIDE.1);
    shot.settle(Duration::from_millis(200));
    shot.click(OUTSIDE.0, OUTSIDE.1);
    shot.settle(Duration::from_millis(400));
    assert_ne!(focused(&mut shot), field, "a click on empty page is a click outside");
}
