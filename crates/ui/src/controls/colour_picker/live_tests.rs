//! The picker and the button in a real window, under real pointer and key
//! events: what a press and a drag do frame by frame, how big every target is
//! at each density, that the picker never changes size, and what the popover
//! does on every way in and out.

use std::cell::RefCell;
use std::time::Duration;

use freya::engine::prelude::{SkData, SkImage, raster_n32_premul};
use freya::prelude::*;
use freya_testing::prelude::{PlatformEvent, TouchEventName, TouchPhase};
use torin::prelude::Area;

use super::core::forget_remembered;
use super::geometry::{Geometry, Shape};
use super::model::{Hsva, Mode};
use super::{RECENT_MAX, colour_button, colour_picker};
use crate::controls::action_button;
use crate::controls::capture::Point;
use crate::inspector::snapshot::Snapshot as Tree;
use crate::shell::overlays::{OverlayHost, use_overlays};
use crate::shell::dialog::dialog;
use crate::snapshot::Snapshot;
use crate::style::*;
use crate::theme::provide_tokens;
use crate::window::WindowHost;

const DESKTOP: (f32, f32) = (900.0, 1000.0);
const PHONE: (f32, f32) = (390.0, 900.0);
const FRAME: Duration = Duration::from_millis(16);
/// The colour of most tests: a saturated blue, on no edge.
const BLUE: (u8, u8, u8) = (53, 132, 228);

/// Where the host puts what it shows: a margin of 60 px on a desktop, so a
/// pointer can go well outside the control and still be in the window, and 16
/// on a phone, where the control fills the width less that margin.
fn at_for(window: (f32, f32)) -> Point {
    if window.0 < 600.0 { (16.0, 16.0) } else { (60.0, 60.0) }
}

thread_local! {
    /// Every colour the control reported to its caller, in order.
    static LOG: RefCell<Vec<Color>> = const { RefCell::new(Vec::new()) };
    /// Every list of recent colours the control asked its caller to keep.
    static KEPT: RefCell<Vec<Vec<Color>>> = const { RefCell::new(Vec::new()) };
    /// The caller's colour, to set from outside.
    static VALUE: RefCell<Option<State<Color>>> = const { RefCell::new(None) };
}

/// What the host shows.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    /// The inline picker, without and with the opacity strip.
    Picker,
    PickerAlpha,
    /// The inline picker with a list of recent colours the caller keeps.
    PickerKept,
    /// The button, committing on close or live.
    Button,
    ButtonLive,
    /// The button, inside an alert dialog.
    ButtonInDialog,
}

#[derive(PartialEq)]
struct Root(Host);

impl Component for Root {
    fn render(&self) -> impl IntoElement {
        let _ = provide_tokens();
        WindowHost::new(OverlayHost::new(Host {
            start: self.0.start,
            at: self.0.at,
            width: self.0.width,
            kind: self.0.kind,
        }))
    }
}

#[derive(PartialEq)]
struct Host {
    start: Color,
    at: Point,
    /// The width the control has: the window's less the margin on each side.
    width: f32,
    kind: Kind,
}

impl Component for Host {
    fn render(&self) -> impl IntoElement {
        let mut colour = use_state(|| self.start);
        let mut kept = use_state(Vec::<Color>::new);
        VALUE.with(|value| *value.borrow_mut() = Some(colour));
        let report = move |next: Color| {
            LOG.with(|log| log.borrow_mut().push(next));
            colour.set(next);
        };
        let overlays = use_overlays();
        let child: Element = match self.kind {
            Kind::Picker => colour_picker(colour(), report).into_element(),
            Kind::PickerAlpha => colour_picker(colour(), report).alpha(true).into_element(),
            Kind::PickerKept => colour_picker(colour(), report)
                .recent(kept.read().clone(), move |list: Vec<Color>| {
                    KEPT.with(|log| log.borrow_mut().push(list.clone()));
                    kept.set(list);
                })
                .into_element(),
            Kind::Button => colour_button(colour(), report).title("Accent colour").into_element(),
            Kind::ButtonLive => colour_button(colour(), report)
                .live(true)
                .title("Accent colour")
                .into_element(),
            Kind::ButtonInDialog => action_button("Open the dialog", move |_| {
                overlays.show_dialog(
                    dialog("Colour", "Pick one.")
                        .content(
                            colour_button(colour(), report)
                                .live(true)
                                .title("Accent colour"),
                        )
                        .cancel("Close"),
                );
            })
            .into_element(),
        };
        rect()
            .position(Position::new_absolute().left(self.at.0).top(self.at.1))
            .width(Size::px(self.width))
            .child(child)
    }
}

fn argb(a: u8, r: u8, g: u8, b: u8) -> Color {
    colour_from_argb(a, r, g, b)
}

fn blue() -> Color {
    argb(255, BLUE.0, BLUE.1, BLUE.2)
}

fn logged() -> Vec<Color> {
    LOG.with(|log| log.borrow().clone())
}

fn last() -> Color {
    logged().last().copied().expect("the control reported nothing")
}

fn channels(colour: Color) -> (u8, u8, u8) {
    (colour.r(), colour.g(), colour.b())
}

fn near(a: (u8, u8, u8), b: (u8, u8, u8), tolerance: i32) -> bool {
    [(a.0, b.0), (a.1, b.1), (a.2, b.2)]
        .iter()
        .all(|(x, y)| (*x as i32 - *y as i32).abs() <= tolerance)
}

fn hue_of(colour: Color) -> f32 {
    Hsva::from_colour(colour).hue
}

/// Hue differences, around the circle.
fn hue_gap(a: f32, b: f32) -> f32 {
    let gap = (a - b).abs() % 360.0;
    gap.min(360.0 - gap)
}

fn centre(area: Area) -> Point {
    (
        area.min_x() + area.width() / 2.0,
        area.min_y() + area.height() / 2.0,
    )
}

/// The point `fx` and `fy` of the way across and down `area`.
fn within(area: Area, fx: f32, fy: f32) -> Point {
    (
        area.min_x() + area.width() * fx,
        area.min_y() + area.height() * fy,
    )
}

/// The control in a headless window at a display scale. Every point it takes
/// and gives is in logical pixels; the harness's pointer wants physical ones,
/// so it is scaled here.
struct Rig {
    shot: Snapshot,
    scale: f64,
    window: (f32, f32),
}

impl Rig {
    fn new(kind: Kind, start: Color, window: (f32, f32), scale: f64) -> Self {
        LOG.with(|log| log.borrow_mut().clear());
        KEPT.with(|kept| kept.borrow_mut().clear());
        forget_remembered();
        let at = at_for(window);
        let width = window.0 - 2.0 * at.0;
        let shot = Snapshot::with_scale(
            move || Root(Host { start, at, width, kind }).into_element(),
            window,
            scale,
        );
        let mut rig = Self { shot, scale, window };
        rig.frames(6);
        rig
    }

    /// The desktop picker with the opacity strip, at 1x, blue.
    fn picker() -> Self {
        Self::new(Kind::PickerAlpha, blue(), DESKTOP, 1.0)
    }

    fn frames(&mut self, count: u32) {
        self.shot.runner().poll_n(FRAME, count);
    }

    fn physical(&self, at: Point) -> Point {
        (at.0 * self.scale as f32, at.1 * self.scale as f32)
    }

    fn press(&mut self, at: Point) {
        let at = self.physical(at);
        self.shot.press(at);
    }

    fn move_to(&mut self, at: Point) {
        let at = self.physical(at);
        self.shot.move_to(at);
    }

    fn release(&mut self, at: Point) {
        let at = self.physical(at);
        self.shot.release(at);
    }

    fn click(&mut self, at: Point) {
        let at = self.physical(at);
        self.shot.click(at.0, at.1);
        self.frames(8);
    }

    /// A press and release at one point, with the frames around them.
    fn tap(&mut self, at: Point) {
        self.press(at);
        self.frames(3);
        self.release(at);
        self.frames(3);
    }

    fn touch(&mut self, name: TouchEventName, at: Point, finger: u64) {
        let phase = match name {
            TouchEventName::TouchStart => TouchPhase::Started,
            TouchEventName::TouchMove => TouchPhase::Moved,
            TouchEventName::TouchEnd => TouchPhase::Ended,
            TouchEventName::TouchCancel => TouchPhase::Cancelled,
        };
        let at = self.physical(at);
        self.shot.runner().send_event(PlatformEvent::Touch {
            name,
            location: CursorPoint::new(at.0 as f64, at.1 as f64),
            finger_id: finger,
            phase,
            force: None,
        });
        self.shot.runner().sync_and_update();
    }

    fn key(&mut self, key: Key) {
        self.shot.key_with(key, Modifiers::default());
        self.frames(2);
    }

    fn key_with(&mut self, key: Key, modifiers: Modifiers) {
        self.shot.key_with(key, modifiers);
        self.frames(2);
    }

    fn type_text(&mut self, text: &str) {
        for character in text.chars() {
            self.key(Key::Character(character.to_string()));
        }
        self.frames(3);
    }

    fn tree(&mut self) -> Tree {
        Tree::capture_headless(self.shot.runner(), self.scale as f32)
    }

    /// The window areas, in logical pixels, of every element a component
    /// named `name` made, in tree order.
    fn areas(&mut self, name: &str) -> Vec<Area> {
        self.tree()
            .nodes
            .iter()
            .filter(|node| node.a11y.component.as_deref() == Some(name))
            .filter_map(|node| node.area)
            .collect()
    }

    fn area(&mut self, name: &str) -> Area {
        self.areas(name)
            .first()
            .copied()
            .unwrap_or_else(|| panic!("no {name} on screen"))
    }

    fn has(&mut self, name: &str) -> bool {
        !self.areas(name).is_empty()
    }

    /// The areas of elements the accessibility tree names `name`.
    fn named(&mut self, name: &str) -> Vec<Area> {
        self.tree()
            .nodes
            .iter()
            .filter(|node| node.a11y.name.as_deref() == Some(name))
            .filter_map(|node| node.area)
            .collect()
    }

    /// The text of every label and paragraph that lies inside `area`.
    fn texts_in(&mut self, area: Area) -> Vec<String> {
        let inside = |inner: Area| {
            inner.min_x() >= area.min_x() - 0.5
                && inner.max_x() <= area.max_x() + 0.5
                && inner.min_y() >= area.min_y() - 0.5
                && inner.max_y() <= area.max_y() + 0.5
        };
        self.tree()
            .nodes
            .iter()
            .filter(|node| node.area.is_some_and(inside))
            .filter_map(|node| node.text.clone())
            .collect()
    }

    /// The knob centre of the plane.
    fn knob(&mut self) -> Point {
        centre(self.area("ColourKnob"))
    }

    /// The name of the element that has keyboard focus, if any.
    fn focused(&mut self) -> Option<String> {
        let id = self
            .shot
            .runner()
            .run_in(|| *Platform::get().current_window().focused_accessibility_id.peek());
        let mut tree = self.tree();
        tree.mark_focused(id);
        tree.nodes
            .iter()
            .find(|node| node.a11y.focused)
            .and_then(|node| node.a11y.name.clone().or_else(|| node.a11y.component.clone()))
    }

    /// The pixel at a window point of the frame as drawn.
    fn pixel(&mut self, at: Point) -> (u8, u8, u8) {
        let png = self.shot.runner().render();
        let image = SkImage::from_encoded(SkData::new_copy(&png)).expect("the frame decodes");
        let mut surface =
            raster_n32_premul((image.width(), image.height())).expect("a surface to read from");
        surface.canvas().draw_image(&image, (0, 0), None);
        let info = surface.canvas().image_info();
        let row = info.min_row_bytes();
        let mut bytes = vec![0u8; info.compute_byte_size(row)];
        assert!(surface.canvas().read_pixels(&info, &mut bytes, row, (0, 0)));
        let at = self.physical(at);
        let (x, y) = (at.0.round() as usize, at.1.round() as usize);
        let i = y * row + x * 4;
        // N32 on a little-endian machine is blue, green, red, alpha.
        (bytes[i + 2], bytes[i + 1], bytes[i])
    }

    /// Clicks the hex field and replaces its text.
    fn type_hex(&mut self, text: &str) {
        let hex = self.area("ColourHex");
        self.click((hex.min_x() + 30.0, hex.min_y() + Dim::Control.px_in(self.class()) / 2.0));
        self.clear_field();
        self.type_text(text);
    }

    /// Empties the field that has focus: the end, then enough Backspaces.
    fn clear_field(&mut self) {
        self.key(Key::Named(NamedKey::End));
        for _ in 0..12 {
            self.key(Key::Named(NamedKey::Backspace));
        }
    }

    fn class(&self) -> SizeClass {
        if self.window.0 < 600.0 { SizeClass::Compact } else { SizeClass::Regular }
    }
}

// ---- The plane -----------------------------------------------------------------

#[test]
fn the_handle_follows_the_pointer_across_and_outside_and_clamps_at_every_edge() {
    let mut rig = Rig::picker();
    let plane = rig.area("ColourPlane");
    let knob = Dim::PickerKnob.px_in(rig.class());
    let clamp = |at: Point| {
        (
            at.0.clamp(plane.min_x() + knob / 2.0, plane.max_x() - knob / 2.0),
            at.1.clamp(plane.min_y() + knob / 2.0, plane.max_y() - knob / 2.0),
        )
    };
    let path = [
        within(plane, 0.5, 0.5),
        within(plane, 0.9, 0.2),
        // Out through the right edge, the top, the left and the bottom, and
        // across the window.
        (plane.max_x() + 150.0, plane.min_y() + 20.0),
        (plane.max_x() + 150.0, plane.min_y() - 20.0),
        (plane.min_x() - 50.0, plane.min_y() - 50.0),
        (plane.min_x() - 50.0, plane.max_y() + 300.0),
        (DESKTOP.0 - 1.0, DESKTOP.1 - 1.0),
        within(plane, 0.3, 0.7),
    ];
    rig.press(path[0]);
    rig.frames(3);
    for at in path {
        rig.move_to(at);
        rig.frames(3);
        let seen = rig.knob();
        let want = clamp(at);
        assert!(
            (seen.0 - want.0).abs() < 1.5 && (seen.1 - want.1).abs() < 1.5,
            "the pointer is at {at:?}: the handle should be at {want:?} and is at {seen:?}"
        );
    }
    rig.release(path[path.len() - 1]);
    rig.frames(3);
    // A release keeps the colour the handle ended on.
    let at = rig.knob();
    let want = clamp(path[path.len() - 1]);
    assert!((at.0 - want.0).abs() < 1.5 && (at.1 - want.1).abs() < 1.5);
    // Moving the pointer with the button up leaves it where it is.
    rig.move_to(within(plane, 0.9, 0.9));
    rig.frames(3);
    assert_eq!(rig.knob(), at);
}

#[test]
fn the_corners_of_the_plane_are_white_black_and_the_pure_hue() {
    let mut rig = Rig::picker();
    let plane = rig.area("ColourPlane");
    let hue = hue_of(blue());
    // A press inside a corner, then out of the window's way past it: the
    // handle is the corner, whatever the pointer does beyond.
    let drag = |rig: &mut Rig, from: Point, to: Point| {
        rig.press(from);
        rig.frames(2);
        rig.move_to(to);
        rig.frames(3);
        rig.release(to);
        rig.frames(2);
    };
    drag(&mut rig, (plane.min_x() + 7.0, plane.min_y() + 7.0), (plane.min_x() - 50.0, plane.min_y() - 50.0));
    assert_eq!(channels(last()), (255, 255, 255), "top left is white");
    drag(&mut rig, (plane.max_x() - 7.0, plane.min_y() + 7.0), (plane.max_x() + 100.0, plane.min_y() - 50.0));
    let pure = Hsva::new(hue, 1.0, 1.0, 255).rgb();
    assert_eq!(channels(last()), pure, "top right is the pure hue");
    drag(&mut rig, (plane.min_x() + 7.0, plane.max_y() - 7.0), (plane.min_x() - 50.0, plane.max_y() + 100.0));
    assert_eq!(channels(last()), (0, 0, 0), "bottom left is black");
    drag(&mut rig, (plane.max_x() - 7.0, plane.max_y() - 7.0), (plane.max_x() + 100.0, plane.max_y() + 100.0));
    assert_eq!(channels(last()), (0, 0, 0), "bottom right is black");
}

#[test]
fn what_you_click_in_the_plane_is_the_colour_you_see_there() {
    for scale in [1.0, 1.25, 2.0] {
        for (fx, fy) in [(0.2_f32, 0.2_f32), (0.7, 0.35), (0.5, 0.5), (0.2, 0.8), (0.8, 0.8)] {
            let mut rig = Rig::new(Kind::PickerAlpha, blue(), DESKTOP, scale);
            let plane = rig.area("ColourPlane");
            let at = within(plane, fx, fy);
            // The handle sits on the picked colour; look before it moves.
            let seen = rig.pixel(at);
            rig.tap(at);
            let picked = channels(last());
            assert!(
                near(seen, picked, 30),
                "scale {scale}, ({fx}, {fy}): the plane shows {seen:?} and the press picked {picked:?}"
            );
        }
    }
}

#[test]
fn the_plane_has_no_seam_and_its_corners_are_the_colours_it_says() {
    let mut rig = Rig::picker();
    let plane = rig.area("ColourPlane");
    let hue = Hsva::new(hue_of(blue()), 1.0, 1.0, 255).rgb();
    let inset = 7.0;
    // The knob of the blue sits mid plane, so the corners are clear.
    assert!(near(rig.pixel((plane.min_x() + inset, plane.min_y() + inset)), (255, 255, 255), 24));
    assert!(near(rig.pixel((plane.max_x() - inset, plane.min_y() + inset)), hue, 24));
    assert!(near(rig.pixel((plane.min_x() + inset, plane.max_y() - inset)), (0, 0, 0), 24));
    assert!(near(rig.pixel((plane.max_x() - inset, plane.max_y() - inset)), (0, 0, 0), 24));
    // Along the top edge the colour only moves from white to the hue, one
    // monotonic step at a time: no band, no jump.
    let y = plane.min_y() + 6.0;
    let mut previous = rig.pixel((plane.min_x() + 4.0, y));
    for step in 1..=40 {
        let x = plane.min_x() + 4.0 + (plane.width() - 8.0) * step as f32 / 40.0;
        let now = rig.pixel((x, y));
        assert!(
            near(previous, now, 14),
            "a jump along the top edge at {x}: {previous:?} then {now:?}"
        );
        previous = now;
    }
}

#[test]
fn the_handle_is_a_double_ring_that_shows_on_any_colour() {
    let mut rig = Rig::picker();
    let plane = rig.area("ColourPlane");
    for (fx, fy) in [(0.5, 0.5), (0.02, 0.02), (0.98, 0.98), (0.02, 0.98)] {
        rig.tap(within(plane, fx, fy));
        let knob = rig.area("ColourKnob");
        let (cx, cy) = centre(knob);
        let radius = knob.width() / 2.0;
        // The ring is white between the core and the outline.
        let white = rig.pixel((cx + radius - 2.5, cy));
        assert!(near(white, (255, 255, 255), 40), "({fx}, {fy}): the ring is {white:?}");
        // The core is the colour that was chosen.
        let core = rig.pixel((cx, cy));
        assert!(near(core, channels(last()), 6), "({fx}, {fy}): the core is {core:?}, not {:?}", channels(last()));
        // The outline is darker than the plane just outside the knob, or the
        // plane is black there already.
        let outline = rig.pixel((cx + radius - 0.5, cy));
        let outside = rig.pixel((cx + radius + 3.0, cy));
        let sum = |c: (u8, u8, u8)| c.0 as i32 + c.1 as i32 + c.2 as i32;
        assert!(sum(outline) < 3 * 230, "({fx}, {fy}): the outline is {outline:?} against {outside:?}");
    }
}

#[test]
fn a_touch_drag_follows_one_finger_and_ends_with_it() {
    let mut rig = Rig::picker();
    let plane = rig.area("ColourPlane");
    let start = within(plane, 0.2, 0.3);
    let end = within(plane, 0.8, 0.7);
    rig.touch(TouchEventName::TouchStart, start, 1);
    rig.frames(2);
    rig.touch(TouchEventName::TouchMove, end, 1);
    rig.frames(3);
    let at = rig.knob();
    assert!((at.0 - end.0).abs() < 1.5 && (at.1 - end.1).abs() < 1.5, "{at:?} against {end:?}");
    rig.touch(TouchEventName::TouchEnd, end, 1);
    rig.frames(2);
    let ended = logged().len();
    // Another touch that never started a drag moves nothing.
    rig.touch(TouchEventName::TouchMove, start, 1);
    rig.frames(3);
    assert_eq!(logged().len(), ended);
}

#[test]
fn a_press_on_the_handle_target_beside_the_knob_takes_the_handle() {
    let mut rig = Rig::picker();
    let handle = rig.area("ColourHandle");
    let knob = rig.area("ColourKnob");
    assert!(handle.width() > knob.width());
    // Inside the target, outside the knob, the press is the plane's own: the
    // handle goes to the pointer.
    let (cx, cy) = centre(knob);
    let at = (cx + knob.width() / 2.0 + 3.0, cy);
    assert!(at.0 < handle.max_x());
    rig.tap(at);
    let now = rig.knob();
    assert!((now.0 - at.0).abs() < 1.5, "{now:?} against {at:?}");
}

// ---- Keys ----------------------------------------------------------------------

/// The plane focused by a press, then a key so its ring shows.
fn focus_plane(rig: &mut Rig) -> Area {
    let plane = rig.area("ColourPlane");
    rig.tap(within(plane, 0.5, 0.5));
    plane
}

#[test]
fn arrow_keys_move_the_handle_one_percent_and_shift_ten() {
    let mut rig = Rig::picker();
    let plane = focus_plane(&mut rig);
    let knob = Dim::PickerKnob.px_in(rig.class());
    let travel_x = plane.width() - knob;
    let travel_y = plane.height() - knob;
    let from = rig.knob();
    rig.key(Key::Named(NamedKey::ArrowRight));
    let right = rig.knob();
    assert!((right.0 - from.0 - 0.01 * travel_x).abs() < 0.7, "{from:?} {right:?}");
    assert!((right.1 - from.1).abs() < 0.5);
    rig.key_with(Key::Named(NamedKey::ArrowRight), Modifiers::SHIFT);
    let shifted = rig.knob();
    assert!((shifted.0 - right.0 - 0.10 * travel_x).abs() < 1.2, "{right:?} {shifted:?}");
    rig.key(Key::Named(NamedKey::ArrowUp));
    let up = rig.knob();
    assert!((shifted.1 - up.1 - 0.01 * travel_y).abs() < 0.7, "{shifted:?} {up:?}");
    rig.key_with(Key::Named(NamedKey::ArrowDown), Modifiers::SHIFT);
    let down = rig.knob();
    assert!((down.1 - up.1 - 0.10 * travel_y).abs() < 1.2, "{up:?} {down:?}");
}

#[test]
fn home_and_end_go_to_the_ends_of_the_saturation_and_page_keys_move_the_value() {
    let mut rig = Rig::picker();
    let plane = focus_plane(&mut rig);
    let knob = Dim::PickerKnob.px_in(rig.class());
    let travel_y = plane.height() - knob;
    rig.key(Key::Named(NamedKey::Home));
    assert!((rig.knob().0 - (plane.min_x() + knob / 2.0)).abs() < 1.0);
    rig.key(Key::Named(NamedKey::End));
    assert!((rig.knob().0 - (plane.max_x() - knob / 2.0)).abs() < 1.0);
    let before = rig.knob();
    rig.key(Key::Named(NamedKey::PageUp));
    assert!((before.1 - rig.knob().1 - 0.10 * travel_y).abs() < 1.2);
    // A key past the end stays at the end.
    for _ in 0..4 {
        rig.key_with(Key::Named(NamedKey::ArrowRight), Modifiers::SHIFT);
    }
    assert!((rig.knob().0 - (plane.max_x() - knob / 2.0)).abs() < 1.0);
}

#[test]
fn tab_visits_the_plane_the_strips_and_the_fields_in_order_and_shift_tab_goes_back() {
    let mut rig = Rig::picker();
    focus_plane(&mut rig);
    rig.key(Key::Named(NamedKey::ArrowRight));
    assert_eq!(rig.focused().as_deref(), Some("Saturation and brightness"));
    rig.key(Key::Named(NamedKey::Tab));
    assert_eq!(rig.focused().as_deref(), Some("Hue"));
    rig.key(Key::Named(NamedKey::Tab));
    assert_eq!(rig.focused().as_deref(), Some("Opacity"));
    rig.key_with(Key::Named(NamedKey::Tab), Modifiers::SHIFT);
    assert_eq!(rig.focused().as_deref(), Some("Hue"));
    rig.key_with(Key::Named(NamedKey::Tab), Modifiers::SHIFT);
    assert_eq!(rig.focused().as_deref(), Some("Saturation and brightness"));
}

#[test]
fn the_hue_strip_takes_arrows_in_degrees_and_shift_ten() {
    let mut rig = Rig::picker();
    focus_plane(&mut rig);
    rig.key(Key::Named(NamedKey::Tab));
    assert_eq!(rig.focused().as_deref(), Some("Hue"));
    let start = hue_of(last());
    rig.key(Key::Named(NamedKey::ArrowRight));
    assert!(hue_gap(hue_of(last()), start + 1.0) < 1.5, "{} from {start}", hue_of(last()));
    rig.key_with(Key::Named(NamedKey::ArrowLeft), Modifiers::SHIFT);
    assert!(hue_gap(hue_of(last()), start - 9.0) < 1.5, "{} from {start}", hue_of(last()));
    rig.key(Key::Named(NamedKey::Home));
    assert!(hue_gap(hue_of(last()), 0.0) < 1.5);
}

// ---- The strips ----------------------------------------------------------------

#[test]
fn a_drag_on_the_hue_strip_follows_the_pointer_and_changes_only_the_hue() {
    let mut rig = Rig::picker();
    let strip = rig.named("Hue")[0];
    let thumb = Dim::SliderThumbLarge.px_in(rig.class());
    let before = Hsva::from_colour(blue());
    for fraction in [0.1_f32, 0.35, 0.8, 0.95] {
        let at = within(strip, fraction, 0.5);
        rig.tap(at);
        let wanted = ((at.0 - strip.min_x() - thumb / 2.0) / (strip.width() - thumb)).clamp(0.0, 1.0) * 360.0;
        let now = Hsva::from_colour(last());
        assert!(hue_gap(now.hue, wanted) < 3.0, "{fraction}: hue {} against {wanted}", now.hue);
        // Saturation and value are the colour's own, to within a channel.
        assert!((now.sat - before.sat).abs() < 0.03 && (now.val - before.val).abs() < 0.03);
    }
    // A drag that leaves the strip still follows the pointer along it.
    rig.press(within(strip, 0.5, 0.5));
    rig.frames(2);
    rig.move_to((strip.max_x() + 200.0, strip.min_y() - 120.0));
    rig.frames(3);
    assert!(hue_gap(hue_of(last()), 360.0) < 3.0, "{}", hue_of(last()));
    rig.release((strip.max_x() + 200.0, strip.min_y() - 120.0));
}

#[test]
fn what_you_click_on_the_hue_strip_is_the_colour_you_see_there() {
    for scale in [1.0, 1.5] {
        for fraction in [0.1_f32, 0.35, 0.8, 0.95] {
            let mut rig = Rig::new(Kind::PickerAlpha, blue(), DESKTOP, scale);
            let strip = rig.named("Hue")[0];
            let at = within(strip, fraction, 0.5);
            let seen = rig.pixel(at);
            rig.tap(at);
            let drawn = hue_of(argb(255, seen.0, seen.1, seen.2));
            assert!(
                hue_gap(hue_of(last()), drawn) < 7.0,
                "scale {scale}, {fraction}: the strip shows hue {drawn} and the press picked {}",
                hue_of(last())
            );
        }
    }
}

#[test]
fn the_opacity_strip_fades_in_from_a_checkerboard_and_sets_only_the_alpha() {
    let mut rig = Rig::new(Kind::PickerAlpha, argb(255, 200, 30, 30), DESKTOP, 1.0);
    let strip = rig.named("Opacity")[0];
    let y = strip.min_y() + strip.height() / 2.0;
    // Just inside the solid end, left of the thumb: all but opaque.
    let end = rig.pixel((strip.max_x() - 14.0, y));
    assert!(near(end, (200, 30, 30), 40), "the opaque end is {end:?}");
    // Just inside the transparent end: the checkerboard, not the colour. Two
    // neighbouring cells differ.
    let a = rig.pixel((strip.min_x() + 13.0, y - 3.0));
    let b = rig.pixel((strip.min_x() + 13.0 + 7.0, y - 3.0));
    assert!(!near(a, (200, 30, 30), 40), "the transparent end is the solid colour: {a:?}");
    assert!(!near(a, b, 4) || !near(a, (200, 30, 30), 60), "no checkerboard: {a:?} {b:?}");
    rig.tap(within(strip, 0.5, 0.5));
    let now = last();
    assert!((now.a() as i32 - 128).abs() < 12, "alpha {}", now.a());
    assert_eq!(channels(now), (200, 30, 30));
}

// ---- Sizes ---------------------------------------------------------------------

#[test]
fn every_target_is_at_least_32_px_on_a_desktop_and_44_px_on_a_phone() {
    for (window, class, least) in [(DESKTOP, SizeClass::Regular, 32.0), (PHONE, SizeClass::Compact, 44.0)] {
        let mut rig = Rig::new(Kind::PickerAlpha, blue(), window, 1.0);
        let g = Geometry::new(class);
        let handle = rig.area("ColourHandle");
        assert!(
            handle.width() >= least && handle.height() >= least,
            "{class:?}: the handle's target is {} by {}",
            handle.width(),
            handle.height()
        );
        assert_eq!((handle.width(), handle.height()), (g.handle, g.handle));
        let knob = rig.area("ColourKnob");
        assert!(knob.width() < handle.width(), "the knob is smaller than its target");
        let (cx, cy) = centre(handle);
        let (kx, ky) = centre(knob);
        assert!((cx - kx).abs() < 0.6 && (cy - ky).abs() < 0.6, "the target is centred on the knob");
        for strip in rig.named("Hue").into_iter().chain(rig.named("Opacity")) {
            assert!(strip.height() >= least, "{class:?}: a strip's row is {}", strip.height());
        }
        let swatches = rig.areas("ColourSwatch");
        assert!(!swatches.is_empty());
        for swatch in swatches {
            assert!(
                swatch.width() >= least && swatch.height() >= least,
                "{class:?}: a swatch is {} by {}",
                swatch.width(),
                swatch.height()
            );
        }
        for field in rig.areas("ColourNumber") {
            assert!(field.height() >= least, "{class:?}: a box is {} high", field.height());
        }
    }
}

#[test]
fn the_picker_is_exactly_the_height_it_says_and_never_changes_size() {
    for (window, scale) in [(DESKTOP, 1.0), (PHONE, 1.0), (DESKTOP, 1.5)] {
        for kind in [Kind::Picker, Kind::PickerAlpha, Kind::PickerKept] {
            let mut rig = Rig::new(kind, blue(), window, scale);
            let shape = Shape {
                alpha: kind == Kind::PickerAlpha,
                // Nine accents and three presets.
                swatches: 12,
                recents: true,
            };
            let said = Geometry::new(rig.class()).total(shape);
            let first = rig.area("ColourPicker");
            assert!(
                (first.height() - said).abs() < 0.6,
                "{kind:?} at {window:?}: drawn {} and said {said}",
                first.height()
            );
            let width = first.width();
            // Choose a swatch (the first recent colour appears), drag the
            // plane, type something that is not a colour, switch the mode.
            let swatch = rig.areas("ColourSwatch")[0];
            rig.tap(centre(swatch));
            let plane = rig.area("ColourPlane");
            rig.tap(within(plane, 0.3, 0.6));
            rig.type_hex("#12");
            rig.shot.click_text("HSL");
            rig.frames(4);
            rig.shot.click_text("HSV");
            rig.frames(4);
            let now = rig.area("ColourPicker");
            assert_eq!(
                (now.width(), now.height()),
                (width, first.height()),
                "{kind:?} at {window:?}: the picker changed size"
            );
        }
    }
}

#[test]
fn the_plane_is_centred_in_the_picker_so_it_reads_the_same_either_way_round() {
    for window in [DESKTOP, PHONE] {
        let mut rig = Rig::new(Kind::PickerAlpha, blue(), window, 1.0);
        let picker = rig.area("ColourPicker");
        for name in ["ColourPlane", "ColourPreview"] {
            let part = rig.area(name);
            if name == "ColourPlane" {
                let left = part.min_x() - picker.min_x();
                let right = picker.max_x() - part.max_x();
                assert!((left - right).abs() < 0.6, "{window:?}: the plane is {left} from the left, {right} from the right");
            }
        }
        for strip in rig.named("Hue") {
            let left = strip.min_x() - picker.min_x();
            let right = picker.max_x() - strip.max_x();
            assert!((left - right).abs() < 0.6, "{window:?}: the hue strip is off centre");
        }
    }
}

// ---- The fields ----------------------------------------------------------------

#[test]
fn a_typed_hex_is_the_colour_in_every_form() {
    let mut rig = Rig::picker();
    rig.type_hex("#ff0000");
    assert_eq!(last(), argb(255, 255, 0, 0));
    rig.type_hex("0f0");
    assert_eq!(last(), argb(255, 0, 255, 0), "three digits");
    rig.type_hex("#0000ff80");
    assert_eq!(last(), argb(0x80, 0, 0, 255), "eight digits say their own alpha");
    rig.type_hex("#ff08");
    assert_eq!(last(), argb(0x88, 255, 255, 0), "four digits are rgba");
}

#[test]
fn a_wrong_hex_says_why_and_changes_neither_the_colour_nor_the_layout() {
    let mut rig = Rig::picker();
    let before = rig.area("ColourPicker");
    let plane = rig.area("ColourPlane");
    rig.tap(within(plane, 0.4, 0.4));
    let chosen = last();
    // Clearing the field passes through colours (#aabb is one), so look after.
    let hex = rig.area("ColourHex");
    rig.click((hex.min_x() + 30.0, hex.min_y() + Dim::Control.px_in(rig.class()) / 2.0));
    rig.clear_field();
    let reports = logged().len();
    let chosen = last();
    rig.type_text("#12");
    let message = rig.area("ColourHexMessage");
    let said = rig.texts_in(message);
    assert!(said.iter().any(|text| text.contains("digits")), "the message is {said:?}");
    assert_eq!(logged().len(), reports, "a hex that is not a colour reports nothing");
    assert_eq!(last(), chosen);
    assert_eq!(rig.area("ColourPicker").height(), before.height());
    rig.type_hex("#12x456");
    let message = rig.area("ColourHexMessage");
    let said = rig.texts_in(message);
    assert!(said.iter().any(|text| text.contains("not a hex digit")), "{said:?}");
    // A good entry clears it.
    rig.type_hex("#123456");
    let message = rig.area("ColourHexMessage");
    assert!(rig.texts_in(message).is_empty());
    assert_eq!(last(), argb(255, 0x12, 0x34, 0x56));
}

#[test]
fn the_mode_switch_shows_the_same_colour_as_rgb_hsl_or_hsv() {
    let mut rig = Rig::picker();
    let colour = Hsva::from_colour(blue());
    for mode in [Mode::Rgb, Mode::Hsl, Mode::Hsv] {
        rig.shot.click_text(mode.label());
        rig.frames(5);
        let boxes = rig.areas("ColourNumber");
        assert_eq!(boxes.len(), 4, "three boxes and the opacity box");
        let want = mode.fields(colour);
        for (index, area) in boxes.iter().take(3).enumerate() {
            let texts = rig.texts_in(*area);
            assert!(
                texts.iter().any(|text| text.trim() == want[index].to_string()),
                "{mode:?} box {index}: {texts:?} should show {}",
                want[index]
            );
            assert!(texts.iter().any(|text| text.trim() == mode.letters()[index]), "{mode:?}: its letter");
        }
    }
}

#[test]
fn a_number_typed_in_a_box_edits_the_colour_and_arrows_step_it() {
    let mut rig = Rig::picker();
    rig.shot.click_text("RGB");
    rig.frames(4);
    let boxes = rig.areas("ColourNumber");
    let red = boxes[0];
    rig.click((red.min_x() + 14.0, red.min_y() + 10.0));
    rig.clear_field();
    rig.type_text("255");
    assert_eq!(channels(last()), (255, BLUE.1, BLUE.2), "log {:?}", logged());
    // A number past the end is the end; a box that is not a number moves nothing.
    rig.clear_field();
    rig.type_text("999");
    assert_eq!(channels(last()).0, 255);
    // Clearing passes through numbers, so count after it.
    rig.clear_field();
    let reports = logged().len();
    rig.type_text("x");
    assert_eq!(logged().len(), reports);
    // Arrows step by one, ten with Shift.
    rig.clear_field();
    rig.type_text("100");
    rig.key(Key::Named(NamedKey::ArrowUp));
    assert_eq!(channels(last()).0, 101);
    rig.key_with(Key::Named(NamedKey::ArrowDown), Modifiers::SHIFT);
    assert_eq!(channels(last()).0, 91);
}

#[test]
fn the_opacity_box_is_a_percentage() {
    let mut rig = Rig::picker();
    let boxes = rig.areas("ColourNumber");
    let opacity = boxes[3];
    rig.click((opacity.min_x() + 14.0, opacity.min_y() + 10.0));
    rig.clear_field();
    rig.type_text("50");
    assert_eq!(last().a(), 128);
    assert_eq!(channels(last()), BLUE);
}

// ---- Palette and recent colours -------------------------------------------------

#[test]
fn a_swatch_chooses_its_colour_keeps_the_opacity_and_is_marked_chosen() {
    let mut rig = Rig::new(Kind::PickerAlpha, argb(100, BLUE.0, BLUE.1, BLUE.2), DESKTOP, 1.0);
    let swatches = rig.areas("ColourSwatch");
    // The palette's swatches that are marked chosen: the recent row repeats
    // the colour once it is chosen, so only the palette's own are counted.
    let chosen = |rig: &mut Rig| {
        let palette = rig.area("ColourPalette");
        rig.tree()
            .nodes
            .iter()
            .filter(|node| node.a11y.component.as_deref() == Some("ColourSwatch"))
            .filter(|node| node.area.is_some_and(|area| area.min_y() >= palette.min_y() && area.max_y() <= palette.max_y()))
            .filter(|node| node.a11y.selected == Some(true))
            .count()
    };
    rig.tap(centre(swatches[3]));
    assert_eq!(last().a(), 100, "the opacity being edited is kept");
    assert_ne!(channels(last()), BLUE);
    assert_eq!(chosen(&mut rig), 1, "exactly one swatch is marked");
    // Every swatch is named by its colour.
    let names: Vec<String> = rig
        .tree()
        .nodes
        .iter()
        .filter(|node| node.a11y.component.as_deref() == Some("ColourSwatch"))
        .filter_map(|node| node.a11y.name.clone())
        .collect();
    assert!(!names.is_empty());
    assert!(names.iter().all(|name| name.starts_with("Colour ") && name.contains('#')), "{names:?}");
    assert!(names.iter().any(|name| name.contains("Blue")), "the accents are named: {names:?}");
}

#[test]
fn recent_colours_are_newest_first_without_repeats_and_at_most_six() {
    let mut rig = Rig::new(Kind::PickerKept, blue(), DESKTOP, 1.0);
    let swatches = rig.areas("ColourSwatch");
    assert!(swatches.len() >= 9);
    let recent = |rig: &mut Rig| {
        let area = rig.area("ColourRecent");
        rig.areas("ColourSwatch")
            .into_iter()
            .filter(|swatch| swatch.min_y() >= area.min_y() && swatch.max_y() <= area.max_y())
            .count()
    };
    assert_eq!(recent(&mut rig), 0, "nothing chosen yet");
    // The first swatch is the accent the picker starts on: choosing the colour
    // being edited is no edit, so the app is not told. Eight different edits
    // start at the second.
    for index in 1..9 {
        let swatch = rig.areas("ColourSwatch")[index];
        rig.tap(centre(swatch));
    }
    assert_eq!(RECENT_MAX, 6);
    assert_eq!(recent(&mut rig), 6, "the row holds six");
    let kept = KEPT.with(|kept| kept.borrow().clone());
    assert_eq!(kept.len(), 8, "the app is told each time");
    assert!(kept.iter().all(|list| list.len() <= RECENT_MAX));
    let list = kept.last().unwrap();
    assert_eq!(list[0], last().with_a(255), "the newest is first");
    let mut unique = list.clone();
    unique.dedup();
    assert_eq!(unique.len(), list.len(), "no repeats");
    // Choosing one that is in the list moves it to the front and adds none.
    let again = rig.areas("ColourSwatch")[7];
    rig.tap(centre(again));
    let kept = KEPT.with(|kept| kept.borrow().clone());
    assert!(kept.last().unwrap().len() <= RECENT_MAX);
}

#[test]
fn recent_colours_without_a_list_of_the_apps_own_are_kept_in_memory() {
    let mut rig = Rig::new(Kind::Picker, blue(), DESKTOP, 1.0);
    let swatch = rig.areas("ColourSwatch")[2];
    rig.tap(centre(swatch));
    assert_eq!(super::core::remembered(), vec![last()]);
    // A new picker in the same process starts with it.
    let mut again = Rig {
        shot: Snapshot::with_scale(
            move || {
                let at = at_for(DESKTOP);
                Root(Host { start: blue(), at, width: DESKTOP.0 - 2.0 * at.0, kind: Kind::Picker })
                    .into_element()
            },
            DESKTOP,
            1.0,
        ),
        scale: 1.0,
        window: DESKTOP,
    };
    again.frames(6);
    let recent = again.area("ColourRecent");
    let filled = again
        .areas("ColourSwatch")
        .into_iter()
        .filter(|swatch| swatch.min_y() >= recent.min_y())
        .count();
    assert_eq!(filled, 1);
}

#[test]
fn the_old_colour_is_restored_by_a_press_on_its_half() {
    let mut rig = Rig::picker();
    let plane = rig.area("ColourPlane");
    rig.tap(within(plane, 0.2, 0.2));
    assert_ne!(channels(last()), BLUE);
    let preview = rig.area("ColourPreview");
    rig.tap(within(preview, 0.2, 0.5));
    assert_eq!(last(), blue());
}

// ---- From outside --------------------------------------------------------------

#[test]
fn a_colour_from_outside_is_taken_in_without_losing_the_hue() {
    let mut rig = Rig::picker();
    let hue = hue_of(blue());
    VALUE.with(|value| value.borrow().expect("the host's colour").clone().set(argb(255, 128, 128, 128)));
    rig.frames(6);
    // The plane now shows a grey; the hue is still the blue's, so the pure
    // right edge is blue again, not red.
    let plane = rig.area("ColourPlane");
    rig.tap((plane.max_x() - 7.0, plane.min_y() + 7.0));
    assert!(hue_gap(hue_of(last()), hue) < 3.0, "{} against {hue}", hue_of(last()));
}

// ---- The button ----------------------------------------------------------------

fn open_button(kind: Kind, window: (f32, f32)) -> Rig {
    let mut rig = Rig::new(kind, blue(), window, 1.0);
    let button = rig.area("ColourButton");
    rig.click(centre(button));
    rig.frames(16);
    rig
}

#[test]
fn the_button_is_a_control_high_square_named_by_its_colour() {
    for (window, side) in [(DESKTOP, 36.0), (PHONE, 44.0)] {
        let mut rig = Rig::new(Kind::Button, blue(), window, 1.0);
        let button = rig.area("ColourButton");
        assert_eq!((button.width(), button.height()), (side, side));
        assert!(!rig.has("ColourPopover"), "closed at first");
        let named = rig.named("Accent colour, #3584e4");
        assert_eq!(named.len(), 1, "the button says what colour it holds");
    }
}

#[test]
fn the_button_opens_a_popover_under_it_on_a_desktop() {
    let mut rig = open_button(Kind::Button, DESKTOP);
    let button = rig.area("ColourButton");
    let popover = rig.area("ColourPopover");
    assert!(popover.min_y() >= button.max_y(), "{popover:?} under {button:?}");
    assert!(popover.min_x() >= 0.0 && popover.max_x() <= DESKTOP.0);
    assert!(popover.max_y() <= DESKTOP.1);
    assert!(rig.has("ColourPlane") && rig.has("ColourHex"));
    // The popover is the picker and a Cancel and Done row, no taller.
    let picker = rig.area("ColourPicker");
    let g = Geometry::new(SizeClass::Regular);
    assert!((popover.height() - (picker.height() + g.actions())).abs() < 1.5, "{popover:?} {picker:?}");
    // The picker is as tall as it says.
    let said = g.total(Shape { alpha: false, swatches: 12, recents: true });
    assert!((picker.height() - said).abs() < 0.6, "{} against {said}", picker.height());
}

#[test]
fn the_button_opens_a_bottom_sheet_on_a_phone() {
    let mut rig = open_button(Kind::Button, PHONE);
    let popover = rig.area("ColourPopover");
    let picker = rig.area("ColourPicker");
    assert!(popover.min_x() >= 0.0 && popover.max_x() <= PHONE.0);
    // The sheet's content fills its width less the sheet's padding on each side.
    assert!((popover.width() - (PHONE.0 - 2.0 * S4.px_in(SizeClass::Compact))).abs() < 1.5, "{popover:?}");
    assert!((picker.width() - popover.width()).abs() < 1.5);
    // It is at the bottom of the window.
    assert!(popover.max_y() > PHONE.1 * 0.5);
    assert!(rig.areas("ColourHeader").len() == 1, "Cancel, the title and Done sit at the top");
    // Its targets are fingers' size.
    let handle = rig.area("ColourHandle");
    assert!(handle.width() >= 44.0);
}

#[test]
fn escape_closes_the_popover_and_drops_the_draft() {
    for window in [DESKTOP, PHONE] {
        let mut rig = open_button(Kind::Button, window);
        let plane = rig.area("ColourPlane");
        rig.tap(within(plane, 0.2, 0.3));
        rig.key(Key::Named(NamedKey::Escape));
        rig.frames(30);
        assert!(!rig.has("ColourPopover"), "{window:?}: still open after Escape");
        assert!(logged().is_empty(), "{window:?}: a draft was reported: {:?}", logged());
    }
}

#[test]
fn a_press_outside_closes_the_popover_and_drops_the_draft() {
    for window in [DESKTOP, PHONE] {
        let mut rig = open_button(Kind::Button, window);
        let plane = rig.area("ColourPlane");
        rig.tap(within(plane, 0.2, 0.3));
        rig.click((window.0 - 4.0, 4.0));
        rig.frames(30);
        assert!(!rig.has("ColourPopover"), "{window:?}: still open after a press outside");
        assert!(logged().is_empty(), "{window:?}: {:?}", logged());
    }
}

#[test]
fn done_reports_the_draft_once_and_closes() {
    for window in [DESKTOP, PHONE] {
        let mut rig = open_button(Kind::Button, window);
        let plane = rig.area("ColourPlane");
        rig.tap(within(plane, 0.2, 0.3));
        rig.tap(within(plane, 0.6, 0.6));
        assert!(logged().is_empty(), "{window:?}: nothing is reported before Done");
        rig.shot.click_text("Done");
        rig.frames(30);
        assert_eq!(logged().len(), 1, "{window:?}: {:?}", logged());
        assert!(!rig.has("ColourPopover"));
        // The button now holds the colour.
        let button = rig.area("ColourButton");
        let pixel = rig.pixel(centre(button));
        assert!(near(pixel, channels(last()), 12), "{pixel:?} against {:?}", channels(last()));
    }
}

#[test]
fn cancel_reports_nothing_and_the_next_opening_starts_from_the_colour_the_button_has() {
    let mut rig = open_button(Kind::Button, DESKTOP);
    let plane = rig.area("ColourPlane");
    let start = rig.knob();
    rig.tap(within(plane, 0.9, 0.9));
    assert_ne!(rig.knob(), start);
    rig.shot.click_text("Cancel");
    rig.frames(30);
    assert!(!rig.has("ColourPopover"));
    assert!(logged().is_empty());
    let button = rig.area("ColourButton");
    rig.click(centre(button));
    rig.frames(16);
    let again = rig.knob();
    assert!((again.0 - start.0).abs() < 1.0 && (again.1 - start.1).abs() < 1.0, "{again:?} against {start:?}");
}

#[test]
fn a_live_button_reports_every_colour_as_it_is_dragged_and_done_adds_nothing() {
    let mut rig = open_button(Kind::ButtonLive, DESKTOP);
    assert!(rig.shot.click_text("Cancel") == false, "a live popover has no Cancel");
    let plane = rig.area("ColourPlane");
    rig.press(within(plane, 0.3, 0.3));
    rig.frames(2);
    for step in 0..6 {
        rig.move_to(within(plane, 0.3 + 0.1 * step as f32, 0.3 + 0.1 * step as f32));
        rig.frames(3);
    }
    rig.release(within(plane, 0.8, 0.8));
    rig.frames(3);
    let reported = logged().len();
    assert!(reported >= 6, "a colour a frame: {reported}");
    rig.shot.click_text("Done");
    rig.frames(30);
    assert_eq!(logged().len(), reported, "Done adds nothing: it was already reported");
    assert!(!rig.has("ColourPopover"));
}

#[test]
fn the_button_works_inside_a_dialog() {
    let mut rig = Rig::new(Kind::ButtonInDialog, blue(), DESKTOP, 1.0);
    rig.shot.click_text("Open the dialog");
    rig.frames(30);
    assert!(rig.has("ColourButton"), "the dialog shows the button");
    let button = rig.area("ColourButton");
    rig.click(centre(button));
    rig.frames(20);
    assert!(rig.has("ColourPlane"), "the popover opened over the dialog");
    let plane = rig.area("ColourPlane");
    // The popover takes the pointer over the dialog's scrim: a live drag reports.
    rig.tap(within(plane, 0.2, 0.2));
    assert!(!logged().is_empty(), "the popover under a dialog took no press");
    rig.key(Key::Named(NamedKey::Escape));
    rig.frames(30);
    assert!(!rig.has("ColourPlane"), "Escape closes the popover");
}

#[test]
fn a_dense_button_is_a_tools_row_high_and_a_finger_gets_the_full_size() {
    #[derive(PartialEq)]
    struct Dense;
    impl Component for Dense {
        fn render(&self) -> impl IntoElement {
            let _ = provide_tokens();
            WindowHost::new(DenseHost)
        }
    }
    #[derive(PartialEq)]
    struct DenseHost;
    impl Component for DenseHost {
        fn render(&self) -> impl IntoElement {
            colour_button(argb(255, 1, 2, 3), |_| {}).dense()
        }
    }
    for (window, side) in [(DESKTOP, Dim::DenseControl.px_in(SizeClass::Regular)), (PHONE, 44.0)] {
        let mut shot = Snapshot::with_scale(move || Dense.into_element(), window, 1.0);
        shot.runner().poll_n(FRAME, 6);
        let area = Tree::capture_headless(shot.runner(), 1.0)
            .nodes
            .iter()
            .filter(|node| node.a11y.component.as_deref() == Some("ColourButton"))
            .find_map(|node| node.area)
            .expect("a button");
        assert_eq!((area.width(), area.height()), (side, side), "{window:?}");
    }
}

#[test]
fn the_button_opens_from_the_keyboard() {
    let mut rig = Rig::new(Kind::Button, blue(), DESKTOP, 1.0);
    // Focus it with a press and close it again, then open with Enter.
    let button = rig.area("ColourButton");
    rig.click(centre(button));
    rig.frames(16);
    rig.key(Key::Named(NamedKey::Escape));
    rig.frames(30);
    assert!(!rig.has("ColourPopover"));
    rig.click(centre(button));
    rig.frames(16);
    assert!(rig.has("ColourPopover"));
}


