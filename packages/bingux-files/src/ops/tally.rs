//! Adding up a folder in the background: how many items and how many bytes.
//!
//! A big folder takes seconds to count, so a dialog starts a [`Counting`] and
//! reads its [`Totals`] while it runs. The figures only grow. Dropping or
//! cancelling the [`Counting`] stops the walk after the entry it is on, so a
//! dialog that closes never leaves a worker crawling a drive.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::listing::Cancel;

/// What a count has found so far, shared between the walker and the reader.
#[derive(Debug, Default)]
pub struct Tally {
    bytes: AtomicU64,
    files: AtomicU64,
    folders: AtomicU64,
    unreadable: AtomicU64,
    done: AtomicBool,
    cancelled: AtomicBool,
}

/// A copy of a [`Tally`] at one moment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Totals {
    /// Bytes of every file under the roots, links counted as links.
    pub bytes: u64,
    /// Files, links and other non-folders.
    pub files: u64,
    /// Folders below the roots. A root folder is not one of its own items.
    pub folders: u64,
    /// Folders that could not be read, so the figures are "at least".
    pub unreadable: u64,
    /// The walk has ended, by finishing or by being cancelled.
    pub done: bool,
    /// The walk ended because it was cancelled: the figures are partial.
    pub cancelled: bool,
}

impl Totals {
    /// Files and folders together.
    pub fn items(&self) -> u64 {
        self.files + self.folders
    }

    /// The figures are not the whole answer: the walk is running, was
    /// stopped, or met a folder it could not read.
    pub fn is_lower_bound(&self) -> bool {
        !self.done || self.cancelled || self.unreadable > 0
    }
}

impl Tally {
    /// Count one file, link or device. Only regular files have a size: a
    /// link is its own few bytes of path, which is not what a person means.
    fn file(&self, meta: &fs::Metadata) {
        self.files.fetch_add(1, Ordering::Relaxed);
        if meta.is_file() {
            self.bytes.fetch_add(meta.len(), Ordering::Relaxed);
        }
    }

    pub fn totals(&self) -> Totals {
        Totals {
            // Read `done` first: whatever it says, the counts are at least as new.
            done: self.done.load(Ordering::Acquire),
            cancelled: self.cancelled.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
            files: self.files.load(Ordering::Relaxed),
            folders: self.folders.load(Ordering::Relaxed),
            unreadable: self.unreadable.load(Ordering::Relaxed),
        }
    }
}

/// Add up `roots` into `tally`, blocking until done or cancelled: a file
/// counts its size, a folder everything inside it. Links are counted as
/// links and never followed, so a loop cannot trap the walk. Root folders are
/// not counted as items of themselves.
pub fn count(roots: &[PathBuf], tally: &Tally, cancel: &Cancel) {
    let mut pending: Vec<(PathBuf, bool)> = roots.iter().map(|root| (root.clone(), true)).collect();
    'walk: while let Some((path, top)) = pending.pop() {
        if cancel.is_cancelled() {
            tally.cancelled.store(true, Ordering::Relaxed);
            break;
        }
        let Ok(meta) = fs::symlink_metadata(&path) else {
            tally.unreadable.fetch_add(1, Ordering::Relaxed);
            continue;
        };
        if !meta.is_dir() {
            tally.file(&meta);
            continue;
        }
        if !top {
            tally.folders.fetch_add(1, Ordering::Relaxed);
        }
        let Ok(children) = fs::read_dir(&path) else {
            tally.unreadable.fetch_add(1, Ordering::Relaxed);
            continue;
        };
        for child in children.flatten() {
            // A folder of a million files must stop within one entry, not
            // after it has been read to the end.
            if cancel.is_cancelled() {
                tally.cancelled.store(true, Ordering::Relaxed);
                break 'walk;
            }
            let Ok(meta) = child.metadata() else {
                tally.unreadable.fetch_add(1, Ordering::Relaxed);
                continue;
            };
            if meta.is_dir() {
                pending.push((child.path(), false));
            } else {
                tally.file(&meta);
            }
        }
    }
    tally.done.store(true, Ordering::Release);
}

/// A count running on its own thread. Dropping it cancels the walk.
#[derive(Debug)]
pub struct Counting {
    tally: Arc<Tally>,
    cancel: Cancel,
}

impl Counting {
    /// Start adding up `roots` on a new thread.
    pub fn start(roots: Vec<PathBuf>) -> Counting {
        let tally = Arc::new(Tally::default());
        let cancel = Cancel::new();
        {
            let (tally, cancel) = (tally.clone(), cancel.clone());
            std::thread::spawn(move || count(&roots, &tally, &cancel));
        }
        Counting { tally, cancel }
    }

    pub fn totals(&self) -> Totals {
        self.tally.totals()
    }

    /// Stop the walk. The figures so far stay readable.
    pub fn cancel(&self) {
        self.cancel.cancel();
    }
}

impl Drop for Counting {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::super::testing::Temp;
    use super::*;

    #[test]
    fn a_folder_is_added_up_without_following_links() {
        let temp = Temp::new("tally-folder");
        let outside = temp.write("outside/big.txt", "0123456789");
        let dir = temp.dir("d");
        temp.write("d/one", "123");
        temp.write("d/a/b/two", "12");
        std::os::unix::fs::symlink(outside.parent().unwrap(), dir.join("link")).unwrap();
        let tally = Tally::default();
        count(std::slice::from_ref(&dir), &tally, &Cancel::new());
        let totals = tally.totals();
        assert_eq!(totals.bytes, 5, "the link is not followed");
        // one, two and the link; folders a and a/b, not the root.
        assert_eq!((totals.files, totals.folders), (3, 2));
        assert!(totals.done && !totals.cancelled && !totals.is_lower_bound());
    }

    #[test]
    fn several_roots_add_up_together() {
        let temp = Temp::new("tally-roots");
        let file = temp.write("f", "1234");
        let dir = temp.dir("d");
        temp.write("d/x", "12");
        let tally = Tally::default();
        count(&[file, dir], &tally, &Cancel::new());
        assert_eq!(tally.totals().bytes, 6);
        assert_eq!(tally.totals().items(), 2);
    }

    #[test]
    fn a_cancelled_count_stops_at_once_and_says_so() {
        let temp = Temp::new("tally-cancel");
        temp.write("d/a", "x");
        let cancel = Cancel::new();
        cancel.cancel();
        let tally = Tally::default();
        count(&[temp.path().join("d")], &tally, &cancel);
        let totals = tally.totals();
        assert_eq!(totals.items(), 0);
        assert!(totals.done && totals.cancelled && totals.is_lower_bound());
    }

    #[test]
    fn a_running_count_stops_when_dropped() {
        let temp = Temp::new("tally-drop");
        for index in 0..200 {
            temp.write(&format!("d/{index}/f"), "x");
        }
        let counting = Counting::start(vec![temp.path().join("d")]);
        let tally = counting.tally.clone();
        counting.cancel();
        drop(counting);
        let started = Instant::now();
        while !tally.totals().done {
            assert!(
                started.elapsed() < Duration::from_secs(5),
                "the walker did not stop"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(tally.totals().items() <= 400);
    }

    #[test]
    fn a_missing_root_is_unreadable_not_a_crash() {
        let temp = Temp::new("tally-missing");
        let tally = Tally::default();
        count(&[temp.path().join("nope")], &tally, &Cancel::new());
        let totals = tally.totals();
        assert_eq!(totals.unreadable, 1);
        assert!(totals.is_lower_bound());
    }

    #[test]
    fn an_unreadable_folder_makes_the_figure_a_lower_bound() {
        use std::os::unix::fs::PermissionsExt;
        if unsafe { libc::getuid() } == 0 {
            eprintln!("skipped: root can read everything");
            return;
        }
        let temp = Temp::new("tally-locked");
        temp.write("d/a", "aa");
        let locked = temp.dir("d/locked");
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
        let tally = Tally::default();
        count(&[temp.path().join("d")], &tally, &Cancel::new());
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        let totals = tally.totals();
        assert_eq!(totals.unreadable, 1);
        assert!(totals.is_lower_bound());
        assert_eq!(totals.bytes, 2);
    }
}
