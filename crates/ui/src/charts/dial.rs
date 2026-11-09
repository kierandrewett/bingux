//! Two dials: the compass (a bearing and a value) and the day arc (where the
//! sun is between rising and setting).
//!
//! Both follow the other charts: an SVG at the measured size, anti-aliased,
//! with the text laid over it as ordinary elements, and a text colour read
//! from the theme. A dial is not mirrored in a right-to-left interface: north
//! is up and the sun rises on the left, as on every map and clock.

use std::f32::consts::PI;

use freya::prelude::*;

use super::data::SeriesColour;
use super::geometry::{CENTRE_GAP, HAIRLINE, LABEL_GAP, LABEL_HEIGHT, RING_TRACK_OPACITY};
use super::palette;
use super::svg::{self, Svg, dot, num, stroke_line};
use super::view::{Align, Text, anchored, canvas};
use crate::component::ComponentName;
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{CAPTION, Ink, TITLE};

/// The compass is a square this wide.
const COMPASS_SIZE: f32 = 132.0;
/// Where the compass's parts sit, as radii from its centre.
const COMPASS_RING: f32 = 61.0;
const TICK_LENGTH: f32 = 4.0;
const POINTER_BASE: f32 = 62.0;
const POINTER_TIP: f32 = 51.0;
const POINTER_HALF: f32 = 6.0;
const LETTER_RADIUS: f32 = 43.0;
const LETTER_BOX: f32 = 16.0;
/// A tick every this many degrees.
const TICK_STEP: u32 = 10;

/// A compass rose with a pointer at a bearing, and a value in the middle.
#[derive(Clone, Debug, PartialEq)]
pub struct Compass {
    bearing: f64,
    centre: Option<(String, String)>,
    label: Option<String>,
}

/// A compass pointing at `bearing`, in degrees clockwise from north.
pub fn compass(bearing: f64) -> Compass {
    Compass {
        bearing,
        centre: None,
        label: None,
    }
}

impl Compass {
    /// The value in the middle and the line under it: `("12", "km/h")`.
    pub fn centre(mut self, value: impl Into<String>, caption: impl Into<String>) -> Self {
        self.centre = Some((value.into(), caption.into()));
        self
    }

    /// The accessible name, such as "Wind from the north-east at 12 km/h".
    pub fn label(mut self, text: impl Into<String>) -> Self {
        self.label = Some(text.into());
        self
    }
}

/// The point at `radius` from `(cx, cy)` on a bearing in radians clockwise from up.
fn on_bearing(cx: f32, cy: f32, radius: f32, bearing: f32) -> (f32, f32) {
    (cx + radius * bearing.sin(), cy - radius * bearing.cos())
}

impl Component for Compass {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let size = COMPASS_SIZE;
        let centre = size / 2.0;
        let muted = Paint::Muted.color(&tokens);
        let mut svg = Svg::new(size, size);
        svg.push(format!(
            r#"<circle cx="{c}" cy="{c}" r="{r}" fill="none" {} stroke-width="{}"/>"#,
            svg::paint("stroke", muted.with_af32(RING_TRACK_OPACITY * 2.0)),
            num(HAIRLINE),
            c = num(centre),
            r = num(COMPASS_RING),
        ));
        let mut degrees = 0;
        while degrees < 360 {
            // The cardinal ticks are left out: the letters stand there.
            if degrees % 90 != 0 {
                let angle = (degrees as f32).to_radians();
                let (x0, y0) = on_bearing(centre, centre, COMPASS_RING, angle);
                let (x1, y1) = on_bearing(centre, centre, COMPASS_RING - TICK_LENGTH, angle);
                svg.push(stroke_line(
                    x0,
                    y0,
                    x1,
                    y1,
                    muted.with_af32(if degrees % 30 == 0 { 0.8 } else { 0.4 }),
                    HAIRLINE,
                ));
            }
            degrees += TICK_STEP;
        }

        // The pointer: a small triangle on the ring, its tip towards the middle,
        // at the bearing the wind comes from.
        let bearing = (self.bearing.rem_euclid(360.0) as f32).to_radians();
        let (bx, by) = on_bearing(centre, centre, POINTER_BASE, bearing);
        let (tx, ty) = on_bearing(centre, centre, POINTER_TIP, bearing);
        let across = bearing + PI / 2.0;
        let (lx, ly) = (
            bx + POINTER_HALF * across.sin(),
            by - POINTER_HALF * across.cos(),
        );
        let (rx, ry) = (
            bx - POINTER_HALF * across.sin(),
            by + POINTER_HALF * across.cos(),
        );
        svg.push(format!(
            r#"<path d="M{} {}L{} {}L{} {}Z" {} stroke-linejoin="round" stroke-width="1.5" {}/>"#,
            num(lx),
            num(ly),
            num(tx),
            num(ty),
            num(rx),
            num(ry),
            svg::paint("fill", Paint::Accent.color(&tokens)),
            svg::paint("stroke", Paint::Accent.color(&tokens)),
        ));

        let mut texts = Vec::new();
        for (index, letter) in ["N", "E", "S", "W"].iter().enumerate() {
            let (x, y) = on_bearing(centre, centre, LETTER_RADIUS, index as f32 * PI / 2.0);
            let style = if index == 0 {
                CAPTION.in_ink(Ink::Text)
            } else {
                CAPTION
            };
            texts.push(anchored(
                style,
                *letter,
                x,
                y - LABEL_HEIGHT / 2.0,
                LETTER_BOX,
                Align::Centre,
                size,
            ));
        }
        if let Some((value, caption)) = &self.centre {
            let width = size / 2.0;
            let left = centre - width / 2.0;
            // The value and its unit are one block, centred on the middle.
            let block = 30.0 + CENTRE_GAP + LABEL_HEIGHT;
            let top = centre - block / 2.0;
            texts.push(Text::new(
                TITLE.in_ink(Ink::Text),
                value.clone(),
                left,
                top,
                width,
                Align::Centre,
            ));
            texts.push(Text::new(
                CAPTION,
                caption.clone(),
                left,
                top + 30.0 + CENTRE_GAP,
                width,
                Align::Centre,
            ));
        }

        let mut dial = canvas(svg.finish(), texts, &tokens, size, size)
            .component("Compass")
            .a11y_role(AccessibilityRole::GenericContainer);
        if let Some(label) = &self.label {
            dial = dial.a11y_alt(label.clone());
        }
        dial
    }
}

/// The day arc's height: the arc, the horizon and the line of times under it.
const ARC_HEIGHT: f32 = 104.0;
/// Room each side of the arc for its end dots and the times under them.
const ARC_SIDE: f32 = 28.0;
const ARC_TOP: f32 = 10.0;
const ARC_LABEL_WIDTH: f32 = 64.0;
const ARC_STEPS: usize = 48;
const SUN_RADIUS: f32 = 6.0;
const ARC_WIDTH: f32 = 2.0;
/// Width before the first measurement.
const ARC_DEFAULT_WIDTH: f32 = 320.0;

/// The sun's day: an arc from sunrise to sunset with the sun on it.
#[derive(Clone, Debug, PartialEq)]
pub struct DayArc {
    progress: Option<f64>,
    from: String,
    to: String,
    label: Option<String>,
}

/// An arc with the sun `progress` of the way across (0 at sunrise, 1 at
/// sunset), or `None` when the sun is down.
pub fn day_arc(progress: Option<f64>) -> DayArc {
    DayArc {
        progress: progress.map(|value| value.clamp(0.0, 1.0)),
        from: String::new(),
        to: String::new(),
        label: None,
    }
}

impl DayArc {
    /// The times under the two ends: `("07:08", "16:20")`.
    pub fn times(mut self, from: impl Into<String>, to: impl Into<String>) -> Self {
        self.from = from.into();
        self.to = to.into();
        self
    }

    /// The accessible name, such as "Sunrise 07:08, sunset 16:20".
    pub fn label(mut self, text: impl Into<String>) -> Self {
        self.label = Some(text.into());
        self
    }
}

/// A point of the arc: `t` runs 0 at the left end to 1 at the right.
fn on_arc(left: f32, right: f32, horizon: f32, rise: f32, t: f32) -> (f32, f32) {
    let middle = (left + right) / 2.0;
    let half = (right - left) / 2.0;
    (
        middle - half * (PI * t).cos(),
        horizon - rise * (PI * t).sin(),
    )
}

/// The path of the arc from `from` to `to` (0 to 1) as short segments.
fn arc_path(left: f32, right: f32, horizon: f32, rise: f32, from: f32, to: f32) -> String {
    let mut d = String::new();
    for step in 0..=ARC_STEPS {
        let t = from + (to - from) * step as f32 / ARC_STEPS as f32;
        let (x, y) = on_arc(left, right, horizon, rise, t);
        d.push_str(&format!(
            "{}{} {}",
            if step == 0 { "M" } else { "L" },
            num(x),
            num(y)
        ));
    }
    d
}

impl Component for DayArc {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut size = use_state(Area::default);
        let measured = size.read().width();
        let width = if measured > 0.0 {
            measured
        } else {
            ARC_DEFAULT_WIDTH
        };
        let height = ARC_HEIGHT;

        let horizon = height - LABEL_GAP - LABEL_HEIGHT - 2.0;
        let rise = horizon - ARC_TOP;
        let left = ARC_SIDE;
        let right = (width - ARC_SIDE).max(left + 1.0);
        let sun = palette::resolve(&tokens, SeriesColour::Warning, 0);
        let muted = Paint::Muted.color(&tokens);

        let mut svg = Svg::new(width, height);
        // The horizon, then the whole arc faint, then the daylight so far in the sun's colour.
        svg.push(stroke_line(
            ARC_SIDE / 2.0,
            horizon,
            width - ARC_SIDE / 2.0,
            horizon,
            muted.with_af32(RING_TRACK_OPACITY * 2.0),
            HAIRLINE,
        ));
        svg.push(format!(
            r#"<path d="{}" fill="none" {} stroke-width="{}" stroke-linecap="round" stroke-dasharray="2 5"/>"#,
            arc_path(left, right, horizon, rise, 0.0, 1.0),
            svg::paint("stroke", muted.with_af32(0.6)),
            num(ARC_WIDTH),
        ));
        if let Some(progress) = self.progress {
            let progress = progress as f32;
            if progress > 0.0 {
                svg.push(format!(
                    r#"<path d="{}" fill="none" {} stroke-width="{}" stroke-linecap="round"/>"#,
                    arc_path(left, right, horizon, rise, 0.0, progress),
                    svg::paint("stroke", sun),
                    num(ARC_WIDTH),
                ));
            }
            let (x, y) = on_arc(left, right, horizon, rise, progress);
            svg.push(dot(
                x,
                y,
                SUN_RADIUS,
                sun,
                Some((Paint::Card.color(&tokens), 2.0)),
            ));
        }
        // The two ends sit on the horizon.
        for t in [0.0_f32, 1.0] {
            let (x, y) = on_arc(left, right, horizon, rise, t);
            svg.push(dot(x, y, 2.5, muted, None));
        }

        let mut texts = Vec::new();
        if !self.from.is_empty() {
            texts.push(anchored(
                CAPTION,
                self.from.clone(),
                left,
                horizon + LABEL_GAP,
                ARC_LABEL_WIDTH,
                Align::Centre,
                width,
            ));
        }
        if !self.to.is_empty() {
            texts.push(anchored(
                CAPTION,
                self.to.clone(),
                right,
                horizon + LABEL_GAP,
                ARC_LABEL_WIDTH,
                Align::Centre,
                width,
            ));
        }

        let mut arc = canvas(svg.finish(), texts, &tokens, width, height)
            .width(Size::fill())
            .component("DayArc")
            .a11y_role(AccessibilityRole::GenericContainer)
            .on_sized(move |event: Event<SizedEventData>| size.set_if_modified(event.area));
        if let Some(label) = &self.label {
            arc = arc.a11y_alt(label.clone());
        }
        arc
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    #[test]
    fn bearings_run_clockwise_from_up() {
        let (x, y) = on_bearing(50.0, 50.0, 10.0, 0.0);
        assert!((x - 50.0).abs() < 1e-4 && (y - 40.0).abs() < 1e-4);
        let (x, y) = on_bearing(50.0, 50.0, 10.0, PI / 2.0);
        assert!((x - 60.0).abs() < 1e-4 && (y - 50.0).abs() < 1e-4);
        let (x, y) = on_bearing(50.0, 50.0, 10.0, TAU / 2.0);
        assert!((x - 50.0).abs() < 1e-3 && (y - 60.0).abs() < 1e-3);
    }

    #[test]
    fn the_arc_runs_from_the_left_end_over_the_top_to_the_right() {
        let (x0, y0) = on_arc(10.0, 110.0, 80.0, 50.0, 0.0);
        let (xm, ym) = on_arc(10.0, 110.0, 80.0, 50.0, 0.5);
        let (x1, y1) = on_arc(10.0, 110.0, 80.0, 50.0, 1.0);
        assert!((x0 - 10.0).abs() < 1e-3 && (y0 - 80.0).abs() < 1e-3);
        assert!((xm - 60.0).abs() < 1e-3 && (ym - 30.0).abs() < 1e-3);
        assert!((x1 - 110.0).abs() < 1e-3 && (y1 - 80.0).abs() < 1e-3);
    }
}
