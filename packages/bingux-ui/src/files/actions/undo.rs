//! What Ctrl+Z can take back: a short stack of the last changes.
//!
//! The stack belongs to the process, not to a window: a change made in one
//! file view can be undone from another, which is what a person expects when
//! they move a file and then change folder.

use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

use super::backend::{Restored, Trashed};

/// How many changes are remembered. The oldest is forgotten past this.
const DEPTH: usize = 20;

/// One change and what is needed to reverse it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Step {
    /// `from` is now called `to`.
    Rename { from: PathBuf, to: PathBuf },
    /// These went to the Trash.
    Trash(Vec<Trashed>),
    /// These came out of the Trash, back to where they were or into a folder
    /// somebody chose. Each remembers what Replace sent to the Trash.
    Restore(Vec<Restored>),
    /// A new, empty folder or file.
    Create { path: PathBuf, folder: bool },
    /// Each item was `(original, now)`: moved out of its folder.
    Move(Vec<(PathBuf, PathBuf)>),
}

impl Step {
    /// The menu text that offers to reverse this step.
    pub(super) fn label(&self) -> &'static str {
        match self {
            Step::Rename { .. } => "Undo Rename",
            Step::Trash(_) => "Undo Move to Trash",
            Step::Restore(_) => "Undo Restore",
            Step::Create { folder: true, .. } => "Undo New Folder",
            Step::Create { folder: false, .. } => "Undo New File",
            Step::Move(_) => "Undo Move",
        }
    }
}

/// The remembered steps, newest last.
#[derive(Debug, Default)]
pub(super) struct UndoStack {
    steps: Vec<Step>,
}

impl UndoStack {
    pub(super) const fn new() -> Self {
        UndoStack { steps: Vec::new() }
    }

    pub(super) fn push(&mut self, step: Step) {
        if self.steps.len() == DEPTH {
            self.steps.remove(0);
        }
        self.steps.push(step);
    }

    pub(super) fn pop(&mut self) -> Option<Step> {
        self.steps.pop()
    }

    pub(super) fn peek(&self) -> Option<&Step> {
        self.steps.last()
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.steps.len()
    }
}

static STACK: Mutex<UndoStack> = Mutex::new(UndoStack::new());

fn stack() -> std::sync::MutexGuard<'static, UndoStack> {
    STACK.lock().unwrap_or_else(PoisonError::into_inner)
}

pub(super) fn push(step: Step) {
    stack().push(step);
}

pub(super) fn pop() -> Option<Step> {
    stack().pop()
}

/// The menu text for the newest step, if there is one.
pub(super) fn next_label() -> Option<&'static str> {
    stack().peek().map(Step::label)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rename(number: usize) -> Step {
        Step::Rename {
            from: PathBuf::from(format!("/a/{number}")),
            to: PathBuf::from(format!("/a/{number}-new")),
        }
    }

    #[test]
    fn last_in_first_out() {
        let mut stack = UndoStack::new();
        assert!(stack.pop().is_none());
        stack.push(rename(1));
        let created = Step::Create {
            path: PathBuf::from("/a/folder"),
            folder: true,
        };
        stack.push(created.clone());
        assert_eq!(stack.peek().map(Step::label), Some("Undo New Folder"));
        assert_eq!(stack.pop(), Some(created));
        assert_eq!(stack.pop(), Some(rename(1)));
        assert!(stack.pop().is_none());
    }

    #[test]
    fn keeps_twenty() {
        let mut stack = UndoStack::new();
        for number in 0..30 {
            stack.push(rename(number));
        }
        assert_eq!(stack.len(), 20);
        assert_eq!(stack.pop(), Some(rename(29)));
        let mut oldest = None;
        while let Some(step) = stack.pop() {
            oldest = Some(step);
        }
        assert_eq!(oldest, Some(rename(10)));
    }

    #[test]
    fn labels_name_the_change() {
        assert_eq!(rename(1).label(), "Undo Rename");
        assert_eq!(Step::Trash(Vec::new()).label(), "Undo Move to Trash");
        assert_eq!(Step::Restore(Vec::new()).label(), "Undo Restore");
        assert_eq!(Step::Move(Vec::new()).label(), "Undo Move");
    }
}
