//! What a file on disk was when it was read or written last.

use std::fs::Metadata;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// The identity of a file's contents, as far as the file system tells: its
/// modification time (to the nanosecond), size and inode.
///
/// A document keeps the stamp from when it read or wrote the file. A file
/// whose stamp is different now was changed by someone else: another
/// program saved it, or replaced it (an atomic save by another editor makes
/// a new inode with the same size and, within a second, the same time).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stamp {
    modified: Option<(i64, u32)>,
    len: u64,
    inode: u64,
}

impl Stamp {
    pub fn of_metadata(metadata: &Metadata) -> Stamp {
        Stamp {
            modified: Some((metadata.mtime(), metadata.mtime_nsec() as u32)),
            len: metadata.len(),
            inode: metadata.ino(),
        }
    }

    /// The stamp of `path` now. Reads the disk: call it on a worker.
    pub fn of_path(path: &Path) -> io::Result<Stamp> {
        std::fs::metadata(path).map(|metadata| Stamp::of_metadata(&metadata))
    }

    /// The stamp of `path` now, or `None` when it cannot be read (it is gone,
    /// or in a folder that cannot be searched).
    pub fn now(path: &Path) -> Option<Stamp> {
        Stamp::of_path(path).ok()
    }

    /// When the file was last written.
    pub fn modified(&self) -> Option<SystemTime> {
        let (seconds, nanos) = self.modified?;
        if seconds >= 0 {
            Some(UNIX_EPOCH + Duration::new(seconds as u64, nanos))
        } else {
            UNIX_EPOCH
                .checked_sub(Duration::new(seconds.unsigned_abs(), 0))
                .map(|time| time + Duration::from_nanos(u64::from(nanos)))
        }
    }

    /// The size in bytes.
    pub fn len(&self) -> u64 {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}
