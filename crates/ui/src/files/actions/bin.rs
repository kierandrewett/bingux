//! The Trash in the file views: moving things to it, taking them out, deleting
//! for good, and emptying it.
//!
//! Every question and every message here is specific. An item the Trash
//! cannot take is never deleted quietly and never ignored: the person is told
//! why ([`TrashRefusal`]), and where deleting for good is a fair thing to offer
//! it is offered behind the usual question, with the focus on Cancel. The
//! folders the desktop stands on (home, Documents, the Trash itself) are
//! refused with the reason and no offer.
//!
//! Inside the Trash a delete is always permanent and always asks, with no Shift
//! needed: it is the only delete there is. Taking something out is Restore,
//! which asks only what needs asking: to make a folder that is gone, and what
//! to do about a name that is taken. Both have an Undo toast.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use bingux_files::Cancel;
use bingux_files::display;
use bingux_files::ops::InBin;
use freya::prelude::*;

use super::backend::{
    self, ConflictPolicy, OpError, Progress, RestoreFailure, RestorePlan, Restored, TrashRefusal,
    TrashSummary,
};
use super::transfer::{Asker, begin, error_text, finish, follow, select_paths};
use super::undo::{self, Step};
use super::{Env, describe, notify};
use crate::files::{ChooserRequest, choose};
use crate::shell::{CANCEL_ID, DialogIcon, ResponseKind, dialog};
use crate::task::spawn_blocking;

// ---------------------------------------------------------------------------
// Words
// ---------------------------------------------------------------------------

/// What `paths` are called in a sentence: the name in quotes, or the count.
fn what(paths: &[PathBuf]) -> String {
    describe(paths)
}

/// The toast after a move to the Trash: `Moved “a.txt” to Trash`, `Moved 3
/// items to Trash`.
pub(super) fn moved_text(paths: &[PathBuf]) -> String {
    format!("Moved {} to Trash", what(paths))
}

/// The toast after a restore: `Restored “a.txt”`, `Restored 3 items`.
pub(super) fn restored_text(paths: &[PathBuf]) -> String {
    format!("Restored {}", what(paths))
}

/// `1 item`, `12 items`.
fn items_text(count: u64) -> String {
    format!("{count} {}", if count == 1 { "item" } else { "items" })
}

/// The words of a question before deleting for good.
#[derive(Debug, PartialEq)]
pub(super) struct Prompt {
    pub title: String,
    pub body: String,
}

/// The question before deleting `paths` for good. `reason` is why the Trash
/// was not used; `in_trash` says they are in the Trash already, where there
/// is no Trash to avoid.
pub(super) fn delete_prompt(paths: &[PathBuf], reason: Option<&str>, in_trash: bool) -> Prompt {
    let (it, they, them) = if paths.len() == 1 {
        ("It", "it", "it")
    } else {
        ("They", "they", "them")
    };
    let mut body = String::new();
    if let Some(reason) = reason {
        body.push_str(reason);
        body.push(' ');
    }
    if in_trash {
        body.push_str(&format!(
            "{it} will be deleted permanently. This cannot be undone: nothing can bring {them} back."
        ));
    } else {
        body.push_str(&format!(
            "{it} will be deleted for good. This cannot be undone: {they} will not go to the Trash, and Undo cannot bring {them} back."
        ));
    }
    Prompt {
        title: format!("Delete {} permanently?", what(paths)),
        body,
    }
}

/// The question before the Trash is emptied: how many items and how much
/// space, so the person knows what is going.
pub(super) fn empty_prompt(summary: TrashSummary) -> Prompt {
    let size = if summary.bytes > 0 {
        format!(" ({})", display::size(summary.bytes))
    } else {
        String::new()
    };
    let (them, are) = if summary.items == 1 { ("It", "will") } else { ("They", "will") };
    Prompt {
        title: "Empty the Trash?".to_owned(),
        body: format!(
            "{}{size} in the Trash {are} be deleted for good. This cannot be undone. {them} will not be recoverable.",
            items_text(summary.items)
        ),
    }
}

/// What to tell a person about items the Trash refused, or the question to
/// ask. `delete` are the ones Delete permanently may be offered for, with the
/// reason; `refused` the ones it may not, with the sentence to say.
#[derive(Debug, PartialEq)]
struct Refusals {
    delete: Vec<PathBuf>,
    delete_reason: Option<String>,
    refused: Vec<PathBuf>,
    refused_text: Option<String>,
}

/// Sort the refused items into those Delete permanently is a fair offer for
/// and those it is not.
fn sort_refusals(refused: Vec<(PathBuf, TrashRefusal)>) -> Refusals {
    let mut out = Refusals {
        delete: Vec::new(),
        delete_reason: None,
        refused: Vec::new(),
        refused_text: None,
    };
    let mut delete_reasons: Vec<TrashRefusal> = Vec::new();
    let mut refused_reasons: Vec<TrashRefusal> = Vec::new();
    for (path, refusal) in refused {
        if refusal.offers_delete() {
            out.delete.push(path);
            if !delete_reasons.contains(&refusal) {
                delete_reasons.push(refusal);
            }
        } else {
            out.refused.push(path);
            if !refused_reasons.contains(&refusal) {
                refused_reasons.push(refusal);
            }
        }
    }
    // One reason says itself; several different ones say only that they
    // cannot go (the dialog then names no single cause).
    let plural_delete = out.delete.len() > 1;
    out.delete_reason = match delete_reasons.as_slice() {
        [] => None,
        [one] => Some(one.reason(plural_delete)),
        _ => Some(
            if plural_delete {
                "They cannot be moved to the Trash."
            } else {
                "It cannot be moved to the Trash."
            }
            .to_owned(),
        ),
    };
    let plural_refused = out.refused.len() > 1;
    out.refused_text = match refused_reasons.as_slice() {
        [] => None,
        [one] => Some(format!("{}: {}", what(&out.refused), one.reason(plural_refused))),
        _ => Some(format!("{} cannot be moved to the Trash.", what(&out.refused))),
    };
    out
}

/// Which of `paths` the Trash can take, and why it refuses the rest.
#[derive(Debug, PartialEq)]
struct TrashPlan {
    bin: Vec<PathBuf>,
    refused: Vec<(PathBuf, TrashRefusal)>,
}

fn plan_trash(
    paths: Vec<PathBuf>,
    check: impl Fn(&Path) -> Result<(), TrashRefusal>,
) -> TrashPlan {
    let mut plan = TrashPlan {
        bin: Vec::new(),
        refused: Vec::new(),
    };
    for path in paths {
        match check(&path) {
            Ok(()) => plan.bin.push(path),
            Err(refusal) => plan.refused.push((path, refusal)),
        }
    }
    plan
}

// ---------------------------------------------------------------------------
// Move to the Trash
// ---------------------------------------------------------------------------

/// Move to the Trash. This is the ordinary way to remove something, and it
/// can be undone. An item the Trash cannot take is told about, with the reason,
/// and where deleting it for good is a fair offer the person is asked: never
/// quietly deleted, never quietly skipped.
pub(super) fn trash(env: &Env, paths: Vec<PathBuf>) {
    if paths.is_empty() {
        return;
    }
    let plan = plan_trash(paths, backend::check_trash);
    let refusals = sort_refusals(plan.refused);
    if plan.bin.is_empty() {
        report_refusals(env, refusals);
    } else {
        move_to_trash(env, plan.bin, refusals);
    }
}

/// Tell about what the Trash refused: a message for what may not be deleted
/// instead, then the question for what may.
fn report_refusals(env: &Env, refusals: Refusals) {
    if let Some(text) = &refusals.refused_text {
        notify(env.ui, text.clone(), true);
    }
    if !refusals.delete.is_empty() {
        ask_delete(env, refusals.delete, refusals.delete_reason.as_deref());
    }
}

/// Bin `paths`, then tell about the items the Trash refused.
fn move_to_trash(env: &Env, paths: Vec<PathBuf>, then: Refusals) {
    if !begin(env, format!("Moving {} to Trash", what(&paths)), None) {
        return;
    }
    let env = env.clone();
    spawn_in_window(async move {
        let attempted = paths.clone();
        let outcome = spawn_blocking(move || backend::trash(&paths)).await;
        finish(&env);
        match outcome {
            Ok(trashed) => {
                let moved: Vec<PathBuf> = trashed.iter().map(|item| item.original.clone()).collect();
                undo::push(Step::Trash(trashed));
                let again = env.clone();
                env.overlays.toast_with_action(moved_text(&moved), "Undo", move |_: ()| {
                    super::transfer::undo(&again)
                });
                report_refusals(&env, then);
            }
            Err(error) => trash_failed(&env, attempted, error),
        }
        env.reload.call(());
    });
}

/// The Trash could not take `paths` after all (the check is advice; the move
/// is the truth). Nothing was moved.
fn trash_failed(env: &Env, paths: Vec<PathBuf>, error: OpError) {
    let it = if paths.len() == 1 { "it" } else { "them" };
    match error {
        OpError::CannotTrash(refusal) => {
            let refusals = sort_refusals(paths.into_iter().map(|path| (path, refusal.clone())).collect());
            report_refusals(env, refusals);
        }
        // Nothing the person can fix and nothing to offer: it needs a copy
        // between drives, which is a decision.
        OpError::CopyNeeded => ask_copy(env, paths),
        // The folder it is in cannot be changed: it cannot be moved out of it,
        // and deleting it needs the same permission, so say so, and still ask
        // (a share may allow the one and not the other).
        OpError::Permission => ask_delete(
            env,
            paths,
            Some(&format!("You do not have permission to move {it} to the Trash.")),
        ),
        other => notify(env.ui, error_text("move to the Trash", &other), true),
    }
}

/// Moving to the Trash would copy a big tree between drives. Ask, and say it
/// can be left alone.
fn ask_copy(env: &Env, paths: Vec<PathBuf>) {
    let it = if paths.len() == 1 { "it" } else { "them" };
    let env = env.clone();
    let title = format!("Copy {} to the Trash?", what(&paths));
    env.overlays.show_dialog(
        dialog(
            title,
            format!(
                "Moving {it} to the Trash means copying {it} to another drive first, which can take a long time and uses space there. To remove {it} without that, cancel and use Delete permanently from the menu opened with Shift."
            ),
        )
        .cancel("Cancel")
        .default_action(CANCEL_ID)
        .icon(DialogIcon::Warning)
        .response("Copy to Trash", ResponseKind::Suggested, {
            let env = env.clone();
            move |_: ()| move_copying(&env, paths.clone())
        }),
    );
}

fn move_copying(env: &Env, paths: Vec<PathBuf>) {
    if !begin(env, format!("Copying {} to Trash", what(&paths)), None) {
        return;
    }
    let env = env.clone();
    spawn_in_window(async move {
        let outcome = spawn_blocking(move || backend::trash_copying(&paths)).await;
        finish(&env);
        match outcome {
            Ok(trashed) => {
                let moved: Vec<PathBuf> = trashed.iter().map(|item| item.original.clone()).collect();
                undo::push(Step::Trash(trashed));
                let again = env.clone();
                env.overlays.toast_with_action(moved_text(&moved), "Undo", move |_: ()| {
                    super::transfer::undo(&again)
                });
            }
            Err(error) => notify(env.ui, error_text("move to the Trash", &error), true),
        }
        env.reload.call(());
    });
}

// ---------------------------------------------------------------------------
// Delete for good
// ---------------------------------------------------------------------------

/// Delete permanently (Shift+Delete, Delete permanently in the Shift menu, and
/// Delete or the menu inside the Trash). Always asks first.
pub(super) fn confirm_delete(env: &Env, paths: Vec<PathBuf>) {
    if !paths.is_empty() {
        ask_delete(env, paths, None);
    }
}

/// Ask before deleting for good. Cancel is the default action and has the
/// focus, so a stray Enter keeps the files.
pub(super) fn ask_delete(env: &Env, paths: Vec<PathBuf>, reason: Option<&str>) {
    let in_trash = paths.iter().all(|path| backend::in_trash(path));
    let prompt = delete_prompt(&paths, reason, in_trash);
    let env = env.clone();
    env.overlays.show_dialog(
        dialog(prompt.title, prompt.body)
            .cancel("Cancel")
            .default_action(CANCEL_ID)
            .icon(DialogIcon::Warning)
            .response("Delete permanently", ResponseKind::Destructive, {
                let env = env.clone();
                move |_: ()| delete(&env, paths.clone())
            }),
    );
}

fn delete(env: &Env, paths: Vec<PathBuf>) {
    let whole = what(&paths);
    run_permanent(
        env,
        "Deleting",
        whole.clone(),
        format!("Deleted {whole}."),
        // An item of the Trash takes its info file and its size line with it.
        move |progress, cancel| backend::delete_trashed(&paths, progress, cancel),
    );
}

// ---------------------------------------------------------------------------
// Empty the Trash
// ---------------------------------------------------------------------------

/// Count what the Trash holds (on a worker, with a Stop button: a huge Trash
/// takes a while), then ask with the count and the size in the question.
pub(super) fn confirm_empty_trash(env: &Env) {
    let cancel = Cancel::new();
    if !begin(env, "Counting what is in the Trash".to_owned(), Some(cancel.clone())) {
        return;
    }
    let env = env.clone();
    spawn_in_window(async move {
        let outcome = spawn_blocking(move || backend::trash_summary(&cancel)).await;
        finish(&env);
        match outcome {
            Ok(summary) if summary.items == 0 => notify(env.ui, "The Trash is already empty.", false),
            Ok(summary) => {
                let prompt = empty_prompt(summary);
                env.overlays.show_dialog(
                    dialog(prompt.title, prompt.body)
                        .cancel("Cancel")
                        .default_action(CANCEL_ID)
                        .icon(DialogIcon::Warning)
                        .response("Empty Trash", ResponseKind::Destructive, {
                            let env = env.clone();
                            move |_: ()| empty_trash(&env)
                        }),
                );
            }
            Err(OpError::Cancelled) => notify(env.ui, "Counting stopped.", false),
            Err(error) => notify(env.ui, error_text("count the Trash", &error), true),
        }
    });
}

fn empty_trash(env: &Env) {
    run_permanent(
        env,
        "Emptying",
        "the Trash".to_owned(),
        "Emptied the Trash.".to_owned(),
        backend::empty_trash,
    );
}

/// A permanent removal, with a busy line and a cancel button. Only called
/// from the answer to a question.
fn run_permanent(
    env: &Env,
    verb: &'static str,
    whole: String,
    done_text: String,
    job: impl FnOnce(&Progress, &Cancel) -> Result<(), OpError> + Send + 'static,
) {
    let cancel = Cancel::new();
    if !begin(env, format!("{verb} {whole}"), Some(cancel.clone())) {
        return;
    }
    let progress = Progress::new();
    let done = Arc::new(AtomicBool::new(false));
    spawn_in_window(follow(
        env.clone(),
        verb,
        whole,
        progress.clone(),
        cancel.clone(),
        Arc::new(Asker::default()),
        done.clone(),
    ));
    let env = env.clone();
    spawn_in_window(async move {
        let outcome = spawn_blocking(move || job(&progress, &cancel)).await;
        done.store(true, Ordering::Release);
        finish(&env);
        match outcome {
            Ok(()) => notify(env.ui, done_text, false),
            Err(OpError::Cancelled) => notify(env.ui, format!("{verb} stopped."), false),
            Err(error) => notify(env.ui, error_text("delete", &error), true),
        }
        env.reload.call(());
    });
}

// ---------------------------------------------------------------------------
// Restore
// ---------------------------------------------------------------------------

/// Restore: put `paths`, items of the Trash, back where they were. Asks only
/// to make a folder that is gone, and about a name that is taken.
pub(super) fn restore(env: &Env, paths: Vec<PathBuf>) {
    plan_then(env, paths, None);
}

/// Restore to…: put `paths` into a folder the person chooses.
pub(super) fn restore_to(env: &Env, paths: Vec<PathBuf>) {
    let Some(window) = env.window else {
        notify(env.ui, "There is no window to choose a folder in.", true);
        return;
    };
    let start = backend::trash_items(&paths)
        .into_iter()
        .find_map(|item| item.original.and_then(|original| original.parent().map(Path::to_path_buf)))
        .filter(|folder| folder.is_dir())
        .unwrap_or_else(|| env.home.clone());
    let env = env.clone();
    choose(
        window,
        ChooserRequest::select_folder()
            .title("Restore to")
            .accept_label("Restore")
            .current_folder(start),
        move |chosen| {
            if let Some(folder) = chosen.and_then(|chosen| chosen.paths.into_iter().next()) {
                restore_into(&env, paths, folder);
            }
        },
    );
}

/// Restore into `folder` with each item's original name (also what dragging
/// items out of the Trash does).
pub(super) fn restore_into(env: &Env, paths: Vec<PathBuf>, folder: PathBuf) {
    plan_then(env, paths, Some(folder));
}

/// Read the info files on a worker, then ask what needs asking.
fn plan_then(env: &Env, paths: Vec<PathBuf>, into: Option<PathBuf>) {
    if paths.is_empty() {
        return;
    }
    let env = env.clone();
    spawn_in_window(async move {
        let planned = spawn_blocking(move || backend::plan_restore(&paths, into.as_deref())).await;
        let unknown = planned.unknown();
        if !unknown.is_empty() {
            let names: Vec<PathBuf> = unknown.iter().map(|entry| entry.item.in_trash.clone()).collect();
            notify(env.ui, no_record_text(&names), true);
        }
        let plan = planned.without_unknown();
        if plan.entries.is_empty() {
            return;
        }
        let missing = plan.missing_folders();
        if missing.is_empty() {
            run_restore(&env, plan, false);
        } else {
            ask_to_make_folders(&env, plan, missing);
        }
    });
}

/// `“a” has no record of where it came from. Use Restore to… to choose a
/// folder.`
fn no_record_text(paths: &[PathBuf]) -> String {
    let has = if paths.len() == 1 { "has" } else { "have" };
    format!(
        "{} {has} no record of where {} came from. Use Restore to… to choose a folder.",
        what(paths),
        if paths.len() == 1 { "it" } else { "they" }
    )
}

/// The question before a folder that is gone is made again.
fn ask_to_make_folders(env: &Env, plan: RestorePlan, missing: Vec<PathBuf>) {
    let whole: Vec<PathBuf> = plan.entries.iter().map(|entry| entry.item.in_trash.clone()).collect();
    let (folders, verb) = match missing.as_slice() {
        [only] => (format!("The folder {}", only.display()), "is"),
        many => (format!("{} folders, such as {},", many.len(), many[0].display()), "are"),
    };
    let env = env.clone();
    env.overlays.show_dialog(
        dialog(
            format!("Make the folder again to restore {}?", what(&whole)),
            format!("{folders} {verb} no longer there. Restoring will make {} again.", if missing.len() == 1 { "it" } else { "them" }),
        )
        .cancel("Cancel")
        .default_action("restore")
        .response("Restore", ResponseKind::Suggested, {
            let env = env.clone();
            move |_: ()| run_restore(&env, plan.clone(), true)
        }),
    );
}

/// Do the restore with a busy line, a Stop button and the question for a name
/// that is taken, then say how it went and offer Undo.
fn run_restore(env: &Env, plan: RestorePlan, create_missing: bool) {
    let whole = what(
        &plan
            .entries
            .iter()
            .map(|entry| entry.item.in_trash.clone())
            .collect::<Vec<_>>(),
    );
    let cancel = Cancel::new();
    if !begin(env, format!("Restoring {whole}"), Some(cancel.clone())) {
        return;
    }
    let progress = Progress::new();
    let asker = Arc::new(Asker::default());
    let done = Arc::new(AtomicBool::new(false));
    spawn_in_window(follow(
        env.clone(),
        "Restoring",
        whole,
        progress.clone(),
        cancel.clone(),
        asker.clone(),
        done.clone(),
    ));
    let env = env.clone();
    spawn_in_window(async move {
        let outcome = spawn_blocking(move || {
            let policy = {
                let cancel = cancel.clone();
                ConflictPolicy::Ask(Box::new(move |conflict| asker.decide(conflict, &cancel)))
            };
            backend::restore(&plan, create_missing, &policy, &progress, &cancel)
        })
        .await;
        done.store(true, Ordering::Release);
        finish(&env);
        match outcome {
            Ok(restored) => restored_done(&env, restored),
            Err(RestoreFailure { done, error }) => {
                let any = !done.is_empty();
                if any {
                    undo::push(Step::Restore(done.clone()));
                    select_back(&env, &done);
                }
                match error {
                    OpError::Cancelled => notify(env.ui, "Restoring stopped.", false),
                    other => notify(env.ui, error_text_plain(&other), true),
                }
            }
        }
        env.reload.call(());
    });
}

/// An error as the status line says it: a restore's messages already name the
/// item and the folder, so nothing is added in front of them.
fn error_text_plain(error: &OpError) -> String {
    error.to_string()
}

/// Remember the restore for Undo and tell about it with a toast.
fn restored_done(env: &Env, restored: Vec<Restored>) {
    if restored.is_empty() {
        notify(env.ui, "Nothing was restored.", false);
        return;
    }
    let paths: Vec<PathBuf> = restored.iter().map(|item| item.to.clone()).collect();
    select_back(env, &restored);
    undo::push(Step::Restore(restored));
    let again = env.clone();
    env.overlays
        .toast_with_action(restored_text(&paths), "Undo", move |_: ()| super::transfer::undo(&again));
}

/// A restore into the folder on show selects what came back; elsewhere the
/// selection is left alone (the items were in the Trash).
fn select_back(env: &Env, restored: &[Restored]) {
    let here: Vec<PathBuf> = restored
        .iter()
        .map(|item| item.to.clone())
        .filter(|path| path.parent() == env.folder.as_deref())
        .collect();
    if !here.is_empty() {
        select_paths(env, &here);
    }
}

/// Undo of a restore: what came out goes back to the Trash and what it
/// replaced is taken back out. Blocking. Returns what is back in place.
pub(super) fn undo_restore(items: &[Restored]) -> Result<Vec<PathBuf>, OpError> {
    let mut back = Vec::new();
    for item in items {
        backend::trash(std::slice::from_ref(&item.to))?;
        if let Some(displaced) = &item.displaced {
            back.extend(backend::untrash(std::slice::from_ref(displaced))?);
        }
    }
    Ok(back)
}

// ---------------------------------------------------------------------------
// Drag and drop
// ---------------------------------------------------------------------------

/// What a drop of `paths` on `dest` means when the Trash is involved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Route {
    /// An ordinary move or copy.
    Plain,
    /// Onto the Trash: they are moved to it (a copy cannot go there).
    IntoTrash,
    /// Onto a folder inside the Trash: nothing goes in there.
    Refuse,
    /// Out of the Trash by a move: they are restored into the folder.
    OutOfTrash,
}

pub(super) fn route(
    paths: &[PathBuf],
    dest: &Path,
    copy: bool,
    place: impl Fn(&Path) -> Option<InBin>,
) -> Route {
    if let Some(place) = place(dest) {
        return if place.is_root() { Route::IntoTrash } else { Route::Refuse };
    }
    let all_items = !paths.is_empty()
        && paths
            .iter()
            .all(|path| place(path).is_some_and(|place| place.is_item()));
    if !copy && all_items { Route::OutOfTrash } else { Route::Plain }
}

/// Handle a drop or paste that involves the Trash. Returns whether it did;
/// `false` means an ordinary transfer.
pub(super) fn redirect(env: &Env, paths: &[PathBuf], dest: &Path, copy: bool) -> bool {
    match route(paths, dest, copy, backend::trash_place) {
        Route::Plain => false,
        Route::IntoTrash => {
            trash(env, paths.to_vec());
            true
        }
        Route::Refuse => {
            notify(
                env.ui,
                "Nothing can be put inside a folder in the Trash. Drop it on the Trash itself to move it there.",
                true,
            );
            true
        }
        Route::OutOfTrash => {
            restore_into(env, paths.to_vec(), dest.to_path_buf());
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(PathBuf::from).collect()
    }

    const HOME_BIN: &str = "/home/a/.local/share/Trash";

    /// A stand-in for [`backend::trash_place`] that knows the home bin.
    fn place(path: &Path) -> Option<InBin> {
        let files = Path::new(HOME_BIN).join("files");
        let below = path.strip_prefix(&files).ok()?;
        Some(InBin {
            bin: PathBuf::from(HOME_BIN),
            below: below.to_path_buf(),
        })
    }

    // ---- the words ---------------------------------------------------------

    #[test]
    fn the_toasts_name_the_item_or_count_them() {
        assert_eq!(moved_text(&paths(&["/a/report.txt"])), "Moved “report.txt” to Trash");
        assert_eq!(moved_text(&paths(&["/a/x", "/a/y", "/a/z"])), "Moved 3 items to Trash");
        assert_eq!(restored_text(&paths(&["/a/report.txt"])), "Restored “report.txt”");
        assert_eq!(restored_text(&paths(&["/a/x", "/a/y"])), "Restored 2 items");
    }

    #[test]
    fn the_delete_question_says_it_cannot_be_undone_in_and_out_of_the_trash() {
        let out = delete_prompt(&paths(&["/a/report.txt"]), None, false);
        assert_eq!(out.title, "Delete “report.txt” permanently?");
        assert!(out.body.starts_with("It will be deleted for good."), "{}", out.body);
        assert!(out.body.contains("will not go to the Trash"), "{}", out.body);
        assert!(out.body.contains("cannot be undone"), "{}", out.body);

        let many = delete_prompt(
            &paths(&["/a/x", "/a/y", "/a/z"]),
            Some("They cannot be moved to the Trash."),
            false,
        );
        assert_eq!(many.title, "Delete 3 items permanently?");
        assert!(many.body.starts_with("They cannot be moved to the Trash. They will be deleted"), "{}", many.body);
        assert!(many.body.contains("Undo cannot bring them back"), "{}", many.body);

        // In the Trash there is no Trash to avoid: no talk of it.
        let bin = delete_prompt(&paths(&["/a/x", "/a/y"]), None, true);
        assert_eq!(bin.title, "Delete 2 items permanently?");
        assert!(bin.body.starts_with("They will be deleted permanently."), "{}", bin.body);
        assert!(!bin.body.contains("Trash"), "{}", bin.body);
        assert!(bin.body.contains("cannot be undone"), "{}", bin.body);
    }

    #[test]
    fn the_empty_question_has_the_count_and_the_size() {
        let some = empty_prompt(TrashSummary { items: 124, bytes: 3_200_000_000 });
        assert_eq!(some.title, "Empty the Trash?");
        assert!(some.body.starts_with("124 items (3.2 GB) in the Trash will be deleted for good."), "{}", some.body);
        assert!(some.body.contains("cannot be undone"), "{}", some.body);
        let one = empty_prompt(TrashSummary { items: 1, bytes: 4096 });
        assert!(one.body.starts_with("1 item (4.1 kB) in the Trash"), "{}", one.body);
        let bare = empty_prompt(TrashSummary { items: 3, bytes: 0 });
        assert!(bare.body.starts_with("3 items in the Trash will"), "{}", bare.body);
    }

    // ---- what the Trash refuses ---------------------------------------------

    fn check(path: &Path) -> Result<(), TrashRefusal> {
        let text = path.to_str().unwrap();
        if text.starts_with("/mnt/nas") {
            Err(TrashRefusal::NoBin)
        } else if text.starts_with("/mnt/cd") {
            Err(TrashRefusal::ReadOnly)
        } else if text == "/home/a" {
            Err(TrashRefusal::Home)
        } else if text == "/home/a/Documents" {
            Err(TrashRefusal::UserFolder("Documents"))
        } else if text.starts_with(HOME_BIN) {
            Err(TrashRefusal::InTrash)
        } else {
            Ok(())
        }
    }

    #[test]
    fn items_the_trash_refuses_are_sorted_by_whether_a_delete_is_a_fair_offer() {
        let plan = plan_trash(
            paths(&["/home/a/x", "/mnt/nas/b", "/home/a", "/mnt/cd/c", "/home/a/Documents"]),
            check,
        );
        assert_eq!(plan.bin, paths(&["/home/a/x"]));
        let refusals = sort_refusals(plan.refused);
        assert_eq!(refusals.delete, paths(&["/mnt/nas/b", "/mnt/cd/c"]));
        assert_eq!(
            refusals.delete_reason.as_deref(),
            Some("They cannot be moved to the Trash."),
            "two different reasons: no single cause is named"
        );
        assert_eq!(refusals.refused, paths(&["/home/a", "/home/a/Documents"]));
        assert_eq!(
            refusals.refused_text.as_deref(),
            Some("2 items cannot be moved to the Trash."),
        );
    }

    #[test]
    fn one_refusal_is_told_with_its_own_reason_and_the_item() {
        let plan = plan_trash(paths(&["/home/a/Documents"]), check);
        assert!(plan.bin.is_empty());
        let refusals = sort_refusals(plan.refused);
        assert!(refusals.delete.is_empty(), "never offered a delete");
        assert_eq!(
            refusals.refused_text.as_deref(),
            Some("“Documents”: It is your Documents folder, so it cannot be moved to the Trash.")
        );
        let nas = sort_refusals(plan_trash(paths(&["/mnt/nas/b"]), check).refused);
        assert_eq!(nas.delete, paths(&["/mnt/nas/b"]));
        assert_eq!(
            nas.delete_reason.as_deref(),
            Some("Its drive has no Trash and one cannot be made there, so it cannot be moved to the Trash.")
        );
        let bin = sort_refusals(plan_trash(paths(&["/home/a/.local/share/Trash/files/x"]), check).refused);
        assert_eq!(bin.delete_reason.as_deref(), Some("It is in the Trash already."));
        let home = sort_refusals(plan_trash(paths(&["/home/a"]), check).refused);
        assert_eq!(
            home.refused_text.as_deref(),
            Some("“a”: Your home folder cannot be moved to the Trash.")
        );
    }

    #[test]
    fn nothing_refused_means_nothing_to_say() {
        let plan = plan_trash(paths(&["/home/a/x", "/home/a/y"]), check);
        assert_eq!(plan.bin.len(), 2);
        let refusals = sort_refusals(plan.refused);
        assert!(refusals.delete.is_empty() && refusals.refused.is_empty());
        assert_eq!(refusals.delete_reason, None);
        assert_eq!(refusals.refused_text, None);
    }

    #[test]
    fn an_item_with_no_record_says_so_and_points_at_restore_to() {
        assert_eq!(
            no_record_text(&paths(&["/t/files/stray.txt"])),
            "“stray.txt” has no record of where it came from. Use Restore to… to choose a folder."
        );
        assert_eq!(
            no_record_text(&paths(&["/t/files/a", "/t/files/b"])),
            "2 items have no record of where they came from. Use Restore to… to choose a folder."
        );
    }

    // ---- drag and drop -----------------------------------------------------

    #[test]
    fn a_drop_on_the_trash_trashes_and_a_drop_out_of_it_restores() {
        let bin_root = format!("{HOME_BIN}/files");
        let item = format!("{HOME_BIN}/files/a.txt");
        let inside = format!("{HOME_BIN}/files/proj/x.txt");
        let sub = format!("{HOME_BIN}/files/proj");
        // Onto the Trash place: trash them, even with Ctrl (a copy cannot go there).
        for copy in [false, true] {
            assert_eq!(route(&paths(&["/home/a/x"]), Path::new(&bin_root), copy, place), Route::IntoTrash);
        }
        // Onto a folder in the Trash: refused.
        assert_eq!(route(&paths(&["/home/a/x"]), Path::new(&sub), false, place), Route::Refuse);
        // Out of the Trash into a folder: restore, unless it is a copy.
        assert_eq!(route(&paths(&[&item]), Path::new("/home/a/Docs"), false, place), Route::OutOfTrash);
        assert_eq!(route(&paths(&[&item]), Path::new("/home/a/Docs"), true, place), Route::Plain);
        // Something inside a trashed folder is not an item: a plain move.
        assert_eq!(route(&paths(&[&inside]), Path::new("/home/a/Docs"), false, place), Route::Plain);
        // A mix is not all items either.
        assert_eq!(route(&paths(&[&item, "/home/a/x"]), Path::new("/home/a/Docs"), false, place), Route::Plain);
        // Ordinary drops stay ordinary.
        assert_eq!(route(&paths(&["/home/a/x"]), Path::new("/home/a/Docs"), false, place), Route::Plain);
        assert_eq!(route(&[], Path::new("/home/a/Docs"), false, place), Route::Plain);
    }
}
