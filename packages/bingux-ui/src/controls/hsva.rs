//! The colour picker's model: a colour held as hue, saturation, value and
//! alpha.
//!
//! The picker edits this, never an RGB colour it converts back from. RGB
//! cannot say which hue a grey was on or which saturation a black had, so a
//! picker that re-derives hue, saturation and value from the colour it just
//! made loses them the moment the pointer touches an edge: the hue thumb snaps
//! to red when the saturation reaches 0, and the saturation thumb snaps to the
//! left when the value reaches 0. Here the three numbers are the truth and the
//! RGB colour is only ever produced from them.
//!
//! A colour that arrives from outside (a swatch, a typed hex, the caller's
//! value) is taken in with [`Hsva::editing`], which keeps whatever the new
//! colour cannot say.

use freya::prelude::Color;

use crate::style::colour_from_argb;

/// The top of the hue range, in degrees. 360 is the same red as 0, and is
/// kept as 360 so a hue thumb dragged to the far end stays at the far end.
pub(crate) const HUE_MAX: f32 = 360.0;

/// A colour as hue (degrees), saturation and value (0 to 1) and alpha.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Hsva {
    pub hue: f32,
    pub sat: f32,
    pub val: f32,
    pub alpha: u8,
}

impl Hsva {
    /// A colour, with each part clamped to its range. A NaN becomes 0.
    pub(crate) fn new(hue: f32, sat: f32, val: f32, alpha: u8) -> Self {
        Self {
            hue: clamp(hue, 0.0, HUE_MAX),
            sat: clamp(sat, 0.0, 1.0),
            val: clamp(val, 0.0, 1.0),
            alpha,
        }
    }

    /// The hue, saturation and value that make `colour`. A grey has hue 0 and
    /// a black has saturation 0 here; use [`editing`](Self::editing) when
    /// there is a colour already being edited.
    pub(crate) fn from_colour(colour: Color) -> Self {
        let (hue, sat, val) = rgb_to_hsv(colour.r(), colour.g(), colour.b());
        Self::new(hue, sat, val, colour.a())
    }

    /// `colour`, as an edit of `self`. When `colour` is what `self` makes,
    /// `self` is returned untouched, so a caller that hands the picker's own
    /// colour back changes nothing. Otherwise the new colour's hue is kept from
    /// `self` if it is a grey (it has none), and its saturation as well if it
    /// is black (it has none either).
    pub(crate) fn editing(self, colour: Color) -> Self {
        if self.colour() == colour {
            return self;
        }
        if self.rgb() == (colour.r(), colour.g(), colour.b()) {
            return Self {
                alpha: colour.a(),
                ..self
            };
        }
        let found = Self::from_colour(colour);
        Self {
            hue: if found.sat == 0.0 || found.val == 0.0 {
                self.hue
            } else {
                found.hue
            },
            sat: if found.val == 0.0 { self.sat } else { found.sat },
            val: found.val,
            alpha: found.alpha,
        }
    }

    /// The colour, with its alpha.
    pub(crate) fn colour(self) -> Color {
        let (r, g, b) = self.rgb();
        colour_from_argb(self.alpha, r, g, b)
    }

    /// The colour at full opacity: what the alpha strip fades in from.
    pub(crate) fn opaque(self) -> Color {
        self.with_alpha(255).colour()
    }

    /// This hue at full saturation and value: the square's pure corner.
    pub(crate) fn hue_colour(self) -> Color {
        Self::new(self.hue, 1.0, 1.0, 255).colour()
    }

    /// The red, green and blue of the colour.
    pub(crate) fn rgb(self) -> (u8, u8, u8) {
        hsv_to_rgb(self.hue, self.sat, self.val)
    }

    pub(crate) fn with_hue(self, hue: f32) -> Self {
        Self::new(hue, self.sat, self.val, self.alpha)
    }

    pub(crate) fn with_sv(self, sat: f32, val: f32) -> Self {
        Self::new(self.hue, sat, val, self.alpha)
    }

    pub(crate) fn with_alpha(self, alpha: u8) -> Self {
        Self { alpha, ..self }
    }
}

fn clamp(value: f32, low: f32, high: f32) -> f32 {
    if value.is_nan() {
        low
    } else {
        value.clamp(low, high)
    }
}

/// Hue in degrees, saturation and value from 8-bit channels.
fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let span = max - min;
    let sat = if max > 0.0 { span / max } else { 0.0 };
    let hue = if span <= 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / span).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / span + 2.0)
    } else {
        60.0 * ((r - g) / span + 4.0)
    };
    (hue, sat, max)
}

/// 8-bit channels from hue in degrees, saturation and value. 360 degrees is
/// 0 degrees.
fn hsv_to_rgb(hue: f32, sat: f32, val: f32) -> (u8, u8, u8) {
    let sector = hue.rem_euclid(HUE_MAX) / 60.0;
    let chroma = val * sat;
    let second = chroma * (1.0 - (sector % 2.0 - 1.0).abs());
    let floor = val - chroma;
    let (r, g, b) = match sector as u32 {
        0 => (chroma, second, 0.0),
        1 => (second, chroma, 0.0),
        2 => (0.0, chroma, second),
        3 => (0.0, second, chroma),
        4 => (second, 0.0, chroma),
        _ => (chroma, 0.0, second),
    };
    let channel = |part: f32| ((part + floor) * 255.0).round().clamp(0.0, 255.0) as u8;
    (channel(r), channel(g), channel(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argb(a: u8, r: u8, g: u8, b: u8) -> Color {
        colour_from_argb(a, r, g, b)
    }

    /// Every RGB colour on a grid that includes both ends of each channel.
    fn grid() -> impl Iterator<Item = (u8, u8, u8)> {
        let steps: Vec<u8> = (0..=255u32)
            .filter(|v| v % 5 == 0 || *v == 255)
            .map(|v| v as u8)
            .collect();
        steps.clone().into_iter().flat_map(move |r| {
            let steps = steps.clone();
            steps.clone().into_iter().flat_map(move |g| {
                steps.clone().into_iter().map(move |b| (r, g, b))
            })
        })
    }

    #[test]
    fn rgb_survives_a_round_trip_through_hsv() {
        for (r, g, b) in grid() {
            let (h, s, v) = rgb_to_hsv(r, g, b);
            assert_eq!(hsv_to_rgb(h, s, v), (r, g, b), "{r},{g},{b} via {h},{s},{v}");
        }
        for grey in 0..=255u8 {
            let (h, s, v) = rgb_to_hsv(grey, grey, grey);
            assert_eq!(hsv_to_rgb(h, s, v), (grey, grey, grey));
        }
    }

    #[test]
    fn a_colour_is_unchanged_by_being_edited_with_itself() {
        // The caller hands the picker's own colour back after each change; that
        // must never move a thumb, whatever the colour is.
        let states = [
            Hsva::new(211.0, 0.77, 0.89, 255),
            Hsva::new(211.0, 0.0, 0.5, 255),
            Hsva::new(211.0, 0.6, 0.0, 255),
            Hsva::new(360.0, 1.0, 1.0, 128),
            Hsva::new(0.0, 0.0, 0.0, 0),
            Hsva::new(123.456, 0.001, 0.999, 7),
        ];
        for state in states {
            assert_eq!(state.editing(state.colour()), state);
        }
    }

    #[test]
    fn hue_is_kept_when_saturation_reaches_zero() {
        let blue = Hsva::new(211.0, 0.77, 0.89, 255);
        let grey = blue.with_sv(0.0, 0.5);
        assert_eq!(grey.hue, 211.0);
        // The caller echoes the grey back: the hue is still 211, not 0.
        let echoed = grey.editing(grey.colour());
        assert_eq!(echoed.hue, 211.0);
        // And a grey that comes from outside borrows the hue being edited.
        let outside = blue.editing(argb(255, 128, 128, 128));
        assert_eq!((outside.hue, outside.sat), (211.0, 0.0));
        assert_eq!(outside.colour(), argb(255, 128, 128, 128));
    }

    #[test]
    fn hue_and_saturation_are_kept_when_value_reaches_zero() {
        let blue = Hsva::new(211.0, 0.77, 0.89, 255);
        let black = blue.with_sv(0.77, 0.0);
        assert_eq!(black.colour(), argb(255, 0, 0, 0));
        assert_eq!((black.hue, black.sat), (211.0, 0.77));
        let outside = blue.editing(argb(255, 0, 0, 0));
        assert_eq!((outside.hue, outside.sat, outside.val), (211.0, 0.77, 0.0));
    }

    #[test]
    fn a_new_chromatic_colour_replaces_the_hue() {
        let blue = Hsva::new(211.0, 0.77, 0.89, 255);
        let green = blue.editing(argb(255, 48, 209, 88));
        assert!((green.hue - 135.0).abs() < 2.0, "{}", green.hue);
        assert_eq!(green.colour(), argb(255, 48, 209, 88));
    }

    #[test]
    fn hue_360_is_red_and_stays_360() {
        let end = Hsva::new(360.0, 1.0, 1.0, 255);
        assert_eq!(end.colour(), Hsva::new(0.0, 1.0, 1.0, 255).colour());
        assert_eq!(end.with_sv(0.5, 0.5).hue, 360.0);
        assert_eq!(Hsva::new(999.0, 0.5, 0.5, 255).hue, 360.0);
        assert_eq!(Hsva::new(-5.0, 0.5, 0.5, 255).hue, 0.0);
    }

    #[test]
    fn parts_are_clamped_and_nan_is_zero() {
        let wild = Hsva::new(f32::NAN, 2.0, -1.0, 9);
        assert_eq!((wild.hue, wild.sat, wild.val), (0.0, 1.0, 0.0));
        assert_eq!(Hsva::new(10.0, f32::NAN, f32::NAN, 9).sat, 0.0);
    }

    #[test]
    fn alpha_is_independent_of_the_colour() {
        let state = Hsva::new(211.0, 0.77, 0.89, 255);
        let faint = state.with_alpha(40);
        assert_eq!(faint.rgb(), state.rgb());
        assert_eq!(faint.opaque(), state.colour());
        // An outside colour with the same rgb but another alpha keeps the
        // exact hue, saturation and value being edited.
        let edited = state.editing(faint.colour());
        assert_eq!(edited, faint);
    }

    #[test]
    fn primaries_have_the_hues_the_strip_shows() {
        for (hue, (r, g, b)) in [
            (0.0, (255, 0, 0)),
            (60.0, (255, 255, 0)),
            (120.0, (0, 255, 0)),
            (180.0, (0, 255, 255)),
            (240.0, (0, 0, 255)),
            (300.0, (255, 0, 255)),
            (360.0, (255, 0, 0)),
        ] {
            assert_eq!(Hsva::new(hue, 1.0, 1.0, 255).rgb(), (r, g, b), "hue {hue}");
        }
    }
}
