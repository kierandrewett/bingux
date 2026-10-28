//! What is in the bins as a whole, and taking things out of them for good:
//! counting for the question before Empty Trash, emptying, and deleting
//! chosen items. Nothing here asks; a caller **must** ask first.

use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;

use crate::listing::Cancel;
use crate::mounts;

use super::trash::{existing_bins, home_trash};
use super::transfer::delete;
use super::trash_info::{cached_dir_size, disk_use, forget_sizes, item_in};
use super::{OpError, Progress};

fn uid() -> u32 {
    // SAFETY: `getuid` has no preconditions and cannot fail.
    unsafe { libc::getuid() }
}

fn bins() -> Vec<PathBuf> {
    existing_bins(&mounts::all_mounts(), home_trash().as_deref(), uid())
}

/// What the bins hold, for the question before they are emptied.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TrashSummary {
    /// How many items: the entries a person sees in the Trash, not the files
    /// inside a folder.
    pub items: u64,
    /// The disk they use, in bytes: what emptying the Trash frees.
    pub bytes: u64,
}

/// Is there nothing in any bin? Reads the first entry of each `files` folder
/// and the mount table (cached for two seconds), so it is cheap enough for a
/// sidebar icon.
pub fn trash_is_empty() -> bool {
    bins_are_empty(&bins())
}

fn bins_are_empty(bins: &[PathBuf]) -> bool {
    !bins.iter().any(|bin| {
        fs::read_dir(bin.join("files")).is_ok_and(|mut entries| entries.next().is_some())
    })
}

/// How many items the bins hold and how much disk they use. A folder's size
/// comes from the bin's `directorysizes` cache when it is still right, else
/// the folder is walked. Stops with [`OpError::Cancelled`] when `cancel` is
/// set. For a huge Trash this takes a while: call it from a worker and let
/// the person cancel.
pub fn trash_summary(cancel: &Cancel) -> Result<TrashSummary, OpError> {
    summarize(&bins(), cancel)
}

fn summarize(bins: &[PathBuf], cancel: &Cancel) -> Result<TrashSummary, OpError> {
    let mut summary = TrashSummary::default();
    for bin in bins {
        let Ok(entries) = fs::read_dir(bin.join("files")) else {
            continue;
        };
        for entry in entries.flatten() {
            if cancel.is_cancelled() {
                return Err(OpError::Cancelled);
            }
            let Ok(meta) = entry.path().symlink_metadata() else {
                continue;
            };
            summary.items += 1;
            summary.bytes += if meta.is_dir() {
                match cached_dir_size(bin, &entry.file_name()) {
                    Some(size) => size,
                    None => disk_use(&entry.path(), cancel, None).ok_or(OpError::Cancelled)?,
                }
            } else {
                meta.blocks() * 512
            };
        }
    }
    Ok(summary)
}

/// Delete everything in every bin for good. This cannot be undone; ask
/// first. Progress counts what is removed, and [`Cancel`] stops it between
/// items. Nothing is done when the bins are empty.
pub fn empty_trash(progress: &Progress, cancel: &Cancel) -> Result<(), OpError> {
    empty_bins(&bins(), progress, cancel)
}

pub(super) fn empty_bins(bins: &[PathBuf], progress: &Progress, cancel: &Cancel) -> Result<(), OpError> {
    let mut doomed = Vec::new();
    for bin in bins {
        // The files first: an info file with no file is harmless, a file with
        // no info could not be put back.
        for part in ["files", "info"] {
            if let Ok(entries) = fs::read_dir(bin.join(part)) {
                doomed.extend(entries.flatten().map(|entry| entry.path()));
            }
        }
    }
    if doomed.is_empty() {
        return Ok(());
    }
    delete(&doomed, progress, cancel)?;
    // Every folder is gone, so the cache of their sizes goes too.
    for bin in bins {
        match fs::remove_file(bin.join("directorysizes")) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.into()),
            _ => {}
        }
    }
    Ok(())
}

/// Delete items of the bins for good: each item, its `.trashinfo` and its
/// line of `directorysizes`. A path inside a binned folder is deleted like
/// any other. Cannot be undone; ask first. Stops at the first error; what was
/// deleted stays deleted, and the info files of the items that are gone go
/// too, so no orphan is left that would number a later item's name.
pub fn delete_trashed(paths: &[PathBuf], progress: &Progress, cancel: &Cancel) -> Result<(), OpError> {
    delete_trashed_in(home_trash().as_deref(), paths, progress, cancel)
}

fn delete_trashed_in(
    home: Option<&std::path::Path>,
    paths: &[PathBuf],
    progress: &Progress,
    cancel: &Cancel,
) -> Result<(), OpError> {
    let items: Vec<_> = paths
        .iter()
        .filter_map(|path| item_in(path, home))
        .collect();
    let result = delete(paths, progress, cancel);
    let mut by_bin: Vec<(PathBuf, Vec<OsString>)> = Vec::new();
    for item in &items {
        let gone = item.in_trash.symlink_metadata().is_err();
        if !gone {
            continue;
        }
        let _ = fs::remove_file(&item.info);
        let name = item.name_in_trash().to_os_string();
        match by_bin.iter_mut().find(|(bin, _)| *bin == item.bin) {
            Some((_, names)) => names.push(name),
            None => by_bin.push((item.bin.clone(), vec![name])),
        }
    }
    for (bin, names) in &by_bin {
        let names: Vec<&std::ffi::OsStr> = names.iter().map(OsString::as_os_str).collect();
        forget_sizes(bin, &names);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::super::testing::Temp;
    use crate::mounts::Mount;
    use std::path::Path;
    use super::super::trash::tests::context;
    use super::super::trash::{Copying, trash_in};
    use super::*;

    #[test]
    fn the_bins_that_exist_are_found_on_every_drive() {
        let temp = Temp::new("bins");
        let home = temp.dir("home/Trash");
        temp.dir("home/Trash/files");
        temp.dir("usb/.Trash-1000/files");
        temp.dir("other/.Trash-1000/info"); // no files folder: not a bin
        // A shared bin that is not sticky is not used.
        temp.dir("shared/.Trash/1000/files");
        let table = mounts_at(&[
            &temp.path().join("usb"),
            &temp.path().join("other"),
            &temp.path().join("shared"),
        ]);
        let bins = existing_bins(&table, Some(&home), 1000);
        assert_eq!(bins, [home, temp.path().join("usb/.Trash-1000")]);
    }

    fn mounts_at(points: &[&Path]) -> Vec<Mount> {
        let text: Vec<String> = points
            .iter()
            .enumerate()
            .map(|(index, point)| {
                format!(
                    "{id} 1 0:{id} / {point} rw - ext4 /dev/disk{id} rw",
                    id = index + 10,
                    point = point.display()
                )
            })
            .collect();
        mounts::parse_mountinfo(&text.join("\n"))
    }

    #[test]
    fn emptying_deletes_files_and_info_in_every_bin_and_nothing_else() {
        let temp = Temp::new("empty");
        let bin_a = temp.dir("a/Trash");
        let bin_b = temp.dir("b/.Trash-1000");
        for bin in [&bin_a, &bin_b] {
            let relative = bin.strip_prefix(temp.path()).unwrap().display().to_string();
            temp.write(&format!("{relative}/files/x.txt"), "x");
            temp.write(&format!("{relative}/files/dir/y.txt"), "y");
            temp.write(&format!("{relative}/info/x.txt.trashinfo"), "i");
            temp.write(&format!("{relative}/directorysizes"), "4096 1 dir\n");
        }
        let outside = temp.write("keep/me.txt", "keep");
        let progress = Progress::new();
        empty_bins(&[bin_a.clone(), bin_b.clone()], &progress, &Cancel::new()).unwrap();
        for bin in [&bin_a, &bin_b] {
            assert_eq!(fs::read_dir(bin.join("files")).unwrap().count(), 0);
            assert_eq!(fs::read_dir(bin.join("info")).unwrap().count(), 0);
            assert!(!bin.join("directorysizes").exists(), "the size cache goes too");
        }
        assert!(bin_a.join("files").is_dir(), "the bin itself stays");
        assert_eq!(fs::read_to_string(outside).unwrap(), "keep");
        assert_eq!(progress.files_done(), 8);
    }

    #[test]
    fn emptying_an_empty_bin_is_not_an_error_and_cancel_is_obeyed() {
        let temp = Temp::new("empty-none");
        let bin = temp.dir("Trash");
        temp.dir("Trash/files");
        empty_bins(&[bin.clone()], &Progress::new(), &Cancel::new()).unwrap();
        temp.write("Trash/files/a.txt", "a");
        let cancel = Cancel::new();
        cancel.cancel();
        assert_eq!(
            empty_bins(&[bin.clone()], &Progress::new(), &cancel),
            Err(OpError::Cancelled)
        );
        assert!(bin.join("files/a.txt").exists());
    }

    #[test]
    fn the_summary_counts_items_not_the_files_inside_them() {
        let temp = Temp::new("summary");
        let ctx = context(&temp, &[]);
        let one = temp.write("home/a.txt", &"x".repeat(5000));
        let folder = temp.dir("home/proj");
        temp.write("home/proj/p1", &"y".repeat(9000));
        temp.write("home/proj/p2", "z");
        let link = temp.path().join("home/link");
        std::os::unix::fs::symlink(&one, &link).unwrap();
        trash_in(&ctx, &[one, folder, link], Copying::Small).unwrap();
        let summary = summarize(&[ctx.home_trash.clone()], &Cancel::new()).unwrap();
        assert_eq!(summary.items, 3, "a file, a folder and a link");
        assert!(summary.bytes >= 14_000, "the blocks of every file inside: {}", summary.bytes);
        assert_eq!(summarize(&[], &Cancel::new()), Ok(TrashSummary::default()));
        let cancel = Cancel::new();
        cancel.cancel();
        assert_eq!(summarize(&[ctx.home_trash.clone()], &cancel), Err(OpError::Cancelled));
    }

    #[test]
    fn the_summary_trusts_the_size_cache_only_while_it_is_right() {
        let temp = Temp::new("summary-cache");
        let ctx = context(&temp, &[]);
        let folder = temp.dir("home/proj");
        temp.write("home/proj/p1", &"y".repeat(9000));
        trash_in(&ctx, &[folder], Copying::Small).unwrap();
        let real = summarize(&[ctx.home_trash.clone()], &Cancel::new()).unwrap().bytes;
        // A cache line that claims a huge size is believed while the info
        // file has the time it records...
        let info = ctx.home_trash.join("info/proj.trashinfo");
        let mtime = fs::metadata(&info).unwrap().mtime();
        fs::write(ctx.home_trash.join("directorysizes"), format!("123456789 {mtime} proj\n")).unwrap();
        assert_eq!(summarize(&[ctx.home_trash.clone()], &Cancel::new()).unwrap().bytes, 123_456_789);
        // ...and not once the info file has changed.
        fs::write(ctx.home_trash.join("directorysizes"), format!("123456789 {} proj\n", mtime - 10)).unwrap();
        assert_eq!(summarize(&[ctx.home_trash.clone()], &Cancel::new()).unwrap().bytes, real);
    }

    #[test]
    fn deleting_an_item_takes_its_info_and_its_size_line_and_leaves_the_rest() {
        let temp = Temp::new("delete-trashed");
        let ctx = context(&temp, &[]);
        let folder = temp.dir("home/proj");
        temp.write("home/proj/p", "p");
        let keep = temp.write("home/keep.txt", "k");
        let gone = temp.write("home/gone.txt", "g");
        let items = trash_in(&ctx, &[folder, keep, gone], Copying::Small).unwrap();
        assert!(ctx.home_trash.join("directorysizes").exists());
        let doomed = vec![items[0].in_trash.clone(), items[2].in_trash.clone()];
        let progress = Progress::new();
        delete_trashed_in(Some(&ctx.home_trash), &doomed, &progress, &Cancel::new()).unwrap();
        for index in [0, 2] {
            assert!(!items[index].in_trash.exists() && !items[index].info.exists());
        }
        assert!(items[1].in_trash.exists() && items[1].info.exists(), "the other item is untouched");
        assert!(!ctx.home_trash.join("directorysizes").exists(), "the folder's line went");
        assert!(progress.files_done() >= 3);
    }

    #[test]
    fn a_path_inside_a_binned_folder_is_deleted_without_touching_the_info() {
        let temp = Temp::new("delete-inside");
        let ctx = context(&temp, &[]);
        let folder = temp.dir("home/proj");
        temp.write("home/proj/inner.txt", "i");
        let items = trash_in(&ctx, &[folder], Copying::Small).unwrap();
        let inner = items[0].in_trash.join("inner.txt");
        delete_trashed_in(Some(&ctx.home_trash), &[inner.clone()], &Progress::new(), &Cancel::new()).unwrap();
        assert!(!inner.exists());
        assert!(items[0].in_trash.exists() && items[0].info.exists());
    }

    #[test]
    fn a_cancelled_delete_keeps_the_info_of_what_is_still_there() {
        let temp = Temp::new("delete-cancel");
        let ctx = context(&temp, &[]);
        let file = temp.write("home/a.txt", "a");
        let items = trash_in(&ctx, &[file], Copying::Small).unwrap();
        let cancel = Cancel::new();
        cancel.cancel();
        let result = delete_trashed_in(Some(&ctx.home_trash), &[items[0].in_trash.clone()], &Progress::new(), &cancel);
        assert_eq!(result, Err(OpError::Cancelled));
        assert!(items[0].in_trash.exists() && items[0].info.exists(), "still restorable");
    }

    #[test]
    fn an_empty_bin_is_empty_and_one_item_is_not() {
        let temp = Temp::new("is-empty");
        let ctx = context(&temp, &[]);
        assert!(bins_are_empty(&[]), "no bin at all");
        let file = temp.write("home/a.txt", "a");
        trash_in(&ctx, &[file], Copying::Small).unwrap();
        assert!(!bins_are_empty(&[ctx.home_trash.clone()]));
        empty_bins(&[ctx.home_trash.clone()], &Progress::new(), &Cancel::new()).unwrap();
        assert!(bins_are_empty(&[ctx.home_trash.clone()]));
    }
}
