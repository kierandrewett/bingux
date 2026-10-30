//! The colour picker's model: a colour held as hue, saturation, value and
//! alpha, with exact conversions to and from the forms a person types.
//!
//! The picker edits this, never an RGB colour it converts back from. RGB
//! cannot say which hue a grey was on or which saturation a black had, so a
//! picker that re-derives hue, saturation and value from the colour it just
//! made loses them the moment the pointer touches an edge: the hue thumb snaps
//! to red when the saturation reaches 0, and the saturation thumb to the left
//! when the value reaches 0. Here the three numbers are the truth and the RGB
//! colour is only ever produced from them.
//!
//! A colour that arrives from outside (a swatch, a typed hex, the caller's
//! value) is taken in with [`Hsva::editing`], which keeps whatever the new
//! colour cannot say.
//!
//! Everything here is pure: no toolkit, no state. [`Mode`] and its field
//! functions are the numeric boxes' arithmetic (RGB, HSL and HSV as whole
//! numbers); [`parse_hex`] is the hex box's grammar; [`push_recent`] is the
//! recent-colours list's rule.

use freya::prelude::Color;

use crate::style::colour_from_argb;

/// The top of the hue range, in degrees. 360 is the same red as 0, and is
/// kept as 360 so a hue thumb dragged to the far end stays at the far end.
pub const HUE_MAX: f32 = 360.0;

/// A colour as hue (degrees), saturation and value (0 to 1) and alpha
/// (0 to 255). Every constructor clamps each part into its range.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hsva {
    pub hue: f32,
    pub sat: f32,
    pub val: f32,
    pub alpha: u8,
}

impl Hsva {
    /// A colour, with each part clamped to its range. A NaN becomes 0.
    pub fn new(hue: f32, sat: f32, val: f32, alpha: u8) -> Self {
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
    pub fn from_colour(colour: Color) -> Self {
        let (hue, sat, val) = rgb_to_hsv(colour.r(), colour.g(), colour.b());
        Self::new(hue, sat, val, colour.a())
    }

    /// `colour`, as an edit of `self`. When `colour` is what `self` makes,
    /// `self` is returned untouched, so a caller that hands the picker's own
    /// colour back changes nothing. Otherwise the new colour's hue is kept from
    /// `self` if it is a grey (it has none), and its saturation as well if it
    /// is black (it has none either).
    pub fn editing(self, colour: Color) -> Self {
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
    pub fn colour(self) -> Color {
        let (r, g, b) = self.rgb();
        colour_from_argb(self.alpha, r, g, b)
    }

    /// The colour at full opacity: what the alpha strip fades in from.
    pub fn opaque(self) -> Color {
        self.with_alpha(255).colour()
    }

    /// This hue at full saturation and value: the plane's pure corner.
    pub fn hue_colour(self) -> Color {
        Self::new(self.hue, 1.0, 1.0, 255).colour()
    }

    /// The red, green and blue of the colour.
    pub fn rgb(self) -> (u8, u8, u8) {
        hsv_to_rgb(self.hue, self.sat, self.val)
    }

    pub fn with_hue(self, hue: f32) -> Self {
        Self::new(hue, self.sat, self.val, self.alpha)
    }

    pub fn with_sv(self, sat: f32, val: f32) -> Self {
        Self::new(self.hue, sat, val, self.alpha)
    }

    pub fn with_alpha(self, alpha: u8) -> Self {
        Self { alpha, ..self }
    }

    /// The same colour with no transparency: a picker that has no alpha
    /// strip only ever makes opaque colours.
    pub fn solid(self) -> Self {
        self.with_alpha(255)
    }

    /// Hue in degrees, saturation and lightness (0 to 1) of the colour as
    /// HSL. The hue is the model's own, so a grey keeps the hue it was on.
    pub fn hsl(self) -> (f32, f32, f32) {
        let (r, g, b) = self.rgb();
        let (_, sat, light) = rgb_to_hsl(r, g, b);
        // HSL and HSV share a hue; a grey has none of its own in RGB, so the
        // one being edited is the hue.
        let chromatic = sat > 0.0 && light > 0.0 && light < 1.0;
        (if chromatic { rgb_hue(r, g, b) } else { self.hue }, sat, light)
    }

    /// A colour from HSL, with each part clamped. The colour is the nearest
    /// 8-bit RGB one; the hue stays as given, so HSL entry of a grey keeps
    /// its hue.
    pub fn from_hsl(hue: f32, sat: f32, light: f32, alpha: u8) -> Self {
        let hue = clamp(hue, 0.0, HUE_MAX);
        let (r, g, b) = hsl_to_rgb(hue, clamp(sat, 0.0, 1.0), clamp(light, 0.0, 1.0));
        let found = Self::from_colour(colour_from_argb(alpha, r, g, b));
        Self {
            hue,
            sat: found.sat,
            val: found.val,
            alpha,
        }
    }

    /// A colour from RGB channels, taking the hue and saturation `self` has
    /// where the channels cannot say one (a grey, a black).
    pub fn with_rgb(self, r: u8, g: u8, b: u8) -> Self {
        self.editing(colour_from_argb(self.alpha, r, g, b))
    }

    /// A colour from float channels that may lie outside the sRGB gamut, such
    /// as the result of a wide-gamut or out-of-range computation. Each channel
    /// is clamped into 0 to 1 and the answer says whether any was moved.
    pub fn from_float_rgb(r: f32, g: f32, b: f32, alpha: f32) -> Gamut {
        let inside = |c: f32| c.is_finite() && (0.0..=1.0).contains(&c);
        let clipped = !(inside(r) && inside(g) && inside(b));
        let byte = |c: f32| (clamp(c, 0.0, 1.0) * 255.0).round() as u8;
        let a = byte(alpha);
        Gamut {
            colour: Self::from_colour(colour_from_argb(a, byte(r), byte(g), byte(b))),
            clipped,
        }
    }

    /// The colour as the hex text the picker's field shows: `#rrggbb`, or
    /// `#rrggbbaa` when `with_alpha` and the colour is translucent.
    pub fn to_hex(self, with_alpha: bool) -> String {
        let (r, g, b) = self.rgb();
        if with_alpha && self.alpha != 255 {
            format!("#{r:02x}{g:02x}{b:02x}{:02x}", self.alpha)
        } else {
            format!("#{r:02x}{g:02x}{b:02x}")
        }
    }

    /// The colour a hex entry says, as an edit of `self`: see [`parse_hex`]
    /// for what is accepted. Digits that say no alpha keep `self`'s.
    pub fn edited_by_hex(self, text: &str) -> Result<Self, HexError> {
        let hex = parse_hex(text)?;
        let alpha = hex.alpha.unwrap_or(self.alpha);
        Ok(self.editing(colour_from_argb(alpha, hex.r, hex.g, hex.b)))
    }
}

/// A float colour brought into gamut: see [`Hsva::from_float_rgb`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gamut {
    pub colour: Hsva,
    /// Whether a channel lay outside 0 to 1 (or was not a number) and was moved.
    pub clipped: bool,
}

// ---- Hex -----------------------------------------------------------------------

/// A parsed hex colour: the channels, and the alpha if the digits said one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hex {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    /// `None` for three and six digits, which say no alpha.
    pub alpha: Option<u8>,
}

/// Why a hex entry is not a colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HexError {
    /// Nothing but, perhaps, the `#`.
    Empty,
    /// A character that is not a hex digit.
    BadDigit(char),
    /// The wrong number of digits: not 3, 4, 6 or 8.
    Length(usize),
}

impl HexError {
    /// What the field says about the entry, short enough for one line.
    pub fn message(self) -> String {
        match self {
            HexError::Empty => "Enter a hex colour, such as #3584e4.".to_string(),
            HexError::BadDigit(c) => format!("\u{201c}{c}\u{201d} is not a hex digit."),
            HexError::Length(n) => format!("{n} digits: use 3, 4, 6 or 8."),
        }
    }
}

/// Reads a hex colour: an optional `#` and then 3 (`rgb`), 4 (`rgba`), 6
/// (`rrggbb`) or 8 (`rrggbbaa`) hex digits, in either case, with white space
/// around it ignored. This is what a paste of `#3584E4 ` or `3584e4` is.
pub fn parse_hex(text: &str) -> Result<Hex, HexError> {
    let text = text.trim();
    let digits = text.strip_prefix('#').unwrap_or(text).trim_start();
    if digits.is_empty() {
        return Err(HexError::Empty);
    }
    if let Some(bad) = digits.chars().find(|c| !c.is_ascii_hexdigit()) {
        return Err(HexError::BadDigit(bad));
    }
    let count = digits.chars().count();
    let value = u32::from_str_radix(digits, 16).map_err(|_| HexError::Length(count))?;
    let double = |shift: u32| (((value >> shift) & 0xf) as u8) * 17;
    Ok(match count {
        3 => Hex { r: double(8), g: double(4), b: double(0), alpha: None },
        4 => Hex { r: double(12), g: double(8), b: double(4), alpha: Some(double(0)) },
        6 => Hex {
            r: (value >> 16) as u8,
            g: (value >> 8) as u8,
            b: value as u8,
            alpha: None,
        },
        8 => Hex {
            r: (value >> 24) as u8,
            g: (value >> 16) as u8,
            b: (value >> 8) as u8,
            alpha: Some(value as u8),
        },
        n => return Err(HexError::Length(n)),
    })
}

// ---- Numeric modes -------------------------------------------------------------

/// What the three numeric boxes show.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mode {
    Rgb,
    Hsl,
    Hsv,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::Rgb, Mode::Hsl, Mode::Hsv];

    /// The key of the mode's segment and its label.
    pub fn key(self) -> &'static str {
        match self {
            Mode::Rgb => "rgb",
            Mode::Hsl => "hsl",
            Mode::Hsv => "hsv",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Mode::Rgb => "RGB",
            Mode::Hsl => "HSL",
            Mode::Hsv => "HSV",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Mode::ALL.into_iter().find(|mode| mode.key() == key)
    }

    /// The names of the three boxes, in order.
    pub fn letters(self) -> [&'static str; 3] {
        match self {
            Mode::Rgb => ["R", "G", "B"],
            Mode::Hsl => ["H", "S", "L"],
            Mode::Hsv => ["H", "S", "V"],
        }
    }

    /// The spoken names of the three boxes.
    pub fn names(self) -> [&'static str; 3] {
        match self {
            Mode::Rgb => ["Red", "Green", "Blue"],
            Mode::Hsl => ["Hue", "Saturation", "Lightness"],
            Mode::Hsv => ["Hue", "Saturation", "Value"],
        }
    }

    /// The largest whole number each box takes: 255 for a channel, 360 for a
    /// hue and 100 for a percentage. The least is always 0.
    pub fn maxima(self) -> [i32; 3] {
        match self {
            Mode::Rgb => [255, 255, 255],
            Mode::Hsl | Mode::Hsv => [360, 100, 100],
        }
    }

    /// The whole numbers the boxes show for `colour`.
    pub fn fields(self, colour: Hsva) -> [i32; 3] {
        match self {
            Mode::Rgb => {
                let (r, g, b) = colour.rgb();
                [r as i32, g as i32, b as i32]
            }
            Mode::Hsl => {
                let (h, s, l) = colour.hsl();
                [h.round() as i32, (s * 100.0).round() as i32, (l * 100.0).round() as i32]
            }
            Mode::Hsv => [
                colour.hue.round() as i32,
                (colour.sat * 100.0).round() as i32,
                (colour.val * 100.0).round() as i32,
            ],
        }
    }

    /// `colour` with box `index` (0 to 2) set to `value`, clamped into the
    /// box's range. The other boxes keep the values they show, so editing one
    /// does not move another, and a grey keeps the hue it was on.
    pub fn edited(self, colour: Hsva, index: usize, value: i32) -> Hsva {
        let mut fields = self.fields(colour);
        let Some(slot) = fields.get_mut(index) else {
            return colour;
        };
        *slot = value.clamp(0, self.maxima()[index]);
        let [a, b, c] = fields;
        match self {
            Mode::Rgb => {
                // Only the edited channel's change is a change: the other two
                // are the colour's own, so a rounding never creeps in.
                if (a as u8, b as u8, c as u8) == colour.rgb() {
                    colour
                } else {
                    colour.with_rgb(a as u8, b as u8, c as u8)
                }
            }
            Mode::Hsl => Hsva::from_hsl(a as f32, b as f32 / 100.0, c as f32 / 100.0, colour.alpha),
            Mode::Hsv => Hsva::new(a as f32, b as f32 / 100.0, c as f32 / 100.0, colour.alpha),
        }
    }
}

/// The percentage the alpha box shows: 0 to 100.
pub fn alpha_percent(alpha: u8) -> i32 {
    (alpha as f32 / 255.0 * 100.0).round() as i32
}

/// The alpha a percentage makes.
pub fn alpha_from_percent(percent: i32) -> u8 {
    (percent.clamp(0, 100) as f32 / 100.0 * 255.0).round() as u8
}

// ---- Recent colours ------------------------------------------------------------

/// `list` with `colour` at the front: a colour already in it moves to the
/// front, and the list is held to `max` colours, oldest dropped.
pub fn push_recent(list: &[Color], colour: Color, max: usize) -> Vec<Color> {
    let mut next = Vec::with_capacity(list.len().min(max) + 1);
    next.push(colour);
    next.extend(list.iter().copied().filter(|seen| *seen != colour));
    next.truncate(max);
    next
}

// ---- Conversions ---------------------------------------------------------------

fn clamp(value: f32, low: f32, high: f32) -> f32 {
    if value.is_nan() {
        low
    } else {
        value.clamp(low, high)
    }
}

/// Hue in degrees, saturation and value from 8-bit channels.
pub(super) fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let max = r.max(g).max(b) as f32 / 255.0;
    let min = r.min(g).min(b) as f32 / 255.0;
    let sat = if max > 0.0 { (max - min) / max } else { 0.0 };
    (rgb_hue(r, g, b), sat, max)
}

/// The hue, in degrees, of 8-bit channels; 0 for a grey.
fn rgb_hue(r: u8, g: u8, b: u8) -> f32 {
    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let max = r.max(g).max(b);
    let span = max - r.min(g).min(b);
    if span <= 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / span).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / span + 2.0)
    } else {
        60.0 * ((r - g) / span + 4.0)
    }
}

/// 8-bit channels from hue in degrees, saturation and value. 360 degrees is
/// 0 degrees.
pub(super) fn hsv_to_rgb(hue: f32, sat: f32, val: f32) -> (u8, u8, u8) {
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

/// Hue in degrees, saturation and lightness from 8-bit channels.
pub(super) fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let (rf, gf, bf) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let light = (max + min) / 2.0;
    let span = max - min;
    let sat = if span <= 0.0 {
        0.0
    } else {
        span / (1.0 - (2.0 * light - 1.0).abs())
    };
    (rgb_hue(r, g, b), sat.clamp(0.0, 1.0), light)
}

/// 8-bit channels from hue in degrees, saturation and lightness.
pub(super) fn hsl_to_rgb(hue: f32, sat: f32, light: f32) -> (u8, u8, u8) {
    let chroma = (1.0 - (2.0 * light - 1.0).abs()) * sat;
    let sector = hue.rem_euclid(HUE_MAX) / 60.0;
    let second = chroma * (1.0 - (sector % 2.0 - 1.0).abs());
    let floor = light - chroma / 2.0;
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
            steps
                .clone()
                .into_iter()
                .flat_map(move |g| steps.clone().into_iter().map(move |b| (r, g, b)))
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
    fn rgb_survives_a_round_trip_through_hsl() {
        for (r, g, b) in grid() {
            let (h, s, l) = rgb_to_hsl(r, g, b);
            assert_eq!(hsl_to_rgb(h, s, l), (r, g, b), "{r},{g},{b} via {h},{s},{l}");
        }
        for grey in 0..=255u8 {
            let (h, s, l) = rgb_to_hsl(grey, grey, grey);
            assert_eq!(hsl_to_rgb(h, s, l), (grey, grey, grey));
        }
    }

    #[test]
    fn rgb_survives_the_model_in_every_mode_on_the_whole_grid() {
        // Through the model and the whole numbers a person sees: RGB is exact;
        // HSL and HSV are at most a channel step off, as whole percentages.
        for (r, g, b) in grid() {
            let colour = Hsva::from_colour(argb(255, r, g, b));
            assert_eq!(colour.rgb(), (r, g, b));
            let [rr, gg, bb] = Mode::Rgb.fields(colour);
            assert_eq!((rr, gg, bb), (r as i32, g as i32, b as i32));
            for mode in [Mode::Hsl, Mode::Hsv] {
                let shown = mode.fields(colour);
                let edited = mode.edited(colour, 0, shown[0]);
                let (er, eg, eb) = edited.rgb();
                for (a, b) in [(er, r), (eg, g), (eb, b)] {
                    assert!(
                        (a as i32 - b as i32).abs() <= 5,
                        "{mode:?} {shown:?} moved ({r},{g},{b}) to ({er},{eg},{eb})"
                    );
                }
            }
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
        assert_eq!(Hsva::new(f32::INFINITY, 0.5, 0.5, 9).hue, 360.0);
        assert_eq!(Hsva::new(f32::NEG_INFINITY, 0.5, 0.5, 9).hue, 0.0);
    }

    #[test]
    fn alpha_is_independent_of_the_colour() {
        let state = Hsva::new(211.0, 0.77, 0.89, 255);
        let faint = state.with_alpha(40);
        assert_eq!(faint.rgb(), state.rgb());
        assert_eq!(faint.opaque(), state.colour());
        assert_eq!(faint.solid(), state);
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

    #[test]
    fn hsl_agrees_with_the_known_colours() {
        let red = Hsva::new(0.0, 1.0, 1.0, 255);
        assert_eq!(red.hsl(), (0.0, 1.0, 0.5));
        let white = Hsva::new(40.0, 0.0, 1.0, 255);
        // A white keeps the hue being edited, with no saturation.
        assert_eq!(white.hsl(), (40.0, 0.0, 1.0));
        let black = Hsva::new(40.0, 0.5, 0.0, 255);
        let (h, s, l) = black.hsl();
        assert_eq!((h, s, l), (40.0, 0.0, 0.0));
        let mid = Hsva::from_hsl(210.0, 0.5, 0.5, 255);
        assert_eq!(mid.rgb(), (64, 128, 191));
    }

    #[test]
    fn editing_one_box_leaves_the_others_as_shown() {
        let colour = Hsva::from_colour(argb(255, 53, 132, 228));
        let moved = Mode::Rgb.edited(colour, 0, 200);
        assert_eq!(moved.rgb(), (200, 132, 228));
        let moved = Mode::Hsv.edited(colour, 1, 0);
        assert_eq!(moved.sat, 0.0);
        // The hue is still the colour's own: a grey made by the S box keeps it.
        assert_eq!(moved.hue.round(), Mode::Hsv.fields(colour)[0] as f32);
        let moved = Mode::Hsl.edited(colour, 2, 100);
        assert_eq!(moved.rgb(), (255, 255, 255));
        assert_eq!(Mode::Hsl.fields(moved)[0], Mode::Hsl.fields(colour)[0]);
    }

    #[test]
    fn a_box_clamps_what_is_typed_into_its_range() {
        let colour = Hsva::from_colour(argb(255, 53, 132, 228));
        assert_eq!(Mode::Rgb.edited(colour, 0, 999).rgb().0, 255);
        assert_eq!(Mode::Rgb.edited(colour, 2, -40).rgb().2, 0);
        assert_eq!(Mode::Hsv.edited(colour, 0, 9999).hue, 360.0);
        assert_eq!(Mode::Hsv.edited(colour, 1, 101).sat, 1.0);
        assert_eq!(Mode::Hsl.edited(colour, 1, -1).sat, 0.0);
        // A box that is not there changes nothing.
        assert_eq!(Mode::Rgb.edited(colour, 3, 7), colour);
    }

    #[test]
    fn an_untouched_rgb_box_keeps_the_exact_hsv() {
        // Setting a channel to the value it already shows must not turn the
        // hue, saturation and value into their RGB-rounded selves.
        let colour = Hsva::new(211.3, 0.77, 0.89, 255);
        let (r, _, _) = colour.rgb();
        assert_eq!(Mode::Rgb.edited(colour, 0, r as i32), colour);
    }

    #[test]
    fn alpha_percent_round_trips() {
        for percent in 0..=100 {
            assert_eq!(alpha_percent(alpha_from_percent(percent)), percent);
        }
        assert_eq!(alpha_from_percent(-5), 0);
        assert_eq!(alpha_from_percent(500), 255);
        assert_eq!(alpha_percent(255), 100);
        assert_eq!(alpha_percent(0), 0);
    }

    // ---- Hex ---------------------------------------------------------------

    #[test]
    fn hex_accepts_three_four_six_and_eight_digits_with_or_without_a_hash() {
        let blue = Hex { r: 0x35, g: 0x84, b: 0xe4, alpha: None };
        assert_eq!(parse_hex("#3584e4"), Ok(blue));
        assert_eq!(parse_hex("3584e4"), Ok(blue));
        assert_eq!(parse_hex("#3584E4"), Ok(blue));
        assert_eq!(parse_hex("  #3584e4\n"), Ok(blue));
        assert_eq!(
            parse_hex("#3584e480"),
            Ok(Hex { alpha: Some(0x80), ..blue })
        );
        assert_eq!(
            parse_hex("#fc0"),
            Ok(Hex { r: 0xff, g: 0xcc, b: 0x00, alpha: None })
        );
        assert_eq!(
            parse_hex("fc08"),
            Ok(Hex { r: 0xff, g: 0xcc, b: 0x00, alpha: Some(0x88) })
        );
    }

    #[test]
    fn hex_says_why_an_entry_is_not_a_colour() {
        assert_eq!(parse_hex(""), Err(HexError::Empty));
        assert_eq!(parse_hex("#"), Err(HexError::Empty));
        assert_eq!(parse_hex("   "), Err(HexError::Empty));
        assert_eq!(parse_hex("#gg0000"), Err(HexError::BadDigit('g')));
        assert_eq!(parse_hex("#12345"), Err(HexError::Length(5)));
        assert_eq!(parse_hex("#1234567"), Err(HexError::Length(7)));
        assert_eq!(parse_hex("#123456789"), Err(HexError::Length(9)));
        assert_eq!(parse_hex("#12"), Err(HexError::Length(2)));
        assert_eq!(parse_hex("##fff"), Err(HexError::BadDigit('#')));
        assert_eq!(parse_hex("#-fff"), Err(HexError::BadDigit('-')));
        assert_eq!(parse_hex("#+fff"), Err(HexError::BadDigit('+')));
        assert_eq!(parse_hex("0xfff"), Err(HexError::BadDigit('x')));
        assert!(!HexError::Length(5).message().is_empty());
        assert!(HexError::BadDigit('g').message().contains('g'));
        // Non-ASCII digits are not hex digits, and do not panic.
        assert_eq!(parse_hex("#\u{ff11}\u{ff12}\u{ff13}"), Err(HexError::BadDigit('\u{ff11}')));
    }

    #[test]
    fn hex_round_trips_every_colour_on_the_grid() {
        for (r, g, b) in grid() {
            for alpha in [0u8, 1, 127, 128, 254, 255] {
                let colour = Hsva::from_colour(argb(alpha, r, g, b));
                let text = colour.to_hex(true);
                let back = Hsva::from_colour(colour.colour()).edited_by_hex(&text).unwrap();
                assert_eq!(back.colour(), argb(alpha, r, g, b), "{text}");
                // Without alpha the text is six digits and the alpha is kept.
                let six = colour.to_hex(false);
                assert_eq!(six.len(), 7);
                assert_eq!(colour.edited_by_hex(&six).unwrap().colour(), argb(alpha, r, g, b));
            }
        }
    }

    #[test]
    fn hex_text_is_lower_case_and_opaque_colours_have_six_digits() {
        let colour = Hsva::from_colour(argb(255, 0xAB, 0xCD, 0xEF));
        assert_eq!(colour.to_hex(true), "#abcdef");
        assert_eq!(colour.with_alpha(0x80).to_hex(true), "#abcdef80");
        assert_eq!(colour.with_alpha(0x80).to_hex(false), "#abcdef");
    }

    #[test]
    fn a_hex_entry_that_says_no_alpha_keeps_the_alpha_being_edited() {
        let editing = Hsva::new(211.0, 0.5, 0.5, 90);
        assert_eq!(
            editing.edited_by_hex("#ff0000").unwrap().colour(),
            argb(90, 255, 0, 0)
        );
        assert_eq!(editing.edited_by_hex("f00").unwrap().colour(), argb(90, 255, 0, 0));
        assert_eq!(
            editing.edited_by_hex("#ff000080").unwrap().colour(),
            argb(0x80, 255, 0, 0)
        );
        assert_eq!(editing.edited_by_hex("#f008").unwrap().colour(), argb(0x88, 255, 0, 0));
        assert!(editing.edited_by_hex("#ff00").is_ok(), "four digits are rgba");
        assert_eq!(editing.edited_by_hex("#ff000"), Err(HexError::Length(5)));
    }

    #[test]
    fn a_hex_entry_of_a_grey_keeps_the_hue_being_edited() {
        let editing = Hsva::new(211.0, 0.5, 0.5, 255);
        let grey = editing.edited_by_hex("#808080").unwrap();
        assert_eq!((grey.hue, grey.sat), (211.0, 0.0));
    }

    // ---- Gamut ---------------------------------------------------------------

    #[test]
    fn a_colour_outside_the_gamut_is_clamped_and_says_so() {
        let inside = Hsva::from_float_rgb(0.2, 0.4, 0.6, 1.0);
        assert!(!inside.clipped);
        assert_eq!(inside.colour.rgb(), (51, 102, 153));
        let hot = Hsva::from_float_rgb(1.2, -0.1, 0.5, 2.0);
        assert!(hot.clipped);
        assert_eq!(hot.colour.colour(), argb(255, 255, 0, 128));
        let broken = Hsva::from_float_rgb(f32::NAN, f32::INFINITY, 0.0, f32::NAN);
        assert!(broken.clipped);
        assert_eq!(broken.colour.colour(), argb(0, 0, 255, 0));
    }

    // ---- Recent colours ------------------------------------------------------

    #[test]
    fn a_recent_colour_goes_to_the_front_once_and_the_list_is_bounded() {
        let a = argb(255, 1, 2, 3);
        let b = argb(255, 4, 5, 6);
        let c = argb(255, 7, 8, 9);
        let list = push_recent(&[], a, 3);
        assert_eq!(list, vec![a]);
        let list = push_recent(&list, b, 3);
        assert_eq!(list, vec![b, a]);
        // A colour seen before moves up and is not listed twice.
        let list = push_recent(&list, a, 3);
        assert_eq!(list, vec![a, b]);
        let list = push_recent(&list, c, 3);
        assert_eq!(list, vec![c, a, b]);
        // The oldest is dropped.
        let d = argb(255, 10, 11, 12);
        assert_eq!(push_recent(&list, d, 3), vec![d, c, a]);
        // A different alpha is a different colour.
        let faint = a.with_a(10);
        assert_eq!(push_recent(&list, faint, 4), vec![faint, c, a, b]);
        assert_eq!(push_recent(&list, d, 0), Vec::<Color>::new());
    }

    #[test]
    fn modes_are_found_by_their_keys() {
        for mode in Mode::ALL {
            assert_eq!(Mode::from_key(mode.key()), Some(mode));
            assert_eq!(mode.letters().len(), 3);
            assert_eq!(mode.names().len(), 3);
        }
        assert_eq!(Mode::from_key("cmyk"), None);
    }
}
