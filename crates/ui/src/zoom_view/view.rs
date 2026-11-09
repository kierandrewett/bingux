//! The arithmetic of a zoomable, pannable picture: no Freya, so it is tested
//! without a window.
//!
//! A [`View`] is how the content is placed in a box (the viewport): its zoom as
//! a multiple of "fit", where its middle is relative to the box's middle, and
//! how it is turned and mirrored. "Fit" is the scale at which the whole turned
//! content fits the box, never above one: small content is not stretched.

/// One zoom step: the factor a `+` multiplies the zoom by.
pub const STEP: f32 = 1.25;
/// The smallest zoom, as a multiple of "fit".
pub const MIN_ZOOM: f32 = 0.5;
/// The largest zoom as a multiple of "fit" when the content is small; larger
/// content may go further, up to [`MAX_PIXEL_SCALE`] screen pixels per pixel.
pub const MAX_ZOOM: f32 = 32.0;
/// The most screen pixels one content pixel may cover.
pub const MAX_PIXEL_SCALE: f32 = 16.0;
/// How far Shift and an arrow pan, in logical pixels.
pub const PAN_STEP: f32 = 64.0;

/// How the content is placed: its zoom (1 is "fit"), where its middle is
/// relative to the box's middle, and how it is turned and mirrored.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub zoom: f32,
    pub pan: (f32, f32),
    /// Quarter turns clockwise, 0 to 3, applied after the mirror.
    pub turns: u8,
    /// Mirrored left to right in the content's own space, before it is turned.
    pub mirrored: bool,
}

impl Default for View {
    fn default() -> Self {
        View {
            zoom: 1.0,
            pan: (0.0, 0.0),
            turns: 0,
            mirrored: false,
        }
    }
}

/// What the person last asked for, which a resize keeps true: the content
/// stays fitted, or stays at actual size, until they zoom or pan.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FitMode {
    /// The whole content in the box.
    Fit,
    /// One content pixel on one screen pixel.
    Actual,
}

/// The size of content of `natural` pixels once turned `turns` quarters.
pub fn turned(natural: (f32, f32), turns: u8) -> (f32, f32) {
    if turns % 2 == 1 {
        (natural.1, natural.0)
    } else {
        natural
    }
}

/// The scale at which the whole turned content fits `viewport`, never above 1:
/// small content is not stretched.
pub fn fit_scale(natural: (f32, f32), turns: u8, viewport: (f32, f32)) -> f32 {
    let (width, height) = turned(natural, turns);
    if width <= 0.0 || height <= 0.0 || viewport.0 <= 0.0 || viewport.1 <= 0.0 {
        return 1.0;
    }
    (viewport.0 / width).min(viewport.1 / height).min(1.0)
}

impl View {
    /// The content's scale: screen pixels per content pixel.
    pub fn scale(&self, natural: (f32, f32), viewport: (f32, f32)) -> f32 {
        fit_scale(natural, self.turns, viewport) * self.zoom
    }

    /// The largest zoom for content of `natural` pixels: sixteen screen pixels
    /// per pixel, and never less than [`MAX_ZOOM`] times fit.
    pub fn max_zoom(natural: Option<(f32, f32)>, turns: u8, viewport: (f32, f32)) -> f32 {
        match natural {
            Some(natural) => (MAX_PIXEL_SCALE / fit_scale(natural, turns, viewport)).max(MAX_ZOOM),
            None => MAX_ZOOM,
        }
    }

    /// Zoom by `factor` about `at`, a point relative to the box's middle, so
    /// the pixel under the pointer stays under it.
    pub fn zoom_at(
        self,
        factor: f32,
        at: (f32, f32),
        natural: Option<(f32, f32)>,
        viewport: (f32, f32),
    ) -> View {
        let most = Self::max_zoom(natural, self.turns, viewport);
        let zoom = (self.zoom * factor).clamp(MIN_ZOOM, most);
        let ratio = zoom / self.zoom;
        let pan = (
            at.0 - (at.0 - self.pan.0) * ratio,
            at.1 - (at.1 - self.pan.1) * ratio,
        );
        View { zoom, pan, ..self }.clamped(natural, viewport)
    }

    /// Zoom so the content's scale is `scale`, about `at`.
    pub fn zoom_to_scale(
        self,
        scale: f32,
        at: (f32, f32),
        natural: Option<(f32, f32)>,
        viewport: (f32, f32),
    ) -> View {
        let now = match natural {
            Some(natural) => self.scale(natural, viewport),
            None => self.zoom,
        };
        if now <= 0.0 {
            return self;
        }
        self.zoom_at(scale / now, at, natural, viewport)
    }

    /// The pan limited to what keeps the content covering the box where it
    /// can: content smaller than the box on an axis stays centred there.
    pub fn clamped(self, natural: Option<(f32, f32)>, viewport: (f32, f32)) -> View {
        let shown = self.shown(natural, viewport);
        let limit = |shown: f32, room: f32| ((shown - room) / 2.0).max(0.0);
        let (x, y) = (limit(shown.0, viewport.0), limit(shown.1, viewport.1));
        View {
            pan: (self.pan.0.clamp(-x, x), self.pan.1.clamp(-y, y)),
            ..self
        }
    }

    /// The size the turned content takes on screen.
    pub fn shown(&self, natural: Option<(f32, f32)>, viewport: (f32, f32)) -> (f32, f32) {
        match natural {
            Some(natural) => {
                let scale = self.scale(natural, viewport);
                let (width, height) = turned(natural, self.turns);
                (width * scale, height * scale)
            }
            None => (viewport.0 * self.zoom, viewport.1 * self.zoom),
        }
    }

    /// The same view moved by `delta` screen pixels, kept inside the content.
    pub fn panned(
        self,
        delta: (f32, f32),
        natural: Option<(f32, f32)>,
        viewport: (f32, f32),
    ) -> View {
        View {
            pan: (self.pan.0 + delta.0, self.pan.1 + delta.1),
            ..self
        }
        .clamped(natural, viewport)
    }

    /// The zoom at which one content pixel is one device pixel, or `None` when
    /// the content's size is not known. `display` is the window's scale factor:
    /// at 2 a device pixel is half a logical one.
    pub fn actual_zoom(
        natural: Option<(f32, f32)>,
        turns: u8,
        viewport: (f32, f32),
        display: f32,
    ) -> Option<f32> {
        natural.map(|natural| 1.0 / display.max(0.25) / fit_scale(natural, turns, viewport))
    }

    /// "100%": the content's scale as a person reads it, in device pixels,
    /// when it is known.
    pub fn percent(
        &self,
        natural: Option<(f32, f32)>,
        viewport: (f32, f32),
        display: f32,
    ) -> Option<String> {
        natural.map(|natural| {
            format!(
                "{}%",
                (self.scale(natural, viewport) * display.max(0.25) * 100.0).round() as u32
            )
        })
    }

    /// A quarter turn. A turn refits: the pan and zoom of the old orientation
    /// mean nothing.
    pub fn rotated(self, right: bool, natural: Option<(f32, f32)>, viewport: (f32, f32)) -> View {
        let turns = if right {
            (self.turns + 1) % 4
        } else {
            (self.turns + 3) % 4
        };
        View {
            turns,
            zoom: 1.0,
            pan: (0.0, 0.0),
            mirrored: self.mirrored,
        }
        .clamped(natural, viewport)
    }

    /// Mirrored left to right as it is seen, whatever the turn.
    pub fn flipped_horizontally(self) -> View {
        View {
            turns: (4 - self.turns) % 4,
            mirrored: !self.mirrored,
            pan: (-self.pan.0, self.pan.1),
            ..self
        }
    }

    /// Mirrored top to bottom as it is seen, whatever the turn.
    pub fn flipped_vertically(self) -> View {
        View {
            turns: (6 - self.turns) % 4,
            mirrored: !self.mirrored,
            pan: (self.pan.0, -self.pan.1),
            ..self
        }
    }

    /// Fitted, keeping the turn and the mirror.
    pub fn fitted(self) -> View {
        View {
            turns: self.turns,
            mirrored: self.mirrored,
            ..View::default()
        }
    }

    /// At actual size, keeping the turn and the mirror, when the size is known.
    pub fn at_actual(
        self,
        natural: Option<(f32, f32)>,
        viewport: (f32, f32),
        display: f32,
    ) -> View {
        match Self::actual_zoom(natural, self.turns, viewport, display) {
            Some(zoom) => View {
                zoom: zoom.clamp(MIN_ZOOM, Self::max_zoom(natural, self.turns, viewport)),
                pan: (0.0, 0.0),
                ..self
            }
            .clamped(natural, viewport),
            None => self,
        }
    }

    /// Whether the whole content is in the box (nothing to pan).
    pub fn is_fit(&self) -> bool {
        (self.zoom - 1.0).abs() < 1e-3
    }
}

/// What a key does to the content.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    ZoomIn,
    ZoomOut,
    Fit,
    Actual,
    Rotate(bool),
    FlipHorizontal,
    FlipVertical,
    /// Move the content by this many logical pixels (Shift and an arrow).
    Pan(f32, f32),
}

/// What `key` means to a zoomable view, or `None`. Letters act only without
/// Ctrl, Alt and Meta.
pub fn route(key: &freya::prelude::Key, modifiers: freya::prelude::Modifiers) -> Option<Action> {
    use freya::prelude::{Key, Modifiers, NamedKey};
    if modifiers.intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::META) {
        return None;
    }
    if modifiers.contains(Modifiers::SHIFT) {
        if let Key::Named(named) = key {
            return match named {
                NamedKey::ArrowLeft => Some(Action::Pan(PAN_STEP, 0.0)),
                NamedKey::ArrowRight => Some(Action::Pan(-PAN_STEP, 0.0)),
                NamedKey::ArrowUp => Some(Action::Pan(0.0, PAN_STEP)),
                NamedKey::ArrowDown => Some(Action::Pan(0.0, -PAN_STEP)),
                _ => None,
            };
        }
    }
    let Key::Character(text) = key else {
        return None;
    };
    match text.as_str() {
        "+" | "=" => Some(Action::ZoomIn),
        "-" | "_" => Some(Action::ZoomOut),
        "0" => Some(Action::Fit),
        "1" => Some(Action::Actual),
        "r" => Some(Action::Rotate(true)),
        "R" => Some(Action::Rotate(false)),
        "h" | "H" => Some(Action::FlipHorizontal),
        "v" | "V" => Some(Action::FlipVertical),
        _ => None,
    }
}

/// What two fingers moving did: the factor the distance between them changed
/// by, the middle between them now (in the view's own coordinates), and how far
/// that middle moved.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PinchStep {
    pub factor: f32,
    pub middle: (f32, f32),
    pub moved: (f32, f32),
}

/// The gesture of two fingers that were at `before` and are at `now`, or
/// `None` while they are too close together for the ratio to mean anything.
pub fn pinch_step(before: [(f32, f32); 2], now: [(f32, f32); 2]) -> Option<PinchStep> {
    let distance = |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).hypot(a.1 - b.1);
    let middle = |a: (f32, f32), b: (f32, f32)| ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
    let (was, is) = (distance(before[0], before[1]), distance(now[0], now[1]));
    if was < 24.0 || is < 24.0 {
        return None;
    }
    let (m0, m1) = (middle(before[0], before[1]), middle(now[0], now[1]));
    Some(PinchStep {
        factor: is / was,
        middle: m1,
        moved: (m1.0 - m0.0, m1.1 - m0.1),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use freya::prelude::{Key, Modifiers, NamedKey};

    const BOX: (f32, f32) = (800.0, 600.0);

    #[test]
    fn small_content_is_not_stretched_and_large_content_fits() {
        assert_eq!(fit_scale((320.0, 200.0), 0, BOX), 1.0);
        assert_eq!(fit_scale((1600.0, 1200.0), 0, BOX), 0.5);
        // Turned on its side, wide content is tall.
        assert_eq!(fit_scale((1600.0, 400.0), 1, BOX), 0.375);
        assert_eq!(fit_scale((0.0, 0.0), 0, BOX), 1.0);
    }

    #[test]
    fn the_percentage_is_the_contents_own_scale_in_device_pixels() {
        let view = View::default();
        assert_eq!(
            view.percent(Some((1600.0, 1200.0)), BOX, 1.0).as_deref(),
            Some("50%")
        );
        assert_eq!(
            view.percent(Some((320.0, 200.0)), BOX, 1.0).as_deref(),
            Some("100%")
        );
        // On a 2x display the same fit is twice as many device pixels per pixel.
        assert_eq!(
            view.percent(Some((1600.0, 1200.0)), BOX, 2.0).as_deref(),
            Some("100%")
        );
        assert_eq!(view.percent(None, BOX, 1.0), None);
    }

    #[test]
    fn actual_size_is_the_zoom_that_undoes_the_fit() {
        let zoom = View::actual_zoom(Some((1600.0, 1200.0)), 0, BOX, 1.0).unwrap();
        assert_eq!(zoom, 2.0);
        let at_actual = View {
            zoom,
            ..View::default()
        };
        assert_eq!(at_actual.scale((1600.0, 1200.0), BOX), 1.0);
        assert_eq!(View::actual_zoom(None, 0, BOX, 1.0), None);
        // A 2x display draws one pixel per device pixel at half the logical scale.
        let on_hidpi = View::default().at_actual(Some((1600.0, 1200.0)), BOX, 2.0);
        assert_eq!(on_hidpi.scale((1600.0, 1200.0), BOX), 0.5);
    }

    #[test]
    fn zoom_keeps_the_point_under_the_pointer() {
        let natural = Some((1600.0, 1200.0));
        let view = View::default().zoom_at(2.0, (100.0, 0.0), natural, BOX);
        assert_eq!(view.zoom, 2.0);
        // The point 100 right of the middle stays 100 right of the middle:
        // the content moved 100 left, which the clamp allows (400 of slack).
        assert_eq!(view.pan, (-100.0, 0.0));
    }

    #[test]
    fn the_pan_stays_inside_the_content() {
        let natural = Some((1600.0, 1200.0));
        // At fit the content fills the box: nothing to pan.
        let fit = View {
            pan: (50.0, 50.0),
            ..View::default()
        }
        .clamped(natural, BOX);
        assert_eq!(fit.pan, (0.0, 0.0));
        // At 4x it is 3200 wide: 1200 of slack each side.
        let zoomed = View {
            zoom: 4.0,
            pan: (5000.0, -5000.0),
            ..View::default()
        }
        .clamped(natural, BOX);
        assert_eq!(zoomed.pan, (1200.0, -900.0));
        // Small content stays centred.
        let small = View {
            pan: (30.0, 30.0),
            ..View::default()
        }
        .clamped(Some((100.0, 100.0)), BOX);
        assert_eq!(small.pan, (0.0, 0.0));
    }

    #[test]
    fn zoom_is_limited_by_the_smallest_and_by_sixteen_pixels_per_pixel() {
        let view = View::default().zoom_at(1000.0, (0.0, 0.0), None, BOX);
        assert_eq!(view.zoom, MAX_ZOOM);
        let view = View::default().zoom_at(0.0001, (0.0, 0.0), None, BOX);
        assert_eq!(view.zoom, MIN_ZOOM);
        // A big photo fits at a tenth: its limit is sixteen device pixels, 160x fit.
        let photo = Some((8000.0, 6000.0));
        let fit = fit_scale((8000.0, 6000.0), 0, BOX);
        let most = View::default().zoom_at(10_000.0, (0.0, 0.0), photo, BOX);
        assert!(
            (most.scale((8000.0, 6000.0), BOX) - MAX_PIXEL_SCALE).abs() < 1e-3,
            "{fit}"
        );
    }

    #[test]
    fn a_turn_refits_and_four_turns_come_round() {
        let natural = Some((1600.0, 400.0));
        let mut view = View {
            zoom: 3.0,
            pan: (10.0, 10.0),
            ..View::default()
        };
        view = view.rotated(true, natural, BOX);
        assert_eq!((view.turns, view.zoom, view.pan), (1, 1.0, (0.0, 0.0)));
        for _ in 0..3 {
            view = view.rotated(true, natural, BOX);
        }
        assert_eq!(view.turns, 0);
        assert_eq!(view.rotated(false, natural, BOX).turns, 3);
    }

    #[test]
    fn a_flip_mirrors_what_is_seen_whatever_the_turn() {
        let view = View::default();
        let flipped = view.flipped_horizontally();
        assert_eq!((flipped.turns, flipped.mirrored), (0, true));
        // Flipping twice is the identity in every orientation.
        for turns in 0..4 {
            let start = View {
                turns,
                mirrored: turns % 2 == 0,
                ..View::default()
            };
            assert_eq!(start.flipped_horizontally().flipped_horizontally(), start);
            assert_eq!(start.flipped_vertically().flipped_vertically(), start);
        }
        // A quarter turn, then a horizontal flip as seen, is the content mirrored
        // in its own space and turned the other way.
        let turned = View {
            turns: 1,
            ..View::default()
        }
        .flipped_horizontally();
        assert_eq!((turned.turns, turned.mirrored), (3, true));
        // A vertical flip is a horizontal flip and a half turn.
        let vertical = View::default().flipped_vertically();
        assert_eq!((vertical.turns, vertical.mirrored), (2, true));
    }

    #[test]
    fn fit_and_actual_keep_the_orientation() {
        let view = View {
            zoom: 3.0,
            pan: (4.0, 4.0),
            turns: 2,
            mirrored: true,
        };
        let fit = view.fitted();
        assert_eq!(
            (fit.zoom, fit.pan, fit.turns, fit.mirrored),
            (1.0, (0.0, 0.0), 2, true)
        );
        let actual = view.at_actual(Some((1600.0, 1200.0)), BOX, 1.0);
        assert_eq!((actual.turns, actual.mirrored, actual.zoom), (2, true, 2.0));
    }

    #[test]
    fn keys_zoom_fit_turn_flip_and_pan() {
        let key = |text: &str, modifiers| route(&Key::Character(text.to_owned().into()), modifiers);
        let plain = Modifiers::empty();
        assert_eq!(key("+", plain), Some(Action::ZoomIn));
        assert_eq!(key("=", plain), Some(Action::ZoomIn));
        assert_eq!(key("-", plain), Some(Action::ZoomOut));
        assert_eq!(key("0", plain), Some(Action::Fit));
        assert_eq!(key("1", plain), Some(Action::Actual));
        assert_eq!(key("r", plain), Some(Action::Rotate(true)));
        assert_eq!(key("R", plain), Some(Action::Rotate(false)));
        assert_eq!(key("h", plain), Some(Action::FlipHorizontal));
        assert_eq!(key("v", plain), Some(Action::FlipVertical));
        assert_eq!(key("x", plain), None);
        assert_eq!(key("r", Modifiers::CONTROL), None);
        assert_eq!(
            route(&Key::Named(NamedKey::ArrowLeft), Modifiers::SHIFT),
            Some(Action::Pan(PAN_STEP, 0.0))
        );
        // A bare arrow is the owner's (the next file), not the view's.
        assert_eq!(route(&Key::Named(NamedKey::ArrowLeft), plain), None);
    }

    #[test]
    fn two_fingers_apart_zoom_in_and_moving_together_pan() {
        let step = pinch_step(
            [(100.0, 100.0), (200.0, 100.0)],
            [(50.0, 100.0), (250.0, 100.0)],
        )
        .unwrap();
        assert_eq!(step.factor, 2.0);
        assert_eq!(step.middle, (150.0, 100.0));
        assert_eq!(step.moved, (0.0, 0.0));
        let drag = pinch_step(
            [(100.0, 100.0), (200.0, 100.0)],
            [(110.0, 120.0), (210.0, 120.0)],
        )
        .unwrap();
        assert_eq!((drag.factor, drag.moved), (1.0, (10.0, 20.0)));
        // Fingers that are nearly together say nothing about a ratio.
        assert_eq!(
            pinch_step(
                [(100.0, 100.0), (110.0, 100.0)],
                [(100.0, 100.0), (200.0, 100.0)]
            ),
            None
        );
    }
}
