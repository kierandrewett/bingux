//! How a locale writes numbers: the decimal mark and the digit grouping.
//!
//! The calculator reads `LC_ALL`, `LC_NUMERIC` and `LANG`, in that order, and
//! needs nothing else. Only the number format matters here, so the table
//! below holds the languages whose format differs from English.

use serde::{Deserialize, Serialize};

/// How the integer digits are grouped.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Grouping {
    /// 1,234,567
    Thousands,
    /// 12,34,567 (lakhs and crores).
    Indian,
    /// 1234567
    None,
}

/// The marks a locale writes numbers with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Locale {
    pub decimal: char,
    pub group: char,
    pub grouping: Grouping,
}

/// A narrow no-break space, the thin gap French and Norwegian number
/// formats use. It never breaks a line in the middle of a number.
pub const NARROW_NBSP: char = '\u{202f}';
/// A no-break space.
pub const NBSP: char = '\u{a0}';

impl Locale {
    /// English: `1,234.5`.
    pub const EN: Locale = Locale {
        decimal: '.',
        group: ',',
        grouping: Grouping::Thousands,
    };

    /// The locale of the environment, English when it names none.
    pub fn from_env() -> Locale {
        let tag = ["LC_ALL", "LC_NUMERIC", "LANG"]
            .iter()
            .filter_map(|name| std::env::var(name).ok())
            .find(|value| !value.is_empty() && value != "C" && value != "POSIX");
        match tag {
            Some(tag) => Locale::from_tag(&tag),
            None => Locale::EN,
        }
    }

    /// The locale named by a tag such as `de_DE.UTF-8`, `fr-CA` or `en_IN`.
    pub fn from_tag(tag: &str) -> Locale {
        let tag = tag.split(['.', '@']).next().unwrap_or(tag);
        let mut parts = tag.split(['_', '-']);
        let language = parts.next().unwrap_or("").to_ascii_lowercase();
        let region = parts.next().unwrap_or("").to_ascii_uppercase();
        let comma = |group: char| Locale {
            decimal: ',',
            group,
            grouping: Grouping::Thousands,
        };
        match (language.as_str(), region.as_str()) {
            ("de", "CH") | ("de", "LI") | ("it", "CH") => Locale {
                decimal: '.',
                group: '\u{2019}',
                grouping: Grouping::Thousands,
            },
            ("fr", "CA") => comma(NBSP),
            ("fr", _) | ("nb", _) | ("nn", _) | ("no", _) => comma(NARROW_NBSP),
            ("de", _)
            | ("it", _)
            | ("nl", _)
            | ("tr", _)
            | ("id", _)
            | ("da", _)
            | ("el", _)
            | ("ro", _)
            | ("hr", _)
            | ("sl", _)
            | ("sr", _)
            | ("vi", _)
            | ("is", _) => comma('.'),
            ("es", "MX") | ("es", "US") | ("es", "PR") | ("es", "GT") | ("es", "DO") => Locale::EN,
            ("es", _) | ("pt", "BR") => comma('.'),
            ("pt", _) => comma(NBSP),
            ("ru", _)
            | ("uk", _)
            | ("pl", _)
            | ("cs", _)
            | ("sk", _)
            | ("sv", _)
            | ("fi", _)
            | ("bg", _)
            | ("hu", _)
            | ("lt", _)
            | ("lv", _)
            | ("et", _)
            | ("be", _) => comma(NBSP),
            ("en", "IN")
            | ("hi", _)
            | ("bn", _)
            | ("ta", _)
            | ("te", _)
            | ("mr", _)
            | ("gu", _) => Locale {
                decimal: '.',
                group: ',',
                grouping: Grouping::Indian,
            },
            _ => Locale::EN,
        }
    }

    /// `digits` (ASCII digits only) with the group mark between the groups.
    pub fn group_digits(&self, digits: &str) -> String {
        let len = digits.len();
        let sizes: &[usize] = match self.grouping {
            Grouping::None => return digits.to_string(),
            Grouping::Thousands => &[3],
            Grouping::Indian => &[3, 2],
        };
        if len <= sizes[0] {
            return digits.to_string();
        }
        // Cut from the right: the first group is `sizes[0]` wide, every other
        // one is the last size listed.
        let mut cuts = Vec::new();
        let mut at = len;
        let mut index = 0;
        loop {
            let width = sizes[index.min(sizes.len() - 1)];
            if at <= width {
                break;
            }
            at -= width;
            cuts.push(at);
            index += 1;
        }
        let mut out = String::with_capacity(len + cuts.len() * 3);
        for (position, ch) in digits.chars().enumerate() {
            if cuts.contains(&position) {
                out.push(self.group);
            }
            out.push(ch);
        }
        out
    }
}

impl Default for Locale {
    fn default() -> Locale {
        Locale::EN
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_groups_by_thousands() {
        assert_eq!(Locale::EN.group_digits("1234567"), "1,234,567");
        assert_eq!(Locale::EN.group_digits("123"), "123");
        assert_eq!(Locale::EN.group_digits("1234"), "1,234");
        assert_eq!(Locale::EN.group_digits("123456"), "123,456");
    }

    #[test]
    fn indian_groups_twos_after_the_first_three() {
        let loc = Locale::from_tag("en_IN");
        assert_eq!(loc.group_digits("1234567"), "12,34,567");
        assert_eq!(loc.group_digits("12345678"), "1,23,45,678");
        assert_eq!(loc.group_digits("1234"), "1,234");
    }

    #[test]
    fn german_and_french_marks() {
        let de = Locale::from_tag("de_DE.UTF-8");
        assert_eq!((de.decimal, de.group), (',', '.'));
        let fr = Locale::from_tag("fr_FR");
        assert_eq!((fr.decimal, fr.group), (',', NARROW_NBSP));
        let ch = Locale::from_tag("de_CH");
        assert_eq!((ch.decimal, ch.group), ('.', '\u{2019}'));
    }

    #[test]
    fn unknown_and_posix_tags_are_english() {
        assert_eq!(Locale::from_tag("C"), Locale::EN);
        assert_eq!(Locale::from_tag("xx_YY"), Locale::EN);
    }
}
