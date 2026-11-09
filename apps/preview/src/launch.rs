//! What a command line argument names: a file or a folder, as a path, a `~`
//! path or a `file://` address. Preview's window location is the file it was
//! asked to show, `bingux-preview:///path/to/file`, made different for every
//! launch so opening the file a window already shows still brings it forward.

use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use bingux_files::location::percent_decode;
use bingux_ui::nav::Location;

/// The scheme of Preview's locations.
pub const SCHEME: &str = "bingux-preview";

/// The query key that makes every launch's location different.
const LAUNCH: &str = "launch";

/// The absolute path `argument` names, byte for byte, with `.` and `..`
/// resolved by name. `None` for an address that is not on this computer.
pub fn path_of(argument: &OsStr) -> Option<PathBuf> {
    let bytes = argument.as_bytes();
    let text = std::str::from_utf8(bytes).ok().map(str::trim);
    let path = match text {
        Some(uri) if uri.to_ascii_lowercase().starts_with("file://") => {
            let rest = &uri["file://".len()..];
            let (host, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
            if !host.is_empty() && !host.eq_ignore_ascii_case("localhost") {
                return None;
            }
            PathBuf::from(OsString::from_vec(percent_decode(if path.is_empty() {
                "/"
            } else {
                path
            })))
        }
        Some(uri) if uri.contains("://") => return None,
        _ => PathBuf::from(argument),
    };
    Some(absolute(&path))
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// `path` made absolute against the current folder and the home folder.
fn absolute(path: &Path) -> PathBuf {
    let joined = match path.strip_prefix("~") {
        Ok(rest) => home().join(rest),
        Err(_) if path.is_absolute() => path.to_path_buf(),
        Err(_) => std::env::current_dir()
            .unwrap_or_else(|_| home())
            .join(path),
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

/// The window location for `argument`, or `""` (which only raises a running
/// window) with the reason on stderr when it names nothing that is there.
pub fn normalise(argument: &OsStr) -> String {
    let refuse = |reason: String| {
        eprintln!("bingux-preview: {reason}");
        String::new()
    };
    let Some(path) = path_of(argument) else {
        return refuse(format!(
            "{}: only files on this computer can be opened",
            argument.to_string_lossy()
        ));
    };
    if let Err(error) = std::fs::metadata(&path) {
        let text = error.to_string();
        return refuse(format!(
            "{}: {}",
            path.display(),
            text.split(" (os error").next().unwrap_or(&text)
        ));
    }
    address_of(&path)
}

/// The location of `path`: its components as segments, byte for byte.
pub fn address_of(path: &Path) -> String {
    let mut location = Location::rooted(SCHEME);
    for part in path.components() {
        if let Component::Normal(name) = part {
            location = location.join_bytes(name.as_bytes());
        }
    }
    location.with_query(LAUNCH, next_launch()).to_string()
}

fn next_launch() -> String {
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    format!("{nanos}-{}", COUNT.fetch_add(1, Ordering::Relaxed))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(text: &str) -> Option<PathBuf> {
        path_of(OsStr::new(text))
    }

    #[test]
    fn a_path_a_tilde_and_a_file_address_are_all_paths() {
        assert_eq!(
            path("/usr/share/a.png"),
            Some(PathBuf::from("/usr/share/a.png"))
        );
        assert_eq!(
            path("/usr/share/../lib/./a.png"),
            Some(PathBuf::from("/usr/lib/a.png"))
        );
        assert_eq!(
            path("file:///tmp/a%20b.png"),
            Some(PathBuf::from("/tmp/a b.png"))
        );
        assert_eq!(
            path("file://localhost/tmp/a.png"),
            Some(PathBuf::from("/tmp/a.png"))
        );
        assert_eq!(path("file://other-computer/tmp/a.png"), None);
        assert_eq!(path("https://example.org/a.png"), None);
        assert_eq!(path("~/a.png"), Some(home().join("a.png")));
    }

    #[test]
    fn a_relative_path_follows_the_current_folder() {
        let here = std::env::current_dir().unwrap();
        assert_eq!(path("src/a.png"), Some(here.join("src/a.png")));
    }

    #[test]
    fn the_location_carries_the_path_and_is_new_every_time() {
        let one = address_of(Path::new("/tmp/a b.png"));
        let two = address_of(Path::new("/tmp/a b.png"));
        assert_ne!(one, two);
        let location = Location::parse(&one).unwrap();
        assert_eq!(location.os_path(), PathBuf::from("/tmp/a b.png"));
    }

    #[test]
    fn a_name_that_is_not_utf8_is_named_by_its_bytes() {
        let argument = OsStr::from_bytes(b"/tmp/caf\xe9.png");
        let address = address_of(&path_of(argument).unwrap());
        assert_eq!(
            Location::parse(&address).unwrap().os_path(),
            PathBuf::from(argument)
        );
    }
}
