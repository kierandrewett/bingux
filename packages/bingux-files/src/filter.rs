//! File filters, shaped like the portal's.
//!
//! `org.freedesktop.portal.FileChooser` describes a filter as
//! `(name, [(kind, pattern)])`, where `kind` 0 is a glob and 1 is a MIME
//! type. [`Filter`] and [`Rule`] are that, one to one; [`Rule::from_portal`]
//! and [`Rule::to_portal`] convert.

use crate::entry::Entry;
use crate::mime;

/// One pattern of a filter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rule {
    /// A glob on the file name: `*.png`, `report-??.txt`, `[abc]*`. Matching
    /// ignores case, as GTK's does since 4.4, so `*.png` shows `A.PNG`.
    Glob(String),
    /// A MIME type, or a group such as `image/*`; see [`mime::is_a`].
    Mime(String),
}

impl Rule {
    /// The portal's `(u, s)` pair: 0 is a glob, 1 a MIME type.
    pub fn to_portal(&self) -> (u32, String) {
        match self {
            Rule::Glob(pattern) => (0, pattern.clone()),
            Rule::Mime(mime) => (1, mime.clone()),
        }
    }

    /// The rule for a portal `(u, s)` pair, or `None` for an unknown kind.
    pub fn from_portal(kind: u32, value: &str) -> Option<Rule> {
        match kind {
            0 => Some(Rule::Glob(value.to_string())),
            1 => Some(Rule::Mime(value.to_string())),
            _ => None,
        }
    }
}

/// A named set of rules. A file passes when any rule matches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Filter {
    pub name: String,
    pub rules: Vec<Rule>,
}

impl Filter {
    pub fn new(name: impl Into<String>) -> Self {
        Filter {
            name: name.into(),
            rules: Vec::new(),
        }
    }

    /// Add a glob rule.
    pub fn glob(mut self, pattern: impl Into<String>) -> Self {
        self.rules.push(Rule::Glob(pattern.into()));
        self
    }

    /// Add a MIME rule.
    pub fn mime(mut self, mime: impl Into<String>) -> Self {
        self.rules.push(Rule::Mime(mime.into()));
        self
    }

    /// A filter from the portal's wire form.
    pub fn from_portal(name: impl Into<String>, rules: &[(u32, String)]) -> Self {
        Filter {
            name: name.into(),
            rules: rules
                .iter()
                .filter_map(|(kind, value)| Rule::from_portal(*kind, value))
                .collect(),
        }
    }

    /// The portal's wire form of the rules.
    pub fn to_portal(&self) -> (String, Vec<(u32, String)>) {
        (
            self.name.clone(),
            self.rules.iter().map(Rule::to_portal).collect(),
        )
    }

    /// Whether a file called `name` of type `mime` passes. A filter with no
    /// rules passes everything.
    pub fn accepts(&self, name: &str, mime: &str) -> bool {
        self.rules.is_empty()
            || self.rules.iter().any(|rule| match rule {
                Rule::Glob(pattern) => glob_match(pattern, name),
                Rule::Mime(wanted) => mime::is_a(mime, wanted),
            })
    }

    /// Whether `entry` is shown. Folders always are: a filter narrows files,
    /// and a person must still be able to open a folder to find one.
    pub fn accepts_entry(&self, entry: &Entry) -> bool {
        entry.is_dir() || self.accepts(&entry.name, entry.mime.mime)
    }
}

/// Match `name` against a glob of `*` (any run), `?` (one character) and
/// `[abc]`, `[a-z]`, `[!abc]` (one character from a set), ignoring case.
pub fn glob_match(pattern: &str, name: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().flat_map(char::to_lowercase).collect();
    let name: Vec<char> = name.chars().flat_map(char::to_lowercase).collect();
    let (mut p, mut n) = (0, 0);
    // Where the last `*` is, and how much of the name it has swallowed.
    let mut star: Option<(usize, usize)> = None;
    while n < name.len() {
        if let Some(&token) = pattern.get(p) {
            match token {
                '*' => {
                    star = Some((p, n));
                    p += 1;
                    continue;
                }
                '?' => {
                    p += 1;
                    n += 1;
                    continue;
                }
                '[' => match parse_class(&pattern, p, name[n]) {
                    Some((true, width)) => {
                        p += width;
                        n += 1;
                        continue;
                    }
                    Some((false, _)) => {}
                    // No closing `]`: the bracket is a plain character.
                    None if name[n] == '[' => {
                        p += 1;
                        n += 1;
                        continue;
                    }
                    None => {}
                },
                literal if literal == name[n] => {
                    p += 1;
                    n += 1;
                    continue;
                }
                _ => {}
            }
        }
        // A mismatch: let the last `*` swallow one more character.
        match star {
            Some((star_p, star_n)) => {
                p = star_p + 1;
                n = star_n + 1;
                star = Some((star_p, star_n + 1));
            }
            None => return false,
        }
    }
    pattern[p..].iter().all(|c| *c == '*')
}

/// Whether `c` is in the class that starts at `pattern[start]` (a `[`), and
/// how many pattern characters the class takes. `None` when it has no
/// closing `]`.
fn parse_class(pattern: &[char], start: usize, c: char) -> Option<(bool, usize)> {
    let mut first = start + 1;
    let negate = matches!(pattern.get(first), Some('!' | '^'));
    if negate {
        first += 1;
    }
    // A `]` straight after the opening is a member, not the end.
    let mut end = first + 1;
    while end < pattern.len() && pattern[end] != ']' {
        end += 1;
    }
    if end >= pattern.len() {
        return None;
    }
    let set = &pattern[first..end];
    let mut hit = false;
    let mut index = 0;
    while index < set.len() {
        if index + 2 < set.len() && set[index + 1] == '-' {
            hit |= set[index] <= c && c <= set[index + 2];
            index += 3;
        } else {
            hit |= set[index] == c;
            index += 1;
        }
    }
    Some((hit != negate, end + 1 - start))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globs() {
        assert!(glob_match("*.png", "A.PNG"));
        assert!(glob_match("*.conf", "wg0.conf"));
        assert!(!glob_match("*.conf", "wg0.conf.bak"));
        assert!(glob_match("report-??.txt", "report-12.txt"));
        assert!(glob_match("[a-c]*", "beta"));
        assert!(!glob_match("[!a-c]*", "beta"));
        assert!(glob_match("*", ""));
        assert!(glob_match("a*b*c", "aXXbYYc"));
        assert!(!glob_match("a*b*c", "aXXbYY"));
    }

    #[test]
    fn portal_round_trip() {
        let filter = Filter::new("VPN").glob("*.ovpn").mime("text/plain");
        let (name, rules) = filter.to_portal();
        assert_eq!(Filter::from_portal(name, &rules), filter);
        assert!(filter.accepts("x.ovpn", "application/x-openvpn-profile"));
        assert!(filter.accepts("notes", "text/markdown"));
        assert!(!filter.accepts("a.png", "image/png"));
    }
}
