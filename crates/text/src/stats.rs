//! Document statistics: lines, words, characters, paragraphs and an
//! estimate of reading time.

use ropey::{Rope, RopeSlice};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub lines: usize,
    pub words: usize,
    /// User-perceived characters (grapheme clusters), line breaks excluded.
    pub characters: usize,
    pub characters_no_spaces: usize,
    pub paragraphs: usize,
    pub bytes: usize,
}

impl Stats {
    /// Minutes to read at 230 words a minute, at least one for any text.
    pub fn reading_minutes(&self) -> usize {
        if self.words == 0 {
            0
        } else {
            self.words.div_ceil(230)
        }
    }
}

/// Counts `text`. A line at a time, so a large file needs no copy.
pub fn count(text: &Rope) -> Stats {
    count_slice(text.slice(..))
}

pub fn count_slice(text: RopeSlice) -> Stats {
    let mut stats = Stats {
        lines: text.len_lines(),
        bytes: text.len_bytes(),
        ..Stats::default()
    };
    if text.len_chars() == 0 {
        stats.lines = 1;
        return stats;
    }
    let mut in_paragraph = false;
    let mut buffer = String::new();
    for line in text.lines() {
        buffer.clear();
        for chunk in line.chunks() {
            buffer.push_str(chunk);
        }
        let line = buffer.trim_end_matches(['\n', '\r']);
        if line.trim().is_empty() {
            in_paragraph = false;
        } else if !in_paragraph {
            stats.paragraphs += 1;
            in_paragraph = true;
        }
        stats.words += line.unicode_words().count();
        for grapheme in line.graphemes(true) {
            stats.characters += 1;
            if !grapheme.chars().all(char::is_whitespace) {
                stats.characters_no_spaces += 1;
            }
        }
    }
    stats
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_words_and_paragraphs() {
        let stats = count(&Rope::from_str(
            "One two three.\nFour.\n\nFive é\u{301} six\n",
        ));
        assert_eq!(stats.words, 7);
        assert_eq!(stats.paragraphs, 2);
        assert_eq!(stats.lines, 5);
        assert_eq!(stats.characters, 14 + 5 + 10);
        assert_eq!(stats.reading_minutes(), 1);
    }

    #[test]
    fn empty_text() {
        let stats = count(&Rope::new());
        assert_eq!(
            stats,
            Stats {
                lines: 1,
                ..Stats::default()
            }
        );
    }
}
