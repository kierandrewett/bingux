//! One process at a time per profile: `flock` on the profile's `lock` file,
//! held for the life of the process. The kernel drops it when the process
//! ends, however it ends, so there is never a stale lock to clean up.

use std::fs::{File, OpenOptions};
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use crate::profiles::Folders;
use crate::store::{Files, Problem};

/// Why a profile could not be locked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LockError {
    /// Another process has it open.
    InUse,
    /// The folder or the lock file could not be made.
    Io(Problem),
}

/// The lock of an open profile. Dropping it lets another process open the profile.
#[derive(Debug)]
pub struct ProfileLock {
    _file: File,
}

impl ProfileLock {
    /// `flock(2)` on `file`.
    pub fn flock(file: &File, operation: libc::c_int) -> io::Result<()> {
        loop {
            // SAFETY: the descriptor belongs to `file`, which outlives the call.
            if unsafe { libc::flock(file.as_raw_fd(), operation) } == 0 {
                return Ok(());
            }
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::EINTR) {
                return Err(error);
            }
        }
    }

    /// Takes the lock of the profile in `folders`, making its folder if it is
    /// missing. Never waits: a profile another process holds is `InUse`.
    pub fn take(folders: &Folders) -> Result<Self, LockError> {
        Files::private_dir(&folders.root).map_err(|error| LockError::Io(Problem::of(&error)))?;
        Self::take_file(&folders.lock())
    }

    fn take_file(path: &Path) -> Result<Self, LockError> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)
            .map_err(|error| LockError::Io(Problem::of(&error)))?;
        Self::flock(&file, libc::LOCK_EX | libc::LOCK_NB).map_err(|error| {
            if error.raw_os_error() == Some(libc::EWOULDBLOCK) {
                LockError::InUse
            } else {
                LockError::Io(Problem::of(&error))
            }
        })?;
        Ok(Self { _file: file })
    }
}

/// An exclusive `flock` on `path`, waiting for it, released when dropped. For
/// the short read-change-replace of the registry.
pub struct Held {
    _file: File,
}

impl Held {
    pub fn wait(path: &Path) -> io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)?;
        ProfileLock::flock(&file, libc::LOCK_EX)?;
        Ok(Self { _file: file })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profiles::Places;
    use crate::store::Files;

    #[test]
    fn a_second_open_of_the_same_profile_is_in_use_until_the_first_lets_go() {
        let dir = Files::temp();
        let folders = Places::named_folders(&dir.join("p"));
        let first = ProfileLock::take(&folders).expect("the first open");
        // flock locks belong to the open file, so a second open in the same
        // process conflicts exactly as one in another process does.
        assert_eq!(ProfileLock::take(&folders).unwrap_err(), LockError::InUse);
        drop(first);
        assert!(ProfileLock::take(&folders).is_ok());
        Files::remove_tree(&dir).unwrap();
    }

    #[test]
    fn a_profile_on_a_read_only_folder_cannot_be_locked() {
        use std::os::unix::fs::PermissionsExt;
        let dir = Files::temp();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o500)).unwrap();
        let folders = Places::named_folders(&dir.join("p"));
        assert_eq!(
            ProfileLock::take(&folders).unwrap_err(),
            LockError::Io(Problem::ReadOnly)
        );
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        Files::remove_tree(&dir).unwrap();
    }
}
