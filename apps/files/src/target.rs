//! Locations of folders: `file:///home/me/Documents`.
//!
//! The window's location is the folder a copy of Files was asked to open. A
//! command line names a folder as a path, a `~` path, a `file://` URI or one
//! of these locations. [`normalise`] turns it into a location before the
//! running copy sees it, so a relative path means the folder of the process
//! that was started, not the folder of the one that is running. An option is
//! never an address (`bingux_ui::app` refuses it before this is asked), and
//! an argument that names no folder or file is said so on stderr and opens
//! nothing: it never becomes a place in anybody's history.

use std::ffi::OsStr;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bingux_files::location::percent_decode;
use bingux_ui::files::{
    FOLDER_SCHEME as SCHEME, RECENT_SCHEME as RECENT, Roots, TRASH_SCHEME as TRASH, Typed, Where, home_folder,
    location_of, place_of,
};
use bingux_ui::prelude::Location;

/// The query key that makes every launch's location different, so opening the
/// folder a window already shows still opens a new tab. A location with no
/// path and no launch is the window's first, with no folder asked for: the
/// home folder. With one it is the root folder, `/`.
const LAUNCH: &str = "launch";

/// How long a launch waits for the disk to say whether a path is there: a
/// network mount that is gone would otherwise hold the launch for minutes.
const DEADLINE: Duration = Duration::from_secs(3);

/// What a command line argument names, before the disk is asked.
#[derive(Debug, PartialEq, Eq)]
pub struct Named {
    /// The location to hand to the window.
    pub address: String,
    /// The path in it that has to exist, when it names one.
    pub path: Option<PathBuf>,
}

/// The address text for a command line argument, taken as the bytes the
/// process was given, or `""` when it names nothing that can be opened (the
/// reason is on stderr; an empty address only raises a running window, and
/// opens the home folder in a new one). A folder whose name is not valid UTF-8
/// is a valid argument, and the address names those exact bytes; only a path
/// can hold such a byte (a URI is ASCII), so an argument that is not UTF-8 is
/// a path.
pub fn normalise(argument: &OsStr) -> String {
    let refuse = |reason: String| {
        eprintln!("bingux-files: {reason}");
        String::new()
    };
    let named = match parse(argument) {
        Ok(named) => named,
        Err(reason) => return refuse(reason),
    };
    if let Some(path) = &named.path
        && let Err(reason) = exists(path)
    {
        return refuse(format!("{}: {reason}", path.display()));
    }
    named.address
}

/// What `argument` names, by its text alone: nothing is read from the disk.
/// `Err` is why it cannot be opened, in a sentence for stderr.
pub fn parse(argument: &OsStr) -> Result<Named, String> {
    let launch = next_launch();
    let Ok(text) = std::str::from_utf8(argument.as_bytes()) else {
        return Ok(named(Path::new(argument), &launch));
    };
    let uri = text.trim();
    let lower = uri.to_ascii_lowercase();
    if matches!(lower.as_str(), "recent:" | "recent://" | "recent:///") {
        let address = Location::rooted(RECENT).with_query(LAUNCH, launch).to_string();
        return Ok(Named { address, path: None });
    }
    if lower.starts_with("trash:") {
        // `trash:///`, and a folder in it (`trash:///Photos`). The bin's
        // folder may not exist until something is trashed: Files shows it
        // empty, as its sidebar's Trash does.
        let roots = Roots::current();
        let Typed::Folder(path) = roots.parse(uri, &home_folder()) else {
            return Err(format!("{text}: not a location"));
        };
        let path = if roots.trash_folder().is_none() { trash_folder() } else { path };
        let address = address_of(&path, &launch);
        let inside = roots.below_trash(&path).is_some_and(|below| !below.as_os_str().is_empty());
        return Ok(Named { address, path: inside.then_some(path) });
    }
    if lower.starts_with("bingux-files:") {
        // The application's own name for the folders: a path below it, or the
        // home folder when it has none.
        let location = Location::parse(uri).ok_or_else(|| format!("{text}: not a location"))?;
        let path = if location.segments().is_empty() { home_folder() } else { location.os_path() };
        return Ok(named(&path, &launch));
    }
    if lower.starts_with("file://") {
        // `file:///path` and `file://localhost/path`; a host that is another
        // computer is not a folder here.
        let rest = &uri["file://".len()..];
        let (host, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
        if !host.is_empty() && !host.eq_ignore_ascii_case("localhost") {
            return Err(format!("{text}: the folder is on another computer ({host})"));
        }
        let path = PathBuf::from(std::ffi::OsString::from_vec(percent_decode(if path.is_empty() { "/" } else { path })));
        return Ok(named(&path, &launch));
    }
    if let Some((scheme, _)) = uri.split_once("://")
        && scheme.starts_with(|first: char| first.is_ascii_alphabetic())
        && scheme.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    {
        return Err(format!(
            "{text}: only folders on this computer can be opened (a path, file://, recent:/// or trash:///)"
        ));
    }
    // A path is its bytes, spaces included.
    Ok(named(Path::new(text), &launch))
}

/// A different text for every launch, so no two launches have one location.
fn next_launch() -> String {
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_nanos());
    format!("{nanos}-{}", COUNT.fetch_add(1, Ordering::Relaxed))
}

/// The folder of the Trash, where trashed files are.
fn trash_folder() -> PathBuf {
    Roots::current().trash_folder().map_or_else(|| home_folder().join(".local/share/Trash/files"), Path::to_path_buf)
}

/// The path `path` names for the window, which must be there (except the
/// Trash's own folder, which is not made until something is binned and is
/// still the Trash).
fn named(path: &Path, launch: &str) -> Named {
    let path = absolute(path);
    let must_exist = Roots::current().trash_folder() != Some(path.as_path());
    Named { address: address_of(&path, launch), path: must_exist.then_some(path) }
}

/// Whether `path` can be reached: `Err` says why not, as `ls` does. The
/// disk is asked on a thread of its own, with a deadline.
fn exists(path: &Path) -> Result<(), String> {
    let (send, receive) = mpsc::channel();
    let asked = path.to_path_buf();
    let spawned = std::thread::Builder::new()
        .name("bingux-files-argument".into())
        .spawn(move || {
            let _ = send.send(std::fs::metadata(&asked).map(|_| ()));
        });
    if spawned.is_err() {
        return Ok(());
    }
    match receive.recv_timeout(DEADLINE) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => {
            let text = error.to_string();
            Err(text.split(" (os error").next().unwrap_or(&text).to_string())
        }
        Err(_) => Err("not responding".to_string()),
    }
}

/// The address of the place `path` names, made absolute, byte for byte. The
/// Trash's folder is `trash:///` and what is in it below that, as everywhere.
fn address_of(path: &Path, launch: &str) -> String {
    location_of(&Where::Folder(absolute(path))).with_query(LAUNCH, launch).to_string()
}

/// What a location asks the window to show: a folder or the recent files,
/// and the file to select in it when the location names a file (a file
/// argument opens its folder with the file selected). A location with no
/// folder named opens the home folder; the root folder is `/` with a launch.
pub fn destination(location: &Location) -> (Where, Option<PathBuf>) {
    if !matches!(location.scheme(), RECENT | TRASH) && location.segments().is_empty() && location.query(LAUNCH).is_none() {
        return (Where::Folder(home_folder()), None);
    }
    match place_of(location) {
        Where::Folder(path) if path.is_file() => {
            let folder = path.parent().map_or_else(|| path.clone(), Path::to_path_buf);
            (Where::Folder(folder), Some(path))
        }
        at => (at, None),
    }
}

/// The place a location names, for the tests that only care where.
#[cfg(test)]
fn place(location: &Location) -> Where {
    destination(location).0
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

    fn folder_of(argument: &OsStr) -> Where {
        let named = parse(argument).expect("a place");
        place(&Location::parse(&named.address).expect("a location"))
    }

    fn folder(argument: &str) -> Where {
        folder_of(OsStr::new(argument))
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
        let one = normalise(OsStr::new("/usr"));
        std::thread::sleep(std::time::Duration::from_millis(2));
        assert_ne!(one, normalise(OsStr::new("/usr")));
    }

    #[test]
    fn a_folder_that_is_not_utf8_is_named_by_its_exact_bytes() {
        let argument = OsStr::from_bytes(b"/var/tmp/PhoneChecks-home/caf\xe9-\xff");
        let address = parse(argument).expect("a path").address;
        assert!(address.contains("/caf%E9-%FF"), "{address}");
        assert_eq!(folder_of(argument), Where::Folder(PathBuf::from(argument)));
        // Another name that displays alike is another folder.
        let other = OsStr::from_bytes(b"/var/tmp/PhoneChecks-home/caf\xe9-\xfe");
        assert_ne!(folder_of(other), folder_of(argument));
        // So is the text of the replacement characters it displays as.
        let shown = OsStr::new("/var/tmp/PhoneChecks-home/caf\u{fffd}-\u{fffd}");
        assert_ne!(folder_of(shown), folder_of(argument));
    }

    #[test]
    fn a_relative_path_that_is_not_utf8_follows_the_current_folder() {
        let here = std::env::current_dir().unwrap();
        let name = OsStr::from_bytes(b"caf\xe9-\xff");
        assert_eq!(folder_of(name), Where::Folder(here.join(name)));
    }

    #[test]
    fn a_home_path_that_is_not_utf8_keeps_its_bytes() {
        let name = OsStr::from_bytes(b"~/caf\xe9-\xff");
        let Where::Folder(path) = folder_of(name) else {
            panic!("not a folder");
        };
        assert!(path.starts_with(home_folder()));
        assert!(path.ends_with(OsStr::from_bytes(b"caf\xe9-\xff")));
    }

    fn address(argument: &str) -> Result<String, String> {
        parse(OsStr::new(argument)).map(|named| named.address)
    }

    /// A folder of `files`, `links` and nothing else, removed when dropped.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(tag: &str) -> Self {
            let root = std::env::temp_dir().join(format!("bingux-files-target-{}-{tag}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(root.join("folder/inside")).expect("folders");
            std::fs::write(root.join("folder/a file.txt"), "a").expect("a file");
            std::os::unix::fs::symlink(root.join("folder"), root.join("link")).expect("a link");
            std::os::unix::fs::symlink(root.join("nothing"), root.join("dangling")).expect("a dangling link");
            Scratch(root)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn schemes_that_are_not_folders_here_are_refused_with_a_reason() {
        for text in ["https://example.com/x", "smb://server/share", "trashed://", "ftp://h/", "sftp://u@h/p"] {
            let reason = address(text).expect_err(text);
            assert!(reason.contains(text), "{reason}");
            assert!(reason.contains("only folders on this computer"), "{reason}");
        }
        let other = address("file://server/share").expect_err("another computer");
        assert!(other.contains("another computer (server)"), "{other}");
    }

    #[test]
    fn a_folder_called_something_with_a_colon_is_a_path() {
        assert_eq!(folder("/tmp/a:b"), Where::Folder("/tmp/a:b".into()));
        assert_eq!(folder("/tmp/odd:/x"), Where::Folder("/tmp/odd:/x".into()));
    }

    #[test]
    fn the_root_folder_is_the_root_and_not_home() {
        assert_eq!(folder("/"), Where::Folder("/".into()));
        assert_eq!(folder("file:///"), Where::Folder("/".into()));
        // No argument at all is the window's own start: home.
        let first = Location::rooted(SCHEME);
        assert_eq!(place(&first), Where::Folder(home_folder()));
    }

    #[test]
    fn tilde_and_the_application_scheme_open_the_home_folder() {
        assert_eq!(folder("~"), Where::Folder(home_folder()));
        assert_eq!(folder("~/"), Where::Folder(home_folder()));
        assert_eq!(folder("bingux-files://"), Where::Folder(home_folder()));
        assert_eq!(folder("bingux-files:///usr/lib"), Where::Folder("/usr/lib".into()));
    }

    #[test]
    fn a_trailing_slash_and_dots_name_the_same_folder() {
        let plain = folder("/usr/lib");
        for text in ["/usr/lib/", "/usr//lib", "/usr/./lib/.", "/usr/share/../lib//"] {
            assert_eq!(folder(text), plain, "{text}");
        }
    }

    #[test]
    fn the_trash_is_its_folder() {
        let Where::Folder(path) = folder("trash:///") else {
            panic!("not a folder");
        };
        assert!(path.ends_with("Trash/files"), "{}", path.display());
        assert_eq!(folder("trash:"), Where::Folder(path.clone()));
        assert_eq!(folder("TRASH://"), Where::Folder(path));
    }

    #[test]
    fn the_trash_has_its_own_address_and_a_folder_in_it_is_below_that() {
        let root = parse(OsStr::new("trash:///")).expect("the trash");
        assert!(root.address.starts_with("trash:///?launch="), "{}", root.address);
        assert_eq!(root.path, None, "the bin's folder may not exist yet, and is still the Trash");
        let Where::Folder(trash) = folder("trash:///") else {
            panic!("not a folder");
        };
        // A real path to it, a file: URI of it and the trash: address are one place.
        let real = parse(trash.as_os_str()).expect("the real path");
        assert!(real.address.starts_with("trash:///?launch="), "{}", real.address);
        let uri = format!("file://{}", trash.display());
        assert!(parse(OsStr::new(&uri)).expect("file uri").address.starts_with("trash:///?launch="));
        let inside = parse(OsStr::new("trash:///Old%20folder")).expect("a folder in the trash");
        assert!(inside.address.starts_with("trash:///Old%20folder?launch="), "{}", inside.address);
        assert_eq!(inside.path, Some(trash.join("Old folder")));
        assert_eq!(folder("trash:///Old%20folder"), Where::Folder(trash.join("Old folder")));
        // A window asked for nothing but `trash:///` opens the Trash, not home.
        let bare = Location::parse("trash:///").unwrap();
        assert_eq!(destination(&bare).0, Where::Folder(trash));
    }

    #[test]
    fn recent_has_no_path_to_check() {
        let named = parse(OsStr::new("recent://")).expect("recent");
        assert_eq!(named.path, None);
        assert!(Location::parse(&named.address).is_some_and(|location| location.scheme() == RECENT));
    }

    #[test]
    fn a_path_is_kept_to_the_byte_and_not_trimmed() {
        assert_eq!(folder(" /tmp "), Where::Folder(std::env::current_dir().unwrap().join(" /tmp ")));
        assert_eq!(folder("file:///tmp/a%20b "), Where::Folder("/tmp/a b".into()));
    }

    #[test]
    fn the_disk_is_asked_whether_the_path_is_there() {
        let scratch = Scratch::new("exists");
        let root = &scratch.0;
        for yes in ["folder", "folder/", "folder/a file.txt", "link", "link/inside", "folder/inside/.."] {
            let path = root.join(yes);
            assert_eq!(exists(&path), Ok(()), "{yes}");
            // The whole command line gives an address for it.
            assert!(!normalise(path.as_os_str()).is_empty(), "{yes}");
        }
        for no in ["nothing", "folder/nothing", "dangling", "a file.txt/inside"] {
            let path = root.join(no);
            assert!(exists(&path).is_err(), "{no}");
            assert_eq!(normalise(path.as_os_str()), "", "{no}: nothing is opened");
        }
        assert_eq!(exists(&root.join("nothing")), Err("No such file or directory".to_string()));
    }

    #[test]
    fn a_link_opens_as_the_link_and_is_not_resolved() {
        let scratch = Scratch::new("link");
        let link = scratch.0.join("link");
        assert_eq!(folder_of(link.as_os_str()), Where::Folder(link.clone()));
        let inside = link.join("inside");
        assert_eq!(folder_of(inside.as_os_str()), Where::Folder(inside));
    }

    #[test]
    fn a_file_opens_its_folder_with_the_file_selected() {
        let scratch = Scratch::new("file");
        let file = scratch.0.join("folder/a file.txt");
        let named = parse(file.as_os_str()).expect("a path");
        let location = Location::parse(&named.address).expect("a location");
        assert_eq!(destination(&location), (Where::Folder(scratch.0.join("folder")), Some(file.clone())));
        // A folder selects nothing, and so does the folder a link names.
        for folder in ["folder", "link"] {
            let named = parse(scratch.0.join(folder).as_os_str()).expect("a path");
            let location = Location::parse(&named.address).expect("a location");
            assert_eq!(destination(&location), (Where::Folder(scratch.0.join(folder)), None));
        }
        // The file through a link: the folder is the link's, the name is kept.
        let through = scratch.0.join("link/a file.txt");
        let named = parse(through.as_os_str()).expect("a path");
        let location = Location::parse(&named.address).expect("a location");
        assert_eq!(destination(&location), (Where::Folder(scratch.0.join("link")), Some(through)));
        // As a file:// URI too.
        let uri = format!("file://{}", file.display()).replace(' ', "%20");
        let named = parse(OsStr::new(&uri)).expect("a uri");
        let location = Location::parse(&named.address).expect("a location");
        assert_eq!(destination(&location).1, Some(file));
    }

    #[test]
    fn no_command_line_text_is_a_place_without_a_folder_or_a_reason() {
        for text in ["--snapshot", "-x", "recent", "trash", "folder-that-is-not-there", ""] {
            // Text with no scheme is a relative path: normalise checks the disk.
            let named = parse(OsStr::new(text)).expect("syntax only");
            let wanted = named.path.is_some() || named.address.is_empty();
            assert!(wanted, "{text:?}");
        }
        assert_eq!(normalise(OsStr::new("--snapshot")), "", "a relative path that is not there");
    }
}
