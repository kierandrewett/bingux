//! Noticing that a file was changed by someone else.
//!
//! [`FileWatch`] watches one file with inotify (through the `notify` crate)
//! and calls a function when the file on disk is no longer what the document
//! last read or wrote: another program saved it, replaced it, moved it away
//! or deleted it.
//!
//! - **The folder is watched, not the file**, because most programs save by
//!   writing a new file and renaming it over the old one, which leaves a
//!   watch on the old inode watching nothing.
//! - **Changes are judged by the file, not by the event.** A burst of events
//!   (a program writing a file in pieces) is waited out (120 ms of quiet),
//!   then the file's [`Stamp`] is compared with the one the document knows.
//!   Equal means nothing changed that matters and nothing is reported.
//! - **The document's own saves are not reported**: after writing, the
//!   document gives the stamp of what it wrote to [`FileWatch::acknowledge`].
//! - Each change is reported once. What was reported is the new state, and
//!   the next report is of a change from it.
//!
//! Where inotify cannot be used (the limit of watches is reached, or a
//! network share sends no events) [`FileWatch::new`] fails or events never
//! come; a document therefore also compares stamps when its window is focused
//! again, which costs one `stat`.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use notify::{RecommendedWatcher, RecursiveMode, Watcher};

use super::stamp::Stamp;

/// How long the events of one change are waited for.
const QUIET: Duration = Duration::from_millis(120);
/// The longest a steady stream of events is waited out.
const LONGEST: Duration = Duration::from_secs(1);

/// What happened to the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileChange {
    /// The file has other contents than the document knows (or is a
    /// different file under the same name).
    Changed,
    /// The file is gone: deleted, or moved away.
    Removed,
}

/// A watch on one file. Dropping it ends the watch.
pub struct FileWatch {
    path: PathBuf,
    known: Arc<Mutex<Option<Stamp>>>,
    _watcher: RecommendedWatcher,
}

impl FileWatch {
    /// Watches `path`, whose stamp the document knows to be `known` (`None`:
    /// it did not exist). `on_change` is called on a thread of its own, once
    /// per change; send what it gets to the UI (a [`crate::task::Feed`]).
    pub fn new(
        path: &Path,
        known: Option<Stamp>,
        on_change: impl Fn(FileChange) + Send + 'static,
    ) -> notify::Result<FileWatch> {
        // A link is followed: saves are written through it, and a change to
        // the target is a change to the file.
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let folder = path
            .parent()
            .unwrap_or_else(|| Path::new("/"))
            .to_path_buf();
        let name = path.file_name().map(std::ffi::OsStr::to_os_string);
        let (events, quiet) = mpsc::channel::<()>();
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                if let Ok(event) = event
                    && !event.kind.is_access()
                    && event
                        .paths
                        .iter()
                        .any(|path| path.file_name() == name.as_deref())
                {
                    let _ = events.send(());
                }
            })?;
        watcher.watch(&folder, RecursiveMode::NonRecursive)?;
        let known = Arc::new(Mutex::new(known));
        let shared = Arc::clone(&known);
        let watched = path.clone();
        std::thread::Builder::new()
            .name("bingux-file-watch".into())
            .spawn(move || {
                // The sender lives in the watcher's callback: when the watch
                // is dropped the channel closes and the thread ends.
                while quiet.recv().is_ok() {
                    let started = Instant::now();
                    loop {
                        match quiet.recv_timeout(QUIET) {
                            Ok(()) if started.elapsed() < LONGEST => {}
                            Ok(()) | Err(RecvTimeoutError::Timeout) => break,
                            Err(RecvTimeoutError::Disconnected) => return,
                        }
                    }
                    let now = Stamp::now(&watched);
                    let changed = {
                        let mut known = shared.lock().unwrap_or_else(PoisonError::into_inner);
                        if *known == now {
                            false
                        } else {
                            *known = now;
                            true
                        }
                    };
                    if changed {
                        on_change(if now.is_some() {
                            FileChange::Changed
                        } else {
                            FileChange::Removed
                        });
                    }
                }
            })
            .map_err(notify::Error::io)?;
        Ok(FileWatch {
            path,
            known,
            _watcher: watcher,
        })
    }

    /// The file watched (with links followed).
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Tells the watch what the document now knows the file to be: after a
    /// save (the stamp [`write_atomic`](super::write_atomic) returned), a
    /// reload, or when the person chose to keep their version of a file that
    /// changed.
    pub fn acknowledge(&self, stamp: Option<Stamp>) {
        *self.known.lock().unwrap_or_else(PoisonError::into_inner) = stamp;
    }

    /// What the watch believes the file is.
    pub fn known(&self) -> Option<Stamp> {
        *self.known.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::write_atomic;
    use std::sync::mpsc::Receiver;

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bingux-watch-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn watch(path: &Path) -> (FileWatch, Receiver<FileChange>) {
        let (send, receive) = mpsc::channel();
        let watch = FileWatch::new(path, Stamp::now(path), move |change| {
            let _ = send.send(change);
        })
        .unwrap();
        (watch, receive)
    }

    const WAIT: Duration = Duration::from_secs(5);
    const NOTHING: Duration = Duration::from_millis(600);

    #[test]
    fn another_programs_write_is_reported_once() {
        let dir = folder("write");
        let path = dir.join("a.txt");
        std::fs::write(&path, "one").unwrap();
        let (_watch, changes) = watch(&path);
        // A program that writes in several steps is one change.
        for chunk in ["two", "three", "four"] {
            std::fs::write(&path, chunk).unwrap();
        }
        assert_eq!(changes.recv_timeout(WAIT), Ok(FileChange::Changed));
        assert_eq!(
            changes.recv_timeout(NOTHING),
            Err(RecvTimeoutError::Timeout)
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_save_by_rename_over_the_file_is_reported() {
        let dir = folder("rename");
        let path = dir.join("a.txt");
        std::fs::write(&path, "one").unwrap();
        let (_watch, changes) = watch(&path);
        let other = dir.join("a.txt.tmp");
        std::fs::write(&other, "something longer").unwrap();
        std::fs::rename(&other, &path).unwrap();
        assert_eq!(changes.recv_timeout(WAIT), Ok(FileChange::Changed));
        // And a second save by rename is reported again.
        std::fs::write(&other, "again, longer still").unwrap();
        std::fs::rename(&other, &path).unwrap();
        assert_eq!(changes.recv_timeout(WAIT), Ok(FileChange::Changed));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn deleting_and_moving_away_are_removal_and_coming_back_is_a_change() {
        let dir = folder("remove");
        let path = dir.join("a.txt");
        std::fs::write(&path, "one").unwrap();
        let (_watch, changes) = watch(&path);
        std::fs::rename(&path, dir.join("moved.txt")).unwrap();
        assert_eq!(changes.recv_timeout(WAIT), Ok(FileChange::Removed));
        std::fs::write(&path, "back").unwrap();
        assert_eq!(changes.recv_timeout(WAIT), Ok(FileChange::Changed));
        std::fs::remove_file(&path).unwrap();
        assert_eq!(changes.recv_timeout(WAIT), Ok(FileChange::Removed));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_documents_own_save_is_not_reported() {
        let dir = folder("own");
        let path = dir.join("a.txt");
        std::fs::write(&path, "one").unwrap();
        let (watch, changes) = watch(&path);
        let stamp = write_atomic(&path, b"saved by the document").unwrap();
        watch.acknowledge(Some(stamp));
        assert_eq!(
            changes.recv_timeout(NOTHING),
            Err(RecvTimeoutError::Timeout)
        );
        assert_eq!(watch.known(), Some(stamp));
        // Someone else's change after it is.
        std::fs::write(&path, "and then someone else").unwrap();
        assert_eq!(changes.recv_timeout(WAIT), Ok(FileChange::Changed));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn keeping_ones_version_makes_the_changed_file_the_new_normal() {
        let dir = folder("keep");
        let path = dir.join("a.txt");
        std::fs::write(&path, "one").unwrap();
        let (watch, changes) = watch(&path);
        std::fs::write(&path, "theirs").unwrap();
        assert_eq!(changes.recv_timeout(WAIT), Ok(FileChange::Changed));
        // The change was reported once; nothing more comes until another.
        assert_eq!(
            changes.recv_timeout(NOTHING),
            Err(RecvTimeoutError::Timeout)
        );
        watch.acknowledge(Stamp::now(&path));
        std::fs::write(&path, "theirs again, longer").unwrap();
        assert_eq!(changes.recv_timeout(WAIT), Ok(FileChange::Changed));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn other_files_in_the_folder_and_reads_are_not_the_file() {
        let dir = folder("others");
        let path = dir.join("a.txt");
        std::fs::write(&path, "one").unwrap();
        let (_watch, changes) = watch(&path);
        std::fs::write(dir.join("b.txt"), "other").unwrap();
        let _ = std::fs::read(&path).unwrap();
        // Touching the file without changing it: same stamp or not, a write
        // of the same bytes in the same instant is not a change we can see.
        assert_eq!(
            changes.recv_timeout(NOTHING),
            Err(RecvTimeoutError::Timeout)
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn dropping_the_watch_ends_its_thread_and_reports_nothing_more() {
        let dir = folder("drop");
        let path = dir.join("a.txt");
        std::fs::write(&path, "one").unwrap();
        let (watch, changes) = watch(&path);
        drop(watch);
        std::fs::write(&path, "two").unwrap();
        assert!(matches!(
            changes.recv_timeout(NOTHING),
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected)
        ));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_file_that_is_not_there_yet_is_reported_when_it_appears() {
        let dir = folder("absent");
        let path = dir.join("later.txt");
        let (_watch, changes) = watch(&path);
        std::fs::write(&path, "now").unwrap();
        assert_eq!(changes.recv_timeout(WAIT), Ok(FileChange::Changed));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
