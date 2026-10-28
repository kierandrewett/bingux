//! Locations of folders: `file:///home/me/Documents`.
//!
//! The window's location is the folder a copy of Files was asked to open. A
//! command line names a folder as a path, a `~` path, a `file://` URI or one
//! of these locations. [`normalise`] turns it into a location before the
//! running copy sees it, so a relative path means the folder of the process
//! that was started, not the folder of the one that is running.

use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use bingux_files::location::percent_decode;
use bingux_ui::files::{FOLDER_SCHEME as SCHEME, RECENT_SCHEME as RECENT, Where, home_folder, place_of};
use bingux_ui::prelude::Location;

/// The query key that makes every launch's location different, so opening the
/// folder a window already shows still opens a new tab.
const LAUNCH: &str = "launch";

/// The address text for a command line argument.
pub fn normalise(argument: &str) -> String {
    let text = argument.trim();
    let launch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos())
        .to_string();
    if text == "recent:///" || text == "recent://" || text == "recent:" {
        return Location::rooted(RECENT).with_query(LAUNCH, launch).to_string();
    }
    if text.starts_with("bingux-files:") {
        // Already a location: only mark the launch.
        return match Location::parse(text) {
            Some(location) => location.with_query(LAUNCH, launch).to_string(),
            None => text.to_string(),
        };
    }
    let path = match text.strip_prefix("file://") {
        Some(rest) => {
            // `file:///path` and `file://localhost/path`.
            let rest = rest.strip_prefix("localhost").unwrap_or(rest);
            PathBuf::from(OsString::from_vec(percent_decode(rest)))
        }
        None => PathBuf::from(text),
    };
    // Anything else with a scheme (smb://, trash://) is not a path here; the
    // window will open the home folder rather than guess.
    if text.contains("://") && !text.starts_with("file://") {
        return Location::rooted(SCHEME).with_query(LAUNCH, launch).to_string();
    }
    let absolute = absolute(&path);
    let mut location = Location::rooted(SCHEME);
    for part in absolute.components() {
        if let Component::Normal(name) = part {
            location = location.join(name.to_string_lossy().into_owned());
        }
    }
    location.with_query(LAUNCH, launch).to_string()
}

/// The place a location names: a folder, or the recent files. A file's
/// location shows the folder that holds it.
pub fn place(location: &Location) -> Where {
    // A launch with no folder named opens home; the root folder is `/`.
    if location.scheme() != RECENT && location.segments().is_empty() {
        return Where::Folder(home_folder());
    }
    match place_of(location) {
        Where::Folder(path) if path.is_file() => {
            Where::Folder(path.parent().map(Path::to_path_buf).unwrap_or(path))
        }
        at => at,
    }
}

/// `path` made absolute against the current folder and the home folder,
/// with `.` and `..` resolved by name (a link is not followed).
fn absolute(path: &Path) -> PathBuf {
    let home = home_folder();
    let joined = match path.strip_prefix("~") {
        Ok(rest) => home.join(rest),
        Err(_) if path.is_absolute() => path.to_path_buf(),
        Err(_) => std::env::current_dir().unwrap_or(home).join(path),
    };
    let mut clean = PathBuf::new();
    for part in joined.components() {
        match part {
            Component::ParentDir => {
                clean.pop();
            }
            Component::CurDir => {}
            other => clean.push(other.as_os_str()),
        }
    }
    clean
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(argument: &str) -> Where {
        place(&Location::parse(&normalise(argument)).expect("a location"))
    }

    #[test]
    fn a_path_is_a_folder() {
        assert_eq!(folder("/usr/share"), Where::Folder("/usr/share".into()));
    }

    #[test]
    fn dots_are_resolved_by_name() {
        assert_eq!(folder("/usr/share/../lib/./"), Where::Folder("/usr/lib".into()));
    }

    #[test]
    fn a_uri_is_decoded() {
        assert_eq!(
            folder("file:///tmp/a%20b%23c"),
            Where::Folder("/tmp/a b#c".into())
        );
    }

    #[test]
    fn names_with_address_characters_survive() {
        assert_eq!(folder("/tmp/a?b=c&d%e"), Where::Folder("/tmp/a?b=c&d%e".into()));
    }

    #[test]
    fn a_relative_path_follows_the_current_folder() {
        let here = std::env::current_dir().unwrap();
        assert_eq!(folder("src"), Where::Folder(here.join("src")));
    }

    #[test]
    fn recent_is_a_place() {
        assert_eq!(folder("recent:///"), Where::Recent);
    }

    #[test]
    fn every_launch_has_its_own_location() {
        let one = normalise("/usr");
        std::thread::sleep(std::time::Duration::from_millis(2));
        assert_ne!(one, normalise("/usr"));
    }
}
