//! What a person does to files: rename, create, bin, delete, copy, move,
//! compress, extract, and read properties.
//!
//! Every function here **blocks**. Call them from a worker thread. Long
//! operations take a [`Progress`] to poll (every 100 ms is cheap) and a
//! [`Cancel`](crate::Cancel) to stop them. Nothing here overwrites a file
//! unless the caller asked for [`Resolution::Replace`].
//!
//! | Area | Functions |
//! | ---- | --------- |
//! | Names | [`rename`], [`create_file`], [`create_dir`], [`keep_both_name`] |
//! | The bin | [`can_trash`], [`trash`], [`untrash`], [`in_trash`] |
//! | Permanent (ask first) | [`delete`], [`empty_trash`] |
//! | Transfers | [`copy`], [`move_to`], [`ConflictPolicy`] |
//! | Archives | [`compress`], [`extract`], [`Format`] |
//! | Properties | [`properties`], [`stat_properties`], [`permissions`], [`chmod`], [`chmod_tree`] (apply to enclosed items), [`Counting`] (a folder's size and its biggest children, in the background), [`Hashing`] (a file's SHA-256, in the background), [`file_details`] (inode, links, disk use, extended attributes), [`user_name`], [`group_name`] |
//! | Clipboard | [`uri_of`], [`parse_uri_list`], [`gnome_copied_files`], [`parse_gnome_copied_files`] |
//!
//! # What they promise
//!
//! - A rename, and a move on one drive, is one `renameat2(RENAME_NOREPLACE)`.
//! - A move between drives removes the source only after the copy is durable
//!   and checked.
//! - A copy writes a part file and renames it into place, so a half-written
//!   file never has its real name; reflinks, `copy_file_range` and sparse
//!   files keep it fast.
//! - On removable media, shares, FAT, exFAT and NTFS, "done" means on the
//!   device: files, folders and the drive are flushed first, and
//!   [`Progress::finishing`] names the drive meanwhile.
//! - [`delete`] and [`empty_trash`] cannot be undone. A caller **must** ask the
//!   person first; nothing in this module does.
//!
//! The details are in the documentation of [`copy`] and [`move_to`].
//!
//! # Copying with a question
//!
//! ```no_run
//! use std::path::{Path, PathBuf};
//! use bingux_files::{Cancel, ops::{self, ConflictPolicy, Progress, Resolution}};
//!
//! let progress = Progress::new();
//! let policy = ConflictPolicy::Ask(Box::new(|conflict| {
//!     println!("{} already exists", conflict.existing.display());
//!     Resolution::KeepBoth
//! }));
//! let done = ops::copy(
//!     &[PathBuf::from("/home/me/a.txt")],
//!     Path::new("/home/me/Documents"),
//!     &policy,
//!     &progress,
//!     &Cancel::new(),
//! )?;
//! # Ok::<(), ops::OpError>(())
//! ```

use std::ffi::CString;
use std::fmt;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::location::NameError;

mod archive;
mod clip;
mod data;
mod details;
mod digest;
mod durable;
mod names;
mod props;
mod tally;
mod transfer;
mod restore;
mod trash;
mod unpack;
mod trash_empty;
mod trash_info;
mod pack;
mod pack_plan;
mod media_plan;
pub mod prefs;
mod shrink;
pub mod tool;

pub use archive::{can_extract, extract_available};
pub use unpack::{
    Inspection, Overwrite, Unpacked, UnpackOptions, check_password, extract, inspect, unpack,
};
pub use media_plan::{
    AUDIO_KBPS, AudioCodec, AudioOptions, DIMENSIONS, FRAME_RATES, HEIGHTS, ImageFormat, ImageOptions,
    MediaKind, Probe, ReduceOptions, SOUND_KBPS, Sound, Tools, VideoCodec, VideoOptions, VideoQuality,
    free_reduced_name, image_format_of, reduced_name,
};
pub use shrink::{
    Batch, Estimate, FileReport, Live, Outcome, ReduceJob, Reduced, Stage, all_media, estimate,
    kind_of_name, parallelism, reduce,
};
pub use pack::{ItemKind, OnConflict, Packed, PackJob, Scan, pack, parent_of, scan};
pub use pack_plan::{
    Excludes, Format, Level, NAME_LIMIT, NameCheck, PackOptions, PatternError, Password,
    SPLIT_SIZES_MIB, base_of, check_name, file_name, free_base, split_label, suggested_base,
};
pub use clip::{gnome_copied_files, parse_gnome_copied_files, parse_uri_list, uri_of};
pub use details::{FileDetails, Xattr, file_details, xattrs};
pub use digest::{HashOutcome, HashProgress, Hashing, Sha256};
pub use names::{create_dir, create_file, keep_both_name, rename};
pub use props::{
    Permissions, Properties, TreeChange, Volume, chmod, chmod_tree, group_name, permissions, properties,
    stat_properties, tree_mode, user_name,
};
pub use tally::{ChildSize, Counting, Tally, Totals, count};
pub use transfer::{Conflict, ConflictPolicy, Resolution, copy, delete, move_to};
pub use restore::{RestoreEntry, RestoreFailure, RestorePlan, Restored, plan_restore, restore};
pub use trash::{
    Trashed, TrashRefusal, can_trash, check_trash, in_trash, trash, trash_copying, untrash,
};
pub(crate) use trash_info::BinFolder;
pub use trash_empty::{
    TrashSummary, delete_trashed, empty_trash, trash_is_empty, trash_summary,
};
pub use trash_info::{InBin, TrashItem, cached_dir_size, trash_item, trash_items, trash_place};

/// Why an operation did not happen. Each one reads as a plain sentence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpError {
    /// Something with that name is already there.
    Exists,
    /// The system said no.
    Permission,
    /// The file or folder is gone.
    NotFound,
    /// The name cannot be used.
    Invalid(NameError),
    /// The caller stopped it.
    Cancelled,
    /// This machine or drive cannot do it (a missing tool, a drive with no bin).
    Unsupported(String),
    /// Anything else, already worded for a person.
    Other(String),
    /// It cannot go to the bin, and why (see [`TrashRefusal`]).
    CannotTrash(TrashRefusal),
    /// Binning it takes a copy between two drives, and it is big: ask first,
    /// then use [`trash_copying`].
    CopyNeeded,
}

impl fmt::Display for OpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpError::Exists => f.write_str("Something with that name already exists."),
            OpError::Permission => f.write_str("You do not have permission to do that."),
            OpError::NotFound => f.write_str("That file or folder no longer exists."),
            OpError::Invalid(error) => error.fmt(f),
            OpError::Cancelled => f.write_str("The operation was cancelled."),
            OpError::Unsupported(text) | OpError::Other(text) => f.write_str(text),
            OpError::CannotTrash(refusal) => refusal.fmt(f),
            OpError::CopyNeeded => f.write_str(
                "Moving it to the Trash means copying it to another place first, which can take a long time.",
            ),
        }
    }
}

impl std::error::Error for OpError {}

impl From<NameError> for OpError {
    fn from(error: NameError) -> Self {
        OpError::Invalid(error)
    }
}

impl From<io::Error> for OpError {
    fn from(error: io::Error) -> Self {
        match error.kind() {
            io::ErrorKind::NotFound => return OpError::NotFound,
            io::ErrorKind::AlreadyExists => return OpError::Exists,
            io::ErrorKind::PermissionDenied => return OpError::Permission,
            _ => {}
        }
        match error.raw_os_error() {
            Some(libc::ENOSPC) => OpError::Other("There is no space left on the drive.".into()),
            Some(libc::EROFS) => OpError::Other("The drive is read-only.".into()),
            Some(libc::EXDEV) => {
                OpError::Unsupported("That cannot be done between two drives in one step.".into())
            }
            Some(libc::ENAMETOOLONG) => OpError::Invalid(NameError::TooLong),
            Some(libc::ENOTEMPTY) => OpError::Other("The folder is not empty.".into()),
            Some(libc::EBUSY) => OpError::Other("The file or folder is in use.".into()),
            Some(libc::EDQUOT) => OpError::Other("Your storage quota is full.".into()),
            _ => OpError::Other(plain(&error)),
        }
    }
}

/// An I/O error as a sentence: capitalised, no `(os error N)`, a full stop.
fn plain(error: &io::Error) -> String {
    let text = error.to_string();
    let text = match text.find(" (os error") {
        Some(index) => &text[..index],
        None => &text,
    };
    let mut chars = text.chars();
    let mut out: String = match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::from("The operation failed"),
    };
    if !out.ends_with('.') {
        out.push('.');
    }
    out
}

/// How far a long operation has got. Clones share the counters, so give one
/// clone to the worker and keep another to poll.
///
/// For [`copy`] and [`delete`] the totals are counted by a scan that runs
/// beside the work, so they grow at first: [`fraction`](Progress::fraction)
/// is `None` until the scan has finished.
#[derive(Clone, Debug, Default)]
pub struct Progress(Arc<ProgressInner>);

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[derive(Debug, Default)]
struct ProgressInner {
    bytes_done: AtomicU64,
    bytes_total: AtomicU64,
    files_done: AtomicU64,
    files_total: AtomicU64,
    scanning: AtomicBool,
    current: Mutex<String>,
    /// The drive being written out at the end of an operation.
    finishing: Mutex<Option<String>>,
    /// The drive an operation has finished writing out.
    flushed: Mutex<Option<String>>,
}

impl Progress {
    pub fn new() -> Progress {
        Progress::default()
    }

    pub fn bytes_done(&self) -> u64 {
        self.0.bytes_done.load(Ordering::Relaxed)
    }

    pub fn bytes_total(&self) -> u64 {
        self.0.bytes_total.load(Ordering::Relaxed)
    }

    /// Files, links and folders handled so far.
    pub fn files_done(&self) -> u64 {
        self.0.files_done.load(Ordering::Relaxed)
    }

    pub fn files_total(&self) -> u64 {
        self.0.files_total.load(Ordering::Relaxed)
    }

    /// The name of the item being handled, or empty.
    pub fn current(&self) -> String {
        self.0
            .current
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// The totals are still being counted.
    pub fn is_scanning(&self) -> bool {
        self.0.scanning.load(Ordering::Acquire)
    }

    /// Done, from 0 to 1: by bytes when there are any, else by items. `None`
    /// while the totals are still being counted or when nothing is known.
    pub fn fraction(&self) -> Option<f32> {
        if self.is_scanning() {
            return None;
        }
        let (done, total) = if self.bytes_total() > 0 {
            (self.bytes_done(), self.bytes_total())
        } else {
            (self.files_done(), self.files_total())
        };
        (total > 0).then(|| (done as f64 / total as f64).clamp(0.0, 1.0) as f32)
    }

    pub(crate) fn add_total(&self, bytes: u64, files: u64) {
        self.0.bytes_total.fetch_add(bytes, Ordering::Relaxed);
        self.0.files_total.fetch_add(files, Ordering::Relaxed);
    }

    pub(crate) fn add_bytes(&self, bytes: u64) {
        self.0.bytes_done.fetch_add(bytes, Ordering::Relaxed);
    }

    pub(crate) fn add_files(&self, files: u64) {
        self.0.files_done.fetch_add(files, Ordering::Relaxed);
    }

    /// One item that took no time (a rename): count it in both columns.
    pub(crate) fn count_instant(&self) {
        self.add_total(0, 1);
        self.add_files(1);
    }

    /// The work is over: a total that was still being counted cannot be
    /// smaller than what was done.
    pub(crate) fn settle(&self) {
        self.0
            .bytes_total
            .fetch_max(self.bytes_done(), Ordering::Relaxed);
        self.0
            .files_total
            .fetch_max(self.files_done(), Ordering::Relaxed);
    }

    pub(crate) fn set_scanning(&self, scanning: bool) {
        self.0.scanning.store(scanning, Ordering::Release);
    }

    /// "Finishing": the work is done and the drive named is being written
    /// out. `None` while the data is still being copied, and again once the
    /// drive has it.
    pub fn finishing(&self) -> Option<String> {
        lock(&self.0.finishing).clone()
    }

    /// The drive the operation wrote out for good, by the time it is over.
    /// Set only for removable media, shares and the like (see
    /// [`mounts::write_target`](crate::mounts::write_target)): on those,
    /// "done" means the data is on the device.
    pub fn flushed_to(&self) -> Option<String> {
        lock(&self.0.flushed).clone()
    }

    pub(crate) fn set_finishing(&self, drive: Option<&str>) {
        *lock(&self.0.finishing) = drive.map(str::to_owned);
    }

    pub(crate) fn set_flushed(&self, drive: &str) {
        *lock(&self.0.flushed) = Some(drive.to_owned());
    }

    pub(crate) fn set_current(&self, name: &str) {
        let mut current = self
            .0
            .current
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        current.clear();
        current.push_str(name);
    }
}

fn c_path(path: &Path) -> io::Result<CString> {
    CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains a NUL byte"))
}

/// `rename(2)` that fails with `AlreadyExists` instead of replacing `to`.
///
/// Uses `renameat2(RENAME_NOREPLACE)`. Where the file system or kernel does
/// not know it (NFS, old kernels) the fallback checks first and renames
/// second, which leaves a window of a few microseconds.
pub(crate) fn rename_noreplace(from: &Path, to: &Path) -> io::Result<()> {
    let from_c = c_path(from)?;
    let to_c = c_path(to)?;
    // SAFETY: both strings are NUL-terminated and outlive the call.
    let result = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            libc::AT_FDCWD,
            from_c.as_ptr(),
            libc::AT_FDCWD,
            to_c.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if result == 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    match error.raw_os_error() {
        Some(libc::ENOSYS | libc::EINVAL | libc::ENOTSUP) => {
            if to.symlink_metadata().is_ok() {
                Err(io::ErrorKind::AlreadyExists.into())
            } else {
                std::fs::rename(from, to)
            }
        }
        _ => Err(error),
    }
}

/// A scratch directory for tests, removed on drop.
#[cfg(test)]
pub(crate) mod testing {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU32, Ordering};

    pub struct Temp(pub PathBuf);

    impl Temp {
        pub fn new(tag: &str) -> Temp {
            Temp::in_dir(&std::env::temp_dir(), tag)
        }

        pub fn in_dir(parent: &Path, tag: &str) -> Temp {
            static COUNTER: AtomicU32 = AtomicU32::new(0);
            let path = parent.join(format!(
                "bingux-files-ops-{}-{}-{tag}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Temp(path)
        }

        pub fn path(&self) -> &Path {
            &self.0
        }

        pub fn write(&self, relative: &str, text: &str) -> PathBuf {
            let path = self.0.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, text).unwrap();
            path
        }

        pub fn dir(&self, relative: &str) -> PathBuf {
            let path = self.0.join(relative);
            fs::create_dir_all(&path).unwrap();
            path
        }
    }

    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// Run `work` with the Trash pointed at `data`. The environment is the
    /// process's, so the tests that change it take turns.
    pub fn with_data_home<T>(data: &Path, work: impl FnOnce() -> T) -> T {
        static TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _turn = TURN.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let previous = std::env::var_os("XDG_DATA_HOME");
        // SAFETY: the lock above keeps the tests that set it apart, and no
        // other test in this crate reads it.
        unsafe { std::env::set_var("XDG_DATA_HOME", data) };
        let outcome = work();
        match previous {
            // SAFETY: as above.
            Some(value) => unsafe { std::env::set_var("XDG_DATA_HOME", value) },
            None => unsafe { std::env::remove_var("XDG_DATA_HOME") },
        }
        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_errors_map_to_plain_variants() {
        let not_found = io::Error::from(io::ErrorKind::NotFound);
        assert_eq!(OpError::from(not_found), OpError::NotFound);
        let exists = io::Error::from(io::ErrorKind::AlreadyExists);
        assert_eq!(OpError::from(exists), OpError::Exists);
        let denied = io::Error::from_raw_os_error(libc::EACCES);
        assert_eq!(OpError::from(denied), OpError::Permission);
        let full = io::Error::from_raw_os_error(libc::ENOSPC);
        assert_eq!(
            OpError::from(full).to_string(),
            "There is no space left on the drive."
        );
        let long = io::Error::from_raw_os_error(libc::ENAMETOOLONG);
        assert_eq!(OpError::from(long), OpError::Invalid(NameError::TooLong));
    }

    #[test]
    fn other_errors_read_as_a_sentence() {
        let error = io::Error::from_raw_os_error(libc::EIO);
        let text = OpError::from(error).to_string();
        assert!(!text.contains("os error"), "{text}");
        assert!(text.ends_with('.'), "{text}");
        assert!(text.chars().next().unwrap().is_uppercase(), "{text}");
    }

    #[test]
    fn progress_fraction_waits_for_the_scan() {
        let progress = Progress::new();
        assert_eq!(progress.fraction(), None);
        progress.set_scanning(true);
        progress.add_total(200, 4);
        progress.add_bytes(50);
        assert_eq!(progress.fraction(), None, "still counting");
        progress.set_scanning(false);
        assert_eq!(progress.fraction(), Some(0.25));
        progress.add_bytes(500);
        assert_eq!(progress.fraction(), Some(1.0), "never above one");
    }

    #[test]
    fn progress_falls_back_to_item_counts() {
        let progress = Progress::new();
        progress.add_total(0, 4);
        progress.add_files(1);
        assert_eq!(progress.fraction(), Some(0.25));
        let clone = progress.clone();
        clone.set_current("a.txt");
        assert_eq!(progress.current(), "a.txt");
        assert_eq!(progress.files_done(), 1);
        assert_eq!(progress.files_total(), 4);
    }

    #[test]
    fn rename_noreplace_refuses_to_replace() {
        let temp = testing::Temp::new("noreplace");
        let a = temp.write("a", "first");
        let b = temp.write("b", "second");
        let error = rename_noreplace(&a, &b).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read_to_string(&b).unwrap(), "second");
        assert!(a.exists());
        let c = temp.path().join("c");
        rename_noreplace(&a, &c).unwrap();
        assert_eq!(std::fs::read_to_string(&c).unwrap(), "first");
    }
}
