//! The saturation and value plane: saturation across, value up, in the hue
//! being edited.
//!
//! The plane is one Skia draw: a horizontal gradient from white to the pure
//! hue, multiplied by a vertical one from white to black, with dithering on.
//! That is exactly `hue * saturation * value` at every pixel, with no stacked
//! boxes to leave a seam and no 8-bit bands. It is a [`Capture`]: a press on
//! it takes the pointer until the button comes up, wherever the pointer goes,
//! and the handle is always where the pointer is, clamped to the plane.
//!
//! The handle is a round knob drawn with a double ring (a dark outline and a
//! white ring, so it shows on white, on black and on every hue between)
//! inside a target [`Dim::PickerHandle`] across, 32 px on a desktop and 44 px
//! for a finger, which is larger than the knob and centred on it. The target
//! is a child of the plane and overhangs its edge when the handle is at one,
//! so the handle stays easy to grab in a corner.
//!
//! The plane takes the arrow keys, 1 % a press, 10 % with Shift; Home and End
//! go to no saturation and full saturation, Page Up and Page Down move the
//! value ten percent.
//!
//! [`Capture`]: crate::controls::capture::Capture

use freya::engine::prelude::{
    BlendMode, Colors, Gradient, Interpolation, Paint as Pen, PaintStyle, Point as SkPoint,
    Rect as Cell, SkColor4f, TileMode, shaders,
};
use freya::prelude::*;

use super::core::Link;
use super::geometry::Geometry;
use super::model::Hsva;
use crate::component::ComponentName;
use crate::controls::capture::{CaptureEvent, Phase, Point, fraction, use_capture};
use crate::motion::Focusable;
use crate::style::*;
use crate::surface::use_tokens;

/// A key press moves the handle this fraction of its range.
const KEY_STEP: f32 = 0.01;
/// Shift, Page Up and Page Down make a key press this many steps.
const BIG: f32 = 10.0;

/// The saturation and value under a pointer at `local`, in the plane's own
/// coordinates. The handle's centre travels inside the plane by the knob's
/// radius, so a pointer within that margin of an edge, or beyond it, is the
/// edge: pure white, pure black and the pure hue are easy to reach.
pub(super) fn plane_at(local: Point, size: (f32, f32), knob: f32) -> (f32, f32) {
    let radius = knob / 2.0;
    (
        fraction(local.0, radius, size.0 - knob),
        1.0 - fraction(local.1, radius, size.1 - knob),
    )
}

/// The centre of the knob for `sat` and `val`, in the plane's own coordinates:
/// the inverse of [`plane_at`].
pub(super) fn knob_centre(sat: f32, val: f32, size: (f32, f32), knob: f32) -> Point {
    let radius = knob / 2.0;
    (
        radius + sat * (size.0 - knob),
        radius + (1.0 - val) * (size.1 - knob),
    )
}

/// What a key does to the handle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Nudge {
    /// Steps along each axis; right and up are positive. Shift makes them [`BIG`].
    By { x: f32, y: f32 },
    /// To no saturation.
    Start,
    /// To full saturation.
    End,
}

pub(super) fn nudge(key: &Key, modifiers: Modifiers) -> Option<Nudge> {
    let steps = if modifiers.contains(Modifiers::SHIFT) {
        BIG
    } else {
        1.0
    };
    Some(match key {
        Key::Named(NamedKey::ArrowLeft) => Nudge::By { x: -steps, y: 0.0 },
        Key::Named(NamedKey::ArrowRight) => Nudge::By { x: steps, y: 0.0 },
        Key::Named(NamedKey::ArrowUp) => Nudge::By { x: 0.0, y: steps },
        Key::Named(NamedKey::ArrowDown) => Nudge::By { x: 0.0, y: -steps },
        Key::Named(NamedKey::PageUp) => Nudge::By { x: 0.0, y: BIG },
        Key::Named(NamedKey::PageDown) => Nudge::By { x: 0.0, y: -BIG },
        Key::Named(NamedKey::Home) => Nudge::Start,
        Key::Named(NamedKey::End) => Nudge::End,
        _ => return None,
    })
}

/// The colour after `nudge`: left and right move the saturation, up and down
/// the value.
pub(super) fn nudged(current: Hsva, nudge: Nudge) -> Hsva {
    match nudge {
        Nudge::By { x, y } => {
            current.with_sv(current.sat + x * KEY_STEP, current.val + y * KEY_STEP)
        }
        Nudge::Start => current.with_sv(0.0, current.val),
        Nudge::End => current.with_sv(1.0, current.val),
    }
}

/// The plane's fill: saturation across and value down in `hue`, as one paint.
fn paint_plane(hue: Color) -> impl FnMut(&mut FillRenderContext) + 'static {
    move |context| {
        let (width, height) = (context.size.width, context.size.height);
        let ramp = |from: Color, to: Color, end: SkPoint| {
            let colours = [SkColor4f::from(from), SkColor4f::from(to)];
            let gradient = Gradient::new(
                Colors::new(&colours, None, TileMode::Clamp, None),
                Interpolation::default(),
            );
            shaders::linear_gradient((SkPoint::new(0.0, 0.0), end), &gradient, None)
        };
        let white = colour_from_argb(255, 255, 255, 255);
        let black = colour_from_argb(255, 0, 0, 0);
        let (Some(across), Some(down)) = (
            ramp(white, hue, SkPoint::new(width, 0.0)),
            ramp(white, black, SkPoint::new(0.0, height)),
        ) else {
            return;
        };
        let mut pen = Pen::default();
        pen.set_style(PaintStyle::Fill);
        pen.set_dither(true);
        pen.set_shader(shaders::blend(BlendMode::Multiply, across, down));
        context
            .canvas
            .draw_rect(Cell::from_wh(width, height), &pen);
    }
}

/// The plane for the colour `hsva`, editing through `link`.
#[derive(PartialEq)]
pub(super) struct Plane {
    pub hsva: Hsva,
    pub link: Link,
    pub geometry: Geometry,
    /// Fill the width the parent gives, as a phone's picker does.
    pub fill: bool,
}

impl Component for Plane {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let scale = *Platform::get().current_window().scale_factor.read();
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let g = self.geometry;
        let mut measured = use_state(|| 0.0_f32);
        let width = if *measured.read() > 0.0 {
            *measured.read()
        } else {
            g.width
        };
        let size = (width, g.plane);
        let link = self.link.clone();
        let capture = use_capture({
            let link = link.clone();
            move |event: CaptureEvent| {
                link.grab(matches!(event.phase, Phase::Start | Phase::Move));
                if event.phase == Phase::Start {
                    a11y_id.request_focus();
                }
                if event.phase != Phase::Cancel {
                    let (sat, val) = plane_at(event.local, size, g.knob);
                    link.set(link.current().with_sv(sat, val));
                }
            }
        });
        let on_key = {
            let link = link.clone();
            move |event: Event<KeyboardEventData>| {
                let Some(nudge) = nudge(&event.key, event.modifiers) else {
                    return;
                };
                event.stop_propagation();
                link.set(nudged(link.current(), nudge));
            }
        };
        let settle = link.clone();

        let (cx, cy) = knob_centre(self.hsva.sat, self.hsva.val, size, g.knob);
        let left = snapped(cx - g.handle / 2.0, scale);
        let top = snapped(cy - g.handle / 2.0, scale);
        let chosen = Hsva::new(self.hsva.hue, self.hsva.sat, self.hsva.val, 255).colour();
        let (sat, val) = (
            (self.hsva.sat * 100.0).round() as i32,
            (self.hsva.val * 100.0).round() as i32,
        );

        let knob = knob(&tokens, g.knob, chosen);
        let handle = rect()
            .component("ColourHandle")
            .layer(Layer::Relative(2))
            .position(Position::new_absolute().left(left).top(top))
            .width(Size::px(g.handle))
            .height(Size::px(g.handle))
            .tw(&tokens, &[center()])
            .child(knob);
        let face = painted(
            rect()
                .component("ColourPlaneFace")
                .position(Position::new_absolute().left(0.0).top(0.0))
                .width(Size::px(width))
                .height(Size::px(g.plane))
                .tw(&tokens, &[rounded(Radius::Control)]),
            paint_plane(self.hsva.hue_colour()),
        );
        // A crisp inner edge, so the plane is a shape on any background.
        let edge = rect()
            .component("ColourPlaneEdge")
            .layer(Layer::Relative(1))
            .position(Position::new_absolute().left(0.0).top(0.0))
            .width(Size::px(width))
            .height(Size::px(g.plane))
            .interactive(false)
            .tw(&tokens, &[rounded(Radius::Control), border(Line::Hairline)]);

        let plane = rect()
            .component("ColourPlane")
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Slider)
            .a11y_alt("Saturation and brightness")
            .a11y_builder(move |builder| {
                builder.set_value(format!("Saturation {sat}%, brightness {val}%"));
            })
            .maybe(self.fill, |plane| plane.width(Size::fill()))
            .maybe(!self.fill, |plane| plane.width(Size::px(g.width)))
            .height(Size::px(g.plane))
            .tw(&tokens, &[rounded(Radius::Control)])
            .on_key_down(on_key)
            .on_key_up(move |_: Event<KeyboardEventData>| settle.settle())
            .child(face)
            .child(edge)
            .child(handle);
        let tracker = capture.clone();
        capture
            .handlers(plane)
            .on_sized(move |event: Event<SizedEventData>| {
                tracker.sized(&event);
                measured.set_if_modified(event.area.width());
            })
            .focusable(focus())
    }
}

/// The knob: a dark outline, a white ring and the chosen colour in the middle.
/// Pure white and a translucent black, not theme colours: the plane is a
/// colour space, and the ring must show on every part of it.
fn knob(tokens: &crate::theme::Tokens, size: f32, chosen: Color) -> Rect {
    let outline = Dim::Hairline.px();
    let ring = Dim::Ring.px();
    let white_size = size - 2.0 * outline;
    let core_size = white_size - 2.0 * ring;
    rect()
        .component("ColourKnob")
        .width(Size::px(size))
        .height(Size::px(size))
        .interactive(false)
        .tw(
            tokens,
            &[
                center(),
                rounded(Radius::Pill),
                fill(colour_from_argb(150, 0, 0, 0)),
                shadow(Elevation::Knob),
            ],
        )
        .child(
            rect()
                .width(Size::px(white_size))
                .height(Size::px(white_size))
                .tw(
                    tokens,
                    &[
                        center(),
                        rounded(Radius::Pill),
                        fill(colour_from_argb(255, 255, 255, 255)),
                    ],
                )
                .child(
                    rect()
                        .width(Size::px(core_size))
                        .height(Size::px(core_size))
                        .tw(tokens, &[rounded(Radius::Pill), fill(chosen)]),
                ),
        )
}

/// A whole device pixel at or near `offset`, so the handle's edge is crisp at
/// a fractional display scale.
pub(super) fn snapped(offset: f32, scale: f64) -> f32 {
    let scale = scale as f32;
    if scale > 0.0 {
        (offset * scale).round() / scale
    } else {
        offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KNOB: f32 = 20.0;
    const SIZE: (f32, f32) = (264.0, 168.0);

    #[test]
    fn the_plane_maps_the_pointer_to_saturation_and_value() {
        let (sat, val) = plane_at((SIZE.0 / 2.0, SIZE.1 / 2.0), SIZE, KNOB);
        assert!((sat - 0.5).abs() < 1e-6 && (val - 0.5).abs() < 1e-6);
        // Saturation grows to the right and value upwards.
        assert_eq!(plane_at((KNOB / 2.0, KNOB / 2.0), SIZE, KNOB), (0.0, 1.0));
        assert_eq!(
            plane_at((SIZE.0 - KNOB / 2.0, SIZE.1 - KNOB / 2.0), SIZE, KNOB),
            (1.0, 0.0)
        );
    }

    #[test]
    fn a_pointer_past_an_edge_of_the_plane_is_the_edge() {
        assert_eq!(plane_at((-500.0, -500.0), SIZE, KNOB), (0.0, 1.0));
        assert_eq!(plane_at((5000.0, 5000.0), SIZE, KNOB), (1.0, 0.0));
        assert_eq!(plane_at((100.0, -9999.0), SIZE, KNOB).1, 1.0);
        assert_eq!(plane_at((-9999.0, 60.0), SIZE, KNOB).0, 0.0);
        // Inside the knob's margin of an edge it is the edge too.
        assert_eq!(plane_at((2.0, 3.0), SIZE, KNOB), (0.0, 1.0));
        assert_eq!(plane_at((SIZE.0 - 1.0, SIZE.1 - 1.0), SIZE, KNOB), (1.0, 0.0));
        // Not a number is the corner, never a panic.
        assert_eq!(plane_at((f32::NAN, f32::NAN), SIZE, KNOB), (0.0, 1.0));
    }

    #[test]
    fn the_knob_centre_is_the_pointer_wherever_the_knob_can_go() {
        for (x, y) in [(40.0_f32, 30.0_f32), (100.0, 66.0), (180.0, 120.0), (10.0, 10.0)] {
            let (sat, val) = plane_at((x, y), SIZE, KNOB);
            let (cx, cy) = knob_centre(sat, val, SIZE, KNOB);
            assert!((cx - x).abs() < 1e-3, "{x}: knob at {cx}");
            assert!((cy - y).abs() < 1e-3, "{y}: knob at {cy}");
        }
    }

    #[test]
    fn a_plane_narrower_than_a_knob_does_not_divide_by_zero() {
        assert_eq!(plane_at((5.0, 5.0), (10.0, 10.0), KNOB), (0.0, 1.0));
        let (x, y) = knob_centre(0.5, 0.5, (10.0, 10.0), KNOB);
        assert!(x.is_finite() && y.is_finite());
    }

    #[test]
    fn arrows_nudge_one_percent_and_shift_ten() {
        let none = Modifiers::default();
        let shift = Modifiers::SHIFT;
        let right = Key::Named(NamedKey::ArrowRight);
        assert_eq!(nudge(&right, none), Some(Nudge::By { x: 1.0, y: 0.0 }));
        assert_eq!(nudge(&right, shift), Some(Nudge::By { x: 10.0, y: 0.0 }));
        assert_eq!(
            nudge(&Key::Named(NamedKey::ArrowUp), none),
            Some(Nudge::By { x: 0.0, y: 1.0 })
        );
        assert_eq!(
            nudge(&Key::Named(NamedKey::ArrowDown), shift),
            Some(Nudge::By { x: 0.0, y: -10.0 })
        );
        assert_eq!(
            nudge(&Key::Named(NamedKey::PageUp), none),
            Some(Nudge::By { x: 0.0, y: BIG })
        );
        assert_eq!(nudge(&Key::Named(NamedKey::Home), none), Some(Nudge::Start));
        assert_eq!(nudge(&Key::Named(NamedKey::End), none), Some(Nudge::End));
        assert_eq!(nudge(&Key::Character("a".into()), none), None);
        assert_eq!(nudge(&Key::Named(NamedKey::Tab), none), None);
    }

    #[test]
    fn keys_move_the_plane_by_a_percent_and_stop_at_the_edges() {
        let state = Hsva::new(211.0, 0.5, 0.5, 255);
        let right = nudged(state, Nudge::By { x: 1.0, y: 0.0 });
        assert!((right.sat - 0.51).abs() < 1e-6 && right.val == 0.5);
        let up = nudged(state, Nudge::By { x: 0.0, y: 10.0 });
        assert!((up.val - 0.6).abs() < 1e-6 && up.sat == 0.5);
        let past = nudged(Hsva::new(211.0, 0.99, 0.01, 255), Nudge::By { x: 10.0, y: -10.0 });
        assert_eq!((past.sat, past.val), (1.0, 0.0));
        // The hue is not the plane's to change.
        assert_eq!(right.hue, 211.0);
        // Home and End are the two ends of the saturation, and keep the value.
        assert_eq!(nudged(state, Nudge::Start).sat, 0.0);
        assert_eq!(nudged(state, Nudge::End).sat, 1.0);
        assert_eq!(nudged(state, Nudge::End).val, 0.5);
    }

    #[test]
    fn a_handle_snaps_to_a_whole_device_pixel() {
        assert_eq!(snapped(10.4, 1.0), 10.0);
        assert_eq!(snapped(10.4, 2.0), 10.5);
        // At 1.5 a device pixel is two thirds of a logical one.
        let at = snapped(10.4, 1.5);
        assert!(((at * 1.5) - (at * 1.5).round()).abs() < 1e-4);
        assert!((at - 10.4).abs() <= 1.0 / 3.0 + 1e-4);
        // A scale that is not usable leaves it alone.
        assert_eq!(snapped(10.4, 0.0), 10.4);
    }
}
