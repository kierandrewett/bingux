//! Saving a file so that a failure leaves the old one whole.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::stamp::Stamp;

/// Why a save did not happen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveError {
    /// The person may not write the file or its folder.
    Permission,
    /// The drive is full.
    Full,
    /// The folder does not exist (it was removed, or the drive is gone).
    FolderMissing,
    /// The file system is read-only.
    ReadOnly,
    /// The name is not allowed or is too long.
    BadName,
    /// Anything else; the text is the system's, for a log, not for the person.
    Other(String),
}

impl SaveError {
    fn of(error: &io::Error) -> SaveError {
        match error.raw_os_error() {
            Some(libc::ENOSPC | libc::EDQUOT) => return SaveError::Full,
            Some(libc::EROFS) => return SaveError::ReadOnly,
            Some(libc::ENAMETOOLONG | libc::EINVAL) => return SaveError::BadName,
            _ => {}
        }
        match error.kind() {
            io::ErrorKind::PermissionDenied => SaveError::Permission,
            io::ErrorKind::StorageFull => SaveError::Full,
            io::ErrorKind::NotFound => SaveError::FolderMissing,
            io::ErrorKind::ReadOnlyFilesystem => SaveError::ReadOnly,
            _ => SaveError::Other(error.to_string()),
        }
    }

    /// What happened and what to do next, for the body of a dialog titled
    /// "Could not save “name”".
    pub fn advice(&self) -> String {
        match self {
            SaveError::Permission => crate::tr!("ui-document-save-error-permission"),
            SaveError::Full => crate::tr!("ui-document-save-error-full"),
            SaveError::FolderMissing => crate::tr!("ui-document-save-error-folder"),
            SaveError::ReadOnly => crate::tr!("ui-document-save-error-read-only"),
            SaveError::BadName => crate::tr!("ui-document-save-error-name"),
            SaveError::Other(_) => crate::tr!("ui-document-save-error-other"),
        }
    }

    /// The whole message, for a place with no title of its own (a toast).
    pub fn message(&self, name: &str) -> String {
        crate::tr!(
            "ui-document-save-failed-message",
            name = name,
            advice = self.advice()
        )
    }
}

/// Writes `bytes` to `path`, so that a crash, a full drive or a pulled cable
/// leaves the old file as it was: the bytes go to a hidden file in the same
/// folder, are flushed, and are renamed over the old file, which keeps its
/// mode. A link is written through (the link stays a link). Where the
/// folder cannot be written but the file can, the file is written in place.
///
/// Returns the stamp of the file as written, for [`FileWatch::acknowledge`]
/// and for the document's own record. Reads and writes the disk: call it on
/// a worker.
///
/// [`FileWatch::acknowledge`]: super::FileWatch::acknowledge
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<Stamp, SaveError> {
    write_with(path, bytes, &Disk)
}

/// The three steps of a save that can fail, so that a test can fail each one
/// (a full drive, a permission error and a folder that vanishes cannot be made
/// on a test machine that runs as root).
trait Steps {
    fn create(&self, temp: &Path) -> io::Result<fs::File>;
    fn write(&self, file: &mut fs::File, bytes: &[u8]) -> io::Result<()>;
    fn rename(&self, temp: &Path, target: &Path) -> io::Result<()>;
}

struct Disk;

impl Steps for Disk {
    fn create(&self, temp: &Path) -> io::Result<fs::File> {
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(temp)
    }

    fn write(&self, file: &mut fs::File, bytes: &[u8]) -> io::Result<()> {
        file.write_all(bytes)
    }

    fn rename(&self, temp: &Path, target: &Path) -> io::Result<()> {
        fs::rename(temp, target)
    }
}

/// Makes every temporary name unique, so two saves of one file at once (two
/// windows, or a save and an autosave) never write the same temporary file.
static TEMPORARY: AtomicU64 = AtomicU64::new(0);

fn write_with(path: &Path, bytes: &[u8], steps: &dyn Steps) -> Result<Stamp, SaveError> {
    let target = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let folder = target.parent().unwrap_or_else(|| Path::new("."));
    let name = target
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let existing = fs::metadata(&target).ok();
    let temp: PathBuf = folder.join(format!(
        ".{name}.bingux-save-{}-{}",
        std::process::id(),
        TEMPORARY.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| -> io::Result<()> {
        let mut file = steps.create(&temp)?;
        steps.write(&mut file, bytes)?;
        file.sync_all()?;
        if let Some(metadata) = &existing {
            fs::set_permissions(&temp, metadata.permissions())?;
        }
        steps.rename(&temp, &target)?;
        // The rename is only durable once the folder entry is.
        if let Ok(folder) = fs::File::open(folder) {
            let _ = folder.sync_all();
        }
        Ok(())
    })();
    match result {
        Ok(()) => Stamp::of_path(&target).map_err(|error| SaveError::of(&error)),
        Err(error) => {
            let _ = fs::remove_file(&temp);
            // A folder we cannot create files in, around a file we may write.
            if error.kind() == io::ErrorKind::PermissionDenied
                && existing
                    .as_ref()
                    .is_some_and(|metadata| !metadata.permissions().readonly())
                && let Ok(mut file) = fs::OpenOptions::new()
                    .write(true)
                    .truncate(true)
                    .open(&target)
                && steps
                    .write(&mut file, bytes)
                    .and_then(|()| file.sync_all())
                    .is_ok()
            {
                return Stamp::of_path(&target).map_err(|error| SaveError::of(&error));
            }
            Err(SaveError::of(&error))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("bingux-document-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn writes_whole_and_keeps_the_mode() {
        let dir = scratch("write");
        let path = dir.join("a.sh");
        fs::write(&path, "old").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        let stamp = write_atomic(&path, b"one\ntwo\n").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"one\ntwo\n");
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o755
        );
        assert_eq!(stamp, Stamp::of_path(&path).unwrap());
        // No temporary file is left beside it.
        let names: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["a.sh"]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_link_stays_a_link() {
        let dir = scratch("link");
        let real = dir.join("real.txt");
        let link = dir.join("link.txt");
        fs::write(&real, "old").unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        write_atomic(&link, b"new").unwrap();
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read(&real).unwrap(), b"new");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_new_file_is_created_and_a_missing_folder_is_reported() {
        let dir = scratch("new");
        let path = dir.join("fresh.txt");
        write_atomic(&path, b"x").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"x");
        let error = write_atomic(&dir.join("gone/inner.txt"), b"x").unwrap_err();
        assert_eq!(error, SaveError::FolderMissing);
        assert!(
            error
                .message("inner.txt")
                .contains("folder is no longer there")
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_unwritable_folder_round_a_writable_file_is_written_in_place() {
        // Root ignores modes: the case cannot be made there.
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let dir = scratch("inplace");
        let path = dir.join("f.txt");
        fs::write(&path, "old").unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).unwrap();
        let outcome = write_atomic(&path, b"new");
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
        outcome.unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"new");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_read_only_file_in_a_read_only_folder_is_a_permission_error() {
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let dir = scratch("denied");
        let path = dir.join("f.txt");
        fs::write(&path, "old").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).unwrap();
        let outcome = write_atomic(&path, b"new");
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(outcome.unwrap_err(), SaveError::Permission);
        assert_eq!(fs::read(&path).unwrap(), b"old");
        fs::remove_dir_all(&dir).unwrap();
    }

    /// A save whose steps fail where the test says, and does the rest on disk.
    struct Fault {
        step: &'static str,
        errno: i32,
        /// What a test does between two steps of the save (another program
        /// replaces the file, the folder vanishes).
        before_rename: Option<Box<dyn Fn(&Path, &Path)>>,
    }

    impl Fault {
        fn at(step: &'static str, errno: i32) -> Fault {
            Fault {
                step,
                errno,
                before_rename: None,
            }
        }

        fn meanwhile(action: impl Fn(&Path, &Path) + 'static) -> Fault {
            Fault {
                step: "",
                errno: 0,
                before_rename: Some(Box::new(action)),
            }
        }
    }

    impl Steps for Fault {
        fn create(&self, temp: &Path) -> io::Result<fs::File> {
            if self.step == "create" {
                return Err(io::Error::from_raw_os_error(self.errno));
            }
            Disk.create(temp)
        }

        fn write(&self, file: &mut fs::File, bytes: &[u8]) -> io::Result<()> {
            if self.step == "write" {
                // Half of it reaches the disk before the drive fills up.
                file.write_all(&bytes[..bytes.len() / 2])?;
                return Err(io::Error::from_raw_os_error(self.errno));
            }
            Disk.write(file, bytes)
        }

        fn rename(&self, temp: &Path, target: &Path) -> io::Result<()> {
            if let Some(action) = &self.before_rename {
                action(temp, target);
            }
            if self.step == "rename" {
                return Err(io::Error::from_raw_os_error(self.errno));
            }
            Disk.rename(temp, target)
        }
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_drive_that_fills_up_half_way_leaves_the_old_file_and_no_leftovers() {
        let dir = scratch("full");
        let path = dir.join("a.txt");
        fs::write(&path, "old contents").unwrap();
        let error =
            write_with(&path, b"0123456789", &Fault::at("write", libc::ENOSPC)).unwrap_err();
        assert_eq!(error, SaveError::Full);
        assert!(error.message("a.txt").contains("drive is full"));
        assert_eq!(fs::read(&path).unwrap(), b"old contents");
        assert_eq!(names(&dir), ["a.txt"]);
        // The same for a quota, and when the new file would have been the first.
        let error = write_with(&path, b"x", &Fault::at("write", libc::EDQUOT)).unwrap_err();
        assert_eq!(error, SaveError::Full);
        let fresh = dir.join("b.txt");
        write_with(&fresh, b"0123", &Fault::at("write", libc::ENOSPC)).unwrap_err();
        assert!(!fresh.exists());
        assert_eq!(names(&dir), ["a.txt"]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_folder_that_refuses_a_new_file_is_a_permission_error_unless_the_file_may_be_written() {
        let dir = scratch("refused");
        // No file yet: nothing can be written there.
        let fresh = dir.join("fresh.txt");
        let error = write_with(&fresh, b"x", &Fault::at("create", libc::EACCES)).unwrap_err();
        assert_eq!(error, SaveError::Permission);
        assert!(error.message("fresh.txt").contains("permission"));
        assert_eq!(names(&dir), Vec::<String>::new());
        // A read-only file stays as it was.
        let locked = dir.join("locked.txt");
        fs::write(&locked, "old").unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o444)).unwrap();
        let error = write_with(&locked, b"new", &Fault::at("create", libc::EACCES)).unwrap_err();
        assert_eq!(error, SaveError::Permission);
        assert_eq!(fs::read(&locked).unwrap(), b"old");
        // A writable file in a folder that refuses new files is written in place.
        let open = dir.join("open.txt");
        fs::write(&open, "old").unwrap();
        write_with(&open, b"new", &Fault::at("create", libc::EACCES)).unwrap();
        assert_eq!(fs::read(&open).unwrap(), b"new");
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o644)).unwrap();
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_read_only_drive_and_a_bad_name_are_told_apart() {
        let dir = scratch("kinds");
        let path = dir.join("a.txt");
        let error = write_with(&path, b"x", &Fault::at("create", libc::EROFS)).unwrap_err();
        assert_eq!(error, SaveError::ReadOnly);
        let error = write_with(&path, b"x", &Fault::at("create", libc::ENAMETOOLONG)).unwrap_err();
        assert_eq!(error, SaveError::BadName);
        let error = write_with(&path, b"x", &Fault::at("create", libc::EIO)).unwrap_err();
        assert!(matches!(error, SaveError::Other(_)));
        assert!(!error.message("a.txt").contains("Input/output"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_failed_rename_removes_the_temporary_file_and_keeps_the_old_one() {
        let dir = scratch("rename");
        let path = dir.join("a.txt");
        fs::write(&path, "old").unwrap();
        let error = write_with(&path, b"new", &Fault::at("rename", libc::ENOSPC)).unwrap_err();
        assert_eq!(error, SaveError::Full);
        assert_eq!(fs::read(&path).unwrap(), b"old");
        assert_eq!(names(&dir), ["a.txt"]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_folder_that_vanishes_between_writing_and_renaming_is_reported() {
        let dir = scratch("vanish");
        let inner = dir.join("inner");
        fs::create_dir_all(&inner).unwrap();
        let path = inner.join("a.txt");
        fs::write(&path, "old").unwrap();
        let gone = inner.clone();
        let steps = Fault::meanwhile(move |_, _| fs::remove_dir_all(&gone).unwrap());
        let error = write_with(&path, b"new", &steps).unwrap_err();
        assert_eq!(error, SaveError::FolderMissing);
        assert!(!inner.exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_file_replaced_by_another_program_during_the_save_is_replaced_whole() {
        // The save does not look at the file again: the document compares
        // stamps before it saves (see the document's conflict check). What
        // the write itself promises is that the file is one writer's whole
        // contents, never a mix, and that the stamp it returns is the file's.
        let dir = scratch("race");
        let path = dir.join("a.txt");
        fs::write(&path, "old").unwrap();
        let theirs = path.clone();
        let steps =
            Fault::meanwhile(move |_, _| fs::write(&theirs, "theirs, written meanwhile").unwrap());
        let stamp = write_with(&path, b"ours", &steps).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"ours");
        assert_eq!(stamp, Stamp::of_path(&path).unwrap());
        assert_eq!(names(&dir), ["a.txt"]);
        // And a file that is deleted meanwhile is created again.
        let gone = path.clone();
        let steps = Fault::meanwhile(move |_, _| fs::remove_file(&gone).unwrap());
        write_with(&path, b"again", &steps).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"again");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn saves_of_one_file_at_once_never_share_a_temporary_file() {
        let dir = scratch("parallel");
        let path = dir.join("a.txt");
        fs::write(&path, "old").unwrap();
        let payloads: Vec<Vec<u8>> = (0..8u8).map(|n| vec![b'a' + n; 200_000]).collect();
        let threads: Vec<_> = payloads
            .iter()
            .cloned()
            .map(|payload| {
                let path = path.clone();
                std::thread::spawn(move || write_atomic(&path, &payload))
            })
            .collect();
        for thread in threads {
            thread.join().unwrap().unwrap();
        }
        let written = fs::read(&path).unwrap();
        assert!(
            payloads.contains(&written),
            "the file is a mix of two saves"
        );
        assert_eq!(names(&dir), ["a.txt"]);
        fs::remove_dir_all(&dir).unwrap();
    }
}
