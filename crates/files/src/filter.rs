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

/// What a filter is called in this crate's own docs and in the portal's:
/// `FileFilter` in the design notes, [`Filter`] in code.
pub type FileFilter = Filter;

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

    /// A filter from what an app author writes: file extensions, globs and
    /// MIME types, separated by commas, semicolons or spaces.
    ///
    /// | Text | Rule |
    /// | ---- | ---- |
    /// | `png`, `.png` | glob `*.png` |
    /// | `*.png`, `report-??.txt` | glob as written |
    /// | `image/*`, `application/pdf` | MIME type |
    pub fn parse(name: impl Into<String>, spec: &str) -> Self {
        let mut filter = Filter::new(name);
        for token in spec.split(|c: char| c == ',' || c == ';' || c.is_whitespace()) {
            if token.is_empty() {
                continue;
            }
            if token.contains('/') {
                filter.rules.push(Rule::Mime(token.to_string()));
            } else if token.contains(['*', '?', '[']) {
                filter.rules.push(Rule::Glob(token.to_string()));
            } else {
                let extension = token.trim_start_matches('.');
                if !extension.is_empty() {
                    filter.rules.push(Rule::Glob(format!("*.{extension}")));
                }
            }
        }
        filter
    }

    /// Everything.
    pub fn all_files() -> Self {
        Filter::new("All files")
    }

    pub fn images() -> Self {
        Filter::new("Images").mime("image/*")
    }

    pub fn audio() -> Self {
        Filter::new("Audio").mime("audio/*")
    }

    pub fn video() -> Self {
        Filter::new("Video").mime("video/*")
    }

    pub fn documents() -> Self {
        Filter::parse(
            "Documents",
            "pdf doc docx odt rtf xls xlsx ods csv ppt pptx odp epub txt md",
        )
    }

    pub fn archives() -> Self {
        Filter::parse(
            "Archives",
            "zip tar tar.gz tgz gz xz bz2 zst 7z rar iso",
        )
    }

    pub fn text() -> Self {
        Filter::new("Text").mime("text/plain")
    }

    /// The presets of a chooser's filter menu, "All files" first.
    pub fn presets() -> Vec<Filter> {
        vec![
            Filter::all_files(),
            Filter::images(),
            Filter::audio(),
            Filter::video(),
            Filter::documents(),
            Filter::archives(),
            Filter::text(),
        ]
    }

    /// No rules: everything passes.
    pub fn is_all(&self) -> bool {
        self.rules.is_empty()
    }

    /// The rules as people write them: `*.png, image/webp`.
    pub fn patterns_text(&self) -> String {
        let mut text = String::new();
        for rule in &self.rules {
            if !text.is_empty() {
                text.push_str(", ");
            }
            match rule {
                Rule::Glob(pattern) | Rule::Mime(pattern) => text.push_str(pattern),
            }
        }
        text
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

    /// [`Filter::accepts_entry`], by its shorter name.
    pub fn matches(&self, entry: &Entry) -> bool {
        self.accepts_entry(entry)
    }

    /// The filter prepared for testing many entries: patterns are lowered
    /// and sorted into fast cases once, not once per file. [`Listing::select`]
    /// uses this.
    ///
    /// [`Listing::select`]: crate::listing::Listing::select
    pub fn matcher(&self) -> Matcher {
        let mut matcher = Matcher {
            all: self.rules.is_empty(),
            suffixes: Vec::new(),
            globs: Vec::new(),
            groups: Vec::new(),
            mimes: Vec::new(),
        };
        for rule in &self.rules {
            match rule {
                Rule::Glob(pattern) => match plain_suffix(pattern) {
                    Some(suffix) => matcher.suffixes.push(suffix),
                    None => matcher
                        .globs
                        .push(pattern.chars().flat_map(char::to_lowercase).collect()),
                },
                Rule::Mime(wanted) => match wanted.strip_suffix("/*") {
                    Some(group) => matcher.groups.push(group.to_ascii_lowercase()),
                    None => matcher.mimes.push(wanted.clone()),
                },
            }
        }
        matcher
    }
}

/// `.png` for the glob `*.png`: a star, then letters, digits, dots and
/// dashes. The common case, tested as a case-insensitive suffix.
fn plain_suffix(pattern: &str) -> Option<Vec<u8>> {
    let rest = pattern.strip_prefix('*')?;
    (!rest.is_empty()
        && rest
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b'+')))
    .then(|| rest.to_ascii_lowercase().into_bytes())
}

/// A [`Filter`] prepared for many files. See [`Filter::matcher`].
#[derive(Clone, Debug)]
pub struct Matcher {
    all: bool,
    suffixes: Vec<Vec<u8>>,
    globs: Vec<Vec<char>>,
    groups: Vec<String>,
    mimes: Vec<String>,
}

impl Matcher {
    /// Whether a file called `name` of type `mime` passes.
    pub fn accepts(&self, name: &str, mime: &str) -> bool {
        if self.all {
            return true;
        }
        let bytes = name.as_bytes();
        if self.suffixes.iter().any(|suffix| {
            bytes.len() >= suffix.len() && bytes[bytes.len() - suffix.len()..].eq_ignore_ascii_case(suffix)
        }) {
            return true;
        }
        if !self.groups.is_empty()
            && let Some((kind, _)) = mime.split_once('/')
            && self.groups.iter().any(|group| kind.eq_ignore_ascii_case(group))
        {
            return true;
        }
        if self.mimes.iter().any(|wanted| mime::is_a(mime, wanted)) {
            return true;
        }
        if self.globs.is_empty() {
            return false;
        }
        let mut stack = ['\0'; 128];
        let lowered = lower(name, &mut stack);
        let name: &[char] = match &lowered {
            Lowered::Stack(len) => &stack[..*len],
            Lowered::Heap(chars) => chars,
        };
        self.globs.iter().any(|pattern| glob_chars(pattern, name))
    }

    /// Whether `entry` is shown: folders always are.
    pub fn matches(&self, entry: &Entry) -> bool {
        entry.is_dir() || self.accepts(&entry.name, entry.mime.mime)
    }
}

enum Lowered {
    Stack(usize),
    Heap(Vec<char>),
}

/// `text` lower-cased into `stack` when it fits, else on the heap.
fn lower(text: &str, stack: &mut [char; 128]) -> Lowered {
    let mut len = 0;
    for c in text.chars().flat_map(char::to_lowercase) {
        if len == stack.len() {
            return Lowered::Heap(text.chars().flat_map(char::to_lowercase).collect());
        }
        stack[len] = c;
        len += 1;
    }
    Lowered::Stack(len)
}

/// Match `name` against a glob of `*` (any run), `?` (one character) and
/// `[abc]`, `[a-z]`, `[!abc]` (one character from a set), ignoring case.
pub fn glob_match(pattern: &str, name: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().flat_map(char::to_lowercase).collect();
    let mut stack = ['\0'; 128];
    let lowered = lower(name, &mut stack);
    let name: &[char] = match &lowered {
        Lowered::Stack(len) => &stack[..*len],
        Lowered::Heap(chars) => chars,
    };
    glob_chars(&pattern, name)
}

/// [`glob_match`] where upper and lower case differ.
pub fn glob_match_exact(pattern: &str, name: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let name: Vec<char> = name.chars().collect();
    glob_chars(&pattern, &name)
}

/// [`glob_match`] on text that is lower-case already.
fn glob_chars(pattern: &[char], name: &[char]) -> bool {
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
                '[' => match parse_class(pattern, p, name[n]) {
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

    #[test]
    fn parses_what_app_authors_write() {
        let filter = Filter::parse("Pictures", "png, .JPG;image/webp *.gif report-??.txt");
        assert_eq!(
            filter.rules,
            [
                Rule::Glob("*.png".into()),
                Rule::Glob("*.JPG".into()),
                Rule::Mime("image/webp".into()),
                Rule::Glob("*.gif".into()),
                Rule::Glob("report-??.txt".into()),
            ]
        );
        assert_eq!(filter.patterns_text(), "*.png, *.JPG, image/webp, *.gif, report-??.txt");
        assert!(Filter::parse("none", " , ; ").is_all());
    }

    fn file(name: &str) -> Entry {
        Entry::named(
            std::path::PathBuf::from("/x").join(name),
            name.to_string(),
            crate::Kind::File,
            false,
            false,
        )
    }

    #[test]
    fn presets_pick_the_right_files() {
        let names = ["a.PNG", "b.mp3", "c.mkv", "d.pdf", "e.tar.gz", "f.rs", "g.txt", "h"];
        let shown = |filter: &Filter| -> Vec<&str> {
            let matcher = filter.matcher();
            names
                .iter()
                .copied()
                .filter(|name| matcher.matches(&file(name)))
                .collect()
        };
        assert_eq!(shown(&Filter::all_files()).len(), names.len());
        assert_eq!(shown(&Filter::images()), ["a.PNG"]);
        assert_eq!(shown(&Filter::audio()), ["b.mp3"]);
        assert_eq!(shown(&Filter::video()), ["c.mkv"]);
        assert_eq!(shown(&Filter::documents()), ["d.pdf", "g.txt"]);
        assert_eq!(shown(&Filter::archives()), ["e.tar.gz"]);
        assert_eq!(shown(&Filter::text()), ["f.rs", "g.txt"]);
        assert_eq!(Filter::presets()[0].name, "All files");
    }

    #[test]
    fn the_matcher_agrees_with_the_slow_path() {
        let filters = [
            Filter::parse("x", "*.png *.jpg"),
            Filter::parse("y", "image/* application/pdf"),
            Filter::parse("z", "[a-c]*.txt report-??.* *.ÄÖ"),
            Filter::new("empty"),
        ];
        let names = [
            "A.PNG", "photo.jpg", "beta.txt", "report-12.md", "x.ä", "x.Äö", ".png", "png", "d.pdf",
            "a very long name that goes on and on and on and on and on and on and on and on and on and on and on and on and on and on and on and on.txt",
        ];
        for filter in &filters {
            let matcher = filter.matcher();
            for name in names {
                let mime = crate::mime::for_name(name).mime;
                assert_eq!(
                    matcher.accepts(name, mime),
                    filter.accepts(name, mime),
                    "{} on {name}",
                    filter.name
                );
            }
        }
    }

    #[test]
    fn folders_always_pass() {
        let mut folder = file("Pictures");
        folder.kind = crate::Kind::Directory;
        assert!(Filter::parse("x", "png").matches(&folder));
        assert!(Filter::parse("x", "png").matcher().matches(&folder));
    }
}
