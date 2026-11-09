//! Pseudo-locales: every catalogue message rewritten so that a string that
//! did not come from a catalogue stands out, and a layout that cannot take a
//! longer or right-to-left translation shows it.
//!
//! | `BINGUX_PSEUDO_LOCALE` | [`Pseudo`]          | `Delete {$name}?` becomes        |
//! | ---------------------- | ------------------- | -------------------------------- |
//! | `accented`             | `Pseudo::Accented`  | `[Ðééĺééţéé notes.txt?]`         |
//! | `rtl`                  | `Pseudo::Rtl`       | `דהךהתה notes.txt?`, right to left |
//!
//! Only the message's own words change: a placeable (a file name, a number)
//! keeps its value, as a translation would. `Accented` accents every letter,
//! doubles each vowel (about 40 % longer, as German and Finnish run) and
//! brackets the whole message, so a cut-off end shows. `Rtl` writes each
//! letter in Hebrew and turns the window right to left; placeables keep
//! their isolation marks, so a Latin file name sits in the sentence as it
//! would in a real Hebrew one.
//!
//! These are not the overflow stress values `expand`, `ar` and `he`
//! ([`crate::stress`]), which rewrite every drawn string whatever its source:
//! those test a layout, these test that the strings are localised.

use std::borrow::Cow;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicU8, Ordering};

/// A pseudo-locale: the source catalogue rewritten.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Pseudo {
    /// Accented, 40 % longer, bracketed: `[Ńééţẃööŕķ]`.
    Accented,
    /// Every letter Hebrew, read right to left.
    Rtl,
}

impl Pseudo {
    /// The value of `BINGUX_PSEUDO_LOCALE` that turns it on.
    pub const fn name(self) -> &'static str {
        match self {
            Pseudo::Accented => "accented",
            Pseudo::Rtl => "rtl",
        }
    }

    /// The pseudo-locale `name` names (`accented`, `rtl`).
    pub fn from_name(name: &str) -> Option<Pseudo> {
        match name.trim() {
            "accented" => Some(Pseudo::Accented),
            "rtl" | "bidi" => Some(Pseudo::Rtl),
            _ => None,
        }
    }

    /// The language tag it is shown as: the private-use regions Android and
    /// Chromium use for the same two (`en-XA`, `ar-XB`).
    pub const fn tag(self) -> &'static str {
        match self {
            Pseudo::Accented => "en-XA",
            Pseudo::Rtl => "ar-XB",
        }
    }

    /// The rewrite of one piece of a message's own text.
    pub(super) fn transform(self) -> fn(&str) -> Cow<'_, str> {
        match self {
            Pseudo::Accented => accented,
            Pseudo::Rtl => mirrored,
        }
    }

    /// The whole formatted message, after its pieces were rewritten.
    pub(super) fn wrap(self, text: String) -> String {
        match self {
            Pseudo::Accented => format!("[{text}]"),
            Pseudo::Rtl => text,
        }
    }
}

const OFF: u8 = 0;

static STATE: LazyLock<AtomicU8> = LazyLock::new(|| {
    let pseudo = std::env::var("BINGUX_PSEUDO_LOCALE")
        .ok()
        .and_then(|name| Pseudo::from_name(&name));
    AtomicU8::new(encode(pseudo))
});

const fn encode(pseudo: Option<Pseudo>) -> u8 {
    match pseudo {
        None => OFF,
        Some(Pseudo::Accented) => 1,
        Some(Pseudo::Rtl) => 2,
    }
}

pub(super) fn current() -> Option<Pseudo> {
    match STATE.load(Ordering::Relaxed) {
        1 => Some(Pseudo::Accented),
        2 => Some(Pseudo::Rtl),
        _ => None,
    }
}

pub(super) fn set(pseudo: Option<Pseudo>) {
    STATE.store(encode(pseudo), Ordering::Relaxed);
}

/// Every ASCII letter accented and every vowel doubled.
fn accented(text: &str) -> Cow<'_, str> {
    if !text.chars().any(|c| c.is_ascii_alphabetic()) {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len() * 3);
    for c in text.chars() {
        let accented = crate::stress::accent(c);
        out.push(accented);
        if matches!(c.to_ascii_lowercase(), 'a' | 'e' | 'i' | 'o' | 'u' | 'y') {
            out.push(accented);
        }
    }
    Cow::Owned(out)
}

/// Every ASCII letter written in Hebrew: right to left, the same length.
fn mirrored(text: &str) -> Cow<'_, str> {
    match crate::stress::in_script(text, crate::stress::Script::Hebrew) {
        Some(text) => Cow::Owned(text),
        None => Cow::Borrowed(text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accented_text_is_longer_and_has_no_plain_letter() {
        let out = accented("Network name");
        assert!(!out.chars().any(|c| c.is_ascii_alphabetic()), "{out}");
        let ratio = out.chars().count() as f32 / "Network name".chars().count() as f32;
        assert!(ratio >= 1.3, "{out} is only {ratio} times as long");
    }

    #[test]
    fn text_with_no_letter_is_left_alone() {
        assert!(matches!(accented("12 / 3"), Cow::Borrowed(_)));
        assert!(matches!(mirrored("…"), Cow::Borrowed(_)));
    }

    #[test]
    fn names_round_trip() {
        for pseudo in [Pseudo::Accented, Pseudo::Rtl] {
            assert_eq!(Pseudo::from_name(pseudo.name()), Some(pseudo));
        }
        assert_eq!(Pseudo::from_name("expand"), None, "the overflow stress value is not ours");
    }
}
