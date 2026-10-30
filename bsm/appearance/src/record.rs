//! The whitelisted appearance record a user's shell publishes to bsm.
//!
//! A record carries exactly: `color_scheme`, `accent`, `clock_format`, `reduce_motion`
//! and `show_personal_images`. It carries no paths: images arrive as separate file
//! descriptors (see [`crate::publish::Sources`]). Nothing in a record is executed,
//! interpolated into QML source, or used to open anything. Unknown keys, duplicate
//! keys, wrong types, out-of-range values and oversized input are all refused as a
//! whole; there is no partial acceptance.

use std::fmt;

use bsm_proto::control::{Appearance, ClockFormat, ColorScheme};
use serde::Deserialize;

/// Longest accepted JSON record. A valid record is under 200 bytes.
pub const MAX_RECORD_BYTES: usize = 2048;

/// Background the accent is drawn on in the greeter, per scheme. The dark tone is the
/// shell's base tone, the same one the snapshot export uses behind a missing wallpaper.
const DARK_BASE: [u8; 3] = [0x16, 0x16, 0x16];
const LIGHT_BASE: [u8; 3] = [0xf2, 0xf2, 0xf2];
/// WCAG 2.x contrast for non-text user interface components.
pub const MIN_ACCENT_CONTRAST: f64 = 3.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordError {
    TooLarge(usize),
    Json(String),
    ColorScheme(String),
    Accent(String),
    ClockFormat(String),
}

impl fmt::Display for RecordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge(size) => write!(f, "record is {size} bytes, the limit is {MAX_RECORD_BYTES}"),
            Self::Json(why) => write!(f, "record is not valid: {why}"),
            Self::ColorScheme(value) => write!(f, "color_scheme must be \"dark\" or \"light\", not {}", shown(value)),
            Self::Accent(value) => write!(f, "accent must be #rrggbb, not {}", shown(value)),
            Self::ClockFormat(value) => write!(f, "clock_format must be \"12h\" or \"24h\", not {}", shown(value)),
        }
    }
}

impl std::error::Error for RecordError {}

/// A bounded, printable rendering of a rejected value for error messages.
fn shown(value: &str) -> String {
    let text: String = value.chars().take(16).map(|c| if c.is_control() { '?' } else { c }).collect();
    if value.chars().count() > 16 { format!("\"{text}...\"") } else { format!("\"{text}\"") }
}

/// What `PublishAppearance` carries besides the images.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishRecord {
    pub appearance: Appearance,
    /// False opts the account out of showing its personal wallpaper and avatar before login.
    pub show_personal_images: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRecord {
    color_scheme: String,
    accent: String,
    clock_format: String,
    reduce_motion: bool,
    #[serde(default = "default_show")]
    show_personal_images: bool,
}

fn default_show() -> bool {
    true
}

impl PublishRecord {
    /// Parses and validates a JSON record. Duplicate and unknown keys are errors.
    pub fn from_json(text: &str) -> Result<Self, RecordError> {
        if text.len() > MAX_RECORD_BYTES {
            return Err(RecordError::TooLarge(text.len()));
        }
        let raw: RawRecord = serde_json::from_str(text).map_err(|error| RecordError::Json(error.to_string()))?;
        let color_scheme = match raw.color_scheme.as_str() {
            "dark" => ColorScheme::Dark,
            "light" => ColorScheme::Light,
            other => return Err(RecordError::ColorScheme(other.into())),
        };
        let clock_format = match raw.clock_format.as_str() {
            "12h" => ClockFormat::H12,
            "24h" => ClockFormat::H24,
            other => return Err(RecordError::ClockFormat(other.into())),
        };
        Self::new(
            Appearance { color_scheme, accent: raw.accent, clock_format, reduce_motion: raw.reduce_motion },
            raw.show_personal_images,
        )
    }

    /// Validates a typed appearance and normalises the accent (lower case, readable
    /// against the scheme background, see [`readable_accent`]).
    pub fn new(mut appearance: Appearance, show_personal_images: bool) -> Result<Self, RecordError> {
        let accent = parse_accent(&appearance.accent).ok_or_else(|| RecordError::Accent(appearance.accent.clone()))?;
        appearance.accent = readable_accent(accent, appearance.color_scheme);
        Ok(Self { appearance, show_personal_images })
    }

    /// Re-checks a record that was built by hand, for example by a test or another crate.
    pub fn validate(&self) -> Result<(), RecordError> {
        Self::new(self.appearance.clone(), self.show_personal_images).map(|_| ())
    }
}

/// `#rrggbb` (either case) to its channels. Nothing else is accepted: no shorthand, no
/// alpha, no names, no `rgb()`, no whitespace.
pub fn parse_accent(text: &str) -> Option<[u8; 3]> {
    let bytes = text.as_bytes();
    if bytes.len() != 7 || bytes[0] != b'#' {
        return None;
    }
    let mut channels = [0u8; 3];
    for (index, channel) in channels.iter_mut().enumerate() {
        let pair = text.get(1 + index * 2..3 + index * 2)?;
        if !pair.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        *channel = u8::from_str_radix(pair, 16).ok()?;
    }
    Some(channels)
}

fn linear(channel: u8) -> f64 {
    let value = f64::from(channel) / 255.0;
    if value <= 0.04045 { value / 12.92 } else { ((value + 0.055) / 1.055).powf(2.4) }
}

/// WCAG relative luminance.
pub fn luminance(rgb: [u8; 3]) -> f64 {
    0.2126 * linear(rgb[0]) + 0.7152 * linear(rgb[1]) + 0.0722 * linear(rgb[2])
}

/// WCAG contrast ratio, between 1 and 21.
pub fn contrast_ratio(a: [u8; 3], b: [u8; 3]) -> f64 {
    let (a, b) = (luminance(a), luminance(b));
    let (lighter, darker) = if a > b { (a, b) } else { (b, a) };
    (lighter + 0.05) / (darker + 0.05)
}

/// The accent as `#rrggbb`, moved towards white on a dark scheme (towards black on a
/// light one) in 5% steps until it has [`MIN_ACCENT_CONTRAST`] against the scheme
/// background. An accent that already has it is only lower-cased. The result is a
/// function of its inputs, so republishing the same record never changes it.
pub fn readable_accent(accent: [u8; 3], scheme: ColorScheme) -> String {
    let (base, towards) = match scheme {
        ColorScheme::Dark => (DARK_BASE, 255.0),
        ColorScheme::Light => (LIGHT_BASE, 0.0),
    };
    let mut result = accent;
    for step in 1..=20 {
        if contrast_ratio(result, base) >= MIN_ACCENT_CONTRAST {
            break;
        }
        let mix = f64::from(step) * 0.05;
        for (channel, source) in result.iter_mut().zip(accent) {
            *channel = (f64::from(source) * (1.0 - mix) + towards * mix).round() as u8;
        }
    }
    format!("#{:02x}{:02x}{:02x}", result[0], result[1], result[2])
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r##"{"color_scheme":"dark","accent":"#3584E4","clock_format":"24h","reduce_motion":false,"show_personal_images":true}"##;

    #[test]
    fn accepts_a_good_record_and_lowercases_the_accent() {
        let record = PublishRecord::from_json(GOOD).unwrap();
        assert_eq!(record.appearance.accent, "#3584e4");
        assert_eq!(record.appearance.color_scheme, ColorScheme::Dark);
        assert!(record.show_personal_images);
        assert_eq!(record.validate(), Ok(()));
    }

    #[test]
    fn show_personal_images_defaults_to_shown() {
        let text = r##"{"color_scheme":"light","accent":"#336699","clock_format":"12h","reduce_motion":true}"##;
        assert!(PublishRecord::from_json(text).unwrap().show_personal_images);
    }

    #[test]
    fn rejects_bad_accents() {
        for accent in ["", "#fff", "3584e4", "#3584e4 ", " #3584e4", "#3584e4ff", "#35 4e4", "#35ğ4e4", "rgb(1,2,3)", "red", "#gggggg", "#3584e4\n", "url(x)"] {
            let text = GOOD.replace("#3584E4", accent.replace('\n', "\\n").as_str());
            assert!(matches!(PublishRecord::from_json(&text), Err(RecordError::Accent(_))), "{accent:?}");
        }
    }

    #[test]
    fn rejects_unknown_enum_values() {
        assert!(matches!(PublishRecord::from_json(&GOOD.replace("dark", "blue")), Err(RecordError::ColorScheme(_))));
        assert!(matches!(PublishRecord::from_json(&GOOD.replace("24h", "25h")), Err(RecordError::ClockFormat(_))));
    }

    #[test]
    fn rejects_unknown_duplicate_and_mistyped_keys() {
        let extra = GOOD.replace("}", r##","wallpaper":"/etc/shadow"}"##);
        assert!(matches!(PublishRecord::from_json(&extra), Err(RecordError::Json(_))));
        let duplicate = GOOD.replace("{", r##"{"accent":"#000000","##);
        assert!(matches!(PublishRecord::from_json(&duplicate), Err(RecordError::Json(_))));
        let mistyped = GOOD.replace("false", "\"false\"");
        assert!(matches!(PublishRecord::from_json(&mistyped), Err(RecordError::Json(_))));
        assert!(matches!(PublishRecord::from_json("[]"), Err(RecordError::Json(_))));
        assert!(matches!(PublishRecord::from_json(""), Err(RecordError::Json(_))));
    }

    #[test]
    fn rejects_huge_values_without_parsing_them() {
        let huge = GOOD.replace("#3584E4", &"a".repeat(1_000_000));
        assert!(matches!(PublishRecord::from_json(&huge), Err(RecordError::TooLarge(_))));
        let long_error = GOOD.replace("dark", &"x".repeat(1500));
        let error = PublishRecord::from_json(&long_error).unwrap_err();
        assert!(error.to_string().len() < 200, "error text stays short");
    }

    #[test]
    fn low_contrast_accents_are_lifted_deterministically() {
        // Near-black accent on the dark scheme: unreadable as published, readable after.
        let record = PublishRecord::from_json(&GOOD.replace("#3584E4", "#101820")).unwrap();
        let accent = parse_accent(&record.appearance.accent).unwrap();
        assert!(contrast_ratio(accent, DARK_BASE) >= MIN_ACCENT_CONTRAST);
        // And on the light scheme a near-white accent is darkened.
        let light = GOOD.replace("dark", "light").replace("#3584E4", "#f8f8f0");
        let record = PublishRecord::from_json(&light).unwrap();
        let accent = parse_accent(&record.appearance.accent).unwrap();
        assert!(contrast_ratio(accent, LIGHT_BASE) >= MIN_ACCENT_CONTRAST);
        // Idempotent: publishing the adjusted record again changes nothing.
        let again = PublishRecord::new(record.appearance.clone(), true).unwrap();
        assert_eq!(again.appearance, record.appearance);
        // A readable accent is only lower-cased.
        assert_eq!(readable_accent([0x35, 0x84, 0xe4], ColorScheme::Dark), "#3584e4");
    }
}
