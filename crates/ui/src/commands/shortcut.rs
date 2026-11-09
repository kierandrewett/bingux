//! A keyboard shortcut: modifiers and one key, parsed from the text people
//! read (`"Ctrl+Shift+P"`), matched against a key press and shown the way
//! GNOME shows it.
//!
//! # Matching
//!
//! - Ctrl, Alt and Super must be held exactly as written. Shift must be too,
//!   for a letter, a digit or a named key.
//! - A symbol (`?`, `/`, `+`, `[`) is the character the layout types, so its
//!   Shift is whatever that layout needs: `Ctrl+?` is Ctrl with the key that
//!   types `?`, Shift held or not. Write `Ctrl+?`, not `Ctrl+Shift+/`.
//! - Letters ignore case. A layout that types no Latin letter on the key
//!   (Cyrillic, Greek) still reaches `Ctrl+C` through the key's position,
//!   as GTK does.

use std::fmt;
use std::str::FromStr;

use freya::prelude::*;

/// The modifiers a shortcut can name. Others (Caps Lock, Num Lock) are
/// ignored when matching.
fn relevant(modifiers: Modifiers) -> Modifiers {
    modifiers & (Modifiers::CONTROL | Modifiers::ALT | Modifiers::SHIFT | Modifiers::META)
}

/// A key and the modifiers held with it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Shortcut {
    modifiers: Modifiers,
    key: Key,
}

/// Why a shortcut's text could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub text: String,
    pub reason: &'static str,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "cannot read the shortcut {:?}: {}",
            self.text, self.reason
        )
    }
}

impl std::error::Error for ParseError {}

/// Named keys a shortcut may use, with the names it is written and shown
/// with. The first name is the one shown.
const NAMED: &[(NamedKey, &[&str])] = &[
    (NamedKey::Enter, &["Enter", "Return"]),
    (NamedKey::Escape, &["Esc", "Escape"]),
    (NamedKey::Tab, &["Tab"]),
    (NamedKey::Backspace, &["Backspace"]),
    (NamedKey::Delete, &["Delete", "Del"]),
    (NamedKey::Insert, &["Insert", "Ins"]),
    (NamedKey::Home, &["Home"]),
    (NamedKey::End, &["End"]),
    (NamedKey::PageUp, &["Page Up", "PageUp", "PgUp"]),
    (NamedKey::PageDown, &["Page Down", "PageDown", "PgDn"]),
    (NamedKey::ArrowUp, &["Up", "ArrowUp"]),
    (NamedKey::ArrowDown, &["Down", "ArrowDown"]),
    (NamedKey::ArrowLeft, &["Left", "ArrowLeft"]),
    (NamedKey::ArrowRight, &["Right", "ArrowRight"]),
    (NamedKey::ContextMenu, &["Menu"]),
    (NamedKey::F1, &["F1"]),
    (NamedKey::F2, &["F2"]),
    (NamedKey::F3, &["F3"]),
    (NamedKey::F4, &["F4"]),
    (NamedKey::F5, &["F5"]),
    (NamedKey::F6, &["F6"]),
    (NamedKey::F7, &["F7"]),
    (NamedKey::F8, &["F8"]),
    (NamedKey::F9, &["F9"]),
    (NamedKey::F10, &["F10"]),
    (NamedKey::F11, &["F11"]),
    (NamedKey::F12, &["F12"]),
];

/// Characters with a name: `+` separates the parts, and a space cannot be
/// seen.
const CHAR_NAMES: &[(char, &str)] = &[('+', "Plus"), ('-', "Minus"), (' ', "Space")];

/// The order modifiers are written in, as GNOME writes them.
const MODIFIER_ORDER: [(Modifiers, &str); 4] = [
    (Modifiers::CONTROL, "Ctrl"),
    (Modifiers::ALT, "Alt"),
    (Modifiers::SHIFT, "Shift"),
    (Modifiers::META, "Super"),
];

fn modifier_named(name: &str) -> Option<Modifiers> {
    match name.to_ascii_lowercase().as_str() {
        "ctrl" | "control" | "primary" => Some(Modifiers::CONTROL),
        "alt" | "option" => Some(Modifiers::ALT),
        "shift" => Some(Modifiers::SHIFT),
        "super" | "meta" | "logo" | "cmd" => Some(Modifiers::META),
        _ => None,
    }
}

fn key_named(name: &str) -> Option<Key> {
    if let Some((named, _)) = NAMED.iter().find(|(_, names)| {
        names
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(name))
    }) {
        return Some(Key::Named(*named));
    }
    if let Some((c, _)) = CHAR_NAMES
        .iter()
        .find(|(_, n)| n.eq_ignore_ascii_case(name))
    {
        return Some(Key::Character(c.to_string()));
    }
    let mut chars = name.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if !c.is_whitespace() => Some(Key::Character(c.to_lowercase().collect())),
        _ => None,
    }
}

/// Whether a character key stands for a symbol, whose Shift belongs to the
/// layout.
fn is_symbol(text: &str) -> bool {
    text.chars()
        .next()
        .is_some_and(|c| !c.is_alphanumeric() && !c.is_whitespace())
}

/// The Latin letter or digit of a physical key (`KeyC` is `c`), for a layout
/// that types something else there.
fn code_char(code: &Code) -> Option<char> {
    let name = format!("{code:?}");
    let rest = name
        .strip_prefix("Key")
        .or_else(|| name.strip_prefix("Digit"))?;
    let mut chars = rest.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => Some(c.to_ascii_lowercase()),
        _ => None,
    }
}

impl Shortcut {
    /// A shortcut of `modifiers` and `key`. A letter key is kept in lower case.
    pub fn new(modifiers: Modifiers, key: Key) -> Self {
        let key = match key {
            Key::Character(text) => Key::Character(text.to_lowercase()),
            other => other,
        };
        Self {
            modifiers: relevant(modifiers),
            key,
        }
    }

    /// Read a shortcut written as people read it: modifiers then the key,
    /// joined by `+` (`"Ctrl+Shift+P"`, `"F2"`, `"Alt+Left"`, `"Ctrl+?"`,
    /// `"Ctrl+Plus"`). Names ignore case.
    pub fn parse(text: &str) -> Result<Self, ParseError> {
        let fail = |reason| ParseError {
            text: text.to_owned(),
            reason,
        };
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Err(fail("it is empty"));
        }
        // `Ctrl++` is Ctrl with the plus key: the last `+` is the key.
        let (head, key_text) = match trimmed.strip_suffix("++") {
            Some(head) => (head, "+"),
            None if trimmed == "+" => ("", "+"),
            None => match trimmed.rsplit_once('+') {
                Some((head, key)) => (head, key),
                None => ("", trimmed),
            },
        };
        let mut modifiers = Modifiers::empty();
        for part in head.split('+').filter(|part| !part.is_empty()) {
            let modifier = modifier_named(part.trim())
                .ok_or_else(|| fail("a modifier is not Ctrl, Alt, Shift or Super"))?;
            modifiers |= modifier;
        }
        let key = key_named(key_text.trim())
            .ok_or_else(|| fail("the key has no name bingux-ui knows"))?;
        Ok(Self::new(modifiers, key))
    }

    /// The modifiers held with the key.
    pub fn modifiers(&self) -> Modifiers {
        self.modifiers
    }

    /// The key.
    pub fn key(&self) -> &Key {
        &self.key
    }

    /// Whether the shortcut has no modifier other than Shift: a key that
    /// types, or moves, in a focused control. Such a shortcut belongs to a
    /// view, never to the window (WCAG 2.1.4).
    pub fn is_bare(&self) -> bool {
        !self
            .modifiers
            .intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::META)
            && !matches!(
                self.key,
                Key::Named(
                    NamedKey::F1
                        | NamedKey::F2
                        | NamedKey::F3
                        | NamedKey::F4
                        | NamedKey::F5
                        | NamedKey::F6
                        | NamedKey::F7
                        | NamedKey::F8
                        | NamedKey::F9
                        | NamedKey::F10
                        | NamedKey::F11
                        | NamedKey::F12
                )
            )
    }

    /// Whether `event` is this shortcut (see the module docs for the rules).
    pub fn matches(&self, event: &KeyboardEventData) -> bool {
        let held = relevant(event.modifiers);
        match (&self.key, &event.key) {
            (Key::Character(want), Key::Character(got)) => {
                let same_key = got.to_lowercase() == *want
                    || (!got.is_ascii()
                        && want.len() == 1
                        && code_char(&event.code).is_some_and(|c| want.starts_with(c)));
                if !same_key {
                    return false;
                }
                if is_symbol(want) {
                    held - Modifiers::SHIFT == self.modifiers - Modifiers::SHIFT
                } else {
                    held == self.modifiers
                }
            }
            (Key::Named(want), Key::Named(got)) => want == got && held == self.modifiers,
            _ => false,
        }
    }

    /// The key's name as shown: `P`, `F2`, `Page Up`, `?`, `Plus`.
    pub fn key_label(&self) -> String {
        match &self.key {
            Key::Named(named) => NAMED
                .iter()
                .find(|(candidate, _)| candidate == named)
                .map_or_else(|| format!("{named:?}"), |(_, names)| names[0].to_owned()),
            Key::Character(text) => {
                if let Some((_, name)) = CHAR_NAMES
                    .iter()
                    .find(|(c, _)| text.starts_with(*c) && text.len() == 1)
                {
                    (*name).to_owned()
                } else {
                    text.to_uppercase()
                }
            }
        }
    }

    /// The parts as key caps, modifiers first: `["Ctrl", "Shift", "P"]`.
    pub fn caps(&self) -> Vec<String> {
        let mut caps: Vec<String> = MODIFIER_ORDER
            .iter()
            .filter(|(modifier, _)| self.modifiers.contains(*modifier))
            .map(|(_, name)| (*name).to_owned())
            .collect();
        caps.push(self.key_label());
        caps
    }

    /// The shortcut as one line of text: `Ctrl+Shift+P`.
    pub fn label(&self) -> String {
        self.caps().join("+")
    }
}

impl fmt::Display for Shortcut {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.label())
    }
}

impl FromStr for Shortcut {
    type Err = ParseError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse(text)
    }
}

/// Anything a command accepts as a shortcut: a [`Shortcut`] or its text.
pub trait IntoShortcut {
    fn into_shortcut(self) -> Result<Shortcut, ParseError>;
}

impl IntoShortcut for Shortcut {
    fn into_shortcut(self) -> Result<Shortcut, ParseError> {
        Ok(self)
    }
}

impl IntoShortcut for &str {
    fn into_shortcut(self) -> Result<Shortcut, ParseError> {
        Shortcut::parse(self)
    }
}

impl IntoShortcut for String {
    fn into_shortcut(self) -> Result<Shortcut, ParseError> {
        Shortcut::parse(&self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(key: Key, modifiers: Modifiers) -> KeyboardEventData {
        KeyboardEventData::new(key, Code::Unidentified, modifiers)
    }

    fn character(text: &str) -> Key {
        Key::Character(text.to_owned())
    }

    // Failure modes: a written shortcut that reads as another key; a Shift a
    // layout needs for a symbol stopping the shortcut; a letter typed with Shift
    // running a Ctrl+letter shortcut; Plus being read as a separator; extra
    // modifiers running a shorter shortcut.

    #[test]
    fn reads_and_shows_what_people_write() {
        for (text, shown) in [
            ("Ctrl+Shift+P", "Ctrl+Shift+P"),
            ("ctrl+z", "Ctrl+Z"),
            ("Shift+Ctrl+z", "Ctrl+Shift+Z"),
            ("F2", "F2"),
            ("Alt+Left", "Alt+Left"),
            ("Ctrl+?", "Ctrl+?"),
            ("Ctrl++", "Ctrl+Plus"),
            ("Ctrl+Plus", "Ctrl+Plus"),
            ("Ctrl+Minus", "Ctrl+Minus"),
            ("Ctrl+PageDown", "Ctrl+Page Down"),
            ("Super+Space", "Super+Space"),
            ("Shift+Delete", "Shift+Delete"),
        ] {
            assert_eq!(Shortcut::parse(text).unwrap().label(), shown, "{text}");
        }
        assert!(Shortcut::parse("").is_err());
        assert!(Shortcut::parse("Hyper+K").is_err());
        assert!(Shortcut::parse("Ctrl+Nothing").is_err());
    }

    #[test]
    fn letters_need_their_modifiers_exactly() {
        let save = Shortcut::parse("Ctrl+S").unwrap();
        assert!(save.matches(&press(character("s"), Modifiers::CONTROL)));
        assert!(
            save.matches(&press(character("S"), Modifiers::CONTROL)),
            "Caps Lock"
        );
        assert!(!save.matches(&press(
            character("S"),
            Modifiers::CONTROL | Modifiers::SHIFT
        )));
        assert!(!save.matches(&press(character("s"), Modifiers::CONTROL | Modifiers::ALT)));
        assert!(!save.matches(&press(character("s"), Modifiers::empty())));
        let reopen = Shortcut::parse("Ctrl+Shift+T").unwrap();
        assert!(reopen.matches(&press(
            character("T"),
            Modifiers::CONTROL | Modifiers::SHIFT
        )));
        assert!(!reopen.matches(&press(character("t"), Modifiers::CONTROL)));
    }

    #[test]
    fn a_symbol_takes_whatever_shift_the_layout_needs() {
        let help = Shortcut::parse("Ctrl+?").unwrap();
        assert!(help.matches(&press(
            character("?"),
            Modifiers::CONTROL | Modifiers::SHIFT
        )));
        assert!(help.matches(&press(character("?"), Modifiers::CONTROL)));
        assert!(
            !help.matches(&press(character("?"), Modifiers::SHIFT)),
            "Shift alone types it"
        );
        assert!(!help.matches(&press(character("/"), Modifiers::CONTROL)));
        let zoom = Shortcut::parse("Ctrl+Plus").unwrap();
        assert!(zoom.matches(&press(
            character("+"),
            Modifiers::CONTROL | Modifiers::SHIFT
        )));
    }

    #[test]
    fn named_keys_match_named_keys() {
        let rename = Shortcut::parse("F2").unwrap();
        assert!(rename.matches(&press(Key::Named(NamedKey::F2), Modifiers::empty())));
        assert!(!rename.matches(&press(Key::Named(NamedKey::F2), Modifiers::SHIFT)));
        let space = Shortcut::parse("Space").unwrap();
        assert!(space.matches(&press(character(" "), Modifiers::empty())));
        assert!(
            !space.matches(&press(character(" "), Modifiers::SHIFT)),
            "Shift+Space is another key"
        );
    }

    #[test]
    fn a_layout_without_latin_letters_reaches_letters_by_position() {
        let copy = Shortcut::parse("Ctrl+C").unwrap();
        let cyrillic = KeyboardEventData::new(character("с"), Code::KeyC, Modifiers::CONTROL);
        assert!(copy.matches(&cyrillic));
        let other = KeyboardEventData::new(character("с"), Code::KeyX, Modifiers::CONTROL);
        assert!(!copy.matches(&other));
    }

    #[test]
    fn bare_keys_are_the_ones_that_type() {
        assert!(Shortcut::parse("K").unwrap().is_bare());
        assert!(Shortcut::parse("Shift+Delete").unwrap().is_bare());
        assert!(!Shortcut::parse("F2").unwrap().is_bare());
        assert!(!Shortcut::parse("Ctrl+K").unwrap().is_bare());
    }
}
