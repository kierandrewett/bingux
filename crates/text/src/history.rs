//! Undo and redo. Each edit is a transaction: the changes it made, in the
//! order they were applied, and the selection before and after. Typing is
//! grouped into bursts: letters typed one after another, without a pause of
//! [`BURST_GAP`] or a move of the caret in between, undo together, as in
//! TextEdit. A new line, a paste, a cut or any command starts a new step.

use std::time::{Duration, Instant};

use crate::selection::Selection;

/// A pause longer than this ends a typing burst.
pub const BURST_GAP: Duration = Duration::from_millis(1200);

/// How many undo steps are kept. Older steps are forgotten first.
pub const UNDO_LIMIT: usize = 2000;

/// One replacement: `removed` at char `at` became `inserted`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub at: usize,
    pub removed: String,
    pub inserted: String,
}

impl Change {
    pub fn removed_chars(&self) -> usize {
        self.removed.chars().count()
    }

    pub fn inserted_chars(&self) -> usize {
        self.inserted.chars().count()
    }
}

/// What kind of edit a transaction is, for grouping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKind {
    /// Letters typed: merge into the burst before.
    Typing,
    /// Backspace or Delete one character at a time: merges too.
    Deleting,
    /// Anything else: always its own step.
    Other,
}

#[derive(Clone, Debug)]
pub struct Transaction {
    pub changes: Vec<Change>,
    pub before: Selection,
    pub after: Selection,
    pub kind: EditKind,
    /// What the step did, as a menu names it ("duplicate line", for "Undo
    /// duplicate line"). `None` falls back to the kind.
    pub label: Option<&'static str>,
    at: Instant,
    /// A number that is never reused, so the saved state can be recognised.
    id: u64,
}

impl Transaction {
    /// The first line any change of this transaction touched, as a char
    /// index; the caller turns it into a line for invalidation.
    pub fn first_change(&self) -> Option<usize> {
        self.changes.iter().map(|change| change.at).min()
    }

    /// How the step is named: its label, else what its kind says.
    fn name(&self) -> Option<&'static str> {
        self.label.or(match self.kind {
            EditKind::Typing => Some("typing"),
            EditKind::Deleting => Some("delete"),
            EditKind::Other => None,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct History {
    undo: Vec<Transaction>,
    redo: Vec<Transaction>,
    next_id: u64,
    /// The id of the transaction on top when the file was saved or opened;
    /// 0 is "no transaction".
    saved: u64,
    /// Set by a caret move: the next edit starts a new step.
    sealed: bool,
}

impl History {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            ..Self::default()
        }
    }

    /// Records an edit, merging it into the step before when both are part
    /// of one typing burst.
    pub fn record(
        &mut self,
        changes: Vec<Change>,
        before: Selection,
        after: Selection,
        kind: EditKind,
    ) {
        if changes.is_empty() {
            return;
        }
        self.redo.clear();
        let now = Instant::now();
        let merge = !self.sealed
            && kind != EditKind::Other
            && self.undo.last().is_some_and(|last| {
                last.kind == kind
                    && last.id != self.saved
                    && now.duration_since(last.at) < BURST_GAP
                    && last.after == before
                    && !breaks_burst(&changes, kind)
            });
        self.sealed = false;
        if merge {
            let last = self.undo.last_mut().expect("checked above");
            last.changes.extend(changes);
            last.after = after;
            last.at = now;
            return;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.undo.push(Transaction {
            changes,
            before,
            after,
            kind,
            label: None,
            at: now,
            id,
        });
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
    }

    /// The next edit starts a new step, even if it is more typing: the caret
    /// moved, or the window lost focus.
    pub fn seal(&mut self) {
        self.sealed = true;
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Takes the newest step for undoing. The caller reverts its changes and
    /// hands it back with [`History::undone`].
    pub fn pop_undo(&mut self) -> Option<Transaction> {
        self.sealed = true;
        self.undo.pop()
    }

    pub fn undone(&mut self, transaction: Transaction) {
        self.redo.push(transaction);
    }

    pub fn pop_redo(&mut self) -> Option<Transaction> {
        self.sealed = true;
        self.redo.pop()
    }

    pub fn redone(&mut self, transaction: Transaction) {
        self.undo.push(transaction);
    }

    /// Names the newest step, unless a command already did.
    pub fn label_last(&mut self, label: &'static str) {
        if let Some(last) = self.undo.last_mut() {
            last.label.get_or_insert(label);
        }
    }

    /// A value that changes when a step is recorded or merged into: what
    /// `Editor::label_edit` compares to see that a command edited.
    pub fn stamp(&self) -> (usize, u64, usize) {
        (
            self.undo.len(),
            self.top(),
            self.undo.last().map_or(0, |last| last.changes.len()),
        )
    }

    /// What undoing the newest step takes back, as a noun or gerund:
    /// "typing", "duplicate line". `None` for an edit with no name.
    pub fn undo_label(&self) -> Option<&'static str> {
        self.undo.last().and_then(Transaction::name)
    }

    /// What redoing the newest undone step does again.
    pub fn redo_label(&self) -> Option<&'static str> {
        self.redo.last().and_then(Transaction::name)
    }

    /// The document as it is now is the one on disk.
    pub fn mark_saved(&mut self) {
        self.saved = self.top();
        self.sealed = true;
    }

    /// The document differs from the one on disk.
    pub fn is_modified(&self) -> bool {
        self.top() != self.saved
    }

    /// Forget every step: a reload.
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.saved = 0;
        self.sealed = true;
    }

    /// The document no longer matches any saved state (it was restored from
    /// a draft, say), so it counts as modified until saved.
    pub fn mark_unsaved(&mut self) {
        self.saved = u64::MAX;
    }

    fn top(&self) -> u64 {
        self.undo.last().map_or(0, |last| last.id)
    }
}

/// Typing a new line, or a space after a word, is where a burst ends, so an
/// undo takes back a line or a word at a time on a long run of typing.
fn breaks_burst(changes: &[Change], kind: EditKind) -> bool {
    kind == EditKind::Typing && changes.iter().any(|change| change.inserted.contains('\n'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(at: usize, text: &str) -> Vec<Change> {
        vec![Change {
            at,
            removed: String::new(),
            inserted: text.into(),
        }]
    }

    #[test]
    fn typing_groups_into_one_step() {
        let mut history = History::new();
        history.record(
            typed(0, "a"),
            Selection::caret(0),
            Selection::caret(1),
            EditKind::Typing,
        );
        history.record(
            typed(1, "b"),
            Selection::caret(1),
            Selection::caret(2),
            EditKind::Typing,
        );
        assert!(
            history
                .pop_undo()
                .is_some_and(|step| step.changes.len() == 2)
        );
        assert!(!history.can_undo());
    }

    #[test]
    fn a_caret_move_ends_the_burst() {
        let mut history = History::new();
        history.record(
            typed(0, "a"),
            Selection::caret(0),
            Selection::caret(1),
            EditKind::Typing,
        );
        history.seal();
        history.record(
            typed(1, "b"),
            Selection::caret(1),
            Selection::caret(2),
            EditKind::Typing,
        );
        assert_eq!(history.undo.len(), 2);
    }

    #[test]
    fn saving_is_remembered_through_undo() {
        let mut history = History::new();
        assert!(!history.is_modified());
        history.record(
            typed(0, "a"),
            Selection::caret(0),
            Selection::caret(1),
            EditKind::Other,
        );
        assert!(history.is_modified());
        history.mark_saved();
        assert!(!history.is_modified());
        // Typing after a save never merges into the saved step.
        history.record(
            typed(1, "b"),
            Selection::caret(1),
            Selection::caret(2),
            EditKind::Typing,
        );
        assert!(history.is_modified());
        let step = history.pop_undo().unwrap();
        history.undone(step);
        assert!(!history.is_modified());
    }
}
