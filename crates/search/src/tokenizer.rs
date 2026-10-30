//! The tokenizer shared by indexing, queries and highlights.
//!
//! See `design/search.md` section 4.1 for the rules. In short: UAX #29 words,
//! split again on `_` and `.` and at camel-case boundaries, apostrophes
//! dropped, compounds (`Wi-Fi`, `snake_case`, `fooBar`) also indexed joined,
//! text folded to lower case without diacritics, byte ranges kept so the
//! original text can be marked.

use unicode_normalization::char::{decompose_canonical, is_combining_mark};
use unicode_segmentation::UnicodeSegmentation;

/// Version of the tokenizer rules. A snapshot written with another version is
/// not trusted, because its terms would not match what a query now produces.
pub const TOKENIZER_VERSION: u32 = 1;

/// The position gap a hard separator (path or sentence punctuation) adds, so
/// a phrase or proximity cannot straddle it.
pub const HARD_GAP: u32 = 8;

/// Most positions kept per field. A token past it is not indexed.
pub const MAX_POSITION: u32 = u16::MAX as u32;

/// One indexed or queried token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    /// The folded text: lower case, no diacritics.
    pub text: String,
    /// Byte range in the original text.
    pub start: usize,
    pub end: usize,
    /// Position among the field's tokens. A joined token shares the position of
    /// the first part of its compound.
    pub position: u32,
    /// The joined form of a compound (`wifi` for `Wi-Fi`).
    pub joined: bool,
}

/// Appends the folded form of `c` to `out`: diacritics removed, lower case.
pub fn fold_char(c: char, out: &mut String) {
    if c.is_ascii() {
        out.push(c.to_ascii_lowercase());
        return;
    }
    decompose_canonical(c, |d| {
        if !is_combining_mark(d) {
            out.extend(d.to_lowercase());
        }
    });
}

/// The number of chars [`fold_char`] produces for `c`.
pub fn folded_len(c: char) -> usize {
    if c.is_ascii() {
        return 1;
    }
    let mut n = 0;
    decompose_canonical(c, |d| {
        if !is_combining_mark(d) {
            n += d.to_lowercase().count();
        }
    });
    n
}

/// `text` folded to lower case without diacritics.
pub fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    if text.is_ascii() {
        out.push_str(text);
        out.make_ascii_lowercase();
        return out;
    }
    for c in text.chars() {
        fold_char(c, &mut out);
    }
    out
}

fn is_hard(c: char) -> bool {
    matches!(
        c,
        '.' | '!'
            | '?'
            | ';'
            | ','
            | ':'
            | '|'
            | '›'
            | '»'
            | '>'
            | '«'
            | '<'
            | '\n'
            | '\r'
            | '/'
            | '\\'
            | '('
            | ')'
            | '['
            | ']'
            | '{'
            | '}'
            | '"'
            | '·'
            | '•'
    )
}

fn is_hyphen(c: char) -> bool {
    matches!(c, '-' | '\u{2010}' | '\u{2011}')
}

/// A part of a word: byte range and folded text.
struct Part {
    start: usize,
    end: usize,
    text: String,
}

/// Splits `word` (starting at byte `base` of the text) into parts: at `_` and
/// `.`, and at camel-case boundaries. Apostrophes are dropped.
fn split_word(word: &str, base: usize, parts: &mut Vec<Part>) {
    let chars: Vec<(usize, char)> = word.char_indices().collect();
    let mut seg_start = 0usize; // index into chars
    let mut i = 0;
    while i <= chars.len() {
        let at_end = i == chars.len();
        let separator = !at_end && matches!(chars[i].1, '_' | '.');
        if at_end || separator {
            if i > seg_start {
                split_camel(&chars[seg_start..i], word, base, parts);
            }
            seg_start = i + 1;
        }
        i += 1;
    }
}

fn split_camel(seg: &[(usize, char)], word: &str, base: usize, parts: &mut Vec<Part>) {
    let mut from = 0usize;
    for i in 1..=seg.len() {
        let boundary = if i == seg.len() {
            true
        } else {
            let prev = seg[i - 1].1;
            let cur = seg[i].1;
            if prev.is_lowercase() && cur.is_uppercase() {
                true
            } else if prev.is_uppercase() && cur.is_uppercase() {
                // Acronym end: HTTPServer -> HTTP | Server, when the capitals
                // before the break number three or more.
                let next_lower = seg.get(i + 1).is_some_and(|(_, n)| n.is_lowercase());
                let mut run = 0;
                let mut k = i;
                while k > 0 && seg[k - 1].1.is_uppercase() {
                    run += 1;
                    k -= 1;
                }
                next_lower && run >= 3
            } else {
                false
            }
        };
        if boundary {
            let slice = &seg[from..i];
            let start = slice[0].0;
            let last = slice[slice.len() - 1];
            let end = last.0 + last.1.len_utf8();
            let mut text = String::with_capacity(end - start);
            for &(_, c) in slice {
                if c != '\'' && c != '\u{2019}' {
                    fold_char(c, &mut text);
                }
            }
            if !text.is_empty() {
                parts.push(Part {
                    start: base + start,
                    end: base + end,
                    text,
                });
            }
            from = i;
        }
    }
    let _ = word;
}

/// Tokenizes `text`. With `joined`, the joined form of each compound is also
/// emitted (indexing); queries pass `false`. Positions start at `first`.
/// Returns the position after the last token.
pub fn tokenize_into(text: &str, first: u32, joined: bool, out: &mut Vec<Token>) -> u32 {
    let mut position = first;
    let mut prev_end: Option<usize> = None;
    // Parts of the compound being built.
    let mut group: Vec<Part> = Vec::new();

    let flush = |group: &mut Vec<Part>, position: &mut u32, out: &mut Vec<Token>| {
        if group.is_empty() {
            return;
        }
        if joined && group.len() > 1 && *position <= MAX_POSITION {
            let mut text = String::new();
            for part in group.iter() {
                text.push_str(&part.text);
            }
            out.push(Token {
                text,
                start: group[0].start,
                end: group[group.len() - 1].end,
                position: *position,
                joined: true,
            });
        }
        for part in group.drain(..) {
            if *position <= MAX_POSITION {
                out.push(Token {
                    text: part.text,
                    start: part.start,
                    end: part.end,
                    position: *position,
                    joined: false,
                });
            }
            *position += 1;
        }
    };

    for (start, word) in text.unicode_word_indices() {
        if let Some(end) = prev_end {
            let gap = &text[end..start];
            let mut chars = gap.chars();
            let connector = match (chars.next(), chars.next()) {
                (Some(c), None) => is_hyphen(c),
                _ => false,
            };
            if connector && !group.is_empty() {
                // Same compound: Wi-Fi.
            } else {
                flush(&mut group, &mut position, out);
                if gap.chars().any(is_hard) {
                    position += HARD_GAP;
                }
            }
        }
        split_word(word, start, &mut group_scratch(&mut group));
        prev_end = Some(start + word.len());
    }
    flush(&mut group, &mut position, out);
    position
}

/// Lets `split_word` append to `group` without borrowing it twice.
fn group_scratch(group: &mut Vec<Part>) -> &mut Vec<Part> {
    group
}

/// Tokenizes `text` for indexing.
pub fn tokenize(text: &str) -> Vec<Token> {
    let mut out = Vec::new();
    tokenize_into(text, 0, true, &mut out);
    out
}

/// Tokenizes a query: no joined forms.
pub fn tokenize_query(text: &str) -> Vec<Token> {
    let mut out = Vec::new();
    tokenize_into(text, 0, false, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(text: &str) -> Vec<(String, u32)> {
        tokenize(text)
            .into_iter()
            .map(|t| (t.text, t.position))
            .collect()
    }

    #[test]
    fn hyphenated_compound_is_split_and_joined() {
        assert_eq!(
            words("Wi-Fi"),
            vec![("wifi".into(), 0), ("wi".into(), 0), ("fi".into(), 1)]
        );
    }

    #[test]
    fn camel_snake_and_acronyms_split() {
        let t: Vec<_> = tokenize_query("fooBar snake_case HTTPServer IPv4 802.1x")
            .into_iter()
            .map(|t| t.text)
            .collect();
        assert_eq!(
            t,
            ["foo", "bar", "snake", "case", "http", "server", "ipv4", "802", "1x"]
        );
    }

    #[test]
    fn folds_diacritics_and_case() {
        assert_eq!(fold("Résumé ÅNGSTRÖM"), "resume angstrom");
        assert_eq!(tokenize_query("Crème Brûlée")[1].text, "brulee");
    }

    #[test]
    fn byte_ranges_point_into_the_original() {
        let text = "Café Ünï";
        let toks = tokenize_query(text);
        assert_eq!(&text[toks[0].start..toks[0].end], "Café");
        assert_eq!(&text[toks[1].start..toks[1].end], "Ünï");
    }

    #[test]
    fn hard_separators_add_a_gap() {
        let t = tokenize_query("Network › Proxy");
        assert_eq!(t[1].position - t[0].position, 1 + HARD_GAP);
        let t = tokenize_query("a b");
        assert_eq!(t[1].position - t[0].position, 1);
    }

    #[test]
    fn apostrophes_are_dropped_and_cjk_splits_per_character() {
        assert_eq!(tokenize_query("don't")[0].text, "dont");
        let t = tokenize_query("日本語");
        assert_eq!(t.len(), 3);
        assert_eq!(t[2].position, 2);
    }
}
