//! What a launch argument names. A file becomes the address
//! `bingux-text-editor:///abs/path?launch=N`, made absolute here because the
//! running copy does not share this process's folder; `new:` becomes
//! `bingux-text-editor:///?new=N`. Each launch has its own number, so opening
//! the same file twice is two requests, and the second one goes to its tab.

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::ffi::OsStringExt;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use bingux_ui::nav::Location;

pub const SCHEME: &str = "bingux-text-editor";
const LAUNCH: &str = "launch";
const NEW: &str = "new";

/// What a location asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// Raise the window and change nothing.
    Nothing,
    /// A new empty document.
    New,
    Open(PathBuf),
}

fn next_launch() -> String {
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    format!("{nanos}-{}", COUNT.fetch_add(1, Ordering::Relaxed))
}

/// The address an argument names.
pub fn normalise(argument: &OsStr) -> String {
    let text = argument.to_string_lossy();
    if text.trim() == "new:" {
        return Location::rooted(SCHEME)
            .with_query(NEW, next_launch())
            .to_string();
    }
    let path = match text.strip_prefix("file://") {
        Some(rest) => PathBuf::from(std::ffi::OsString::from_vec(percent_decode(
            rest.strip_prefix("localhost").unwrap_or(rest),
        ))),
        None => PathBuf::from(argument),
    };
    let path = absolute(&path);
    let mut location = Location::rooted(SCHEME);
    for part in path.components().skip(1) {
        location = location.join_bytes(part.as_os_str().as_bytes());
    }
    location.with_query(LAUNCH, next_launch()).to_string()
}

/// What `location` asks the window for.
pub fn request(location: &Location) -> Request {
    if location.query(NEW).is_some() {
        return Request::New;
    }
    if location.query(LAUNCH).is_none() || location.segments().is_empty() {
        return Request::Nothing;
    }
    Request::Open(location.os_path())
}

fn percent_decode(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let Some(value) = std::str::from_utf8(&bytes[index + 1..index + 3])
                .ok()
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
        {
            out.push(value);
            index += 3;
            continue;
        }
        out.push(bytes[index]);
        index += 1;
    }
    out
}

/// The window's addresses: any path under the scheme's root.
pub fn table() -> bingux_ui::nav::RouteTable {
    bingux_ui::nav::RouteTable::new(SCHEME)
        .rooted()
        .mode(bingux_ui::nav::Mode::Linear)
        .root("*path")
}

/// `path` made absolute against the current folder and the home folder,
/// with `.` and `..` resolved by name.
pub fn absolute(path: &Path) -> PathBuf {
    let home = crate::prefs::home();
    let joined = match path.strip_prefix("~") {
        Ok(rest) => home.join(rest),
        Err(_) if path.is_absolute() => path.to_path_buf(),
        Err(_) => std::env::current_dir().unwrap_or(home).join(path),
    };
    let mut clean = PathBuf::from("/");
    for part in joined.components() {
        match part {
            Component::ParentDir => {
                clean.pop();
            }
            Component::CurDir | Component::RootDir | Component::Prefix(_) => {}
            Component::Normal(name) => clean.push(name),
        }
    }
    clean
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_and_new_documents() {
        let address = normalise(OsStr::new("/tmp/a b/notes.md"));
        let location = Location::parse(&address).unwrap();
        assert_eq!(
            request(&location),
            Request::Open(PathBuf::from("/tmp/a b/notes.md"))
        );
        let address = normalise(OsStr::new("file:///tmp/caf%C3%A9.txt"));
        assert_eq!(
            request(&Location::parse(&address).unwrap()),
            Request::Open(PathBuf::from("/tmp/café.txt"))
        );
        assert_eq!(
            request(&Location::parse(&normalise(OsStr::new("new:"))).unwrap()),
            Request::New
        );
        assert_eq!(request(&Location::rooted(SCHEME)), Request::Nothing);
    }

    #[test]
    fn relative_paths_resolve() {
        assert_eq!(absolute(Path::new("/a/./b/../c")), PathBuf::from("/a/c"));
    }
}
