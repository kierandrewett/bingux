//! How a [`Query`] meets a file: which names match, how well, and which
//! entries pass the filters.
//!
//! # Matching names
//!
//! Plain words match the file name only, not the folder and not the content.
//! [`Matcher::name_hit`] says how well a name matches, best first:
//!
//! 1. [`Tier::Exact`]: the name is the text.
//! 2. [`Tier::Prefix`]: the name starts with it.
//! 3. [`Tier::Word`]: it appears at the start of a word (after a space, `-`,
//!    `_`, `.` or another non-letter).
//! 4. [`Tier::Substring`]: it appears somewhere in the name.
//! 5. [`Tier::Words`]: each word appears, in any order.
//! 6. [`Tier::Fuzzy`]: the letters appear in order with others between them
//!    (`rpt` finds `my_report.txt`). Only for one word of three letters or
//!    more, never for a phrase, and only when they sit close together.
//!
//! [`Tier::Typo`] is not decided here: it is what the index adds, a name a
//! word is a typo of. [`Tier::Any`] is every name when a query has no words.
//!
//! `match:starts`, `match:exact` and `match:regex` change what counts: see
//! [`NameMode`].
//!
//! # Filters
//!
//! `ext:`, `name:` and excluded words are decided from the name alone
//! ([`Matcher::name_hit`]); `type:`, `size:`, `modified:` and `hidden:` from
//! the entry ([`Matcher::accepts`]), because size and date cost a `stat`.
//! [`Matcher::pre_accepts`] says from the name and the kind of directory
//! entry whether a `type:` can still pass, so the walker only stats what can.

use std::ops::Range;
use std::path::Path;

use regex::{Regex, RegexBuilder};

use super::civil::Clock;
use super::query::{Hidden, Kind, Modified, NameMode, Query, Term};
use crate::entry::Entry;
use crate::filter::{glob_match, glob_match_exact};
use crate::mime;

/// How well a name matches a query; earlier is better.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tier {
    Exact,
    Prefix,
    Word,
    Substring,
    Words,
    Fuzzy,
    /// A word of the query is a near miss of a word of the name.
    Typo,
    /// The query has no words: every name is as good as another.
    Any,
}

/// How a name matches: its tier, how loose a fuzzy match is, and the bytes
/// of the name to draw heavier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    pub tier: Tier,
    /// Zero except for [`Tier::Fuzzy`]: larger is a looser fit.
    pub penalty: u32,
    /// Byte ranges of the name, sorted, not overlapping, on character
    /// boundaries.
    pub ranges: Vec<Range<usize>>,
}

/// Where a result stands in relevance order: its match, then how deep below
/// the folder searched it is (shallow first).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Rank {
    pub tier: Tier,
    pub penalty: u32,
    pub depth: u32,
}

/// Why a query cannot be run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invalid {
    /// In a few words, for the field to show.
    pub reason: String,
}

/// Fuzzy matching starts at this many letters; fewer would match nearly
/// everything.
const FUZZY_MIN: usize = 3;

/// A word compared without regard to case, unless the query asks.
#[derive(Clone, Debug)]
struct Pat {
    chars: Vec<char>,
    phrase: bool,
}

/// A [`Query`] made ready to test many names.
#[derive(Clone, Debug)]
pub struct Matcher {
    query: Query,
    clock: Clock,
    case: bool,
    pos: Vec<Pat>,
    neg: Vec<Pat>,
    /// The words joined by one space, when every one is a plain word and
    /// there are two or more: matched whole before they are matched apart.
    joined: Option<Pat>,
    regex: Option<Regex>,
    /// Seconds since the epoch: from, to (not included).
    when: (Option<i64>, Option<i64>),
}

fn fold(c: char, case: bool) -> char {
    if case {
        c
    } else {
        c.to_lowercase().next().unwrap_or(c)
    }
}

fn pat(term: &Term, case: bool) -> Pat {
    Pat {
        chars: term.text().chars().map(|c| fold(c, case)).collect(),
        phrase: term.is_phrase(),
    }
}

impl Matcher {
    /// Made for `query`, with the clock of this moment.
    pub fn new(query: &Query) -> Result<Matcher, Invalid> {
        Matcher::with_clock(query, Clock::system())
    }

    /// Made for `query`, reading "today" off `clock`.
    pub fn with_clock(query: &Query, clock: Clock) -> Result<Matcher, Invalid> {
        let case = query.filters.case_sensitive;
        let pos: Vec<Pat> = query
            .terms
            .iter()
            .filter(|term| !term.is_excluded())
            .map(|term| pat(term, case))
            .collect();
        let neg = query
            .terms
            .iter()
            .filter(|term| term.is_excluded())
            .map(|term| pat(term, case))
            .collect();
        let joined = (pos.len() > 1 && pos.iter().all(|pat| !pat.phrase)).then(|| {
            let mut chars = Vec::new();
            for (at, pat) in pos.iter().enumerate() {
                if at > 0 {
                    chars.push(' ');
                }
                chars.extend(&pat.chars);
            }
            Pat { chars, phrase: false }
        });
        let regex = if query.filters.name_mode == NameMode::Regex && !pos.is_empty() {
            let source = query.plain_text();
            let built = RegexBuilder::new(&source)
                .case_insensitive(!case)
                .size_limit(1 << 20)
                .build()
                .map_err(|error| Invalid {
                    reason: regex_reason(&error),
                })?;
            Some(built)
        } else {
            None
        };
        let when = query
            .filters
            .modified
            .map_or((None, None), |modified| modified_range(modified, &clock));
        Ok(Matcher {
            query: query.clone(),
            clock,
            case,
            pos,
            neg,
            joined,
            regex,
            when,
        })
    }

    pub fn query(&self) -> &Query {
        &self.query
    }

    /// The moment "today" and "this week" were read at.
    pub fn clock(&self) -> Clock {
        self.clock
    }

    /// The query has no word to find.
    pub fn no_words(&self) -> bool {
        self.pos.is_empty()
    }

    /// Size or date decide something, so every candidate must be read from
    /// the disk first.
    pub fn needs_stat(&self) -> bool {
        self.query.filters.needs_stat()
    }

    /// How `name` matches the words, the `name:` patterns, the extensions
    /// and the words to leave out; `None` when it does not.
    pub fn name_hit(&self, name: &str) -> Option<Hit> {
        let filters = &self.query.filters;
        if !filters.exts.is_empty() {
            let lower = name.to_lowercase();
            if !filters.exts.iter().any(|ext| {
                lower.len() > ext.len() + 1
                    && lower.ends_with(ext.as_str())
                    && lower.as_bytes()[lower.len() - ext.len() - 1] == b'.'
            }) {
                return None;
            }
        }
        if !filters.globs.iter().all(|glob| name_glob(glob, name, self.case)) {
            return None;
        }
        let needs_chars = !self.pos.is_empty() || !self.neg.is_empty();
        if !needs_chars {
            return Some(Hit {
                tier: Tier::Any,
                penalty: 0,
                ranges: Vec::new(),
            });
        }
        let chars: Vec<(usize, char)> = name.char_indices().map(|(at, c)| (at, fold(c, self.case))).collect();
        let name_len = name.len();
        let byte_at = |index: usize| chars.get(index).map_or(name_len, |(at, _)| *at);
        let find = |needle: &[char], at_word: bool| find_in(&chars, needle, at_word);
        if self.neg.iter().any(|pat| find(&pat.chars, false).is_some()) {
            return None;
        }
        if self.pos.is_empty() {
            return Some(Hit {
                tier: Tier::Any,
                penalty: 0,
                ranges: Vec::new(),
            });
        }
        match filters.name_mode {
            NameMode::Regex => {
                let found = self.regex.as_ref()?.find(name)?;
                let tier = if found.start() == 0 && found.end() == name_len {
                    Tier::Exact
                } else if found.start() == 0 {
                    Tier::Prefix
                } else {
                    Tier::Substring
                };
                Some(Hit {
                    tier,
                    penalty: 0,
                    ranges: vec![found.start()..found.end()],
                })
            }
            NameMode::Exact | NameMode::Starts => {
                let whole = self.whole();
                let starts = chars.len() >= whole.len() && chars.iter().zip(&whole).all(|((_, c), w)| c == w);
                if !starts {
                    return None;
                }
                if chars.len() == whole.len() {
                    Some(Hit {
                        tier: Tier::Exact,
                        penalty: 0,
                        ranges: vec![0..name_len],
                    })
                } else if filters.name_mode == NameMode::Starts {
                    Some(Hit {
                        tier: Tier::Prefix,
                        penalty: 0,
                        ranges: vec![0..byte_at(whole.len())],
                    })
                } else {
                    None
                }
            }
            NameMode::Contains => self.contains(&chars, &byte_at),
        }
    }

    /// The words as one text, joined by spaces.
    fn whole(&self) -> Vec<char> {
        let mut chars = Vec::new();
        for (at, pat) in self.pos.iter().enumerate() {
            if at > 0 {
                chars.push(' ');
            }
            chars.extend(&pat.chars);
        }
        chars
    }

    fn contains(&self, chars: &[(usize, char)], byte_at: &dyn Fn(usize) -> usize) -> Option<Hit> {
        let name_len = byte_at(chars.len());
        let range_of = |from: usize, count: usize| byte_at(from)..byte_at(from + count);
        let hit = |tier, ranges| {
            Some(Hit {
                tier,
                penalty: 0,
                ranges,
            })
        };
        let whole: Option<&Pat> = match self.pos.as_slice() {
            [only] => Some(only),
            _ => self.joined.as_ref(),
        };
        if let Some(whole) = whole {
            let query = &whole.chars;
            if chars.len() == query.len() && find_in(chars, query, false) == Some(0) {
                return hit(Tier::Exact, vec![0..name_len]);
            }
            if find_in(chars, query, false) == Some(0) {
                return hit(Tier::Prefix, vec![range_of(0, query.len())]);
            }
            if let Some(start) = find_in(chars, query, true) {
                return hit(Tier::Word, vec![range_of(start, query.len())]);
            }
            if let Some(start) = find_in(chars, query, false) {
                return hit(Tier::Substring, vec![range_of(start, query.len())]);
            }
        }
        if self.pos.len() > 1 {
            let mut ranges = Vec::new();
            for term in &self.pos {
                let start = find_in(chars, &term.chars, true).or_else(|| find_in(chars, &term.chars, false))?;
                ranges.push(range_of(start, term.chars.len()));
            }
            return hit(Tier::Words, merged(ranges));
        }
        if self.pos[0].phrase {
            return None;
        }
        self.fuzzy(chars, byte_at)
    }

    /// The letters in order, as tightly as they fit.
    fn fuzzy(&self, chars: &[(usize, char)], byte_at: &dyn Fn(usize) -> usize) -> Option<Hit> {
        let letters: Vec<char> = self.pos[0].chars.iter().copied().filter(|c| !c.is_whitespace()).collect();
        if letters.len() < FUZZY_MIN || letters.len() > chars.len() {
            return None;
        }
        // Forward to where the last letter lands, then back to the latest
        // start that still fits: the tightest window ending there.
        let mut next = 0;
        let mut end = None;
        for (index, (_, c)) in chars.iter().enumerate() {
            if *c == letters[next] {
                next += 1;
                if next == letters.len() {
                    end = Some(index);
                    break;
                }
            }
        }
        let end = end?;
        let mut positions = vec![end];
        let mut wanted = letters.len() - 1;
        let mut index = end;
        while wanted > 0 {
            index -= 1;
            if chars[index].1 == letters[wanted - 1] {
                wanted -= 1;
                positions.push(index);
            }
        }
        positions.reverse();
        let start = positions[0];
        let window = end - start + 1;
        if window > letters.len() * 2 + 4 {
            return None;
        }
        let ranges = merged(
            positions
                .iter()
                .map(|position| byte_at(*position)..byte_at(*position + 1))
                .collect(),
        );
        let penalty = ((window - letters.len()) * 4 + start) as u32;
        Some(Hit {
            tier: Tier::Fuzzy,
            penalty,
            ranges,
        })
    }

    /// Whether the index may offer `name` as a near miss of the words: it
    /// passes the extension, the `name:` patterns and the words to leave out,
    /// and the words are matched without regard to case in `contains` mode.
    pub fn typo_accepts(&self, name: &str) -> bool {
        let filters = &self.query.filters;
        if self.pos.is_empty() || filters.name_mode != NameMode::Contains || self.case {
            return false;
        }
        if !filters.exts.is_empty() {
            let lower = name.to_lowercase();
            if !filters.exts.iter().any(|ext| lower.ends_with(&format!(".{ext}"))) {
                return false;
            }
        }
        if !filters.globs.iter().all(|glob| name_glob(glob, name, self.case)) {
            return false;
        }
        let chars: Vec<(usize, char)> = name.char_indices().map(|(at, c)| (at, fold(c, false))).collect();
        !self.neg.iter().any(|pat| find_in(&chars, &pat.chars, false).is_some())
    }

    /// The bytes of `name` to draw heavier; empty when it does not match.
    pub fn highlight(&self, name: &str) -> Vec<Range<usize>> {
        self.name_hit(name).map(|hit| hit.ranges).unwrap_or_default()
    }

    /// Whether an entry the walker saw by name and kind only can still pass.
    /// `is_dir` and `link` come from the directory entry, which costs no
    /// `stat`; a link is let through, because its target decides.
    pub fn pre_accepts(&self, name: &str, is_dir: bool, link: bool) -> bool {
        let filters = &self.query.filters;
        if !filters.kinds.is_empty() && !link {
            let kind = if is_dir {
                Some(Kind::Folder)
            } else {
                Kind::of(mime::for_name(name).category)
            };
            if !kind.is_some_and(|kind| filters.kinds.contains(&kind)) {
                return false;
            }
        }
        if filters.size.is_some() && is_dir {
            return false;
        }
        true
    }

    /// Whether `entry`, read from the disk, passes `type:`, `size:`,
    /// `modified:` and `hidden:`.
    pub fn accepts(&self, entry: &Entry) -> bool {
        let filters = &self.query.filters;
        if !filters.kinds.is_empty() {
            let kind = if entry.is_dir() {
                Some(Kind::Folder)
            } else {
                Kind::of(entry.category())
            };
            if !kind.is_some_and(|kind| filters.kinds.contains(&kind)) {
                return false;
            }
        }
        match filters.hidden {
            Hidden::Only if !entry.hidden => return false,
            Hidden::Exclude if entry.hidden => return false,
            _ => {}
        }
        if let Some(size) = filters.size
            && (entry.is_dir() || !size.contains(entry.size))
        {
            return false;
        }
        if filters.modified.is_some() {
            let Some(seconds) = entry.modified.map(seconds_since_epoch) else {
                return false;
            };
            let (from, to) = self.when;
            if from.is_some_and(|from| seconds < from) || to.is_some_and(|to| seconds >= to) {
                return false;
            }
        }
        true
    }

    /// The whole test: name and entry.
    pub fn matches(&self, entry: &Entry) -> bool {
        self.name_hit(&entry.name).is_some() && self.accepts(entry)
    }

    /// Where `entry` stands in relevance order among results below `root`.
    /// An entry the words do not match (the index found it by a near miss)
    /// ranks as a typo.
    pub fn rank(&self, entry: &Entry, root: &Path) -> Rank {
        let (tier, penalty) = match self.name_hit(&entry.name) {
            Some(hit) => (hit.tier, hit.penalty),
            None => (Tier::Typo, 0),
        };
        Rank {
            tier,
            penalty,
            depth: depth_below(&entry.path, root),
        }
    }
}

/// How many folders deep `path` is below `root`: 0 for a file in `root`.
pub fn depth_below(path: &Path, root: &Path) -> u32 {
    path.strip_prefix(root)
        .map_or(0, |rest| rest.components().count().saturating_sub(1) as u32)
}

fn seconds_since_epoch(time: std::time::SystemTime) -> i64 {
    match time.duration_since(std::time::UNIX_EPOCH) {
        Ok(after) => after.as_secs() as i64,
        Err(before) => -(before.duration().as_secs() as i64),
    }
}

fn name_glob(pattern: &str, name: &str, case: bool) -> bool {
    if pattern.contains(['*', '?', '[']) {
        if case { glob_match_exact(pattern, name) } else { glob_match(pattern, name) }
    } else if case {
        // No wildcard: the name contains it, as a plain word does.
        name.contains(pattern)
    } else {
        name.to_lowercase().contains(&pattern.to_lowercase())
    }
}

/// The first place `needle` fits in `chars` (the name, already folded);
/// with `at_word` only where a word starts.
fn find_in(chars: &[(usize, char)], needle: &[char], at_word: bool) -> Option<usize> {
    if needle.is_empty() || chars.len() < needle.len() {
        return None;
    }
    (0..=chars.len() - needle.len()).find(|&start| {
        let starts = start == 0 || !chars[start - 1].1.is_alphanumeric();
        (!at_word || starts)
            && needle
                .iter()
                .enumerate()
                .all(|(offset, c)| chars[start + offset].1 == *c)
    })
}

/// Sort `ranges` and join those that touch.
fn merged(mut ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    ranges.sort_by_key(|range| range.start);
    let mut out: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
    for range in ranges {
        match out.last_mut() {
            Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
            _ => out.push(range),
        }
    }
    out
}

/// The span of `modified`, as seconds since the epoch: from, and the first
/// second that is too late.
fn modified_range(modified: Modified, clock: &Clock) -> (Option<i64>, Option<i64>) {
    let today = clock.today();
    match modified {
        Modified::Today => {
            let start = clock.midnight(today);
            (Some(start), Some(clock.midnight(next_day(today))))
        }
        Modified::Yesterday => (Some(clock.midnight(previous_day(today))), Some(clock.midnight(today))),
        Modified::Days(days) => (Some(clock.now - i64::from(days) * 86_400), None),
        Modified::ThisYear => {
            let first = super::civil::Date {
                year: today.year,
                month: 1,
                day: 1,
            };
            let next = super::civil::Date {
                year: today.year + 1,
                month: 1,
                day: 1,
            };
            (Some(clock.midnight(first)), Some(clock.midnight(next)))
        }
        Modified::On(moment) => (Some(clock.midnight(moment.start())), Some(clock.midnight(moment.end()))),
        Modified::Since(moment) => (Some(clock.midnight(moment.start())), None),
        Modified::Before(moment) => (None, Some(clock.midnight(moment.start()))),
        Modified::Between(from, to) => (Some(clock.midnight(from.start())), Some(clock.midnight(to.end()))),
    }
}

fn next_day(date: super::civil::Date) -> super::civil::Date {
    super::civil::civil_from_days(super::civil::days_from_civil(date) + 1)
}

fn previous_day(date: super::civil::Date) -> super::civil::Date {
    super::civil::civil_from_days(super::civil::days_from_civil(date) - 1)
}

fn regex_reason(error: &regex::Error) -> String {
    match error {
        regex::Error::CompiledTooBig(_) => "regular expression is too big".to_string(),
        _ => "not a valid regular expression".to_string(),
    }
}
