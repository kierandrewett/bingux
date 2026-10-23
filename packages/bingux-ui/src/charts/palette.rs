//! Series colours. The accent comes first. The categorical colours after it
//! sit at golden-angle hue steps from the accent, so neighbours differ, and
//! each is lifted until it reads against the surface.

use freya::prelude::Color;

use super::data::SeriesColour;
use crate::theme::Tokens;

/// Golden-angle hue steps, in degrees, so the categorical colours spread
/// evenly round the wheel.
const HUE_STEPS: [f32; 6] = [137.5, 275.0, 52.5, 190.0, 327.5, 105.0];
const SATURATION: f32 = 0.62;
const LIGHTNESS_DARK: f32 = 0.70;
const LIGHTNESS_LIGHT: f32 = 0.46;
/// How much lightness changes each time a colour is lifted for contrast.
const LIGHTNESS_STEP: f32 = 0.02;
/// Most lifts tried before giving up on contrast.
const MAX_LIFTS: usize = 24;
/// The least contrast a graphic keeps against its surface (WCAG non-text).
pub(crate) const MIN_CONTRAST: f32 = 3.0;

/// The colour of series `index`.
pub(crate) fn resolve(tokens: &Tokens, colour: SeriesColour, index: usize) -> Color {
    match colour {
        SeriesColour::Auto if index == 0 => tokens.accent,
        SeriesColour::Auto => categorical(tokens, index - 1),
        SeriesColour::Accent => tokens.accent,
        SeriesColour::Success => tokens.success,
        SeriesColour::Warning => tokens.warning,
        SeriesColour::Danger => tokens.danger,
        SeriesColour::Muted => tokens.muted,
        SeriesColour::Custom(colour) => colour,
    }
}

/// The `slot`th categorical colour. Past six, each lap shifts the lightness so
/// the repeated hues still differ.
fn categorical(tokens: &Tokens, slot: usize) -> Color {
    let (accent_hue, _, _) = to_hsl(tokens.accent);
    let hue = (accent_hue + HUE_STEPS[slot % HUE_STEPS.len()]).rem_euclid(360.0);
    let lap = (slot / HUE_STEPS.len()) as f32;
    let dark = tokens.scheme.is_dark();
    let mut lightness = if dark {
        LIGHTNESS_DARK
    } else {
        LIGHTNESS_LIGHT
    } + lap * 0.08 * if dark { -1.0 } else { 1.0 };
    lightness = lightness.clamp(0.0, 1.0);
    let mut colour = from_hsl(hue, SATURATION, lightness);
    for _ in 0..MAX_LIFTS {
        if contrast(colour, tokens.surface) >= MIN_CONTRAST {
            break;
        }
        lightness = (lightness
            + if dark {
                LIGHTNESS_STEP
            } else {
                -LIGHTNESS_STEP
            })
        .clamp(0.0, 1.0);
        colour = from_hsl(hue, SATURATION, lightness);
    }
    colour
}

/// WCAG contrast ratio between two colours, from 1 to 21.
pub(crate) fn contrast(a: Color, b: Color) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    let (high, low) = if la > lb { (la, lb) } else { (lb, la) };
    (high + 0.05) / (low + 0.05)
}

fn luminance(colour: Color) -> f32 {
    fn linear(channel: u8) -> f32 {
        let c = f32::from(channel) / 255.0;
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * linear(colour.r()) + 0.7152 * linear(colour.g()) + 0.0722 * linear(colour.b())
}

fn to_hsl(colour: Color) -> (f32, f32, f32) {
    let r = f32::from(colour.r()) / 255.0;
    let g = f32::from(colour.g()) / 255.0;
    let b = f32::from(colour.b()) / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let lightness = (max + min) / 2.0;
    let delta = max - min;
    if delta <= 0.0 {
        return (0.0, 0.0, lightness);
    }
    let saturation = delta / (1.0 - (2.0 * lightness - 1.0).abs());
    let sector = if max == r {
        ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    };
    ((sector * 60.0).rem_euclid(360.0), saturation, lightness)
}

fn from_hsl(hue: f32, saturation: f32, lightness: f32) -> Color {
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let sector = hue.rem_euclid(360.0) / 60.0;
    let x = chroma * (1.0 - (sector % 2.0 - 1.0).abs());
    let m = lightness - chroma / 2.0;
    let (r, g, b) = match sector as u32 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    Color::from_rgb(channel(r + m), channel(g + m), channel(b + m))
}

fn channel(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}
