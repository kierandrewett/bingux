//! An icon button shows no fill at rest.
//!
//! The main-menu button of the Files toolbar once drew the control fill all the
//! time: `menu_button(..).icon(..)` painted `Paint::Control` where `icon_button`
//! paints nothing, so the hamburger looked highlighted beside its neighbours.
//! These tests render the windows the buttons live in (a header with every kind
//! of icon button, and the Files toolbar under the main menu button) and ask
//! every icon button of the frame for the fill it paints. At rest, with nothing
//! hovered, pressed, open or focused, the answer is none. Hovering gives the
//! fill and leaving takes it away again, and Tab moves a ring, not a fill.
//!
//! The same check runs on the Settings header in `bingux-settings`.

use std::path::PathBuf;
use std::time::Duration;

use freya::prelude::*;

use super::menu::{menu_button, menu_item};
use super::overlays::OverlayHost;
use super::window::HeaderBar;
use crate::controls::icon_button;
use crate::files::{Browse, TabMemory, Where, browse};
use crate::icons::Icon;
use crate::snapshot::{IconButtonLook, Snapshot};
use crate::style::*;
use crate::theme::provide_tokens;

const SIZE: (f32, f32) = (1000.0, 600.0);

/// A header with one of each kind of icon button.
#[derive(PartialEq)]
struct Header;

impl Component for Header {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        rect().tw(&tokens, &[w_full(), h_full(), bg(Paint::Bg)]).child(OverlayHost::new(
            HeaderBar::new("Window")
                .leading(icon_button(Icon::ListAdd, |_| {}).tooltip("New tab"))
                .trailing(menu_button("Main menu", vec![menu_item("About", |_| {})]).icon(Icon::OpenMenu))
                .trailing(icon_button(Icon::Refresh, |_| {}).tooltip("Reload")),
        ))
    }
}

/// The Files window's header and toolbar: the main menu button above the
/// file view, whose toolbar has Back, Forward, Up, Reload and New folder.
#[derive(PartialEq)]
struct FilesWindow;

impl Component for FilesWindow {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        let memory = use_hook(|| TabMemory::new(Where::Folder(PathBuf::from("/var/tmp"))));
        rect().tw(&tokens, &[w_full(), h_full(), flex_col(), bg(Paint::Bg)]).child(OverlayHost::new(
            rect()
                .tw(&tokens, &[w_full(), h_full(), flex_col()])
                .child(
                    HeaderBar::new("Files")
                        .trailing(menu_button("Main menu", vec![menu_item("About", |_| {})]).icon(Icon::OpenMenu)),
                )
                .child(rect().tw(&tokens, &[w_full(), h_flex(), flex_col()]).child(browse(Browse::new(memory)))),
        ))
    }
}

fn settled(root: fn() -> Element) -> Snapshot {
    let mut shot = Snapshot::new(root, SIZE);
    shot.settle(Duration::from_millis(900));
    shot
}

fn header() -> Element {
    Header.into_element()
}

fn files_window() -> Element {
    FilesWindow.into_element()
}

/// The buttons that fill now, as `name (component)`.
fn filled(buttons: &[IconButtonLook]) -> Vec<String> {
    buttons
        .iter()
        .filter(|button| button.fill.is_some())
        .map(|button| format!("{} ({})", button.name, button.component))
        .collect()
}

fn menu_of(buttons: &[IconButtonLook]) -> &IconButtonLook {
    buttons
        .iter()
        .find(|button| button.component == "IconMenuButton")
        .expect("the main menu button is on screen")
}

#[test]
fn no_icon_button_of_a_header_has_a_fill_at_rest() {
    let mut shot = settled(header);
    let buttons = shot.icon_buttons();
    assert!(buttons.len() >= 3, "the header's three icon buttons are on screen: {buttons:?}");
    assert_eq!(filled(&buttons), Vec::<String>::new(), "icon buttons that fill at rest");
}

#[test]
fn no_icon_button_of_the_files_toolbar_has_a_fill_at_rest() {
    let mut shot = settled(files_window);
    let buttons = shot.icon_buttons();
    // The menu button and the toolbar's Back, Forward, Up, Reload and New folder.
    assert!(buttons.len() >= 6, "the menu button and the toolbar's buttons are on screen: {buttons:?}");
    assert_eq!(filled(&buttons), Vec::<String>::new(), "icon buttons that fill at rest");
}

#[test]
fn hovering_fills_the_menu_button_and_leaving_takes_the_fill_away() {
    let mut shot = settled(header);
    let area = menu_of(&shot.icon_buttons()).area;
    let (x, y) = (area.min_x() + area.width() / 2.0, area.min_y() + area.height() / 2.0);
    shot.hover(x, y).settle(Duration::from_millis(300));
    assert!(menu_of(&shot.icon_buttons()).fill.is_some(), "a hovered menu button shows its state layer");
    shot.hover(10.0, SIZE.1 - 10.0).settle(Duration::from_millis(300));
    assert_eq!(filled(&shot.icon_buttons()), Vec::<String>::new(), "the fill stayed after the pointer left");
}

#[test]
fn keyboard_focus_draws_a_ring_and_no_fill() {
    let mut shot = settled(files_window);
    for press in 1..=3 {
        shot.key(Key::Named(NamedKey::Tab)).settle(Duration::from_millis(300));
        assert_eq!(
            filled(&shot.icon_buttons()),
            Vec::<String>::new(),
            "Tab press {press} left a filled icon button"
        );
    }
}
