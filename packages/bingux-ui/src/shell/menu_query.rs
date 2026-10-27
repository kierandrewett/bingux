//! Type-ahead for menus and lists: the typed letters build a query, and the
//! rows that match it light up while the others dim. The module is pure: it
//! reads no clock and draws nothing, so the rules are tested without a window.
//!
//! # What the person gets
//!
//! - Typing while a menu or list is open does not close it. The characters
//!   build a query, which restarts once nothing was typed for
//!   [`RESET_MS`]; Backspace edits it.
//! - A row matches when its label contains the query, ignoring case. A label
//!   that starts with the query ranks first, then one with a word that starts
//!   with it, then any other substring. The first match of the best rank is
//!   focused, and the matched letters of every match are marked.
//! - Down, Up, Tab, Shift+Tab, Ctrl+N and Ctrl+P walk through the matches
//!   only, wrapping at the ends. Enter activates the focused match. Escape
//!   clears the query, and only then closes the list.
//! - Space types a space while a query is active. Otherwise it activates, as
//!   it always did.
//! - Rows that cannot be picked (disabled rows, separators) never match.
//!
//! # How a list uses it
//!
//! The list turns its rows into [`Candidate`]s and hands each key press to
//! [`TypeAhead::key`] with the time. The answer is a [`Reaction`]: focus a
//! row, activate a row, swallow the key, or leave it to the list. The list
//! draws each row from [`TypeAhead::mark`] and the query pill from
//! [`TypeAhead::pill`], and calls [`TypeAhead::expire`] when the reset time
//! has passed so a stale query goes away without a key press.

use std::ops::Range;

/// A time in milliseconds on any clock that only moves forward. The caller
/// supplies it, so a test can use a clock it controls.
pub type Ms = u64;

/// How long a query lives after the last key. A key after that starts a new
/// query.
pub const RESET_MS: Ms = 1000;

/// How a label matches the query, best first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MatchKind {
    /// The label starts with the query.
    Prefix,
    /// A word of the label starts with the query. A word starts after
    /// anything that is not a letter or a digit.
    Word,
    /// The query is somewhere inside a word.
    Substring,
}

/// Where and how a label matches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    pub kind: MatchKind,
    /// The matched text, as byte offsets into the label on character
    /// boundaries.
    pub bytes: Range<usize>,
}

/// A row, as the query sees it.
#[derive(Clone, Copy, Debug)]
pub struct Candidate<'a> {
    pub label: &'a str,
    /// Whether the row can be picked. Rows that cannot never match.
    pub selectable: bool,
}

/// How a row shows while a query is active.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mark {
    /// No query, or a row the query does not apply to: draw it as usual.
    Plain,
    /// A selectable row that does not match: draw it dimmed.
    Dim,
    /// A match: draw the matched bytes of the label marked.
    Hit(Range<usize>),
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
enum Slot {
    /// A row that cannot match.
    #[default]
    Skipped,
    Miss,
    Hit(Hit),
}

/// The rows a query matches, in the order of the rows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Matches {
    slots: Vec<Slot>,
    count: usize,
    best: Option<usize>,
}

fn lower(c: char) -> char {
    // One character in, one character out, so positions line up.
    let mut lowered = c.to_lowercase();
    match (lowered.next(), lowered.next()) {
        (Some(single), None) => single,
        _ => c,
    }
}

/// How `label` matches `needle` (already lower case): the best way, and the
/// first place in that way.
fn find_hit(needle: &[char], label: &str) -> Option<Hit> {
    let chars: Vec<(usize, char)> = label
        .char_indices()
        .map(|(at, c)| (at, lower(c)))
        .collect();
    if needle.is_empty() || needle.len() > chars.len() {
        return None;
    }
    let end_of = |last: usize| chars.get(last + 1).map_or(label.len(), |next| next.0);
    let mut inside = None;
    for start in 0..=chars.len() - needle.len() {
        if !needle
            .iter()
            .zip(&chars[start..])
            .all(|(wanted, (_, c))| wanted == c)
        {
            continue;
        }
        let bytes = chars[start].0..end_of(start + needle.len() - 1);
        if start == 0 {
            return Some(Hit {
                kind: MatchKind::Prefix,
                bytes,
            });
        }
        if !chars[start - 1].1.is_alphanumeric() {
            // A word start beats any substring, and no later one can beat it.
            return Some(Hit {
                kind: MatchKind::Word,
                bytes,
            });
        }
        inside.get_or_insert(bytes);
    }
    inside.map(|bytes| Hit {
        kind: MatchKind::Substring,
        bytes,
    })
}

impl Matches {
    /// The rows of `items` that match `query`. A blank query matches nothing.
    pub fn rank(query: &str, items: &[Candidate]) -> Self {
        let needle: Vec<char> = query.chars().map(lower).collect();
        let mut slots = Vec::with_capacity(items.len());
        let mut count = 0;
        let mut best: Option<(MatchKind, usize)> = None;
        for (index, item) in items.iter().enumerate() {
            if !item.selectable {
                slots.push(Slot::Skipped);
                continue;
            }
            match find_hit(&needle, item.label) {
                Some(hit) => {
                    count += 1;
                    // Rows are visited in order, so only a strictly better
                    // kind replaces the best: ties keep the first row.
                    if best.is_none_or(|(kind, _)| hit.kind < kind) {
                        best = Some((hit.kind, index));
                    }
                    slots.push(Slot::Hit(hit));
                }
                None => slots.push(Slot::Miss),
            }
        }
        Self {
            slots,
            count,
            best: best.map(|(_, index)| index),
        }
    }

    /// The number of matching rows.
    pub fn count(&self) -> usize {
        self.count
    }

    /// The row that ranks first: the best kind of match, then the topmost.
    pub fn best(&self) -> Option<usize> {
        self.best
    }

    /// Where row `index` matches.
    pub fn hit(&self, index: usize) -> Option<&Hit> {
        match self.slots.get(index) {
            Some(Slot::Hit(hit)) => Some(hit),
            _ => None,
        }
    }

    /// The matching row after `from` (or before it, when not `forward`),
    /// wrapping round the ends. With no `from`, the first (or last) match.
    pub fn step(&self, from: Option<usize>, forward: bool) -> Option<usize> {
        let len = self.slots.len();
        if self.count == 0 || len == 0 {
            return None;
        }
        let mut at = match (from, forward) {
            (Some(index), _) => index.min(len - 1),
            (None, true) => len - 1,
            (None, false) => 0,
        };
        for _ in 0..len {
            at = if forward {
                (at + 1) % len
            } else {
                (at + len - 1) % len
            };
            if self.hit(at).is_some() {
                return Some(at);
            }
        }
        None
    }
}

/// A key press, as the query understands it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// A printable character, Space included.
    Char(char),
    Backspace,
    /// Down, Tab or Ctrl+N.
    Next,
    /// Up, Shift+Tab or Ctrl+P.
    Previous,
    Enter,
    Escape,
}

/// What a list does with a key after asking the query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reaction {
    /// Not the query's key: the list handles it as it always did.
    Ignored,
    /// The query took the key and has nothing to focus.
    Handled,
    /// Move the keyboard focus to this row, scrolling it into view.
    Focus(usize),
    /// Activate this row, as Enter on it would.
    Activate(usize),
}

/// What the query pill shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pill {
    pub query: String,
    pub count: usize,
}

impl Pill {
    /// The text a screen reader announces when the pill changes.
    pub fn announcement(&self) -> String {
        match self.count {
            0 => format!("No matches for {}", self.query),
            1 => format!("1 match for {}", self.query),
            count => format!("{count} matches for {}", self.query),
        }
    }

    /// The short count shown beside the query.
    pub fn count_text(&self) -> String {
        match self.count {
            0 => "No matches".to_owned(),
            1 => "1 match".to_owned(),
            count => format!("{count} matches"),
        }
    }
}

/// The typed query of one open list and the rows it matches.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TypeAhead {
    query: String,
    typed_at: Ms,
    matches: Matches,
}

impl TypeAhead {
    /// The query typed so far. Empty when there is none.
    #[cfg(test)]
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Whether a query is showing.
    pub fn is_active(&self) -> bool {
        !self.query.is_empty()
    }

    /// The time the query ends if nothing more is typed, for scheduling the
    /// call to [`Self::expire`].
    pub fn expires_at(&self) -> Option<Ms> {
        self.is_active().then(|| self.typed_at + RESET_MS)
    }

    /// Forgets the query. It returns whether there was one.
    pub fn clear(&mut self) -> bool {
        let was = self.is_active();
        self.query.clear();
        self.matches = Matches::default();
        was
    }

    /// Forgets the query once [`RESET_MS`] has passed since the last key. It
    /// returns whether it did, so the list knows to draw again.
    pub fn expire(&mut self, now: Ms) -> bool {
        match self.expires_at() {
            Some(end) if now >= end => self.clear(),
            _ => false,
        }
    }

    /// How row `index` shows now.
    pub fn mark(&self, index: usize) -> Mark {
        if !self.is_active() {
            return Mark::Plain;
        }
        match self.matches.slots.get(index) {
            Some(Slot::Hit(hit)) => Mark::Hit(hit.bytes.clone()),
            Some(Slot::Miss) => Mark::Dim,
            _ => Mark::Plain,
        }
    }

    /// The pill to show, or `None` when there is no query.
    pub fn pill(&self) -> Option<Pill> {
        self.is_active().then(|| Pill {
            query: self.query.clone(),
            count: self.matches.count(),
        })
    }

    /// The matches of the current query.
    #[cfg(test)]
    pub fn matches(&self) -> &Matches {
        &self.matches
    }

    /// Handles `key` at time `now` for a list of `items` whose row `focused`
    /// has the keyboard focus.
    ///
    /// A query that ran out before this key is dropped first, so it never
    /// decides anything.
    pub fn key(
        &mut self,
        key: Key,
        now: Ms,
        items: &[Candidate],
        focused: Option<usize>,
    ) -> Reaction {
        self.expire(now);
        match key {
            Key::Char(c) => {
                if c.is_control() {
                    return Reaction::Ignored;
                }
                // A space starts nothing: with no query it is the list's own
                // activation key.
                if c == ' ' && !self.is_active() {
                    return Reaction::Ignored;
                }
                self.query.push(c);
                self.typed_at = now;
                self.refresh(items)
            }
            Key::Backspace => {
                if !self.is_active() {
                    return Reaction::Ignored;
                }
                self.query.pop();
                self.typed_at = now;
                if self.query.is_empty() {
                    self.matches = Matches::default();
                    return Reaction::Handled;
                }
                self.refresh(items)
            }
            Key::Escape => {
                if self.clear() {
                    Reaction::Handled
                } else {
                    Reaction::Ignored
                }
            }
            Key::Next | Key::Previous => {
                if !self.is_active() {
                    return Reaction::Ignored;
                }
                self.matches = Matches::rank(&self.query, items);
                // Walking is activity too: the query stays while it is used.
                self.typed_at = now;
                let forward = key == Key::Next;
                match self.matches.step(focused, forward) {
                    Some(index) => Reaction::Focus(index),
                    None => Reaction::Handled,
                }
            }
            Key::Enter => {
                if !self.is_active() {
                    return Reaction::Ignored;
                }
                self.matches = Matches::rank(&self.query, items);
                let target = focused
                    .filter(|index| self.matches.hit(*index).is_some())
                    .or(self.matches.best());
                match target {
                    Some(index) => Reaction::Activate(index),
                    None => Reaction::Handled,
                }
            }
        }
    }

    /// Ranks the rows again for the changed query and focuses the best one.
    fn refresh(&mut self, items: &[Candidate]) -> Reaction {
        self.matches = Matches::rank(&self.query, items);
        match self.matches.best() {
            Some(index) => Reaction::Focus(index),
            None => Reaction::Handled,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows<'a>(labels: &[&'a str]) -> Vec<Candidate<'a>> {
        labels
            .iter()
            .map(|label| Candidate {
                label,
                selectable: true,
            })
            .collect()
    }

    fn type_all(ahead: &mut TypeAhead, text: &str, now: Ms, items: &[Candidate]) -> Reaction {
        let mut last = Reaction::Ignored;
        for c in text.chars() {
            last = ahead.key(Key::Char(c), now, items, None);
        }
        last
    }

    #[test]
    fn prefix_ranks_before_word_before_substring() {
        let items = rows(&["My downloads", "Rundown", "Documents", "Downloads"]);
        let matches = Matches::rank("down", &items);
        let kinds: Vec<Option<MatchKind>> = (0..4)
            .map(|index| matches.hit(index).map(|hit| hit.kind))
            .collect();
        assert_eq!(
            kinds,
            [
                Some(MatchKind::Word),
                Some(MatchKind::Substring),
                None,
                Some(MatchKind::Prefix)
            ]
        );
        assert_eq!(matches.best(), Some(3));
        assert_eq!(matches.count(), 3);
    }

    #[test]
    fn the_best_kind_wins_over_an_earlier_row_and_ties_keep_the_first() {
        let items = rows(&["a-cat", "cat", "category"]);
        assert_eq!(Matches::rank("cat", &items).best(), Some(1));
        let items = rows(&["scat", "bobcat", "cats", "cat"]);
        assert_eq!(Matches::rank("ca", &items).best(), Some(2));
    }

    #[test]
    fn matching_ignores_case_and_marks_the_matched_bytes() {
        let items = rows(&["Pictures", "Archive Photos"]);
        let matches = Matches::rank("PHO", &items);
        assert_eq!(matches.hit(0), None);
        let hit = matches.hit(1).expect("a word start");
        assert_eq!(hit.kind, MatchKind::Word);
        assert_eq!(&"Archive Photos"[hit.bytes.clone()], "Pho");
    }

    #[test]
    fn ranges_are_byte_offsets_on_character_boundaries() {
        let items = rows(&["Écoute", "café"]);
        let matches = Matches::rank("é", &items);
        assert_eq!(matches.hit(0).map(|hit| hit.bytes.clone()), Some(0..2));
        let hit = matches.hit(1).expect("inside the word");
        assert_eq!(hit.kind, MatchKind::Substring);
        assert_eq!(&"café"[hit.bytes.clone()], "é");
    }

    #[test]
    fn a_hidden_file_name_matches_at_its_word() {
        let items = rows(&[".config", "reconfig"]);
        let matches = Matches::rank("config", &items);
        assert_eq!(matches.hit(0).map(|hit| hit.kind), Some(MatchKind::Word));
        assert_eq!(matches.hit(1).map(|hit| hit.kind), Some(MatchKind::Substring));
    }

    #[test]
    fn rows_that_cannot_be_picked_never_match() {
        let items = vec![
            Candidate {
                label: "Copy",
                selectable: false,
            },
            Candidate {
                label: "Copy path",
                selectable: true,
            },
        ];
        let matches = Matches::rank("co", &items);
        assert_eq!(matches.count(), 1);
        assert_eq!(matches.best(), Some(1));
        let mut ahead = TypeAhead::default();
        type_all(&mut ahead, "co", 0, &items);
        assert_eq!(ahead.mark(0), Mark::Plain);
        assert_eq!(ahead.mark(1), Mark::Hit(0..2));
    }

    #[test]
    fn typing_focuses_the_best_match_and_marks_the_rows() {
        let items = rows(&["Copy", "Open", "Paste"]);
        let mut ahead = TypeAhead::default();
        assert_eq!(ahead.mark(0), Mark::Plain);
        // A prefix beats an earlier row that only contains the letter.
        assert_eq!(
            ahead.key(Key::Char('o'), 0, &items, None),
            Reaction::Focus(1)
        );
        assert_eq!(ahead.mark(0), Mark::Hit(1..2));
        assert_eq!(ahead.mark(1), Mark::Hit(0..1));
        assert_eq!(ahead.mark(2), Mark::Dim);
        assert_eq!(
            ahead.key(Key::Char('p'), 10, &items, Some(1)),
            Reaction::Focus(1)
        );
        assert_eq!(ahead.mark(0), Mark::Hit(1..3));
        assert_eq!(ahead.mark(1), Mark::Hit(0..2));
    }

    #[test]
    fn a_query_with_no_match_dims_everything_and_takes_the_key() {
        let items = rows(&["Open", "Copy"]);
        let mut ahead = TypeAhead::default();
        assert_eq!(type_all(&mut ahead, "zz", 0, &items), Reaction::Handled);
        assert_eq!(ahead.mark(0), Mark::Dim);
        assert_eq!(ahead.mark(1), Mark::Dim);
        let pill = ahead.pill().expect("a pill");
        assert_eq!(pill.count_text(), "No matches");
        assert_eq!(pill.announcement(), "No matches for zz");
    }

    #[test]
    fn the_pill_counts_the_matches() {
        let items = rows(&["Open", "Open with", "Paste"]);
        let mut ahead = TypeAhead::default();
        assert_eq!(ahead.pill(), None);
        type_all(&mut ahead, "op", 0, &items);
        let pill = ahead.pill().expect("a pill");
        assert_eq!(pill.count, 2);
        assert_eq!(pill.announcement(), "2 matches for op");
        type_all(&mut ahead, "en w", 1, &items);
        assert_eq!(ahead.pill().map(|pill| pill.announcement()), Some("1 match for open w".to_owned()));
    }

    #[test]
    fn down_and_up_cycle_through_only_the_matches() {
        let items = rows(&["Cut", "Copy", "Paste", "Copy path", "Delete"]);
        let mut ahead = TypeAhead::default();
        assert_eq!(type_all(&mut ahead, "cop", 0, &items), Reaction::Focus(1));
        assert_eq!(ahead.key(Key::Next, 1, &items, Some(1)), Reaction::Focus(3));
        assert_eq!(ahead.key(Key::Next, 2, &items, Some(3)), Reaction::Focus(1));
        assert_eq!(ahead.key(Key::Previous, 3, &items, Some(1)), Reaction::Focus(3));
        assert_eq!(ahead.key(Key::Previous, 4, &items, Some(3)), Reaction::Focus(1));
    }

    #[test]
    fn cycling_starts_from_a_focus_that_is_not_a_match() {
        let items = rows(&["Cut", "Copy", "Paste", "Copy path"]);
        let mut ahead = TypeAhead::default();
        type_all(&mut ahead, "copy", 0, &items);
        assert_eq!(ahead.key(Key::Next, 1, &items, Some(2)), Reaction::Focus(3));
        assert_eq!(ahead.key(Key::Previous, 2, &items, Some(2)), Reaction::Focus(1));
        assert_eq!(ahead.key(Key::Next, 3, &items, None), Reaction::Focus(1));
        assert_eq!(ahead.key(Key::Previous, 4, &items, None), Reaction::Focus(3));
    }

    #[test]
    fn arrows_with_no_query_are_the_lists_own() {
        let items = rows(&["Cut", "Copy"]);
        let mut ahead = TypeAhead::default();
        assert_eq!(ahead.key(Key::Next, 0, &items, Some(0)), Reaction::Ignored);
        assert_eq!(ahead.key(Key::Previous, 0, &items, Some(0)), Reaction::Ignored);
        assert_eq!(ahead.key(Key::Enter, 0, &items, Some(0)), Reaction::Ignored);
        assert_eq!(ahead.key(Key::Backspace, 0, &items, Some(0)), Reaction::Ignored);
        assert_eq!(ahead.key(Key::Escape, 0, &items, Some(0)), Reaction::Ignored);
    }

    #[test]
    fn with_no_match_the_arrows_stay_put() {
        let items = rows(&["Cut", "Copy"]);
        let mut ahead = TypeAhead::default();
        type_all(&mut ahead, "x", 0, &items);
        assert_eq!(ahead.key(Key::Next, 1, &items, Some(1)), Reaction::Handled);
        assert_eq!(ahead.key(Key::Enter, 1, &items, Some(1)), Reaction::Handled);
    }

    #[test]
    fn enter_activates_the_focused_match() {
        let items = rows(&["Copy", "Copy path"]);
        let mut ahead = TypeAhead::default();
        type_all(&mut ahead, "copy", 0, &items);
        assert_eq!(ahead.key(Key::Enter, 1, &items, Some(1)), Reaction::Activate(1));
        // A focus that is not a match gives way to the best match.
        let mixed = rows(&["Paste", "Copy"]);
        let mut other = TypeAhead::default();
        type_all(&mut other, "copy", 0, &mixed);
        assert_eq!(other.key(Key::Enter, 1, &mixed, Some(0)), Reaction::Activate(1));
    }

    #[test]
    fn escape_clears_the_query_and_the_next_escape_is_the_lists() {
        let items = rows(&["Copy"]);
        let mut ahead = TypeAhead::default();
        type_all(&mut ahead, "co", 0, &items);
        assert_eq!(ahead.key(Key::Escape, 1, &items, Some(0)), Reaction::Handled);
        assert!(!ahead.is_active());
        assert_eq!(ahead.mark(0), Mark::Plain);
        assert_eq!(ahead.key(Key::Escape, 2, &items, Some(0)), Reaction::Ignored);
    }

    #[test]
    fn backspace_edits_the_query() {
        let items = rows(&["Copy", "Cut", "Paste"]);
        let mut ahead = TypeAhead::default();
        assert_eq!(type_all(&mut ahead, "cu", 0, &items), Reaction::Focus(1));
        assert_eq!(ahead.key(Key::Backspace, 1, &items, Some(1)), Reaction::Focus(0));
        assert_eq!(ahead.query(), "c");
        assert_eq!(ahead.key(Key::Backspace, 2, &items, Some(0)), Reaction::Handled);
        assert!(!ahead.is_active());
        assert_eq!(ahead.key(Key::Backspace, 3, &items, Some(0)), Reaction::Ignored);
    }

    #[test]
    fn space_types_only_while_a_query_is_active() {
        let items = rows(&["Open with", "Open"]);
        let mut ahead = TypeAhead::default();
        assert_eq!(ahead.key(Key::Char(' '), 0, &items, Some(0)), Reaction::Ignored);
        assert!(!ahead.is_active());
        type_all(&mut ahead, "open", 0, &items);
        assert_eq!(ahead.key(Key::Char(' '), 1, &items, Some(1)), Reaction::Focus(0));
        assert_eq!(ahead.query(), "open ");
    }

    #[test]
    fn control_characters_are_not_text() {
        let items = rows(&["Open"]);
        let mut ahead = TypeAhead::default();
        assert_eq!(ahead.key(Key::Char('\u{8}'), 0, &items, None), Reaction::Ignored);
        assert!(!ahead.is_active());
    }

    #[test]
    fn the_query_resets_after_a_second_without_typing() {
        let items = rows(&["Apple", "Banana", "Apricot"]);
        let mut ahead = TypeAhead::default();
        type_all(&mut ahead, "ap", 100, &items);
        assert_eq!(ahead.expires_at(), Some(100 + RESET_MS));
        // Just short of the reset nothing happens.
        assert!(!ahead.expire(100 + RESET_MS - 1));
        assert!(ahead.is_active());
        // A key before the end keeps the query going and moves the end.
        assert_eq!(ahead.key(Key::Char('r'), 900, &items, Some(0)), Reaction::Focus(2));
        assert_eq!(ahead.query(), "apr");
        assert_eq!(ahead.expires_at(), Some(900 + RESET_MS));
        assert!(!ahead.expire(1000 + RESET_MS - 1 - 100));
        assert!(ahead.expire(900 + RESET_MS));
        assert!(!ahead.is_active());
        assert_eq!(ahead.mark(0), Mark::Plain);
        assert_eq!(ahead.expires_at(), None);
    }

    #[test]
    fn a_key_after_the_reset_starts_a_new_query() {
        let items = rows(&["Apple", "Banana"]);
        let mut ahead = TypeAhead::default();
        type_all(&mut ahead, "ap", 0, &items);
        // The timer has not run yet, but the query is stale.
        assert_eq!(
            ahead.key(Key::Char('b'), RESET_MS + 5, &items, Some(0)),
            Reaction::Focus(1)
        );
        assert_eq!(ahead.query(), "b");
    }

    #[test]
    fn a_stale_space_or_backspace_is_the_lists_own() {
        let items = rows(&["Apple"]);
        let mut ahead = TypeAhead::default();
        type_all(&mut ahead, "ap", 0, &items);
        assert_eq!(
            ahead.key(Key::Char(' '), RESET_MS, &items, Some(0)),
            Reaction::Ignored
        );
        assert_eq!(
            ahead.key(Key::Backspace, RESET_MS, &items, Some(0)),
            Reaction::Ignored
        );
    }

    #[test]
    fn cycling_keeps_the_query_alive() {
        let items = rows(&["Copy", "Copy path"]);
        let mut ahead = TypeAhead::default();
        type_all(&mut ahead, "cop", 0, &items);
        assert_eq!(ahead.key(Key::Next, 800, &items, Some(0)), Reaction::Focus(1));
        assert!(!ahead.expire(RESET_MS));
        assert!(ahead.expire(800 + RESET_MS));
    }

    #[test]
    fn hundreds_of_rows_rank_at_once() {
        let labels: Vec<String> = (0..2000).map(|index| format!("folder-{index:04}")).collect();
        let items: Vec<Candidate> = labels
            .iter()
            .map(|label| Candidate {
                label,
                selectable: true,
            })
            .collect();
        let mut ahead = TypeAhead::default();
        assert_eq!(type_all(&mut ahead, "r-19", 0, &items), Reaction::Focus(1900));
        assert_eq!(ahead.matches().count(), 100);
        assert_eq!(ahead.key(Key::Previous, 1, &items, Some(1900)), Reaction::Focus(1999));
    }
}
