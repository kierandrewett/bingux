//! The operations that take time: copy, move and undo. Each runs on the worker pool, shows a busy line with a
//! cancel button while it runs, and says how it ended in the status line.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use bingux_files::Cancel;
use freya::prelude::*;

use super::backend::{self, Conflict, ConflictPolicy, OpError, Progress, Resolution};
use super::undo::{self, Step};
use super::{Busy, Env, clipboard, describe, notify, set};
use crate::controls::checkbox;
use crate::shell::{Overlays, ResponseKind, dialog};
use crate::task::spawn_blocking;

/// How often the busy line and the conflict question are looked at.
const TICK: Duration = Duration::from_millis(100);

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Claim the one busy slot. Two operations at once would fight over the
/// status line and over each other's files.
pub(super) fn begin(env: &Env, label: String, cancel: Option<Cancel>) -> bool {
    if env.ui.busy.peek().is_some() {
        notify(env.ui, "Wait for the current operation to finish.", true);
        return false;
    }
    set(
        env.ui.busy,
        Some(Busy {
            label,
            fraction: None,
            cancel,
        }),
    );
    true
}

pub(super) fn finish(env: &Env) {
    set(env.ui.busy, None);
}

/// Select `paths` once they are listed: what a person expects after a paste.
pub(super) fn select_paths(env: &Env, paths: &[PathBuf]) {
    let mut selection = env.selection;
    let mut selection = selection.write();
    if let Some((first, rest)) = paths.split_first() {
        selection.select_only(first);
        for path in rest {
            selection.toggle(path);
        }
    }
}

pub(super) fn error_text(what: &str, error: &OpError) -> String {
    format!("Could not {what}: {error}")
}

// ---------------------------------------------------------------------------
// Copy and move
// ---------------------------------------------------------------------------

/// Paste the held files into `dest`.
pub(super) fn paste(env: &Env, dest: PathBuf) {
    let Some(held) = clipboard::take() else {
        notify(env.ui, "There is nothing to paste.", false);
        return;
    };
    transfer(env, held.paths, dest, held.cut, held.cut);
}

/// Drag and drop.
pub(super) fn move_into(env: &Env, paths: Vec<PathBuf>, dest: PathBuf, copy: bool) {
    transfer(env, paths, dest, !copy, false);
}

/// The question a copy or move asks when a name is taken.
#[derive(Default)]
pub(super) struct Asker {
    pending: Mutex<Option<Ask>>,
    /// An answer given for every later conflict.
    all: Mutex<Option<Resolution>>,
}

struct Ask {
    conflict: Conflict,
    reply: mpsc::Sender<Resolution>,
    /// Set when the person ticks "Do this for all conflicts".
    remember: Arc<AtomicBool>,
}

impl Asker {
    /// Called on the worker: leave the question for the UI and wait for the
    /// answer. Closing the dialog, or cancelling the operation, answers Cancel.
    pub(super) fn decide(&self, conflict: &Conflict, cancel: &Cancel) -> Resolution {
        if let Some(answer) = *lock(&self.all) {
            return answer;
        }
        let (reply, answer) = mpsc::channel();
        let remember = Arc::new(AtomicBool::new(false));
        *lock(&self.pending) = Some(Ask {
            conflict: conflict.clone(),
            reply,
            remember: remember.clone(),
        });
        loop {
            match answer.recv_timeout(TICK) {
                Ok(resolution) => {
                    if remember.load(Ordering::Relaxed) && resolution != Resolution::Cancel {
                        *lock(&self.all) = Some(resolution);
                    }
                    return resolution;
                }
                Err(RecvTimeoutError::Timeout) if cancel.is_cancelled() => {
                    return Resolution::Cancel;
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return Resolution::Cancel,
            }
        }
    }
}

/// The tick box of the conflict dialog.
struct ApplyToAll(Arc<AtomicBool>);

impl PartialEq for ApplyToAll {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Component for ApplyToAll {
    fn render(&self) -> impl IntoElement {
        let flag = self.0.clone();
        let mut on = use_state(|| false);
        checkbox(on(), move |next: bool| {
            flag.store(next, Ordering::Relaxed);
            on.set(next);
        })
        .text("Do this for all conflicts")
    }
}

/// Put a conflict question on screen. Each answer, and nothing else, goes
/// back to the worker; a dialog closed without one drops the senders, which
/// the worker reads as Cancel.
fn ask_about(overlays: Overlays, ask: Ask) {
    let Ask {
        conflict,
        reply,
        remember,
    } = ask;
    let name = conflict
        .existing
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    let place = conflict
        .existing
        .parent()
        .and_then(Path::file_name)
        .map_or_else(|| "the destination".to_owned(), |name| {
            format!("“{}”", name.to_string_lossy())
        });
    let answer = |resolution: Resolution| {
        let reply = reply.clone();
        move |_: ()| {
            let _ = reply.send(resolution);
        }
    };
    overlays.show_dialog(
        dialog(
            format!("“{name}” already exists"),
            format!("Something with this name is already in {place}."),
        )
        .content(Element::from(ApplyToAll(remember)))
        .cancel("Cancel")
        .response("Skip", ResponseKind::Default, answer(Resolution::Skip))
        .response("Keep both", ResponseKind::Default, answer(Resolution::KeepBoth))
        .response("Replace", ResponseKind::Destructive, answer(Resolution::Replace))
        .default_action("keep_both"),
    );
}

/// What the busy line says: "Copying 3 of 12", or the name for one item, and
/// once all the data is copied and the drive is being written out, that.
fn busy_text(
    verb: &str,
    whole: &str,
    scanning: bool,
    total: u64,
    done: u64,
    finishing: Option<&str>,
) -> String {
    if let Some(drive) = finishing {
        // "Done" only once the drive has the data: a stick pulled out before
        // this is over loses files.
        return format!("Finishing… writing to {drive}");
    }
    if scanning || total <= 1 {
        format!("{verb} {whole}")
    } else {
        format!("{verb} {} of {total}", (done + 1).min(total))
    }
}

fn busy_label(verb: &str, progress: &Progress, whole: &str) -> String {
    busy_text(
        verb,
        whole,
        progress.is_scanning(),
        progress.files_total(),
        progress.files_done(),
        progress.finishing().as_deref(),
    )
}

/// How an operation ends in the status line. A drive that was written out
/// for good (removable media, a share) says so.
fn done_text(past: &str, whole: &str, flushed_to: Option<&str>) -> String {
    match flushed_to {
        Some(drive) => format!("{past} {whole}. Everything is written to {drive}."),
        None => format!("{past} {whole}."),
    }
}

/// Keep the busy line and the conflict question current until `done`.
pub(super) async fn follow(
    env: Env,
    verb: &'static str,
    whole: String,
    progress: Progress,
    cancel: Cancel,
    asker: Arc<Asker>,
    done: Arc<AtomicBool>,
) {
    while !done.load(Ordering::Acquire) {
        if let Some(ask) = lock(&asker.pending).take() {
            ask_about(env.overlays, ask);
        }
        let finishing = progress.finishing().is_some();
        set(
            env.ui.busy,
            Some(Busy {
                label: busy_label(verb, &progress, &whole),
                // The drive says nothing about how far it is: a bar that
                // keeps still says "wait".
                fraction: if finishing { None } else { progress.fraction() },
                // Stopping half way through writing the drive out helps nobody.
                cancel: (!finishing).then(|| cancel.clone()),
            }),
        );
        timer(TICK).await;
    }
}

fn transfer(env: &Env, paths: Vec<PathBuf>, dest: PathBuf, cut: bool, clear_held: bool) {
    if paths.is_empty() {
        return;
    }
    // The Trash is not a folder to copy into: a drop on it trashes, a drop out
    // of it restores, and nothing goes inside it.
    if super::bin::redirect(env, &paths, &dest, !cut) {
        return;
    }
    if paths
        .iter()
        .any(|path| path.is_dir() && dest.starts_with(path))
    {
        notify(env.ui, "A folder cannot go inside itself.", true);
        return;
    }
    let (verb, past) = if cut {
        ("Moving", "Moved")
    } else {
        ("Copying", "Copied")
    };
    let whole = describe(&paths);
    let cancel = Cancel::new();
    if !begin(env, format!("{verb} {whole}"), Some(cancel.clone())) {
        return;
    }
    let progress = Progress::new();
    let report = progress.clone();
    let asker = Arc::new(Asker::default());
    let done = Arc::new(AtomicBool::new(false));
    spawn_in_window(follow(
        env.clone(),
        verb,
        whole.clone(),
        progress.clone(),
        cancel.clone(),
        asker.clone(),
        done.clone(),
    ));
    let env = env.clone();
    spawn_in_window(async move {
        let sources = paths.clone();
        let target = dest.clone();
        let outcome = spawn_blocking(move || {
            let policy = {
                let cancel = cancel.clone();
                ConflictPolicy::Ask(Box::new(move |conflict: &Conflict| {
                    asker.decide(conflict, &cancel)
                }))
            };
            if cut {
                backend::move_to(&sources, &target, &policy, &progress, &cancel)
            } else {
                backend::copy(&sources, &target, &policy, &progress, &cancel)
            }
        })
        .await;
        done.store(true, Ordering::Release);
        finish(&env);
        match outcome {
            Ok(made) => {
                if cut {
                    if clear_held {
                        clipboard::clear();
                    }
                    let moved = pair_moves(&paths, &made);
                    if !moved.is_empty() {
                        undo::push(Step::Move(moved));
                    }
                }
                notify(
                    env.ui,
                    done_text(past, &whole, report.flushed_to().as_deref()),
                    false,
                );
                select_paths(&env, &made);
            }
            Err(OpError::Cancelled) => notify(env.ui, format!("{verb} stopped."), false),
            Err(error) => notify(
                env.ui,
                error_text(if cut { "move" } else { "copy" }, &error),
                true,
            ),
        }
        env.reload.call(());
    });
}

/// Pair what a move made with what it moved: `(original, now)` for each item
/// that arrived. A skipped item has no destination, and a kept-both item
/// arrives under another name, so a name match comes first and the rest pair
/// in order.
pub(super) fn pair_moves(originals: &[PathBuf], arrived: &[PathBuf]) -> Vec<(PathBuf, PathBuf)> {
    let mut left: Vec<&PathBuf> = originals.iter().collect();
    let mut pairs = Vec::new();
    for now in arrived {
        let by_name = left
            .iter()
            .position(|original| original.file_name() == now.file_name());
        let at = match by_name {
            Some(at) => at,
            None if left.is_empty() => break,
            None => 0,
        };
        let original = left.remove(at);
        // A move that went nowhere is not a change to undo.
        if original != now {
            pairs.push((original.clone(), now.clone()));
        }
    }
    pairs
}

// ---------------------------------------------------------------------------
// Undo
// ---------------------------------------------------------------------------

/// Take back the newest change.
pub(super) fn undo(env: &Env) {
    let Some(step) = undo::pop() else {
        notify(env.ui, "There is nothing to undo.", false);
        return;
    };
    let env = env.clone();
    spawn_in_window(async move {
        let again = step.clone();
        let outcome = spawn_blocking(move || reverse(&step)).await;
        match outcome {
            Ok(restored) => {
                notify(env.ui, "Undone.", false);
                select_paths(&env, &restored);
            }
            Err(error) => {
                // Still the newest change: try again once the cause is gone.
                undo::push(again);
                notify(env.ui, error_text("undo that", &error), true);
            }
        }
        env.reload.call(());
    });
}

/// Reverse one step. Blocking. Returns what is back in place.
fn reverse(step: &Step) -> Result<Vec<PathBuf>, OpError> {
    match step {
        Step::Rename { from, to } => {
            let name = from
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .ok_or(OpError::Invalid(bingux_files::location::NameError::Empty))?;
            backend::rename(to, &name).map(|path| vec![path])
        }
        Step::Trash(items) => backend::untrash(items),
        Step::Restore(items) => super::bin::undo_restore(items),
        Step::Create { path, folder } => {
            // An item nobody has touched since is simply removed. One that
            // gained content goes to the Trash instead of being lost.
            let removed = if *folder {
                fs::remove_dir(path)
            } else if fs::metadata(path).is_ok_and(|meta| meta.len() == 0) {
                fs::remove_file(path)
            } else {
                Err(std::io::Error::from(std::io::ErrorKind::DirectoryNotEmpty))
            };
            match removed {
                Ok(()) => Ok(Vec::new()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
                Err(_) => backend::trash(std::slice::from_ref(path)).map(|_| Vec::new()),
            }
        }
        Step::Move(pairs) => {
            let mut back = Vec::new();
            for (original, now) in pairs {
                let Some(folder) = original.parent() else {
                    continue;
                };
                let moved = backend::move_to(
                    std::slice::from_ref(now),
                    folder,
                    &ConflictPolicy::KeepBoth,
                    &Progress::new(),
                    &Cancel::new(),
                )?;
                for path in moved {
                    // It may have come back under another name.
                    if path.file_name() != original.file_name()
                        && let Some(name) = original.file_name()
                    {
                        back.push(backend::rename(&path, &name.to_string_lossy())?);
                    } else {
                        back.push(path);
                    }
                }
            }
            Ok(back)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn moves_pair_by_name() {
        let pairs = pair_moves(&paths(&["/a/x", "/a/y"]), &paths(&["/b/x", "/b/y"]));
        assert_eq!(
            pairs,
            [
                (PathBuf::from("/a/x"), PathBuf::from("/b/x")),
                (PathBuf::from("/a/y"), PathBuf::from("/b/y")),
            ]
        );
    }

    #[test]
    fn a_skipped_item_leaves_no_pair() {
        let pairs = pair_moves(&paths(&["/a/x", "/a/y"]), &paths(&["/b/y"]));
        assert_eq!(pairs, [(PathBuf::from("/a/y"), PathBuf::from("/b/y"))]);
    }

    #[test]
    fn a_renamed_arrival_pairs_in_order() {
        let pairs = pair_moves(&paths(&["/a/x"]), &paths(&["/b/x (copy)"]));
        assert_eq!(pairs, [(PathBuf::from("/a/x"), PathBuf::from("/b/x (copy)"))]);
    }

    #[test]
    fn a_move_to_the_same_place_is_no_change() {
        assert!(pair_moves(&paths(&["/a/x"]), &paths(&["/a/x"])).is_empty());
    }

    #[test]
    fn the_busy_line_says_finishing_while_the_drive_is_written() {
        assert_eq!(busy_text("Copying", "12 items", false, 12, 3, None), "Copying 4 of 12");
        assert_eq!(busy_text("Copying", "“a”", false, 1, 0, None), "Copying “a”");
        assert_eq!(busy_text("Copying", "12 items", true, 5, 2, None), "Copying 12 items", "still counting");
        assert_eq!(
            busy_text("Copying", "12 items", false, 12, 12, Some("My Disk")),
            "Finishing… writing to My Disk"
        );
    }

    #[test]
    fn done_means_written_on_a_drive_that_was_flushed() {
        assert_eq!(done_text("Copied", "“a”", None), "Copied “a”.");
        assert_eq!(
            done_text("Copied", "“a”", Some("My Disk")),
            "Copied “a”. Everything is written to My Disk."
        );
    }
}
