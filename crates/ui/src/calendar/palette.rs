//! Colours of calendars and their events.
//!
//! A calendar's colour is data (a hex colour another program may have written
//! to its `color` file), so it cannot be a `Paint` role. This module is where
//! the data becomes colours, and the only place in the calendar views that
//! builds one: the rest draw with the tints and inks it returns.
//!
//! The default colours are the accents, so a calendar that has no colour of
//! its own follows the desktop's palette, and in the dark scheme is lightened
//! the way the accent is.

use freya::prelude::*;

use crate::contrast::ratio_opaque;
use crate::theme::{Tokens, accent_for, accent_names};

/// The accent names a new calendar is offered, in picker order.
pub fn calendar_accents() -> Vec<&'static str> {
    accent_names().collect()
}

/// `#rrggbb` for an accent name in the light scheme: what is written to a
/// calendar's `color` file.
pub fn accent_hex(name: &str) -> String {
    hex_of(accent_for(name, false))
}

/// The accent a hex colour is, if it is one of the offered accents.
pub fn accent_of_hex(hex: &str) -> Option<&'static str> {
    let wanted = normalise_hex(hex)?;
    accent_names().find(|name| accent_hex(name) == wanted)
}

/// Reads `#rrggbb`, `rrggbb` or `#rrggbbaa`.
pub fn parse_hex(text: &str) -> Option<Color> {
    let hex = normalise_hex(text)?;
    let value = u32::from_str_radix(&hex[1..], 16).ok()?;
    Some(Color::from_rgb(
        (value >> 16) as u8,
        (value >> 8) as u8,
        value as u8,
    ))
}

fn normalise_hex(text: &str) -> Option<String> {
    let hex = text.trim().trim_start_matches('#');
    let hex = if hex.len() == 8 { &hex[..6] } else { hex };
    (hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()))
        .then(|| format!("#{}", hex.to_lowercase()))
}

/// `#rrggbb` of a colour.
pub fn hex_of(colour: Color) -> String {
    format!("#{:02x}{:02x}{:02x}", colour.r(), colour.g(), colour.b())
}

/// The colour of a calendar. `hex` is what its `color` file says; `index` is
/// its place in the list, which picks an accent when it has no colour.
pub fn calendar_colour(hex: Option<&str>, index: usize, dark: bool) -> Color {
    let names = calendar_accents();
    if let Some(hex) = hex {
        if let Some(name) = accent_of_hex(hex) {
            return accent_for(name, dark);
        }
        if let Some(colour) = parse_hex(hex) {
            return colour;
        }
    }
    accent_for(names[index % names.len()], dark)
}

/// How an event of one calendar is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EventColours {
    /// The calendar's colour: the bar down an event's edge, and the fill of a
    /// bar that spans whole days.
    pub solid: Color,
    /// The calendar's colour faintly, behind a timed event's text.
    pub tint: Color,
    /// The tint when the event is selected, or the pointer is over it.
    pub strong: Color,
    /// Text on `solid`: black or white, whichever reads better.
    pub on_solid: Color,
}

/// The colours of an event whose calendar is `base`, on the scheme `tokens`.
pub fn event_colours(tokens: &Tokens, base: Color) -> EventColours {
    let dark = tokens.scheme.is_dark();
    let on_solid = if ratio_opaque(Color::BLACK, base) >= ratio_opaque(Color::WHITE, base) {
        Color::BLACK
    } else {
        Color::WHITE
    };
    EventColours {
        solid: base,
        tint: base.with_af32(if dark { 0.30 } else { 0.20 }),
        strong: base.with_af32(if dark { 0.48 } else { 0.34 }),
        on_solid,
    }
}

/// A dot or a bar of `colour`, a plain fill for a marker.
pub fn marker(colour: Color) -> Rect {
    rect().background(colour)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_colours_are_read_in_every_form() {
        assert_eq!(
            parse_hex("#3584e4"),
            Some(Color::from_rgb(0x35, 0x84, 0xe4))
        );
        assert_eq!(parse_hex("3584E4"), Some(Color::from_rgb(0x35, 0x84, 0xe4)));
        assert_eq!(
            parse_hex("#3584e4ff"),
            Some(Color::from_rgb(0x35, 0x84, 0xe4))
        );
        assert_eq!(parse_hex("blue"), None);
    }

    #[test]
    fn an_accent_written_to_a_file_is_found_again() {
        for name in calendar_accents() {
            assert_eq!(accent_of_hex(&accent_hex(name)), Some(name));
        }
    }

    #[test]
    fn a_calendar_without_a_colour_takes_an_accent_by_place() {
        let first = calendar_colour(None, 0, false);
        let second = calendar_colour(None, 1, false);
        assert_ne!(first, second);
        assert_eq!(calendar_colour(None, 9, false), first);
    }
}
