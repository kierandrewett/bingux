//! Progress indicators (SwiftUI `ProgressView`, libadwaita
//! `GtkProgressBar` and `AdwSpinner`).

use std::f32::consts::{PI, TAU};

use freya::animation::*;
use freya::prelude::*;

use crate::component::ComponentName;
use crate::motion::use_cycle;
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::CAPTION;

/// One sweep of an indeterminate bar, in milliseconds.
const SWEEP_MS: u64 = 1400;
/// One breath of an indeterminate bar under reduced motion, in milliseconds.
const BREATH_MS: u64 = 2400;
/// The moving segment's share of the track, in percent.
const SEGMENT: f32 = 30.0;
/// How faint the breathing fill gets, as an opacity.
const BREATH_LOW: f32 = 0.3;
/// One turn of a spinner, in milliseconds, and under reduced motion.
const SPIN_MS: u64 = 800;
const SPIN_SLOW_MS: u64 = 2400;

/// A 6 px progress bar. Determinate bars fill with the accent colour. An
/// indeterminate bar sweeps a 30% segment across the track and starts over,
/// for as long as it is on screen. Under reduced motion the segment does not
/// travel: the whole fill breathes between faint and solid instead, because a
/// bar that stood still would look frozen.
#[derive(PartialEq)]
pub struct ProgressBar {
    /// `Some(fraction)` in `0.0..=1.0`, or `None` for indeterminate.
    fraction: Option<f64>,
    width: Size,
    text: Option<String>,
}

pub fn progress_bar(fraction: impl Into<Option<f64>>) -> ProgressBar {
    ProgressBar {
        fraction: fraction.into().map(|value| value.clamp(0.0, 1.0)),
        width: Size::fill(),
        text: None,
    }
}

impl ProgressBar {
    pub fn width(mut self, width: Size) -> Self {
        self.width = width;
        self
    }

    /// A caption above the track, with the percentage at the right for
    /// determinate bars.
    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }
}

impl Component for ProgressBar {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let fraction = self.fraction;
        let motion = tokens.motion(160);
        let fill =
            use_animation_transition(fraction.unwrap_or(0.0) as f32 * 100.0, move |from, to| {
                AnimNum::new(from, to)
                    .time(motion)
                    .function(Function::Cubic)
                    .ease(Ease::Out)
            });
        let percent = fill.get().value();

        // The filled share is a run-time percentage, so a raw `Size::percent`.
        // The indeterminate bar is its own component: its loop runs only
        // while the bar is indeterminate, and a label or a width changing on
        // this bar never restarts it.
        let bar: Element = match fraction {
            Some(_) => rect()
                .width(Size::percent(percent))
                .height(Size::fill())
                .tw(&tokens, &[rounded(Radius::Mark), bg(Paint::Accent)])
                .into(),
            None => Indeterminate.into(),
        };

        rect()
            .component("ProgressBar")
            .width(self.width.clone())
            .tw(&tokens, &[flex_col(), gap(S1)])
            .a11y_role(AccessibilityRole::ProgressIndicator)
            .a11y_builder(move |builder| {
                if let Some(fraction) = fraction {
                    builder.set_numeric_value(fraction);
                    builder.set_min_numeric_value(0.0);
                    builder.set_max_numeric_value(1.0);
                }
            })
            .maybe_child(self.text.clone().map(|text| {
                rect()
                    .width(Size::fill())
                    .tw(&tokens, &[flex_row(), justify(Align::Between)])
                    .child(CAPTION.label(&tokens, text))
                    .maybe_child(fraction.map(|fraction| {
                        CAPTION.label(&tokens, format!("{}%", (fraction * 100.0).round() as i64))
                    }))
            }))
            .child(
                rect()
                    .component("ProgressTrack")
                    .width(Size::fill())
                    .tw(
                        &tokens,
                        &[
                            h(Dim::Track),
                            rounded(Radius::Mark),
                            bg(Paint::Outline),
                            clip(),
                        ],
                    )
                    .child(bar),
            )
    }
}

/// Where the sweeping segment's left edge is at `phase` of the cycle (0 to 1),
/// in percent of the track: from just off the left edge (the segment is
/// entirely outside) to just off the right edge. It eases in and out, so it
/// leaves one edge and arrives at the other gently, and the end of a cycle
/// is where the next one begins.
fn sweep_left(phase: f32) -> f32 {
    let eased = 0.5 - 0.5 * (PI * phase).cos();
    -SEGMENT + (100.0 + SEGMENT) * eased
}

/// The opacity of the breathing fill at `phase` of its cycle: faint at both
/// ends, solid half way.
fn breath(phase: f32) -> f32 {
    BREATH_LOW + (1.0 - BREATH_LOW) * (0.5 - 0.5 * (TAU * phase).cos())
}

/// The inside of an indeterminate bar: a segment that sweeps across the track,
/// or a fill that breathes under reduced motion. Its clock ([`use_cycle`])
/// lives in this scope, so it runs for as long as the bar is indeterminate
/// and only restarts when the bar is mounted again.
#[derive(PartialEq)]
struct Indeterminate;

impl Component for Indeterminate {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let reduced = tokens.reduced_motion;
        let phase = use_cycle(if reduced { BREATH_MS } else { SWEEP_MS });
        // The track's width: the sweep is a distance in pixels, and the width
        // is known only after the first layout.
        let mut track = use_state(|| 0.0_f32);
        let width = *track.read();

        if reduced {
            return Element::from(
                rect()
                    .component("ProgressSegment")
                    .width(Size::fill())
                    .height(Size::fill())
                    .tw(&tokens, &[rounded(Radius::Mark), bg(Paint::Accent)])
                    .opacity(breath(phase)),
            );
        }
        // `offset_x` shifts a box's children, not the box itself: it moves the
        // segment inside this strip, and the strip keeps the track's size.
        // (Set on the segment it would move nothing: the segment has no
        // children.) Until the width is known the strip is empty, so the
        // segment's first frame is already at its start.
        Element::from(
            rect()
                .width(Size::fill())
                .height(Size::fill())
                .on_sized(move |event: Event<SizedEventData>| {
                    // Only a change of width draws the strip again.
                    if *track.peek() != event.area.width() {
                        track.set(event.area.width());
                    }
                })
                .offset_x(width * sweep_left(phase) / 100.0)
                .maybe_child((width > 0.0).then(|| {
                    rect()
                        .component("ProgressSegment")
                        .width(Size::percent(SEGMENT))
                        .height(Size::fill())
                        .tw(&tokens, &[rounded(Radius::Mark), bg(Paint::Accent)])
                })),
        )
    }
}

/// Spinner sizes, matching `AdwSpinner` guidance: 16 px inline, 24 px in
/// rows, 48 px for page-level loading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpinnerSize {
    Small,
    Medium,
    Large,
}

impl SpinnerSize {
    fn px(self) -> f32 {
        match self {
            Self::Small => 16.0,
            Self::Medium => 24.0,
            Self::Large => 48.0,
        }
    }

    /// The ring's stroke: 2 px, 3 px or 6 px as the spinner grows.
    fn stroke(self) -> Dim {
        match self {
            Self::Small => Dim::Ring,
            Self::Medium => Dim::SpinnerMedium,
            Self::Large => Dim::Track,
        }
    }
}

/// An indeterminate circular spinner, with an optional caption below it.
/// A faint full ring sits under a turning arc: only the top half of the
/// accent ring is drawn, so the arc reads as a spinning segment.
#[derive(PartialEq)]
pub struct Spinner {
    size: SpinnerSize,
    text: Option<String>,
    ink: Paint,
}

pub fn spinner(size: SpinnerSize) -> Spinner {
    Spinner {
        size,
        text: None,
        ink: Paint::Accent,
    }
}

impl Spinner {
    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    /// Ink of the turning arc; the accent by default.
    pub fn ink(mut self, ink: Paint) -> Self {
        self.ink = ink;
        self
    }
}

impl Component for Spinner {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let px = self.size.px();
        let stroke = self.size.stroke();
        let ink = self.ink;
        // The same loop clock as an indeterminate bar's. Under reduced motion
        // the ring turns slowly instead of standing still.
        let phase = use_cycle(if tokens.reduced_motion { SPIN_SLOW_MS } else { SPIN_MS });
        // The angle is a prop, so each tick re-renders the arc and repaints it
        // with the new rotation.
        let arc = SpinnerArc {
            px,
            stroke: stroke.px(),
            ink,
            angle: phase * 360.0,
        };
        let ring = rect()
            .width(Size::px(px))
            .height(Size::px(px))
            .child(rect().width(Size::px(px)).height(Size::px(px)).tw(
                &tokens,
                &[
                    rounded(Radius::Pill),
                    border(Line::Stroke(stroke, Paint::Track)),
                ],
            ))
            .child(arc);
        rect()
            .component("Spinner")
            .a11y_role(AccessibilityRole::ProgressIndicator)
            .tw(&tokens, &[flex_col(), center(), gap(S2)])
            .child(ring)
            .maybe_child(
                self.text
                    .clone()
                    .map(|text| CAPTION.label(&tokens, text).max_lines(1)),
            )
    }
}

/// The rotating half-ring. Freya clips in unrotated space, so the arc is
/// drawn as a stroked SVG path (the top half of the ring) and the rect that
/// holds it is rotated. Taking `angle` as a prop repaints it each tick.
#[derive(PartialEq)]
struct SpinnerArc {
    px: f32,
    stroke: f32,
    ink: Paint,
    angle: f32,
}

impl Component for SpinnerArc {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let px = self.px;
        let centre = px / 2.0;
        let radius = centre - self.stroke / 2.0;
        let colour = self.ink.color(&tokens);
        let svg = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="{px}" height="{px}" viewBox="0 0 {px} {px}"><path d="M{l} {centre}A{radius} {radius} 0 0 1 {r} {centre}" fill="none" stroke="#{hex}" stroke-opacity="{alpha}" stroke-width="{stroke}"/></svg>"##,
            l = centre - radius,
            r = centre + radius,
            hex = format!("{:02x}{:02x}{:02x}", colour.r(), colour.g(), colour.b()),
            alpha = f32::from(colour.a()) / 255.0,
            stroke = self.stroke,
        );
        rect()
            .width(Size::px(px))
            .height(Size::px(px))
            .rotate(self.angle)
            .position(Position::new_absolute().top(0.0).left(0.0))
            .child(
                SvgViewer::new((svg.clone(), Bytes::from(svg)))
                    .width(Size::px(px))
                    .height(Size::px(px)),
            )
    }
}
