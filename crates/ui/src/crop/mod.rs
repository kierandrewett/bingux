//! A crop box over a picture: handles, aspect presets and keys.
//!
//! [`CropOverlay`] is the box a person drags over content to say which part
//! to keep. It owns the pointer and the keyboard of that gesture and nothing
//! about what is cropped: the owner says how big the content is
//! ([`use_crop`]), draws the content itself (a [`ZoomView`](crate::zoom_view::ZoomView)
//! at fit, a bitmap in a dialog), and reads the result from the
//! [`CropHandle`]. Preview crops pictures with it, the Settings avatar
//! picker crops a square with it, and Media can crop a cover or a frame.
//!
//! ```ignore
//! use bingux_ui::crop::{CropBar, CropOverlay, use_crop};
//!
//! let crop = use_crop((4000.0, 3000.0));          // the content's own pixels
//! rect()
//!     .child(CropOverlay::new(crop).content(picture_at_fit).on_commit(done).on_cancel(back))
//!     .child(CropBar::new(crop).on_commit(done).on_cancel(back))
//! // later: crop.pixels() is (left, top, width, height) in content pixels
//! ```
//!
//! The overlay fills its parent, draws the content it is given inside a margin
//! (room for the handles) and puts the box over the same place a `ZoomView` at
//! fit puts the content (centred, never enlarged), so the two agree about where
//! the picture is. Everything outside the box is dimmed.
//!
//! # What a person can do
//!
//! | Input | Effect |
//! | ----- | ------ |
//! | drag inside the box | move it |
//! | drag a corner or an edge handle | resize it, keeping the shape when an aspect is chosen |
//! | arrows | move the box by a pixel of the screen (a content pixel when zoomed out far) |
//! | Shift and arrows | move ten times as far |
//! | Alt and arrows | resize from the bottom right corner: Right and Down grow it, Left and Up shrink it |
//! | Ctrl+A, Home | the whole content again |
//! | Enter, Escape | the owner's commit and cancel |
//! | the aspect list, the turn button of [`CropBar`] | pick a shape (Free, Original, 1:1, 4:3, 3:2, 16:9...) and turn it on its side |
//!
//! The box is one Tab stop with the library's focus ring; the handles are for
//! the pointer and fingers (their targets are 44 px at compact density), and
//! every one of them has a key equivalent above. Reduced motion changes
//! nothing: the box follows the pointer without animation.
//!
//! What can go wrong with the geometry, and the test that holds each, is
//! listed in [`geometry`].

mod geometry;

use freya::prelude::*;

pub use geometry::{Aspect, CropRect, Grip, MIN_SIDE, drag, fit_ratio, nudge, resize};

use crate::controls::capture::{Capture, CaptureEvent, Phase, use_capture};
use crate::component::ComponentName;
use crate::controls::{action_button, dropdown, icon_button};
use crate::icons::Icon;
use crate::motion::Focusable;
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{CAPTION, Ink};
use crate::zoom_view::fit_scale;

/// The width of the thirds grid's lines, in logical pixels.
const GRID_LINE: f32 = 1.0;

/// A pointer that took a handle: what it took and where the box and the
/// pointer were then.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Held {
    grip: Grip,
    start: CropRect,
    from: (f32, f32),
}

/// What a crop is, reachable from outside the overlay: the box, the shape it
/// is held to, and the size of the content. It is `Copy`; make one with
/// [`use_crop`].
#[derive(Clone, Copy, PartialEq)]
pub struct CropHandle {
    rect: State<CropRect>,
    aspect: State<Aspect>,
    size: State<(f32, f32)>,
    held: State<Option<Held>>,
}

/// A [`CropHandle`] for content of `size` pixels (width, height). Call it from
/// `render`. When `size` changes (another picture, a turned one) the box is the
/// whole content again; the shape stays.
pub fn use_crop(size: (f32, f32)) -> CropHandle {
    let rect = use_state(|| CropRect::full(size));
    let aspect = use_state(|| Aspect::Free);
    let content = use_state(|| size);
    let held = use_state(|| None::<Held>);
    {
        let (mut rect, mut content, mut held) = (rect, content, held);
        use_side_effect_with_deps(
            &(size.0.to_bits(), size.1.to_bits()),
            move |_: &(u32, u32)| {
                if *content.peek() == size {
                    return;
                }
                content.set(size);
                held.set(None);
                let ratio = aspect.peek().ratio(size);
                let whole = CropRect::full(size);
                rect.set(match ratio {
                    Some(ratio) => fit_ratio(whole, ratio, size),
                    None => whole,
                });
            },
        );
    }
    CropHandle {
        rect,
        aspect,
        size: content,
        held,
    }
}

impl CropHandle {
    /// The box, in the content's pixels, kept inside the content.
    pub fn rect(&self) -> CropRect {
        let size = *self.size.read();
        let rect = *self.rect.read();
        nudge(rect, (0.0, 0.0), size)
    }

    /// The size of the content.
    pub fn size(&self) -> (f32, f32) {
        *self.size.read()
    }

    /// The shape the box is held to.
    pub fn aspect(&self) -> Aspect {
        *self.aspect.read()
    }

    /// The box in whole pixels of the content: left, top, width, height.
    pub fn pixels(&self) -> (u32, u32, u32, u32) {
        self.rect().pixels(self.size())
    }

    /// The box as fractions of the content: left, top, width, height.
    pub fn fractions(&self) -> (f32, f32, f32, f32) {
        self.rect().fractions(self.size())
    }

    /// Whether nothing is cut off.
    pub fn is_full(&self) -> bool {
        self.rect().is_full(self.size())
    }

    /// Hold the box to `aspect`; the box is reshaped to it at once.
    pub fn set_aspect(&self, aspect: Aspect) {
        let (mut state, mut rect) = (self.aspect, self.rect);
        let size = *self.size.peek();
        state.set_if_modified(aspect);
        if let Some(ratio) = aspect.ratio(size) {
            let next = fit_ratio(*rect.peek(), ratio, size);
            rect.set_if_modified(next);
        }
    }

    /// The shape on its side: 4:3 becomes 3:4.
    pub fn swap_aspect(&self) {
        self.set_aspect(self.aspect.peek().swapped());
    }

    /// The whole content again, in the shape the box is held to.
    pub fn reset(&self) {
        let size = *self.size.peek();
        let mut rect = self.rect;
        let whole = CropRect::full(size);
        rect.set_if_modified(match self.aspect.peek().ratio(size) {
            Some(ratio) => fit_ratio(whole, ratio, size),
            None => whole,
        });
    }

    /// Put the box at `rect`, kept inside the content.
    pub fn set_rect(&self, rect: CropRect) {
        let size = *self.size.peek();
        let mut state = self.rect;
        state.set_if_modified(nudge(rect, (0.0, 0.0), size));
    }

    /// Whether a pointer holds a handle now.
    pub fn is_dragging(&self) -> bool {
        self.held.read().is_some()
    }

    /// The reports of a pointer that took `grip`, for the overlay's captures.
    fn drive(&self, grip: Grip, event: CaptureEvent, scale: f32) {
        let (mut held, mut rect) = (self.held, self.rect);
        match event.phase {
            Phase::Start => held.set(Some(Held {
                grip,
                start: *rect.peek(),
                from: event.at,
            })),
            Phase::Move | Phase::End => {
                if let Some(taken) = *held.peek()
                    && taken.grip == grip
                    && scale > 0.0
                {
                    let size = *self.size.peek();
                    let delta = (
                        (event.at.0 - taken.from.0) / scale,
                        (event.at.1 - taken.from.1) / scale,
                    );
                    let ratio = self.aspect.peek().ratio(size);
                    rect.set_if_modified(drag(taken.start, grip, delta, size, ratio));
                }
                if event.phase == Phase::End {
                    held.set(None);
                }
            }
            Phase::Cancel => held.set(None),
        }
    }
}

/// What the guide inside the box draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Guide {
    /// Lines at thirds, while the box is held or has the keyboard.
    Thirds,
    /// A circle inside the box, for a picture shown round (an avatar).
    Circle,
    /// Nothing.
    None,
}

/// The crop box over content, with its handles. See the [module](self).
///
/// Give it the content with [`content`](Self::content): the overlay fills its
/// parent, draws the content in it, fitted inside a margin that leaves room for
/// the handles, and lays the box over it.
pub struct CropOverlay {
    handle: CropHandle,
    content: Option<Element>,
    guide: Guide,
    autofocus: bool,
    label: Option<String>,
    on_commit: Option<EventHandler<()>>,
    on_cancel: Option<EventHandler<()>>,
}

impl CropOverlay {
    /// The overlay of `handle`'s crop.
    pub fn new(handle: CropHandle) -> Self {
        CropOverlay {
            handle,
            content: None,
            guide: Guide::Thirds,
            autofocus: true,
            label: None,
            on_commit: None,
            on_cancel: None,
        }
    }

    /// The content the box is over, drawn fitted into the overlay's box the way
    /// a [`ZoomView`](crate::zoom_view::ZoomView) at fit draws it.
    pub fn content(mut self, content: impl Into<Element>) -> Self {
        self.content = Some(content.into());
        self
    }

    /// What is drawn inside the box (thirds by default).
    pub fn guide(mut self, guide: Guide) -> Self {
        self.guide = guide;
        self
    }

    /// Whether the box takes the keyboard when it appears (on by default).
    pub fn autofocus(mut self, autofocus: bool) -> Self {
        self.autofocus = autofocus;
        self
    }

    /// What a screen reader says for the box; "Crop area" by default.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Enter in the box: the owner takes the crop.
    pub fn on_commit(mut self, handler: impl Into<EventHandler<()>>) -> Self {
        self.on_commit = Some(handler.into());
        self
    }

    /// Escape in the box: the owner drops the crop.
    pub fn on_cancel(mut self, handler: impl Into<EventHandler<()>>) -> Self {
        self.on_cancel = Some(handler.into());
        self
    }
}

impl PartialEq for CropOverlay {
    /// Never equal: the content is an element, so the overlay is drawn again
    /// whenever its owner is.
    fn eq(&self, _: &Self) -> bool {
        false
    }
}

fn cursor_of(grip: Grip) -> CursorIcon {
    match grip {
        Grip::Move => CursorIcon::Move,
        Grip::North | Grip::South => CursorIcon::NsResize,
        Grip::East | Grip::West => CursorIcon::EwResize,
        Grip::NorthEast | Grip::SouthWest => CursorIcon::NeswResize,
        Grip::NorthWest | Grip::SouthEast => CursorIcon::NwseResize,
    }
}

impl Component for CropOverlay {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let handle = self.handle;
        let mut area = use_state(Area::default);
        let mut hovering = use_state(|| false);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);

        let size = handle.size();
        let crop = handle.rect();
        let (box_width, box_height) = (area.read().width(), area.read().height());
        // The content sits inside a margin the width of half a handle, so the handles
        // on the content's edges are not cut off by the overlay's own edge.
        let margin = S4.px();
        let room = ((box_width - 2.0 * margin).max(0.0), (box_height - 2.0 * margin).max(0.0));
        let scale = fit_scale(size, 0, room);
        let (frame_width, frame_height) = (size.0 * scale, size.1 * scale);
        let (frame_left, frame_top) = (
            margin + (room.0 - frame_width) / 2.0,
            margin + (room.1 - frame_height) / 2.0,
        );
        let (left, top) = (frame_left + crop.x * scale, frame_top + crop.y * scale);
        let (width, height) = (crop.width * scale, crop.height * scale);
        let dragging = handle.is_dragging();

        // One capture for each part of the box a pointer can take. The count
        // never changes, so the hooks are in the same order every render.
        let all = [
            Grip::Move,
            Grip::NorthWest,
            Grip::NorthEast,
            Grip::SouthEast,
            Grip::SouthWest,
            Grip::North,
            Grip::East,
            Grip::South,
            Grip::West,
        ];
        let captures: Vec<(Grip, Capture)> = all
            .into_iter()
            .map(|grip| {
                let capture = use_capture(move |event: CaptureEvent| {
                    if event.phase == Phase::Start {
                        a11y_id.request_focus();
                    }
                    handle.drive(grip, event, scale);
                });
                (grip, capture)
            })
            .collect();
        let capture_of = |wanted: Grip| -> Capture {
            captures
                .iter()
                .find(|(grip, _)| *grip == wanted)
                .map(|(_, capture)| capture.clone())
                .expect("every grip has a capture")
        };

        let on_commit = self.on_commit.clone();
        let on_cancel = self.on_cancel.clone();
        let on_key = move |event: Event<KeyboardEventData>| {
            let shift = event.modifiers.contains(Modifiers::SHIFT);
            let alt = event.modifiers.contains(Modifiers::ALT);
            let ctrl = event.modifiers.contains(Modifiers::CONTROL);
            let size = handle.size();
            let step = (1.0 / scale.max(f32::EPSILON)).max(1.0) * if shift { 10.0 } else { 1.0 };
            let delta = match event.key {
                Key::Named(NamedKey::ArrowLeft) => Some((-step, 0.0)),
                Key::Named(NamedKey::ArrowRight) => Some((step, 0.0)),
                Key::Named(NamedKey::ArrowUp) => Some((0.0, -step)),
                Key::Named(NamedKey::ArrowDown) => Some((0.0, step)),
                _ => None,
            };
            let handled = if let Some(delta) = delta {
                let now = *handle.rect.peek();
                let next = if alt {
                    resize(now, delta, size, handle.aspect.peek().ratio(size))
                } else {
                    nudge(now, delta, size)
                };
                let mut state = handle.rect;
                state.set_if_modified(next);
                true
            } else {
                match &event.key {
                    Key::Named(NamedKey::Home) => {
                        handle.reset();
                        true
                    }
                    Key::Character(text) if ctrl && text.eq_ignore_ascii_case("a") => {
                        handle.reset();
                        true
                    }
                    Key::Named(NamedKey::Enter) => match &on_commit {
                        Some(commit) => {
                            commit.call(());
                            true
                        }
                        None => false,
                    },
                    Key::Named(NamedKey::Escape) => match &on_cancel {
                        Some(cancel) => {
                            cancel.call(());
                            true
                        }
                        None => false,
                    },
                    _ => false,
                }
            };
            if handled {
                event.stop_propagation();
            }
        };

        let absolute = |left: f32, top: f32, width: f32, height: f32| {
            rect()
                .position(Position::new_absolute().left(left).top(top))
                .width(Size::px(width.max(0.0)))
                .height(Size::px(height.max(0.0)))
        };
        let dim = |left: f32, top: f32, width: f32, height: f32| {
            absolute(left, top, width, height).tw(&tokens, &[bg(Paint::Scrim)])
        };

        let label = self.label.clone().unwrap_or_else(|| Grip::Move.name().to_owned());
        let (pixel_width, pixel_height) = {
            let (_, _, w, h) = handle.pixels();
            (w, h)
        };
        let lit = dragging || focus().is_focused() || *hovering.read();
        let guide = match self.guide {
            Guide::Thirds if lit => {
                let line = |left: f32, top: f32, width: f32, height: f32| {
                    absolute(left, top, width, height).tw(&tokens, &[bg(Paint::Knob)]).opacity(0.5)
                };
                Some(
                    rect()
                        .position(Position::new_absolute().left(0.0).top(0.0))
                        .width(Size::px(width))
                        .height(Size::px(height))
                        .child(line(width / 3.0, 0.0, GRID_LINE, height))
                        .child(line(width * 2.0 / 3.0, 0.0, GRID_LINE, height))
                        .child(line(0.0, height / 3.0, width, GRID_LINE))
                        .child(line(0.0, height * 2.0 / 3.0, width, GRID_LINE)),
                )
            }
            Guide::Circle => {
                let round = width.min(height);
                Some(
                    rect()
                        .position(Position::new_absolute().left(0.0).top(0.0))
                        .width(Size::px(width))
                        .height(Size::px(height))
                        .tw(&tokens, &[flex_row(), center()])
                        .child(
                            rect()
                                .width(Size::px(round))
                                .height(Size::px(round))
                                .tw(
                                    &tokens,
                                    &[rounded(Radius::Pill), border(Line::Edge(Paint::Knob))],
                                ),
                        ),
                )
            }
            _ => None,
        };

        let face = rect()
            .width(Size::px(width))
            .height(Size::px(height))
            .tw(&tokens, &[border(Line::Edge(Paint::Knob))])
            .focusable_inside(focus());
        let body = capture_of(Grip::Move)
            .handlers(
                absolute(left, top, width, height)
                    .component("CropArea")
                    .a11y_id(a11y_id)
                    .a11y_focusable(true)
                    .a11y_role(AccessibilityRole::Group)
                    .a11y_alt(format!("{label}, {pixel_width} by {pixel_height} pixels"))
                    .maybe(self.autofocus, |element| element.a11y_auto_focus(true))
                    .cursor(if dragging { CursorIcon::Grabbing } else { cursor_of(Grip::Move) })
                    .on_pointer_enter(move |_| hovering.set_if_modified(true))
                    .on_pointer_leave(move |_| hovering.set_if_modified(false))
                    .on_key_down(on_key),
            )
            .child(face)
            .maybe_child(guide);

        // Handles: corners always, edges when the side has room for them.
        let hit = Dim::PickerHandle.px();
        let knob = Dim::SliderThumb.px();
        let handles = Grip::HANDLES
            .into_iter()
            .filter(|grip| {
                let (x, y) = grip.anchor();
                let corner = x != 0.5 && y != 0.5;
                corner || (if x == 0.5 { width } else { height }) >= hit * 3.0
            })
            .map(|grip| {
                let (ax, ay) = grip.anchor();
                let (centre_x, centre_y) = (left + ax * width, top + ay * height);
                capture_of(grip)
                    .handlers(
                        absolute(centre_x - hit / 2.0, centre_y - hit / 2.0, hit, hit)
                            .component("CropHandle")
                            .tw(&tokens, &[bg(Paint::Shield), flex_row(), center()])
                            .cursor(cursor_of(grip))
                            .a11y_alt(grip.name()),
                    )
                    .child(
                        rect()
                            .width(Size::px(knob))
                            .height(Size::px(knob))
                            .tw(
                                &tokens,
                                &[
                                    rounded(Radius::Pill),
                                    bg(Paint::Knob),
                                    shadow(Elevation::Knob),
                                ],
                            ),
                    )
                    .into_element()
            });

        // The layer is a box of the overlay's own measured size, because an
        // absolute child cannot ask its parent for a percentage of a size that
        // is only known once the parent is laid out.
        let layer = absolute(0.0, 0.0, box_width, box_height)
            .layer(crate::layers::CROP_BOX)
            .tw(&tokens, &[bg(Paint::Shield), clip()])
            .child(dim(frame_left, frame_top, frame_width, crop.y * scale))
            .child(dim(
                frame_left,
                top + height,
                frame_width,
                frame_height - crop.y * scale - height,
            ))
            .child(dim(frame_left, top, crop.x * scale, height))
            .child(dim(
                left + width,
                top,
                frame_width - crop.x * scale - width,
                height,
            ))
            .child(body)
            .children(handles);
        rect()
            .component("CropOverlay")
            .tw(&tokens, &[w_full(), h_full(), bg(Paint::Recess)])
            .on_sized(move |event: Event<SizedEventData>| {
                if area.peek().size != event.area.size {
                    area.set(event.area);
                }
            })
            .maybe_child(self.content.clone().map(|content| {
                rect()
                    .tw(&tokens, &[w_full(), h_full(), p(S4)])
                    .child(content)
            }))
            .child(layer)
    }
}

/// The bar that goes with a [`CropOverlay`]: the shape list, the turn button,
/// the size of the box, Cancel and the owner's confirming button. It is a
/// floating card; the owner puts it where it wants (the bottom middle).
#[derive(PartialEq)]
pub struct CropBar {
    handle: CropHandle,
    aspects: Vec<Aspect>,
    commit_label: String,
    cancel_label: String,
    on_commit: Option<EventHandler<()>>,
    on_cancel: Option<EventHandler<()>>,
}

impl CropBar {
    pub fn new(handle: CropHandle) -> Self {
        CropBar {
            handle,
            aspects: Aspect::PRESETS.to_vec(),
            commit_label: "Done".to_owned(),
            cancel_label: "Cancel".to_owned(),
            on_commit: None,
            on_cancel: None,
        }
    }

    /// The shapes the list offers (the presets by default). An avatar offers
    /// `[Aspect::SQUARE]` and the list is left out.
    pub fn aspects(mut self, aspects: impl Into<Vec<Aspect>>) -> Self {
        self.aspects = aspects.into();
        self
    }

    /// The confirming button's text: a verb that names the result ("Save a copy").
    pub fn commit_label(mut self, label: impl Into<String>) -> Self {
        self.commit_label = label.into();
        self
    }

    pub fn cancel_label(mut self, label: impl Into<String>) -> Self {
        self.cancel_label = label.into();
        self
    }

    pub fn on_commit(mut self, handler: impl Into<EventHandler<()>>) -> Self {
        self.on_commit = Some(handler.into());
        self
    }

    pub fn on_cancel(mut self, handler: impl Into<EventHandler<()>>) -> Self {
        self.on_cancel = Some(handler.into());
        self
    }
}

impl Component for CropBar {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let handle = self.handle;
        let aspect = handle.aspect();
        let (_, _, pixel_width, pixel_height) = handle.pixels();
        let choices = self.aspects.len() > 1;
        let on_commit = self.on_commit.clone();
        let on_cancel = self.on_cancel.clone();
        let options: Vec<(String, String)> = self
            .aspects
            .iter()
            .map(|aspect| (aspect.id(), aspect.label()))
            .collect();
        // A shape that is not in the list (a turned one) still reads in the button.
        let selected = aspect.id();
        let mut options = options;
        if !options.iter().any(|(id, _)| *id == selected) {
            options.push((selected.clone(), aspect.label()));
        }
        rect()
            .component("CropBar")
            .tw(
                &tokens,
                &[
                    flex_row(),
                    wrap(S2),
                    items(Align::Center),
                    justify(Align::Center),
                    gap(S2),
                    p(S2),
                    rounded(Radius::Card),
                    bg(Paint::Popover),
                    shadow(Elevation::Raised),
                ],
            )
            // On a phone the bar is as wide as the window and wraps; on a desktop it hugs its content.
            .maybe(is_compact(), |bar| bar.tw(&tokens, &[w_full()]))
            .maybe_child(choices.then(|| {
                dropdown(options, selected, move |id: String| {
                    if let Some(aspect) = Aspect::from_id(&id) {
                        handle.set_aspect(aspect);
                    }
                })
                .dense()
                .min_width(Dim::Box24)
            }))
            .maybe_child((choices && aspect.can_swap()).then(|| {
                icon_button(Icon::ObjectRotateRight, move |_| handle.swap_aspect())
                    .tooltip("Turn the shape on its side")
            }))
            .child(
                CAPTION
                    .in_ink(Ink::Muted)
                    .label(&tokens, format!("{pixel_width} \u{d7} {pixel_height}"))
                    .max_lines(1),
            )
            .maybe_child(self.on_cancel.is_some().then(|| {
                action_button(self.cancel_label.clone(), move |_| {
                    if let Some(cancel) = &on_cancel {
                        cancel.call(());
                    }
                })
                .flat()
            }))
            .maybe_child(self.on_commit.is_some().then(|| {
                action_button(self.commit_label.clone(), move |_| {
                    if let Some(commit) = &on_commit {
                        commit.call(());
                    }
                })
                .suggested()
            }))
    }
}
