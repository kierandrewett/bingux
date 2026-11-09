//! The command palette's matching: the letters of the query in order, not
//! necessarily together, ignoring case ("tgsb" finds "Toggle sidebar").
//!
//! A match scores higher when the letters are together, when they start
//! words, and when they start the text; a long gap costs. The palette ranks
//! by score and marks the matched letters.

use std::ops::Range;

/// How a query matched a text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Fuzzy {
    pub score: i32,
    /// The matched letters, as byte ranges of the text: sorted, not
    /// overlapping, on character boundaries.
    pub ranges: Vec<Range<usize>>,
}

const START: i32 = 24;
const WORD_START: i32 = 16;
const TOGETHER: i32 = 12;
const LETTER: i32 = 4;
const GAP: i32 = 1;

fn starts_word(chars: &[(usize, char)], at: usize) -> bool {
    at == 0 || {
        let (before, here) = (chars[at - 1].1, chars[at].1);
        !before.is_alphanumeric() || (before.is_lowercase() && here.is_uppercase())
    }
}

/// How `query` matches `text`, or `None` when its letters are not all there
/// in order. Spaces in the query are ignored, so "new tab" finds "New tab"
/// and "newtab" alike. An empty query matches nothing.
pub(crate) fn fuzzy(query: &str, text: &str) -> Option<Fuzzy> {
    let wanted: Vec<char> = query
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect();
    if wanted.is_empty() {
        return None;
    }
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let lower: Vec<char> = chars
        .iter()
        .map(|(_, c)| c.to_lowercase().next().unwrap_or(*c))
        .collect();

    // For each letter, prefer the next one that continues the run or starts a
    // word, as long as the rest of the query still fits after it; else take
    // the first that leaves room.
    let mut picked: Vec<usize> = Vec::with_capacity(wanted.len());
    let mut from = 0;
    for (index, want) in wanted.iter().enumerate() {
        let mut first = None;
        let mut word = None;
        for at in from..lower.len() {
            if lower[at] != *want {
                continue;
            }
            // Room for the remaining letters after this one, in order.
            if !fits(&lower[at + 1..], &wanted[index + 1..]) {
                break;
            }
            first.get_or_insert(at);
            // A letter right after the previous match keeps the run going:
            // take it.
            if picked.last().is_some_and(|last| last + 1 == at) {
                word = Some(at);
                break;
            }
            if starts_word(&chars, at) {
                word = Some(at);
                break;
            }
        }
        let at = word.or(first)?;
        picked.push(at);
        from = at + 1;
    }

    let mut score = 0;
    for (n, at) in picked.iter().enumerate() {
        score += LETTER;
        if *at == 0 {
            score += START;
        } else if starts_word(&chars, *at) {
            score += WORD_START;
        }
        if n > 0 {
            let previous = picked[n - 1];
            if previous + 1 == *at {
                score += TOGETHER;
            } else {
                score -= GAP * (*at - previous - 1).min(8) as i32;
            }
        }
    }
    // Shorter texts win a tie: "Copy" before "Copy location".
    score -= (chars.len() / 8) as i32;

    let mut ranges: Vec<Range<usize>> = Vec::new();
    for at in picked {
        let start = chars[at].0;
        let end = chars.get(at + 1).map_or(text.len(), |(byte, _)| *byte);
        match ranges.last_mut() {
            Some(last) if last.end == start => last.end = end,
            _ => ranges.push(start..end),
        }
    }
    Some(Fuzzy { score, ranges })
}

/// Whether `wanted` occurs in order in `haystack`.
fn fits(haystack: &[char], wanted: &[char]) -> bool {
    let mut letters = haystack.iter();
    wanted.iter().all(|want| letters.any(|c| c == want))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marked(query: &str, text: &str) -> Vec<String> {
        fuzzy(query, text)
            .unwrap()
            .ranges
            .into_iter()
            .map(|range| text[range].to_owned())
            .collect()
    }

    fn score(query: &str, text: &str) -> i32 {
        fuzzy(query, text).map_or(i32::MIN, |found| found.score)
    }

    // Failure modes: letters out of order matching; a greedy early pick that
    // leaves no room for the rest; marks that split a character or miss the
    // word starts the person typed; ranking a scattered match over a word match.

    #[test]
    fn letters_in_order_match_and_others_do_not() {
        assert!(fuzzy("tgsb", "Toggle sidebar").is_some());
        assert!(fuzzy("bst", "Toggle sidebar").is_none());
        assert!(fuzzy("", "Anything").is_none());
        assert!(fuzzy("NEW TAB", "New tab").is_some());
    }

    #[test]
    fn word_starts_are_marked() {
        assert_eq!(marked("ts", "Toggle sidebar"), ["T", "s"]);
        assert_eq!(marked("sid", "Toggle sidebar"), ["sid"]);
        assert_eq!(marked("nt", "New tab"), ["N", "t"]);
    }

    #[test]
    fn a_late_word_start_does_not_strand_the_rest() {
        // The only "a" after the "b" is in "bar": the "a" of "about" must not
        // be skipped for a word start that leaves no "r".
        assert_eq!(marked("bar", "about bar"), ["bar"]);
        assert!(fuzzy("tbr", "Toggle sidebar").is_some());
    }

    #[test]
    fn together_and_at_word_starts_rank_first() {
        assert!(score("copy", "Copy") > score("copy", "Copy location"));
        assert!(score("cl", "Copy location") > score("cl", "Cancel"));
        assert!(score("sh", "Show hidden files") > score("sh", "Refresh"));
    }

    #[test]
    fn marks_fall_on_character_boundaries() {
        let text = "Ouvrir l’élément";
        let found = fuzzy("élé", text).unwrap();
        for range in found.ranges {
            assert!(text.is_char_boundary(range.start) && text.is_char_boundary(range.end));
        }
    }
}
