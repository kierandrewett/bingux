//! Pictures: fit, actual size, zoom, pan and rotate, with an animated GIF
//! playing.
//!
//! The picture is drawn by Freya's `ImageViewer` (or `GifViewer`), which
//! decodes and caches off the UI thread; this viewer owns the *view* of it,
//! which is pure arithmetic ([`View`]) so it is tested without a window.
//!
//! A picture that is smaller than the box is shown at its own size, not
//! stretched: "Fit" never enlarges. A format whose size the header parser does
//! not read (SVG, HEIF) is fitted to the box by the picture element, and has
//! no percentage.
//!
//! | Key | Does |
//! | --- | ---- |
//! | `+`, `=`, `-` | zoom in and out, a quarter of a step each |
//! | `0` | fit the window |
//! | `1` | actual size, one picture pixel to one screen pixel |
//! | `R`, `Shift+R` | rotate a quarter turn right and left |
//! | Ctrl and wheel | zoom at the pointer; the wheel pans a zoomed picture |
//! | drag | pan a zoomed picture |

use std::sync::Arc;
use std::time::Duration;

use bingux_preview_core::{Loaded, PreviewSource};
use freya::prelude::*;
use freya::torin::prelude::Size2D;

use super::super::subject::Resolved;
use super::super::keys::Keyboard;
use crate::component::ComponentName;
use crate::controls::icon_button;
use crate::icons::Icon;
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{CAPTION, Ink};

/// The longest edge a picture is decoded to. The decoder never enlarges, and
/// scales larger pictures down while reading them.
const DECODE_EDGE: f32 = 4096.0;
/// How long a decoded picture stays in the cache once nothing shows it.
const KEEP: Duration = Duration::from_secs(5);
/// One zoom step: the factor a `+` multiplies the zoom by.
pub(super) const STEP: f32 = 1.25;
/// The smallest and largest zoom, as a multiple of "fit".
pub(super) const MIN_ZOOM: f32 = 0.5;
pub(super) const MAX_ZOOM: f32 = 32.0;
/// A GIF larger than this shows its first frame only: every frame of an
/// animation is decoded into memory.
const GIF_LIMIT: u64 = 24 * 1024 * 1024;

/// How the picture is placed: its zoom (1 is "fit"), where its middle is
/// relative to the box's middle, and how far it is turned.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct View {
    pub zoom: f32,
    pub pan: (f32, f32),
    /// Quarter turns clockwise, 0 to 3.
    pub turns: u8,
}

impl Default for View {
    fn default() -> Self {
        View {
            zoom: 1.0,
            pan: (0.0, 0.0),
            turns: 0,
        }
    }
}

/// The size of a picture of `natural` pixels once turned `turns` quarters.
pub(super) fn turned(natural: (f32, f32), turns: u8) -> (f32, f32) {
    if turns % 2 == 1 { (natural.1, natural.0) } else { natural }
}

/// The scale at which the whole turned picture fits `viewport`, never above 1:
/// a small picture is not stretched.
pub(super) fn fit_scale(natural: (f32, f32), turns: u8, viewport: (f32, f32)) -> f32 {
    let (width, height) = turned(natural, turns);
    if width <= 0.0 || height <= 0.0 || viewport.0 <= 0.0 || viewport.1 <= 0.0 {
        return 1.0;
    }
    (viewport.0 / width).min(viewport.1 / height).min(1.0)
}

impl View {
    /// The picture's scale: screen pixels per picture pixel.
    pub fn scale(&self, natural: (f32, f32), viewport: (f32, f32)) -> f32 {
        fit_scale(natural, self.turns, viewport) * self.zoom
    }

    /// Zoom by `factor` about `at`, a point relative to the box's middle, so
    /// the pixel under the pointer stays under it.
    pub fn zoom_at(self, factor: f32, at: (f32, f32), natural: Option<(f32, f32)>, viewport: (f32, f32)) -> View {
        let zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let ratio = zoom / self.zoom;
        let pan = ((at.0 - (at.0 - self.pan.0) * ratio), (at.1 - (at.1 - self.pan.1) * ratio));
        View { zoom, pan, ..self }.clamped(natural, viewport)
    }

    /// The pan limited to what keeps the picture covering the box where it can:
    /// a picture smaller than the box on an axis stays centred there.
    pub fn clamped(self, natural: Option<(f32, f32)>, viewport: (f32, f32)) -> View {
        let shown = match natural {
            Some(natural) => {
                let scale = self.scale(natural, viewport);
                let (width, height) = turned(natural, self.turns);
                (width * scale, height * scale)
            }
            None => (viewport.0 * self.zoom, viewport.1 * self.zoom),
        };
        let limit = |shown: f32, room: f32| ((shown - room) / 2.0).max(0.0);
        let (x, y) = (limit(shown.0, viewport.0), limit(shown.1, viewport.1));
        View {
            pan: (self.pan.0.clamp(-x, x), self.pan.1.clamp(-y, y)),
            ..self
        }
    }

    /// The zoom at which one picture pixel is one screen pixel, or `None`
    /// when the picture's size is not known.
    pub fn actual_zoom(natural: Option<(f32, f32)>, turns: u8, viewport: (f32, f32)) -> Option<f32> {
        natural.map(|natural| 1.0 / fit_scale(natural, turns, viewport))
    }

    /// "100%": the picture's scale as a person reads it, when it is known.
    pub fn percent(&self, natural: Option<(f32, f32)>, viewport: (f32, f32)) -> Option<String> {
        natural.map(|natural| format!("{}%", (self.scale(natural, viewport) * 100.0).round() as u32))
    }

    pub fn rotated(self, right: bool, natural: Option<(f32, f32)>, viewport: (f32, f32)) -> View {
        let turns = if right { (self.turns + 1) % 4 } else { (self.turns + 3) % 4 };
        // A turn refits: the pan and zoom of the old orientation mean nothing.
        View { turns, zoom: 1.0, pan: (0.0, 0.0) }.clamped(natural, viewport)
    }
}

/// What a key does in the picture viewer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Action {
    ZoomIn,
    ZoomOut,
    Fit,
    Actual,
    Rotate(bool),
}

pub(super) fn route(key: &Key, modifiers: Modifiers) -> Option<Action> {
    if modifiers.intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::META) {
        return None;
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
        _ => None,
    }
}

#[derive(PartialEq)]
pub(super) struct ImageView {
    pub resolved: Arc<Resolved>,
}

/// The size of the picture's pixels, when the header said it.
fn natural_size(resolved: &Resolved) -> Option<(f32, f32)> {
    match &resolved.loaded {
        Loaded::Image(Some(info)) => Some((info.width as f32, info.height as f32)),
        _ => None,
    }
}

impl Component for ImageView {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let view = use_state(View::default);
        let viewport = use_state(|| (0.0f32, 0.0f32));
        let drag = use_state(|| None::<((f32, f32), (f32, f32))>);
        let mut hovering = use_state(|| false);
        let keyboard = use_try_consume::<Keyboard>().unwrap_or_else(Keyboard::always);
        let natural = natural_size(&self.resolved);
        let source = self.resolved.source.clone();
        let current = *view.read();
        let (vw, vh) = *viewport.read();

        let apply = move |action: Action| {
            let mut view = view;
            let (vw, vh) = *viewport.peek();
            let now = *view.peek();
            let next = match action {
                Action::ZoomIn => now.zoom_at(STEP, (0.0, 0.0), natural, (vw, vh)),
                Action::ZoomOut => now.zoom_at(1.0 / STEP, (0.0, 0.0), natural, (vw, vh)),
                Action::Fit => View { turns: now.turns, ..View::default() },
                Action::Actual => match View::actual_zoom(natural, now.turns, (vw, vh)) {
                    Some(zoom) => View { zoom, pan: (0.0, 0.0), turns: now.turns }.clamped(natural, (vw, vh)),
                    None => now,
                },
                Action::Rotate(right) => now.rotated(right, natural, (vw, vh)),
            };
            view.set_if_modified(next);
        };

        let on_key = move |event: Event<KeyboardEventData>| {
            // A pane leaves its keys with the list until it is clicked.
            if !keyboard.held() {
                return;
            }
            if let Some(action) = route(&event.key, event.modifiers) {
                event.stop_propagation();
                apply(action);
            }
        };
        let on_wheel = move |event: Event<WheelEventData>| {
            let mut view = view;
            let (vw, vh) = *viewport.peek();
            let now = *view.peek();
            let delta = event.delta_y as f32;
            if freya_winit::modifiers::live().contains(Modifiers::CONTROL) {
                event.stop_propagation();
                let at = (event.element_location.x as f32 - vw / 2.0, event.element_location.y as f32 - vh / 2.0);
                view.set_if_modified(now.zoom_at((-delta * 0.002).exp(), at, natural, (vw, vh)));
            } else if now.zoom != 1.0 {
                event.stop_propagation();
                let moved = View { pan: (now.pan.0 - event.delta_x as f32, now.pan.1 - delta), ..now };
                view.set_if_modified(moved.clamped(natural, (vw, vh)));
            }
        };

        // Where the picture sits: a box of its own turned size, centred on the
        // viewport and moved by the pan, rotated about its own middle.
        let (width, height) = match natural {
            Some(natural) => {
                let scale = current.scale(natural, (vw, vh));
                (natural.0 * scale, natural.1 * scale)
            }
            None => {
                let (w, h) = turned((vw * current.zoom, vh * current.zoom), current.turns);
                (w, h)
            }
        };
        let left = (vw - width) / 2.0 + current.pan.0;
        let top = (vh - height) / 2.0 + current.pan.1;
        let is_gif = self.resolved.sniffed.mime.mime == "image/gif" && self.resolved.facts.size.is_some_and(|size| size <= GIF_LIMIT);
        let picture = picture(&source, is_gif, width, height);

        let fit_percent = current.percent(natural, (vw, vh));
        let label = fit_percent.unwrap_or_else(|| format!("{}%", (current.zoom * 100.0).round() as u32));
        let label_width = crate::controls::path_label::measure_text("3200%", CAPTION) + 8.0;
        // Zoom out, the percentage, zoom in, fit: shown while the pointer is on
        // the picture or the picture is zoomed in. (R and Shift+R rotate.)
        let controls = (*hovering.read() || current.zoom != 1.0).then(|| {
            rect()
                .tw(&tokens, &[flex_row(), items(Align::Center), gap(S1), p(S1), rounded(Radius::Pill), bg(Paint::Floating), shadow(Elevation::Floating)])
                .child(icon_button(Icon::ZoomOut, move |_| apply(Action::ZoomOut)).ink(Ink::OnFloating).tooltip("Zoom out (-)").enabled(current.zoom > MIN_ZOOM))
                .child(
                    rect()
                        .width(Size::px(label_width))
                        .tw(&tokens, &[flex_row(), justify(Align::Center)])
                        .child(CAPTION.in_ink(Ink::OnFloating).label(&tokens, label).max_lines(1)),
                )
                .child(icon_button(Icon::ZoomIn, move |_| apply(Action::ZoomIn)).ink(Ink::OnFloating).tooltip("Zoom in (+)").enabled(current.zoom < MAX_ZOOM))
                .child(icon_button(Icon::ZoomFitBest, move |_| apply(Action::Fit)).ink(Ink::OnFloating).tooltip("Fit to window (0)"))
        });

        rect()
            .component("PreviewImage")
            .tw(&tokens, &[w_full(), h_full(), clip(), bg(Paint::Recess)])
            .on_global_key_down(on_key)
            .on_pointer_enter(move |_| hovering.set_if_modified(true))
            .on_pointer_leave(move |_| hovering.set_if_modified(false))
            .on_sized(move |event: Event<SizedEventData>| {
                let (mut viewport, mut view) = (viewport, view);
                let size = (event.area.width(), event.area.height());
                if *viewport.peek() != size {
                    viewport.set(size);
                    let now = *view.peek();
                    view.set_if_modified(now.clamped(natural, size));
                }
            })
            .on_wheel(on_wheel)
            .on_pointer_down(move |event: Event<PointerEventData>| {
                let mut drag = drag;
                if event.data().is_primary() {
                    let at = event.global_location();
                    drag.set(Some(((at.x as f32, at.y as f32), view.peek().pan)));
                }
            })
            .on_global_pointer_move(move |event: Event<PointerEventData>| {
                let mut view = view;
                let Some((from, pan)) = *drag.peek() else {
                    return;
                };
                let (vw, vh) = *viewport.peek();
                let at = event.global_location();
                let now = *view.peek();
                let moved = View { pan: (pan.0 + at.x as f32 - from.0, pan.1 + at.y as f32 - from.1), ..now };
                view.set_if_modified(moved.clamped(natural, (vw, vh)));
            })
            .on_global_pointer_up(move |_| {
                let mut drag = drag;
                drag.set_if_modified(None);
            })
            .child(
                rect()
                    .position(Position::new_absolute().left(left).top(top))
                    .width(Size::px(width))
                    .height(Size::px(height))
                    .rotate(f32::from(current.turns) * 90.0)
                    .child(picture),
            )
            .maybe_child(controls.map(|controls| {
                rect()
                    .layer(crate::layers::VIEWER_CONTROLS)
                    .position(Position::new_absolute().bottom(tokens_gap()).left(0.0))
                    .width(Size::fill())
                    .tw(&tokens, &[flex_row(), justify(Align::Center)])
                    .child(controls)
            }))
    }
}

/// The room between the controls and the box's bottom edge.
fn tokens_gap() -> f32 {
    S4.px()
}

/// The picture element at `width` by `height`.
fn picture(source: &PreviewSource, gif: bool, width: f32, height: f32) -> Element {
    let Some(path) = source.as_path().map(ToOwned::to_owned) else {
        // Bytes in memory: the viewer's own cache key is the name.
        let PreviewSource::Bytes { name, data } = source else {
            return Element::from(rect());
        };
        return ImageViewer::new((name.clone(), bytes::Bytes::copy_from_slice(data)))
            .decode_mode(DecodeMode::Custom(Size2D::new(DECODE_EDGE, DECODE_EDGE)))
            .width(Size::px(width))
            .height(Size::px(height))
            .aspect_ratio(AspectRatio::Min)
            .image_cover(ImageCover::Center)
            .into();
    };
    if gif {
        return GifViewer::new(path)
            .asset_age(KEEP)
            .width(Size::px(width))
            .height(Size::px(height))
            .aspect_ratio(AspectRatio::Min)
            .image_cover(ImageCover::Center)
            .into();
    }
    if source.name().to_ascii_lowercase().ends_with(".svg") {
        return SvgViewer::new(path)
            .width(Size::px(width))
            .height(Size::px(height))
            .into();
    }
    ImageViewer::new(path)
        .decode_mode(DecodeMode::Custom(Size2D::new(DECODE_EDGE, DECODE_EDGE)))
        .asset_age(KEEP)
        .width(Size::px(width))
        .height(Size::px(height))
        .aspect_ratio(AspectRatio::Min)
        .image_cover(ImageCover::Center)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOX: (f32, f32) = (800.0, 600.0);

    #[test]
    fn a_small_picture_is_not_stretched_and_a_large_one_fits() {
        assert_eq!(fit_scale((320.0, 200.0), 0, BOX), 1.0);
        assert_eq!(fit_scale((1600.0, 1200.0), 0, BOX), 0.5);
        // Turned on its side, a wide picture is tall.
        assert_eq!(fit_scale((1600.0, 400.0), 1, BOX), 0.375);
        assert_eq!(fit_scale((0.0, 0.0), 0, BOX), 1.0);
    }

    #[test]
    fn the_percentage_is_the_pictures_own_scale() {
        let view = View::default();
        assert_eq!(view.percent(Some((1600.0, 1200.0)), BOX).as_deref(), Some("50%"));
        assert_eq!(view.percent(Some((320.0, 200.0)), BOX).as_deref(), Some("100%"));
        assert_eq!(view.percent(None, BOX), None);
    }

    #[test]
    fn actual_size_is_the_zoom_that_undoes_the_fit() {
        let zoom = View::actual_zoom(Some((1600.0, 1200.0)), 0, BOX).unwrap();
        assert_eq!(zoom, 2.0);
        let at_actual = View { zoom, ..View::default() };
        assert_eq!(at_actual.scale((1600.0, 1200.0), BOX), 1.0);
        assert_eq!(View::actual_zoom(None, 0, BOX), None);
    }

    #[test]
    fn zoom_keeps_the_point_under_the_pointer() {
        let natural = Some((1600.0, 1200.0));
        let view = View::default().zoom_at(2.0, (100.0, 0.0), natural, BOX);
        assert_eq!(view.zoom, 2.0);
        // The point 100 right of the middle stays 100 right of the middle:
        // the picture moved 100 left, which the clamp allows (400 of slack).
        assert_eq!(view.pan, (-100.0, 0.0));
    }

    #[test]
    fn the_pan_stays_inside_the_picture() {
        let natural = Some((1600.0, 1200.0));
        // At fit the picture fills the box: nothing to pan.
        let fit = View { pan: (50.0, 50.0), ..View::default() }.clamped(natural, BOX);
        assert_eq!(fit.pan, (0.0, 0.0));
        // At 4x it is 3200 wide: 1200 of slack each side.
        let zoomed = View { zoom: 4.0, pan: (5000.0, -5000.0), turns: 0 }.clamped(natural, BOX);
        assert_eq!(zoomed.pan, (1200.0, -900.0));
        // A small picture stays centred.
        let small = View { pan: (30.0, 30.0), ..View::default() }.clamped(Some((100.0, 100.0)), BOX);
        assert_eq!(small.pan, (0.0, 0.0));
    }

    #[test]
    fn zoom_is_limited() {
        let view = View::default().zoom_at(1000.0, (0.0, 0.0), None, BOX);
        assert_eq!(view.zoom, MAX_ZOOM);
        let view = View::default().zoom_at(0.0001, (0.0, 0.0), None, BOX);
        assert_eq!(view.zoom, MIN_ZOOM);
    }

    #[test]
    fn a_turn_refits_and_four_turns_come_round() {
        let natural = Some((1600.0, 400.0));
        let mut view = View { zoom: 3.0, pan: (10.0, 10.0), turns: 0 };
        view = view.rotated(true, natural, BOX);
        assert_eq!((view.turns, view.zoom, view.pan), (1, 1.0, (0.0, 0.0)));
        for _ in 0..3 {
            view = view.rotated(true, natural, BOX);
        }
        assert_eq!(view.turns, 0);
        assert_eq!(view.rotated(false, natural, BOX).turns, 3);
    }

    #[test]
    fn keys_zoom_fit_and_turn() {
        let key = |text: &str| route(&Key::Character(text.to_owned().into()), Modifiers::empty());
        assert_eq!(key("+"), Some(Action::ZoomIn));
        assert_eq!(key("="), Some(Action::ZoomIn));
        assert_eq!(key("-"), Some(Action::ZoomOut));
        assert_eq!(key("0"), Some(Action::Fit));
        assert_eq!(key("1"), Some(Action::Actual));
        assert_eq!(key("r"), Some(Action::Rotate(true)));
        assert_eq!(key("R"), Some(Action::Rotate(false)));
        assert_eq!(key("x"), None);
        assert_eq!(route(&Key::Character("r".to_owned().into()), Modifiers::CONTROL), None);
    }
}
