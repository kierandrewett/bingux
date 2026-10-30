//! A menu that lists applications shows each one's icon: in the leading
//! column, the same size on every row, from the first frame to the last, at
//! desktop and phone sizes. The icon slot holds its size while the lookup
//! runs, so no row moves when an icon arrives.

use std::time::Duration;

use freya::prelude::*;
use torin::prelude::Area;

use super::menu::{app_menu_items, leading_width, menu_item, submenu};
use super::overlays::{OverlayHost, Overlays};
use crate::app_icon::AppChoice;
use crate::snapshot::Snapshot;
use crate::style::IconSize;
use crate::theme::provide_tokens;
use crate::window::WindowHost;

const APPS: [(&str, &str); 4] = [
    ("Celluloid", "io.github.celluloid_player.Celluloid"),
    ("mpv Media Player", "mpv"),
    ("VLC media player", "vlc"),
    // An icon the theme does not have: the generic glyph, not a gap.
    ("Zen Browser", "no-such-icon-in-any-theme"),
];

fn choices() -> Vec<AppChoice> {
    APPS.iter()
        .map(|(label, icon)| AppChoice::new(*label, *label, *icon))
        .collect()
}

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
                        menu_item("Open", |_| {}),
                        submenu("Open with", app_menu_items(choices(), |_| {})),
                        menu_item("Properties", |_| {}),
                    ],
                );
            })
    }
}

/// What a row of the submenu shows: its label's area and the icon slot, the
/// 20 px square before the label.
#[derive(Clone, Copy, Debug, PartialEq)]
struct RowBoxes {
    label: Area,
    slot: Area,
}

fn rows(shot: &mut Snapshot, size: f32) -> Vec<RowBoxes> {
    APPS.iter()
        .map(|(label, _)| {
            let label = shot
                .runner()
                .find_many(|node, element| {
                    let text = Label::try_downcast(element)?;
                    (text.text == *label).then(|| node.layout().area)
                })
                .into_iter()
                .next()
                .unwrap_or_else(|| panic!("the row {label}"));
            // The slot: a box of the icon size, left of the label, on its line.
            let slot = shot
                .runner()
                .find_many(|node, _| {
                    let area = node.layout().area;
                    ((area.width() - size).abs() < 0.5
                        && (area.height() - size).abs() < 0.5
                        && area.max_x() <= label.min_x() + 0.5
                        && area.min_y() >= label.min_y() - size
                        && area.max_y() <= label.max_y() + size)
                        .then_some(area)
                })
                .into_iter()
                .next()
                .unwrap_or_else(|| panic!("the icon slot of {label:?}"));
            RowBoxes { label, slot }
        })
        .collect()
}

fn open_submenu(width: f32) -> (Snapshot, f32) {
    let mut shot = Snapshot::new(|| Root.into_element(), (width, 800.0));
    let size = shot.runner().run_in(|| IconSize::AppSmall.px());
    shot.settle(Duration::from_millis(300));
    shot.right_click(40.0, 60.0);
    shot.settle(Duration::from_millis(300));
    assert!(shot.click_text("Open with"), "the Open with row");
    (shot, size)
}

fn check(width: f32) {
    let (mut shot, size) = open_submenu(width);
    // The first frames after the submenu opens: icons still loading.
    shot.settle(Duration::from_millis(16));
    let before = rows(&mut shot, size);
    // Long enough for every lookup to finish.
    shot.settle(Duration::from_millis(1500));
    let after = rows(&mut shot, size);
    assert_eq!(before.len(), APPS.len());
    for (first, last) in before.iter().zip(&after) {
        assert_eq!(first, last, "a row moved when its icon arrived");
    }
    for row in &after {
        assert!((row.slot.width() - size).abs() < 0.5, "an icon is {size} px wide");
        assert!(
            (row.slot.min_x() - after[0].slot.min_x()).abs() < 0.5
                && (row.label.min_x() - after[0].label.min_x()).abs() < 0.5,
            "icons and labels share one column"
        );
    }
}

#[test]
fn every_app_row_has_an_icon_slot_of_one_size_at_desktop() {
    check(800.0);
}

#[test]
fn every_app_row_has_an_icon_slot_of_one_size_at_phone() {
    check(390.0);
}

#[test]
fn the_leading_column_fits_the_widest_item_of_a_mixed_panel() {
    let mut shot = Snapshot::new(|| Root.into_element(), (400.0, 300.0));
    let (control, app_size) = shot
        .runner()
        .run_in(|| (crate::style::Dim::IconControl.px(), IconSize::AppSmall.px()));
    let plain = || menu_item("Plain", |_| {});
    let app = || app_menu_items(choices().into_iter().take(1), |_| {}).remove(0);
    assert_eq!(leading_width(&[plain(), plain()]), None);
    let ticks = shot
        .runner()
        .run_in(|| leading_width(&[plain(), plain().checked(false)]));
    assert_eq!(ticks, Some(control));
    // An application row anywhere widens the column for every row.
    let widths = shot.runner().run_in(|| {
        [
            leading_width(&[plain(), app()]),
            leading_width(&[plain().checked(true), app()]),
            leading_width(&[app(), app()]),
        ]
    });
    assert_eq!(widths, [Some(app_size); 3]);
}
