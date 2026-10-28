//! Stress switches for the overflow checks: pseudo-localisation and a text
//! scale, applied where text is made so that every label, paragraph and span
//! of every app feels them.
//!
//! * **Pseudo-locale** (`BINGUX_PSEUDO_LOCALE=expand`): every string that has
//!   a letter is rewritten to an accented, 40 % longer one, `Network name`
//!   becoming `[Ńéţẃöŕķ ńáḿé ~~~~~]`. German and Finnish run about that much
//!   longer than English, so a layout that survives it survives the
//!   translations, and an untranslated string stands out.
//! * **Text scale** (`BINGUX_TEXT_SCALE=2`): every text size of the type ramp
//!   is multiplied by the factor (the largest the accessibility page offers
//!   is 2), rounded by the label to a quarter pixel.
//!
//! Both are read from the environment once and can be set at run time with
//! [`set_pseudo_locale`] and [`set_text_scale`], as the taxonomy's header
//! switcher does; an app must then redraw. They exist in debug builds and in
//! builds with the `testing` feature (the snapshot and taxonomy binaries);
//! in a release build the functions are constants and cost nothing.
//! `scripts/overflow-check.py` runs every page under them.

use std::borrow::Cow;

/// How much longer pseudo-localised text is than the original.
pub const EXPANSION: f32 = 0.4;

/// The text scales the stress tools offer, as the accessibility page does.
pub const TEXT_SCALES: [f32; 4] = [1.0, 1.25, 1.5, 2.0];

/// The width every popup is opened at when it asks for more
/// (`BINGUX_POPUP_WIDTH=320`, the narrowest a popup can be dragged to), so
/// the overflow checks see each form at its narrowest. Debug and `testing` builds only.
pub fn popup_width() -> Option<f32> {
    #[cfg(any(debug_assertions, feature = "testing"))]
    {
        static WIDTH: std::sync::LazyLock<Option<f32>> = std::sync::LazyLock::new(|| {
            std::env::var("BINGUX_POPUP_WIDTH")
                .ok()
                .and_then(|value| value.trim().parse::<f32>().ok())
                .filter(|width| *width >= 280.0)
        });
        *WIDTH
    }
    #[cfg(not(any(debug_assertions, feature = "testing")))]
    {
        None
    }
}

#[cfg(any(debug_assertions, feature = "testing"))]
mod switches {
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::LazyLock;

    static PSEUDO: LazyLock<AtomicBool> = LazyLock::new(|| {
        AtomicBool::new(std::env::var("BINGUX_PSEUDO_LOCALE").is_ok_and(|value| value == "expand"))
    });
    static SCALE: LazyLock<AtomicU32> = LazyLock::new(|| {
        let scale = std::env::var("BINGUX_TEXT_SCALE")
            .ok()
            .and_then(|value| value.trim().parse::<f32>().ok())
            .filter(|scale| (0.5..=4.0).contains(scale))
            .unwrap_or(1.0);
        AtomicU32::new(scale.to_bits())
    });

    pub fn pseudo() -> bool {
        PSEUDO.load(Ordering::Relaxed)
    }

    pub fn set_pseudo(on: bool) {
        PSEUDO.store(on, Ordering::Relaxed);
    }

    pub fn scale() -> f32 {
        f32::from_bits(SCALE.load(Ordering::Relaxed))
    }

    pub fn set_scale(scale: f32) {
        SCALE.store(scale.clamp(0.5, 4.0).to_bits(), Ordering::Relaxed);
    }
}

#[cfg(not(any(debug_assertions, feature = "testing")))]
mod switches {
    pub fn pseudo() -> bool {
        false
    }
    pub fn set_pseudo(_: bool) {}
    pub fn scale() -> f32 {
        1.0
    }
    pub fn set_scale(_: f32) {}
}

/// Whether text is pseudo-localised now.
pub fn pseudo_locale() -> bool {
    switches::pseudo()
}

/// Turns pseudo-localisation on or off. Debug and `testing` builds only; a release build
/// ignores it.
pub fn set_pseudo_locale(on: bool) {
    switches::set_pseudo(on);
}

/// The factor the type ramp is multiplied by now, 1 when off.
pub fn text_scale() -> f32 {
    switches::scale()
}

/// Sets the text scale, 0.5 to 4. Debug and `testing` builds only; a release build ignores
/// it.
pub fn set_text_scale(scale: f32) {
    switches::set_scale(scale);
}

/// `text` as it should be drawn: itself, or its pseudo-localised form when
/// that is on. Text with no letter in it (a number, a time, an ellipsis) is
/// left as it is.
pub fn localise(text: &str) -> Cow<'_, str> {
    if cfg!(any(debug_assertions, feature = "testing")) && pseudo_locale() {
        match expand(text) {
            Some(expanded) => Cow::Owned(expanded),
            None => Cow::Borrowed(text),
        }
    } else {
        Cow::Borrowed(text)
    }
}

/// [`localise`] for an owned string: the same allocation when nothing changes.
pub fn localised(text: String) -> String {
    match localise(&text) {
        Cow::Owned(expanded) => expanded,
        Cow::Borrowed(_) => text,
    }
}

/// The tail the long-data stress adds to a name: 64 characters, one word, no
/// place to break.
pub const LONG_TAIL: &str = "-Extraordinarily-Long-Unbroken-Name-0123456789abcdefghijklmnopqr";

/// A name that comes from the machine or from a person (a network, a device,
/// a user, a file), as a stress run wants it: with `BINGUX_STRESS_DATA=long`
/// it gets [`LONG_TAIL`] after it, so it is far longer than any row and has no
/// place to break. The original stays at the front, so a script can still find
/// the row by it. Debug and `testing` builds only; otherwise `name` is returned as it is.
/// Apply it where fake or sample data is made, never to what a person typed.
pub fn data(name: impl Into<String>) -> String {
    let name = name.into();
    #[cfg(any(debug_assertions, feature = "testing"))]
    {
        static LONG: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| {
            std::env::var("BINGUX_STRESS_DATA").is_ok_and(|value| value == "long")
        });
        if *LONG && !name.is_empty() {
            return format!("{name}{LONG_TAIL}");
        }
    }
    name
}

/// The accented form of a letter: still readable, but not ASCII, so a font or
/// an encoding that cannot take it shows.
fn accent(letter: char) -> char {
    match letter {
        'a' => 'á',
        'b' => 'ƀ',
        'c' => 'ç',
        'd' => 'ð',
        'e' => 'é',
        'f' => 'ƒ',
        'g' => 'ĝ',
        'h' => 'ĥ',
        'i' => 'í',
        'j' => 'ĵ',
        'k' => 'ķ',
        'l' => 'ĺ',
        'm' => 'ḿ',
        'n' => 'ń',
        'o' => 'ö',
        'p' => 'þ',
        'q' => 'ɋ',
        'r' => 'ŕ',
        's' => 'š',
        't' => 'ţ',
        'u' => 'ü',
        'v' => 'ṽ',
        'w' => 'ẃ',
        'x' => 'ẍ',
        'y' => 'ý',
        'z' => 'ž',
        'A' => 'Á',
        'B' => 'Ɓ',
        'C' => 'Ç',
        'D' => 'Ð',
        'E' => 'É',
        'F' => 'Ƒ',
        'G' => 'Ĝ',
        'H' => 'Ĥ',
        'I' => 'Í',
        'J' => 'Ĵ',
        'K' => 'Ķ',
        'L' => 'Ĺ',
        'M' => 'Ḿ',
        'N' => 'Ń',
        'O' => 'Ö',
        'P' => 'Þ',
        'Q' => 'Ɋ',
        'R' => 'Ŕ',
        'S' => 'Š',
        'T' => 'Ţ',
        'U' => 'Ü',
        'V' => 'Ṽ',
        'W' => 'Ẃ',
        'X' => 'Ẍ',
        'Y' => 'Ý',
        'Z' => 'Ž',
        other => other,
    }
}

/// The pseudo-localised form of `text`: its letters accented and a tail of
/// `~` padding it to at least [`EXPANSION`] longer, in brackets that mark
/// where it starts and ends, so a cut-off string shows. `None` when `text`
/// has no ASCII letter to translate. The tail is separated by a space, so it
/// breaks like a further word and a wrapped label still wraps between words.
pub fn expand(text: &str) -> Option<String> {
    if !text.chars().any(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    let length = text.chars().count();
    let padding = (length as f32 * EXPANSION).ceil() as usize;
    let mut out = String::with_capacity(text.len() + padding + 4);
    out.push('[');
    out.extend(text.chars().map(accent));
    out.push(' ');
    out.extend(std::iter::repeat_n('~', padding));
    out.push(']');
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_label_is_accented_and_forty_percent_longer() {
        let expanded = expand("Network name").unwrap();
        assert_eq!(expanded, "[Ńéţẃöŕķ ńáḿé ~~~~~]");
        assert!(expanded.chars().count() as f32 >= 12.0 * (1.0 + EXPANSION));
    }

    #[test]
    fn text_without_a_letter_is_left_alone() {
        for text in ["", "12:30", "…", "100%", " – "] {
            assert_eq!(expand(text), None, "{text:?}");
        }
    }

    #[test]
    fn every_ascii_letter_is_rewritten_and_nothing_else() {
        let letters: String = ('a'..='z').chain('A'..='Z').collect();
        let expanded = expand(&letters).unwrap();
        for original in letters.chars() {
            assert_ne!(accent(original), original, "{original}");
        }
        assert_eq!(accent('1'), '1');
        assert_eq!(accent(' '), ' ');
        assert!(expanded.starts_with('['));
    }

    #[test]
    fn the_padding_is_one_word_so_a_wrapped_label_breaks_between_words() {
        let expanded = expand("Connect automatically").unwrap();
        assert!(expanded.ends_with(" ~~~~~~~~~]"), "{expanded}");
    }

    #[test]
    fn the_switches_are_off_unless_set() {
        // The environment of a test run sets neither.
        if std::env::var_os("BINGUX_PSEUDO_LOCALE").is_none() {
            assert_eq!(localise("Network name"), "Network name");
        }
        if std::env::var_os("BINGUX_TEXT_SCALE").is_none() {
            assert_eq!(text_scale(), 1.0);
        }
    }
}
