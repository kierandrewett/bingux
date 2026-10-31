//! What a person asks the file search: a typed [`Query`] and the text it is
//! written as.
//!
//! A query is the words to find in names ([`Term`]) and everything that
//! narrows or orders the answer ([`Filters`]). The text form is the one a
//! person types in the search field:
//!
//! ```text
//! annual "tax return" -draft type:document,archive ext:pdf size:>1MB
//! modified:2024 in:Documents hidden:yes case:yes match:regex sort:date
//! ```
//!
//! | Token | Means |
//! | ----- | ----- |
//! | `word`, `"a phrase"` | the name contains it (a phrase needs its words side by side) |
//! | `-word`, `-"a phrase"` | the name does not contain it |
//! | `name:*.tar.gz` | the name fits the pattern (`*`, `?`, `[a-z]`); no wildcard means contains |
//! | `ext:png,jpg` | the name ends in one of these extensions |
//! | `type:image`, `kind:` | `image`, `video`, `audio`, `document`, `archive`, `folder`, `code`; several, separated by commas, mean any of them |
//! | `size:>10MB` | `>`, `>=`, `<`, `<=`, `=`, or `1MB..1GB`; units `B K KB M MB G GB T TB` (1000) and `KiB MiB GiB TiB` (1024) |
//! | `modified:today` | `today`, `yesterday`, `week` (7 days), `month` (30 days), `year` (this calendar year), `3d` (the last 3 days), `2024`, `2024-05`, `2024-05-17`, `>2024-01-01` (since), `<2024` (before), `2024-01-01..2024-02-28` |
//! | `in:Pictures/2024` | only below this folder (relative to where the search starts; `~` and absolute paths work) |
//! | `hidden:yes`, `no`, `only` | hidden files included, left out, or the only ones |
//! | `case:yes` | upper and lower case differ |
//! | `match:starts`, `exact`, `regex` | how the words meet the name; `regex:yes` is `match:regex` |
//! | `sort:name`, `date`, `size`, `relevance` | the order of the answer |
//!
//! Keys and keywords ignore case. A token the parser cannot read (`size:big`,
//! `type:cats`) is not dropped: it stays a literal phrase, and
//! [`Query::parse_checked`] reports it as a [`Problem`] so the field can say
//! what is wrong. A key with nothing after it yet (`type:`) is ignored: it is
//! being typed.
//!
//! [`Query`] prints back with [`Display`](std::fmt::Display), and parsing the
//! printed text gives the same query. The property tests in `query_tests`
//! hold that for random queries and for random text.

use std::fmt::{self, Display, Write};

use super::civil::Moment;
use crate::mime::Category;

// ---------------------------------------------------------------------------
// The query
// ---------------------------------------------------------------------------

/// A search: words to find in names, and filters.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Query {
    pub terms: Vec<Term>,
    pub filters: Filters,
}

/// A word or a phrase the name must (or, with [`Term::exclude`], must not)
/// contain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Term {
    text: String,
    phrase: bool,
    exclude: bool,
}

impl Term {
    /// A term for `text`: a word when it can be written as one, else a
    /// phrase. Quotes in `text` are dropped. `None` when nothing is left.
    pub fn new(text: &str) -> Option<Term> {
        let cleaned: String = text.chars().filter(|c| *c != '"').collect();
        let text = cleaned.trim();
        if text.is_empty() {
            return None;
        }
        let phrase = text.chars().any(char::is_whitespace)
            || text.starts_with('-')
            || looks_like_operator(text);
        Some(Term {
            text: text.to_string(),
            phrase,
            exclude: false,
        })
    }

    /// The same term, as one the name must not contain.
    pub fn excluded(mut self) -> Term {
        self.exclude = true;
        self
    }

    /// A phrase: its words must stand side by side. `None` when empty.
    pub fn phrase(text: &str) -> Option<Term> {
        let mut term = Term::new(text)?;
        term.phrase = true;
        Some(term)
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// Written in quotes: the words stand side by side, and it is never
    /// matched loosely.
    pub fn is_phrase(&self) -> bool {
        self.phrase
    }

    /// The name must not contain it.
    pub fn is_excluded(&self) -> bool {
        self.exclude
    }
}

/// How the free words meet a file name.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NameMode {
    /// The words appear in the name, in any order; names that start with
    /// them, or have them at the start of a word, rank first; three or more
    /// letters also match in order with others between (`rpt` finds
    /// `my_report.txt`).
    #[default]
    Contains,
    /// The name starts with the words.
    Starts,
    /// The name is the words.
    Exact,
    /// The words, joined by spaces, are a regular expression.
    Regex,
}

/// The kinds of file `type:` knows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    Folder,
    Image,
    Video,
    Audio,
    /// Office documents, PDF, books, plain text and markup.
    Document,
    Archive,
    /// Source code and scripts.
    Code,
}

impl Kind {
    pub const ALL: [Kind; 7] = [
        Kind::Folder,
        Kind::Image,
        Kind::Video,
        Kind::Audio,
        Kind::Document,
        Kind::Archive,
        Kind::Code,
    ];

    /// The word `type:` takes.
    pub fn word(self) -> &'static str {
        match self {
            Kind::Folder => "folder",
            Kind::Image => "image",
            Kind::Video => "video",
            Kind::Audio => "audio",
            Kind::Document => "document",
            Kind::Archive => "archive",
            Kind::Code => "code",
        }
    }

    /// The kind a word names, in the singular or the plural.
    pub fn from_word(word: &str) -> Option<Kind> {
        Some(match word.to_ascii_lowercase().as_str() {
            "folder" | "folders" | "dir" | "directory" | "directories" => Kind::Folder,
            "image" | "images" | "picture" | "pictures" | "photo" | "photos" => Kind::Image,
            "video" | "videos" | "movie" | "movies" => Kind::Video,
            "audio" | "music" | "sound" | "sounds" => Kind::Audio,
            "document" | "documents" | "doc" | "docs" => Kind::Document,
            "archive" | "archives" | "zip" => Kind::Archive,
            "code" | "source" | "script" | "scripts" => Kind::Code,
            _ => return None,
        })
    }

    /// The kind of a MIME category, when `type:` has one for it.
    pub fn of(category: Category) -> Option<Kind> {
        Some(match category {
            Category::Folder => Kind::Folder,
            Category::Image => Kind::Image,
            Category::Video => Kind::Video,
            Category::Audio => Kind::Audio,
            Category::Document | Category::Text => Kind::Document,
            Category::Archive => Kind::Archive,
            Category::Code => Kind::Code,
            Category::Other => return None,
        })
    }
}

/// A condition on size, in bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SizeCond {
    Gt(u64),
    Ge(u64),
    Lt(u64),
    Le(u64),
    Eq(u64),
    /// Both ends included.
    Between(u64, u64),
}

impl SizeCond {
    pub fn contains(self, size: u64) -> bool {
        match self {
            SizeCond::Gt(n) => size > n,
            SizeCond::Ge(n) => size >= n,
            SizeCond::Lt(n) => size < n,
            SizeCond::Le(n) => size <= n,
            SizeCond::Eq(n) => size == n,
            SizeCond::Between(low, high) => size >= low && size <= high,
        }
    }

    /// The smallest and largest size the condition lets through, both
    /// included, when it has such a bound.
    pub fn bounds(self) -> (Option<u64>, Option<u64>) {
        match self {
            SizeCond::Gt(n) => (Some(n.saturating_add(1)), None),
            SizeCond::Ge(n) => (Some(n), None),
            SizeCond::Lt(n) => (None, Some(n.saturating_sub(1))),
            SizeCond::Le(n) => (None, Some(n)),
            SizeCond::Eq(n) => (Some(n), Some(n)),
            SizeCond::Between(low, high) => (Some(low), Some(high)),
        }
    }
}

/// A condition on the time a file was last changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modified {
    Today,
    Yesterday,
    /// In the last this many days (`week` is 7, `month` is 30).
    Days(u32),
    /// This calendar year.
    ThisYear,
    /// Within a year, a month or a day.
    On(Moment),
    /// From the start of the year, month or day on.
    Since(Moment),
    /// Before the start of the year, month or day.
    Before(Moment),
    /// From the start of the first to the end of the second.
    Between(Moment, Moment),
}

/// What to do about hidden files.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Hidden {
    /// What the view does: show them when "Show hidden files" is on.
    #[default]
    Follow,
    /// Left out, whatever the view does.
    Exclude,
    Include,
    /// Only hidden files.
    Only,
}

/// The order of the answer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortBy {
    /// Best match first; with no words, by name.
    #[default]
    Relevance,
    Name,
    /// Newest first.
    Modified,
    /// Largest first.
    Size,
}

impl SortBy {
    pub fn word(self) -> &'static str {
        match self {
            SortBy::Relevance => "relevance",
            SortBy::Name => "name",
            SortBy::Modified => "date",
            SortBy::Size => "size",
        }
    }

    pub fn from_word(word: &str) -> Option<SortBy> {
        Some(match word.to_ascii_lowercase().as_str() {
            "relevance" | "best" => SortBy::Relevance,
            "name" => SortBy::Name,
            "date" | "modified" | "time" | "newest" => SortBy::Modified,
            "size" | "largest" => SortBy::Size,
            _ => return None,
        })
    }
}

/// Everything in a query that is not a word.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filters {
    pub name_mode: NameMode,
    /// `name:` patterns; every one must fit.
    pub globs: Vec<String>,
    /// Extensions, lower case, no dot. Any one may fit.
    pub exts: Vec<String>,
    /// Any one may fit.
    pub kinds: Vec<Kind>,
    pub size: Option<SizeCond>,
    pub modified: Option<Modified>,
    /// A folder to look in, as typed.
    pub within: Option<String>,
    pub hidden: Hidden,
    pub case_sensitive: bool,
    pub sort: SortBy,
}

/// Which filter a token of the active filters stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FilterKey {
    Kind,
    Ext,
    Size,
    Modified,
    Name,
    Within,
    Hidden,
    Case,
}

impl Filters {
    /// The filters that are set, in the order they are shown. Sort is not
    /// among them: it orders the answer, it does not narrow it.
    pub fn active(&self) -> Vec<FilterKey> {
        let mut keys = Vec::new();
        if !self.kinds.is_empty() {
            keys.push(FilterKey::Kind);
        }
        if self.modified.is_some() {
            keys.push(FilterKey::Modified);
        }
        if self.size.is_some() {
            keys.push(FilterKey::Size);
        }
        if !self.exts.is_empty() {
            keys.push(FilterKey::Ext);
        }
        if self.name_mode != NameMode::Contains || !self.globs.is_empty() {
            keys.push(FilterKey::Name);
        }
        if self.within.is_some() {
            keys.push(FilterKey::Within);
        }
        if self.hidden != Hidden::Follow {
            keys.push(FilterKey::Hidden);
        }
        if self.case_sensitive {
            keys.push(FilterKey::Case);
        }
        keys
    }

    /// How many filters are set.
    pub fn count(&self) -> usize {
        self.active().len()
    }

    /// Put one filter back to unset.
    pub fn clear(&mut self, key: FilterKey) {
        match key {
            FilterKey::Kind => self.kinds.clear(),
            FilterKey::Ext => self.exts.clear(),
            FilterKey::Size => self.size = None,
            FilterKey::Modified => self.modified = None,
            FilterKey::Name => {
                self.globs.clear();
                self.name_mode = NameMode::Contains;
            }
            FilterKey::Within => self.within = None,
            FilterKey::Hidden => self.hidden = Hidden::Follow,
            FilterKey::Case => self.case_sensitive = false,
        }
    }

    /// Every filter unset. The sort stays: it is not a filter.
    pub fn clear_all(&mut self) {
        *self = Filters {
            sort: self.sort,
            ..Filters::default()
        };
    }

    /// Whether a filter needs the size or the date of a file, which costs a
    /// `stat` the walker would not otherwise make.
    pub fn needs_stat(&self) -> bool {
        self.size.is_some() || self.modified.is_some()
    }
}

impl Query {
    /// A query for these words, with no filters.
    pub fn words(text: &str) -> Query {
        Query::parse(text)
    }

    /// Nothing to look for and no filter: it matches every name.
    pub fn is_blank(&self) -> bool {
        self.terms.is_empty() && self.filters.active().is_empty()
    }

    /// Whether any word is to be found (as opposed to only filters).
    pub fn has_words(&self) -> bool {
        self.terms.iter().any(|term| !term.exclude)
    }

    /// The words that are to be found, joined by spaces.
    pub fn plain_text(&self) -> String {
        let mut text = String::new();
        for term in self.terms.iter().filter(|term| !term.exclude) {
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(&term.text);
        }
        text
    }

    /// Read the text of the search field. Whatever cannot be read stays a
    /// literal phrase.
    pub fn parse(text: &str) -> Query {
        Query::parse_checked(text).0
    }

    /// [`Query::parse`], and what could not be read.
    pub fn parse_checked(text: &str) -> (Query, Vec<Problem>) {
        let mut query = Query::default();
        let mut problems = Vec::new();
        let chars: Vec<char> = text.chars().collect();
        let mut at = 0;
        while at < chars.len() {
            if chars[at].is_whitespace() {
                at += 1;
                continue;
            }
            let start = at;
            let exclude = chars[at] == '-' && chars.get(at + 1).is_some_and(|c| !c.is_whitespace());
            if exclude {
                at += 1;
            }
            // A quoted phrase.
            if chars[at] == '"' {
                at += 1;
                let from = at;
                while at < chars.len() && chars[at] != '"' {
                    at += 1;
                }
                let phrase: String = chars[from..at].iter().collect();
                at = (at + 1).min(chars.len());
                if let Some(term) = Term::phrase(&phrase) {
                    query
                        .terms
                        .push(if exclude { term.excluded() } else { term });
                }
                continue;
            }
            // `key:value`.
            let key_end = (at..chars.len())
                .find(|i| !chars[*i].is_ascii_alphabetic())
                .unwrap_or(chars.len());
            if !exclude
                && key_end > at
                && chars.get(key_end) == Some(&':')
                && let Some(key) = Key::from_word(&chars[at..key_end].iter().collect::<String>())
            {
                let value_start = key_end + 1;
                let (value, next) = if chars.get(value_start) == Some(&'"') {
                    let from = value_start + 1;
                    let mut end = from;
                    while end < chars.len() && chars[end] != '"' {
                        end += 1;
                    }
                    (
                        chars[from..end].iter().collect::<String>(),
                        (end + 1).min(chars.len()),
                    )
                } else {
                    let mut end = value_start;
                    while end < chars.len() && !chars[end].is_whitespace() {
                        end += 1;
                    }
                    (
                        chars[value_start..end]
                            .iter()
                            .filter(|c| **c != '"')
                            .collect::<String>(),
                        end,
                    )
                };
                at = next;
                if value.trim().is_empty() {
                    // Being typed.
                    continue;
                }
                if let Err(reason) = query.filters.apply(key, value.trim()) {
                    let raw: String = chars[start..at].iter().collect();
                    problems.push(Problem {
                        key: key.word(),
                        value: value.trim().to_string(),
                        reason,
                    });
                    if let Some(term) = Term::phrase(&raw) {
                        query.terms.push(term);
                    }
                }
                continue;
            }
            // A plain word.
            let mut end = at;
            while end < chars.len() && !chars[end].is_whitespace() {
                end += 1;
            }
            let word: String = chars[at..end].iter().collect();
            at = end;
            if let Some(term) = Term::new(&word) {
                query
                    .terms
                    .push(if exclude { term.excluded() } else { term });
            }
        }
        (query, problems)
    }
}

/// The text of a search field, split where the person is still typing: what
/// is final, and the last token when it is a `key:value` that has not been
/// closed with a space yet. A search runs on the first part, so `type:imag`
/// is not looked for as a word while it is being typed.
pub fn split_pending(text: &str) -> (&str, &str) {
    if text.chars().last().is_none_or(char::is_whitespace) {
        return (text, "");
    }
    let start = text
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_whitespace())
        .map_or(0, |(at, c)| at + c.len_utf8());
    let last = &text[start..];
    // An unclosed quote makes everything after it one token.
    let quotes = text.matches('"').count();
    if quotes % 2 == 1 {
        return match text.rfind('"') {
            Some(at)
                if text[..at].ends_with(':')
                    && looks_like_operator(
                        &text[..at]
                            .rsplit(char::is_whitespace)
                            .next()
                            .map_or("", |t| t),
                    ) =>
            {
                let from = text[..at].rfind(char::is_whitespace).map_or(0, |i| i + 1);
                (&text[..from], &text[from..])
            }
            _ => (text, ""),
        };
    }
    if looks_like_operator(last.trim_start_matches('-')) && !last.starts_with('-') {
        (&text[..start], last)
    } else {
        (text, "")
    }
}

/// A token the parser could not read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    /// `size`, `type`...
    pub key: &'static str,
    pub value: String,
    /// What is wrong, in a few words: `not a size`.
    pub reason: &'static str,
}

// ---------------------------------------------------------------------------
// Keys and their values
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Key {
    Name,
    Ext,
    Type,
    Size,
    Modified,
    In,
    Hidden,
    Case,
    Match,
    Regex,
    Sort,
}

impl Key {
    fn from_word(word: &str) -> Option<Key> {
        Some(match word.to_ascii_lowercase().as_str() {
            "name" => Key::Name,
            "ext" => Key::Ext,
            "type" | "kind" => Key::Type,
            "size" => Key::Size,
            "modified" | "date" => Key::Modified,
            "in" => Key::In,
            "hidden" => Key::Hidden,
            "case" => Key::Case,
            "match" => Key::Match,
            "regex" => Key::Regex,
            "sort" => Key::Sort,
            _ => return None,
        })
    }

    fn word(self) -> &'static str {
        match self {
            Key::Name => "name",
            Key::Ext => "ext",
            Key::Type => "type",
            Key::Size => "size",
            Key::Modified => "modified",
            Key::In => "in",
            Key::Hidden => "hidden",
            Key::Case => "case",
            Key::Match => "match",
            Key::Regex => "regex",
            Key::Sort => "sort",
        }
    }
}

/// Whether `text` starts with `key:` for a key the parser reads, so that
/// written bare it would be read as a filter.
fn looks_like_operator(text: &str) -> bool {
    let Some((key, _)) = text.split_once(':') else {
        return false;
    };
    key.chars().all(|c| c.is_ascii_alphabetic()) && Key::from_word(key).is_some()
}

fn yes_no(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "yes" | "y" | "true" | "on" | "1" => Some(true),
        "no" | "n" | "false" | "off" | "0" => Some(false),
        _ => None,
    }
}

impl Filters {
    fn apply(&mut self, key: Key, value: &str) -> Result<(), &'static str> {
        match key {
            Key::Name => self.globs.push(value.to_string()),
            Key::Ext => {
                let mut found = Vec::new();
                for part in value.split(',') {
                    let ext = part.trim().trim_start_matches('.').to_lowercase();
                    if ext.is_empty()
                        || ext
                            .chars()
                            .any(|c| c.is_whitespace() || c == '/' || c == '"')
                    {
                        return Err("not an extension");
                    }
                    found.push(ext);
                }
                for ext in found {
                    if !self.exts.contains(&ext) {
                        self.exts.push(ext);
                    }
                }
            }
            Key::Type => {
                let mut found = Vec::new();
                for part in value.split(',') {
                    found.push(Kind::from_word(part.trim()).ok_or("not a type of file")?);
                }
                for kind in found {
                    if !self.kinds.contains(&kind) {
                        self.kinds.push(kind);
                    }
                }
                self.kinds.sort();
            }
            Key::Size => self.size = Some(parse_size(value).ok_or("not a size")?),
            Key::Modified => self.modified = Some(parse_modified(value).ok_or("not a date")?),
            Key::In => {
                if value.contains('"') {
                    return Err("not a folder");
                }
                self.within = Some(value.to_string());
            }
            Key::Hidden => {
                self.hidden = match value.to_ascii_lowercase().as_str() {
                    "only" => Hidden::Only,
                    "default" | "auto" => Hidden::Follow,
                    other => match yes_no(other) {
                        Some(true) => Hidden::Include,
                        Some(false) => Hidden::Exclude,
                        None => return Err("not yes, no or only"),
                    },
                }
            }
            Key::Case => self.case_sensitive = yes_no(value).ok_or("not yes or no")?,
            Key::Match => {
                self.name_mode = match value.to_ascii_lowercase().as_str() {
                    "contains" | "contain" => NameMode::Contains,
                    "starts" | "start" | "prefix" => NameMode::Starts,
                    "exact" | "is" => NameMode::Exact,
                    "regex" | "regexp" => NameMode::Regex,
                    _ => return Err("not contains, starts, exact or regex"),
                }
            }
            Key::Regex => {
                self.name_mode = if yes_no(value).ok_or("not yes or no")? {
                    NameMode::Regex
                } else {
                    NameMode::Contains
                }
            }
            Key::Sort => {
                self.sort = SortBy::from_word(value).ok_or("not relevance, name, date or size")?
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Sizes
// ---------------------------------------------------------------------------

const UNITS: [(&str, u64); 12] = [
    ("b", 1),
    ("k", 1_000),
    ("kb", 1_000),
    ("m", 1_000_000),
    ("mb", 1_000_000),
    ("g", 1_000_000_000),
    ("gb", 1_000_000_000),
    ("t", 1_000_000_000_000),
    ("tb", 1_000_000_000_000),
    ("kib", 1 << 10),
    ("mib", 1 << 20),
    ("gib", 1 << 30),
];

/// A size such as `10MB`, `1.5g`, `300`, in bytes.
pub fn parse_bytes(text: &str) -> Option<u64> {
    let text = text.trim().to_ascii_lowercase();
    let split = text
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(text.len());
    let (number, unit) = text.split_at(split);
    if number.is_empty()
        || number.matches('.').count() > 1
        || number.starts_with('.')
        || number.ends_with('.')
    {
        return None;
    }
    let factor = match unit.trim() {
        "" => 1,
        "tib" => 1 << 40,
        other => UNITS.iter().find(|(name, _)| *name == other)?.1,
    };
    if !number.contains('.') {
        return number.parse::<u64>().ok()?.checked_mul(factor);
    }
    let value: f64 = number.parse().ok()?;
    let bytes = value * factor as f64;
    (bytes.is_finite() && bytes < u64::MAX as f64).then(|| bytes.round() as u64)
}

/// A size as `size:` writes it: the largest unit that divides it exactly.
pub fn format_bytes(bytes: u64) -> String {
    if bytes == 0 {
        return "0".to_string();
    }
    for (unit, factor) in [
        ("TB", 1_000_000_000_000_u64),
        ("GB", 1_000_000_000),
        ("MB", 1_000_000),
        ("KB", 1_000),
    ] {
        if bytes % factor == 0 {
            return format!("{}{unit}", bytes / factor);
        }
    }
    format!("{bytes}B")
}

fn parse_size(value: &str) -> Option<SizeCond> {
    if let Some((low, high)) = value.split_once("..") {
        let (low, high) = (low.trim(), high.trim());
        return match (low.is_empty(), high.is_empty()) {
            (true, true) => None,
            (false, true) => Some(SizeCond::Ge(parse_bytes(low)?)),
            (true, false) => Some(SizeCond::Le(parse_bytes(high)?)),
            (false, false) => {
                let (low, high) = (parse_bytes(low)?, parse_bytes(high)?);
                (low <= high).then_some(SizeCond::Between(low, high))
            }
        };
    }
    for (prefix, make) in [
        (">=", SizeCond::Ge as fn(u64) -> SizeCond),
        ("<=", SizeCond::Le),
        (">", SizeCond::Gt),
        ("<", SizeCond::Lt),
        ("=", SizeCond::Eq),
    ] {
        if let Some(rest) = value.strip_prefix(prefix) {
            return Some(make(parse_bytes(rest)?));
        }
    }
    Some(SizeCond::Eq(parse_bytes(value)?))
}

impl Display for SizeCond {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SizeCond::Gt(n) => write!(f, ">{}", format_bytes(*n)),
            SizeCond::Ge(n) => write!(f, ">={}", format_bytes(*n)),
            SizeCond::Lt(n) => write!(f, "<{}", format_bytes(*n)),
            SizeCond::Le(n) => write!(f, "<={}", format_bytes(*n)),
            SizeCond::Eq(n) => write!(f, "={}", format_bytes(*n)),
            SizeCond::Between(low, high) => {
                write!(f, "{}..{}", format_bytes(*low), format_bytes(*high))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Dates
// ---------------------------------------------------------------------------

fn parse_modified(value: &str) -> Option<Modified> {
    let lower = value.to_ascii_lowercase();
    match lower.as_str() {
        "today" => return Some(Modified::Today),
        "yesterday" => return Some(Modified::Yesterday),
        "week" => return Some(Modified::Days(7)),
        "month" => return Some(Modified::Days(30)),
        "year" => return Some(Modified::ThisYear),
        _ => {}
    }
    if let Some(days) = ["days", "day", "d"]
        .iter()
        .find_map(|unit| lower.strip_suffix(unit))
        && !days.is_empty()
        && days.chars().all(|c| c.is_ascii_digit())
    {
        let days: u32 = days.parse().ok()?;
        return (days > 0).then_some(Modified::Days(days));
    }
    if let Some((from, to)) = lower.split_once("..") {
        let (from, to) = (Moment::parse(from)?, Moment::parse(to)?);
        return (from.start() <= to.start()).then_some(Modified::Between(from, to));
    }
    if let Some(rest) = lower.strip_prefix(">=").or_else(|| lower.strip_prefix('>')) {
        return Some(Modified::Since(Moment::parse(rest)?));
    }
    if let Some(rest) = lower.strip_prefix('<') {
        return Some(Modified::Before(Moment::parse(rest)?));
    }
    Some(Modified::On(Moment::parse(&lower)?))
}

impl Display for Modified {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Modified::Today => f.write_str("today"),
            Modified::Yesterday => f.write_str("yesterday"),
            Modified::Days(7) => f.write_str("week"),
            Modified::Days(30) => f.write_str("month"),
            Modified::Days(n) => write!(f, "{n}d"),
            Modified::ThisYear => f.write_str("year"),
            Modified::On(moment) => write!(f, "{moment}"),
            Modified::Since(moment) => write!(f, ">{moment}"),
            Modified::Before(moment) => write!(f, "<{moment}"),
            Modified::Between(from, to) => write!(f, "{from}..{to}"),
        }
    }
}

// ---------------------------------------------------------------------------
// Printing
// ---------------------------------------------------------------------------

/// A value as `key:value` writes it: in quotes when it has a space.
fn write_value(out: &mut String, key: &str, value: &str) {
    let value: String = value.chars().filter(|c| *c != '"').collect();
    if value.chars().any(char::is_whitespace) {
        let _ = write!(out, "{key}:\"{value}\"");
    } else {
        let _ = write!(out, "{key}:{value}");
    }
}

impl Display for Term {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.exclude {
            f.write_str("-")?;
        }
        if self.phrase {
            write!(f, "\"{}\"", self.text)
        } else {
            f.write_str(&self.text)
        }
    }
}

impl Display for Filters {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = String::new();
        let mut push = |token: &str| {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(token);
        };
        for glob in &self.globs {
            let mut token = String::new();
            write_value(&mut token, "name", glob);
            push(&token);
        }
        if !self.exts.is_empty() {
            push(&format!("ext:{}", self.exts.join(",")));
        }
        if !self.kinds.is_empty() {
            let words: Vec<&str> = self.kinds.iter().map(|kind| kind.word()).collect();
            push(&format!("type:{}", words.join(",")));
        }
        if let Some(size) = &self.size {
            push(&format!("size:{size}"));
        }
        if let Some(modified) = &self.modified {
            push(&format!("modified:{modified}"));
        }
        if let Some(within) = &self.within {
            let mut token = String::new();
            write_value(&mut token, "in", within);
            push(&token);
        }
        match self.hidden {
            Hidden::Follow => {}
            Hidden::Exclude => push("hidden:no"),
            Hidden::Include => push("hidden:yes"),
            Hidden::Only => push("hidden:only"),
        }
        if self.case_sensitive {
            push("case:yes");
        }
        match self.name_mode {
            NameMode::Contains => {}
            NameMode::Starts => push("match:starts"),
            NameMode::Exact => push("match:exact"),
            NameMode::Regex => push("match:regex"),
        }
        if self.sort != SortBy::Relevance {
            push(&format!("sort:{}", self.sort.word()));
        }
        f.write_str(&out)
    }
}

impl Display for Query {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;
        for term in &self.terms {
            if !first {
                f.write_str(" ")?;
            }
            first = false;
            write!(f, "{term}")?;
        }
        let filters = self.filters.to_string();
        if !filters.is_empty() {
            if !first {
                f.write_str(" ")?;
            }
            f.write_str(&filters)?;
        }
        Ok(())
    }
}
