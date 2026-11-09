//! The sky behind the conditions, and the colours of the scales (temperature,
//! UV, air quality): a gradient for each kind of weather by day,
//! at dawn and dusk, and by night, and what is drawn over it: the sun's or
//! the moon's haze, stars, rain and snow (a still layer), and soft clouds in
//! two layers that the hero drifts.
//!
//! These are art, not interface: raw colours are allowed in this file, and
//! the tests at the end hold every gradient to the contrast the text on it
//! needs (white at 4.5:1 against the lightest end), keep the clouds a faint
//! haze, and keep the text's contrast through the clouds wherever they drift.
//!
//! A cloud is never a shape with an edge. It is a few overlapping ellipses,
//! each a radial gradient from a faint centre to nothing at the rim, so the
//! picture is blurred by construction (no filter, which the renderer may not
//! have), has nothing to cut at a border, and does not become a pill behind
//! a line of text.

use bingux_weather::codes::Sky;
use freya::prelude::Color;

/// How the light is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Light {
    Day,
    /// Within the hour either side of sunrise or sunset.
    Twilight,
    Night,
}

/// The two colours of a sky gradient, top to bottom.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gradient {
    pub top: Color,
    pub bottom: Color,
}

fn rgb(hex: u32) -> Color {
    Color::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

fn pair(top: u32, bottom: u32) -> Gradient {
    Gradient {
        top: rgb(top),
        bottom: rgb(bottom),
    }
}

/// The gradient for `sky` in `light`.
pub fn gradient(sky: Sky, light: Light) -> Gradient {
    match (light, sky) {
        (Light::Twilight, Sky::Clear | Sky::MostlyClear) => pair(0x3d4d8f, 0xa85a4e),
        (Light::Twilight, Sky::PartlyCloudy) => pair(0x45508a, 0x9a5a58),
        (Light::Twilight, other) => gradient(other, Light::Night),

        (Light::Day, Sky::Clear) => pair(0x1d5fb8, 0x2b72c6),
        (Light::Day, Sky::MostlyClear) => pair(0x2a68b8, 0x3577bd),
        (Light::Day, Sky::PartlyCloudy) => pair(0x3a64a0, 0x4a6f9a),
        (Light::Day, Sky::Overcast) => pair(0x5b6b7c, 0x4a5a6b),
        (Light::Day, Sky::Fog) => pair(0x5f6f7e, 0x536270),
        (Light::Day, Sky::Drizzle | Sky::Rain) => pair(0x4a5f78, 0x3c4e66),
        (Light::Day, Sky::HeavyRain) => pair(0x34455a, 0x27364a),
        (Light::Day, Sky::FreezingRain) => pair(0x4b6a86, 0x3a566f),
        (Light::Day, Sky::Snow) => pair(0x5f7893, 0x4d6783),
        (Light::Day, Sky::HeavySnow) => pair(0x546a82, 0x43586f),
        (Light::Day, Sky::Thunderstorm) => pair(0x2f3547, 0x232838),

        (Light::Night, Sky::Clear) => pair(0x0b1a3a, 0x1b3566),
        (Light::Night, Sky::MostlyClear) => pair(0x0e1d3d, 0x22386a),
        (Light::Night, Sky::PartlyCloudy) => pair(0x17243f, 0x2c3d5e),
        (Light::Night, Sky::Overcast) => pair(0x1f2735, 0x323c4d),
        (Light::Night, Sky::Fog) => pair(0x2b3441, 0x434d5b),
        (Light::Night, Sky::Drizzle | Sky::Rain) => pair(0x1a2433, 0x2b3a4f),
        (Light::Night, Sky::HeavyRain) => pair(0x131b28, 0x222e40),
        (Light::Night, Sky::FreezingRain) => pair(0x1d2a3b, 0x32465c),
        (Light::Night, Sky::Snow) => pair(0x28364a, 0x3a4a60),
        (Light::Night, Sky::HeavySnow) => pair(0x222f42, 0x344359),
        (Light::Night, Sky::Thunderstorm) => pair(0x151827, 0x232638),
    }
}

/// The colours along the temperature scale in °C, cold to hot: the colour a
/// day's range bar has at each degree, the same on every bar.
pub fn temperature_stops() -> Vec<(f64, Color)> {
    [
        (-15.0, 0x5a6fe0),
        (-5.0, 0x4a9be8),
        (5.0, 0x3fbad0),
        (13.0, 0x5cc47f),
        (19.0, 0xd8c640),
        (26.0, 0xf0a23a),
        (33.0, 0xe8643c),
        (40.0, 0xd03a46),
    ]
    .into_iter()
    .map(|(at, hex)| (at, rgb(hex)))
    .collect()
}

/// The UV index scale of the WHO, green to violet.
pub fn uv_stops() -> Vec<(f64, Color)> {
    [
        (0.0, 0x4caf6a),
        (2.5, 0x9fc94a),
        (4.0, 0xf2c94c),
        (6.0, 0xf2994a),
        (8.0, 0xe5484d),
        (11.0, 0x8e5bd0),
    ]
    .into_iter()
    .map(|(at, hex)| (at, rgb(hex)))
    .collect()
}

/// The US air quality index bands, green to maroon.
pub fn aqi_stops() -> Vec<(f64, Color)> {
    [
        (0.0, 0x4caf6a),
        (50.0, 0xf2c94c),
        (100.0, 0xf2994a),
        (150.0, 0xe5484d),
        (200.0, 0x8e5bd0),
        (300.0, 0x7a2438),
    ]
    .into_iter()
    .map(|(at, hex)| (at, rgb(hex)))
    .collect()
}

/// A small deterministic generator, so a sky is the same sky every time.
struct Scatter(u32);

impl Scatter {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) as f32 / (1u32 << 24) as f32
    }

    fn between(&mut self, low: f32, high: f32) -> f32 {
        low + (high - low) * self.next()
    }
}

/// The view box of the still layer ([`still`]): stars, rain and snow keep
/// their shape when it is stretched to the hero, so it is near the hero's own
/// proportions.
pub const ART_WIDTH: f32 = 400.0;
pub const ART_HEIGHT: f32 = 240.0;

/// The view box of a cloud layer ([`clouds`]) on each side. Clouds are soft
/// ellipses, which stretch to any shape without showing it.
const CLOUD_BOX: f32 = 1000.0;

/// The part of the hero's width, either side of the middle, that the text
/// stands on. Nothing but a faint haze comes into it: the test at the end
/// holds the clouds out of it at every point of their drift.
pub const TEXT_LEFT: f32 = 0.34;
pub const TEXT_RIGHT: f32 = 0.66;
/// The top and bottom of the text, as a part of the hero's height.
const TEXT_TOP: f32 = 0.12;
const TEXT_BOTTOM: f32 = 0.92;

/// How far a cloud layer drifts, as a part of the hero's width. A layer is
/// this much wider than the hero and slides along the extra.
pub const DRIFT: f32 = 0.08;

/// How a soft shape fades from its middle to its rim: (distance from the
/// middle as a part of the radius, opacity as a part of the peak). The same
/// stops are in the SVG's gradient and in the test that measures it.
const FALLOFF: [(f32, f32); 5] = [
    (0.0, 1.0),
    (0.35, 0.72),
    (0.6, 0.32),
    (0.85, 0.07),
    (1.0, 0.0),
];

/// The opacity of a soft shape at `radius` (0 at the middle, 1 at the rim), as
/// a part of its peak.
fn falloff(radius: f32) -> f32 {
    if radius >= 1.0 {
        return 0.0;
    }
    for pair in FALLOFF.windows(2) {
        let ((from, high), (to, low)) = (pair[0], pair[1]);
        if radius <= to {
            return high + (low - high) * (radius - from) / (to - from);
        }
    }
    0.0
}

/// The `<radialGradient>` of a soft shape in `tint`.
fn soft_gradient(id: &str, tint: &str) -> String {
    let stops: String = FALLOFF
        .iter()
        .map(|(at, opacity)| {
            format!(r#"<stop offset="{at}" stop-color="{tint}" stop-opacity="{opacity}"/>"#)
        })
        .collect();
    // Percentages, on a `<rect>`: this renderer reads a plain number as pixels
    // even in a box-relative gradient (a radius of 0.5 is half a pixel, and the
    // shape paints solid black), and it paints no gradient on an `<ellipse>`
    // that is box-relative at all. A rect with a radial gradient fades to
    // nothing along the ellipse inside it, which is the soft shape wanted.
    format!(r#"<radialGradient id="{id}" cx="50%" cy="50%" r="50%">{stops}</radialGradient>"#)
}

/// Which of the two cloud layers a bank of cloud is on. They drift in
/// opposite directions, so the sky is never the same twice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Depth {
    Far,
    Near,
}

/// A soft cloud, in the proportions of its layer (0 to 1 across and down).
#[derive(Clone, Copy, Debug)]
struct Bank {
    x: f32,
    y: f32,
    rx: f32,
    ry: f32,
    depth: Depth,
}

const fn bank(x: f32, y: f32, rx: f32, ry: f32, depth: Depth) -> Bank {
    Bank {
        x,
        y,
        rx,
        ry,
        depth,
    }
}

/// Where a cloud may be: in the corners and along the top and bottom edges,
/// round the text and never under it. The sparser skies use the first few.
const BANKS: [Bank; 8] = [
    bank(0.06, 0.22, 0.22, 0.34, Depth::Far),
    bank(0.94, 0.30, 0.22, 0.36, Depth::Near),
    bank(0.10, 0.86, 0.26, 0.30, Depth::Near),
    bank(0.92, 0.88, 0.26, 0.30, Depth::Far),
    bank(0.50, 0.00, 0.30, 0.14, Depth::Far),
    bank(0.00, 0.52, 0.14, 0.40, Depth::Near),
    bank(1.00, 0.58, 0.14, 0.40, Depth::Far),
    bank(0.50, 1.02, 0.30, 0.12, Depth::Near),
];

/// Fog: low, wide banks along the top and the bottom and at the sides.
const FOG: [Bank; 5] = [
    bank(0.25, 0.04, 0.45, 0.15, Depth::Far),
    bank(0.85, 0.25, 0.22, 0.32, Depth::Near),
    bank(0.12, 0.90, 0.40, 0.22, Depth::Near),
    bank(0.80, 1.00, 0.40, 0.18, Depth::Far),
    bank(0.50, 0.00, 0.60, 0.10, Depth::Near),
];

/// The lobes of a cloud: (across, down, width, height, opacity), as parts of
/// its bank's. Three overlapping soft ellipses make a bank, not a pill.
const LOBES: [(f32, f32, f32, f32, f32); 3] = [
    (0.0, 0.0, 1.0, 1.0, 1.0),
    (-0.5, 0.2, 0.65, 0.8, 0.85),
    (0.55, 0.15, 0.6, 0.75, 0.85),
];

/// One soft ellipse, in the proportions of its layer.
#[derive(Clone, Copy, Debug)]
struct Blob {
    x: f32,
    y: f32,
    rx: f32,
    ry: f32,
    opacity: f32,
    depth: Depth,
}

impl Blob {
    /// Its opacity at a point of its layer.
    fn at(&self, x: f32, y: f32) -> f32 {
        let (dx, dy) = ((x - self.x) / self.rx, (y - self.y) / self.ry);
        self.opacity * falloff((dx * dx + dy * dy).sqrt())
    }
}

/// Whether the sky is dark: night, or the hour round sunset under a sky that
/// gets the night's gradient (see [`gradient`]).
fn dark(sky: Sky, light: Light) -> bool {
    light == Light::Night
        || (light == Light::Twilight
            && !matches!(sky, Sky::Clear | Sky::MostlyClear | Sky::PartlyCloudy))
}

/// The colour and the peak opacity of a sky's clouds. Faint everywhere: a
/// cloud is a haze, not a shape.
fn cloud_style(sky: Sky, light: Light) -> (&'static str, f32) {
    if dark(sky, light) {
        let opacity = match sky {
            Sky::Thunderstorm | Sky::HeavyRain => 0.08,
            _ => 0.10,
        };
        return ("#c8d2ee", opacity);
    }
    let opacity = match sky {
        Sky::MostlyClear => 0.11,
        Sky::PartlyCloudy => 0.13,
        Sky::Thunderstorm | Sky::HeavyRain => 0.08,
        Sky::Fog => 0.10,
        _ => 0.11,
    };
    ("#ffffff", opacity)
}

fn banks(sky: Sky) -> &'static [Bank] {
    match sky {
        Sky::Clear => &[],
        Sky::MostlyClear => &BANKS[..2],
        Sky::PartlyCloudy => &BANKS[..4],
        Sky::Fog => &FOG,
        Sky::Overcast | Sky::Drizzle | Sky::Rain | Sky::FreezingRain | Sky::Snow => &BANKS[..7],
        Sky::HeavyRain | Sky::HeavySnow | Sky::Thunderstorm => &BANKS,
    }
}

/// Every soft ellipse of the clouds of a sky.
fn blobs(sky: Sky, light: Light) -> Vec<Blob> {
    let (_, opacity) = cloud_style(sky, light);
    banks(sky)
        .iter()
        .flat_map(|bank| {
            LOBES
                .iter()
                .map(move |&(dx, dy, width, height, weight)| Blob {
                    x: bank.x + dx * bank.rx,
                    y: bank.y + dy * bank.ry,
                    rx: bank.rx * width,
                    ry: bank.ry * height,
                    opacity: opacity * weight,
                    depth: bank.depth,
                })
        })
        .collect()
}

/// A cloud layer: the soft clouds of one depth, as an SVG to stretch over a
/// box [`DRIFT`] wider than the hero. `None` for a sky with no cloud at that
/// depth. They are drawn blurred (a gradient to nothing at every rim), so no
/// edge shows and nothing is cut at a border.
pub fn clouds(sky: Sky, light: Light, depth: Depth) -> Option<String> {
    let (tint, _) = cloud_style(sky, light);
    let shapes: String = blobs(sky, light)
        .iter()
        .filter(|blob| blob.depth == depth)
        .map(|blob| {
            format!(
                r##"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" fill="url(#s)" fill-opacity="{:.3}"/>"##,
                (blob.x - blob.rx) * CLOUD_BOX,
                (blob.y - blob.ry) * CLOUD_BOX,
                2.0 * blob.rx * CLOUD_BOX,
                2.0 * blob.ry * CLOUD_BOX,
                blob.opacity,
            )
        })
        .collect();
    if shapes.is_empty() {
        return None;
    }
    let gradient = soft_gradient("s", tint);
    Some(format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{CLOUD_BOX}" height="{CLOUD_BOX}" viewBox="0 0 {CLOUD_BOX} {CLOUD_BOX}" preserveAspectRatio="none"><defs>{gradient}</defs>{shapes}</svg>"#
    ))
}

/// The sun's, or the moon's, haze: a soft glow in a corner of a sky where the
/// sun or moon can be seen. (x, y, width, height) as parts of the hero, the
/// colour and the peak opacity.
fn glow(sky: Sky, light: Light) -> Option<(f32, f32, f32, f32, &'static str, f32)> {
    let fainter = if sky == Sky::PartlyCloudy { 0.7 } else { 1.0 };
    match (sky, light) {
        (Sky::Clear | Sky::MostlyClear | Sky::PartlyCloudy, Light::Day) => {
            Some((0.84, 0.14, 0.20, 0.34, "#fff0b8", 0.42 * fainter))
        }
        (Sky::Clear | Sky::MostlyClear | Sky::PartlyCloudy, Light::Twilight) => {
            Some((0.86, 0.70, 0.20, 0.34, "#ffc48a", 0.30 * fainter))
        }
        (Sky::Clear | Sky::MostlyClear | Sky::PartlyCloudy, Light::Night) => {
            Some((0.84, 0.16, 0.18, 0.30, "#dfe6ff", 0.20 * fainter))
        }
        _ => None,
    }
}

/// What stays where it is over the gradient: the sun's or the moon's haze,
/// the stars of a clear night, streaks of rain, flakes of snow and a flash of
/// lightning. A static SVG that stretches to the hero.
pub fn still(sky: Sky, light: Light) -> String {
    let mut body = String::new();
    let mut defs = String::new();
    let mut random = Scatter(match sky {
        Sky::Clear => 1,
        Sky::MostlyClear => 2,
        Sky::PartlyCloudy => 3,
        Sky::Overcast => 4,
        Sky::Fog => 5,
        Sky::Drizzle => 6,
        Sky::Rain => 7,
        Sky::HeavyRain => 8,
        Sky::FreezingRain => 9,
        Sky::Snow => 10,
        Sky::HeavySnow => 11,
        Sky::Thunderstorm => 12,
    });

    if let Some((x, y, width, height, tint, opacity)) = glow(sky, light) {
        defs.push_str(&soft_gradient("g", tint));
        body.push_str(&format!(
            r##"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" fill="url(#g)" fill-opacity="{opacity:.2}"/>"##,
            (x - width) * ART_WIDTH,
            (y - height) * ART_HEIGHT,
            2.0 * width * ART_WIDTH,
            2.0 * height * ART_HEIGHT,
        ));
    }

    let starry =
        light != Light::Day && matches!(sky, Sky::Clear | Sky::MostlyClear | Sky::PartlyCloudy);
    if starry {
        let count = if sky == Sky::Clear { 46 } else { 24 };
        for _ in 0..count {
            let (x, y) = (
                random.between(6.0, ART_WIDTH - 6.0),
                random.between(6.0, ART_HEIGHT * 0.7),
            );
            let radius = random.between(0.5, 1.6);
            let opacity = random.between(0.35, 0.9);
            body.push_str(&format!(
                r##"<circle cx="{x:.1}" cy="{y:.1}" r="{radius:.1}" fill="#ffffff" fill-opacity="{opacity:.2}"/>"##
            ));
        }
    }

    let streaks = match sky {
        Sky::Drizzle => 26,
        Sky::Rain | Sky::FreezingRain => 44,
        Sky::HeavyRain => 70,
        Sky::Thunderstorm => 60,
        _ => 0,
    };
    for _ in 0..streaks {
        let (x, y) = (
            random.between(0.0, ART_WIDTH),
            random.between(-10.0, ART_HEIGHT),
        );
        let length = if sky == Sky::Drizzle {
            random.between(5.0, 9.0)
        } else {
            random.between(10.0, 20.0)
        };
        let opacity = random.between(0.12, 0.3);
        body.push_str(&format!(
            r##"<path d="M{x:.1} {y:.1}l-{dx:.1} {length:.1}" stroke="#ffffff" stroke-opacity="{opacity:.2}" stroke-width="1.2" stroke-linecap="round" fill="none"/>"##,
            dx = length * 0.25,
        ));
    }

    let flakes = match sky {
        Sky::Snow => 36,
        Sky::HeavySnow => 64,
        _ => 0,
    };
    for _ in 0..flakes {
        let (x, y) = (
            random.between(0.0, ART_WIDTH),
            random.between(0.0, ART_HEIGHT),
        );
        let radius = random.between(1.0, 2.6);
        let opacity = random.between(0.3, 0.7);
        body.push_str(&format!(
            r##"<circle cx="{x:.1}" cy="{y:.1}" r="{radius:.1}" fill="#ffffff" fill-opacity="{opacity:.2}"/>"##
        ));
    }

    if sky == Sky::Thunderstorm {
        defs.push_str(&soft_gradient("f", "#ffe9a8"));
        body.push_str(&format!(
            r##"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" fill="url(#f)" fill-opacity="0.18"/>"##,
            (0.84 - 0.16) * ART_WIDTH,
            (0.2 - 0.34) * ART_HEIGHT,
            2.0 * 0.16 * ART_WIDTH,
            2.0 * 0.34 * ART_HEIGHT,
        ));
    }

    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{ART_WIDTH}" height="{ART_HEIGHT}" viewBox="0 0 {ART_WIDTH} {ART_HEIGHT}" preserveAspectRatio="none"><defs>{defs}</defs>{body}</svg>"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn linear(channel: u8) -> f32 {
        let value = f32::from(channel) / 255.0;
        if value <= 0.040_45 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    }

    fn luminance(colour: Color) -> f32 {
        0.2126 * linear(colour.r()) + 0.7152 * linear(colour.g()) + 0.0722 * linear(colour.b())
    }

    const SKIES: [Sky; 12] = [
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
    ];

    #[test]
    fn white_text_reads_on_every_sky() {
        for sky in SKIES {
            for light in [Light::Day, Light::Twilight, Light::Night] {
                let gradient = gradient(sky, light);
                for end in [gradient.top, gradient.bottom] {
                    let ratio = 1.05 / (luminance(end) + 0.05);
                    assert!(ratio >= 4.5, "{sky:?} {light:?} gives {ratio:.2}:1");
                }
            }
        }
    }

    const LIGHTS: [Light; 3] = [Light::Day, Light::Twilight, Light::Night];

    #[test]
    fn the_layers_are_valid_svg_for_every_sky() {
        for sky in SKIES {
            for light in LIGHTS {
                let mut layers = vec![still(sky, light)];
                layers.extend(clouds(sky, light, Depth::Far));
                layers.extend(clouds(sky, light, Depth::Near));
                for svg in layers {
                    assert!(
                        svg.starts_with("<svg") && svg.ends_with("</svg>"),
                        "{sky:?} {light:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_clear_sky_has_no_cloud_and_every_other_has_some() {
        for light in LIGHTS {
            assert!(clouds(Sky::Clear, light, Depth::Far).is_none());
            assert!(clouds(Sky::Clear, light, Depth::Near).is_none());
            for sky in SKIES.into_iter().skip(1) {
                let some = clouds(sky, light, Depth::Far).is_some()
                    || clouds(sky, light, Depth::Near).is_some();
                assert!(some, "{sky:?} {light:?} has no cloud");
            }
        }
    }

    /// The share of the sky's colour that clouds and glow cover at a point of
    /// the hero, with each layer `drift` of the way along its travel. The far
    /// layer goes one way and the near layer the other.
    fn covered(sky: Sky, light: Light, x: f32, y: f32, drift: f32) -> f32 {
        let mut clear = 1.0;
        for blob in blobs(sky, light) {
            let along = match blob.depth {
                Depth::Far => drift,
                Depth::Near => 1.0 - drift,
            };
            // A layer is `1 + DRIFT` as wide as the hero and slides left.
            let at = (x + along * DRIFT) / (1.0 + DRIFT);
            clear *= 1.0 - blob.at(at, y);
        }
        if let Some((gx, gy, width, height, _, opacity)) = glow(sky, light) {
            let (dx, dy) = ((x - gx) / width, (y - gy) / height);
            clear *= 1.0 - opacity * falloff((dx * dx + dy * dy).sqrt());
        }
        1.0 - clear
    }

    fn mix(from: u8, to: f32, share: f32) -> f32 {
        f32::from(from) * (1.0 - share) + to * share
    }

    /// White text's contrast against the sky at a point, under `share` of white.
    fn contrast_at(gradient: Gradient, y: f32, share: f32) -> f32 {
        let along = |top: u8, bottom: u8| f32::from(top) * (1.0 - y) + f32::from(bottom) * y;
        let channel = |top, bottom| {
            let base = along(top, bottom).round() as u8;
            linear(mix(base, 255.0, share).round() as u8)
        };
        let lum = 0.2126 * channel(gradient.top.r(), gradient.bottom.r())
            + 0.7152 * channel(gradient.top.g(), gradient.bottom.g())
            + 0.0722 * channel(gradient.top.b(), gradient.bottom.b());
        1.05 / (lum + 0.05)
    }

    /// The clouds never take the text's contrast below 4.5:1, wherever the
    /// text is, at every point of their drift.
    #[test]
    fn white_text_reads_through_the_clouds_at_every_point_of_their_drift() {
        for sky in SKIES {
            for light in LIGHTS {
                let gradient = gradient(sky, light);
                for step_x in 0..=12 {
                    for step_y in 0..=12 {
                        let x = TEXT_LEFT + (TEXT_RIGHT - TEXT_LEFT) * step_x as f32 / 12.0;
                        let y = TEXT_TOP + (TEXT_BOTTOM - TEXT_TOP) * step_y as f32 / 12.0;
                        for drift in [0.0, 0.25, 0.5, 0.75, 1.0] {
                            let share = covered(sky, light, x, y, drift);
                            let ratio = contrast_at(gradient, y, share);
                            assert!(
                                ratio >= 4.5,
                                "{sky:?} {light:?} at ({x:.2}, {y:.2}), drift {drift}: {ratio:.2}:1 under {share:.3} of cloud"
                            );
                        }
                    }
                }
            }
        }
    }

    /// Nothing is dense: the most any point is covered, anywhere, so that no
    /// cloud reads as a shape.
    #[test]
    fn the_clouds_stay_a_haze() {
        for sky in SKIES {
            for light in LIGHTS {
                for step_x in 0..=40 {
                    for step_y in 0..=40 {
                        let (x, y) = (step_x as f32 / 40.0, step_y as f32 / 40.0);
                        for drift in [0.0, 0.5, 1.0] {
                            let share = covered(sky, light, x, y, drift);
                            assert!(
                                share <= 0.5,
                                "{sky:?} {light:?} at ({x:.2}, {y:.2}): {share:.2} covered"
                            );
                        }
                    }
                }
            }
        }
    }
}
