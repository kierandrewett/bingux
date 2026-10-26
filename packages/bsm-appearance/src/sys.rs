//! Thin, audited wrappers over the few raw `libc` calls the crate needs.
//!
//! Everything that touches a path owned by another principal goes through a
//! directory file descriptor and `*at` calls with `O_NOFOLLOW`, so a path
//! component that is swapped for a symlink between two steps cannot redirect a
//! read or a write. This is the only module (besides the converter sandbox)
//! that contains `unsafe`.

use std::ffi::CString;
use std::fs::File;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

fn cstring(bytes: &[u8]) -> io::Result<CString> {
    CString::new(bytes).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains a NUL byte"))
}

fn check(result: libc::c_int) -> io::Result<libc::c_int> {
    if result < 0 { Err(io::Error::last_os_error()) } else { Ok(result) }
}

fn owned(fd: libc::c_int) -> io::Result<OwnedFd> {
    let fd = check(fd)?;
    // SAFETY: `fd` is a freshly returned, valid descriptor that nothing else owns.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

/// Opens a directory by path. Symlinks in `path` are followed: only used for
/// trusted, configured locations (the cache root, converter roots).
pub fn open_dir_follow(path: &Path) -> io::Result<OwnedFd> {
    let path = cstring(path.as_os_str().as_bytes())?;
    // SAFETY: `path` is a valid NUL terminated string for the duration of the call.
    owned(unsafe {
        libc::open(path.as_ptr(), libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOCTTY)
    })
}

/// Opens the directory `name` below `dir` without following a symlink.
pub fn open_dir_at(dir: RawFd, name: &str) -> io::Result<OwnedFd> {
    let name = cstring(name.as_bytes())?;
    // SAFETY: `name` is a valid NUL terminated string; `dir` is a descriptor owned by the caller.
    owned(unsafe {
        libc::openat(
            dir,
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NOCTTY,
        )
    })
}

/// Opens the regular file `name` below `dir` for reading without following a symlink.
/// `O_NONBLOCK` keeps a FIFO planted by another user from blocking the caller.
pub fn open_file_at(dir: RawFd, name: &str) -> io::Result<File> {
    let name = cstring(name.as_bytes())?;
    // SAFETY: as above.
    let fd = owned(unsafe {
        libc::openat(
            dir,
            name.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NOCTTY | libc::O_NONBLOCK,
        )
    })?;
    Ok(File::from(fd))
}

/// Creates a new file `name` below `dir`, failing if it exists, mode 0644 before umask.
pub fn create_exclusive_at(dir: RawFd, name: &str) -> io::Result<File> {
    let name = cstring(name.as_bytes())?;
    // SAFETY: as above.
    let fd = owned(unsafe {
        libc::openat(
            dir,
            name.as_ptr(),
            libc::O_RDWR | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NOCTTY,
            0o644 as libc::c_uint,
        )
    })?;
    let file = File::from(fd);
    // Umask must not make the greeter's copy unreadable or the file group writable.
    // SAFETY: `file` owns a valid descriptor.
    check(unsafe { libc::fchmod(file.as_raw_fd(), 0o644) })?;
    Ok(file)
}

/// Creates the directory `name` below `dir`. An existing entry is not an error.
pub fn mkdir_at(dir: RawFd, name: &str, mode: u32) -> io::Result<()> {
    let name = cstring(name.as_bytes())?;
    // SAFETY: as above.
    match check(unsafe { libc::mkdirat(dir, name.as_ptr(), mode as libc::mode_t) }) {
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        other => other.map(|_| ()),
    }
}

/// Atomically replaces `to` with `from`, both below `dir`.
pub fn rename_at(dir: RawFd, from: &str, to: &str) -> io::Result<()> {
    let from = cstring(from.as_bytes())?;
    let to = cstring(to.as_bytes())?;
    // SAFETY: both strings are valid and NUL terminated.
    check(unsafe { libc::renameat(dir, from.as_ptr(), dir, to.as_ptr()) }).map(|_| ())
}

/// Removes the file `name` below `dir`. A missing file is not an error.
/// Returns whether something was removed.
pub fn unlink_at(dir: RawFd, name: &str) -> io::Result<bool> {
    let name = cstring(name.as_bytes())?;
    // SAFETY: as above.
    match check(unsafe { libc::unlinkat(dir, name.as_ptr(), 0) }) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        other => other.map(|_| true),
    }
}

pub fn fsync(fd: RawFd) -> io::Result<()> {
    // SAFETY: plain syscall on a descriptor owned by the caller.
    check(unsafe { libc::fsync(fd) }).map(|_| ())
}

/// Locks `fd` exclusively, waiting for other holders. The lock ends when the file closes.
pub fn flock_exclusive(fd: RawFd) -> io::Result<()> {
    loop {
        // SAFETY: plain syscall on a descriptor owned by the caller.
        match check(unsafe { libc::flock(fd, libc::LOCK_EX) }) {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            other => return other.map(|_| ()),
        }
    }
}

pub struct FileStat {
    pub uid: u32,
    pub mode: u32,
    pub size: u64,
}

pub fn fstat(fd: RawFd) -> io::Result<FileStat> {
    // SAFETY: `stat` is plain old data and `fstat` fills it on success.
    let mut stat: libc::stat = unsafe { std::mem::zeroed() };
    // SAFETY: `stat` is a valid out pointer.
    check(unsafe { libc::fstat(fd, &mut stat) })?;
    Ok(FileStat { uid: stat.st_uid, mode: stat.st_mode, size: stat.st_size.max(0) as u64 })
}


pub fn is_regular(mode: u32) -> bool {
    mode & libc::S_IFMT == libc::S_IFREG
}

pub fn effective_uid() -> u32 {
    // SAFETY: geteuid cannot fail.
    unsafe { libc::geteuid() }
}

/// Marks `fd` inheritable across `exec`. Async-signal-safe: used between fork and exec.
pub fn clear_cloexec(fd: RawFd) -> io::Result<()> {
    // SAFETY: fcntl on a descriptor; no allocation, so it is safe after fork.
    check(unsafe { libc::fcntl(fd, libc::F_SETFD, 0) }).map(|_| ())
}

/// Asks the kernel to kill this process when its parent dies. Async-signal-safe.
pub fn die_with_parent() -> io::Result<()> {
    // SAFETY: prctl with integer arguments only.
    check(unsafe { libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL as libc::c_ulong, 0, 0, 0) }).map(|_| ())
}

/// A file opened by [`open_beneath`] and the owner uid of every directory below the
/// root on the way (in order) followed by the file itself.
pub struct Opened {
    pub file: File,
    pub owners: Vec<u32>,
}

/// Opens `root/parts[0]/.../parts[n]` for reading. `root` is trusted and may be reached
/// through symlinks; every component below it is opened with `O_NOFOLLOW`, so a symlink
/// anywhere below the root is refused (`ELOOP`/`ENOTDIR`), and `.`, `..` and empty or
/// slash-containing components are refused outright. The result must be a regular file.
pub fn open_beneath(root: &Path, parts: &[&str]) -> io::Result<Opened> {
    let invalid = |why: &str| io::Error::new(io::ErrorKind::InvalidInput, why.to_string());
    let Some((last, parents)) = parts.split_last() else {
        return Err(invalid("empty path below the root"));
    };
    for part in parts {
        if part.is_empty() || *part == "." || *part == ".." || part.contains('/') || part.contains('\0') {
            return Err(invalid("path component is not a plain name"));
        }
    }
    let mut directory = open_dir_follow(root)?;
    let mut owners = Vec::with_capacity(parts.len());
    for part in parents {
        directory = open_dir_at(directory.as_raw_fd(), part)?;
        owners.push(fstat(directory.as_raw_fd())?.uid);
    }
    let file = open_file_at(directory.as_raw_fd(), last)?;
    let stat = fstat(file.as_raw_fd())?;
    if !is_regular(stat.mode) {
        return Err(invalid("not a regular file"));
    }
    owners.push(stat.uid);
    Ok(Opened { file, owners })
}

/// Opens (creating if needed, mode 0600) the lock file `name` below `dir` for `flock`.
pub fn open_lock_at(dir: RawFd, name: &str) -> io::Result<File> {
    let name = cstring(name.as_bytes())?;
    // SAFETY: as above.
    let fd = owned(unsafe {
        libc::openat(
            dir,
            name.as_ptr(),
            libc::O_RDWR | libc::O_CREAT | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NOCTTY,
            0o600 as libc::c_uint,
        )
    })?;
    Ok(File::from(fd))
}

/// Sets the mode of a directory this process owns, ignoring the umask.
pub fn fchmod(fd: RawFd, mode: u32) -> io::Result<()> {
    // SAFETY: plain syscall on a descriptor owned by the caller.
    check(unsafe { libc::fchmod(fd, mode as libc::mode_t) }).map(|_| ())
}
