//! The file operations every part of a profile uses: private folders, files
//! replaced whole, and what an operating-system error means for the person.
//!
//! Folders are 0700 and files 0600 whatever the umask: the mode is given at
//! creation and set again after, because the umask can only narrow it and a
//! folder made by an earlier version may be wider.

use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

/// Why something could not be written, in the terms the window uses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Problem {
    /// No space left on the disk, or the person's quota is used up.
    DiskFull,
    /// The folder or the file may not be written: a read-only mount, or
    /// permissions that do not let this user write.
    ReadOnly,
    /// Anything else, as the system worded it.
    Other(String),
}

impl Problem {
    /// What an error from the system means.
    pub fn of(error: &io::Error) -> Self {
        match error.raw_os_error() {
            Some(libc::ENOSPC | libc::EDQUOT | libc::EFBIG) => Self::DiskFull,
            Some(libc::EROFS | libc::EACCES | libc::EPERM) => Self::ReadOnly,
            _ => Self::Other(error.to_string()),
        }
    }

    /// One sentence for the person.
    pub fn text(&self) -> String {
        match self {
            Self::DiskFull => "the disk is full".to_string(),
            Self::ReadOnly => "the folder is read-only".to_string(),
            Self::Other(text) => text.clone(),
        }
    }
}

/// Makes `dir` and its missing parents, each new one 0700, and sets `dir`
/// itself to 0700 when it already existed.
pub fn private_dir(dir: &Path) -> io::Result<()> {
    DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
}

/// Writes all of `bytes` to `file` and waits until the disk has them.
pub fn write_synced(file: &mut File, bytes: &[u8]) -> io::Result<()> {
    file.write_all(bytes)?;
    file.sync_all()
}

/// A name for a temporary file next to `path` that no other writer uses.
fn temporary_beside(path: &Path) -> io::Result<PathBuf> {
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no file name"))?
        .to_string_lossy()
        .into_owned();
    Ok(path.with_file_name(format!(
        ".{name}.{}-{}.tmp",
        std::process::id(),
        random_u64()
    )))
}

/// Replaces `path` with `bytes`, mode 0600: a reader sees the old file or the
/// new one, never part of either, and a full disk leaves the old file whole.
pub fn replace(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temporary = temporary_beside(path)?;
    let written = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&temporary)?;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
        write_synced(&mut file, bytes)?;
        fs::rename(&temporary, path)
    })();
    if written.is_err() {
        let _ = fs::remove_file(&temporary);
        return written;
    }
    sync_dir(path.parent().unwrap_or(Path::new(".")))
}

/// Waits until the disk has the names in `dir` (a rename into it).
pub fn sync_dir(dir: &Path) -> io::Result<()> {
    File::open(dir)?.sync_all()
}

/// Reads `path` whole, or `None` when there is no such file.
pub fn read_if_present(path: &Path) -> io::Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// A number that differs on every call: the standard library seeds its hash
/// keys from the kernel's random source once per thread and steps them for
/// each new state.
pub fn random_u64() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos()),
    );
    hasher.finish()
}

/// Bytes used by the files below `dir`, without following symbolic links. A
/// folder that does not exist uses none.
pub fn size_of(dir: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| match entry.file_type() {
            Ok(kind) if kind.is_dir() => size_of(&entry.path()),
            Ok(kind) if kind.is_file() => entry.metadata().map_or(0, |meta| meta.len()),
            _ => 0,
        })
        .sum()
}

/// Removes `dir` and everything below it, without following symbolic links
/// (`remove_dir_all` removes a link, never what it points to). A folder that
/// does not exist is already removed.
pub fn remove_tree(dir: &Path) -> io::Result<()> {
    match fs::symlink_metadata(dir) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
        Ok(meta) if meta.file_type().is_symlink() || !meta.is_dir() => fs::remove_file(dir),
        Ok(_) => fs::remove_dir_all(dir),
    }
}

/// `path` with `suffix` added to its name.
pub fn beside(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

#[cfg(test)]
pub(crate) mod tests {
    use std::os::unix::fs::MetadataExt;

    use super::*;

    pub(crate) fn temp() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "bingux-browser-test-{}-{:x}",
            std::process::id(),
            random_u64()
        ));
        fs::create_dir_all(&dir).expect("a temporary folder");
        dir
    }

    #[test]
    fn a_full_disk_is_named_from_the_kernels_own_error() {
        // /dev/full answers every write with ENOSPC: the real error a full
        // disk gives, through the same write the store uses.
        let mut full = OpenOptions::new()
            .write(true)
            .open("/dev/full")
            .expect("/dev/full");
        let error = write_synced(&mut full, b"{}").expect_err("a full disk refuses the write");
        assert_eq!(Problem::of(&error), Problem::DiskFull);
        assert_eq!(
            Problem::of(&io::Error::from_raw_os_error(libc::EROFS)),
            Problem::ReadOnly
        );
        assert!(matches!(
            Problem::of(&io::Error::from_raw_os_error(libc::EIO)),
            Problem::Other(_)
        ));
    }

    #[test]
    fn a_replaced_file_is_private_and_whole() {
        let dir = temp();
        let path = dir.join("a.json");
        replace(&path, b"one").unwrap();
        replace(&path, b"two").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"two");
        assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
        let left: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name())
            .collect();
        assert_eq!(left, ["a.json"], "no temporary file is left behind");
        remove_tree(&dir).unwrap();
    }

    #[test]
    fn a_failed_replace_leaves_the_old_file() {
        let dir = temp();
        let path = dir.join("a.json");
        replace(&path, b"old").unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o500)).unwrap();
        let error = replace(&path, b"new").expect_err("a read-only folder refuses");
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(Problem::of(&error), Problem::ReadOnly);
        assert_eq!(fs::read(&path).unwrap(), b"old");
        remove_tree(&dir).unwrap();
    }

    #[test]
    fn a_private_folder_is_0700_even_when_it_was_wider() {
        let dir = temp();
        let inner = dir.join("a/b");
        private_dir(&inner).unwrap();
        assert_eq!(fs::metadata(&inner).unwrap().mode() & 0o777, 0o700);
        fs::set_permissions(&inner, fs::Permissions::from_mode(0o775)).unwrap();
        private_dir(&inner).unwrap();
        assert_eq!(fs::metadata(&inner).unwrap().mode() & 0o777, 0o700);
        remove_tree(&dir).unwrap();
    }

    #[test]
    fn removing_a_tree_never_follows_a_link_out_of_it() {
        let dir = temp();
        let outside = temp();
        fs::write(outside.join("keep"), b"x").unwrap();
        fs::create_dir(dir.join("p")).unwrap();
        std::os::unix::fs::symlink(&outside, dir.join("p/link")).unwrap();
        remove_tree(&dir.join("p")).unwrap();
        assert!(outside.join("keep").exists());
        assert_eq!(size_of(&dir.join("p")), 0);
        remove_tree(&dir).unwrap();
        remove_tree(&outside).unwrap();
    }
}
