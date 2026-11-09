//! The pictures of the weather: a sun, a moon, clouds, rain, snow and a bolt,
//! drawn in colour on a 64 by 64 grid, one for each sky by day and by night,
//! and the moon in its phase.
//!
//! They are art, drawn here as SVG text. The same drawing is used at every
//! size (the viewer scales it), so a glyph in the hourly strip is the glyph of
//! the hero, smaller. Raw colours are allowed in this file.

use std::f64::consts::TAU;

use bingux_weather::codes::Sky;
use bytes::Bytes;
use freya::prelude::*;

const DEFS: &str = r##"<defs>
<radialGradient id="sun" cx="0.4" cy="0.35" r="0.75"><stop offset="0" stop-color="#fff0a0"/><stop offset="1" stop-color="#ffab2e"/></radialGradient>
<linearGradient id="moon" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#fff6cf"/><stop offset="1" stop-color="#e2c985"/></linearGradient>
<linearGradient id="cloud" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#ffffff"/><stop offset="1" stop-color="#cdd8e6"/></linearGradient>
<linearGradient id="grey" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#b9c4d2"/><stop offset="1" stop-color="#8d9bb0"/></linearGradient>
<linearGradient id="dark" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#8493a8"/><stop offset="1" stop-color="#5c6a80"/></linearGradient>
</defs>"##;

#[derive(Clone, Copy, PartialEq)]
enum Tone {
    Light,
    Grey,
    Dark,
}

impl Tone {
    fn fill(self) -> &'static str {
        match self {
            Tone::Light => "url(#cloud)",
            Tone::Grey => "url(#grey)",
            Tone::Dark => "url(#dark)",
        }
    }
}

/// A cloud, drawn in a box about 43 by 32 at the origin of its own space,
/// moved by `(x, y)` and scaled by `scale`.
fn cloud(x: f32, y: f32, scale: f32, tone: Tone) -> String {
    format!(
        r#"<g transform="translate({x} {y}) scale({scale})" fill="{}"><circle cx="21" cy="34" r="10"/><circle cx="33" cy="26" r="13"/><circle cx="44" cy="34" r="9"/><rect x="10" y="34" width="43" height="11" rx="5.5"/></g>"#,
        tone.fill()
    )
}

fn sun(cx: f32, cy: f32, radius: f32) -> String {
    let mut out = String::new();
    for ray in 0..8 {
        let angle = ray as f32 * std::f32::consts::FRAC_PI_4;
        let (sin, cos) = angle.sin_cos();
        let (from, to) = (radius + radius * 0.38, radius + radius * 0.8);
        out.push_str(&format!(
            r##"<path d="M{:.1} {:.1}L{:.1} {:.1}" stroke="#ffbf33" stroke-width="{:.1}" stroke-linecap="round"/>"##,
            cx + cos * from,
            cy + sin * from,
            cx + cos * to,
            cy + sin * to,
            radius * 0.22,
        ));
    }
    out.push_str(&format!(
        r#"<circle cx="{cx}" cy="{cy}" r="{radius}" fill="url(#sun)"/>"#
    ));
    out
}

fn moon(cx: f32, cy: f32, radius: f32) -> String {
    // A crescent: the disc less a disc shifted up and to the right.
    let inner = radius * 0.82;
    let (ox, oy) = (cx + radius * 0.5, cy - radius * 0.42);
    format!(
        r##"<mask id="bite"><rect width="64" height="64" fill="#fff"/><circle cx="{ox}" cy="{oy}" r="{inner}" fill="#000"/></mask><circle cx="{cx}" cy="{cy}" r="{radius}" fill="url(#moon)" mask="url(#bite)"/>"##
    )
}

fn stars() -> String {
    [
        (10.0, 12.0, 1.6),
        (52.0, 10.0, 1.3),
        (56.0, 30.0, 1.1),
        (8.0, 34.0, 1.2),
    ]
    .iter()
    .map(|(x, y, r)| {
        format!(r##"<circle cx="{x}" cy="{y}" r="{r}" fill="#ffffff" fill-opacity="0.9"/>"##)
    })
    .collect()
}

fn drops(count: usize, y: f32, length: f32, colour: &str) -> String {
    let xs: &[f32] = match count {
        3 => &[22.0, 32.0, 42.0],
        4 => &[18.0, 28.0, 38.0, 48.0],
        _ => &[24.0, 40.0],
    };
    xs.iter()
        .enumerate()
        .map(|(index, x)| {
            let drop = y + if index % 2 == 1 { 4.0 } else { 0.0 };
            format!(
                r#"<path d="M{x} {drop}l-3 {length}" stroke="{colour}" stroke-width="3" stroke-linecap="round"/>"#
            )
        })
        .collect()
}

fn flakes(count: usize, y: f32) -> String {
    let xs: &[f32] = match count {
        5 => &[16.0, 25.0, 34.0, 43.0, 50.0],
        3 => &[22.0, 32.0, 42.0],
        _ => &[24.0, 40.0],
    };
    xs.iter()
        .enumerate()
        .map(|(index, x)| {
            let at = y + if index % 2 == 1 { 6.0 } else { 0.0 };
            format!(r##"<circle cx="{x}" cy="{at}" r="2.6" fill="#ffffff"/>"##)
        })
        .collect()
}

fn bolt() -> &'static str {
    r##"<path d="M35 38l-9 14h7l-4 11 12-16h-7l5-9z" fill="#ffd23f" stroke="#e8a900" stroke-width="1" stroke-linejoin="round"/>"##
}

fn celestial(day: bool, cx: f32, cy: f32, radius: f32) -> String {
    if day {
        sun(cx, cy, radius)
    } else {
        format!("{}{}", stars(), moon(cx, cy, radius * 1.1))
    }
}

/// The 64 by 64 picture of `sky`, by day or by night.
pub fn glyph(sky: Sky, day: bool) -> String {
    let body = match sky {
        Sky::Clear => celestial(day, 32.0, 32.0, 13.0),
        Sky::MostlyClear => format!(
            "{}{}",
            celestial(day, 26.0, 26.0, 11.0),
            cloud(22.0, 20.0, 0.62, Tone::Light)
        ),
        Sky::PartlyCloudy => format!(
            "{}{}",
            celestial(day, 24.0, 22.0, 10.0),
            cloud(2.0, 14.0, 0.95, Tone::Light)
        ),
        Sky::Overcast => format!(
            "{}{}",
            cloud(14.0, -2.0, 0.78, Tone::Dark),
            cloud(-5.0, 9.0, 1.05, Tone::Grey)
        ),
        Sky::Fog => {
            let mut lines = cloud(2.0, -4.0, 1.0, Tone::Grey);
            for (y, x, width) in [(44.0, 12.0, 38.0), (51.0, 20.0, 34.0), (58.0, 10.0, 30.0)] {
                lines.push_str(&format!(
                    r##"<rect x="{x}" y="{y}" width="{width}" height="3.4" rx="1.7" fill="#dfe6ef" fill-opacity="0.9"/>"##
                ));
            }
            lines
        }
        Sky::Drizzle => format!(
            "{}{}",
            cloud(2.0, -6.0, 1.0, Tone::Grey),
            drops(3, 42.0, 6.0, "#8cc4ff")
        ),
        Sky::Rain => format!(
            "{}{}",
            cloud(2.0, -6.0, 1.0, Tone::Grey),
            drops(3, 42.0, 9.0, "#5aaeff")
        ),
        Sky::HeavyRain => format!(
            "{}{}",
            cloud(2.0, -6.0, 1.0, Tone::Dark),
            drops(4, 41.0, 12.0, "#4aa0ff")
        ),
        Sky::FreezingRain => format!(
            "{}{}{}",
            cloud(2.0, -6.0, 1.0, Tone::Grey),
            drops(2, 42.0, 9.0, "#5aaeff"),
            flakes(2, 54.0).replace("#ffffff", "#bfe9ff")
        ),
        Sky::Snow => format!("{}{}", cloud(2.0, -6.0, 1.0, Tone::Grey), flakes(3, 47.0)),
        Sky::HeavySnow => format!("{}{}", cloud(2.0, -6.0, 1.0, Tone::Dark), flakes(5, 46.0)),
        Sky::Thunderstorm => format!(
            "{}{}{}",
            cloud(2.0, -8.0, 1.0, Tone::Dark),
            drops(2, 40.0, 9.0, "#5aaeff"),
            bolt()
        ),
    };
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64">{DEFS}{body}</svg>"#
    )
}

/// The moon in its phase (0 new, 0.5 full) on a 64 by 64 grid: a dark disc
/// with the lit part drawn over it, on the right while it grows.
pub fn moon_phase(phase: f64) -> String {
    let (cx, cy, r) = (32.0_f64, 32.0_f64, 26.0_f64);
    let waxing = phase < 0.5;
    let folded = if waxing { phase } else { 1.0 - phase };
    let k = (TAU * folded).cos();
    let rx = r * k.abs();
    let lit = if folded < 0.004 {
        String::new()
    } else if folded > 0.496 {
        format!(r#"<circle cx="{cx}" cy="{cy}" r="{r}" fill="url(#moon)"/>"#)
    } else {
        // The lit limb is the right half circle, closed by the terminator: an
        // ellipse arc that bulges right for a crescent and left for a gibbous.
        let sweep = if k > 0.0 { 0 } else { 1 };
        format!(
            r#"<path d="M{cx} {top}A{r} {r} 0 0 1 {cx} {bottom}A{rx:.2} {r} 0 0 {sweep} {cx} {top}Z" fill="url(#moon)"/>"#,
            top = cy - r,
            bottom = cy + r,
        )
    };
    let mirror = if waxing {
        String::new()
    } else {
        format!(r#" transform="translate({} 0) scale(-1 1)""#, cx * 2.0)
    };
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64">{DEFS}<circle cx="{cx}" cy="{cy}" r="{r}" fill="#2c3446"/><g{mirror}>{lit}</g></svg>"##
    )
}

/// The picture of `sky` at `size` px square.
pub fn view(sky: Sky, day: bool, size: f32) -> Element {
    picture(glyph(sky, day), size, size)
}

/// The moon in `phase` at `size` px square.
pub fn moon_view(phase: f64, size: f32) -> Element {
    picture(moon_phase(phase), size, size)
}

/// An SVG drawn into a box of `width` by `height` px.
pub fn picture(svg: String, width: f32, height: f32) -> Element {
    SvgViewer::new((svg.clone(), Bytes::from(svg)))
        .width(Size::px(width))
        .height(Size::px(height))
        .into_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_sky_draws_by_day_and_night() {
        for sky in [
            Sky::Clear,
            Sky::MostlyClear,
            Sky::PartlyCloudy,
            Sky::Overcast,
            Sky::Fog,
            Sky::Drizzle,
            Sky::Rain,
            Sky::HeavyRain,
            Sky::FreezingRain,
            Sky::Snow,
            Sky::HeavySnow,
            Sky::Thunderstorm,
        ] {
            for day in [true, false] {
                let svg = glyph(sky, day);
                assert!(svg.starts_with("<svg") && svg.ends_with("</svg>"));
                assert!(!svg.contains("NaN"));
            }
        }
    }

    #[test]
    fn the_moon_has_every_phase() {
        for step in 0..=20 {
            let svg = moon_phase(f64::from(step) / 20.0);
            assert!(svg.ends_with("</svg>") && !svg.contains("NaN"));
        }
    }
}
