//! The sky behind the conditions, and the colours of the scales (temperature,
//! UV, air quality): a gradient for each kind of weather by day,
//! at dawn and dusk, and by night, and the stars, cloud puffs, rain and snow
//! drawn over it.
//!
//! These are art, not interface: raw colours are allowed in this file, and
//! the test at the end holds every gradient to the contrast the text on it
//! needs (white at 4.5:1 against the lightest end).

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

/// The view box of [`art`].
pub const ART_WIDTH: f32 = 400.0;
pub const ART_HEIGHT: f32 = 240.0;

/// What is drawn over the gradient: stars in a clear night, soft cloud puffs
/// in a cloudy sky, streaks of rain and flakes of snow. Faint, so the text over
/// it keeps its contrast. An SVG that fills its box and crops to it.
pub fn art(sky: Sky, light: Light) -> String {
    let mut body = String::new();
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

    let puffs = match sky {
        Sky::Clear => 0,
        Sky::MostlyClear => 2,
        Sky::PartlyCloudy => 4,
        Sky::Fog => 0,
        Sky::Overcast | Sky::Drizzle | Sky::Rain | Sky::FreezingRain | Sky::Snow => 6,
        Sky::HeavyRain | Sky::HeavySnow | Sky::Thunderstorm => 8,
    };
    let puff_opacity = if sky == Sky::Thunderstorm || sky == Sky::HeavyRain {
        0.16
    } else {
        0.12
    };
    for _ in 0..puffs {
        let (x, y) = (
            random.between(-20.0, ART_WIDTH),
            random.between(10.0, ART_HEIGHT * 0.65),
        );
        let (width, height) = (random.between(110.0, 220.0), random.between(26.0, 48.0));
        body.push_str(&format!(
            r##"<rect x="{x:.1}" y="{y:.1}" width="{width:.1}" height="{height:.1}" rx="{r:.1}" fill="#ffffff" fill-opacity="{puff_opacity}"/>"##,
            r = height / 2.0,
        ));
    }

    if sky == Sky::Fog {
        for line in 0..6 {
            let y = 24.0 + line as f32 * 36.0;
            let (x, width) = (random.between(-30.0, 120.0), random.between(200.0, 320.0));
            body.push_str(&format!(
                r##"<rect x="{x:.1}" y="{y:.1}" width="{width:.1}" height="9" rx="4.5" fill="#ffffff" fill-opacity="0.12"/>"##
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
        let opacity = random.between(0.18, 0.4);
        body.push_str(&format!(
            r##"<path d="M{x:.1} {y:.1}l-{dx:.1} {length:.1}" stroke="#ffffff" stroke-opacity="{opacity:.2}" stroke-width="1.4" stroke-linecap="round" fill="none"/>"##,
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
        let radius = random.between(1.0, 2.8);
        let opacity = random.between(0.35, 0.8);
        body.push_str(&format!(
            r##"<circle cx="{x:.1}" cy="{y:.1}" r="{radius:.1}" fill="#ffffff" fill-opacity="{opacity:.2}"/>"##
        ));
    }

    if sky == Sky::Thunderstorm {
        body.push_str(
            r##"<path d="M268 18l-34 62h26l-14 52 48-72h-28z" fill="#ffe9a8" fill-opacity="0.16"/>"##,
        );
    }

    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{ART_WIDTH}" height="{ART_HEIGHT}" viewBox="0 0 {ART_WIDTH} {ART_HEIGHT}">{body}</svg>"#
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

    #[test]
    fn art_is_valid_svg_for_every_sky() {
        for sky in SKIES {
            let svg = art(sky, Light::Night);
            assert!(
                svg.starts_with("<svg") && svg.ends_with("</svg>"),
                "{sky:?}"
            );
        }
    }
}
