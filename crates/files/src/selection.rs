//! Which entries of a list are selected, and the keys that move that.
//!
//! A [`Selection`] holds paths, not row numbers, so it survives the folder
//! being listed again after a change, a re-sort or a filter change: the same
//! files stay selected and the rows move. Every operation that depends on
//! order takes the visible `order` of the list.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// A key that moves the cursor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Move {
    Up,
    Down,
    Home,
    End,
    /// Up by this many rows.
    PageUp(usize),
    PageDown(usize),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    selected: Vec<PathBuf>,
    /// Where a range started: the last row chosen without Shift.
    anchor: Option<PathBuf>,
    /// The row the keyboard is on.
    cursor: Option<PathBuf>,
}

impl Selection {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.selected.is_empty()
    }

    /// The selected paths, in the order they were selected.
    pub fn paths(&self) -> &[PathBuf] {
        &self.selected
    }

    pub fn cursor(&self) -> Option<&Path> {
        self.cursor.as_deref()
    }

    pub fn contains(&self, path: &Path) -> bool {
        self.selected.iter().any(|selected| selected == path)
    }

    pub fn clear(&mut self) {
        *self = Selection::default();
    }

    /// Select only `path`: a plain click, or an arrow key.
    pub fn select_only(&mut self, path: &Path) {
        self.selected = vec![path.to_path_buf()];
        self.anchor = Some(path.to_path_buf());
        self.cursor = Some(path.to_path_buf());
    }

    /// Add or remove `path` and keep the rest: Ctrl-click, or Ctrl+Space.
    pub fn toggle(&mut self, path: &Path) {
        if let Some(at) = self.selected.iter().position(|selected| selected == path) {
            self.selected.remove(at);
        } else {
            self.selected.push(path.to_path_buf());
        }
        self.anchor = Some(path.to_path_buf());
        self.cursor = Some(path.to_path_buf());
    }

    /// Select the range from the anchor to `path`, replacing the selection:
    /// Shift-click, or Shift with an arrow. With no anchor it selects `path`.
    pub fn extend_to(&mut self, path: &Path, order: &[&Path]) {
        let to = order.iter().position(|candidate| *candidate == path);
        let from = self
            .anchor
            .as_deref()
            .and_then(|anchor| order.iter().position(|candidate| *candidate == anchor));
        match (from, to) {
            (Some(from), Some(to)) => {
                let (low, high) = (from.min(to), from.max(to));
                self.selected = order[low..=high].iter().map(|p| p.to_path_buf()).collect();
                self.cursor = Some(path.to_path_buf());
            }
            _ => self.select_only(path),
        }
    }

    /// Select everything in `order`: Ctrl+A.
    pub fn select_all(&mut self, order: &[&Path]) {
        self.selected = order.iter().map(|p| p.to_path_buf()).collect();
        if self.cursor.is_none() {
            self.cursor = order.first().map(|p| p.to_path_buf());
            self.anchor = self.cursor.clone();
        }
    }

    /// Drop everything that is not in `order`, after the list changed.
    pub fn retain_visible(&mut self, order: &[&Path]) {
        let visible = |path: &PathBuf| order.contains(&path.as_path());
        self.selected.retain(visible);
        if self.anchor.as_ref().is_some_and(|path| !visible(path)) {
            self.anchor = None;
        }
        if self.cursor.as_ref().is_some_and(|path| !visible(path)) {
            self.cursor = None;
        }
    }

    /// Move the cursor by `key` and return the row it lands on. With
    /// `extend` (Shift) the selection grows to it; otherwise it becomes just
    /// that row. With no cursor yet, Down and Home land on the first row and
    /// Up and End on the last.
    pub fn step(&mut self, key: Move, order: &[&Path], extend: bool) -> Option<usize> {
        let last = order.len().checked_sub(1)?;
        let from = self
            .cursor
            .as_deref()
            .and_then(|cursor| order.iter().position(|candidate| *candidate == cursor));
        let to = match (key, from) {
            (Move::Home, _) => 0,
            (Move::End, _) => last,
            (Move::Down, None) | (Move::PageDown(_), None) => 0,
            (Move::Up, None) | (Move::PageUp(_), None) => last,
            (Move::Down, Some(at)) => (at + 1).min(last),
            (Move::Up, Some(at)) => at.saturating_sub(1),
            (Move::PageDown(rows), Some(at)) => (at + rows.max(1)).min(last),
            (Move::PageUp(rows), Some(at)) => at.saturating_sub(rows.max(1)),
        };
        if extend {
            self.extend_to(order[to], order);
        } else {
            self.select_only(order[to]);
        }
        Some(to)
    }

    /// Move only the cursor, leaving the selection alone: Ctrl with an arrow.
    pub fn set_cursor(&mut self, path: &Path) {
        self.cursor = Some(path.to_path_buf());
    }
}

/// Collects typed letters into a prefix for jumping to a row by name. The
/// prefix starts again after a pause.
#[derive(Clone, Debug)]
pub struct TypeAhead {
    buffer: String,
    last: Option<Instant>,
}

/// How long a pause ends a word.
const PAUSE: Duration = Duration::from_millis(1000);

impl Default for TypeAhead {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeAhead {
    pub fn new() -> Self {
        TypeAhead {
            buffer: String::new(),
            last: None,
        }
    }

    /// Whether a word is being typed at `now`: a letter came less than a
    /// pause ago. Other keys, such as Space for Quick Look, leave a word
    /// that is being typed alone.
    pub fn is_active(&self, now: Instant) -> bool {
        !self.buffer.is_empty()
            && self
                .last
                .is_some_and(|last| now.duration_since(last) <= PAUSE)
    }

    /// Add a typed character at `now` and return the prefix so far.
    pub fn push(&mut self, character: char, now: Instant) -> &str {
        if self
            .last
            .is_none_or(|last| now.duration_since(last) > PAUSE)
        {
            self.buffer.clear();
        }
        self.last = Some(now);
        self.buffer.push(character);
        &self.buffer
    }
}

/// The row to jump to for `prefix`: the first name at or after `from` that
/// starts with it, ignoring case, wrapping round. A repeated single letter
/// cycles through names that start with it. `name_at` is the name of row `i`
/// of `count` rows, in list order.
pub fn type_ahead_target<'a>(
    count: usize,
    name_at: impl Fn(usize) -> &'a str,
    from: Option<usize>,
    prefix: &str,
) -> Option<usize> {
    if count == 0 || prefix.is_empty() {
        return None;
    }
    let lower = prefix.to_lowercase();
    let mut chars = lower.chars();
    let repeated = chars.clone().count() > 1 && chars.all(|c| lower.starts_with(c));
    let single: String = lower.chars().take(1).collect();
    let (needle, skip_current) = if repeated {
        (single, true)
    } else {
        (lower, false)
    };
    let start = from.map_or(0, |at| if skip_current { at + 1 } else { at });
    (0..count)
        .map(|offset| (start + offset) % count)
        .find(|at| name_at(*at).to_lowercase().starts_with(&needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn click_toggle_and_range() {
        let list = paths(&["a", "b", "c", "d", "e"]);
        let order: Vec<&Path> = list.iter().map(PathBuf::as_path).collect();
        let mut selection = Selection::new();
        selection.select_only(order[1]);
        selection.extend_to(order[3], &order);
        assert_eq!(selection.paths().len(), 3);
        selection.toggle(order[2]);
        assert_eq!(selection.paths().len(), 2);
        selection.select_only(order[4]);
        assert_eq!(selection.paths(), [PathBuf::from("e")]);
    }

    #[test]
    fn keys_move_and_extend() {
        let list = paths(&["a", "b", "c"]);
        let order: Vec<&Path> = list.iter().map(PathBuf::as_path).collect();
        let mut selection = Selection::new();
        assert_eq!(selection.step(Move::Down, &order, false), Some(0));
        assert_eq!(selection.step(Move::Down, &order, true), Some(1));
        assert_eq!(selection.step(Move::End, &order, true), Some(2));
        assert_eq!(selection.paths().len(), 3);
        assert_eq!(selection.step(Move::Up, &order, false), Some(1));
        assert_eq!(selection.paths(), [PathBuf::from("b")]);
        selection.retain_visible(&order[2..]);
        assert!(selection.is_empty());
    }

    #[test]
    fn type_ahead_wraps_and_cycles() {
        let names = ["Alpha", "beta", "Bravo", "charlie"];
        assert_eq!(
            type_ahead_target(names.len(), |i| names[i], None, "b"),
            Some(1)
        );
        assert_eq!(
            type_ahead_target(names.len(), |i| names[i], Some(1), "bb"),
            Some(2)
        );
        assert_eq!(
            type_ahead_target(names.len(), |i| names[i], Some(3), "al"),
            Some(0)
        );
        assert_eq!(
            type_ahead_target(names.len(), |i| names[i], None, "z"),
            None
        );
    }

    #[test]
    fn a_word_is_being_typed_until_the_pause() {
        let mut typed = TypeAhead::new();
        let start = Instant::now();
        assert!(!typed.is_active(start), "nothing typed yet");
        typed.push('a', start);
        assert!(typed.is_active(start + Duration::from_millis(400)));
        assert!(!typed.is_active(start + Duration::from_millis(1500)));
    }
}
