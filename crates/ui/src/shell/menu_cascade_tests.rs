//! A cascade of menus paints every level as a real surface: the root menu, the View submenu, the
//! Sort by submenu and the one beyond it each have the same fill, border and shadow, and a level
//! that lands over the ones before it hides what is under it.
//!
//! The scene is the Files context menu on the window background: root (New folder .. Properties),
//! View, Sort by, and for the fourth level Then by. In a 700 px window Sort by has no room on the
//! right of View, so it opens on the left of it, over the root menu; the layout is the menu's
//! own and is left alone.
//!
//! A card is judged by its pixels. The same cascade is also drawn in a window wide enough that no
//! level lies over another; the last card, inset past its rounded corners, must be the same pixels
//! in both, so nothing of the menus under it shows through.

use std::time::Duration;

use freya::engine::prelude::{SkData, SkImage, raster_n32_premul};
use freya::prelude::*;
use torin::prelude::Area;

use super::menu::{MenuItem, menu_item, menu_separator, submenu};
use super::overlays::{OverlayHost, Overlays};
use crate::controls::direction::{Direction, window_directed};
use crate::snapshot::Snapshot;
use crate::style::{Dim, S2};
use crate::theme::provide_tokens;
use crate::window::WindowHost;

/// The window of the report, drawn at the compact density (rows 52 px tall), in which the third
/// level has no room on the right of the second.
const NARROW: (f32, f32) = (700.0, 500.0);
/// The same at the regular density, which a window from 740 px wide has: the third level has no
/// room here either, and a window wide enough for every level draws the same cards.
const REGULAR: (f32, f32) = (760.0, 500.0);
const WIDE: (f32, f32) = (1500.0, 500.0);
/// A phone: no level has room beside the one before it.
const PHONE: (f32, f32) = (390.0, 700.0);

/// What the menus are made of: the levels shown, and whether the labels and shortcuts of the
/// menus *under* the last one are swapped for others, which must make no difference to it.
#[derive(Clone, Copy, PartialEq)]
struct Scene {
    /// 3 is Sort by, 4 is Then by.
    depth: usize,
    rtl: bool,
    swapped: bool,
}

/// `text`, or other letters in the same places when the scene swaps what is under the last level.
fn text(scene: Scene, text: &str) -> String {
    if scene.swapped {
        // Every letter moved on by three, so each label stays one of its own.
        text.chars()
            .map(|c| match c {
                'a'..='z' => (b'a' + (c as u8 - b'a' + 3) % 26) as char,
                'A'..='Z' => (b'A' + (c as u8 - b'A' + 3) % 26) as char,
                _ => c,
            })
            .collect()
    } else {
        text.to_owned()
    }
}

/// A label of Sort by: under the level beyond it at depth 4, so swapped with the rest there.
fn text_under(scene: Scene, label: &str) -> String {
    if scene.depth > 3 { text(scene, label) } else { label.to_owned() }
}

fn items(scene: Scene) -> Vec<MenuItem> {
    let t = |label: &str| text(scene, label);
    let s = |label: &str| text_under(scene, label);
    let mut sort = vec![
        menu_item(s("Name"), |_| {}).checked(true),
        menu_item(s("Size"), |_| {}).checked(false),
        menu_item(s("Type"), |_| {}).checked(false),
        menu_item(s("Modified"), |_| {}).checked(false),
        menu_separator(),
        menu_item(s("Ascending"), |_| {}).checked(true),
        menu_item(s("Descending"), |_| {}).checked(false),
        menu_separator(),
        menu_item(s("Folders first"), |_| {}).checked(true),
    ];
    if scene.depth > 3 {
        sort.push(menu_separator());
        sort.push(submenu(
            "Then by",
            vec![
                menu_item("Date", |_| {}),
                menu_item("Owner", |_| {}),
                menu_item("Rating", |_| {}),
            ],
        ));
    }
    vec![
        menu_item(t("New folder"), |_| {}).shortcut(t("Ctrl+Shift+N")),
        menu_item(t("New file"), |_| {}),
        menu_separator(),
        menu_item(t("Paste"), |_| {}).shortcut(t("Ctrl+V")),
        menu_separator(),
        menu_item(t("Select all"), |_| {}).shortcut(t("Ctrl+A")),
        menu_item(t("Invert selection"), |_| {}).shortcut(t("Ctrl+Shift+A")),
        menu_separator(),
        submenu(
            "View",
            vec![
                menu_item(t("List"), |_| {}).checked(true),
                menu_item(t("Grid"), |_| {}).checked(false),
                menu_separator(),
                submenu(t("Icon size"), vec![menu_item("Small", |_| {}), menu_item("Large", |_| {})]),
                submenu("Sort by", sort),
                menu_separator(),
                menu_item(t("Show hidden files"), |_| {}).shortcut(t("Ctrl+H")),
            ],
        ),
        menu_item(t("Refresh"), |_| {}).shortcut(t("F5")),
        menu_item(t("Properties"), |_| {}).shortcut(t("Alt+Enter")),
    ]
}

#[derive(PartialEq)]
struct Root(Scene);

impl Component for Root {
    fn render(&self) -> impl IntoElement {
        let _ = provide_tokens();
        WindowHost::new(OverlayHost::new(Page(self.0)))
    }
}

#[derive(PartialEq)]
struct Page(Scene);

impl Component for Page {
    fn render(&self) -> impl IntoElement {
        let _ = crate::surface::use_tokens();
        let overlays = try_consume_context::<Overlays>();
        let scene = self.0;
        rect().expanded().on_pointer_down(move |event: Event<PointerEventData>| {
            let right = matches!(
                event.data(),
                PointerEventData::Mouse(mouse) if mouse.button == Some(MouseButton::Right)
            );
            let (Some(overlays), true) = (overlays, right) else {
                return;
            };
            let at = event.global_location();
            overlays.show_menu((at.x as f32, at.y as f32), items(scene));
        })
    }
}

/// The cascade opened down to `scene.depth` in a window of `window`, settled. The pointer opens
/// the menu 52 px in from the start edge, as in the report (the phone's menu is at its edge).
fn open(scene: Scene, window: (f32, f32)) -> Snapshot {
    let direction = if scene.rtl { Direction::Rtl } else { Direction::Ltr };
    let mut shot = Snapshot::with_scale(
        move || window_directed(direction, Root(scene)).into_element(),
        window,
        1.0,
    );
    shot.settle(Duration::from_millis(300));
    let x = if scene.rtl { window.0 - 52.0 } else { 52.0 };
    shot.right_click(x, 30.0);
    shot.settle(Duration::from_millis(300));
    let mut rows = vec!["View", "Sort by"];
    if scene.depth > 3 {
        rows.push("Then by");
    }
    for row in rows {
        assert!(shot.click_text(row), "the {row} row");
        shot.settle(Duration::from_millis(300));
    }
    shot
}

/// The smallest box of `width` that holds `label`'s text: the card of the menu that row is in.
fn card(shot: &mut Snapshot, label: &str, width: f32) -> Area {
    let at = shot
        .runner()
        .find_many(|node, element| {
            let shown = Label::try_downcast(element)?;
            (shown.text == label).then(|| node.layout().area)
        })
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("the row {label}"));
    shot.runner()
        .find_many(|node, _| {
            let area = node.layout().area;
            ((area.width() - width).abs() < 0.5
                && area.min_x() <= at.min_x()
                && area.max_x() >= at.max_x()
                && area.min_y() <= at.min_y()
                && area.max_y() >= at.max_y())
            .then_some(area)
        })
        .into_iter()
        .min_by(|a, b| a.height().total_cmp(&b.height()))
        .unwrap_or_else(|| panic!("the card of {label}"))
}

/// The cards of a scene, root first: each is found by a row only it has.
fn cards(shot: &mut Snapshot, scene: Scene) -> Vec<Area> {
    let (wide, narrow) = shot.runner().run_in(|| (Dim::MenuWide.px(), Dim::Menu.px()));
    // Each card is found by the first row only it has, as the scene spells it, in a place no
    // later card covers.
    let mut found = vec![
        card(shot, &text(scene, "New folder"), wide),
        card(shot, &text(scene, "List"), wide),
        card(shot, &text_under(scene, "Name"), narrow),
    ];
    if scene.depth > 3 {
        found.push(card(shot, "Date", narrow));
    }
    found
}

/// The pixels of a frame: `(width, rgb)` rows from the top.
struct Frame {
    width: usize,
    rgb: Vec<[u8; 3]>,
}

impl Frame {
    fn of(shot: &mut Snapshot) -> Frame {
        let png = shot.runner().render();
        let image = SkImage::from_encoded(SkData::new_copy(&png)).expect("the frame decodes");
        let mut surface =
            raster_n32_premul((image.width(), image.height())).expect("a surface to read from");
        surface.canvas().draw_image(&image, (0, 0), None);
        let info = surface.canvas().image_info();
        let row = info.min_row_bytes();
        let mut bytes = vec![0u8; info.compute_byte_size(row)];
        assert!(surface.canvas().read_pixels(&info, &mut bytes, row, (0, 0)));
        let rgb = bytes.chunks_exact(4).map(|px| [px[2], px[1], px[0]]).collect();
        Frame { width: image.width() as usize, rgb }
    }

    fn at(&self, x: f32, y: f32) -> [u8; 3] {
        self.rgb[y as usize * self.width + x as usize]
    }

    /// The pixels of `area` pulled in by `inset` on every side, row by row.
    fn inside(&self, area: Area, inset: f32) -> Vec<[u8; 3]> {
        let mut out = Vec::new();
        for y in (area.min_y() + inset) as usize..(area.max_y() - inset) as usize {
            for x in (area.min_x() + inset) as usize..(area.max_x() - inset) as usize {
                out.push(self.rgb[y * self.width + x]);
            }
        }
        out
    }
}

/// How many pixels of two equal-sized crops differ by more than a level.
fn differing(a: &[[u8; 3]], b: &[[u8; 3]]) -> usize {
    assert_eq!(a.len(), b.len(), "crops of one size");
    a.iter()
        .zip(b)
        .filter(|(a, b)| (0..3).any(|c| a[c].abs_diff(b[c]) > 1))
        .count()
}

/// Past the corner radius and the hairline, into the card's body.
const INSET: f32 = 12.0;

/// Saves the narrow frame as `<BINGUX_MENU_SHOTS>/<name>.png` when the variable is set, for a
/// person to look at.
fn keep(shot: &mut Snapshot, name: &str) {
    if let Some(dir) = std::env::var_os("BINGUX_MENU_SHOTS") {
        shot.save(std::path::Path::new(&dir).join(format!("{name}.png")));
    }
}

/// How far a card's shadow reaches past its edge: nothing of the card under it is read this close.
const SHADOW: f32 = 40.0;

/// Every card has the fill of the root: the colour along its top padding, where no row is, at
/// each point that no later card (or its shadow) is over.
fn assert_one_surface(shot: &mut Snapshot, scene: Scene, window: &str) {
    let frame = Frame::of(shot);
    let cards = cards(shot, scene);
    let strip = |index: usize| -> Vec<[u8; 3]> {
        let card = cards[index];
        let y = card.min_y() + S2.fixed_px() / 2.0;
        let mut x = card.min_x() + 16.0;
        let mut seen = Vec::new();
        while x <= card.max_x() - 16.0 {
            let covered = cards[index + 1..].iter().any(|later| {
                x >= later.min_x() - SHADOW
                    && x <= later.max_x() + SHADOW
                    && y >= later.min_y() - SHADOW
                    && y <= later.max_y() + SHADOW
            });
            if !covered {
                seen.push(frame.at(x, y));
            }
            x += 4.0;
        }
        seen
    };
    let root = strip(0);
    assert!(!root.is_empty(), "{window}: the root's top padding is in the open");
    for level in 1..cards.len() {
        let seen = strip(level);
        assert!(!seen.is_empty(), "{window}: level {} has top padding in the open", level + 1);
        for colour in seen {
            assert_eq!(colour, root[0], "{window}: level {} has the fill of the root menu", level + 1);
        }
    }
}

/// Every level past the first has the fill of the root and does not change when the text under
/// it does. With `reference`, the last also looks as it does in a window wide enough that nothing
/// lies under it (the two windows must be at one density, so the rows are the same height).
fn check(scene: Scene, window: (f32, f32), reference: bool) {
    let mut shown = open(scene, window);
    let shown_cards = cards(&mut shown, scene);
    let shown_frame = Frame::of(&mut shown);
    let name = |what: &str| {
        format!("{}-{what}-{}-{}", if scene.rtl { "rtl" } else { "ltr" }, window.0, scene.depth)
    };
    keep(&mut shown, &name("shown"));
    assert_one_surface(&mut shown, scene, "shown");

    // The last level lies over an earlier one, or the test says nothing.
    let last = shown_cards.len() - 1;
    let over = (0..last).any(|index| {
        let (a, b) = (shown_cards[last], shown_cards[index]);
        a.min_x() < b.max_x() && b.min_x() < a.max_x() && a.min_y() < b.max_y() && b.min_y() < a.max_y()
    });
    assert!(over, "{shown_cards:?}: the last level lies over an earlier one in a {window:?} window");

    // With the text of every menu under the last swapped for other text, the last is the same
    // pixels.
    let swapped = Scene { swapped: true, ..scene };
    let mut other = open(swapped, window);
    let other_cards = cards(&mut other, swapped);
    let other_frame = Frame::of(&mut other);
    assert_eq!(other_cards, shown_cards, "the swap moves nothing");
    assert_eq!(
        differing(
            &shown_frame.inside(shown_cards[last], INSET),
            &other_frame.inside(other_cards[last], INSET)
        ),
        0,
        "the labels of the menus under level {} show through it",
        last + 1
    );

    if !reference {
        return;
    }
    let mut wide = open(scene, WIDE);
    let wide_cards = cards(&mut wide, scene);
    let wide_frame = Frame::of(&mut wide);
    keep(&mut wide, &name("wide"));
    assert_one_surface(&mut wide, scene, "wide");
    // The last level only: the shadow of the one beside it falls on the earlier ones, and not on
    // the same side in the two windows.
    assert_eq!(
        differing(
            &shown_frame.inside(shown_cards[last], INSET),
            &wide_frame.inside(wide_cards[last], INSET)
        ),
        0,
        "level {} shows what is under it, or is not the card it is with room",
        last + 1
    );
}

#[test]
fn the_third_level_is_a_card_of_its_own_over_the_root() {
    check(Scene { depth: 3, rtl: false, swapped: false }, NARROW, false);
}

#[test]
fn the_fourth_level_is_a_card_of_its_own_over_the_ones_before_it() {
    check(Scene { depth: 4, rtl: false, swapped: false }, NARROW, false);
}

#[test]
fn the_third_level_is_a_card_of_its_own_in_a_right_to_left_window() {
    check(Scene { depth: 3, rtl: true, swapped: false }, NARROW, false);
}

#[test]
fn the_fourth_level_is_a_card_of_its_own_in_a_right_to_left_window() {
    check(Scene { depth: 4, rtl: true, swapped: false }, NARROW, false);
}

#[test]
fn every_level_is_the_card_it_is_with_room_at_the_regular_density() {
    check(Scene { depth: 3, rtl: false, swapped: false }, REGULAR, true);
    check(Scene { depth: 4, rtl: false, swapped: false }, REGULAR, true);
    check(Scene { depth: 3, rtl: true, swapped: false }, REGULAR, true);
    check(Scene { depth: 4, rtl: true, swapped: false }, REGULAR, true);
}

#[test]
fn every_level_is_a_card_of_its_own_on_a_phone() {
    check(Scene { depth: 3, rtl: false, swapped: false }, PHONE, false);
    check(Scene { depth: 4, rtl: true, swapped: false }, PHONE, false);
}
