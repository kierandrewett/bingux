//! The facts of one `stat` that are for the curious: inode, device, links,
//! disk usage, the security context and the extended attributes.

use std::ffi::{CString, OsStr};
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use super::OpError;

/// The longest value of an extended attribute that is read. SELinux labels
/// and ACLs are far smaller; a thumbnail stored by a tool is not worth it.
const VALUE_LIMIT: usize = 4096;
/// How many attributes are listed.
const ATTRIBUTE_LIMIT: usize = 200;
/// How many characters of a text value are shown.
const SHOWN_CHARS: usize = 200;
/// What is shown for a value that could not be read or is too long.
const NOT_SHOWN: &str = "Not shown";
/// How many bytes of a binary value are shown, as hex.
const SHOWN_BYTES: usize = 24;

/// One extended attribute, its value already worded for a person.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Xattr {
    /// `user.xdg.origin.url`, `security.selinux`.
    pub name: String,
    /// The text of the value, or its first bytes in hex when it is not text.
    pub value: String,
}

/// What `stat`, the security module and the file system add to a name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileDetails {
    pub inode: u64,
    /// The device the file is on, as the kernel's major and minor numbers.
    pub device: (u32, u32),
    /// Names that lead to this file (for a folder, its sub-folders and itself).
    pub hard_links: u64,
    /// The size the file system likes to read and write in.
    pub block_size: u64,
    /// What the file takes on the drive, which differs from its size for a
    /// sparse, compressed or small file.
    pub disk_bytes: u64,
    /// The SELinux label, on a system that sets one.
    pub security_context: Option<String>,
    /// The file has access control list entries beyond owner, group and
    /// others.
    pub acl: bool,
    /// Every attribute, in the order the file system gives them.
    pub xattrs: Vec<Xattr>,
}

/// Read the [`FileDetails`] of `path`, following a link to its target. The
/// attributes are an empty list on a drive that has none.
pub fn file_details(path: &Path) -> Result<FileDetails, OpError> {
    let meta = fs::metadata(path).or_else(|_| fs::symlink_metadata(path))?;
    let xattrs = xattrs(path);
    let named = |name: &str| xattrs.iter().find(|attribute| attribute.name == name);
    let security_context = named("security.selinux").map(|attribute| attribute.value.clone());
    let acl = named("system.posix_acl_access").is_some() || named("system.posix_acl_default").is_some();
    Ok(FileDetails {
        inode: meta.ino(),
        device: device_numbers(meta.dev()),
        hard_links: meta.nlink(),
        block_size: meta.blksize(),
        disk_bytes: meta.blocks() * 512,
        security_context,
        acl,
        xattrs,
    })
}

/// The major and minor of a `st_dev`, as glibc's `major` and `minor` split
/// it.
fn device_numbers(dev: u64) -> (u32, u32) {
    let major = ((dev >> 8) & 0xfff) | ((dev >> 32) & !0xfff);
    let minor = (dev & 0xff) | ((dev >> 12) & !0xff);
    (major as u32, minor as u32)
}

/// The extended attributes of `path`. A file system without them, or one that
/// does not let this person list them, gives an empty list.
pub fn xattrs(path: &Path) -> Vec<Xattr> {
    let Ok(c_path) = CString::new(path.as_os_str().as_bytes()) else {
        return Vec::new();
    };
    let Some(names) = list(&c_path) else {
        return Vec::new();
    };
    names
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
        .take(ATTRIBUTE_LIMIT)
        .filter_map(|name| {
            let attribute = CString::new(name).ok()?;
            Some(Xattr {
                name: OsStr::from_bytes(name).to_string_lossy().into_owned(),
                value: read(&c_path, &attribute).map_or_else(|| NOT_SHOWN.to_owned(), |value| value_text(&value)),
            })
        })
        .collect()
}

/// The NUL-separated names, or `None` if the file system gives none. The size
/// is asked first; the list may grow between the calls, so a short buffer is
/// asked again.
fn list(path: &CString) -> Option<Vec<u8>> {
    for _ in 0..3 {
        // SAFETY: `path` is NUL-terminated; a null buffer with size 0 asks for
        // the size the list needs.
        let size = unsafe { libc::listxattr(path.as_ptr(), std::ptr::null_mut(), 0) };
        if size <= 0 {
            return None;
        }
        let mut buffer = vec![0_u8; size as usize];
        // SAFETY: the buffer is `size` bytes long and outlives the call.
        let got = unsafe { libc::listxattr(path.as_ptr(), buffer.as_mut_ptr().cast(), buffer.len()) };
        if got >= 0 {
            buffer.truncate(got as usize);
            return Some(buffer);
        }
        if std::io::Error::last_os_error().raw_os_error() != Some(libc::ERANGE) {
            return None;
        }
    }
    None
}

/// One attribute's value, or `None` when it is gone or longer than
/// [`VALUE_LIMIT`].
fn read(path: &CString, name: &CString) -> Option<Vec<u8>> {
    let mut buffer = vec![0_u8; VALUE_LIMIT];
    // SAFETY: both strings are NUL-terminated and the buffer is as long as
    // the size passed.
    let got = unsafe {
        libc::getxattr(path.as_ptr(), name.as_ptr(), buffer.as_mut_ptr().cast(), buffer.len())
    };
    (got >= 0).then(|| {
        buffer.truncate(got as usize);
        buffer
    })
}

/// A value as a person reads it: text when it is text (a trailing NUL, which
/// the security modules add, dropped), else the first bytes in hex.
fn value_text(bytes: &[u8]) -> String {
    let text = bytes.strip_suffix(&[0]).unwrap_or(bytes);
    match std::str::from_utf8(text) {
        Ok(text) if !text.chars().any(char::is_control) => {
            if text.chars().count() > SHOWN_CHARS {
                let head: String = text.chars().take(SHOWN_CHARS).collect();
                format!("{head}…")
            } else {
                text.to_owned()
            }
        }
        _ => {
            let head: Vec<String> = bytes
                .iter()
                .take(SHOWN_BYTES)
                .map(|byte| format!("{byte:02x}"))
                .collect();
            let more = if bytes.len() > SHOWN_BYTES { "…" } else { "" };
            format!("{}{more} ({} bytes)", head.join(" "), bytes.len())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::Temp;
    use super::*;

    #[test]
    fn the_device_is_split_like_glibc_does() {
        // /dev/sda1 is 8:1, /dev/nvme0n1p2 is 259:2.
        assert_eq!(device_numbers(0x0801), (8, 1));
        assert_eq!(device_numbers((259 << 8) | 2), (259, 2));
        assert_eq!(device_numbers(0), (0, 0));
    }

    #[test]
    fn text_values_are_shown_and_others_in_hex() {
        assert_eq!(value_text(b"https://example.com/a"), "https://example.com/a");
        assert_eq!(value_text(b"system_u:object_r:user_home_t:s0\0"), "system_u:object_r:user_home_t:s0");
        assert_eq!(value_text(&[0, 1, 0xff]), "00 01 ff (3 bytes)");
        let long = "x".repeat(300);
        assert_eq!(value_text(long.as_bytes()), format!("{}…", "x".repeat(SHOWN_CHARS)));
        let blob = [7_u8; 40];
        assert!(value_text(&blob).ends_with("… (40 bytes)"));
    }

    #[test]
    fn a_files_numbers_come_from_stat() {
        let temp = Temp::new("details-stat");
        let file = temp.write("a", "hello");
        std::fs::hard_link(&file, temp.path().join("b")).unwrap();
        let details = file_details(&file).unwrap();
        assert_eq!(details.hard_links, 2);
        assert!(details.inode > 0 && details.block_size >= 512);
        assert!(details.disk_bytes >= 5 || details.disk_bytes == 0);
        assert_eq!(file_details(&temp.path().join("gone")), Err(OpError::NotFound));
    }

    #[test]
    fn a_user_attribute_is_listed_when_the_drive_keeps_them() {
        let temp = Temp::new("details-xattr");
        let file = temp.write("a", "x");
        let c_path = CString::new(file.as_os_str().as_bytes()).unwrap();
        let name = CString::new("user.bingux.test").unwrap();
        // SAFETY: NUL-terminated strings and a value of the length passed.
        let set = unsafe { libc::setxattr(c_path.as_ptr(), name.as_ptr(), b"hello".as_ptr().cast(), 5, 0) };
        if set != 0 {
            eprintln!("skipped: this drive keeps no user attributes");
            return;
        }
        let found = xattrs(&file);
        assert!(
            found.contains(&Xattr {
                name: "user.bingux.test".to_owned(),
                value: "hello".to_owned()
            }),
            "{found:?}"
        );
        assert!(!file_details(&file).unwrap().acl);
    }

    #[test]
    fn a_file_with_no_attributes_has_none() {
        let temp = Temp::new("details-none");
        let file = temp.write("a", "x");
        // A SELinux system labels every file; any other has none.
        assert!(xattrs(&file).iter().all(|a| a.name.starts_with("security.")));
    }
}
