//! Type-ahead for menus and lists: the typed letters build a query, and the
//! rows that match it light up while the others dim. The module is pure: it
//! reads no clock and draws nothing, so the rules are tested without a window.
//!
//! # What the person gets
//!
//! - Typing while a menu or list is open does not close it. The characters
//!   build a query.
//! - **The query never times out.** It ends only when the person edits it
//!   away or acts on it: Backspace removes the last character, Ctrl+Backspace
//!   the last word, Delete or Ctrl+A everything, Escape everything (and only
//!   the next Escape closes the list), and picking a row or closing the list
//!   drops it. A pause of any length, as a slow typist or someone using
//!   assistive technology makes, leaves it as it was, and typing goes on from
//!   there. Nothing in this module can age a query: it has no clock.
//! - A row matches when its label contains the query, ignoring case. A label
//!   that starts with the query ranks first, then one with a word that starts
//!   with it, then any other substring. The first match of the best rank is
//!   focused, and the matched letters of every match are marked.
//! - Down, Up, Tab, Shift+Tab, Ctrl+N and Ctrl+P walk through the matches
//!   only, wrapping at the ends. Enter activates the focused match.
//! - Space types a space while a query is active. Otherwise it activates, as
//!   it always did.
//! - Rows that cannot be picked (disabled rows, separators) never match.
//!
//! # How a list uses it
//!
//! The list turns its rows into [`Candidate`]s and hands each key press to
//! [`TypeAhead::key`]. The answer is a [`Reaction`]: focus a row, activate a
//! row, swallow the key, or leave it to the list. The list draws each row
//! from [`TypeAhead::mark`] and the query pill from [`TypeAhead::pill`], and
//! calls [`TypeAhead::clear`] when it closes or a row is picked.
//!
//! The one place a reset is allowed is a dropdown whose list is shut: there
//! typing picks a value at once and no pill shows the query, so a stale
//! prefix would be invisible. That reset belongs to the window side
//! ([`super::type_ahead`]), not to this module.

use std::ops::Range;

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
    /// Ctrl+Backspace: the last word.
    BackWord,
    /// Delete or Ctrl+A: the whole query.
    Clear,
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

    /// Forgets the query. It returns whether there was one.
    pub fn clear(&mut self) -> bool {
        let was = self.is_active();
        self.query.clear();
        self.matches = Matches::default();
        was
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

    /// Handles `key` for a list of `items` whose row `focused` has the
    /// keyboard focus. There is no time here on purpose: a query only ends
    /// when the person ends it.
    pub fn key(&mut self, key: Key, items: &[Candidate], focused: Option<usize>) -> Reaction {
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
                self.refresh(items)
            }
            Key::Backspace => {
                if !self.is_active() {
                    return Reaction::Ignored;
                }
                self.query.pop();
                self.refresh(items)
            }
            Key::BackWord => {
                if !self.is_active() {
                    return Reaction::Ignored;
                }
                // Trailing spaces go with the word before them.
                self.query.truncate(self.query.trim_end().len());
                let word_start = self
                    .query
                    .char_indices()
                    .rev()
                    .find(|(_, c)| c.is_whitespace())
                    .map_or(0, |(at, c)| at + c.len_utf8());
                self.query.truncate(word_start);
                self.refresh(items)
            }
            Key::Escape | Key::Clear => {
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
    /// An emptied query shows nothing and has nothing to focus.
    fn refresh(&mut self, items: &[Candidate]) -> Reaction {
        if self.query.is_empty() {
            self.matches = Matches::default();
            return Reaction::Handled;
        }
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

    fn type_all(ahead: &mut TypeAhead, text: &str, items: &[Candidate]) -> Reaction {
        let mut last = Reaction::Ignored;
        for c in text.chars() {
            last = ahead.key(Key::Char(c), items, None);
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
        type_all(&mut ahead, "co", &items);
        assert_eq!(ahead.mark(0), Mark::Plain);
        assert_eq!(ahead.mark(1), Mark::Hit(0..2));
    }

    #[test]
    fn typing_focuses_the_best_match_and_marks_the_rows() {
        let items = rows(&["Copy", "Open", "Paste"]);
        let mut ahead = TypeAhead::default();
        assert_eq!(ahead.mark(0), Mark::Plain);
        // A prefix beats an earlier row that only contains the letter.
        assert_eq!(ahead.key(Key::Char('o'), &items, None), Reaction::Focus(1));
        assert_eq!(ahead.mark(0), Mark::Hit(1..2));
        assert_eq!(ahead.mark(1), Mark::Hit(0..1));
        assert_eq!(ahead.mark(2), Mark::Dim);
        assert_eq!(ahead.key(Key::Char('p'), &items, Some(1)), Reaction::Focus(1));
        assert_eq!(ahead.mark(0), Mark::Hit(1..3));
        assert_eq!(ahead.mark(1), Mark::Hit(0..2));
    }

    #[test]
    fn a_query_with_no_match_dims_everything_and_takes_the_key() {
        let items = rows(&["Open", "Copy"]);
        let mut ahead = TypeAhead::default();
        assert_eq!(type_all(&mut ahead, "zz", &items), Reaction::Handled);
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
        type_all(&mut ahead, "op", &items);
        let pill = ahead.pill().expect("a pill");
        assert_eq!(pill.count, 2);
        assert_eq!(pill.announcement(), "2 matches for op");
        type_all(&mut ahead, "en w", &items);
        assert_eq!(ahead.pill().map(|pill| pill.announcement()), Some("1 match for open w".to_owned()));
    }

    #[test]
    fn down_and_up_cycle_through_only_the_matches() {
        let items = rows(&["Cut", "Copy", "Paste", "Copy path", "Delete"]);
        let mut ahead = TypeAhead::default();
        assert_eq!(type_all(&mut ahead, "cop", &items), Reaction::Focus(1));
        assert_eq!(ahead.key(Key::Next, &items, Some(1)), Reaction::Focus(3));
        assert_eq!(ahead.key(Key::Next, &items, Some(3)), Reaction::Focus(1));
        assert_eq!(ahead.key(Key::Previous, &items, Some(1)), Reaction::Focus(3));
        assert_eq!(ahead.key(Key::Previous, &items, Some(3)), Reaction::Focus(1));
    }

    #[test]
    fn cycling_starts_from_a_focus_that_is_not_a_match() {
        let items = rows(&["Cut", "Copy", "Paste", "Copy path"]);
        let mut ahead = TypeAhead::default();
        type_all(&mut ahead, "copy", &items);
        assert_eq!(ahead.key(Key::Next, &items, Some(2)), Reaction::Focus(3));
        assert_eq!(ahead.key(Key::Previous, &items, Some(2)), Reaction::Focus(1));
        assert_eq!(ahead.key(Key::Next, &items, None), Reaction::Focus(1));
        assert_eq!(ahead.key(Key::Previous, &items, None), Reaction::Focus(3));
    }

    #[test]
    fn arrows_with_no_query_are_the_lists_own() {
        let items = rows(&["Cut", "Copy"]);
        let mut ahead = TypeAhead::default();
        assert_eq!(ahead.key(Key::Next, &items, Some(0)), Reaction::Ignored);
        assert_eq!(ahead.key(Key::Previous, &items, Some(0)), Reaction::Ignored);
        assert_eq!(ahead.key(Key::Enter, &items, Some(0)), Reaction::Ignored);
        assert_eq!(ahead.key(Key::Backspace, &items, Some(0)), Reaction::Ignored);
        assert_eq!(ahead.key(Key::BackWord, &items, Some(0)), Reaction::Ignored);
        assert_eq!(ahead.key(Key::Clear, &items, Some(0)), Reaction::Ignored);
        assert_eq!(ahead.key(Key::Escape, &items, Some(0)), Reaction::Ignored);
    }

    #[test]
    fn with_no_match_the_arrows_stay_put() {
        let items = rows(&["Cut", "Copy"]);
        let mut ahead = TypeAhead::default();
        type_all(&mut ahead, "x", &items);
        assert_eq!(ahead.key(Key::Next, &items, Some(1)), Reaction::Handled);
        assert_eq!(ahead.key(Key::Enter, &items, Some(1)), Reaction::Handled);
    }

    #[test]
    fn enter_activates_the_focused_match() {
        let items = rows(&["Copy", "Copy path"]);
        let mut ahead = TypeAhead::default();
        type_all(&mut ahead, "copy", &items);
        assert_eq!(ahead.key(Key::Enter, &items, Some(1)), Reaction::Activate(1));
        // A focus that is not a match gives way to the best match.
        let mixed = rows(&["Paste", "Copy"]);
        let mut other = TypeAhead::default();
        type_all(&mut other, "copy", &mixed);
        assert_eq!(other.key(Key::Enter, &mixed, Some(0)), Reaction::Activate(1));
    }

    #[test]
    fn escape_clears_the_query_and_the_next_escape_is_the_lists() {
        let items = rows(&["Copy"]);
        let mut ahead = TypeAhead::default();
        type_all(&mut ahead, "co", &items);
        assert_eq!(ahead.key(Key::Escape, &items, Some(0)), Reaction::Handled);
        assert!(!ahead.is_active());
        assert_eq!(ahead.mark(0), Mark::Plain);
        assert_eq!(ahead.pill(), None);
        assert_eq!(ahead.key(Key::Escape, &items, Some(0)), Reaction::Ignored);
    }

    #[test]
    fn backspace_edits_the_query() {
        let items = rows(&["Copy", "Cut", "Paste"]);
        let mut ahead = TypeAhead::default();
        assert_eq!(type_all(&mut ahead, "cu", &items), Reaction::Focus(1));
        assert_eq!(ahead.key(Key::Backspace, &items, Some(1)), Reaction::Focus(0));
        assert_eq!(ahead.query(), "c");
        assert_eq!(ahead.key(Key::Backspace, &items, Some(0)), Reaction::Handled);
        assert!(!ahead.is_active());
        assert_eq!(ahead.key(Key::Backspace, &items, Some(0)), Reaction::Ignored);
    }

    #[test]
    fn backspace_takes_a_whole_character_not_a_byte() {
        let items = rows(&["Écoute", "Café"]);
        let mut ahead = TypeAhead::default();
        type_all(&mut ahead, "café", &items);
        assert_eq!(ahead.key(Key::Backspace, &items, Some(1)), Reaction::Focus(1));
        assert_eq!(ahead.query(), "caf");
    }

    #[test]
    fn ctrl_backspace_takes_the_last_word() {
        let items = rows(&["Open with", "Open", "Paste"]);
        let mut ahead = TypeAhead::default();
        type_all(&mut ahead, "open wi", &items);
        assert_eq!(ahead.key(Key::BackWord, &items, Some(0)), Reaction::Focus(0));
        assert_eq!(ahead.query(), "open ");
        // Spaces at the end go with the word before them.
        assert_eq!(ahead.key(Key::BackWord, &items, Some(0)), Reaction::Handled);
        assert!(!ahead.is_active());
        assert_eq!(ahead.key(Key::BackWord, &items, Some(0)), Reaction::Ignored);
    }

    #[test]
    fn ctrl_backspace_on_one_word_clears_the_query() {
        let items = rows(&["Open"]);
        let mut ahead = TypeAhead::default();
        type_all(&mut ahead, "ope", &items);
        assert_eq!(ahead.key(Key::BackWord, &items, Some(0)), Reaction::Handled);
        assert!(!ahead.is_active());
        assert_eq!(ahead.mark(0), Mark::Plain);
    }

    #[test]
    fn clear_empties_the_query_and_only_with_one() {
        let items = rows(&["Copy", "Cut"]);
        let mut ahead = TypeAhead::default();
        assert_eq!(ahead.key(Key::Clear, &items, None), Reaction::Ignored);
        type_all(&mut ahead, "cop", &items);
        assert_eq!(ahead.key(Key::Clear, &items, Some(0)), Reaction::Handled);
        assert!(!ahead.is_active());
        assert_eq!(ahead.pill(), None);
        assert_eq!(ahead.mark(0), Mark::Plain);
    }

    #[test]
    fn space_types_only_while_a_query_is_active() {
        let items = rows(&["Open with", "Open"]);
        let mut ahead = TypeAhead::default();
        assert_eq!(ahead.key(Key::Char(' '), &items, Some(0)), Reaction::Ignored);
        assert!(!ahead.is_active());
        type_all(&mut ahead, "open", &items);
        assert_eq!(ahead.key(Key::Char(' '), &items, Some(1)), Reaction::Focus(0));
        assert_eq!(ahead.query(), "open ");
    }

    #[test]
    fn control_characters_are_not_text() {
        let items = rows(&["Open"]);
        let mut ahead = TypeAhead::default();
        assert_eq!(ahead.key(Key::Char('\u{8}'), &items, None), Reaction::Ignored);
        assert!(!ahead.is_active());
    }

    // The query takes no time, so no pause can end it; the menu's own tests
    // (`menu_nav`) run the same promise against a clock.
    #[test]
    fn the_query_stays_exactly_as_typed_until_the_person_changes_it() {
        let items = rows(&["English", "Español", "Deutsch"]);
        let mut ahead = TypeAhead::default();
        assert_eq!(type_all(&mut ahead, "eng", &items), Reaction::Focus(0));
        // Reading marks and the pill changes nothing.
        for _ in 0..3 {
            assert_eq!(ahead.query(), "eng");
            assert_eq!(ahead.mark(0), Mark::Hit(0..3));
            assert_eq!(ahead.mark(1), Mark::Dim);
            assert_eq!(ahead.pill().map(|pill| pill.count), Some(1));
        }
        // Typing carries on from where it was.
        assert_eq!(ahead.key(Key::Char('l'), &items, Some(0)), Reaction::Focus(0));
        assert_eq!(ahead.query(), "engl");
    }

    #[test]
    fn walking_and_enter_do_not_end_the_query() {
        let items = rows(&["Copy", "Copy path"]);
        let mut ahead = TypeAhead::default();
        type_all(&mut ahead, "cop", &items);
        assert_eq!(ahead.key(Key::Next, &items, Some(0)), Reaction::Focus(1));
        assert_eq!(ahead.query(), "cop");
        assert_eq!(ahead.key(Key::Enter, &items, Some(1)), Reaction::Activate(1));
        // Picking is the list's: it clears the query when it closes.
        assert!(ahead.is_active());
        assert!(ahead.clear());
        assert!(!ahead.clear());
    }

    #[test]
    fn a_typed_edit_and_escape_sequence_ends_with_the_list_closing() {
        let items = rows(&["English", "Español"]);
        let mut ahead = TypeAhead::default();
        type_all(&mut ahead, "eng", &items);
        type_all(&mut ahead, "x", &items);
        assert_eq!(ahead.pill().map(|pill| pill.count), Some(0));
        assert_eq!(ahead.key(Key::Backspace, &items, Some(0)), Reaction::Focus(0));
        assert_eq!(ahead.query(), "eng");
        assert_eq!(ahead.key(Key::Escape, &items, Some(0)), Reaction::Handled);
        assert_eq!(ahead.key(Key::Escape, &items, Some(0)), Reaction::Ignored);
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
        assert_eq!(type_all(&mut ahead, "r-19", &items), Reaction::Focus(1900));
        assert_eq!(ahead.matches().count(), 100);
        assert_eq!(ahead.key(Key::Previous, &items, Some(1900)), Reaction::Focus(1999));
    }
}
