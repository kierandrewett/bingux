//! A colour picker: a saturation and value square, hue and alpha strips, a hex
//! field and a row of theme swatches. The picker holds no colour of its own;
//! each change goes to `on_change` and the caller passes the value back.

use freya::prelude::*;

use super::text_field::text_field;
use crate::component::ComponentName;
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
/// chosen.
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

impl Component for ColourPicker {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let hsv = self.value.to_hsv();
        let (hue, sat, val) = (hsv.h, hsv.s, hsv.v);
        let alpha = self.value.a();
        let on_change = self.on_change.clone();

        let square = sv_square(tokens, hue, sat, val, self.value, on_change.clone());
        let hue_strip = hue_strip(tokens, hue, self.value, on_change.clone());
        let alpha_strip = alpha_strip(tokens, alpha, self.value, on_change.clone());
        let swatches = SWATCHES.iter().map(|hex| {
            let colour = colour_from_hex(hex).unwrap_or(Paint::Backdrop.color(&tokens));
            swatch(tokens, colour, self.value, on_change.clone())
        });

        rect()
            .component("ColourPicker")
            .key(colour_to_hex(self.value))
            .width(Size::px(WIDTH))
            .tw(&tokens, &[flex_col(), gap(S2), p(S2)])
            .child(square)
            .child(hue_strip)
            .child(alpha_strip)
            .child(
                rect()
                    .tw(&tokens, &[w_full(), flex_row(), gap(S1)])
                    .children(swatches),
            )
            .child(HexField {
                value: self.value,
                on_change: on_change.clone(),
            })
    }
}

/// The hex text of a colour, editable; a valid entry is committed on submit.
#[derive(PartialEq)]
struct HexField {
    value: Color,
    on_change: EventHandler<Color>,
}

impl Component for HexField {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let text = use_state(|| colour_to_hex(self.value));
        let on_change = self.on_change.clone();
        // A flat line in the code face, with no fill or radius, like the rules.
        rect()
            .tw(&tokens, &[w_full(), flex_row(), items(Align::Center)])
            .child(
                text_field(text, move |shown: String| {
                    if let Some(colour) = colour_from_hex(shown.trim()) {
                        on_change.call(colour);
                    }
                })
                .flat()
                .fill_width(),
            )
    }
}

/// Saturation across, value down. The square is the hue at full saturation
/// and value, overlaid with white fading to the right and black fading down.
/// The thumb's centre travels inside the square by its radius, so the handle
/// never hangs off an edge.
fn sv_square(
    tokens: Tokens,
    hue: f32,
    sat: f32,
    val: f32,
    value: Color,
    on_change: EventHandler<Color>,
) -> impl IntoElement {
    let origin = use_state(|| (0.0_f64, 0.0_f64));
    let pressing = use_state(|| false);
    let thumb = Dim::Thumb.px();
    let (travel_x, travel_y) = (PANEL_W - thumb, SQUARE_H - thumb);
    let base = colour_from_hsv(hue, 1.0, 1.0);
    let white = LinearGradient::new()
        .angle(90.0)
        .stop(GradientStop::new(Paint::Knob.color(&tokens), 0.0))
        .stop(GradientStop::new(
            Paint::Knob.color(&tokens).with_a(0),
            100.0,
        ));
    let black = LinearGradient::new()
        .angle(180.0)
        .stop(GradientStop::new(
            Paint::Backdrop.color(&tokens).with_a(0),
            0.0,
        ))
        .stop(GradientStop::new(Paint::Backdrop.color(&tokens), 100.0));
    let thumb_x = (sat * travel_x).clamp(0.0, travel_x);
    let thumb_y = ((1.0 - val) * travel_y).clamp(0.0, travel_y);
    let on_down = {
        let mut origin = origin;
        let mut pressing = pressing;
        let on_change = on_change.clone();
        move |event: Event<PointerEventData>| {
            if !event.data().is_primary() {
                return;
            }
            event.stop_propagation();
            let local = event.element_location();
            let global = event.global_location();
            origin.set((global.x - local.x, global.y - local.y));
            pressing.set(true);
            let s = ((local.x as f32 - thumb / 2.0) / travel_x).clamp(0.0, 1.0);
            let v = 1.0 - ((local.y as f32 - thumb / 2.0) / travel_y).clamp(0.0, 1.0);
            on_change.call(colour_from_hsv(hue, s, v).with_a(value.a()));
        }
    };
    let on_move = {
        let on_change = on_change.clone();
        move |event: Event<PointerEventData>| {
            if *pressing.peek() {
                event.prevent_default();
                let (ox, oy) = *origin.peek();
                let x = (event.global_location().x - ox) as f32 - thumb / 2.0;
                let y = (event.global_location().y - oy) as f32 - thumb / 2.0;
                let s = (x / travel_x).clamp(0.0, 1.0);
                let v = 1.0 - (y / travel_y).clamp(0.0, 1.0);
                on_change.call(colour_from_hsv(hue, s, v).with_a(value.a()));
            }
        }
    };
    let on_up = {
        let mut pressing = pressing;
        move |_: Event<PointerEventData>| pressing.set(false)
    };
    rect()
        .width(Size::px(PANEL_W))
        .height(Size::px(SQUARE_H))
        .tw(&tokens, &[fill(base), rounded(Radius::Mark), clip()])
        .on_pointer_down(on_down)
        .on_capture_global_pointer_move(on_move)
        .on_global_pointer_up(on_up)
        .child(gradient_fill(layer(0.0, PANEL_W, SQUARE_H), white))
        .child(gradient_fill(layer(0.0, PANEL_W, SQUARE_H), black))
        .child(thumb_handle(tokens, thumb_x, thumb_y, value))
}

/// A hue strip from red through the spectrum back to red.
fn hue_strip(
    tokens: Tokens,
    hue: f32,
    value: Color,
    on_change: EventHandler<Color>,
) -> impl IntoElement {
    let gradient = (0..=6).fold(LinearGradient::new().angle(90.0), |gradient, step| {
        let colour = colour_from_hsv(step as f32 * 60.0, 1.0, 1.0);
        gradient.stop(GradientStop::new(colour, step as f32 * (100.0 / 6.0)))
    });
    strip(
        tokens,
        gradient,
        false,
        hue / 360.0,
        value,
        on_change,
        move |local_x| {
            let h = local_x * 360.0;
            colour_from_hsv(h, 1.0, 1.0)
        },
    )
}

/// An alpha strip from transparent to the colour at full opacity.
fn alpha_strip(
    tokens: Tokens,
    alpha: u8,
    value: Color,
    on_change: EventHandler<Color>,
) -> impl IntoElement {
    let solid = value.with_a(255);
    let gradient = LinearGradient::new()
        .angle(90.0)
        .stop(GradientStop::new(solid.with_a(0), 0.0))
        .stop(GradientStop::new(solid, 100.0));
    let frac = alpha as f32 / 255.0;
    strip(
        tokens,
        gradient,
        true,
        frac,
        value,
        on_change,
        move |local_x| value.with_a((local_x * 255.0).round() as u8),
    )
}

/// A horizontal strip; a press or drag reports the colour at the pointer
/// through `colour_at`, with the pointer's fraction of the track. The track
/// runs between the thumb's centres, inset by its radius at each end, so the
/// handle never hangs off the strip's edge.
fn strip(
    tokens: Tokens,
    gradient: LinearGradient,
    checker: bool,
    frac: f32,
    value: Color,
    on_change: EventHandler<Color>,
    colour_at: impl Fn(f32) -> Color + Clone + 'static,
) -> impl IntoElement {
    let origin = use_state(|| 0.0_f64);
    let pressing = use_state(|| false);
    let thumb = Dim::Thumb.px();
    let travel = PANEL_W - thumb;
    let thumb_x = (frac * travel).clamp(0.0, travel);
    let on_down = {
        let mut origin = origin;
        let mut pressing = pressing;
        let on_change = on_change.clone();
        let colour_at = colour_at.clone();
        move |event: Event<PointerEventData>| {
            if !event.data().is_primary() {
                return;
            }
            event.stop_propagation();
            let local = event.element_location();
            let global = event.global_location();
            origin.set(global.x - local.x);
            pressing.set(true);
            let f = ((local.x as f32 - thumb / 2.0) / travel).clamp(0.0, 1.0);
            on_change.call(colour_at(f));
        }
    };
    let on_move = {
        let on_change = on_change.clone();
        move |event: Event<PointerEventData>| {
            if *pressing.peek() {
                event.prevent_default();
                let x = (event.global_location().x - *origin.peek()) as f32 - thumb / 2.0;
                let f = (x / travel).clamp(0.0, 1.0);
                on_change.call(colour_at(f));
            }
        }
    };
    let on_up = {
        let mut pressing = pressing;
        move |_: Event<PointerEventData>| pressing.set(false)
    };
    let body = rect()
        .width(Size::px(PANEL_W))
        .height(Size::px(STRIP_H))
        .tw(&tokens, &[rounded(Radius::Pill)])
        .on_pointer_down(on_down)
        .on_capture_global_pointer_move(on_move)
        .on_global_pointer_up(on_up);
    let track =
        || layer(thumb / 2.0, travel, STRIP_H).tw(&tokens, &[rounded(Radius::Pill), clip()]);
    let body = if checker {
        body.child(checker_cells(track(), travel, tokens))
    } else {
        body
    };
    body.child(gradient_fill(track(), gradient))
        .child(thumb_handle(
            tokens,
            thumb_x,
            (STRIP_H - thumb) / 2.0,
            value,
        ))
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

/// Light and dark cells filling a `width`-wide track, so a translucent strip
/// shows its alpha against a visible pattern.
fn checker_cells(mut body: Rect, width: f32, tokens: Tokens) -> Rect {
    let light = Paint::Knob.color(&tokens);
    let dark = Paint::Control.color(&tokens);
    let cols = (width / CHECK).round() as usize;
    let rows = (STRIP_H / CHECK).round() as usize;
    let (cell_w, cell_h) = (width / cols as f32, STRIP_H / rows as f32);
    for row in 0..rows {
        for col in 0..cols {
            let tone = if (row + col) % 2 == 0 { light } else { dark };
            body = body.child(
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
    body
}

/// A colour as a square chip of `size` pixels, with a two-by-two checkerboard
/// behind it, so a translucent colour shows its alpha against a visible
/// pattern. Every swatch of a colour that can be transparent uses this.
pub fn colour_chip(tokens: Tokens, colour: Color, size: f32) -> impl IntoElement {
    let half = size / 2.0;
    let light = Paint::Knob.color(&tokens);
    let dark = Paint::Control.color(&tokens);
    let mut chip = rect().width(Size::px(size)).height(Size::px(size)).tw(
        &tokens,
        &[rounded(Radius::Pill), border(Line::Ring(Paint::Separator))],
    );
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
        rect()
            .position(Position::new_absolute().left(0.0).top(0.0))
            .width(Size::px(size))
            .height(Size::px(size))
            .tw(&tokens, &[rounded(Radius::Pill), fill(colour)]),
    )
}

/// A round handle at an offset inside its parent, outlined so it reads on any
/// colour.
fn thumb_handle(tokens: Tokens, x: f32, y: f32, value: Color) -> impl IntoElement {
    rect().position(Position::new_absolute().left(x).top(y)).tw(
        &tokens,
        &[
            w(Dim::Thumb),
            h(Dim::Thumb),
            rounded(Radius::Pill),
            border(Line::Ring(Paint::Knob)),
            fill(value.with_a(255)),
        ],
    )
}

/// A theme colour; a press chooses it.
fn swatch(
    tokens: Tokens,
    colour: Color,
    value: Color,
    on_change: EventHandler<Color>,
) -> impl IntoElement {
    let selected = colour == value.with_a(colour.a());
    let ring = rect()
        .tw(
            &tokens,
            &[
                w(Dim::Swatch),
                h(Dim::Swatch),
                rounded(Radius::Pill),
                fill(colour),
            ],
        )
        .on_pointer_down(move |_: Event<PointerEventData>| on_change.call(colour));
    if selected {
        ring.tw(&tokens, &[border(Line::Ring(Paint::Knob))])
    } else {
        ring
    }
}
