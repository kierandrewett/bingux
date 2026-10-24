//! Progress indicators (SwiftUI `ProgressView`, libadwaita
//! `GtkProgressBar` and `AdwSpinner`).

use freya::animation::*;
use freya::prelude::*;

use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::CAPTION;

/// A 6 px progress bar. Determinate bars fill with the accent colour;
/// an indeterminate bar sweeps a 30% segment across the track.
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
        let sweep = use_animation(move |conf| {
            conf.on_creation(OnCreation::Run);
            conf.on_finish(OnFinish::restart());
            AnimNum::new(-30.0, 100.0)
                .time(1400)
                .function(Function::Sine)
                .ease(Ease::InOut)
        });
        let percent = fill.get().value();
        let sweep_x = sweep.get().value();

        // The filled share is a run-time percentage, so a raw `Size::percent`.
        let bar: Element = match fraction {
            Some(_) => rect()
                .width(Size::percent(percent))
                .height(Size::fill())
                .tw(&tokens, &[rounded(Radius::Mark), bg(Paint::Accent)])
                .into(),
            None if tokens.reduced_motion => rect()
                .width(Size::percent(30.0))
                .height(Size::fill())
                .tw(&tokens, &[rounded(Radius::Mark), bg(Paint::Accent)])
                .into(),
            None => SweepTrack { sweep_x }.into(),
        };

        rect()
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

/// The moving segment of an indeterminate bar. Measures the track so the
/// percentage sweep maps to pixels.
#[derive(PartialEq)]
struct SweepTrack {
    sweep_x: f32,
}

impl Component for SweepTrack {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut size = use_state(Area::default);
        let width = size.read().width();
        rect()
            .width(Size::fill())
            .height(Size::fill())
            .on_sized(move |event: Event<SizedEventData>| size.set(event.area))
            .child(
                rect()
                    .width(Size::percent(30.0))
                    .height(Size::fill())
                    .tw(&tokens, &[rounded(Radius::Mark), bg(Paint::Accent)])
                    .offset_x(width * self.sweep_x / 100.0),
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
        let animation = use_animation(move |conf| {
            conf.on_creation(OnCreation::Run);
            conf.on_finish(OnFinish::restart());
            AnimNum::new(0.0, 360.0).time(if tokens.reduced_motion { 2400 } else { 800 })
        });
        // The angle is a prop, so each animation tick re-renders the arc and
        // repaints it with the new rotation.
        let arc = SpinnerArc {
            px,
            stroke: stroke.px(),
            ink,
            angle: animation.get().value(),
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
