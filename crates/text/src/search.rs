//! Find and replace: a query (plain text or a regular expression, matching
//! case or not, whole words or not) and the matches it has in a rope.
//!
//! Searching reads a snapshot of the rope, which is a pointer copy, so a
//! long search can run on a worker while the person keeps typing; its
//! answer names the revision it was for.

use regex::{Regex, RegexBuilder};
use ropey::Rope;

/// Matches past this many are not collected; the count says "10,000+".
pub const MATCH_LIMIT: usize = 10_000;

/// What to look for.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Query {
    pub text: String,
    pub regex: bool,
    pub case_sensitive: bool,
    pub whole_word: bool,
}

impl Query {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// The compiled pattern, or the reason it does not compile (for a
    /// regular expression the person is still typing).
    pub fn compile(&self) -> Result<Regex, String> {
        let pattern = if self.regex {
            self.text.clone()
        } else {
            regex::escape(&self.text)
        };
        let pattern = if self.whole_word {
            format!(r"\b(?:{pattern})\b")
        } else {
            pattern
        };
        // A plain query with no capitals matches any case; one with a
        // capital in it, or the Match Case option, matches exactly.
        let insensitive =
            !self.case_sensitive && (self.regex || !self.text.chars().any(char::is_uppercase));
        RegexBuilder::new(&pattern)
            .case_insensitive(insensitive)
            .multi_line(true)
            .size_limit(1 << 24)
            .build()
            .map_err(|error| describe_error(&error))
    }
}

fn describe_error(error: &regex::Error) -> String {
    match error {
        regex::Error::Syntax(text) => {
            let last = text
                .lines()
                .filter(|line| line.starts_with("error:"))
                .next_back();
            match last {
                Some(line) => {
                    let message = line.trim_start_matches("error:").trim();
                    let mut chars = message.chars();
                    match chars.next() {
                        Some(first) => first.to_uppercase().chain(chars).collect(),
                        None => "Not a valid pattern".into(),
                    }
                }
                None => "Not a valid pattern".into(),
            }
        }
        regex::Error::CompiledTooBig(_) => "The pattern is too large".into(),
        _ => "Not a valid pattern".into(),
    }
}

/// The matches of a query, as char ranges.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Matches {
    pub ranges: Vec<(usize, usize)>,
    /// More matches exist than [`MATCH_LIMIT`].
    pub truncated: bool,
}

impl Matches {
    /// The index of the first match at or after `at`, wrapping round.
    pub fn next_from(&self, at: usize) -> Option<usize> {
        if self.ranges.is_empty() {
            return None;
        }
        let index = self.ranges.partition_point(|(start, _)| *start < at);
        Some(if index == self.ranges.len() { 0 } else { index })
    }

    /// The index of the last match that ends before `at`, wrapping round.
    pub fn previous_from(&self, at: usize) -> Option<usize> {
        if self.ranges.is_empty() {
            return None;
        }
        let index = self.ranges.partition_point(|(_, end)| *end <= at);
        Some(if index == 0 {
            self.ranges.len() - 1
        } else {
            index - 1
        })
    }

    /// The index of the match that is exactly `range`.
    pub fn index_of(&self, range: (usize, usize)) -> Option<usize> {
        self.ranges.binary_search(&range).ok()
    }

    /// Matches whose start is in `start..end`, for drawing a screen of them.
    pub fn within(&self, start: usize, end: usize) -> &[(usize, usize)] {
        let first = self
            .ranges
            .partition_point(|(_, match_end)| *match_end < start);
        let last = self
            .ranges
            .partition_point(|(match_start, _)| *match_start < end);
        &self.ranges[first..last.max(first)]
    }
}

/// Every match of `pattern` in `rope`. Empty matches (from `^` or `a*`) are
/// left out: they cannot be shown or replaced one at a time.
pub fn find_all(rope: &Rope, pattern: &Regex) -> Matches {
    let text: String = if rope.len_bytes() < 1 << 30 {
        rope.to_string()
    } else {
        String::new()
    };
    let mut ranges = Vec::new();
    let mut truncated = false;
    // Byte offsets to chars, walking forward once.
    let mut last_byte = 0usize;
    let mut last_char = 0usize;
    let mut to_char = |byte: usize| {
        if byte >= last_byte {
            last_char += text[last_byte..byte].chars().count();
        } else {
            last_char -= text[byte..last_byte].chars().count();
        }
        last_byte = byte;
        last_char
    };
    for found in pattern.find_iter(&text) {
        if found.start() == found.end() {
            continue;
        }
        if ranges.len() == MATCH_LIMIT {
            truncated = true;
            break;
        }
        let start = to_char(found.start());
        let end = to_char(found.end());
        ranges.push((start, end));
    }
    Matches { ranges, truncated }
}

/// The text that replaces the match `range` of `pattern`: `replacement` as
/// typed, or with `$1` and `${name}` filled in for a regular expression.
pub fn replacement_for(
    rope: &Rope,
    range: (usize, usize),
    pattern: &Regex,
    query: &Query,
    replacement: &str,
) -> String {
    if !query.regex {
        return replacement.to_owned();
    }
    // The match is found again in its line (or its own text when it spans
    // lines), so look-arounds at its edges see what is next to it.
    let line_start = rope.line_to_char(rope.char_to_line(range.0));
    let line_end_line = rope.char_to_line(range.1.saturating_sub(1).max(range.0));
    let line_end = if line_end_line + 1 < rope.len_lines() {
        rope.line_to_char(line_end_line + 1)
    } else {
        rope.len_chars()
    };
    let haystack = rope.slice(line_start..line_end).to_string();
    let offset = rope.slice(line_start..range.0).len_bytes();
    let mut out = String::new();
    if let Some(captures) = pattern.captures_at(&haystack, offset)
        && captures.get(0).is_some_and(|whole| whole.start() == offset)
    {
        captures.expand(&unescape(replacement), &mut out);
        return out;
    }
    unescape(replacement)
}

/// `\n` and `\t` in a regular expression's replacement are a new line and a
/// tab, as in other editors.
fn unescape(replacement: &str) -> String {
    let mut out = String::with_capacity(replacement.len());
    let mut chars = replacement.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_queries_ignore_case_until_a_capital() {
        let rope = Rope::from_str("Apple apple APPLE");
        let all = find_all(&rope, &Query::plain("apple").compile().unwrap());
        assert_eq!(all.ranges.len(), 3);
        let exact = find_all(&rope, &Query::plain("Apple").compile().unwrap());
        assert_eq!(exact.ranges, vec![(0, 5)]);
    }

    #[test]
    fn whole_words() {
        let rope = Rope::from_str("cat concat cat.");
        let query = Query {
            text: "cat".into(),
            whole_word: true,
            ..Query::default()
        };
        assert_eq!(
            find_all(&rope, &query.compile().unwrap()).ranges,
            vec![(0, 3), (11, 14)]
        );
    }

    #[test]
    fn char_offsets_past_multibyte_text() {
        let rope = Rope::from_str("日本語 and 日本");
        let found = find_all(&rope, &Query::plain("日本").compile().unwrap());
        assert_eq!(found.ranges, vec![(0, 2), (8, 10)]);
    }

    #[test]
    fn regex_replacement_fills_groups() {
        let rope = Rope::from_str("let x = 1;\nlet y = 2;\n");
        let query = Query {
            text: r"let (\w+) = (\d+)".into(),
            regex: true,
            ..Query::default()
        };
        let pattern = query.compile().unwrap();
        let found = find_all(&rope, &pattern);
        assert_eq!(found.ranges.len(), 2);
        assert_eq!(
            replacement_for(
                &rope,
                found.ranges[1],
                &pattern,
                &query,
                "const $1: i32 = $2"
            ),
            "const y: i32 = 2"
        );
    }

    #[test]
    fn bad_patterns_say_why() {
        let query = Query {
            text: "(unclosed".into(),
            regex: true,
            ..Query::default()
        };
        let error = query.compile().unwrap_err();
        assert!(error.to_lowercase().contains("unclosed"), "{error}");
    }

    #[test]
    fn next_and_previous_wrap() {
        let matches = Matches {
            ranges: vec![(2, 4), (10, 12)],
            truncated: false,
        };
        assert_eq!(matches.next_from(5), Some(1));
        assert_eq!(matches.next_from(11), Some(0));
        assert_eq!(matches.previous_from(10), Some(0));
        assert_eq!(matches.previous_from(1), Some(1));
        assert_eq!(matches.within(3, 9), &[(2, 4)]);
    }
}
