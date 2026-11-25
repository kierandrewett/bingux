//! What a launch asks the browser to open.
//!
//! `bingux-browser example.com` and a link opened by another program
//! (`xdg-open https://example.com`) name an address. The window's location is
//! `bingux-browser://?open=ADDRESS&launch=N`: [`normalise`] builds it before
//! the running copy sees it, so a relative path means the folder of the
//! process that was started, and `launch` makes every launch a different
//! location, so opening the address a window already shows still opens a tab.

use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use bingux_ui::prelude::Location;

/// The scheme of the window's location.
pub const SCHEME: &str = "bingux-browser";

/// The address text a command line argument means: an address or words as the
/// address bar reads them, except that a path becomes a `file://` address,
/// made absolute here.
pub fn address_of(argument: &str) -> String {
    let text = argument.trim();
    let path = if let Some(rest) = text.strip_prefix("~/") {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join(rest))
    } else if text.starts_with("./") || text.starts_with("../") {
        std::env::current_dir().ok().map(|dir| dir.join(text))
    } else {
        None
    };
    match path {
        Some(path) => format!("file://{}", clean(&path).display()),
        None => text.to_string(),
    }
}

/// `path` with `.` and `..` resolved by name.
fn clean(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

/// The location a launch with `argument` opens.
pub fn normalise(argument: &OsStr) -> String {
    let launch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos())
        .to_string();
    let text = argument.to_string_lossy();
    Location::rooted(SCHEME)
        .with_query("open", address_of(&text))
        .with_query("launch", launch)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_and_words_are_left_for_the_address_bar() {
        assert_eq!(address_of("example.com"), "example.com");
        assert_eq!(address_of("  https://example.com/a?b=c "), "https://example.com/a?b=c");
        assert_eq!(address_of("rust lifetimes"), "rust lifetimes");
    }

    #[test]
    fn a_path_becomes_an_absolute_file_address() {
        let cwd = std::env::current_dir().expect("a current folder");
        let expected = format!("file://{}", cwd.join("page.html").display());
        assert_eq!(address_of("./page.html"), expected);
        let up = address_of("../x/y.html");
        assert!(up.starts_with("file:///"), "{up}");
        assert!(!up.contains(".."), "{up}");
    }

    #[test]
    fn a_launch_is_a_location_that_carries_the_address_and_is_never_the_same_twice() {
        let first = normalise(OsStr::new("https://example.com/?a=1&b=2"));
        let location = Location::parse_in(SCHEME, &first).expect("a location");
        assert_eq!(location.query("open"), Some("https://example.com/?a=1&b=2"));
        std::thread::sleep(std::time::Duration::from_millis(2));
        let second = normalise(OsStr::new("https://example.com/?a=1&b=2"));
        assert_ne!(first, second);
    }
}
