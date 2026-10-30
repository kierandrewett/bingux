//! `hit_area` widens what a finger can press at compact density without moving
//! anything. These tests press real nodes in a 390 px wide window: inside the
//! painted face, in the ring between the face and the touch target, and
//! outside the target. Freya delivers a press to a node that listens and whose
//! own box holds the pointer, then bubbles it up, so the transparent target
//! must listen for the control above it to hear a press outside its face.

use std::cell::{Cell, RefCell};
use std::time::Duration;

use freya::prelude::*;

use crate::component::ComponentName;
use super::switch;
use crate::inspector::snapshot::Snapshot as Tree;
use crate::shell::WindowControls;
use crate::shell::overlays::OverlayHost;
use crate::snapshot::Snapshot;
use crate::style::*;
use crate::theme::provide_tokens;
use crate::window::WindowHost;

const PHONE: (f32, f32) = (390.0, 800.0);

thread_local! {
    static TOGGLES: Cell<u32> = const { Cell::new(0) };
    static LAST: Cell<bool> = const { Cell::new(false) };
    static ACTIONS: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
}

#[derive(PartialEq)]
struct Root(Page);

#[derive(PartialEq, Clone, Copy)]
enum Page {
    Switch,
    WindowControls,
}

impl Component for Root {
    fn render(&self) -> impl IntoElement {
        let _ = provide_tokens();
        WindowHost::new(OverlayHost::new(self.0))
    }
}

impl Component for Page {
    fn render(&self) -> impl IntoElement {
        let on = use_state(|| false);
        let body = match self {
            // A switch with a marker beside it: the marker shows where the
            // switch's own box ends, so a hit area that took room would move it.
            Page::Switch => rect()
                .component("Row")
                .tw_layout(&[flex_row(), items(Align::Center), gap(S4)])
                .child(switch(on(), {
                    let mut on = on;
                    move |value| {
                        TOGGLES.with(|count| count.set(count.get() + 1));
                        LAST.with(|last| last.set(value));
                        on.set(value);
                    }
                }))
                .child(rect().component("Marker").tw_layout(&[w(Dim::Box5), h(Dim::Box5)]))
                .into_element(),
            Page::WindowControls => WindowControls.into_element(),
        };
        // A solid page: a hover layer is a few percent of black over nothing, which
        // a frame read back without its alpha cannot tell from no hover at all.
        rect()
            .tw_layout(&[flex_col(), p(S8), w_full(), h_full()])
            .background(Color::from_rgb(236, 236, 240))
            .child(body)
    }
}

fn start(page: Page) -> Snapshot {
    TOGGLES.with(|count| count.set(0));
    ACTIONS.with(|actions| actions.borrow_mut().clear());
    let mut shot = Snapshot::new(move || Root(page).into_element(), PHONE);
    shot.settle(Duration::from_millis(300));
    shot
}

fn areas(shot: &mut Snapshot, component: &str) -> Vec<(f32, f32, f32, f32)> {
    let tree = Tree::capture_headless(shot.runner(), 1.0);
    tree.nodes
        .iter()
        .filter(|node| node.a11y.component.as_deref() == Some(component))
        .filter_map(|node| node.area)
        .map(|area| (area.min_x(), area.min_y(), area.width(), area.height()))
        .collect()
}

fn toggles() -> u32 {
    TOGGLES.with(Cell::get)
}

fn actions() -> Vec<&'static str> {
    ACTIONS.with(|actions| actions.borrow().clone())
}

/// Called by the window controls when a press reaches one (test builds).
pub(crate) fn record_action(name: &'static str) {
    ACTIONS.with(|actions| actions.borrow_mut().push(name));
}

#[test]
fn the_switch_keeps_its_painted_size_and_moves_nothing() {
    let mut shot = start(Page::Switch);
    let switch = areas(&mut shot, "Switch")[0];
    let row = areas(&mut shot, "Row")[0];
    let marker = areas(&mut shot, "Marker")[0];
    let width = Dim::SwitchWidth.px_in(SizeClass::Compact);
    let height = Dim::SwitchHeight.px_in(SizeClass::Compact);
    assert_eq!((switch.2, switch.3), (width, height), "the switch is its painted size");
    assert_eq!(row.3, height, "the row is no taller than the switch: the target takes no room");
    assert!(
        (marker.0 - (switch.0 + width + S4.px_in(SizeClass::Compact))).abs() < 0.5,
        "the marker sits one gap after the switch: {marker:?} {switch:?}"
    );
    assert!(height < TOUCH_MIN, "the painted face is smaller than a finger target");
}

#[test]
fn a_press_in_the_ring_around_the_switch_face_toggles_it() {
    let mut shot = start(Page::Switch);
    let (x, y, width, height) = areas(&mut shot, "Switch")[0];
    let (cx, cy) = (x + width / 2.0, y + height / 2.0);

    shot.click(cx, cy);
    assert_eq!(toggles(), 1, "a press on the face toggles");

    // Above and below the face, inside the 44 px target.
    let ring = (TOUCH_MIN - height) / 2.0 - 2.0;
    assert!(ring > 2.0, "the ring is {ring} px");
    shot.click(cx, cy + height / 2.0 + ring);
    assert_eq!(toggles(), 2, "a press just below the face is inside the target");
    shot.click(cx, cy - height / 2.0 - ring);
    assert_eq!(toggles(), 3, "a press just above the face is inside the target");
}

#[test]
fn a_press_outside_the_switch_target_does_not_toggle_it() {
    let mut shot = start(Page::Switch);
    let (x, y, width, height) = areas(&mut shot, "Switch")[0];
    let (cx, cy) = (x + width / 2.0, y + height / 2.0);
    shot.click(cx, cy + TOUCH_MIN / 2.0 + 6.0);
    shot.click(cx, cy - TOUCH_MIN / 2.0 - 6.0);
    shot.click(x + width + 6.0, cy + height / 2.0 + 6.0);
    assert_eq!(toggles(), 0, "nothing outside the target reaches the switch");
}

#[test]
fn a_touch_in_the_ring_toggles_the_switch_once() {
    let mut shot = start(Page::Switch);
    let (x, y, width, height) = areas(&mut shot, "Switch")[0];
    shot.touch(x + width / 2.0, y + height + 6.0);
    assert_eq!(toggles(), 1, "a finger below the face toggles it exactly once");
}

#[test]
fn the_window_controls_keep_their_painted_size() {
    let mut shot = start(Page::WindowControls);
    let controls = areas(&mut shot, "WindowControl");
    assert_eq!(controls.len(), 3);
    let face = Dim::WindowControl.px_in(SizeClass::Compact);
    for control in &controls {
        assert_eq!((control.2, control.3), (face, face), "{control:?}");
    }
    let strip = areas(&mut shot, "WindowControls")[0];
    assert_eq!(strip.3, face, "the strip is as tall as a face");
    assert!(face < TOUCH_MIN);
}

#[test]
fn a_press_in_the_ring_around_a_window_control_reaches_it() {
    let mut shot = start(Page::WindowControls);
    let controls = areas(&mut shot, "WindowControl");
    let (x, y, side, _) = controls[0];
    let (cx, cy) = (x + side / 2.0, y + side / 2.0);

    shot.click(cx, cy);
    assert_eq!(actions(), ["minimise"], "a press on the face runs the action");

    // Below the face and left of it, inside the 44 px target of the first control.
    shot.click(cx, cy + side / 2.0 + 5.0);
    shot.click(cx - side / 2.0 - 5.0, cy);
    assert_eq!(actions(), ["minimise"; 3], "the ring around the face runs the action");
}

#[test]
fn a_press_outside_a_window_control_target_runs_nothing() {
    let mut shot = start(Page::WindowControls);
    let controls = areas(&mut shot, "WindowControl");
    let (x, y, side, _) = controls[0];
    let (cx, cy) = (x + side / 2.0, y + side / 2.0);
    shot.click(cx, cy + TOUCH_MIN / 2.0 + 6.0);
    shot.click(cx, cy - TOUCH_MIN / 2.0 - 6.0);
    shot.click(cx - TOUCH_MIN / 2.0 - 6.0, cy);
    assert!(actions().is_empty(), "{:?}", actions());
}

// ---------------------------------------------------------------------------
// Hover through the ring
// ---------------------------------------------------------------------------
//
// A press in the ring is heard because the hit area listens. Hover is the
// other half: `pointer_enter` and `pointer_leave` go to the topmost node only,
// so a control with a hit area reads its hover from `pointer_over` and
// `pointer_out`, which bubble. These tests move a real pointer and look at
// the drawn face: it fills while the pointer is anywhere in the expanded
// ring, and clears when the pointer leaves it.

/// The pixels of one frame, as the renderer drew them.
struct Frame {
    bytes: Vec<u8>,
    row: usize,
    scale: f32,
}

impl Frame {
    fn of(shot: &mut Snapshot) -> Self {
        use freya::engine::prelude::{SkData, SkImage, raster_n32_premul};
        let png = shot.runner().render();
        let image = SkImage::from_encoded(SkData::new_copy(&png)).expect("the frame decodes");
        let mut surface = raster_n32_premul((image.width(), image.height())).expect("a surface to read from");
        surface.canvas().draw_image(&image, (0, 0), None);
        let info = surface.canvas().image_info();
        let row = info.min_row_bytes();
        let mut bytes = vec![0u8; info.compute_byte_size(row)];
        assert!(surface.canvas().read_pixels(&info, &mut bytes, row, (0, 0)));
        Self { bytes, row, scale: image.width() as f32 / PHONE.0 }
    }

    /// The sum of every channel over a box, in logical pixels: a fill that
    /// changes any pixel of the face changes this.
    fn sum(&self, (x, y, width, height): (f32, f32, f32, f32)) -> (u64, u64, u64) {
        let mut total = (0u64, 0u64, 0u64);
        let (x0, y0) = ((x * self.scale) as usize, (y * self.scale) as usize);
        let (x1, y1) = (((x + width) * self.scale) as usize, ((y + height) * self.scale) as usize);
        for py in y0..y1 {
            for px in x0..x1 {
                let at = py * self.row + px * 4;
                // N32 on a little-endian machine is blue, green, red, alpha.
                total.0 += u64::from(self.bytes[at + 2]);
                total.1 += u64::from(self.bytes[at + 1]);
                total.2 += u64::from(self.bytes[at]);
            }
        }
        total
    }
}

/// Where the pointer rests: well clear of every control on the page.
fn far_away(shot: &mut Snapshot) {
    shot.hover(PHONE.0 - 20.0, PHONE.1 - 20.0);
    shot.settle(Duration::from_millis(250));
}

#[test]
fn the_switch_fills_while_the_pointer_is_in_the_ring_and_clears_when_it_leaves() {
    let mut shot = start(Page::Switch);
    let face = areas(&mut shot, "Switch")[0];
    let (x, y, width, height) = face;
    let (cx, cy) = (x + width / 2.0, y + height / 2.0);
    far_away(&mut shot);
    let rest = Frame::of(&mut shot).sum(face);

    shot.hover(cx, cy);
    shot.settle(Duration::from_millis(250));
    let on_face = Frame::of(&mut shot).sum(face);
    assert_ne!(on_face, rest, "the face takes the hover fill under the pointer");

    // Below the face, inside the 44 px target, outside the painted switch.
    let ring = height / 2.0 + 5.0;
    assert!(ring < TOUCH_MIN / 2.0, "the probe is inside the target");
    shot.hover(cx, cy + ring);
    shot.settle(Duration::from_millis(250));
    assert_eq!(Frame::of(&mut shot).sum(face), on_face, "the pointer in the ring fills the face exactly as the pointer on it");
    shot.hover(cx, cy - ring);
    shot.settle(Duration::from_millis(250));
    assert_eq!(Frame::of(&mut shot).sum(face), on_face, "and from above");

    // Just outside the target: it clears.
    shot.hover(cx, cy + TOUCH_MIN / 2.0 + 6.0);
    shot.settle(Duration::from_millis(250));
    assert_eq!(Frame::of(&mut shot).sum(face), rest, "outside the target the face is at rest");
    far_away(&mut shot);
    assert_eq!(Frame::of(&mut shot).sum(face), rest, "and far away");
}

#[test]
fn a_window_control_fills_while_the_pointer_is_in_the_ring_and_clears_when_it_leaves() {
    let mut shot = start(Page::WindowControls);
    let controls = areas(&mut shot, "WindowControl");
    let face = controls[0];
    let (x, y, side, _) = face;
    let (cx, cy) = (x + side / 2.0, y + side / 2.0);
    far_away(&mut shot);
    let rest = Frame::of(&mut shot).sum(face);

    shot.hover(cx, cy);
    shot.settle(Duration::from_millis(250));
    let on_face = Frame::of(&mut shot).sum(face);
    assert_ne!(on_face, rest, "the face takes the hover fill under the pointer");

    // Below the face and inside the 44 px target, where no other control is.
    let ring = side / 2.0 + 5.0;
    assert!(ring < TOUCH_MIN / 2.0, "the probe is inside the target");
    shot.hover(cx, cy + ring);
    shot.settle(Duration::from_millis(250));
    assert_eq!(Frame::of(&mut shot).sum(face), on_face, "the pointer in the ring fills the face exactly as the pointer on it");

    shot.hover(cx, cy + TOUCH_MIN / 2.0 + 6.0);
    shot.settle(Duration::from_millis(250));
    assert_eq!(Frame::of(&mut shot).sum(face), rest, "outside the target the face is at rest");
    far_away(&mut shot);
    assert_eq!(Frame::of(&mut shot).sum(face), rest, "and far away");
}

#[test]
fn close_lights_red_from_the_ring_and_goes_out_when_the_pointer_leaves() {
    let mut shot = start(Page::WindowControls);
    let controls = areas(&mut shot, "WindowControl");
    let face = controls[2];
    let (x, y, side, _) = face;
    let (cx, cy) = (x + side / 2.0, y + side / 2.0);
    far_away(&mut shot);
    let rest = Frame::of(&mut shot).sum(face);

    // Below the face, in the ring of the last control (nothing to its right).
    shot.hover(cx, cy + side / 2.0 + 5.0);
    shot.settle(Duration::from_millis(250));
    let lit = Frame::of(&mut shot).sum(face);
    assert_ne!(lit, rest, "the close control reacts to a pointer in its ring");
    // The sum covers the face with its white glyph, which adds to every channel: red is 1.7 times green
    // here. Which of green and blue is the lower depends on the accent and scheme the run reads from
    // gsettings (the real dconf in a shell, the memory backend of `scripts/app_env.py`), so red is held
    // well above both and neither is ranked against the other.
    assert!(
        lit.0 * 2 > lit.1 * 3 && lit.0 * 2 > lit.2 * 3 && lit.1 < rest.1,
        "and it goes red (red well above green and blue, green below the grey page): {lit:?} against {rest:?}"
    );

    far_away(&mut shot);
    assert_eq!(Frame::of(&mut shot).sum(face), rest, "it goes out when the pointer leaves");
}

#[test]
fn only_the_control_under_the_pointer_lights_where_two_targets_overlap() {
    let mut shot = start(Page::WindowControls);
    let controls = areas(&mut shot, "WindowControl");
    far_away(&mut shot);
    let rest: Vec<_> = controls.iter().map(|face| Frame::of(&mut shot).sum(*face)).collect();

    // The middle control's face: the 44 px targets of both neighbours reach toward it.
    let (x, y, side, _) = controls[1];
    shot.hover(x + side / 2.0, y + side / 2.0);
    shot.settle(Duration::from_millis(250));
    let frame = Frame::of(&mut shot);
    assert_ne!(frame.sum(controls[1]), rest[1], "the control under the pointer lights");
    assert_eq!(frame.sum(controls[0]), rest[0], "its left neighbour does not");
    assert_eq!(frame.sum(controls[2]), rest[2], "its right neighbour does not");
}
