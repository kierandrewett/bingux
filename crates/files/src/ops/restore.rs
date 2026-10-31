//! Putting items of the bin back: to where they were ([`plan_restore`] with
//! no folder), or into a folder somebody picked.
//!
//! Restoring is two steps so a person is asked what needs asking. First
//! [`plan_restore`] reads each item's `.trashinfo` and says where it would go,
//! which folders on the way are gone, and which names are taken. Then
//! [`restore`] does it, with the answers: make the missing folders (or not),
//! and what to do about a taken name ([`ConflictPolicy`]: skip, keep both, or
//! replace, where the thing that was there goes to the Trash so it can be
//! brought back).
//!
//! An item moves with one `renameat2(RENAME_NOREPLACE)` when the folder is on
//! the bin's drive, else through the same checked copy a move between drives
//! uses. Its `.trashinfo` and its line of `directorysizes` go with it.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

use crate::listing::Cancel;

use super::durable::sync_dirs;
use super::names::keep_both_name;
use super::transfer::{Conflict, ConflictPolicy, Resolution, move_to};
use super::trash::{Context, Copying, Trashed, trash_in};
use super::trash_info::{TrashItem, forget_sizes, item_in, split_extension_bytes};
use super::{OpError, Progress, rename_noreplace};

/// One item to put back and what stands in the way.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestoreEntry {
    pub item: TrashItem,
    /// Where it goes. `None` when its info file is missing or says nothing
    /// and no folder was chosen: nobody knows where it came from.
    pub target: Option<PathBuf>,
    /// Folders on the way to `target` that do not exist, outermost first.
    pub missing: Vec<PathBuf>,
    /// Something is already at `target`.
    pub taken: bool,
}

/// What [`restore`] is about to do.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RestorePlan {
    pub entries: Vec<RestoreEntry>,
}

impl RestorePlan {
    /// The entries that cannot be restored to their place: no record of it.
    pub fn unknown(&self) -> Vec<&RestoreEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.target.is_none())
            .collect()
    }

    /// Every folder that has to be made, once, outermost first.
    pub fn missing_folders(&self) -> Vec<PathBuf> {
        let mut folders: Vec<PathBuf> = Vec::new();
        for entry in &self.entries {
            for folder in &entry.missing {
                if !folders.contains(folder) {
                    folders.push(folder.clone());
                }
            }
        }
        folders.sort_by_key(|folder| folder.components().count());
        folders
    }

    /// At least one name is taken: a question to answer.
    pub fn any_taken(&self) -> bool {
        self.entries.iter().any(|entry| entry.taken)
    }

    /// The same plan without the entries that have no place to go.
    pub fn without_unknown(&self) -> RestorePlan {
        RestorePlan {
            entries: self
                .entries
                .iter()
                .filter(|entry| entry.target.is_some())
                .cloned()
                .collect(),
        }
    }
}

/// One item that is back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Restored {
    /// The item as it was in the bin.
    pub from: TrashItem,
    /// Where it is now.
    pub to: PathBuf,
    /// What was at `to` and went to the Trash to make room (Replace).
    pub displaced: Option<Trashed>,
}

/// A restore that stopped: what was put back before, and why it stopped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestoreFailure {
    pub done: Vec<Restored>,
    pub error: OpError,
}

/// Read `paths`, items of a bin, and say where each would go: its original
/// place, or `into` with its original name when a folder was chosen. A path
/// that is not an item of a bin (inside a binned folder, or elsewhere) is left
/// out. Reads one small file per item.
pub fn plan_restore(paths: &[PathBuf], into: Option<&Path>) -> RestorePlan {
    plan_restore_in(super::trash::home_trash().as_deref(), paths, into)
}

pub(super) fn plan_restore_in(
    home_trash: Option<&Path>,
    paths: &[PathBuf],
    into: Option<&Path>,
) -> RestorePlan {
    let entries = paths
        .iter()
        .filter_map(|path| item_in(path, home_trash))
        .map(|item| {
            let target = match into {
                Some(folder) => Some(folder.join(item.original_name())),
                None => item.original.clone(),
            };
            let missing = match (&target, into) {
                (Some(target), None) => missing_folders(target),
                _ => Vec::new(),
            };
            let taken = target
                .as_deref()
                .is_some_and(|target| target.symlink_metadata().is_ok());
            RestoreEntry {
                item,
                target,
                missing,
                taken,
            }
        })
        .collect();
    RestorePlan { entries }
}

/// The folders above `target` that do not exist, outermost first.
fn missing_folders(target: &Path) -> Vec<PathBuf> {
    let mut missing = Vec::new();
    let mut at = target.parent();
    while let Some(dir) = at {
        match fs::metadata(dir) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                missing.push(dir.to_path_buf())
            }
            _ => break,
        }
        at = dir.parent();
    }
    missing.reverse();
    missing
}

/// Put every entry of `plan` back. `create_missing` makes the folders that are
/// gone (ask first); without it an entry whose folder is gone is an error.
/// `conflicts` settles a name that is taken: `Skip` leaves the item in the
/// Trash, `KeepBoth` restores it under a "(copy)" name, `Replace` sends what is
/// there to the Trash and puts the item in its place. An entry with no target
/// is an error: filter them with [`RestorePlan::without_unknown`] first.
///
/// Stops at the first error; what was already put back stays, and is returned
/// in the failure so it can be undone.
pub fn restore(
    plan: &RestorePlan,
    create_missing: bool,
    conflicts: &ConflictPolicy,
    progress: &Progress,
    cancel: &Cancel,
) -> Result<Vec<Restored>, RestoreFailure> {
    let ctx = Context::system();
    restore_in(
        ctx.as_ref(),
        plan,
        create_missing,
        conflicts,
        progress,
        cancel,
    )
}

pub(super) fn restore_in(
    ctx: Option<&Context>,
    plan: &RestorePlan,
    create_missing: bool,
    conflicts: &ConflictPolicy,
    progress: &Progress,
    cancel: &Cancel,
) -> Result<Vec<Restored>, RestoreFailure> {
    let mut done = Vec::with_capacity(plan.entries.len());
    progress.add_total(0, plan.entries.len() as u64);
    for entry in &plan.entries {
        if cancel.is_cancelled() {
            return Err(RestoreFailure {
                done,
                error: OpError::Cancelled,
            });
        }
        match restore_one(ctx, entry, create_missing, conflicts, progress, cancel) {
            Ok(Some(restored)) => {
                done.push(restored);
                progress.add_files(1);
            }
            Ok(None) => progress.add_files(1),
            Err(error) => return Err(RestoreFailure { done, error }),
        }
    }
    Ok(done)
}

fn decide(policy: &ConflictPolicy, conflict: &Conflict) -> Resolution {
    match policy {
        ConflictPolicy::Skip => Resolution::Skip,
        ConflictPolicy::Replace => Resolution::Replace,
        ConflictPolicy::KeepBoth => Resolution::KeepBoth,
        ConflictPolicy::Ask(ask) => ask(conflict),
    }
}

fn quoted(path: &Path) -> String {
    format!(
        "“{}”",
        path.file_name()
            .map_or_else(|| path.to_string_lossy(), |name| name.to_string_lossy())
    )
}

fn restore_one(
    ctx: Option<&Context>,
    entry: &RestoreEntry,
    create_missing: bool,
    conflicts: &ConflictPolicy,
    progress: &Progress,
    cancel: &Cancel,
) -> Result<Option<Restored>, OpError> {
    let item = &entry.item;
    let Some(mut target) = entry.target.clone() else {
        return Err(OpError::Other(format!(
            "There is no record of where {} came from. Use Restore to… to choose a folder.",
            quoted(&item.in_trash)
        )));
    };
    let Some(parent) = target.parent().map(Path::to_path_buf) else {
        return Err(OpError::Invalid(crate::location::NameError::Empty));
    };
    prepare_folder(&parent, &target, create_missing)?;

    let mut displaced = None;
    if target.symlink_metadata().is_ok() {
        let conflict = Conflict {
            source: item.in_trash.clone(),
            existing: target.clone(),
        };
        match decide(conflicts, &conflict) {
            Resolution::Skip => return Ok(None),
            Resolution::Cancel => return Err(OpError::Cancelled),
            Resolution::KeepBoth => {
                let name = keep_both_target(&parent, target.file_name().unwrap_or_default());
                target = parent.join(name);
            }
            Resolution::Replace => {
                let ctx = ctx
                    .ok_or_else(|| OpError::Other("Your home folder could not be found.".into()))?;
                // What is there goes to the Trash, so Replace can be undone.
                let moved = trash_in(ctx, std::slice::from_ref(&target), Copying::Small).map_err(
                    |error| {
                        OpError::Other(format!(
                            "{} could not be moved to the Trash to make room: {error}",
                            quoted(&target)
                        ))
                    },
                )?;
                displaced = moved.into_iter().next();
            }
        }
    }

    progress.set_current(&item.original_name().to_string_lossy());
    place(&item.in_trash, &target, progress, cancel)?;
    let _ = fs::remove_file(&item.info);
    if let (Some(bin), Some(name)) = (
        item.info.parent().and_then(Path::parent),
        item.in_trash.file_name(),
    ) {
        forget_sizes(bin, &[name]);
    }
    let mut synced: Vec<&Path> = vec![&parent];
    if let Some(info_dir) = item.info.parent() {
        synced.push(info_dir);
    }
    sync_dirs(&synced)?;
    Ok(Some(Restored {
        from: item.clone(),
        to: target,
        displaced,
    }))
}

/// Make sure `parent` is a folder to put `target` in: make it when it is gone
/// and `create_missing`, else say what is wrong.
fn prepare_folder(parent: &Path, target: &Path, create_missing: bool) -> Result<(), OpError> {
    match fs::metadata(parent) {
        Ok(meta) if meta.is_dir() => Ok(()),
        Ok(_) => Err(OpError::Other(format!(
            "{} is not a folder, so {} cannot be put back there.",
            parent.display(),
            quoted(target)
        ))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            if !create_missing {
                return Err(OpError::Other(format!(
                    "The folder {} no longer exists, so {} cannot be put back.",
                    parent.display(),
                    quoted(target)
                )));
            }
            let made = missing_folders(target);
            fs::create_dir_all(parent).map_err(|error| restore_io_error(error, target))?;
            let mut synced: Vec<&Path> = made.iter().filter_map(|dir| dir.parent()).collect();
            synced.push(parent);
            sync_dirs(&synced)?;
            Ok(())
        }
        Err(error) => Err(restore_io_error(error, target)),
    }
}

/// Move `from` (in the bin) to `to`, refusing to replace anything. Across
/// drives it is the checked copy a move uses.
fn place(from: &Path, to: &Path, progress: &Progress, cancel: &Cancel) -> Result<(), OpError> {
    match rename_noreplace(from, to) {
        Ok(()) => Ok(()),
        Err(error) if error.raw_os_error() == Some(libc::EXDEV) => {
            across_drives(from, to, progress, cancel)
        }
        Err(error) => Err(restore_io_error(error, to)),
    }
}

fn across_drives(
    from: &Path,
    to: &Path,
    progress: &Progress,
    cancel: &Cancel,
) -> Result<(), OpError> {
    let parent = to.parent().ok_or(OpError::NotFound)?;
    let landed = move_to(
        &[from.to_path_buf()],
        parent,
        &ConflictPolicy::KeepBoth,
        progress,
        cancel,
    )?;
    let landed = landed.into_iter().next().ok_or(OpError::NotFound)?;
    if landed != to {
        rename_noreplace(&landed, to).map_err(|error| restore_io_error(error, to))?;
    }
    Ok(())
}

/// What a failed move back means, in words that name the item and the folder.
fn restore_io_error(error: io::Error, target: &Path) -> OpError {
    let parent = target.parent().map_or_else(
        || "its folder".to_owned(),
        |parent| parent.display().to_string(),
    );
    let name = quoted(target);
    match error.raw_os_error() {
        Some(libc::EACCES | libc::EPERM) => OpError::Other(format!(
            "You do not have permission to put {name} back in {parent}."
        )),
        Some(libc::EROFS) => OpError::Other(format!(
            "{parent} is on a read-only drive, so {name} cannot be put back there."
        )),
        Some(libc::ENOTDIR) => OpError::Other(format!(
            "{parent} is not a folder, so {name} cannot be put back there."
        )),
        Some(libc::ENOENT) => OpError::Other(format!(
            "The folder {parent} no longer exists, so {name} cannot be put back."
        )),
        _ => OpError::from(error),
    }
}

/// A name for a copy beside something that has the name: "a (copy).txt", and
/// for a name that is not UTF-8, "name (copy 2)" built from its own bytes.
fn keep_both_target(parent: &Path, name: &OsStr) -> OsString {
    let taken = |candidate: &OsStr| parent.join(candidate).symlink_metadata().is_ok();
    if let Some(text) = name.to_str() {
        return OsString::from(keep_both_name(text, |candidate| {
            taken(OsStr::new(candidate))
        }));
    }
    let (stem, extension) = split_extension_bytes(name.as_bytes());
    let mut count = 1u32;
    loop {
        let label = if count == 1 {
            " (copy)".to_owned()
        } else {
            format!(" (copy {count})")
        };
        let mut out = stem.to_vec();
        out.extend_from_slice(label.as_bytes());
        out.extend_from_slice(extension);
        let candidate = OsString::from_vec(out);
        if !taken(&candidate) {
            return candidate;
        }
        count += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::Temp;
    use super::super::trash::tests::context;
    use super::super::trash::{Copying, trash_in};
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// Trash `files` in a made-up system and plan restoring them.
    fn binned(temp: &Temp, files: &[&Path]) -> (Context, Vec<Trashed>) {
        let ctx = context(temp, &[]);
        let paths: Vec<PathBuf> = files.iter().map(|path| path.to_path_buf()).collect();
        let items = trash_in(&ctx, &paths, Copying::Small).unwrap();
        (ctx, items)
    }

    fn plan_of(ctx: &Context, items: &[Trashed], into: Option<&Path>) -> RestorePlan {
        let paths: Vec<PathBuf> = items.iter().map(|item| item.in_trash.clone()).collect();
        plan_restore_in(Some(&ctx.home_trash), &paths, into)
    }

    fn run(
        ctx: &Context,
        plan: &RestorePlan,
        create: bool,
        policy: &ConflictPolicy,
    ) -> Result<Vec<Restored>, RestoreFailure> {
        restore_in(
            Some(ctx),
            plan,
            create,
            policy,
            &Progress::new(),
            &Cancel::new(),
        )
    }

    #[test]
    fn restore_puts_an_item_back_where_it_was_and_cleans_up_after_it() {
        let temp = Temp::new("restore");
        let file = temp.write("home/Docs/a.txt", "one");
        let folder = temp.dir("home/proj");
        temp.write("home/proj/x", "x");
        let (ctx, items) = binned(&temp, &[&file, &folder]);
        assert!(ctx.home_trash.join("directorysizes").exists());
        let plan = plan_of(&ctx, &items, None);
        assert_eq!(plan.entries.len(), 2);
        assert!(
            plan.entries
                .iter()
                .all(|entry| !entry.taken && entry.missing.is_empty())
        );
        assert_eq!(plan.entries[0].target.as_deref(), Some(file.as_path()));
        let done = run(&ctx, &plan, false, &ConflictPolicy::Skip).unwrap();
        assert_eq!(done.len(), 2);
        assert_eq!(fs::read_to_string(&file).unwrap(), "one");
        assert!(folder.join("x").exists());
        assert!(
            !items[0].info.exists() && !items[1].info.exists(),
            "the info files go"
        );
        assert!(
            !ctx.home_trash.join("directorysizes").exists(),
            "and the size line"
        );
        assert_eq!(
            fs::read_dir(ctx.home_trash.join("files")).unwrap().count(),
            0
        );
        assert_eq!(done[0].to, file);
        assert_eq!(done[0].displaced, None);
    }

    #[test]
    fn a_missing_folder_is_reported_and_only_made_when_asked() {
        let temp = Temp::new("restore-missing");
        let file = temp.write("home/deep/er/a.txt", "a");
        let (ctx, items) = binned(&temp, &[&file]);
        fs::remove_dir_all(temp.path().join("home/deep")).unwrap();
        let plan = plan_of(&ctx, &items, None);
        let deep = temp.path().join("home/deep");
        assert_eq!(plan.entries[0].missing, [deep.clone(), deep.join("er")]);
        assert_eq!(plan.missing_folders(), [deep.clone(), deep.join("er")]);
        // Not asked: it fails, says which folder, and moves nothing.
        let failure = run(&ctx, &plan, false, &ConflictPolicy::Skip).unwrap_err();
        let OpError::Other(text) = &failure.error else {
            panic!("{failure:?}")
        };
        assert!(
            text.contains("no longer exists") && text.contains("er"),
            "{text}"
        );
        assert!(items[0].in_trash.exists());
        assert!(failure.done.is_empty());
        // Asked: the folders are made and the file is back.
        run(&ctx, &plan, true, &ConflictPolicy::Skip).unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "a");
    }

    #[test]
    fn a_taken_name_asks_and_each_answer_does_what_it_says() {
        let temp = Temp::new("restore-taken");
        let file = temp.write("home/a.txt", "trashed");
        let (ctx, items) = binned(&temp, &[&file]);
        let plan = {
            fs::write(&file, "newer").unwrap();
            plan_of(&ctx, &items, None)
        };
        assert!(plan.entries[0].taken && plan.any_taken());

        // Skip: nothing changes.
        let done = run(&ctx, &plan, false, &ConflictPolicy::Skip).unwrap();
        assert!(done.is_empty());
        assert_eq!(fs::read_to_string(&file).unwrap(), "newer");
        assert!(items[0].in_trash.exists() && items[0].info.exists());

        // Cancel through the question: stops, nothing changes.
        let ask_cancel = ConflictPolicy::Ask(Box::new(|_| Resolution::Cancel));
        assert_eq!(
            run(&ctx, &plan, false, &ask_cancel).unwrap_err().error,
            OpError::Cancelled
        );
        assert!(items[0].in_trash.exists());

        // Keep both: the restored one gets a "(copy)" name beside the newer.
        let done = run(&ctx, &plan, false, &ConflictPolicy::KeepBoth).unwrap();
        assert_eq!(done[0].to, temp.path().join("home/a (copy).txt"));
        assert_eq!(fs::read_to_string(&done[0].to).unwrap(), "trashed");
        assert_eq!(fs::read_to_string(&file).unwrap(), "newer");
    }

    #[test]
    fn replace_sends_what_was_there_to_the_trash_so_it_can_come_back() {
        let temp = Temp::new("restore-replace");
        let file = temp.write("home/a.txt", "old");
        let (ctx, items) = binned(&temp, &[&file]);
        fs::write(&file, "newer").unwrap();
        let plan = plan_of(&ctx, &items, None);
        let asked = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen = asked.clone();
        let policy = ConflictPolicy::Ask(Box::new(move |conflict| {
            seen.lock().unwrap().push(conflict.clone());
            Resolution::Replace
        }));
        let done = run(&ctx, &plan, false, &policy).unwrap();
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            "old",
            "the trashed one is back"
        );
        let displaced = done[0].displaced.clone().expect("the newer one was kept");
        assert_eq!(fs::read_to_string(&displaced.in_trash).unwrap(), "newer");
        assert_eq!(
            *asked.lock().unwrap(),
            [Conflict {
                source: items[0].in_trash.clone(),
                existing: file.clone()
            }]
        );
        // And Undo of the replace: the restored one goes out again, the
        // displaced one comes back.
        let again = trash_in(&ctx, &[file.clone()], Copying::Small).unwrap();
        assert_eq!(fs::read_to_string(&again[0].in_trash).unwrap(), "old");
        super::super::trash::untrash(&[displaced]).unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "newer");
    }

    #[test]
    fn restore_to_a_chosen_folder_uses_the_original_name_not_the_numbered_one() {
        let temp = Temp::new("restore-into");
        let first = temp.write("home/one/a.txt", "1");
        let second = temp.write("home/two/a.txt", "2");
        let (ctx, items) = binned(&temp, &[&first, &second]);
        assert_eq!(items[1].in_trash.file_name().unwrap(), "a.2.txt");
        let elsewhere = temp.dir("home/elsewhere");
        let plan = plan_of(&ctx, &items[1..], Some(&elsewhere));
        assert_eq!(
            plan.entries[0].target.as_deref(),
            Some(elsewhere.join("a.txt").as_path())
        );
        assert!(plan.entries[0].missing.is_empty());
        let done = run(&ctx, &plan, false, &ConflictPolicy::Skip).unwrap();
        assert_eq!(done[0].to, elsewhere.join("a.txt"));
        assert_eq!(fs::read_to_string(elsewhere.join("a.txt")).unwrap(), "2");
        assert!(!items[1].info.exists());
        assert!(
            items[0].in_trash.exists(),
            "the other item stays in the Trash"
        );
    }

    #[test]
    fn an_item_with_no_record_can_only_go_to_a_chosen_folder() {
        let temp = Temp::new("restore-orphan");
        let ctx = context(&temp, &[]);
        let files = ctx.home_trash.join("files");
        fs::create_dir_all(&files).unwrap();
        fs::write(files.join("stray.txt"), "s").unwrap();
        fs::write(files.join("broken.txt"), "b").unwrap();
        fs::create_dir_all(ctx.home_trash.join("info")).unwrap();
        fs::write(
            ctx.home_trash.join("info/broken.txt.trashinfo"),
            "[Trash Info]\nDeletionDate=2024-01-01T00:00:00\n",
        )
        .unwrap();
        let paths = vec![files.join("stray.txt"), files.join("broken.txt")];
        let plan = plan_restore_in(Some(&ctx.home_trash), &paths, None);
        assert_eq!(
            plan.unknown().len(),
            2,
            "no info and no path both mean unknown"
        );
        assert!(plan.without_unknown().entries.is_empty());
        let failure = run(&ctx, &plan, false, &ConflictPolicy::Skip).unwrap_err();
        let OpError::Other(text) = failure.error else {
            panic!()
        };
        assert!(
            text.contains("no record") && text.contains("Restore to"),
            "{text}"
        );
        let into = temp.dir("home/chosen");
        let plan = plan_restore_in(Some(&ctx.home_trash), &paths, Some(&into));
        assert!(plan.unknown().is_empty());
        run(&ctx, &plan, false, &ConflictPolicy::Skip).unwrap();
        assert!(into.join("stray.txt").exists() && into.join("broken.txt").exists());
    }

    #[test]
    fn what_is_not_an_item_is_not_planned() {
        let temp = Temp::new("restore-inside");
        let folder = temp.dir("home/proj");
        temp.write("home/proj/inner.txt", "i");
        let (ctx, items) = binned(&temp, &[&folder]);
        let inner = items[0].in_trash.join("inner.txt");
        let plan = plan_restore_in(
            Some(&ctx.home_trash),
            &[inner, temp.path().join("home/elsewhere")],
            None,
        );
        assert!(plan.entries.is_empty());
    }

    #[test]
    fn names_that_are_not_utf8_come_back_exactly_and_keep_both_works() {
        let temp = Temp::new("restore-bytes");
        let name = OsString::from_vec(b"caf\xe9 \xff.txt".to_vec());
        let file = temp.path().join("home").join(&name);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "x").unwrap();
        let (ctx, items) = binned(&temp, &[&file]);
        fs::write(&file, "newer").unwrap();
        let plan = plan_of(&ctx, &items, None);
        let done = run(&ctx, &plan, false, &ConflictPolicy::KeepBoth).unwrap();
        let expected = OsString::from_vec(b"caf\xe9 \xff (copy).txt".to_vec());
        assert_eq!(done[0].to.file_name().unwrap(), expected.as_os_str());
        assert_eq!(fs::read_to_string(&done[0].to).unwrap(), "x");
        assert_eq!(fs::read_to_string(&file).unwrap(), "newer");
    }

    #[test]
    fn a_failure_part_way_keeps_and_reports_what_was_restored() {
        let temp = Temp::new("restore-partial");
        let first = temp.write("home/a/one.txt", "1");
        let second = temp.write("home/b/two.txt", "2");
        let (ctx, items) = binned(&temp, &[&first, &second]);
        // The second one's folder becomes a plain file.
        fs::remove_dir_all(temp.path().join("home/b")).unwrap();
        fs::write(temp.path().join("home/b"), "in the way").unwrap();
        let plan = plan_of(&ctx, &items, None);
        let failure = run(&ctx, &plan, true, &ConflictPolicy::Skip).unwrap_err();
        assert_eq!(failure.done.len(), 1);
        assert_eq!(failure.done[0].to, first);
        let OpError::Other(text) = &failure.error else {
            panic!("{:?}", failure.error)
        };
        assert!(text.contains("not a folder"), "{text}");
        assert!(first.exists() && items[1].in_trash.exists());
    }

    #[test]
    fn a_folder_that_cannot_be_written_is_a_clear_permission_error() {
        if unsafe { libc::getuid() } == 0 {
            return;
        }
        let temp = Temp::new("restore-perm");
        let file = temp.write("home/locked/a.txt", "a");
        let (ctx, items) = binned(&temp, &[&file]);
        let locked = temp.path().join("home/locked");
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).unwrap();
        let plan = plan_of(&ctx, &items, None);
        let failure = run(&ctx, &plan, false, &ConflictPolicy::Skip).unwrap_err();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        let OpError::Other(text) = &failure.error else {
            panic!("{:?}", failure.error)
        };
        assert!(
            text.contains("permission") && text.contains("locked") && text.contains("a.txt"),
            "{text}"
        );
        assert!(items[0].in_trash.exists(), "it stays in the Trash");
    }

    #[test]
    fn io_failures_are_worded_with_the_item_and_the_folder() {
        let target = Path::new("/mnt/usb/Photos/a.jpg");
        let text = |code: i32| match restore_io_error(io::Error::from_raw_os_error(code), target) {
            OpError::Other(text) => text,
            other => panic!("{other:?}"),
        };
        assert_eq!(
            text(libc::EROFS),
            "/mnt/usb/Photos is on a read-only drive, so “a.jpg” cannot be put back there."
        );
        assert_eq!(
            text(libc::EACCES),
            "You do not have permission to put “a.jpg” back in /mnt/usb/Photos."
        );
        assert_eq!(
            text(libc::ENOTDIR),
            "/mnt/usb/Photos is not a folder, so “a.jpg” cannot be put back there."
        );
        assert_eq!(
            text(libc::ENOENT),
            "The folder /mnt/usb/Photos no longer exists, so “a.jpg” cannot be put back."
        );
        assert_eq!(
            restore_io_error(io::Error::from_raw_os_error(libc::ENOSPC), target),
            OpError::Other("There is no space left on the drive.".into())
        );
    }

    #[test]
    fn a_move_between_drives_lands_under_the_original_name() {
        let temp = Temp::new("restore-xdev");
        let src = temp.write("bin/files/a.2.txt", "x");
        let to = temp.path().join("dest/a.txt");
        fs::create_dir_all(to.parent().unwrap()).unwrap();
        // The checked copy path, driven directly (one drive in a test).
        across_drives(&src, &to, &Progress::new(), &Cancel::new()).unwrap();
        assert_eq!(fs::read_to_string(&to).unwrap(), "x");
        assert!(!src.exists());
        // The numbered name is taken at the destination: still lands right.
        let src = temp.write("bin/files/b.2.txt", "y");
        temp.write("dest/b.2.txt", "taken");
        let to = temp.path().join("dest/b.txt");
        across_drives(&src, &to, &Progress::new(), &Cancel::new()).unwrap();
        assert_eq!(fs::read_to_string(&to).unwrap(), "y");
        assert_eq!(
            fs::read_to_string(temp.path().join("dest/b.2.txt")).unwrap(),
            "taken"
        );
    }

    #[test]
    fn cancel_stops_before_the_next_item() {
        let temp = Temp::new("restore-cancel");
        let file = temp.write("home/a.txt", "a");
        let (ctx, items) = binned(&temp, &[&file]);
        let plan = plan_of(&ctx, &items, None);
        let cancel = Cancel::new();
        cancel.cancel();
        let failure = restore_in(
            Some(&ctx),
            &plan,
            false,
            &ConflictPolicy::Skip,
            &Progress::new(),
            &cancel,
        )
        .unwrap_err();
        assert_eq!(failure.error, OpError::Cancelled);
        assert!(items[0].in_trash.exists());
    }
}
