//! A row of controls in a menu (the zoom row of a browser), a row's detail and
//! a row's tooltip, in a window: a control runs and leaves the menu open, a
//! control that closes the menu closes it, the keyboard walks the controls, a
//! disabled row says why when the pointer rests on it.

use std::time::Duration;

use freya::prelude::*;

use super::menu::{menu_item, submenu};
use super::menu_controls::{menu_control, menu_controls, menu_value_control};
use super::overlays::{OverlayHost, Overlays};
use crate::icons::Icon;
use crate::nav::{Location, RouteTable, provide_router};
use crate::snapshot::Snapshot;
use crate::theme::provide_tokens;
use crate::window::WindowHost;

#[derive(PartialEq)]
struct Root;

impl Component for Root {
    fn render(&self) -> impl IntoElement {
        let _ = provide_tokens();
        // A window with a router counts an open menu as a modal, as every app does.
        provide_router(
            std::sync::Arc::new(RouteTable::new("test")),
            &Location::rooted("test"),
        );
        WindowHost::new(OverlayHost::new(Page))
    }
}

#[derive(PartialEq)]
struct Page;

impl Component for Page {
    fn render(&self) -> impl IntoElement {
        let _ = crate::surface::use_tokens();
        let mut level = use_state(|| 100u32);
        let mut full = use_state(|| false);
        let overlays = try_consume_context::<Overlays>();
        rect()
            .expanded()
            .child(format!("full screen: {}", full()))
            .on_pointer_down(move |event: Event<PointerEventData>| {
                let right = matches!(
                    event.data(),
                    PointerEventData::Mouse(mouse) if mouse.button == Some(MouseButton::Right)
                );
                let (Some(overlays), true) = (overlays, right) else {
                    return;
                };
                let at = event.global_location();
                overlays.show_menu(
                    (at.x as f32, at.y as f32),
                    vec![
                        menu_controls(
                            "Zoom",
                            vec![
                                menu_control(Icon::MagnifierMinus, "Zoom out", move |_| {
                                    level.set(level() - 10)
                                }),
                                menu_value_control(
                                    move || format!("{}%", level()),
                                    "Reset zoom",
                                    move |_| level.set(100),
                                ),
                                menu_control(Icon::MagnifierPlus, "Zoom in", move |_| {
                                    level.set(level() + 10)
                                }),
                                menu_control(Icon::CornersOut, "Full screen", move |_| {
                                    full.set(true)
                                })
                                .divided()
                                .closes_menu(),
                            ],
                        ),
                        menu_item("New tab", |_| {}),
                        submenu("Profile", vec![menu_item("Work", |_| {})]).detail("Personal"),
                        menu_item("Passwords", |_| {})
                            .disabled()
                            .tooltip("Passwords are not built yet"),
                    ],
                );
            })
    }
}

fn shows(shot: &mut Snapshot, text: &str) -> bool {
    !shot
        .runner()
        .find_many(|_, element| Label::try_downcast(element).filter(|label| label.text == text))
        .is_empty()
}

fn menu() -> Snapshot {
    let mut shot = Snapshot::new(|| Root.into_element(), (600.0, 500.0));
    shot.settle(Duration::from_millis(300));
    shot.right_click(40.0, 60.0);
    shot.settle(Duration::from_millis(300));
    shot
}

#[test]
fn a_control_runs_and_the_menu_stays_open() {
    let mut shot = menu();
    assert!(shows(&mut shot, "100%"));
    assert!(shot.click_name("Zoom in"), "the plus button");
    shot.settle(Duration::from_millis(200));
    assert!(shot.click_name("Zoom in"), "and again, with no reopening");
    shot.settle(Duration::from_millis(200));
    assert!(shows(&mut shot, "120%"), "the value follows the presses");
    assert!(shows(&mut shot, "New tab"), "the menu is still open");
    assert!(shot.click_text("120%"));
    shot.settle(Duration::from_millis(200));
    assert!(shows(&mut shot, "100%"));
    assert!(shows(&mut shot, "New tab"));
}

#[test]
fn a_control_that_closes_the_menu_closes_it_and_runs() {
    let mut shot = menu();
    assert!(shot.click_name("Full screen"));
    shot.settle(Duration::from_millis(400));
    assert!(!shows(&mut shot, "New tab"), "the menu closed");
    assert!(shows(&mut shot, "full screen: true"), "and the control ran");
}

#[test]
fn the_keyboard_walks_the_controls_of_the_row() {
    let mut shot = menu();
    // The pointer opened it: nothing is highlighted. Down reaches the row.
    shot.key(Key::Named(NamedKey::ArrowDown));
    shot.key(Key::Named(NamedKey::ArrowRight));
    shot.key(Key::Named(NamedKey::ArrowRight));
    shot.key(Key::Named(NamedKey::Enter));
    shot.settle(Duration::from_millis(200));
    assert!(shows(&mut shot, "110%"), "Enter ran the plus button");
    assert!(shows(&mut shot, "New tab"), "and the menu stayed open");
    shot.key(Key::Named(NamedKey::ArrowLeft));
    shot.key(Key::Named(NamedKey::Enter));
    shot.settle(Duration::from_millis(200));
    assert!(shows(&mut shot, "100%"), "Left moved to Reset zoom");
}

#[test]
fn a_row_shows_its_detail_after_its_label() {
    let mut shot = menu();
    assert!(shows(&mut shot, "Profile"));
    assert!(shows(&mut shot, "Personal"));
}

#[test]
fn a_disabled_row_says_why_when_the_pointer_rests_on_it() {
    let mut shot = menu();
    assert!(!shows(&mut shot, "Passwords are not built yet"));
    assert!(shot.hover_text("Passwords"));
    shot.settle(Duration::from_millis(900));
    assert!(shows(&mut shot, "Passwords are not built yet"));
}
