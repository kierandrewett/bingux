//! What Ctrl+Z can take back: each change a file view makes is one step on
//! the window's [`UndoStack`](crate::undo::UndoStack), so Edit > Undo and
//! Redo, the palette and the toasts' Undo all reach it.
//!
//! A step's files move on a worker. While one runs the step is busy; once
//! it is undone it remembers what came back, which is what Redo acts on, and
//! a redo leaves a fresh step for the next Undo. An undo that fails leaves
//! the change as the newest one, to try again once the cause is gone.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use freya::prelude::*;

use super::backend::{Restored, Trashed};
use super::transfer::{error_text, redo_step, reverse, select_in};
use super::{Env, Selection, Ui, notify};
use crate::task::spawn_blocking;
use crate::undo::{UndoStack, UndoStep};

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
    /// The change's name, as Edit > Undo shows it: "Undo rename".
    pub(super) fn name(&self) -> &'static str {
        match self {
            Step::Rename { .. } => "Rename",
            Step::Trash(_) => "Move to Trash",
            Step::Restore(_) => "Restore",
            Step::Create { folder: true, .. } => "New folder",
            Step::Create { folder: false, .. } => "New file",
            Step::Move(_) => "Move",
        }
    }

    /// A restore cannot be done again: the item it took out of the Trash
    /// has a new place in the Trash after the undo.
    fn redoable(&self) -> bool {
        !matches!(self, Step::Restore(_))
    }
}

/// Where a step is.
enum Phase {
    /// Done: Undo applies.
    Done(Step),
    /// Undone, with what came back: Redo applies.
    Undone(Step, Vec<PathBuf>),
    /// On a worker.
    Busy,
}

type Shared = Rc<RefCell<Phase>>;

/// What a step needs of the view after it is pushed: no listing, which a
/// step kept for a long time would hold in memory.
#[derive(Clone)]
struct Undoing {
    ui: Ui,
    selection: State<Selection>,
    reload: EventHandler<()>,
    undo: UndoStack,
}

/// Remember `step` on the window's undo stack.
pub(super) fn push(env: &Env, step: Step) {
    let with = Undoing {
        ui: env.ui,
        selection: env.selection,
        reload: env.reload.clone(),
        undo: env.undo,
    };
    let undo = with.undo;
    undo.push(undo_step(with, Rc::new(RefCell::new(Phase::Done(step)))));
}

/// The stack's step for the change in `phase`.
fn undo_step(with: Undoing, phase: Shared) -> UndoStep {
    let (name, redoable) = match &*phase.borrow() {
        Phase::Done(step) | Phase::Undone(step, _) => (step.name(), step.redoable()),
        Phase::Busy => ("Change", false),
    };
    let undo = {
        let (with, phase) = (with.clone(), phase.clone());
        move || start_undo(&with, &phase)
    };
    if redoable {
        UndoStep::new(name, undo, move || start_redo(&with, &phase))
    } else {
        UndoStep::undo_only(name, undo)
    }
}

fn start_undo(with: &Undoing, phase: &Shared) {
    let step = match std::mem::replace(&mut *phase.borrow_mut(), Phase::Busy) {
        Phase::Done(step) => step,
        // Undone already (a redo that failed), or still on a worker.
        other => {
            *phase.borrow_mut() = other;
            return;
        }
    };
    let (with, phase) = (with.clone(), phase.clone());
    spawn_in_window(async move {
        let reversing = step.clone();
        match spawn_blocking(move || reverse(&reversing)).await {
            Ok(back) => {
                notify(with.ui, "Undone.", false);
                select_in(with.selection, &back);
                *phase.borrow_mut() = Phase::Undone(step, back);
            }
            Err(error) => {
                // Still the newest change: try again once the cause is gone.
                *phase.borrow_mut() = Phase::Done(step);
                with.undo.push(undo_step(with.clone(), phase.clone()));
                notify(with.ui, error_text("undo that", &error), true);
            }
        }
        with.reload.call(());
    });
}

fn start_redo(with: &Undoing, phase: &Shared) {
    let (step, back) = match std::mem::replace(&mut *phase.borrow_mut(), Phase::Busy) {
        Phase::Undone(step, back) => (step, back),
        other => {
            *phase.borrow_mut() = other;
            return;
        }
    };
    let (with, phase) = (with.clone(), phase.clone());
    spawn_in_window(async move {
        let (again, came_back) = (step.clone(), back.clone());
        match spawn_blocking(move || redo_step(&again, &came_back)).await {
            Ok((now, shown)) => {
                notify(with.ui, "Redone.", false);
                select_in(with.selection, &shown);
                *phase.borrow_mut() = Phase::Done(now);
            }
            Err(error) => {
                *phase.borrow_mut() = Phase::Undone(step, back);
                notify(with.ui, error_text("redo that", &error), true);
            }
        }
        with.reload.call(());
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    // Failure mode: Edit > Undo showing a title that does not say what it
    // takes back, or a restore offered for Redo when it cannot be redone.

    #[test]
    fn names_say_the_change() {
        let rename = Step::Rename {
            from: PathBuf::from("/a/1"),
            to: PathBuf::from("/a/2"),
        };
        assert_eq!(rename.name(), "Rename");
        assert_eq!(Step::Trash(Vec::new()).name(), "Move to Trash");
        assert_eq!(Step::Restore(Vec::new()).name(), "Restore");
        assert_eq!(Step::Move(Vec::new()).name(), "Move");
        assert!(!Step::Restore(Vec::new()).redoable());
        assert!(rename.redoable());
    }
}
