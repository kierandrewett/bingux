//! One `statx` call, and what the model reads from it.
//!
//! A listing needs a name and a kind for every entry, and `getdents` already
//! gives both. Sizes, times, permissions and owners need `statx`, so they are
//! fetched lazily: for the rows a person can see, and in the background for
//! the rest on mounts where that is cheap. This module is the only place the
//! model calls `statx`.

use std::ffi::CString;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::entry::Kind;

/// What `statx` told us about one path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatInfo {
    /// The kind of the path itself, or of its target when `follow` was set
    /// and the path is a symbolic link.
    pub kind: Kind,
    /// The path is a symbolic link. Only known without `follow`.
    pub is_link: bool,
    /// Bytes. Zero for anything but a regular file.
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub created: Option<SystemTime>,
    pub accessed: Option<SystemTime>,
    /// Permission bits, `0o755` style (no file-type bits).
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
}

/// Identifies a folder's state for the cache: if none of these changed, no
/// entry was added, removed or renamed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirStamp {
    pub modified_ns: i128,
    pub changed_ns: i128,
    pub dev: u64,
    pub ino: u64,
}

fn raw(path: &Path, follow: bool) -> io::Result<libc::statx> {
    let c_path = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    // SAFETY: `c_path` is NUL-terminated and outlives the call; `buf` is a
    // plain-old-data struct that `statx` fully initialises on success.
    unsafe {
        let mut buf: libc::statx = std::mem::zeroed();
        let flags = if follow { 0 } else { libc::AT_SYMLINK_NOFOLLOW };
        let mask = libc::STATX_TYPE
            | libc::STATX_MODE
            | libc::STATX_UID
            | libc::STATX_GID
            | libc::STATX_SIZE
            | libc::STATX_MTIME
            | libc::STATX_ATIME
            | libc::STATX_CTIME
            | libc::STATX_BTIME;
        if libc::statx(libc::AT_FDCWD, c_path.as_ptr(), flags, mask, &mut buf) != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(buf)
    }
}

fn time(stamp: &libc::statx_timestamp) -> Option<SystemTime> {
    let nanos = stamp.tv_nsec;
    if stamp.tv_sec >= 0 {
        Some(UNIX_EPOCH + Duration::new(stamp.tv_sec as u64, nanos))
    } else {
        UNIX_EPOCH.checked_sub(Duration::new(stamp.tv_sec.unsigned_abs(), 0))
    }
}

fn kind_of_mode(mode: u16) -> (Kind, bool) {
    match u32::from(mode) & libc::S_IFMT {
        libc::S_IFDIR => (Kind::Directory, false),
        libc::S_IFREG => (Kind::File, false),
        libc::S_IFLNK => (Kind::Other, true),
        _ => (Kind::Other, false),
    }
}

/// `statx` on `path`. With `follow`, a symbolic link is described by its
/// target and `is_link` is false; use [`stat_link`] to resolve a link and
/// still know it is one.
pub fn stat(path: &Path, follow: bool) -> io::Result<StatInfo> {
    let buf = raw(path, follow)?;
    let (kind, is_link) = kind_of_mode(buf.stx_mode);
    Ok(StatInfo {
        kind,
        is_link,
        size: if kind == Kind::File { buf.stx_size } else { 0 },
        modified: time(&buf.stx_mtime),
        created: (buf.stx_mask & libc::STATX_BTIME != 0)
            .then(|| time(&buf.stx_btime))
            .flatten(),
        accessed: time(&buf.stx_atime),
        mode: u32::from(buf.stx_mode) & 0o7777,
        uid: buf.stx_uid,
        gid: buf.stx_gid,
    })
}

/// A directory entry that may be a link: what the link itself is, resolved
/// to the target where there is one. A link that points nowhere is
/// `Kind::Other` with the link's own times. This is two `statx` calls for a
/// link and one for everything else.
pub fn stat_entry(path: &Path) -> io::Result<StatInfo> {
    let own = stat(path, false)?;
    if !own.is_link {
        return Ok(own);
    }
    match stat(path, true) {
        Ok(target) => Ok(StatInfo {
            is_link: true,
            ..target
        }),
        Err(_) => Ok(StatInfo {
            kind: Kind::Other,
            is_link: true,
            size: 0,
            ..own
        }),
    }
}

/// The stamp of the folder `path` (a followed `statx`).
pub fn dir_stamp(path: &Path) -> io::Result<DirStamp> {
    let buf = raw(path, true)?;
    let ns = |stamp: &libc::statx_timestamp| {
        i128::from(stamp.tv_sec) * 1_000_000_000 + i128::from(stamp.tv_nsec)
    };
    Ok(DirStamp {
        modified_ns: ns(&buf.stx_mtime),
        changed_ns: ns(&buf.stx_ctime),
        dev: (u64::from(buf.stx_dev_major) << 32) | u64::from(buf.stx_dev_minor),
        ino: buf.stx_ino,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn describes_files_folders_and_links() {
        let dir = std::env::temp_dir().join(format!("bingux-files-stat-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("folder")).unwrap();
        fs::write(dir.join("file"), b"12345").unwrap();
        std::os::unix::fs::symlink("folder", dir.join("to-folder")).unwrap();
        std::os::unix::fs::symlink("nowhere", dir.join("broken")).unwrap();

        let file = stat_entry(&dir.join("file")).unwrap();
        assert_eq!((file.kind, file.size, file.is_link), (Kind::File, 5, false));
        assert!(file.modified.is_some());
        let folder = stat_entry(&dir.join("folder")).unwrap();
        assert_eq!((folder.kind, folder.size), (Kind::Directory, 0));
        let link = stat_entry(&dir.join("to-folder")).unwrap();
        assert_eq!((link.kind, link.is_link), (Kind::Directory, true));
        let broken = stat_entry(&dir.join("broken")).unwrap();
        assert_eq!((broken.kind, broken.is_link), (Kind::Other, true));
        assert!(stat_entry(&dir.join("absent")).is_err());
        let first = dir_stamp(&dir).unwrap();
        fs::write(dir.join("new"), b"").unwrap();
        assert_ne!(dir_stamp(&dir).unwrap(), first);
        let _ = fs::remove_dir_all(&dir);
    }
}
