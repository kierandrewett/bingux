//! Progress indicators (SwiftUI `ProgressView`, libadwaita
//! `GtkProgressBar` and `AdwSpinner`).

use freya::animation::*;
use freya::prelude::*;

use crate::scale::*;
use crate::surface::use_tokens;

const BAR_HEIGHT: f32 = 6.0;

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

    /// A 12 px muted caption above the track, with the percentage at the
    /// right for determinate bars.
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

        let bar: Element = match fraction {
            Some(_) => rect()
                .width(Size::percent(percent))
                .height(Size::fill())
                .corner_radius(RADIUS_SM)
                .background(tokens.accent)
                .into(),
            None if tokens.reduced_motion => rect()
                .width(Size::percent(30.0))
                .height(Size::fill())
                .corner_radius(RADIUS_SM)
                .background(tokens.accent)
                .into(),
            None => SweepTrack { sweep_x }.into(),
        };

        rect()
            .width(self.width.clone())
            .spacing(S1)
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
                    .direction(Direction::Horizontal)
                    .main_align(Alignment::SpaceBetween)
                    .child(label().text(text).font_size(TEXT_SM).color(tokens.muted))
                    .maybe_child(fraction.map(|fraction| {
                        label()
                            .text(format!("{}%", (fraction * 100.0).round() as i64))
                            .font_size(TEXT_SM)
                            .color(tokens.muted)
                    }))
            }))
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::px(BAR_HEIGHT))
                    .corner_radius(RADIUS_SM)
                    .background(tokens.outline)
                    .overflow(Overflow::Clip)
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
                    .corner_radius(RADIUS_SM)
                    .background(tokens.accent)
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
}

/// An indeterminate circular spinner in the accent colour, with an
/// optional muted label below it.
#[derive(PartialEq)]
pub struct Spinner {
    size: SpinnerSize,
    text: Option<String>,
    color: Option<Color>,
}

pub fn spinner(size: SpinnerSize) -> Spinner {
    Spinner {
        size,
        text: None,
        color: None,
    }
}

impl Spinner {
    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    /// Ring colour; the accent by default.
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
}

impl Component for Spinner {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let px = self.size.px();
        let stroke = (px / 8.0).max(2.0);
        let ring_color = self.color.unwrap_or(tokens.accent);
        let animation = use_animation(move |conf| {
            conf.on_creation(OnCreation::Run);
            conf.on_finish(OnFinish::restart());
            AnimNum::new(0.0, 360.0).time(if tokens.reduced_motion { 2400 } else { 800 })
        });
        let ring = rect()
            .width(Size::px(px))
            .height(Size::px(px))
            .corner_radius(px / 2.0)
            .rotate(animation.get().value())
            .border(
                Border::new()
                    .width(stroke)
                    .alignment(BorderAlignment::Inner)
                    .fill(ring_color.with_af32(0.22)),
            )
            .child(
                rect()
                    .width(Size::px(px))
                    .height(Size::px(px / 2.0))
                    .overflow(Overflow::Clip)
                    .child(
                        rect()
                            .width(Size::px(px))
                            .height(Size::px(px))
                            .corner_radius(px / 2.0)
                            .border(
                                Border::new()
                                    .width(stroke)
                                    .alignment(BorderAlignment::Inner)
                                    .fill(ring_color),
                            ),
                    ),
            );
        rect()
            .a11y_role(AccessibilityRole::ProgressIndicator)
            .cross_align(Alignment::Center)
            .spacing(S2)
            .child(ring)
            .maybe_child(self.text.clone().map(|text| {
                label()
                    .text(text)
                    .font_size(TEXT_SM)
                    .color(tokens.muted)
                    .max_lines(1)
            }))
    }
}
