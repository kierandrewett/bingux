//! A context menu never paints before it is placed: from the frame it mounts
//! until it settles, whenever the menu is visible at all it is where it will
//! stay. A menu that drew for a frame or two at the wrong place (the corner of
//! the window, or its unmeasured size) and then jumped was the flash behind
//! "menu items appear when they should not".

use std::time::Duration;

use freya::prelude::*;

use super::menu::{menu_item, menu_separator};
use super::overlays::{OverlayHost, Overlays};
use crate::snapshot::Snapshot;
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
        let _ = crate::surface::use_tokens();
        let overlays = try_consume_context::<Overlays>();
        rect()
            .expanded()
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
                        menu_item("Copy the whole element", |_| {}),
                        menu_item("Paste", |_| {}),
                        menu_separator(),
                        menu_item("Properties", |_| {}),
                    ],
                );
            })
    }
}

/// What a frame shows of the menu: the top-left of its "Properties" label and
/// the product of the opacities of every box around it that has one.
fn frame_of_menu(shot: &mut Snapshot) -> Option<((f32, f32), f32)> {
    let labels = shot.runner().find_many(|node, element| {
        let label = Label::try_downcast(element)?;
        (label.text == "Properties").then(|| node.layout().area)
    });
    let area = labels.first().copied()?;
    let centre = torin::prelude::Point2D::new(
        area.min_x() + area.width() / 2.0,
        area.min_y() + area.height() / 2.0,
    );
    let opacity = shot
        .runner()
        .find_many(|node, element| {
            let own = element.effect()?.opacity?;
            node.layout().area.contains(centre).then_some(own)
        })
        .into_iter()
        .product::<f32>();
    Some(((area.min_x(), area.min_y()), opacity))
}

#[test]
fn a_context_menu_is_never_visible_away_from_where_it_settles() {
    let mut shot = Snapshot::new(
        || Root.into_element(),
        (800.0, 600.0),
    );
    shot.settle(Duration::from_millis(300));
    assert!(
        frame_of_menu(&mut shot).is_none(),
        "a menu is open before anyone asked for one"
    );

    shot.right_click(300.0, 200.0);
    let mut frames = vec![frame_of_menu(&mut shot)];
    for _ in 0..40 {
        shot.settle(Duration::from_millis(16));
        frames.push(frame_of_menu(&mut shot));
    }
    let settled = frames
        .last()
        .copied()
        .flatten()
        .expect("the menu is open once it has settled");
    assert!(settled.1 > 0.99, "the settled menu is fully opaque, not {}", settled.1);
    assert!(
        (settled.0.0 - 300.0).abs() < 200.0 && (settled.0.1 - 200.0).abs() < 200.0,
        "the menu settles by the pointer, not at {:?}",
        settled.0
    );
    for (index, frame) in frames.iter().enumerate() {
        let Some((at, opacity)) = frame else { continue };
        if *opacity > 0.02 {
            assert!(
                (at.0 - settled.0.0).abs() < 1.0 && (at.1 - settled.0.1).abs() < 1.0,
                "frame {index}: the menu shows at {at:?} (opacity {opacity}) but settles at {:?}",
                settled.0
            );
        }
    }
}
