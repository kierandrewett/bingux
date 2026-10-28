//! Running Compress and Extract: the busy line while a job works, and the
//! toast when it is over.
//!
//! Every job runs on the worker pool (`bingux_files::ops::pack`, `reduce` and
//! `unpack` block), reports through a [`Progress`] that this module polls
//! every 150 ms for the busy line, and can be stopped with the busy line's
//! cancel button: the programs are killed and the hidden work folder goes, so
//! nothing partial is left.
//!
//! When a job ends a toast says what was made, with **Show** (selects the
//! result in the list) and **Undo** (sends what was made to the Trash and
//! puts back what was replaced). A failure is a toast with the program's own
//! words.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use bingux_files::Cancel;
use bingux_files::ops::{
    Batch, OnConflict, OpError, PackJob, Packed, PackOptions, Progress, ReduceJob, Reduced, Stage,
    Trashed, UnpackOptions, Unpacked, pack, reduce, trash, unpack, untrash,
};
use bingux_files::ops::prefs::Prefs;
use freya::prelude::*;

use super::actions::{Busy, Env, notify, set};
use super::compress_model::{created_text, reduced_text};
use crate::shell::{BannerKind, Toast};
use crate::task::{spawn_blocking, spawn_detached};

/// How often the busy line is brought up to date.
const TICK: Duration = Duration::from_millis(150);

/// What a finished job leaves, for Show and Undo.
#[derive(Clone, Debug, Default)]
struct Made {
    /// Where the result is: what Show selects.
    show: Vec<PathBuf>,
    /// What Undo sends to the Trash.
    created: Vec<PathBuf>,
    /// What Undo puts back.
    restore: Vec<Trashed>,
}

/// Claim the one busy slot, as every long operation does: two at once would
/// fight over the status line and over each other's files.
fn begin(env: &Env, label: String, cancel: Cancel) -> bool {
    if env.ui.busy.peek().is_some() {
        notify(env.ui, "Wait for the current operation to finish.", true);
        return false;
    }
    set(
        env.ui.busy,
        Some(Busy {
            label,
            fraction: None,
            cancel: Some(cancel),
        }),
    );
    true
}

fn finish(env: &Env) {
    set(env.ui.busy, None);
}

/// Select `paths` in the view, opening their folder first if it is not the
/// one on show.
fn show(env: &Env, paths: &[PathBuf]) {
    let Some(first) = paths.first() else {
        return;
    };
    if let Some(parent) = first.parent()
        && env.folder.as_deref() != Some(parent)
    {
        env.open_folder.call(parent.to_path_buf());
    }
    let mut selection = env.selection;
    let mut selection = selection.write();
    selection.select_only(first);
    for path in &paths[1..] {
        selection.toggle(path);
    }
}

/// Send what was made to the Trash and put back what it replaced.
fn undo_made(env: &Env, made: Made) {
    let env = env.clone();
    spawn_in_window(async move {
        let outcome = spawn_blocking(move || -> Result<(), OpError> {
            if !made.created.is_empty() {
                trash(&made.created)?;
            }
            if !made.restore.is_empty() {
                untrash(&made.restore)?;
            }
            Ok(())
        })
        .await;
        match outcome {
            Ok(()) => notify(env.ui, "Undone.", false),
            Err(error) => notify(env.ui, format!("Could not undo that: {error}"), true),
        }
        env.reload.call(());
    });
}

/// The toast of a finished job: `text`, Show and a quiet Undo.
fn done_toast(env: &Env, text: String, made: Made) {
    let mut toast = Toast::new(text).kind(BannerKind::Neutral);
    if !made.show.is_empty() {
        let (shown, show_env) = (made.show.clone(), env.clone());
        toast = toast.action("Show", move |()| show(&show_env, &shown));
    }
    if !made.created.is_empty() {
        let undo_env = env.clone();
        toast = toast.action("Undo", move |()| undo_made(&undo_env, made.clone()));
    }
    env.overlays.show_toast(toast);
}

fn failure_toast(env: &Env, text: String) {
    env.overlays.show_toast(Toast::new(text).kind(BannerKind::Error));
}

/// What the busy line says: `Compressing “Photos”, 12 of 40 items`.
fn label(verb: &str, whole: &str, progress: &Progress) -> String {
    if progress.is_scanning() {
        return format!("{verb} {whole}: counting");
    }
    if let Some(drive) = progress.finishing() {
        return format!("Finishing, writing to {drive}");
    }
    let (done, total) = (progress.files_done(), progress.files_total());
    if total > 1 {
        format!("{verb} {whole}, {done} of {total}")
    } else {
        format!("{verb} {whole}")
    }
}

/// Keep the busy line current until `done`.
async fn follow(env: Env, verb: &'static str, whole: String, progress: Progress, batch: Option<Batch>, cancel: Cancel, done: Arc<AtomicBool>) {
    while !done.load(Ordering::Acquire) {
        let finishing = progress.finishing().is_some();
        let mut text = label(verb, &whole, &progress);
        if let Some(batch) = &batch {
            // The file being worked on and how far it is: the per-file line.
            let lines = batch.lines();
            let total = lines.len();
            let finished = lines.iter().filter(|line| line.stage == Stage::Done).count();
            if let Some(live) = lines.iter().find(|line| line.stage == Stage::Working) {
                let percent = live.fraction.map(|fraction| format!(", {}%", (fraction * 100.0).round() as u32)).unwrap_or_default();
                text = format!("{verb} {} of {total}: {}{percent}", (finished + 1).min(total), live.name);
            }
        }
        set(
            env.ui.busy,
            Some(Busy {
                label: text,
                fraction: if finishing { None } else { progress.fraction() },
                cancel: (!finishing).then(|| cancel.clone()),
            }),
        );
        timer(TICK).await;
    }
}

/// Make an archive.
pub(super) fn run_pack(env: &Env, job: PackJob, whole: String, prefs: Option<Prefs>) {
    let cancel = Cancel::new();
    if !begin(env, format!("Compressing {whole}"), cancel.clone()) {
        return;
    }
    if let Some(prefs) = prefs {
        spawn_detached("compress-prefs", move || {
            let _ = prefs.save();
        });
    }
    let progress = Progress::new();
    let done = Arc::new(AtomicBool::new(false));
    spawn_in_window(follow(env.clone(), "Compressing", whole, progress.clone(), None, cancel.clone(), done.clone()));
    let env = env.clone();
    spawn_in_window(async move {
        let outcome = spawn_blocking(move || pack(&job, &progress, &cancel)).await;
        done.store(true, Ordering::Release);
        finish(&env);
        match outcome {
            Ok(packed) => packed_toast(&env, packed),
            Err(OpError::Cancelled) => notify(env.ui, "Compression stopped.", false),
            Err(error) => failure_toast(&env, format!("Could not compress: {error}")),
        }
        env.reload.call(());
    });
}

fn packed_toast(env: &Env, packed: Packed) {
    let name = packed.archive.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut text = created_text(&name);
    if packed.files.len() > 1 {
        text = format!("{text} in {} volumes", packed.files.len());
    }
    if let Some(problem) = &packed.trash_failed {
        text = format!("{text}. The originals were not moved to the Trash: {problem}");
    } else if !packed.trashed.is_empty() {
        text = format!("{text}. The originals are in the Trash");
    }
    let mut restore = packed.replaced.clone();
    restore.extend(packed.trashed.clone());
    // Undo trashes the new archive first, then brings back what it replaced
    // or what was deleted after it.
    let made = Made {
        show: vec![packed.archive.clone()],
        created: packed.files.clone(),
        restore,
    };
    done_toast(env, text, made);
}

/// Reduce pictures, films and songs.
pub(super) fn run_reduce(env: &Env, job: ReduceJob, whole: String, prefs: Option<Prefs>) {
    let cancel = Cancel::new();
    if !begin(env, format!("Reducing {whole}"), cancel.clone()) {
        return;
    }
    if let Some(prefs) = prefs {
        spawn_detached("compress-prefs", move || {
            let _ = prefs.save();
        });
    }
    let progress = Progress::new();
    let batch = Batch::new();
    let done = Arc::new(AtomicBool::new(false));
    spawn_in_window(follow(env.clone(), "Reducing", whole, progress.clone(), Some(batch.clone()), cancel.clone(), done.clone()));
    let env = env.clone();
    spawn_in_window(async move {
        let outcome = spawn_blocking(move || reduce(&job, &progress, &batch, &cancel)).await;
        done.store(true, Ordering::Release);
        finish(&env);
        match outcome {
            Ok(reduced) => reduced_toast(&env, &reduced),
            Err(OpError::Cancelled) => notify(env.ui, "Reducing stopped.", false),
            Err(error) => failure_toast(&env, format!("Could not reduce: {error}")),
        }
        env.reload.call(());
    });
}

fn reduced_toast(env: &Env, reduced: &Reduced) {
    let (from, to) = reduced.sizes();
    let failures = reduced.failures();
    let mut text = reduced_text(reduced.reduced(), reduced.kept(), failures.len(), from, to);
    // The first failure says why, in the program's words.
    if let Some(first) = failures.first() {
        let why = match &first.outcome {
            bingux_files::ops::Outcome::Failed { error } => error.clone(),
            bingux_files::ops::Outcome::Skipped { reason } => reason.clone(),
            _ => String::new(),
        };
        if !why.is_empty() {
            text = format!("{text}. {why}");
        }
    }
    let notes: Vec<String> = reduced
        .files
        .iter()
        .filter_map(|file| match &file.outcome {
            bingux_files::ops::Outcome::Reduced { note: Some(note), .. } => Some(note.clone()),
            _ => None,
        })
        .collect();
    if let Some(note) = notes.first() {
        text = format!("{text}. {note}");
    }
    let made = Made {
        show: reduced.created(),
        created: reduced.created(),
        restore: reduced.trashed(),
    };
    if made.created.is_empty() {
        let error = failures.first().is_some() && reduced.kept() == 0;
        if error {
            failure_toast(env, text);
        } else {
            env.overlays.show_toast(Toast::new(text));
        }
        return;
    }
    done_toast(env, text, made);
}

/// Extract an archive.
pub(super) fn run_unpack(env: &Env, archive: PathBuf, options: UnpackOptions, whole: String) {
    let cancel = Cancel::new();
    if !begin(env, format!("Extracting {whole}"), cancel.clone()) {
        return;
    }
    let progress = Progress::new();
    let done = Arc::new(AtomicBool::new(false));
    spawn_in_window(follow(env.clone(), "Extracting", whole, progress.clone(), None, cancel.clone(), done.clone()));
    let env = env.clone();
    spawn_in_window(async move {
        let outcome = spawn_blocking(move || unpack(&archive, &options, &progress, &cancel)).await;
        done.store(true, Ordering::Release);
        finish(&env);
        match outcome {
            Ok(unpacked) => unpacked_toast(&env, unpacked),
            Err(OpError::Cancelled) => notify(env.ui, "Extracting stopped.", false),
            Err(error) => failure_toast(&env, format!("Could not extract: {error}")),
        }
        env.reload.call(());
    });
}

fn unpacked_toast(env: &Env, unpacked: Unpacked) {
    let text = match (unpacked.items.as_slice(), unpacked.skipped) {
        ([], skipped) if skipped > 0 => "Nothing was extracted: every name was taken".to_string(),
        ([only], 0) => format!("Extracted to “{}”", name_of(only)),
        (items, 0) => format!("Extracted {} items", items.len()),
        (items, skipped) => format!("Extracted {} items, {skipped} skipped because the name was taken", items.len()),
    };
    let made = Made {
        show: unpacked.items.clone(),
        created: unpacked.items.clone(),
        restore: unpacked.replaced.clone(),
    };
    if made.created.is_empty() {
        env.overlays.show_toast(Toast::new(text));
        return;
    }
    done_toast(env, text, made);
}

fn name_of(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// A quick archive: the default name beside the originals, no questions, the
/// first free name when it is taken.
pub(super) fn run_quick(env: &Env, roots: Vec<PathBuf>, base: String, format: bingux_files::ops::Format, whole: String) {
    let Some(dest_dir) = roots.first().and_then(|path| path.parent()).map(Path::to_path_buf) else {
        return;
    };
    let job = PackJob {
        roots,
        dest_dir,
        base,
        options: PackOptions { format, ..PackOptions::default() },
        on_conflict: OnConflict::KeepBoth,
    };
    run_pack(env, job, whole, None);
}

/// Compress with the default options in `format`, with no questions: the
/// name of the one item, or "Archive", beside the originals, the first free
/// name when it is taken.
pub(super) fn quick(env: &Env, selected: &[bingux_files::Entry], format: bingux_files::ops::Format) {
    let subject = super::compress_model::Subject::of(selected);
    let whole = match subject.names.as_slice() {
        [only] => format!("“{only}”"),
        many => format!("{} items", many.len()),
    };
    run_quick(env, subject.paths.clone(), subject.default_base(), format, whole);
}

/// Extract Here: into a new folder beside the archive, replacing nothing.
pub(super) fn extract_here(env: &Env, archive: PathBuf) {
    let Some(dest_dir) = archive.parent().map(Path::to_path_buf) else {
        return;
    };
    let options = UnpackOptions {
        dest_dir,
        create_folder: true,
        overwrite: bingux_files::ops::Overwrite::KeepBoth,
        password: None,
    };
    let whole = format!("“{}”", name_of(&archive));
    run_unpack(env, archive, options, whole);
}
