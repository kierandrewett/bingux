//! The disk side of opening and saving a document. Every function reads or
//! writes the disk: the manager calls them on a worker.

use std::ffi::CString;
use std::fs;
use std::io::{self, Read};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use super::stamp::Stamp;
use super::write::{SaveError, write_atomic};

/// A file as it was read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Loaded {
    /// The bytes of the file.
    pub bytes: Vec<u8>,
    /// What the file was when the bytes were read (see [`Stamp`]).
    pub stamp: Stamp,
    /// Whether the person may write the file. A document of a file they may
    /// not write is saved with Save As.
    pub writable: bool,
}

/// Why a file did not open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenError {
    /// There is no such file.
    Missing,
    /// The person may not read it.
    Permission,
    /// It is a folder, or something else that holds no content.
    NotAFile,
    /// It is bigger than the app opens (the size, in bytes).
    TooLarge(u64),
    /// Anything else; the text is the system's, for a log, not for the person.
    Other(String),
}

impl OpenError {
    fn of(error: &io::Error) -> OpenError {
        match error.kind() {
            io::ErrorKind::NotFound => OpenError::Missing,
            io::ErrorKind::PermissionDenied => OpenError::Permission,
            io::ErrorKind::IsADirectory => OpenError::NotAFile,
            _ => match error.raw_os_error() {
                Some(libc::EISDIR) => OpenError::NotAFile,
                Some(libc::ENOENT | libc::ENOTDIR | libc::ELOOP) => OpenError::Missing,
                _ => OpenError::Other(error.to_string()),
            },
        }
    }

    /// What happened and what to do next, for the body of a dialog titled
    /// "Could not open “name”".
    pub fn advice(&self) -> &'static str {
        match self {
            OpenError::Missing => "It is no longer there. Check the name, or look for it in Files.",
            OpenError::Permission => "You do not have permission to read it.",
            OpenError::NotAFile => "It is a folder, not a file.",
            OpenError::TooLarge(_) => "It is too big to open here.",
            OpenError::Other(_) => "Check that the drive is connected, then try again.",
        }
    }

    /// The whole message, for a place with no title of its own (a toast).
    pub fn message(&self, name: &str) -> String {
        format!("Could not open “{name}”. {}", self.advice())
    }
}

/// Whether the person may write `path`, as the system decides (modes, owner,
/// ACLs and a read-only mount all count). A file that is not there yet is
/// writable when its folder is.
pub(crate) fn is_writable(path: &Path) -> bool {
    let probe = if path.exists() {
        path
    } else {
        path.parent()
            .filter(|folder| !folder.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
    };
    let Ok(probe) = CString::new(probe.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: `probe` is a NUL-terminated string that lives for the call.
    unsafe { libc::access(probe.as_ptr(), libc::W_OK) == 0 }
}

/// Reads `path`, refusing a file over `limit` bytes. The stamp is taken
/// before the read and again after: a file that changed during the read is
/// read again (up to three times), so the stamp never describes other bytes
/// than the ones returned.
pub(crate) fn read(path: &Path, limit: u64) -> Result<Loaded, OpenError> {
    let mut last = OpenError::Other("the file kept changing".into());
    for _ in 0..3 {
        let mut file = fs::File::open(path).map_err(|error| OpenError::of(&error))?;
        let before = file.metadata().map_err(|error| OpenError::of(&error))?;
        if !before.is_file() {
            return Err(OpenError::NotAFile);
        }
        if before.len() > limit {
            return Err(OpenError::TooLarge(before.len()));
        }
        let mut bytes = Vec::with_capacity(before.len() as usize);
        // Reads one byte past the limit, so a file that grows while it is
        // read cannot fill the memory.
        (&mut file)
            .take(limit + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| OpenError::of(&error))?;
        if bytes.len() as u64 > limit {
            return Err(OpenError::TooLarge(bytes.len() as u64));
        }
        let after = file.metadata().map_err(|error| OpenError::of(&error))?;
        let stamp = Stamp::of_metadata(&before);
        if Stamp::of_metadata(&after) == stamp {
            return Ok(Loaded {
                bytes,
                stamp,
                writable: is_writable(path),
            });
        }
        last = OpenError::Other("the file kept changing".into());
    }
    Err(last)
}

/// Why a save did not happen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SaveFailure {
    /// The disk refused it.
    Disk(SaveError),
    /// The file on disk is not the one the document read: another program
    /// changed it (the stamp is what it is now). Nothing was written.
    Changed(Stamp),
    /// The app could not turn the document into bytes (the text is for the
    /// person: what is wrong and what to do).
    Encode(String),
}

/// Saves `bytes` over `path` if the file is still what the document read,
/// `expected` (`None` for a file the document has not read: a new one, or
/// Save As, where the chooser has already asked about replacing). The
/// check and the write are two steps: a program that writes between them
/// is overwritten, which is as far as a file system lets an editor see.
pub(crate) fn save(
    path: &Path,
    bytes: &[u8],
    expected: Option<Stamp>,
) -> Result<Stamp, SaveFailure> {
    if let Some(expected) = expected
        && let Some(now) = Stamp::now(path)
        && now != expected
    {
        return Err(SaveFailure::Changed(now));
    }
    write_atomic(path, bytes).map_err(SaveFailure::Disk)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bingux-doc-io-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_file_is_read_whole_with_its_stamp_and_bytes_that_are_not_text_survive() {
        let dir = scratch("read");
        let path = dir.join("a.bin");
        let bytes: Vec<u8> = (0..=255u8).collect();
        fs::write(&path, &bytes).unwrap();
        let loaded = read(&path, 1 << 20).unwrap();
        assert_eq!(loaded.bytes, bytes);
        assert_eq!(loaded.stamp, Stamp::of_path(&path).unwrap());
        assert!(loaded.writable);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn what_cannot_be_opened_says_why() {
        let dir = scratch("errors");
        assert_eq!(read(&dir.join("none"), 10), Err(OpenError::Missing));
        assert_eq!(read(&dir, 10), Err(OpenError::NotAFile));
        // A name below a file is missing, not a crash.
        let file = dir.join("f");
        fs::write(&file, "x").unwrap();
        assert_eq!(read(&file.join("inside"), 10), Err(OpenError::Missing));
        let big = dir.join("big");
        fs::write(&big, vec![0u8; 100]).unwrap();
        assert_eq!(read(&big, 99), Err(OpenError::TooLarge(100)));
        assert_eq!(read(&big, 100).unwrap().bytes.len(), 100);
        for error in [
            OpenError::Missing,
            OpenError::Permission,
            OpenError::NotAFile,
            OpenError::TooLarge(1),
            OpenError::Other("EIO".into()),
        ] {
            let message = error.message("a.txt");
            assert!(message.starts_with("Could not open “a.txt”."), "{message}");
            assert!(!message.contains("EIO"), "{message}");
        }
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_file_the_person_cannot_read_is_a_permission_error() {
        // Root reads anything: the case cannot be made there.
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let dir = scratch("unreadable");
        let path = dir.join("secret");
        fs::write(&path, "x").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();
        assert_eq!(read(&path, 10), Err(OpenError::Permission));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_read_only_file_reads_as_not_writable() {
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let dir = scratch("readonly");
        let path = dir.join("f");
        fs::write(&path, "x").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();
        assert!(!read(&path, 10).unwrap().writable);
        assert!(!is_writable(&path));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_save_over_a_file_that_changed_writes_nothing() {
        let dir = scratch("conflict");
        let path = dir.join("a.txt");
        fs::write(&path, "one").unwrap();
        let read_at = Stamp::of_path(&path).unwrap();
        // Another program saves: a new file under the name (a new inode).
        let other = dir.join("other");
        fs::write(&other, "theirs").unwrap();
        fs::rename(&other, &path).unwrap();
        let failure = save(&path, b"ours", Some(read_at)).unwrap_err();
        assert_eq!(
            failure,
            SaveFailure::Changed(Stamp::of_path(&path).unwrap())
        );
        assert_eq!(fs::read(&path).unwrap(), b"theirs");
        // Told to overwrite, it does.
        save(&path, b"ours", None).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"ours");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_save_over_an_unchanged_or_missing_file_goes_through() {
        let dir = scratch("clean");
        let path = dir.join("a.txt");
        fs::write(&path, "one").unwrap();
        let stamp = save(&path, b"two", Some(Stamp::of_path(&path).unwrap())).unwrap();
        assert_eq!(stamp, Stamp::of_path(&path).unwrap());
        // The file was deleted since it was read: the save makes it again.
        fs::remove_file(&path).unwrap();
        save(&path, b"three", Some(stamp)).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"three");
        fs::remove_dir_all(&dir).unwrap();
    }
}
