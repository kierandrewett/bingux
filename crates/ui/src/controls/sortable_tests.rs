//! The sortable list, with real pointer, touch and key input on a headless
//! window: the grip and rows are measured on screen, a drag is played one
//! 16 ms frame at a time, and what each frame draws is checked.
//!
//! With `SORTABLE_FRAMES=DIR` the strips are also written as PNG frames, a
//! directory per scene, for the review sheets in `docs/bingux-ui.md`.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use freya::prelude::*;
use freya_core::element::ElementExt;

use super::sortable;
use crate::rows::row;
use crate::shell::navigation::PageScroll;
use crate::snapshot::Snapshot;
use crate::style::*;
use crate::surface::use_tokens;
use crate::theme::provide_tokens;
use crate::window::WindowHost;

const NAMES: [&str; 4] = ["Sound", "Network", "Power", "Keyboard"];
const FRAME: Duration = Duration::from_millis(16);
const DESKTOP: (f32, f32) = (900.0, 560.0);
const PHONE: (f32, f32) = (390.0, 800.0);
const AFTER: &str = "After the list";

/// What the caller saw: every `on_reorder`, and every row pressed.
#[derive(Clone, Default)]
struct Log {
    moves: Rc<RefCell<Vec<(usize, usize)>>>,
    pressed: Rc<RefCell<Vec<String>>>,
}

impl PartialEq for Log {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.moves, &other.moves)
    }
}

#[derive(Clone, PartialEq)]
struct Page {
    log: Log,
    line: bool,
    count: usize,
}

impl Component for Page {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let count = self.count;
        let mut order = use_state(move || {
            (0..count)
                .map(|index| match NAMES.get(index) {
                    Some(name) => name.to_string(),
                    None => format!("Item {index}"),
                })
                .collect::<Vec<_>>()
        });
        let names = order.read().clone();
        let pressed = self.log.pressed.clone();
        let moves = self.log.moves.clone();
        let rows = names
            .iter()
            .map(|name| {
                let pressed = pressed.clone();
                let title = name.clone();
                row(name.clone())
                    .on_press(move |_| pressed.borrow_mut().push(title.clone()))
                    .into()
            })
            .collect();
        let mut list = sortable(rows, move |(from, to): (usize, usize)| {
            moves.borrow_mut().push((from, to));
            let mut list = order.write();
            let item = list.remove(from);
            list.insert(to, item);
        })
        .labels(names.clone());
        if self.line {
            list = list.drop_indicator();
        }
        rect()
            .expanded()
            .tw(&tokens, &[flex_col(), p(S4), gap(S4)])
            .child(list)
            .child(styled_after(&tokens))
    }
}

/// The window root: the tokens, then the window host that moves focus on Tab
/// and sets the density from the window's width.
#[derive(Clone, PartialEq)]
struct Root {
    page: Page,
}

impl Component for Root {
    fn render(&self) -> impl IntoElement {
        let _tokens = provide_tokens();
        WindowHost::new(self.page.clone())
    }
}

fn styled_after(_tokens: &crate::theme::Tokens) -> Element {
    crate::typography::styled_label(crate::typography::BODY, AFTER).into_element()
}

fn app(log: Log, line: bool, count: usize, size: (f32, f32)) -> Snapshot {
    Snapshot::new(
        move || {
            Root {
                page: Page {
                    log: log.clone(),
                    line,
                    count,
                },
            }
            .into_element()
        },
        size,
    )
}

/// The middle of a box.
fn mid(area: &Area) -> (f32, f32) {
    (area.min_x() + area.width() / 2.0, area.min_y() + area.height() / 2.0)
}

/// The text of a label or a paragraph.
fn text_of(element: &dyn ElementExt) -> Option<String> {
    if let Some(label) = Label::try_downcast(element) {
        return Some(label.text.to_string());
    }
    Paragraph::try_downcast(element)
        .map(|paragraph| paragraph.spans.iter().map(|span| span.text.to_string()).collect())
}

/// A label's box.
fn label(shot: &mut Snapshot, text: &str) -> Area {
    shot.runner()
        .find_many(|node, element| {
            (text_of(element)? == text).then(|| node.layout().area)
        })
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("no label {text:?}"))
}

/// The names of the rows, top to bottom as drawn.
fn order(shot: &mut Snapshot) -> Vec<String> {
    let mut found: Vec<(f32, String)> = shot.runner().find_many(|node, element| {
        let text = text_of(element)?;
        (NAMES.contains(&text.as_str()) || text.starts_with("Item "))
            .then(|| (node.layout().area.min_y(), text))
    });
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    found.into_iter().map(|(_, text)| text).collect()
}

/// The accessibility label of every node, with its box and role.
fn nodes_named(shot: &mut Snapshot, name: &str) -> Vec<(Area, accesskit::Role)> {
    shot.runner().find_many(|node, element| {
        let data = element.accessibility();
        (data.builder.label() == Some(name)).then(|| (node.layout().area, data.builder.role()))
    })
}

/// The scale and opacity a box is drawn at, for the node at `at`.
fn drawn(shot: &mut Snapshot, at: (f32, f32)) -> (f32, f32) {
    let found: Vec<(f32, f32)> = shot.runner().find_many(|node, element| {
        let effect = element.effect()?;
        let area = node.layout().area;
        let inside = area.contains((at.0, at.1).into());
        (inside && (effect.scale.is_some() || effect.opacity.is_some()) && effect.opacity != Some(0.0))
            .then(|| (effect.scale.map_or(1.0, |scale| scale.x), effect.opacity.unwrap_or(1.0)))
    });
    found
        .into_iter()
        .fold((1.0, 1.0), |(scale, opacity), (s, o)| (scale * s, opacity * o))
}

struct Grip {
    centre: (f32, f32),
    area: Area,
}

/// The grip of the row named `name`.
fn grip(shot: &mut Snapshot, name: &str) -> Grip {
    let found = nodes_named(shot, &format!("Reorder {name}"));
    let (area, role) = found
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("no grip named Reorder {name}"));
    assert_eq!(role, accesskit::Role::Button, "the grip is a button");
    Grip {
        centre: mid(&area),
        area,
    }
}

fn frames(shot: &mut Snapshot, count: u32) {
    shot.runner().poll_n(FRAME, count);
}

fn start(size: (f32, f32), line: bool, count: usize) -> (Snapshot, Log) {
    let log = Log::default();
    let mut shot = app(log.clone(), line, count, size);
    shot.settle(Duration::from_millis(400));
    (shot, log)
}

fn names(order: &[&str]) -> Vec<String> {
    order.iter().map(|name| name.to_string()).collect()
}

/// With `SORTABLE_FRAMES` set, the directory a scene writes its frames to.
fn frames_dir(scene: &str) -> Option<std::path::PathBuf> {
    std::env::var_os("SORTABLE_FRAMES").map(|dir| std::path::Path::new(&dir).join(scene))
}

#[test]
fn the_grip_is_a_small_icon_in_a_row_button_slot_at_the_end_of_a_standard_row() {
    for (size, class) in [(DESKTOP, SizeClass::Regular), (PHONE, SizeClass::Compact)] {
        let (mut shot, _) = start(size, false, 4);
        let slot = Dim::RowButton.px_in(class);
        let row_height = Dim::Row.px_in(class);
        let inset = ROW_INSET.px_in(class);
        let pad = S4.px_in(class);
        let mut tops = Vec::new();
        for name in NAMES {
            let grip = grip(&mut shot, name);
            let text = label(&mut shot, name);
            assert!(
                (grip.area.width() - slot).abs() < 0.51 && (grip.area.height() - slot).abs() < 0.51,
                "{size:?} {name}: the slot is {}x{}, not {slot}",
                grip.area.width(),
                grip.area.height()
            );
            // Level with the row's text, and the same distance from the row's
            // top and bottom as from its end.
            assert!(
                (grip.centre.1 - (text.min_y() + text.height() / 2.0)).abs() < 1.0,
                "{size:?} {name}: the grip is not on the row's middle"
            );
            let card_right = size.0 - pad - 1.0;
            assert!(
                (card_right - grip.area.max_x() - inset).abs() < 1.01,
                "{size:?} {name}: {} from the card's end, not {inset}",
                card_right - grip.area.max_x()
            );
            tops.push(grip.area.min_y());
            // The glyph is the 16 px icon token, centred in the slot.
            let glyph = class.pick(16.0, 18.0);
            let drawn: Vec<Area> = shot.runner().find_many(|node, _| {
                let area = node.layout().area;
                ((area.width() - glyph).abs() < 0.01
                    && (area.height() - glyph).abs() < 0.01
                    && grip.area.contains(mid(&area).into()))
                .then_some(area)
            });
            assert!(!drawn.is_empty(), "{size:?} {name}: no {glyph} px glyph inside the grip");
            assert!(
                (mid(&drawn[0]).0 - grip.centre.0).abs() < 0.51
                    && (mid(&drawn[0]).1 - grip.centre.1).abs() < 0.51,
                "{size:?} {name}: the glyph is off the slot's middle"
            );
        }
        // Uniform rows, flush: no gaps between them.
        for pair in tops.windows(2) {
            assert!(
                (pair[1] - pair[0] - row_height).abs() < 0.51,
                "{size:?}: rows are {} apart, not {row_height}",
                pair[1] - pair[0]
            );
        }
        // The content of every row starts at the same place, one gutter
        // inside the card.
        let lefts: Vec<f32> = NAMES.iter().map(|name| label(&mut shot, name).min_x()).collect();
        assert!(lefts.windows(2).all(|pair| (pair[0] - pair[1]).abs() < 0.01), "{lefts:?}");
        assert!(
            (lefts[0] - (pad + 1.0 + GUTTER.px_in(class))).abs() < 1.01,
            "{size:?}: the text starts at {}",
            lefts[0]
        );
    }
}

#[test]
fn a_grip_has_a_name_a_role_and_a_place() {
    let (mut shot, _) = start(DESKTOP, false, 4);
    for name in NAMES {
        let found = nodes_named(&mut shot, &format!("Reorder {name}"));
        assert_eq!(found.len(), 1, "one grip names {name}");
        assert_eq!(found[0].1, accesskit::Role::Button);
    }
    let described: Vec<(Option<usize>, Option<usize>)> = shot.runner().find_many(|_, element| {
        let data = element.accessibility();
        (data.builder.label() == Some("Reorder Power"))
            .then(|| (data.builder.position_in_set(), data.builder.size_of_set()))
    });
    assert_eq!(described, vec![(Some(3), Some(4))]);
}

/// Plays a mouse drag of the first row down `distance` pixels, in `steps`
/// moves with a frame after each, and returns what every frame drew:
/// (held row's y, row 1's y, the footer's y, the held row's scale and opacity).
fn pull_down(
    shot: &mut Snapshot,
    distance: f32,
    steps: u32,
) -> (Vec<(f32, f32, f32, (f32, f32))>, (f32, f32)) {
    let grip = grip(shot, "Sound");
    let mut seen = Vec::new();
    shot.press((grip.centre.0, grip.centre.1));
    for step in 1..=steps {
        let t = step as f32 / steps as f32;
        shot.move_to((grip.centre.0, grip.centre.1 + distance * t));
        frames(shot, 1);
        let held = label(shot, "Sound");
        seen.push((
            held.min_y(),
            label(shot, "Network").min_y(),
            label(shot, AFTER).min_y(),
            drawn(shot, mid(&held)),
        ));
    }
    (seen, (grip.centre.0, grip.centre.1 + distance))
}

#[test]
fn a_dragged_row_lifts_follows_the_pointer_and_the_next_row_slides_up() {
    let (mut shot, log) = start(DESKTOP, false, 4);
    let home = label(&mut shot, "Sound").min_y();
    let next = label(&mut shot, "Network").min_y();
    let footer = label(&mut shot, AFTER).min_y();
    let row_height = next - home;
    let dir = frames_dir("desktop-drag");
    if let Some(dir) = &dir {
        shot.record_frames(dir);
    }
    let (seen, end) = pull_down(&mut shot, 80.0, 16);
    frames(&mut shot, 12);
    let settled = (
        label(&mut shot, "Sound").min_y(),
        label(&mut shot, "Network").min_y(),
    );

    // It follows the pointer.
    assert!((seen.last().unwrap().0 - (home + 80.0)).abs() < 1.0, "{:?}", seen.last());
    // It is lifted: bigger and a little translucent, once held.
    let (scale, opacity) = seen.last().unwrap().3;
    assert!(scale > 1.01 && scale <= 1.021, "the held row grows to ~1.02, not {scale}");
    assert!(opacity < 1.0 && opacity > 0.9, "and fades a little, to {opacity}");
    // Nothing outside the list moves, in any frame.
    assert!(seen.iter().all(|frame| (frame.2 - footer).abs() < 0.01), "the footer moved: {seen:?}");
    // The row it passes slides out of the way by one row height, smoothly:
    // never a jump of more than a row, monotone, arriving after a few frames.
    let slid: Vec<f32> = seen.iter().map(|frame| frame.1).collect();
    assert!(slid.windows(2).all(|pair| pair[1] <= pair[0] + 0.01), "not monotone: {slid:?}");
    assert!((slid.last().unwrap() - (next - row_height)).abs() < 1.0, "{slid:?}");
    assert!(
        slid.iter().any(|y| *y < next - 1.0 && *y > next - row_height + 1.0),
        "no part-way frame: {slid:?}"
    );
    assert!(log.moves.borrow().is_empty(), "nothing is reported while it is held");

    // Letting go settles into place with the shadow gone, then reports once.
    shot.release(end);
    for _ in 0..40 {
        frames(&mut shot, 1);
        if !log.moves.borrow().is_empty() {
            break;
        }
    }
    frames(&mut shot, 6);
    assert_eq!(log.moves.borrow().as_slice(), &[(0, 1)]);
    assert_eq!(order(&mut shot), names(&["Network", "Sound", "Power", "Keyboard"]));
    assert!((label(&mut shot, "Network").min_y() - home).abs() < 0.6, "settled {settled:?}");
    assert!((label(&mut shot, "Sound").min_y() - next).abs() < 0.6);
    assert!((label(&mut shot, AFTER).min_y() - footer).abs() < 0.01);
    let at = label(&mut shot, "Sound");
    let (scale, opacity) = drawn(&mut shot, mid(&at));
    assert!((scale - 1.0).abs() < 0.001 && (opacity - 1.0).abs() < 0.001, "the lift is gone");
    assert!(log.pressed.borrow().is_empty(), "the drop pressed no row: {:?}", log.pressed.borrow());
}

#[test]
fn dropping_where_it_started_reports_nothing_and_escape_takes_it_home() {
    let (mut shot, log) = start(DESKTOP, false, 4);
    let home = label(&mut shot, "Sound").min_y();
    let next = label(&mut shot, "Network").min_y();
    let (_, end) = pull_down(&mut shot, 90.0, 6);
    assert!(label(&mut shot, "Network").min_y() < next, "row 1 has started to slide");
    shot.key(Key::Named(NamedKey::Escape));
    frames(&mut shot, 30);
    shot.release(end);
    frames(&mut shot, 20);
    assert!(log.moves.borrow().is_empty(), "Escape reports nothing: {:?}", log.moves.borrow());
    assert_eq!(order(&mut shot), names(&NAMES));
    assert!((label(&mut shot, "Sound").min_y() - home).abs() < 0.6, "it is home again");
    assert!((label(&mut shot, "Network").min_y() - next).abs() < 0.6);
    // And nothing is held: a hover over the list does not move anything.
    let target = grip(&mut shot, "Power").centre;
    shot.move_to((target.0, target.1 - 100.0));
    frames(&mut shot, 4);
    assert!((label(&mut shot, "Sound").min_y() - home).abs() < 0.01);
}

#[test]
fn a_drag_across_two_rows_moves_two_places() {
    let (mut shot, log) = start(DESKTOP, false, 4);
    let (_, end) = pull_down(&mut shot, 130.0, 16);
    shot.release(end);
    frames(&mut shot, 45);
    assert_eq!(log.moves.borrow().as_slice(), &[(0, 2)]);
    assert_eq!(order(&mut shot), names(&["Network", "Power", "Sound", "Keyboard"]));
}

#[test]
fn up_and_down_on_a_focused_grip_slide_the_swap_and_focus_follows_the_item() {
    let (mut shot, log) = start(DESKTOP, false, 4);
    let home = label(&mut shot, "Sound").min_y();
    let next = label(&mut shot, "Network").min_y();
    let dir = frames_dir("desktop-keyboard");
    if let Some(dir) = &dir {
        shot.record_frames(dir);
    }
    // Rows are focusable and so are grips: row 0, grip 0.
    shot.key(Key::Named(NamedKey::Tab));
    shot.key(Key::Named(NamedKey::Tab));
    shot.key(Key::Named(NamedKey::ArrowDown));
    // The swap is drawn in between, not at once.
    let mut path = Vec::new();
    for _ in 0..14 {
        frames(&mut shot, 1);
        path.push((label(&mut shot, "Sound").min_y(), label(&mut shot, "Network").min_y()));
    }
    assert!(
        path.iter().any(|(sound, network)| *sound > home + 1.0 && *sound < next - 1.0 && *network < next - 1.0 && *network > home + 1.0),
        "no frame between the two places: {path:?}"
    );
    assert!(path.windows(2).all(|pair| pair[1].0 >= pair[0].0 - 0.01), "Sound only moves down: {path:?}");
    frames(&mut shot, 10);
    assert_eq!(log.moves.borrow().as_slice(), &[(0, 1)]);
    assert_eq!(order(&mut shot), names(&["Network", "Sound", "Power", "Keyboard"]));
    // The move is announced, politely, to assistive technology.
    let announced: Vec<bool> = shot.runner().find_many(|_, element| {
        let data = element.accessibility();
        (data.builder.label() == Some("Sound moved to position 2 of 4"))
            .then(|| data.builder.live() == Some(accesskit::Live::Polite))
    });
    assert_eq!(announced, vec![true], "a polite live region says where the item went");
    // The focus ring is on the moved item's grip: Down again moves it on.
    shot.key(Key::Named(NamedKey::ArrowDown));
    frames(&mut shot, 25);
    assert_eq!(log.moves.borrow().as_slice(), &[(0, 1), (1, 2)]);
    assert_eq!(order(&mut shot), names(&["Network", "Power", "Sound", "Keyboard"]));
    // Home sends it to the start and End to the end, and the ends stop.
    shot.key(Key::Named(NamedKey::Home));
    frames(&mut shot, 25);
    assert_eq!(order(&mut shot), names(&["Sound", "Network", "Power", "Keyboard"]));
    shot.key(Key::Named(NamedKey::ArrowUp));
    frames(&mut shot, 25);
    assert_eq!(log.moves.borrow().len(), 3, "the first item cannot go up");
    shot.key(Key::Named(NamedKey::End));
    frames(&mut shot, 25);
    assert_eq!(order(&mut shot), names(&["Network", "Power", "Keyboard", "Sound"]));
}

#[test]
fn alt_and_an_arrow_on_a_focused_row_move_it() {
    let (mut shot, log) = start(DESKTOP, false, 4);
    shot.key(Key::Named(NamedKey::Tab));
    shot.key_with(Key::Named(NamedKey::ArrowDown), Modifiers::ALT);
    frames(&mut shot, 25);
    assert_eq!(log.moves.borrow().as_slice(), &[(0, 1)]);
    assert_eq!(order(&mut shot), names(&["Network", "Sound", "Power", "Keyboard"]));
    // The focus is now on the moved item's grip, so the arrow, with or
    // without Alt, goes on moving it.
    shot.key_with(Key::Named(NamedKey::ArrowDown), Modifiers::ALT);
    frames(&mut shot, 25);
    assert_eq!(log.moves.borrow().as_slice(), &[(0, 1), (1, 2)]);
    assert_eq!(order(&mut shot), names(&["Network", "Power", "Sound", "Keyboard"]));
}

#[test]
fn a_finger_on_the_grip_lifts_at_once_and_a_held_row_lifts_after_a_beat() {
    let (mut shot, log) = start(PHONE, false, 4);
    let home = label(&mut shot, "Sound").min_y();
    let row_height = label(&mut shot, "Network").min_y() - home;
    let dir = frames_dir("phone-drag");
    if let Some(dir) = &dir {
        shot.record_frames(dir);
    }
    // The grip: a touch picks the row up with no wait.
    let grip_at = grip(&mut shot, "Sound").centre;
    shot.run_script(&format!("touch_down {},{}", grip_at.0, grip_at.1), "settings")
        .unwrap();
    for step in 1..=12 {
        let y = grip_at.1 + 1.0 + (row_height * 1.4) * step as f32 / 12.0;
        shot.run_script(&format!("touch_move {},{y}", grip_at.0), "settings").unwrap();
        frames(&mut shot, 1);
    }
    let held = label(&mut shot, "Sound");
    assert!(held.min_y() > home + row_height, "the row follows the finger: {}", held.min_y());
    shot.run_script(
        &format!("touch_up {},{}", grip_at.0, grip_at.1 + row_height * 1.4),
        "settings",
    )
    .unwrap();
    frames(&mut shot, 40);
    assert_eq!(log.moves.borrow().as_slice(), &[(0, 1)]);
    assert!(log.pressed.borrow().is_empty(), "lifting the finger pressed no row");

    // The row: a finger resting for half a second picks it up.
    let row_at = (label(&mut shot, "Power").min_x() + 20.0, mid(&label(&mut shot, "Power")).1);
    shot.run_script(&format!("touch_down {},{}", row_at.0, row_at.1), "settings").unwrap();
    frames(&mut shot, 6);
    let before = drawn(&mut shot, row_at);
    assert!(before.0 <= 1.0, "not lifted yet");
    shot.settle(Duration::from_millis(560));
    let after = drawn(&mut shot, row_at);
    assert!(after.0 > 1.01, "held for a beat, the row lifts: {after:?}");
    shot.run_script(&format!("touch_move {},{}", row_at.0, row_at.1 - row_height * 1.8), "settings")
        .unwrap();
    frames(&mut shot, 3);
    shot.run_script(&format!("touch_up {},{}", row_at.0, row_at.1 - row_height * 1.8), "settings")
        .unwrap();
    frames(&mut shot, 40);
    assert_eq!(log.moves.borrow().as_slice(), &[(0, 1), (2, 0)]);
    assert!(log.pressed.borrow().is_empty(), "the finger that moved a row did not press it");
}

#[test]
fn a_tap_on_a_row_presses_it_and_does_not_start_a_drag() {
    let (mut shot, log) = start(PHONE, false, 4);
    let at = mid(&label(&mut shot, "Network"));
    shot.touch(at.0, at.1);
    frames(&mut shot, 10);
    assert_eq!(log.pressed.borrow().as_slice(), &["Network".to_string()]);
    assert!(log.moves.borrow().is_empty());
}

#[test]
fn a_long_list_shows_a_line_and_leaves_the_other_rows_still() {
    let (mut shot, log) = start(DESKTOP, true, 4);
    let home = label(&mut shot, "Sound").min_y();
    let next = label(&mut shot, "Network").min_y();
    let third = label(&mut shot, "Power").min_y();
    let row_height = next - home;
    let dir = frames_dir("desktop-line");
    if let Some(dir) = &dir {
        shot.record_frames(dir);
    }
    let (seen, end) = pull_down(&mut shot, 2.2 * row_height, 16);
    frames(&mut shot, 10);
    assert!(
        (label(&mut shot, "Network").min_y() - next).abs() < 0.01
            && (label(&mut shot, "Power").min_y() - third).abs() < 0.01,
        "the others stay: {seen:?}"
    );
    // An accent line, 2 px high, inset by the gutter, sits where it would land.
    let class = SizeClass::Regular;
    let lines: Vec<Area> = shot.runner().find_many(|node, _| {
        let area = node.layout().area;
        ((area.height() - Dim::Ring.px_in(class)).abs() < 0.01
            && area.width() > 100.0
            && area.min_y() > home)
            .then_some(area)
    });
    assert!(!lines.is_empty(), "no drop line");
    let line = lines[0];
    let expected = label(&mut shot, "Power").min_y() + row_height - (label(&mut shot, "Power").height() - row_height).max(0.0);
    assert!((mid(&line).1 - expected).abs() <= 6.0 || mid(&line).1 > third, "line at {}", mid(&line).1);
    shot.release(end);
    frames(&mut shot, 10);
    assert_eq!(log.moves.borrow().as_slice(), &[(0, 2)]);
    assert_eq!(order(&mut shot), names(&["Network", "Power", "Sound", "Keyboard"]));
}

/// A page that scrolls: the list sits `ABOVE` px down, with room below it.
const ABOVE: f32 = 300.0;
const BELOW: f32 = 900.0;
const SCROLLING: (f32, f32) = (900.0, 500.0);

#[derive(Clone, PartialEq)]
struct Scrolling {
    log: Log,
}

impl Component for Scrolling {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let scroll = use_scroll_controller(ScrollConfig::default);
        use_provide_context(move || PageScroll {
            scroll,
            compact: false,
        });
        let mut order = use_state(|| NAMES.iter().map(|name| name.to_string()).collect::<Vec<_>>());
        let names = order.read().clone();
        let moves = self.log.moves.clone();
        let rows = names.iter().map(|name| row(name.clone()).into()).collect();
        let list = sortable(rows, move |(from, to): (usize, usize)| {
            moves.borrow_mut().push((from, to));
            let mut list = order.write();
            let item = list.remove(from);
            list.insert(to, item);
        })
        .labels(names);
        ScrollView::new_controlled(scroll).child(
            rect()
                .tw(&tokens, &[w_full(), flex_col(), px(S4)])
                .child(rect().height(Size::px(ABOVE)))
                .child(list)
                .child(rect().height(Size::px(BELOW))),
        )
    }
}

#[test]
fn a_row_held_at_the_bottom_edge_scrolls_the_page_and_stays_under_the_pointer() {
    let log = Log::default();
    let page = log.clone();
    let mut shot = Snapshot::new(
        move || {
            struct Host(Log);
            impl PartialEq for Host {
                fn eq(&self, other: &Self) -> bool {
                    self.0 == other.0
                }
            }
            impl Component for Host {
                fn render(&self) -> impl IntoElement {
                    let _tokens = provide_tokens();
                    WindowHost::new(Scrolling { log: self.0.clone() })
                }
            }
            Host(page.clone()).into_element()
        },
        SCROLLING,
    );
    shot.settle(Duration::from_millis(400));
    let handle = grip(&mut shot, "Sound");
    let last_before = label(&mut shot, "Keyboard").min_y();
    shot.press(handle.centre);
    frames(&mut shot, 4);
    // Into the bottom band of the window and hold there.
    let edge = SCROLLING.1 - 12.0;
    let mut at = handle.centre.1;
    while at < edge {
        at = (at + 20.0).min(edge);
        shot.move_to((handle.centre.0, at));
        frames(&mut shot, 1);
    }
    frames(&mut shot, 30);
    let scrolled = last_before - label(&mut shot, "Keyboard").min_y();
    assert!(scrolled > 60.0, "the page scrolled {scrolled} px while the row was held at the edge");
    // The held row is still under the pointer, not left behind with the page.
    let held = label(&mut shot, "Sound");
    assert!(
        (mid(&held).1 - edge).abs() < 60.0,
        "the held row is at {} and the pointer at {edge}",
        mid(&held).1
    );
    // It stops when the pointer leaves the band, and Escape takes the row home.
    shot.move_to((handle.centre.0, SCROLLING.1 / 2.0));
    frames(&mut shot, 5);
    let still = label(&mut shot, "Keyboard").min_y();
    frames(&mut shot, 10);
    assert!((label(&mut shot, "Keyboard").min_y() - still).abs() < 0.5, "the page kept scrolling");
    shot.key(Key::Named(NamedKey::Escape));
    frames(&mut shot, 30);
    assert!(log.moves.borrow().is_empty());
    assert_eq!(order(&mut shot), names(&NAMES));
}

#[test]
fn a_single_item_has_a_grip_that_does_nothing() {
    let (mut shot, log) = start(DESKTOP, false, 1);
    let at = grip(&mut shot, "Sound").centre;
    shot.press(at);
    shot.move_to((at.0, at.1 + 60.0));
    frames(&mut shot, 4);
    shot.release((at.0, at.1 + 60.0));
    frames(&mut shot, 6);
    assert!(log.moves.borrow().is_empty());
}

/// Writes the review strips (one PNG per step) when `SORTABLE_FRAMES` names a
/// directory; otherwise it does nothing. `scripts` below are the same inputs
/// the tests above check, played at the desktop and phone sizes.
#[test]
fn review_strips() {
    let Some(root) = std::env::var_os("SORTABLE_FRAMES") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    for (name, size) in [("desktop", DESKTOP), ("phone", PHONE)] {
        let save = |shot: &mut Snapshot, scene: &str, step: &str| {
            shot.save(root.join(format!("{name}-{scene}-{step}.png")));
        };

        // Pick up, move over two rows, drop.
        let (mut shot, _) = start(size, false, 4);
        let handle = grip(&mut shot, "Sound");
        let row_height = label(&mut shot, "Network").min_y() - label(&mut shot, "Sound").min_y();
        save(&mut shot, "drag", "1-rest");
        shot.press((handle.centre.0, handle.centre.1));
        frames(&mut shot, 8);
        save(&mut shot, "drag", "2-picked-up");
        let mut at = handle.centre.1;
        for (step, travel) in [("3-first-row", 0.55), ("4-second-row", 1.6)] {
            let goal = handle.centre.1 + row_height * travel;
            while at < goal {
                at = (at + 6.0).min(goal);
                shot.move_to((handle.centre.0, at));
                frames(&mut shot, 1);
            }
            frames(&mut shot, 8);
            save(&mut shot, "drag", step);
        }
        shot.release((handle.centre.0, at));
        frames(&mut shot, 3);
        save(&mut shot, "drag", "5-dropped");
        frames(&mut shot, 25);
        save(&mut shot, "drag", "6-settled");

        // Keyboard.
        let (mut shot, _) = start(size, false, 4);
        save(&mut shot, "keys", "1-rest");
        shot.key(Key::Named(NamedKey::Tab));
        shot.key(Key::Named(NamedKey::Tab));
        save(&mut shot, "keys", "2-grip-focused");
        shot.key(Key::Named(NamedKey::ArrowDown));
        frames(&mut shot, 3);
        save(&mut shot, "keys", "3-sliding");
        frames(&mut shot, 20);
        save(&mut shot, "keys", "4-moved");
        shot.key(Key::Named(NamedKey::ArrowDown));
        frames(&mut shot, 4);
        save(&mut shot, "keys", "5-sliding-again");
        frames(&mut shot, 20);
        save(&mut shot, "keys", "6-moved-again");

        // Drop line.
        let (mut shot, _) = start(size, true, 4);
        let handle = grip(&mut shot, "Sound");
        shot.press((handle.centre.0, handle.centre.1));
        let mut at = handle.centre.1;
        let goal = handle.centre.1 + row_height * 1.6;
        while at < goal {
            at += 6.0;
            shot.move_to((handle.centre.0, at));
            frames(&mut shot, 1);
        }
        frames(&mut shot, 8);
        save(&mut shot, "line", "1-over-two-rows");
        shot.release((handle.centre.0, at));
        frames(&mut shot, 25);
        save(&mut shot, "line", "2-dropped");
    }
}

/// Every 16 ms frame of a keyboard move and of a drop, written to
/// `SORTABLE_FRAMES/<size>-film-keys` and `-film-drop` when the variable is set.
#[test]
fn review_filmstrips() {
    let Some(root) = std::env::var_os("SORTABLE_FRAMES") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    for (name, size) in [("desktop", DESKTOP), ("phone", PHONE)] {
        let (mut shot, _) = start(size, false, 4);
        shot.key(Key::Named(NamedKey::Tab));
        shot.key(Key::Named(NamedKey::Tab));
        shot.settle(Duration::from_millis(250));
        shot.record_frames(root.join(format!("{name}-film-keys")));
        shot.key(Key::Named(NamedKey::ArrowDown));
        shot.settle(Duration::from_millis(400));

        let (mut shot, _) = start(size, false, 4);
        let handle = grip(&mut shot, "Sound");
        let row_height = label(&mut shot, "Network").min_y() - label(&mut shot, "Sound").min_y();
        shot.press((handle.centre.0, handle.centre.1));
        let end = (handle.centre.0, handle.centre.1 + row_height * 1.4);
        shot.move_to(end);
        shot.settle(Duration::from_millis(300));
        shot.record_frames(root.join(format!("{name}-film-drop")));
        shot.release(end);
        shot.settle(Duration::from_millis(400));
    }
}
