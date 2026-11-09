//! The range bar: a span inside a scale, on a pill track.
//!
//! One bar says three things at a glance: the scale (the track), where a
//! value lies in it (the filled span) and, when there is one, a marker (now).
//! A day's low and high among the lows and highs of the week, the UV index on
//! its scale from low to extreme, the air quality index on its bands.
//!
//! The fill is a gradient over the *scale*, not over the span: the colour at
//! a position is the same on every bar that uses the same stops, so a column
//! of bars reads as one picture (cold at the left, warm at the right).

use freya::prelude::*;

use super::palette;
use crate::component::ComponentName;
use crate::style::*;
use crate::surface::use_tokens;

/// The widest the marker's dot is, and the least a span may fill, as a share
/// of the track, so a one-value span is still a visible pill.
const MARKER: f32 = 10.0;
const LEAST_SPAN: f32 = 0.05;

/// A span of a scale on a track. Build one with [`range_bar`].
#[derive(Clone, Debug, PartialEq)]
pub struct RangeBar {
    domain: (f64, f64),
    span: Option<(f64, f64)>,
    marker: Option<f64>,
    stops: Vec<(f64, Color)>,
    label: Option<String>,
}

/// A bar for a scale from `from` to `to`. The whole scale is filled until
/// [`RangeBar::span`] says otherwise.
pub fn range_bar(from: f64, to: f64) -> RangeBar {
    RangeBar {
        domain: if to > from {
            (from, to)
        } else {
            (from, from + 1.0)
        },
        span: None,
        marker: None,
        stops: Vec::new(),
        label: None,
    }
}

impl RangeBar {
    /// The part of the scale to fill.
    pub fn span(mut self, low: f64, high: f64) -> Self {
        self.span = Some(if low <= high {
            (low, high)
        } else {
            (high, low)
        });
        self
    }

    /// A dot at `value`: now, on a scale.
    pub fn marker(mut self, value: f64) -> Self {
        self.marker = Some(value);
        self
    }

    /// The colours along the scale, as `(position in the scale, colour)` in
    /// ascending order. Without them the fill is the accent.
    pub fn colours(mut self, stops: Vec<(f64, Color)>) -> Self {
        self.stops = stops;
        self
    }

    /// The accessible name of the bar, such as "Low 4°, high 11°".
    pub fn label(mut self, text: impl Into<String>) -> Self {
        self.label = Some(text.into());
        self
    }

    fn fraction(&self, value: f64) -> f32 {
        let (from, to) = self.domain;
        (((value - from) / (to - from)).clamp(0.0, 1.0)) as f32
    }
}

/// The gradient of the filled span: `stops` of the scale between the span's two
/// ends, as positions along the span in percent.
fn span_gradient(stops: &[(f64, Color)], from: f64, to: f64) -> LinearGradient {
    // Freya's gradient `angle` is not CSS's: 270 runs left to right.
    let mut gradient = LinearGradient::new().angle(270.0);
    let width = (to - from).max(f64::EPSILON);
    gradient = gradient.stop(GradientStop::new(palette::blend(stops, from), 0.0));
    for &(at, colour) in stops {
        if at > from && at < to {
            gradient = gradient.stop(GradientStop::new(
                colour,
                (((at - from) / width) * 100.0) as f32,
            ));
        }
    }
    gradient.stop(GradientStop::new(palette::blend(stops, to), 100.0))
}

impl Component for RangeBar {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut size = use_state(Area::default);
        let (low, high) = self.span.unwrap_or(self.domain);
        let start = self.fraction(low);
        let end = self.fraction(high).max(start + LEAST_SPAN).min(1.0);
        let start = start.min(end - LEAST_SPAN).max(0.0);

        let fill = rect()
            .width(Size::percent((end - start) * 100.0))
            .height(Size::fill());
        let fill = if self.stops.is_empty() {
            fill.tw(&tokens, &[bg(Paint::Accent)])
        } else {
            // The span's own end positions in the scale, after the minimum
            // width widened it.
            let (from, to) = self.domain;
            let at = |fraction: f32| from + (to - from) * f64::from(fraction);
            gradient_fill(fill, span_gradient(&self.stops, at(start), at(end)))
        };
        let track = rect()
            .width(Size::fill())
            .tw(
                &tokens,
                &[
                    h(Dim::SliderTrack),
                    rounded(Radius::Pill),
                    bg(Paint::Control),
                    clip(),
                    flex_row(),
                ],
            )
            .child(
                rect()
                    .width(Size::percent(start * 100.0))
                    .height(Size::fill()),
            )
            .child(fill);

        let measured = size.read().width();
        let marker = self.marker.filter(|_| measured > 0.0).map(|value| {
            let x = (measured - MARKER) * self.fraction(value);
            rect()
                .position(Position::new_absolute().left(x.round()).top(0.0))
                .width(Size::px(MARKER))
                .height(Size::px(MARKER))
                .tw(
                    &tokens,
                    &[
                        rounded(Radius::Pill),
                        bg(Paint::Knob),
                        shadow(Elevation::Knob),
                    ],
                )
        });

        let mut bar = rect()
            .component("RangeBar")
            .width(Size::fill())
            .height(Size::px(MARKER))
            .tw(&tokens, &[flex_col(), justify(Align::Center)])
            .a11y_role(AccessibilityRole::GenericContainer)
            .on_sized(move |event: Event<SizedEventData>| size.set_if_modified(event.area))
            .child(track)
            .maybe_child(marker);
        if let Some(label) = &self.label {
            bar = bar.a11y_alt(label.clone());
        }
        bar
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_domain_is_never_empty() {
        let bar = range_bar(5.0, 5.0);
        assert!(bar.domain.1 > bar.domain.0);
        assert_eq!(range_bar(0.0, 10.0).fraction(5.0), 0.5);
        assert_eq!(range_bar(0.0, 10.0).fraction(20.0), 1.0);
    }
}
