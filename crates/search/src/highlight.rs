//! Marking the matched words of a stored value.

use std::collections::HashMap;
use std::ops::Range;

use crate::text::{Compiled, Kind, Source};
use crate::tokenizer;

/// How much of a token a query term marks.
#[derive(Clone, Copy)]
struct Reach {
    /// Marks the whole token.
    whole: bool,
    /// Otherwise, this many folded characters from the start.
    prefix_chars: usize,
}

/// The terms a query matched, built from its compiled form.
#[derive(Default)]
pub(crate) struct Matcher {
    terms: HashMap<String, Reach>,
}

impl Matcher {
    pub fn add(&mut self, c: &Compiled) {
        for word in &c.words {
            for src in &word.sources {
                match src {
                    Source::Terms(exps) => {
                        for e in exps {
                            let Some(term) = &e.term else { continue };
                            let reach = match e.kind {
                                Kind::Prefix => Reach {
                                    whole: false,
                                    prefix_chars: word.chars,
                                },
                                _ => Reach {
                                    whole: true,
                                    prefix_chars: 0,
                                },
                            };
                            self.put(term, reach);
                        }
                    }
                    Source::Phrase { terms, .. } => {
                        for t in terms {
                            self.put(
                                t,
                                Reach {
                                    whole: true,
                                    prefix_chars: 0,
                                },
                            );
                        }
                    }
                }
            }
        }
    }

    fn put(&mut self, term: &str, reach: Reach) {
        match self.terms.get_mut(term) {
            Some(old) => {
                if reach.whole {
                    *old = reach;
                } else if !old.whole {
                    old.prefix_chars = old.prefix_chars.max(reach.prefix_chars);
                }
            }
            None => {
                self.terms.insert(term.to_string(), reach);
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }

    /// The byte ranges of `text` the query matched: sorted, not overlapping.
    pub fn ranges(&self, text: &str) -> Vec<Range<usize>> {
        let mut out: Vec<Range<usize>> = Vec::new();
        for tok in tokenizer::tokenize(text) {
            let Some(reach) = self.terms.get(&tok.text) else {
                continue;
            };
            let end = if reach.whole || tok.joined {
                tok.end
            } else {
                prefix_end(text, tok.start, tok.end, reach.prefix_chars)
            };
            if end > tok.start {
                out.push(tok.start..end);
            }
        }
        out.sort_by_key(|r| (r.start, r.end));
        let mut merged: Vec<Range<usize>> = Vec::with_capacity(out.len());
        for r in out {
            match merged.last_mut() {
                Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
                _ => merged.push(r),
            }
        }
        merged
    }
}

/// The byte offset in `text[start..end]` after `chars` folded characters.
fn prefix_end(text: &str, start: usize, end: usize, chars: usize) -> usize {
    let mut counted = 0;
    for (i, c) in text[start..end].char_indices() {
        if counted >= chars {
            return start + i;
        }
        if c == '\'' || c == '\u{2019}' {
            continue;
        }
        counted += tokenizer::folded_len(c);
    }
    end
}

/// `text` with `ranges` wrapped in the tags.
pub(crate) fn mark(text: &str, ranges: &[Range<usize>], pre: &str, post: &str) -> String {
    let mut out = String::with_capacity(text.len() + ranges.len() * (pre.len() + post.len()));
    let mut at = 0;
    for r in ranges {
        out.push_str(&text[at..r.start]);
        out.push_str(pre);
        out.push_str(&text[r.clone()]);
        out.push_str(post);
        at = r.end;
    }
    out.push_str(&text[at..]);
    out
}

/// A window of about `size` characters around the first range, cut at word
/// boundaries, with an ellipsis where text was dropped. Returns the window
/// bounds in bytes.
pub(crate) fn crop(text: &str, ranges: &[Range<usize>], size: usize) -> Range<usize> {
    if text.chars().count() <= size {
        return 0..text.len();
    }
    let first = ranges.first().map_or(0, |r| r.start);
    // Start a third of the window before the match.
    let back = size / 3;
    let mut start = first;
    let mut n = 0;
    for (i, _) in text[..first].char_indices().rev() {
        if n >= back {
            break;
        }
        start = i;
        n += 1;
    }
    // Snap to a word start.
    if start > 0
        && let Some(sp) = text[start..].find(char::is_whitespace)
        && start + sp < first
    {
        start += sp + 1;
    }
    let mut end = text.len();
    let mut n = 0;
    for (i, _) in text[start..].char_indices() {
        if n >= size {
            end = start + i;
            break;
        }
        n += 1;
    }
    if end < text.len()
        && let Some(sp) = text[..end].rfind(char::is_whitespace)
        && sp > first
    {
        end = sp;
    }
    start..end
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_ranges() {
        assert_eq!(mark("Wi-Fi proxy", &[0..5, 6..9], "<em>", "</em>"), "<em>Wi-Fi</em> <em>pro</em>xy");
    }

    #[test]
    fn crop_keeps_the_match_in_view() {
        let text = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu";
        let at = text.find("iota").unwrap();
        let r = crop(text, &[at..at + 4], 24);
        assert!(text[r].contains("iota"));
    }
}
