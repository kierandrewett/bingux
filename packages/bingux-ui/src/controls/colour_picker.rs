//! A colour picker: a saturation and value square, hue and alpha strips, a row
//! of theme swatches and a hex field.
//!
//! The picker edits a colour held as hue, saturation, value and alpha
//! ([`Hsva`]) and reports each change to `on_change` as the RGB colour it
//! makes. The caller passes the colour back as `value`. A `value` that is what
//! the picker just reported changes nothing; any other `value` (the caller set
//! it, a swatch or the hex field made it) is taken in as an edit of the colour
//! on screen, so a grey keeps its hue and a black its saturation.
//!
//! The square, the hue strip and the alpha strip are [`Capture`]s: a press on
//! one takes the pointer until the button comes up, wherever the pointer goes,
//! and the thumb is always where the pointer is, clamped to the control. They
//! take the arrow keys too: one percent a press (one degree on the hue strip),
//! ten with Shift, Home and End for the ends of a strip.
//!
//! [`Capture`]: super::capture::Capture

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use freya::prelude::*;

use super::capture::{CaptureEvent, Phase, Point, fraction, use_capture};
use super::focus_stop::focus_stop;
use super::hsva::{HUE_MAX, Hsva};
use super::text_field::text_field;
use crate::component::ComponentName;
use crate::motion::Focusable;
use crate::style::*;
use crate::surface::use_tokens;
use crate::theme::Tokens;

/// Width of the saturation square and the strips, in pixels.
const PANEL_W: f32 = 200.0;
/// The picker's outer width: the panel plus the card's padding on each side.
pub const WIDTH: f32 = PANEL_W + 2.0 * 8.0;
/// Height of the saturation square, in pixels.
const SQUARE_H: f32 = 132.0;
/// Height of the hue and alpha strips, in pixels.
const STRIP_H: f32 = 14.0;
/// Edge length of one checker cell behind the alpha strip.
const CHECK: f32 = 7.0;
/// A key press moves a thumb this fraction of its range, except on the hue
/// strip, where it is one degree.
const KEY_STEP: f32 = 0.01;
/// Shift, Page Up and Page Down make a key press this many steps.
const BIG: f32 = 10.0;

/// The picker's full height, for placement only: its padding, the square, two
/// strips, a swatch row and a control-height hex field, with a gap between
/// each of the five parts. The card itself sizes to its children.
pub fn height() -> f32 {
    6.0 * S2.px() + SQUARE_H + 2.0 * STRIP_H + Dim::Swatch.px() + Dim::Control.px()
}

/// Theme colours offered as one-click swatches, as hex text.
pub const SWATCHES: &[&str] = &[
    "#1c1c1e", "#3a3a3c", "#8e8e93", "#ffffff", "#0a84ff", "#30d158", "#ff9f0a", "#ff453a",
];

/// The picker for `value`; `on_change` receives every new colour as it is
/// chosen, at most once per frame while a thumb is dragged.
#[derive(PartialEq)]
pub struct ColourPicker {
    value: Color,
    on_change: EventHandler<Color>,
}

/// A colour picker for `value`.
pub fn colour_picker(value: Color, on_change: impl Into<EventHandler<Color>>) -> ColourPicker {
    ColourPicker {
        value,
        on_change: on_change.into(),
    }
}

/// What the parts of the picker share: the colour being edited and the way
/// out. Equal to another link of the same picker, so a part whose own values
/// have not changed is not drawn again when its neighbour is.
#[derive(Clone)]
struct Link {
    /// The colour being edited, in the parts the picker edits it in.
    model: State<Hsva>,
    /// The caller's newest `on_change`.
    out: Rc<RefCell<EventHandler<Color>>>,
    /// Whether a drag is on: the caller's `value` is not taken in meanwhile.
    held: Rc<Cell<bool>>,
}

impl PartialEq for Link {
    fn eq(&self, other: &Self) -> bool {
        self.model == other.model
            && Rc::ptr_eq(&self.out, &other.out)
            && Rc::ptr_eq(&self.held, &other.held)
    }
}

impl Link {
    /// The colour being edited, without subscribing to it.
    fn current(&self) -> Hsva {
        *self.model.peek()
    }

    /// Edits the colour, and reports it when the RGB colour is a new one.
    fn set(&self, next: Hsva) {
        let before = self.current();
        if before == next {
            return;
        }
        let mut model = self.model;
        model.set(next);
        if before.colour() != next.colour() {
            let out = self.out.borrow().clone();
            out.call(next.colour());
        }
    }

    /// Edits the colour to `colour`, keeping what it cannot say.
    fn set_colour(&self, colour: Color) {
        self.set(self.current().editing(colour));
    }

    /// The caller's `value` is not what is being edited: take it in. A drag
    /// in progress is the truth meanwhile; the caller's echo of it is late.
    fn rebase(&self, value: Color) {
        if self.held.get() {
            return;
        }
        let current = self.current();
        if current.colour() != value {
            let mut model = self.model;
            model.set(current.editing(value));
        }
    }

    /// The colour to draw for the caller's `value`: the colour being edited,
    /// or `value` taken in when it is not that. Needs no state write, so no
    /// frame shows the old colour.
    fn shown(&self, value: Color) -> Hsva {
        let current = *self.model.read();
        if self.held.get() || current.colour() == value {
            current
        } else {
            current.editing(value)
        }
    }

    /// Notes a drag report: a drag is on from its start until its end.
    fn grab(&self, phase: Phase) {
        self.held.set(matches!(phase, Phase::Start | Phase::Move));
    }
}

impl Component for ColourPicker {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let model = use_state(|| Hsva::from_colour(self.value));
        let out = use_hook(|| Rc::new(RefCell::new(self.on_change.clone())));
        let held = use_hook(|| Rc::new(Cell::new(false)));
        *out.borrow_mut() = self.on_change.clone();
        let link = Link { model, out, held };
        let rebasing = link.clone();
        use_side_effect_with_deps(&self.value, move |value: &Color| rebasing.rebase(*value));
        let shown = link.shown(self.value);

        rect()
            .component("ColourPicker")
            .width(Size::px(WIDTH))
            .tw(&tokens, &[flex_col(), gap(S2), p(S2)])
            .child(SvSquare {
                hue: shown.hue,
                sat: shown.sat,
                val: shown.val,
                link: link.clone(),
            })
            .child(Strip {
                kind: StripKind::Hue,
                fraction: StripKind::Hue.fraction(shown),
                colour: shown.hue_colour(),
                link: link.clone(),
            })
            .child(Strip {
                kind: StripKind::Alpha,
                fraction: StripKind::Alpha.fraction(shown),
                colour: shown.opaque(),
                link: link.clone(),
            })
            .child(Swatches {
                value: shown.colour(),
                link: link.clone(),
            })
            .child(HexField {
                colour: shown.colour(),
                link,
            })
    }
}

// ---- Pointer and key maps (pure) -----------------------------------------------

/// The saturation and value under a pointer at `local`, in the square's own
/// coordinates. The thumb's centre travels inside the square by the thumb's
/// radius, so a pointer within that margin of an edge, or beyond it, is the
/// edge: pure white, pure black and the pure hue are easy to reach.
fn square_at(local: Point, thumb: f32) -> (f32, f32) {
    let radius = thumb / 2.0;
    (
        fraction(local.0, radius, PANEL_W - thumb),
        1.0 - fraction(local.1, radius, SQUARE_H - thumb),
    )
}

/// How far along a strip a pointer at `local` is, from 0 to 1.
fn strip_at(local: Point, thumb: f32) -> f32 {
    fraction(local.0, thumb / 2.0, PANEL_W - thumb)
}

/// What a key does to a thumb.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Nudge {
    /// Steps along each axis; right and up are positive. Shift and the page
    /// keys make them [`BIG`].
    By { x: f32, y: f32 },
    /// To the start of the range.
    Start,
    /// To the end of the range.
    End,
}

fn nudge(key: &Key, modifiers: Modifiers) -> Option<Nudge> {
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

/// The colour after `nudge` on the square: left and right move the
/// saturation, up and down the value. The square has no start or end.
fn nudged_square(current: Hsva, nudge: Nudge) -> Option<Hsva> {
    let Nudge::By { x, y } = nudge else {
        return None;
    };
    Some(current.with_sv(current.sat + x * KEY_STEP, current.val + y * KEY_STEP))
}

/// The two strips: what they edit and how.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StripKind {
    Hue,
    Alpha,
}

impl StripKind {
    /// Where along the strip `colour`'s part is, from 0 to 1.
    fn fraction(self, colour: Hsva) -> f32 {
        match self {
            StripKind::Hue => colour.hue / HUE_MAX,
            StripKind::Alpha => colour.alpha as f32 / 255.0,
        }
    }

    /// `colour` with its part at `fraction` of the strip.
    fn apply(self, colour: Hsva, fraction: f32) -> Hsva {
        match self {
            StripKind::Hue => colour.with_hue(fraction * HUE_MAX),
            StripKind::Alpha => {
                colour.with_alpha((fraction * 255.0).round().clamp(0.0, 255.0) as u8)
            }
        }
    }

    /// The fraction one key step moves: a degree, or a percent.
    fn step(self) -> f32 {
        match self {
            StripKind::Hue => 1.0 / HUE_MAX,
            StripKind::Alpha => KEY_STEP,
        }
    }

    fn label(self) -> &'static str {
        match self {
            StripKind::Hue => "Hue",
            StripKind::Alpha => "Opacity",
        }
    }

    /// The value assistive technology reads at `fraction`, and the largest.
    fn reading(self, fraction: f32) -> (f64, f64) {
        match self {
            StripKind::Hue => ((fraction * HUE_MAX) as f64, HUE_MAX as f64),
            StripKind::Alpha => ((fraction * 100.0).round() as f64, 100.0),
        }
    }
}

/// The colour after `nudge` on a strip.
fn nudged_strip(kind: StripKind, current: Hsva, nudge: Nudge) -> Hsva {
    let at = kind.fraction(current);
    let to = match nudge {
        Nudge::By { x, y } => at + (x + y) * kind.step(),
        Nudge::Start => 0.0,
        Nudge::End => 1.0,
    };
    kind.apply(current, to.clamp(0.0, 1.0))
}

// ---- The square ----------------------------------------------------------------

/// Saturation across, value down. The square is the hue at full saturation
/// and value, overlaid with white fading to the right and black fading down.
/// The thumb's centre travels inside the square by its radius, so the handle
/// never hangs off an edge.
#[derive(PartialEq)]
struct SvSquare {
    hue: f32,
    sat: f32,
    val: f32,
    link: Link,
}

impl Component for SvSquare {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let scale = *Platform::get().current_window().scale_factor.read();
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let link = self.link.clone();
        let capture = use_capture({
            let link = link.clone();
            move |event: CaptureEvent| {
                link.grab(event.phase);
                if event.phase == Phase::Start {
                    a11y_id.request_focus();
                }
                if event.phase != Phase::Cancel {
                    let (sat, val) = square_at(event.local, Dim::Thumb.px());
                    link.set(link.current().with_sv(sat, val));
                }
            }
        });
        let on_key = move |event: Event<KeyboardEventData>| {
            let Some(next) = nudge(&event.key, event.modifiers)
                .and_then(|nudge| nudged_square(link.current(), nudge))
            else {
                return;
            };
            event.stop_propagation();
            link.set(next);
        };

        let thumb = Dim::Thumb.px();
        let (travel_x, travel_y) = (PANEL_W - thumb, SQUARE_H - thumb);
        let thumb_x = snapped(self.sat * travel_x, scale);
        let thumb_y = snapped((1.0 - self.val) * travel_y, scale);
        let pure = Hsva::new(self.hue, 1.0, 1.0, 255).colour();
        let chosen = Hsva::new(self.hue, self.sat, self.val, 255).colour();
        // Pure white and pure black, not theme colours: the square is a colour
        // space, and the corner a pointer picks must be the corner it shows.
        // Freya's gradient `angle` is not CSS's: 270 runs left to right and 0
        // runs top to bottom.
        let white = colour_from_argb(255, 255, 255, 255);
        let black = colour_from_argb(255, 0, 0, 0);
        let to_right = LinearGradient::new()
            .angle(270.0)
            .stop(GradientStop::new(white, 0.0))
            .stop(GradientStop::new(white.with_a(0), 100.0));
        let to_bottom = LinearGradient::new()
            .angle(0.0)
            .stop(GradientStop::new(black.with_a(0), 0.0))
            .stop(GradientStop::new(black, 100.0));

        let (sat, val) = (
            (self.sat * 100.0).round() as i32,
            (self.val * 100.0).round() as i32,
        );
        let square = rect()
            .component("ColourSquare")
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Slider)
            .a11y_alt("Saturation and value")
            .a11y_builder(move |builder| {
                builder.set_value(format!("Saturation {sat}%, value {val}%"));
            })
            .width(Size::px(PANEL_W))
            .height(Size::px(SQUARE_H))
            .tw(&tokens, &[fill(pure), rounded(Radius::Mark), clip()])
            .on_key_down(on_key)
            // Layers are explicit: Freya paints siblings by depth as well as by
            // order, and the fades must lie over the pure hue and under the thumb.
            .child(gradient_fill(
                layer(0.0, PANEL_W, SQUARE_H).layer(Layer::Relative(1)),
                to_right,
            ))
            .child(gradient_fill(
                layer(0.0, PANEL_W, SQUARE_H).layer(Layer::Relative(2)),
                to_bottom,
            ))
            .child(thumb_handle(&tokens, thumb_x, thumb_y, chosen));
        capture.bind(square).focusable(focus())
    }
}

// ---- The strips ----------------------------------------------------------------

/// A horizontal strip: the hue from red through the spectrum back to red, or
/// the alpha from transparent to the colour at full opacity. The track runs
/// between the thumb's centres, inset by its radius at each end, so the handle
/// never hangs off the strip's edge.
#[derive(PartialEq)]
struct Strip {
    kind: StripKind,
    /// Where the thumb is along the strip, from 0 to 1.
    fraction: f32,
    /// The thumb's colour, and the alpha strip's solid end.
    colour: Color,
    link: Link,
}

impl Component for Strip {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let scale = *Platform::get().current_window().scale_factor.read();
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let kind = self.kind;
        let link = self.link.clone();
        let capture = use_capture({
            let link = link.clone();
            move |event: CaptureEvent| {
                link.grab(event.phase);
                if event.phase == Phase::Start {
                    a11y_id.request_focus();
                }
                if event.phase != Phase::Cancel {
                    let at = strip_at(event.local, Dim::Thumb.px());
                    link.set(kind.apply(link.current(), at));
                }
            }
        });
        let on_key = move |event: Event<KeyboardEventData>| {
            let Some(nudge) = nudge(&event.key, event.modifiers) else {
                return;
            };
            event.stop_propagation();
            link.set(nudged_strip(kind, link.current(), nudge));
        };

        let thumb = Dim::Thumb.px();
        let travel = PANEL_W - thumb;
        let thumb_x = snapped(self.fraction * travel, scale);
        let (reading, most) = kind.reading(self.fraction);
        // Freya's gradient `angle` is not CSS's: 270 runs left to right.
        let gradient = match kind {
            StripKind::Hue => (0..=6).fold(LinearGradient::new().angle(270.0), |gradient, step| {
                let stop = Hsva::new(step as f32 * 60.0, 1.0, 1.0, 255).colour();
                gradient.stop(GradientStop::new(stop, step as f32 * (100.0 / 6.0)))
            }),
            StripKind::Alpha => LinearGradient::new()
                .angle(270.0)
                .stop(GradientStop::new(self.colour.with_a(0), 0.0))
                .stop(GradientStop::new(self.colour.with_a(255), 100.0)),
        };
        let track =
            layer(thumb / 2.0, travel, STRIP_H).tw(&tokens, &[rounded(Radius::Pill), clip()]);
        let strip = rect()
            .component(match kind {
                StripKind::Hue => "ColourHue",
                StripKind::Alpha => "ColourAlpha",
            })
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Slider)
            .a11y_alt(kind.label())
            .a11y_builder(move |builder| {
                builder.set_numeric_value(reading);
                builder.set_min_numeric_value(0.0);
                builder.set_max_numeric_value(most);
            })
            .width(Size::px(PANEL_W))
            .height(Size::px(STRIP_H))
            .tw(&tokens, &[rounded(Radius::Pill)])
            .on_key_down(on_key)
            .maybe_child((kind == StripKind::Alpha).then_some(Checker))
            .child(gradient_fill(track.layer(Layer::Relative(2)), gradient))
            .child(thumb_handle(
                &tokens,
                thumb_x,
                (STRIP_H - thumb) / 2.0,
                self.colour.with_a(255),
            ));
        capture.bind(strip).focusable(focus())
    }
}

/// Light and dark cells behind the alpha strip, so a translucent colour shows
/// its alpha against a visible pattern. It never changes, so it is drawn once.
#[derive(PartialEq)]
struct Checker;

impl Component for Checker {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let thumb = Dim::Thumb.px();
        let width = PANEL_W - thumb;
        let light = Paint::Knob.color(&tokens);
        let dark = Paint::Control.color(&tokens);
        let cols = (width / CHECK).round() as usize;
        let rows = (STRIP_H / CHECK).round() as usize;
        let (cell_w, cell_h) = (width / cols as f32, STRIP_H / rows as f32);
        let mut track =
            layer(thumb / 2.0, width, STRIP_H).tw(&tokens, &[rounded(Radius::Pill), clip()]);
        for row in 0..rows {
            for col in 0..cols {
                let tone = if (row + col) % 2 == 0 { light } else { dark };
                track = track.child(
                    rect()
                        .position(
                            Position::new_absolute()
                                .left(col as f32 * cell_w)
                                .top(row as f32 * cell_h),
                        )
                        .width(Size::px(cell_w))
                        .height(Size::px(cell_h))
                        .tw(&tokens, &[fill(tone)]),
                );
            }
        }
        track
    }
}

// ---- Swatches and the hex field ------------------------------------------------

/// The row of theme swatches.
#[derive(PartialEq)]
struct Swatches {
    value: Color,
    link: Link,
}

impl Component for Swatches {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let swatches = SWATCHES.iter().map(|hex| {
            let colour = colour_from_hex(hex).unwrap_or(Paint::Backdrop.color(&tokens));
            Element::from(Swatch {
                colour,
                selected: colour == self.value.with_a(colour.a()),
                link: self.link.clone(),
            })
        });
        rect()
            .tw(&tokens, &[w_full(), flex_row(), gap(S1)])
            .children(swatches)
    }
}

/// A theme colour; a press or an activation key chooses it, keeping the alpha
/// being edited.
#[derive(PartialEq)]
struct Swatch {
    colour: Color,
    selected: bool,
    link: Link,
}

impl Component for Swatch {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let (colour, link) = (self.colour, self.link.clone());
        let chip = rect()
            .component("ColourSwatch")
            .tw(
                &tokens,
                &[
                    w(Dim::Swatch),
                    h(Dim::Swatch),
                    rounded(Radius::Pill),
                    fill(colour),
                ],
            )
            .maybe(self.selected, |chip| {
                chip.tw(&tokens, &[border(Line::Ring(Paint::Knob))])
            });
        focus_stop(
            chip,
            format!("Colour {}", colour_to_hex(colour)),
            AccessibilityRole::Button,
            true,
            Radius::Pill,
            EventHandler::new(move |_: ()| {
                link.set_colour(colour.with_a(link.current().alpha));
            }),
        )
    }
}

/// The hex text of a colour, editable. A valid entry is the colour as it is
/// typed; the text is rewritten only when the colour changes to one the text
/// does not already say, so dragging a thumb shows its hex here and typing
/// here is never replaced under the caret.
#[derive(PartialEq)]
struct HexField {
    colour: Color,
    link: Link,
}

impl Component for HexField {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let text = use_state(|| colour_to_hex(self.colour));
        use_side_effect_with_deps(&self.colour, move |colour: &Color| {
            if colour_from_hex(text.peek().trim()) != Some(*colour) {
                let mut text = text;
                text.set(colour_to_hex(*colour));
            }
        });
        let typed = self.link.clone();
        let submitted = self.link.clone();
        // A flat line in the code face, with no fill or radius, like the rules.
        rect()
            .tw(&tokens, &[w_full(), flex_row(), items(Align::Center)])
            .child(
                text_field(text, move |_: String| {
                    // Enter tidies what was typed into the colour it makes.
                    let mut text = text;
                    text.set(colour_to_hex(submitted.current().colour()));
                })
                .on_change(move |shown: String| {
                    if let Some(colour) = typed_colour(&shown, typed.current()) {
                        typed.set_colour(colour);
                    }
                })
                .flat()
                .fill_width(),
            )
    }
}

/// The colour a hex entry says, or `None` while it is not one. Six digits say
/// no alpha, so they keep the alpha of `editing`; eight say their own.
fn typed_colour(text: &str, editing: Hsva) -> Option<Color> {
    let text = text.trim();
    let colour = colour_from_hex(text)?;
    let digits = text.strip_prefix('#').unwrap_or(text).len();
    Some(if digits == 6 {
        colour.with_a(editing.alpha)
    } else {
        colour
    })
}

// ---- Drawing -------------------------------------------------------------------

/// A whole device pixel at or near `offset`, so a thumb's edge is crisp at a
/// fractional display scale.
fn snapped(offset: f32, scale: f64) -> f32 {
    let scale = scale as f32;
    if scale > 0.0 {
        (offset * scale).round() / scale
    } else {
        offset
    }
}

/// An absolutely positioned box `width` by `height`, starting `left` pixels in
/// from its parent's edge, so stacked layers overlap instead of splitting the
/// parent's height.
fn layer(left: f32, width: f32, height: f32) -> Rect {
    rect()
        .position(Position::new_absolute().left(left).top(0.0))
        .width(Size::px(width))
        .height(Size::px(height))
}

/// A colour as a round chip of `size` pixels, with a two-by-two checkerboard
/// behind it, so a translucent colour shows its alpha against a visible
/// pattern. Every swatch of a colour that can be transparent uses this.
pub fn colour_chip(tokens: Tokens, colour: Color, size: f32) -> impl IntoElement {
    let half = size / 2.0;
    let light = Paint::Knob.color(&tokens);
    let dark = Paint::Control.color(&tokens);
    let mut chip = rect()
        .component("ColourChip")
        .width(Size::px(size))
        .height(Size::px(size))
        .tw(&tokens, &[rounded(Radius::Pill), clip()]);
    for row in 0..2 {
        for col in 0..2 {
            let tone = if (row + col) % 2 == 0 { light } else { dark };
            chip = chip.child(
                rect()
                    .position(
                        Position::new_absolute()
                            .left(col as f32 * half)
                            .top(row as f32 * half),
                    )
                    .width(Size::px(half))
                    .height(Size::px(half))
                    .tw(&tokens, &[fill(tone)]),
            );
        }
    }
    chip.child(
        layer(0.0, size, size)
            .layer(Layer::Relative(1))
            .tw(&tokens, &[fill(colour)]),
    )
    .child(
        layer(0.0, size, size)
            .layer(Layer::Relative(2))
            .tw(
            &tokens,
            &[rounded(Radius::Pill), border(Line::Ring(Paint::Separator))],
        ))
}

/// A round handle at an offset inside its parent, outlined so it reads on any
/// colour.
fn thumb_handle(tokens: &Tokens, x: f32, y: f32, fill_colour: Color) -> Rect {
    rect()
        .component("ColourThumb")
        .layer(Layer::Relative(4))
        .position(Position::new_absolute().left(x).top(y))
        .tw(
            tokens,
            &[
                w(Dim::Thumb),
                h(Dim::Thumb),
                rounded(Radius::Pill),
                border(Line::Ring(Paint::Knob)),
                shadow(Elevation::Knob),
                fill(fill_colour),
            ],
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    const THUMB: f32 = 12.0;

    fn blue() -> Hsva {
        Hsva::from_colour(colour_from_argb(255, 53, 132, 228))
    }

    #[test]
    fn the_square_maps_the_pointer_to_saturation_and_value() {
        let (sat, val) = square_at((PANEL_W / 2.0, SQUARE_H / 2.0), THUMB);
        assert!((sat - 0.5).abs() < 1e-6 && (val - 0.5).abs() < 1e-6);
        // Saturation grows to the right and value upwards.
        assert_eq!(square_at((THUMB / 2.0, THUMB / 2.0), THUMB), (0.0, 1.0));
        assert_eq!(
            square_at((PANEL_W - THUMB / 2.0, SQUARE_H - THUMB / 2.0), THUMB),
            (1.0, 0.0)
        );
    }

    #[test]
    fn a_pointer_past_an_edge_of_the_square_is_the_edge() {
        assert_eq!(square_at((-500.0, -500.0), THUMB), (0.0, 1.0));
        assert_eq!(square_at((5000.0, 5000.0), THUMB), (1.0, 0.0));
        assert_eq!(square_at((100.0, -9999.0), THUMB).1, 1.0);
        assert_eq!(square_at((-9999.0, 60.0), THUMB).0, 0.0);
        // Inside the thumb's margin of an edge it is the edge too.
        assert_eq!(square_at((2.0, 3.0), THUMB), (0.0, 1.0));
        assert_eq!(square_at((PANEL_W - 1.0, SQUARE_H - 1.0), THUMB), (1.0, 0.0));
    }

    #[test]
    fn the_thumb_centre_is_the_pointer_wherever_the_thumb_can_go() {
        for (x, y) in [(40.0_f32, 30.0_f32), (100.0, 66.0), (180.0, 120.0), (6.0, 6.0)] {
            let (sat, val) = square_at((x, y), THUMB);
            let centre_x = THUMB / 2.0 + sat * (PANEL_W - THUMB);
            let centre_y = THUMB / 2.0 + (1.0 - val) * (SQUARE_H - THUMB);
            assert!((centre_x - x).abs() < 1e-3, "{x}: thumb at {centre_x}");
            assert!((centre_y - y).abs() < 1e-3, "{y}: thumb at {centre_y}");
        }
    }

    #[test]
    fn a_strip_maps_the_pointer_to_a_fraction_and_clamps() {
        assert_eq!(strip_at((THUMB / 2.0, 7.0), THUMB), 0.0);
        assert_eq!(strip_at((PANEL_W - THUMB / 2.0, 7.0), THUMB), 1.0);
        assert!((strip_at((PANEL_W / 2.0, 7.0), THUMB) - 0.5).abs() < 1e-6);
        assert_eq!(strip_at((-300.0, 7.0), THUMB), 0.0);
        assert_eq!(strip_at((900.0, -400.0), THUMB), 1.0);
    }

    #[test]
    fn a_strip_part_survives_being_set_and_read_back() {
        let colour = blue();
        for fraction in [0.0, 0.1, 0.5, 0.9, 1.0] {
            let hue = StripKind::Hue.apply(colour, fraction);
            assert!((StripKind::Hue.fraction(hue) - fraction).abs() < 1e-6);
            let alpha = StripKind::Alpha.apply(colour, fraction);
            assert!((StripKind::Alpha.fraction(alpha) - fraction).abs() < 1.0 / 255.0);
        }
        // The hue strip changes the hue and nothing else.
        let moved = StripKind::Hue.apply(colour.with_alpha(40), 0.25);
        assert_eq!((moved.sat, moved.val, moved.alpha), (colour.sat, colour.val, 40));
        // The alpha strip changes the alpha and nothing else.
        let faded = StripKind::Alpha.apply(colour, 0.5);
        assert_eq!((faded.hue, faded.sat, faded.val), (colour.hue, colour.sat, colour.val));
        assert_eq!(faded.alpha, 128);
    }

    #[test]
    fn arrows_nudge_one_step_and_shift_ten() {
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
    fn keys_move_the_square_by_a_percent_and_stop_at_the_edges() {
        let state = Hsva::new(211.0, 0.5, 0.5, 255);
        let right = nudged_square(state, Nudge::By { x: 1.0, y: 0.0 }).unwrap();
        assert!((right.sat - 0.51).abs() < 1e-6 && right.val == 0.5);
        let up = nudged_square(state, Nudge::By { x: 0.0, y: 10.0 }).unwrap();
        assert!((up.val - 0.6).abs() < 1e-6 && up.sat == 0.5);
        let past = nudged_square(Hsva::new(211.0, 0.99, 0.01, 255), Nudge::By { x: 10.0, y: -10.0 })
            .unwrap();
        assert_eq!((past.sat, past.val), (1.0, 0.0));
        // The hue is not the square's to change.
        assert_eq!(right.hue, 211.0);
        assert_eq!(nudged_square(state, Nudge::Start), None);
    }

    #[test]
    fn keys_move_a_strip_by_a_degree_or_a_percent_and_home_end_go_to_the_ends() {
        let state = Hsva::new(100.0, 0.5, 0.5, 100);
        let hue = nudged_strip(StripKind::Hue, state, Nudge::By { x: 1.0, y: 0.0 });
        assert!((hue.hue - 101.0).abs() < 1e-3, "{}", hue.hue);
        let hue = nudged_strip(StripKind::Hue, state, Nudge::By { x: -10.0, y: 0.0 });
        assert!((hue.hue - 90.0).abs() < 1e-3, "{}", hue.hue);
        assert_eq!(nudged_strip(StripKind::Hue, state, Nudge::Start).hue, 0.0);
        assert_eq!(nudged_strip(StripKind::Hue, state, Nudge::End).hue, 360.0);
        // Up and right both raise it; down and left both lower it.
        let up = nudged_strip(StripKind::Hue, state, Nudge::By { x: 0.0, y: 1.0 });
        assert!((up.hue - 101.0).abs() < 1e-3);
        let alpha = nudged_strip(StripKind::Alpha, state, Nudge::By { x: 10.0, y: 0.0 });
        assert_eq!(alpha.alpha, 126); // 100 + 10 % of 255, rounded
        assert_eq!(nudged_strip(StripKind::Alpha, state, Nudge::End).alpha, 255);
        assert_eq!(nudged_strip(StripKind::Alpha, state, Nudge::Start).alpha, 0);
        // The strips never leave their range.
        let low = Hsva::new(0.2, 0.5, 0.5, 2);
        assert_eq!(nudged_strip(StripKind::Hue, low, Nudge::By { x: -10.0, y: 0.0 }).hue, 0.0);
        assert_eq!(nudged_strip(StripKind::Alpha, low, Nudge::By { x: -10.0, y: 0.0 }).alpha, 0);
    }

    #[test]
    fn a_typed_hex_keeps_the_alpha_unless_it_says_one() {
        let editing = Hsva::new(211.0, 0.5, 0.5, 90);
        assert_eq!(
            typed_colour("#ff0000", editing),
            Some(colour_from_argb(90, 255, 0, 0))
        );
        assert_eq!(
            typed_colour("ff0000", editing),
            Some(colour_from_argb(90, 255, 0, 0))
        );
        assert_eq!(
            typed_colour(" #ff000080 ", editing),
            Some(colour_from_argb(0x80, 255, 0, 0))
        );
        assert_eq!(typed_colour("#ff00", editing), None);
        assert_eq!(typed_colour("#fff", editing), None);
        assert_eq!(typed_colour("#gg0000", editing), None);
        assert_eq!(typed_colour("", editing), None);
    }

    #[test]
    fn a_thumb_snaps_to_a_whole_device_pixel() {
        assert_eq!(snapped(10.4, 1.0), 10.0);
        assert_eq!(snapped(10.4, 2.0), 10.5);
        // At 1.5 a device pixel is two thirds of a logical one.
        let snapped = snapped(10.4, 1.5);
        assert!(((snapped * 1.5) - (snapped * 1.5).round()).abs() < 1e-4);
        assert!((snapped - 10.4).abs() <= 1.0 / 3.0 + 1e-4);
        // A scale that is not usable leaves it alone.
        assert_eq!(super::snapped(10.4, 0.0), 10.4);
    }
}

/// The picker in a real window, under real pointer events: what a press and a
/// drag do, frame by frame, at 1x and at fractional scales.
#[cfg(all(test, feature = "testing"))]
mod live {
    use std::cell::RefCell;
    use std::time::Duration;

    use freya::engine::prelude::{SkData, SkImage, raster_n32_premul};
    use freya_testing::prelude::{PlatformEvent, TouchEventName, TouchPhase};

    use super::*;
    use crate::inspector::snapshot::Snapshot as Tree;
    use crate::snapshot::Snapshot;
    use crate::theme::provide_tokens;

    const WINDOW: (f32, f32) = (480.0, 420.0);
    const FRAME: Duration = Duration::from_millis(16);
    /// The picker's colour in most tests: a saturated blue, not on any edge.
    const BLUE: (u8, u8, u8) = (53, 132, 228);
    /// Where the host puts the picker's corner.
    const AT: Point = (20.0, 20.0);

    thread_local! {
        /// Every colour the picker reported to its caller, in order.
        static LOG: RefCell<Vec<Color>> = const { RefCell::new(Vec::new()) };
        /// The caller's colour, to set from outside.
        static VALUE: RefCell<Option<State<Color>>> = const { RefCell::new(None) };
    }

    /// A caller that keeps the colour and passes it back, and notes each
    /// report. The key "x" carries the picker 30 px down.
    #[derive(PartialEq)]
    struct Host {
        start: Color,
        /// Where the picker's corner is in the window.
        at: Point,
    }

    impl Component for Host {
        fn render(&self) -> impl IntoElement {
            let _tokens = provide_tokens();
            let mut colour = use_state(|| self.start);
            let mut shift = use_state(|| 0.0_f32);
            VALUE.with(|value| *value.borrow_mut() = Some(colour));
            rect()
                .position(
                    Position::new_absolute()
                        .left(self.at.0)
                        .top(self.at.1 + shift()),
                )
                .on_global_key_down(move |event: Event<KeyboardEventData>| {
                    if matches!(&event.key, Key::Character(text) if text == "x") {
                        *shift.write() += 30.0;
                    }
                })
                .child(colour_picker(colour(), move |next: Color| {
                    LOG.with(|log| log.borrow_mut().push(next));
                    colour.set(next);
                }))
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
        logged()
            .last()
            .copied()
            .expect("the picker reported nothing")
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

    /// The centre of an area.
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

    /// A picker in a headless window at a display scale. Every point it takes
    /// and gives is in logical pixels; the harness's pointer wants physical
    /// ones, so it is scaled here.
    struct Rig {
        shot: Snapshot,
        scale: f64,
    }

    impl Rig {
        fn new(start: Color, at: Point, scale: f64) -> Self {
            LOG.with(|log| log.borrow_mut().clear());
            let shot =
                Snapshot::with_scale(move || Host { start, at }.into_element(), WINDOW, scale);
            let mut rig = Self { shot, scale };
            rig.frames(4);
            rig
        }

        /// The usual picker: blue, at [`AT`], at 1x.
        fn standard() -> Self {
            Self::new(blue(), AT, 1.0)
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

        fn hover(&mut self, at: Point) {
            let at = self.physical(at);
            self.shot.hover(at.0, at.1);
        }

        fn click(&mut self, at: Point) {
            let at = self.physical(at);
            self.shot.click(at.0, at.1);
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

        fn key(&mut self, key: Key, modifiers: Modifiers) {
            self.shot.key_with(key, modifiers);
        }

        /// The window areas, in logical pixels, of every element a component
        /// named `name` made, in tree order.
        fn areas(&mut self, name: &str) -> Vec<Area> {
            Tree::capture_headless(self.shot.runner(), self.scale as f32)
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

        /// The thumb centres, in the order the picker draws them: the square,
        /// the hue strip, the alpha strip.
        fn thumbs(&mut self) -> Vec<Point> {
            self.areas("ColourThumb").into_iter().map(centre).collect()
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
    }

    #[test]
    fn what_you_click_on_the_hue_strip_is_the_colour_you_see_there() {
        // The strip draws red at its left end, through the spectrum, to red at
        // its right. A press at a point must give the hue drawn at that point.
        for scale in [1.0, 1.5] {
            for fraction in [0.1_f32, 0.35, 0.8, 0.95] {
                let mut rig = Rig::new(blue(), AT, scale);
                let strip = rig.area("ColourHue");
                let at = within(strip, fraction, 0.5);
                let seen = rig.pixel(at);
                rig.tap(at);
                let drawn = hue_of(argb(255, seen.0, seen.1, seen.2));
                assert!(
                    hue_gap(hue_of(last()), drawn) < 6.0,
                    "scale {scale}, {fraction}: the strip shows hue {drawn} and the press picked {}",
                    hue_of(last())
                );
            }
        }
    }

    #[test]
    fn what_you_click_in_the_square_is_the_colour_you_see_there() {
        // White at the left, the hue at the right, black at the bottom.
        for scale in [1.0, 1.25] {
            for (fx, fy) in [
                (0.2_f32, 0.2_f32),
                (0.7, 0.35),
                (0.5, 0.5),
                (0.2, 0.8),
                (0.8, 0.8),
            ] {
                let mut rig = Rig::new(blue(), AT, scale);
                let square = rig.area("ColourSquare");
                let at = within(square, fx, fy);
                // The thumb sits on the picked colour; look before it moves.
                let seen = rig.pixel(at);
                rig.tap(at);
                let picked = channels(last());
                assert!(
                    near(seen, picked, 30),
                    "scale {scale}, ({fx}, {fy}): the square shows {seen:?} and the press picked {picked:?}"
                );
            }
        }
    }

    #[test]
    fn the_alpha_strip_fades_in_from_the_left() {
        let mut rig = Rig::new(argb(255, 200, 30, 30), AT, 1.0);
        let strip = rig.area("ColourAlpha");
        let y = strip.min_y() + strip.height() / 2.0;
        // Just inside the solid end, left of the thumb: all but opaque.
        let end = rig.pixel((strip.max_x() - 15.0, y));
        let hue = hue_of(argb(255, 200, 30, 30));
        assert!(
            hue_gap(hue_of(argb(255, end.0, end.1, end.2)), hue) < 8.0,
            "the opaque end of the alpha strip is {end:?}, not the colour"
        );
        // Just inside the transparent end: the checker, not the colour.
        let start = rig.pixel((strip.min_x() + 8.0, y));
        assert!(
            !near(start, (200, 30, 30), 40),
            "the transparent end of the alpha strip is the solid colour: {start:?}"
        );
    }

    #[test]
    fn a_drag_in_the_square_follows_the_pointer_across_and_outside_and_keeps_the_hue() {
        let mut rig = Rig::standard();
        let start_hue = hue_of(blue());
        let square = rig.area("ColourSquare");
        let path = [
            within(square, 0.5, 0.5),
            within(square, 0.9, 0.2),
            // Out through the right edge, the top, the left (white), the
            // bottom (black) and across the window.
            (square.max_x() + 150.0, square.min_y() + 20.0),
            (square.max_x() + 150.0, square.min_y() - 20.0),
            (square.min_x() - 20.0, square.min_y() + 40.0),
            (square.min_x() - 20.0, square.max_y() + 200.0),
            within(square, 0.7, 0.6),
        ];
        rig.press(path[0]);
        for at in &path[1..] {
            rig.move_to(*at);
            rig.frames(3);
            let hsv = Hsva::from_colour(last());
            // Wherever the colour has a hue, it is the hue being edited.
            if hsv.sat > 0.08 && hsv.val > 0.08 {
                assert!(
                    hue_gap(hsv.hue, start_hue) < 4.0,
                    "at {at:?} the hue is {} not {start_hue}",
                    hsv.hue
                );
            }
        }
        let end = *path.last().unwrap();
        rig.release(end);
        rig.frames(3);
        // The thumb is where the pointer ended.
        let thumb = rig.thumbs()[0];
        assert!(
            (thumb.0 - end.0).abs() < 1.5 && (thumb.1 - end.1).abs() < 1.5,
            "thumb {thumb:?}, pointer {end:?}"
        );
    }

    #[test]
    fn the_hue_survives_the_saturation_and_value_reaching_zero() {
        let mut rig = Rig::standard();
        let square = rig.area("ColourSquare");
        let hue_thumb_x = rig.thumbs()[1].0;
        let grey_at = (square.min_x() - 20.0, square.min_y() + 66.0);
        let black_at = (square.min_x() - 20.0, square.max_y() + 80.0);
        rig.press(within(square, 0.5, 0.5));
        // Left past the edge: saturation 0, a grey.
        rig.move_to(grey_at);
        rig.frames(3);
        assert_eq!(last().r(), last().g());
        assert_eq!(last().g(), last().b());
        // Straight down: value 0, black.
        rig.move_to(black_at);
        rig.frames(3);
        assert_eq!(channels(last()), (0, 0, 0));
        rig.release(black_at);
        rig.frames(3);
        // The hue thumb has not moved: a grey and a black have no hue to lose.
        let after = rig.thumbs()[1].0;
        assert!(
            (after - hue_thumb_x).abs() < 1.5,
            "the hue thumb moved from {hue_thumb_x} to {after}"
        );
        // Drag back up and right: the colour is blue again, not red.
        rig.press(within(square, 0.2, 0.9));
        rig.move_to(within(square, 0.9, 0.1));
        rig.frames(3);
        rig.release(within(square, 0.9, 0.1));
        rig.frames(3);
        assert!(hue_gap(hue_of(last()), hue_of(blue())) < 4.0);
    }

    #[test]
    fn the_hue_strip_changes_only_the_hue() {
        // Saturation, value and alpha were all being edited; a hue drag must
        // not put them back to 1.
        let start = argb(128, BLUE.0, BLUE.1, BLUE.2);
        let mut rig = Rig::new(start, AT, 1.0);
        let strip = rig.area("ColourHue");
        let before = Hsva::from_colour(start);
        let end = (strip.max_x() + 200.0, strip.max_y() + 90.0);
        rig.press(within(strip, 0.2, 0.5));
        rig.move_to(within(strip, 0.7, 0.5));
        rig.frames(3);
        rig.move_to(end);
        rig.frames(3);
        rig.release(end);
        rig.frames(3);
        for colour in logged() {
            assert_eq!(colour.a(), 128, "the alpha was changed by a hue drag");
        }
        let after = Hsva::from_colour(last());
        assert!(
            (after.sat - before.sat).abs() < 0.03,
            "{} -> {}",
            before.sat,
            after.sat
        );
        assert!(
            (after.val - before.val).abs() < 0.03,
            "{} -> {}",
            before.val,
            after.val
        );
        // Dragged to the far end it is red, and the thumb stays at that end.
        assert!(hue_gap(after.hue, 0.0) < 4.0);
        let thumb = rig.thumbs()[1];
        assert!(
            thumb.0 > strip.max_x() - 8.0,
            "the hue thumb is at {} of a strip ending at {}",
            thumb.0,
            strip.max_x()
        );
    }

    #[test]
    fn a_drag_in_a_strip_follows_the_pointer_outside_the_strip() {
        let mut rig = Rig::standard();
        let strip = rig.area("ColourAlpha");
        rig.press(within(strip, 0.5, 0.5));
        // Up and left past the strip: alpha 0. Down and right: alpha 255.
        rig.move_to((strip.min_x() - 20.0, strip.min_y() - 20.0));
        rig.frames(3);
        assert_eq!(last().a(), 0);
        let far = (strip.max_x() + 100.0, strip.max_y() + 150.0);
        rig.move_to(far);
        rig.frames(3);
        assert_eq!(last().a(), 255);
        rig.release(far);
        rig.frames(3);
        // The thumb is at the right end, inside the strip.
        let thumb = rig.thumbs()[2];
        assert!(thumb.0 > strip.max_x() - 8.0 && thumb.0 < strip.max_x());
    }

    #[test]
    fn a_burst_of_moves_makes_one_report_a_frame() {
        let mut rig = Rig::standard();
        let square = rig.area("ColourSquare");
        rig.press(within(square, 0.1, 0.1));
        let before = logged().len();
        // Fifty moves with no frame between them.
        for step in 0..50 {
            rig.move_to(within(square, 0.1 + step as f32 * 0.015, 0.5));
        }
        let burst = logged().len() - before;
        assert!(burst <= 1, "{burst} reports for fifty moves in one frame");
        rig.frames(3);
        assert!(
            logged().len() - before <= 2,
            "the newest move is reported once"
        );
        // The report that came was the newest position, not the first.
        let sat = Hsva::from_colour(last()).sat;
        assert!(sat > 0.7, "the last report was for saturation {sat}");
        rig.release(within(square, 0.85, 0.5));
        rig.frames(3);
    }

    #[test]
    fn nothing_follows_the_pointer_once_the_button_is_up() {
        let mut rig = Rig::standard();
        let square = rig.area("ColourSquare");
        rig.press(within(square, 0.3, 0.3));
        rig.move_to(within(square, 0.6, 0.6));
        rig.frames(3);
        rig.release((square.max_x() + 60.0, square.max_y() + 60.0));
        rig.frames(3);
        let done = logged().len();
        for step in 0..20 {
            rig.move_to(within(square, 0.05 * step as f32, 0.04 * step as f32));
            rig.frames(1);
        }
        assert_eq!(logged().len(), done, "the picker followed a hover");
    }

    #[test]
    fn a_press_outside_the_controls_does_not_start_a_drag() {
        let mut rig = Rig::standard();
        let square = rig.area("ColourSquare");
        let outside = (square.max_x() + 100.0, square.min_y() + 50.0);
        rig.press(outside);
        rig.move_to(within(square, 0.5, 0.5));
        rig.frames(3);
        rig.release(within(square, 0.5, 0.5));
        assert!(
            logged().is_empty(),
            "a drag that began outside moved the picker"
        );
    }

    #[test]
    fn the_thumbs_sit_under_the_pointer_at_every_scale() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let mut rig = Rig::new(blue(), (23.0, 17.0), scale);
            let square = rig.area("ColourSquare");
            let hue = rig.area("ColourHue");
            let alpha = rig.area("ColourAlpha");
            // One point inside the travel of each control.
            let targets = [
                within(square, 0.31, 0.42),
                within(hue, 0.62, 0.5),
                within(alpha, 0.27, 0.5),
            ];
            for (index, target) in targets.into_iter().enumerate() {
                rig.tap(target);
                let thumb = rig.thumbs()[index];
                assert!(
                    (thumb.0 - target.0).abs() <= 1.0,
                    "scale {scale}, control {index}: thumb x {} for a press at {}",
                    thumb.0,
                    target.0
                );
                if index == 0 {
                    assert!(
                        (thumb.1 - target.1).abs() <= 1.0,
                        "scale {scale}: thumb y {} for a press at {}",
                        thumb.1,
                        target.1
                    );
                }
            }
        }
    }

    #[test]
    fn a_picker_moved_mid_drag_is_followed() {
        // A drag is on, and the picker is carried 30 px down while the pointer
        // rests. The next report maps the pointer onto the picker where it is
        // now, 30 px higher up it than before.
        let mut rig = Rig::standard();
        let square = rig.area("ColourSquare");
        let still = within(square, 0.5, 0.5);
        rig.press(still);
        rig.frames(3);
        rig.key(Key::Character("x".into()), Modifiers::default());
        rig.frames(4);
        rig.move_to((still.0 + 1.0, still.1));
        rig.frames(3);
        // The pointer was 66 px down the square, and is now 36: value 0.75.
        let value = Hsva::from_colour(last()).val;
        assert!((value - 0.75).abs() < 0.03, "value {value}, not 0.75");
        rig.release((still.0 + 1.0, still.1));
        rig.frames(3);
    }

    #[test]
    fn a_touch_drag_follows_one_finger_and_ends_with_it() {
        let mut rig = Rig::standard();
        let square = rig.area("ColourSquare");
        let out = (square.max_x() + 150.0, square.min_y() - 15.0);
        rig.touch(TouchEventName::TouchStart, within(square, 0.5, 0.5), 1);
        rig.frames(2);
        rig.touch(TouchEventName::TouchMove, out, 1);
        rig.frames(3);
        // A second finger is somebody else's.
        rig.touch(TouchEventName::TouchMove, within(square, 0.1, 0.9), 2);
        rig.frames(3);
        let hsv = Hsva::from_colour(last());
        assert!(hsv.sat > 0.98 && hsv.val > 0.98, "{hsv:?}");
        rig.touch(TouchEventName::TouchEnd, out, 1);
        rig.frames(3);
        let done = logged().len();
        // The finger is up: a touch that moves now is not a drag of ours.
        rig.touch(TouchEventName::TouchMove, within(square, 0.1, 0.9), 1);
        rig.frames(3);
        assert_eq!(logged().len(), done);
    }

    #[test]
    fn a_lost_finger_does_not_leave_the_picker_following_the_mouse() {
        let mut rig = Rig::standard();
        let square = rig.area("ColourSquare");
        rig.touch(TouchEventName::TouchStart, within(square, 0.5, 0.5), 1);
        rig.frames(2);
        // The finger is never released and never cancelled where we can hear it.
        let done = logged().len();
        rig.hover(within(square, 0.9, 0.9));
        rig.frames(3);
        rig.hover(within(square, 0.2, 0.2));
        rig.frames(3);
        assert_eq!(logged().len(), done, "the mouse dragged a lost touch");
    }

    #[test]
    fn a_cancelled_touch_ends_the_drag() {
        let mut rig = Rig::standard();
        let square = rig.area("ColourSquare");
        let at = within(square, 0.5, 0.5);
        rig.touch(TouchEventName::TouchStart, at, 1);
        rig.frames(2);
        rig.touch(TouchEventName::TouchCancel, at, 1);
        rig.frames(2);
        let done = logged().len();
        rig.touch(TouchEventName::TouchMove, within(square, 0.9, 0.1), 1);
        rig.frames(3);
        assert_eq!(logged().len(), done);
    }

    #[test]
    fn arrow_keys_move_the_focused_thumb_and_shift_moves_it_ten_times_as_far() {
        let mut rig = Rig::standard();
        let strip = rig.area("ColourHue");
        // A press on the strip focuses it.
        rig.tap(within(strip, 0.5, 0.5));
        let base = hue_of(last());
        rig.key(Key::Named(NamedKey::ArrowRight), Modifiers::default());
        let one = hue_of(last());
        assert!(
            (one - base - 1.0).abs() < 1.5,
            "one step moved the hue from {base} to {one}"
        );
        rig.key(Key::Named(NamedKey::ArrowRight), Modifiers::SHIFT);
        let ten = hue_of(last());
        assert!(
            (ten - one - 10.0).abs() < 1.5,
            "a Shift step moved the hue from {one} to {ten}"
        );
        rig.key(Key::Named(NamedKey::End), Modifiers::default());
        rig.frames(2);
        assert!(
            rig.thumbs()[1].0 > strip.max_x() - 8.0,
            "End is the far end of the strip"
        );
        rig.key(Key::Named(NamedKey::Home), Modifiers::default());
        rig.frames(2);
        assert!(
            rig.thumbs()[1].0 < strip.min_x() + 8.0,
            "Home is the near end of the strip"
        );
    }

    #[test]
    fn arrow_keys_move_the_square_by_a_percent() {
        let mut rig = Rig::standard();
        let square = rig.area("ColourSquare");
        rig.tap(within(square, 0.5, 0.5));
        let base = Hsva::from_colour(last());
        rig.key(Key::Named(NamedKey::ArrowRight), Modifiers::default());
        let right = Hsva::from_colour(last());
        assert!(
            (right.sat - base.sat - 0.01).abs() < 0.012,
            "{base:?} {right:?}"
        );
        rig.key(Key::Named(NamedKey::ArrowDown), Modifiers::SHIFT);
        let down = Hsva::from_colour(last());
        assert!(
            (right.val - down.val - 0.10).abs() < 0.02,
            "{right:?} {down:?}"
        );
    }

    #[test]
    fn a_colour_from_outside_is_taken_in_without_losing_the_hue() {
        let mut rig = Rig::standard();
        let strip = rig.area("ColourHue");
        rig.tap(within(strip, 0.83, 0.5));
        let hue_x = rig.thumbs()[1].0;
        // The caller sets a grey. A grey has no hue; the hue thumb stays.
        rig.shot.runner().run_in(|| {
            let mut value = VALUE.with(|value| value.borrow().expect("the host's colour"));
            value.set(argb(255, 128, 128, 128));
        });
        rig.frames(4);
        let now = rig.thumbs();
        assert!(
            (now[1].0 - hue_x).abs() < 1.5,
            "the hue thumb jumped to {}",
            now[1].0
        );
        // The caller sets a green: now the hue thumb goes there.
        rig.shot.runner().run_in(|| {
            let mut value = VALUE.with(|value| value.borrow().expect("the host's colour"));
            value.set(argb(255, 0, 200, 0));
        });
        rig.frames(4);
        let green = rig.thumbs()[1].0;
        let thumb = Dim::Thumb.px();
        let expected = strip.min_x() + thumb / 2.0 + (120.0 / 360.0) * (PANEL_W - thumb);
        assert!(
            (green - expected).abs() < 2.0,
            "the hue thumb is at {green}, not {expected}"
        );
    }

    #[test]
    fn a_swatch_chooses_its_colour_and_keeps_the_alpha() {
        let mut rig = Rig::new(argb(100, BLUE.0, BLUE.1, BLUE.2), AT, 1.0);
        // The orange swatch is the seventh of eight.
        let swatches = rig.areas("ColourSwatch");
        assert_eq!(swatches.len(), SWATCHES.len());
        rig.click(centre(swatches[6]));
        rig.frames(3);
        assert_eq!(last().a(), 100);
        assert_eq!(channels(last()), (0xff, 0x9f, 0x0a));
    }

    #[test]
    fn the_chip_shows_the_whole_colour() {
        #[derive(PartialEq)]
        struct Chips;
        impl Component for Chips {
            fn render(&self) -> impl IntoElement {
                let tokens = provide_tokens();
                rect()
                    .position(Position::new_absolute().left(10.0).top(10.0))
                    .child(colour_chip(
                        tokens,
                        colour_from_argb(255, 200, 40, 40),
                        24.0,
                    ))
            }
        }
        let mut shot = Snapshot::new(|| Chips.into_element(), (60.0, 60.0));
        shot.runner().poll_n(FRAME, 3);
        let mut rig = Rig { shot, scale: 1.0 };
        // Inside the circle, in every quadrant: the chip's colour.
        for at in [
            (17.0, 17.0),
            (27.0, 17.0),
            (17.0, 27.0),
            (27.0, 27.0),
            (22.0, 22.0),
        ] {
            let seen = rig.pixel(at);
            assert!(near(seen, (200, 40, 40), 6), "{at:?} is {seen:?}");
        }
    }
}
