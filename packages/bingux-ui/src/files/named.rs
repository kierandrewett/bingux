//! Named roots: places whose address is a name, not a path.
//!
//! Most places of a file view are folders and their address is the path.
//! A few are *roots* a person names: **Home** (shown as `Home`, never
//! `/home/kieran`), **Computer** (the file system's root), **Recent** (no
//! folder at all, `recent:///`) and **Trash** (a folder on disk, but its
//! address is `trash:///`, and the names inside it are the names the items had
//! before they were deleted, not the `files/` folder and `.trashinfo` names of
//! the bin).
//!
//! | Root | URI | Shown as | Icon | Is a path |
//! | ---- | --- | -------- | ---- | --------- |
//! | Computer | `file:///` | the computer's name | folder | yes |
//! | Home | `file:///home/me` | `Home` | home | yes |
//! | Recent | `recent:///` | `Recent` | recent | no |
//! | Trash | `trash:///` | `Trash` | trash | no |
//!
//! Everything a view keys on (the listing, the history, the watcher, the
//! sidebar's selected row, the rules of what may be done) stays the one
//! [`Where`]: the Trash is [`Where::Folder`] of the bin's `files` folder, so a
//! real path typed or opened from anywhere is the same place as `trash:///`
//! with no mapping to forget. What is virtual is every way of *saying* the
//! place, and this module is the only one that does it:
//!
//! - the URI of a place, both ways ([`Roots::uri`], [`Roots::parse`]);
//! - the text of the address bar's field ([`Roots::edit_text`]);
//! - the crumbs of the trail ([`Roots::crumbs`]);
//! - the name of a tab, the window and a history row ([`Roots::title`]);
//! - the icon of the place ([`Roots::icon`]);
//! - which place is above ([`Roots::parent`]).
//!
//! A new named root (a network place, a drive by its label) is one more
//! [`Root`] with its row in each of those functions; nothing else needs to
//! learn its name. Another drive's bin keeps its path: only the home bin is
//! the Trash place of the sidebar.

use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use bingux_files::PlaceKind;
use bingux_files::location::{self, file_uri, percent_decode};
use bingux_files::ops;

use super::directory::Where;
use super::file_icon::FileIcon;

/// The scheme of the recent files, `recent:///`.
pub const RECENT_SCHEME: &str = "recent";
/// The scheme of the Trash, `trash:///`.
pub const TRASH_SCHEME: &str = "trash";

/// Whether the Trash holds something, as the sidebar last looked (it looks
/// every couple of seconds, off the UI thread), so a crumb, a tab and the
/// sidebar draw the same bin. Starts empty.
static TRASH_FULL: AtomicBool = AtomicBool::new(false);

/// The sidebar saw the Trash `full` or empty.
pub(super) fn note_trash_full(full: bool) {
    TRASH_FULL.store(full, Ordering::Relaxed);
}

/// A place a person names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Root {
    Computer,
    Home,
    Recent,
    Trash,
}

impl Root {
    pub const ALL: [Root; 4] = [Root::Computer, Root::Home, Root::Recent, Root::Trash];

    /// The name the root goes by in a tab, the window and a history row.
    pub const fn name(self) -> &'static str {
        match self {
            Root::Computer => "Computer",
            Root::Home => "Home",
            Root::Recent => "Recent",
            Root::Trash => "Trash",
        }
    }

    /// The scheme of the root's URI.
    pub const fn scheme(self) -> &'static str {
        match self {
            Root::Computer | Root::Home => "file",
            Root::Recent => RECENT_SCHEME,
            Root::Trash => TRASH_SCHEME,
        }
    }

    /// The address is a name and not the path of a folder: copying it gives
    /// `recent:///` or `trash:///`, and typing it opens the place.
    pub const fn is_virtual(self) -> bool {
        matches!(self, Root::Recent | Root::Trash)
    }

    /// The sidebar row this root is.
    pub const fn place_kind(self) -> PlaceKind {
        match self {
            Root::Computer => PlaceKind::Computer,
            Root::Home => PlaceKind::Home,
            Root::Recent => PlaceKind::Recent,
            Root::Trash => PlaceKind::Trash,
        }
    }

    /// The icon of the root: the sidebar's, in a crumb, a tab and a menu. The
    /// Trash's is the bin the sidebar last saw: full, or empty and faint.
    pub fn icon(self) -> FileIcon {
        match self {
            Root::Trash => FileIcon::for_trash(TRASH_FULL.load(Ordering::Relaxed)),
            other => FileIcon::for_place_kind(other.place_kind()),
        }
    }
}

/// What a typed text turned out to name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Typed {
    Recent,
    /// A folder (or file) by path. The Trash is its folder here, as everywhere.
    Folder(PathBuf),
}

impl Typed {
    pub fn place(self) -> Where {
        match self {
            Typed::Recent => Where::Recent,
            Typed::Folder(path) => Where::Folder(path),
        }
    }
}

/// One stop of a trail: what it says and the place it opens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stop {
    pub label: String,
    pub at: Where,
}

/// The folders the named roots stand on: the home folder and the home bin's
/// `files` folder. Made up by a test, read from the environment by the app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Roots {
    home: PathBuf,
    trash: Option<PathBuf>,
}

impl Roots {
    pub fn new(home: impl Into<PathBuf>, trash: Option<PathBuf>) -> Roots {
        Roots { home: home.into(), trash }
    }

    /// The roots of this session: `$HOME` and `$XDG_DATA_HOME/Trash/files`.
    pub fn current() -> Roots {
        Roots::at_home(super::directory::home_folder())
    }

    /// The roots of a view whose home folder is `home`.
    pub fn at_home(home: impl Into<PathBuf>) -> Roots {
        Roots::new(home, ops::trash_folder())
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    /// The folder the Trash lists, which may not exist before the first item
    /// is binned. `None` without a data home.
    pub fn trash_folder(&self) -> Option<&Path> {
        self.trash.as_deref()
    }

    /// The path of `path` below the Trash's folder: empty for the Trash itself.
    /// `None` for anything outside it.
    pub fn below_trash<'a>(&self, path: &'a Path) -> Option<&'a Path> {
        path.strip_prefix(self.trash.as_deref()?).ok()
    }

    /// The root that is exactly `at`.
    pub fn root_of(&self, at: &Where) -> Option<Root> {
        match at {
            Where::Recent => Some(Root::Recent),
            Where::Folder(path) if Some(path.as_path()) == self.trash.as_deref() => Some(Root::Trash),
            Where::Folder(path) if *path == self.home && self.home != Path::new("/") => Some(Root::Home),
            Where::Folder(path) if path == Path::new("/") => Some(Root::Computer),
            Where::Folder(_) => None,
        }
    }

    /// The root `at` is, or is inside of when that root is one with a name of
    /// its own (the Trash, whose items are not at a path anyone knows).
    pub fn inside_of(&self, at: &Where) -> Option<Root> {
        match at {
            Where::Recent => Some(Root::Recent),
            Where::Folder(path) => self.below_trash(path).map(|_| Root::Trash),
        }
    }

    // ---- saying a place ---------------------------------------------------

    /// The URI of `at`: `recent:///`, `trash:///` and `trash:///Folder/inside`
    /// (percent-encoded), else the `file:` URI. What copying a location gives.
    pub fn uri(&self, at: &Where) -> String {
        match at {
            Where::Recent => format!("{RECENT_SCHEME}:///"),
            Where::Folder(path) => match self.below_trash(path) {
                Some(below) => {
                    let encoded = file_uri(below);
                    format!("{TRASH_SCHEME}:///{}", encoded.trim_start_matches("file://"))
                }
                None => file_uri(path),
            },
        }
    }

    /// The text of the address bar's field for `at`: the path with a trailing
    /// slash, `recent:///`, or `trash:///` (and `trash:///Folder/` inside).
    pub fn edit_text(&self, at: &Where) -> String {
        match at {
            Where::Recent => format!("{RECENT_SCHEME}:///"),
            Where::Folder(path) => match self.below_trash(path) {
                Some(below) if below.as_os_str().is_empty() => format!("{TRASH_SCHEME}:///"),
                Some(below) => {
                    let names: Vec<String> = below.components().map(|part| escape_name(part.as_os_str())).collect();
                    format!("{TRASH_SCHEME}:///{}/", names.join("/"))
                }
                None if path == Path::new("/") => "/".into(),
                None => format!("{}/", path.display()),
            },
        }
    }

    /// What a typed text names. `recent:`, `recent://` and `recent:///` are
    /// the recent files; `trash:`, `trash://`, `trash:///` and `trash:///a/b`
    /// are the Trash (and `a/b` inside it); anything else is a path as
    /// [`location::resolve`] reads it, so `~/.local/share/Trash/files` is the
    /// same place as `trash:///`.
    pub fn parse(&self, text: &str, base: &Path) -> Typed {
        let text = text.trim();
        if let Some(rest) = scheme_rest(text, RECENT_SCHEME)
            && rest.trim_matches('/').is_empty()
        {
            return Typed::Recent;
        }
        if let (Some(rest), Some(trash)) = (scheme_rest(text, TRASH_SCHEME), self.trash.as_deref()) {
            return Typed::Folder(inside(trash, rest));
        }
        Typed::Folder(location::resolve(text, base, &self.home))
    }

    /// The root a bare word names (`Trash`, `recent`), for a text that is not
    /// a folder anywhere: a folder of that name next to you wins.
    pub fn by_word(&self, text: &str) -> Option<Typed> {
        let word = text.trim().trim_end_matches('/');
        if word.eq_ignore_ascii_case("trash") {
            return self.trash.clone().map(Typed::Folder);
        }
        word.eq_ignore_ascii_case("recent").then_some(Typed::Recent)
    }

    /// The path text the completer reads for a `trash:///name` being typed:
    /// the same names in the Trash's folder. `None` for any other text (which
    /// the completer reads as it is) and for `trash:` before its slashes.
    pub fn completion_text(&self, typed: &str) -> Option<String> {
        let trash = self.trash.as_deref()?;
        let typed = typed.trim_start();
        let rest = scheme_rest(typed, TRASH_SCHEME)?;
        if !rest.starts_with('/') {
            return None;
        }
        let decoded = String::from_utf8_lossy(&percent_decode(rest)).into_owned();
        Some(format!("{}/{}", trash.display(), decoded.trim_start_matches('/')))
    }

    /// The crumbs of `at`, outermost first. The Trash is one crumb, `Trash`,
    /// and what is inside it follows by the names the items had when they were
    /// deleted. A path in the home folder starts at Home, any other at the
    /// computer.
    pub fn crumbs(&self, at: &Where) -> Vec<Stop> {
        let path = match at {
            Where::Recent => return vec![Stop { label: Root::Recent.name().into(), at: Where::Recent }],
            Where::Folder(path) => path,
        };
        let Some((trash, below)) = self.trash.as_deref().zip(self.below_trash(path)) else {
            return location::crumbs(path, &self.home)
                .into_iter()
                .map(|crumb| Stop { label: crumb.label, at: Where::Folder(crumb.path) })
                .collect();
        };
        let mut stops = vec![Stop { label: Root::Trash.name().into(), at: Where::Folder(trash.to_path_buf()) }];
        let mut walked = trash.to_path_buf();
        for (index, part) in below.components().enumerate() {
            walked.push(part.as_os_str());
            let label = if index == 0 { item_name(&walked) } else { part.as_os_str().to_string_lossy().into_owned() };
            stops.push(Stop { label, at: Where::Folder(walked.clone()) });
        }
        stops
    }

    /// The name of `at` in a tab, the window title and a history row: the
    /// folder's own name, and `Home`, `Computer`, `Recent` and `Trash` for the
    /// roots. Inside the Trash it is the name the item had before it was
    /// deleted.
    pub fn title(&self, at: &Where) -> String {
        if let Some(root) = self.root_of(at) {
            return root.name().to_string();
        }
        match at {
            Where::Recent => Root::Recent.name().to_string(),
            Where::Folder(path) if self.below_trash(path).is_some() => {
                self.crumbs(at).pop().map_or_else(|| Root::Trash.name().to_string(), |stop| stop.label)
            }
            Where::Folder(path) => path
                .file_name()
                .map_or_else(|| Root::Computer.name().to_string(), |name| name.to_string_lossy().into_owned()),
        }
    }

    /// The icon of `at` in a tab or a crumb.
    pub fn icon(&self, at: &Where) -> FileIcon {
        match at {
            Where::Recent => Root::Recent.icon(),
            Where::Folder(_) if self.root_of(at) == Some(Root::Trash) => Root::Trash.icon(),
            Where::Folder(path) => FileIcon::folder_at(path),
        }
    }

    /// The place above `at` (Up), `None` for the roots with nothing above
    /// them: the Trash, Recent and the computer. Inside the Trash it stays in
    /// the Trash.
    pub fn parent(&self, at: &Where) -> Option<Where> {
        let Where::Folder(path) = at else {
            return None;
        };
        if self.root_of(at) == Some(Root::Trash) {
            return None;
        }
        path.parent().map(|parent| Where::Folder(parent.to_path_buf()))
    }

    /// The text a person reads for `path` where a path is spelled out (a
    /// status line, properties): the path, or `Trash` for the Trash and
    /// `Trash / Name` inside it.
    pub fn spoken(&self, at: &Where) -> String {
        match at {
            Where::Recent => Root::Recent.name().to_string(),
            Where::Folder(_) if self.inside_of(at).is_some() => {
                self.crumbs(at).into_iter().map(|stop| stop.label).collect::<Vec<_>>().join(" / ")
            }
            Where::Folder(path) => path.display().to_string(),
        }
    }
}

/// The name an item of the Trash goes by: the name it had before it was
/// deleted, else its name in the bin.
fn item_name(in_trash: &Path) -> String {
    match ops::trash_item(in_trash) {
        Some(item) => item.original_name().to_string_lossy().into_owned(),
        None => in_trash.file_name().unwrap_or_default().to_string_lossy().into_owned(),
    }
}

/// The text after `scheme:` and its optional `//`, when `text` starts with
/// that scheme in any case.
fn scheme_rest<'a>(text: &'a str, scheme: &str) -> Option<&'a str> {
    let head = text.get(..scheme.len())?;
    if !head.eq_ignore_ascii_case(scheme) {
        return None;
    }
    let rest = text[scheme.len()..].strip_prefix(':')?;
    Some(rest.strip_prefix("//").unwrap_or(rest))
}

/// `path` below `folder`: `.` and `..` resolved by name, never above `folder`
/// (a URI path cannot climb out of its root), each name percent-decoded.
fn inside(folder: &Path, path: &str) -> PathBuf {
    let mut below: Vec<OsString> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                below.pop();
            }
            name => below.push(OsString::from_vec(percent_decode(name))),
        }
    }
    let mut found = folder.to_path_buf();
    found.extend(below);
    found
}

/// A name as the address bar's field spells it: as it is, except `%` and
/// bytes that are not text, which are percent-encoded so that
/// [`Roots::parse`] reads back the very same name.
fn escape_name(name: &OsStr) -> String {
    match std::str::from_utf8(name.as_bytes()) {
        Ok(text) => text
            .chars()
            .map(|c| if c == '%' || c.is_control() { format!("%{:02X}", c as u32) } else { c.to_string() })
            .collect(),
        Err(_) => name
            .as_bytes()
            .iter()
            .map(|&byte| if byte.is_ascii_graphic() && byte != b'%' { (byte as char).to_string() } else { format!("%{byte:02X}") })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn folder(path: &str) -> Where {
        Where::Folder(PathBuf::from(path))
    }

    /// The roots of a user `me` whose Trash is at the usual place.
    fn roots() -> Roots {
        Roots::new("/home/me", Some(PathBuf::from("/home/me/.local/share/Trash/files")))
    }

    const TRASH: &str = "/home/me/.local/share/Trash/files";

    #[test]
    fn each_root_has_a_name_a_scheme_and_an_icon() {
        let names: Vec<_> = Root::ALL.iter().map(|root| root.name()).collect();
        assert_eq!(names, ["Computer", "Home", "Recent", "Trash"]);
        let schemes: Vec<_> = Root::ALL.iter().map(|root| root.scheme()).collect();
        assert_eq!(schemes, ["file", "file", "recent", "trash"]);
        let named: Vec<_> = Root::ALL.iter().map(|root| root.is_virtual()).collect();
        assert_eq!(named, [false, false, true, true], "only Recent and Trash are not paths");
        for root in [Root::Computer, Root::Home, Root::Recent] {
            assert_eq!(root.icon(), FileIcon::for_place_kind(root.place_kind()), "{root:?}");
        }
        // The Trash is a bin, empty or full as the sidebar last saw it.
        assert_eq!(Root::Trash.icon().glyph, FileIcon::for_trash(false).glyph);
        assert_ne!(Root::Trash.icon().glyph, Root::Home.icon().glyph);
    }

    #[test]
    fn the_roots_are_found_by_where_they_stand() {
        let roots = roots();
        assert_eq!(roots.root_of(&folder("/home/me")), Some(Root::Home));
        assert_eq!(roots.root_of(&folder("/")), Some(Root::Computer));
        assert_eq!(roots.root_of(&folder(TRASH)), Some(Root::Trash));
        assert_eq!(roots.root_of(&Where::Recent), Some(Root::Recent));
        assert_eq!(roots.root_of(&folder("/home/me/Documents")), None);
        assert_eq!(roots.root_of(&folder("/home/me/.local/share/Trash")), None, "the bin's own folder is no root");
        assert_eq!(roots.inside_of(&folder(&format!("{TRASH}/old"))), Some(Root::Trash));
        assert_eq!(roots.inside_of(&folder("/home/me/old")), None);
    }

    #[test]
    fn a_place_is_said_as_its_uri_and_read_back() {
        let roots = roots();
        let uris = [
            (Where::Recent, "recent:///"),
            (folder(TRASH), "trash:///"),
            (folder(&format!("{TRASH}/Old folder")), "trash:///Old%20folder"),
            (folder(&format!("{TRASH}/Old folder/sub#1")), "trash:///Old%20folder/sub%231"),
            (folder("/home/me/Documents"), "file:///home/me/Documents"),
            (folder("/"), "file:///"),
        ];
        for (at, uri) in uris {
            assert_eq!(roots.uri(&at), uri);
            assert_eq!(roots.parse(uri, Path::new("/srv")).place(), at, "{uri}");
        }
    }

    #[test]
    fn every_way_to_write_the_trash_is_the_trash() {
        let roots = roots();
        let trash = Typed::Folder(PathBuf::from(TRASH));
        for text in [
            "trash:",
            "trash:/",
            "trash://",
            "trash:///",
            " TRASH:/// ",
            "Trash://",
            "~/.local/share/Trash/files",
            "~/.local/share/Trash/files/",
            "/home/me/.local/share/Trash/files/",
            "/home/me/./.local/share/Trash/x/../files",
            "file:///home/me/.local/share/Trash/files",
            "trash:///a/..",
            "trash:///..",
            "trash:///../../etc",
        ] {
            let expect = if text.ends_with("etc") { Typed::Folder(PathBuf::from(TRASH).join("etc")) } else { trash.clone() };
            assert_eq!(roots.parse(text, Path::new("/srv")), expect, "{text:?}");
        }
    }

    #[test]
    fn what_is_typed_inside_the_trash_is_inside_it() {
        let roots = roots();
        let inside = |text: &str| roots.parse(text, Path::new("/srv")).place();
        assert_eq!(inside("trash:///Old/sub"), folder(&format!("{TRASH}/Old/sub")));
        assert_eq!(inside("trash:///Old/./sub/"), folder(&format!("{TRASH}/Old/sub")));
        assert_eq!(inside("trash:///a%20b"), folder(&format!("{TRASH}/a b")));
        assert_eq!(inside("trash:Old"), folder(&format!("{TRASH}/Old")));
        // A bad escape stays as written, as in a file URI.
        assert_eq!(inside("trash:///100%"), folder(&format!("{TRASH}/100%")));
    }

    #[test]
    fn recent_is_recent_in_every_spelling() {
        let roots = roots();
        for text in ["recent:", "recent:/", "recent://", "recent:///", " Recent:/// "] {
            assert_eq!(roots.parse(text, Path::new("/srv")), Typed::Recent, "{text:?}");
        }
        // A path that only starts with the word is a path.
        assert_eq!(
            roots.parse("recent:///x", Path::new("/srv")),
            Typed::Folder(PathBuf::from("/srv/recent:/x"))
        );
        assert_eq!(roots.edit_text(&Where::Recent), "recent:///");
    }

    #[test]
    fn ordinary_paths_are_read_as_before() {
        let roots = roots();
        let at = |text: &str, base: &str| roots.parse(text, Path::new(base));
        assert_eq!(at("~/Music", "/srv"), Typed::Folder(PathBuf::from("/home/me/Music")));
        assert_eq!(at("..", "/srv/a"), Typed::Folder(PathBuf::from("/srv")));
        assert_eq!(at("x", "/srv"), Typed::Folder(PathBuf::from("/srv/x")));
        assert_eq!(at("file:///etc/", "/srv"), Typed::Folder(PathBuf::from("/etc")));
    }

    #[test]
    fn a_bare_word_is_a_root_only_when_no_folder_has_the_name() {
        let roots = roots();
        assert_eq!(roots.by_word("Trash"), Some(Typed::Folder(PathBuf::from(TRASH))));
        assert_eq!(roots.by_word(" trash/ "), Some(Typed::Folder(PathBuf::from(TRASH))));
        assert_eq!(roots.by_word("RECENT"), Some(Typed::Recent));
        assert_eq!(roots.by_word("trashy"), None);
        assert_eq!(roots.by_word("Documents"), None);
        // Without a bin there is no Trash to name.
        assert_eq!(Roots::new("/home/me", None).by_word("trash"), None);
    }

    #[test]
    fn the_field_says_trash_and_reads_it_back_without_changing_its_size() {
        let roots = roots();
        assert_eq!(roots.edit_text(&folder(TRASH)), "trash:///");
        assert_eq!(roots.edit_text(&folder(&format!("{TRASH}/Old folder/sub"))), "trash:///Old folder/sub/");
        assert_eq!(roots.edit_text(&folder("/home/me/Documents")), "/home/me/Documents/");
        assert_eq!(roots.edit_text(&folder("/")), "/");
        for at in [folder(TRASH), folder(&format!("{TRASH}/100% a#b/c")), folder(&format!("{TRASH}/x y"))] {
            let text = roots.edit_text(&at);
            assert_eq!(roots.parse(&text, Path::new("/")).place(), at, "{text}");
        }
    }

    #[test]
    fn a_name_that_is_not_text_survives_the_field_and_the_uri() {
        let roots = roots();
        let name = OsStr::from_bytes(b"caf\xe9-\xff");
        let at = Where::Folder(PathBuf::from(TRASH).join(name));
        for text in [roots.edit_text(&at), roots.uri(&at)] {
            assert!(text.is_ascii() || text.starts_with("trash:///"), "{text}");
            assert_eq!(roots.parse(&text, Path::new("/")).place(), at, "{text}");
        }
        assert_eq!(roots.edit_text(&at), "trash:///caf%E9-%FF/");
    }

    #[test]
    fn the_trash_is_one_crumb_and_what_is_in_it_follows() {
        let roots = roots();
        let labels = |at: &Where| roots.crumbs(at).into_iter().map(|stop| stop.label).collect::<Vec<_>>();
        assert_eq!(labels(&folder(TRASH)), ["Trash"]);
        assert_eq!(labels(&folder(&format!("{TRASH}/Old/sub"))), ["Trash", "Old", "sub"]);
        // Never the folders the bin is made of.
        let stops = roots.crumbs(&folder(&format!("{TRASH}/Old")));
        assert_eq!(stops[0].at, folder(TRASH));
        assert_eq!(stops[1].at, folder(&format!("{TRASH}/Old")));
        assert_eq!(labels(&Where::Recent), ["Recent"]);
        assert_eq!(labels(&folder("/home/me/Documents")), ["Home", "Documents"]);
        assert_eq!(roots.crumbs(&folder("/home/me"))[0].label, "Home");
        assert_eq!(labels(&folder("/home/me/.local/share")), ["Home", ".local", "share"], "the bin's parents are plain folders");
    }

    #[test]
    fn titles_are_what_tabs_windows_and_history_rows_read() {
        let roots = roots();
        let title = |at: Where| roots.title(&at);
        assert_eq!(title(folder(TRASH)), "Trash");
        assert_eq!(title(Where::Recent), "Recent");
        assert_eq!(title(folder("/home/me")), "Home");
        assert_eq!(title(folder("/")), "Computer");
        assert_eq!(title(folder("/home/me/Documents")), "Documents");
        assert_eq!(title(folder(&format!("{TRASH}/Old/sub"))), "sub");
        assert_eq!(roots.spoken(&folder(&format!("{TRASH}/Old/sub"))), "Trash / Old / sub");
        assert_eq!(roots.spoken(&folder(TRASH)), "Trash");
        assert_eq!(roots.spoken(&folder("/srv/x")), "/srv/x");
    }

    #[test]
    fn up_never_leaves_the_trash_for_the_bin_it_lives_in() {
        let roots = roots();
        assert_eq!(roots.parent(&folder(TRASH)), None);
        assert_eq!(roots.parent(&folder(&format!("{TRASH}/Old"))), Some(folder(TRASH)));
        assert_eq!(roots.parent(&folder(&format!("{TRASH}/Old/sub"))), Some(folder(&format!("{TRASH}/Old"))));
        assert_eq!(roots.parent(&folder("/home/me")), Some(folder("/home")));
        assert_eq!(roots.parent(&Where::Recent), None);
    }

    #[test]
    fn the_trash_is_completed_from_its_own_folder() {
        let roots = roots();
        assert_eq!(roots.completion_text("trash:///Ol"), Some(format!("{TRASH}/Ol")));
        assert_eq!(roots.completion_text("trash:///"), Some(format!("{TRASH}/")));
        assert_eq!(roots.completion_text("trash://"), None, "before the root's slash there is nothing to complete");
        assert_eq!(roots.completion_text("trash:///Old/s"), Some(format!("{TRASH}/Old/s")));
        assert_eq!(roots.completion_text("/home/me/Do"), None);
        assert_eq!(roots.completion_text("recent:///"), None);
    }

    /// A bin on a drive (`.Trash-1000`), so the names come from real info
    /// files without touching the environment.
    fn bin_with(items: &[(&str, &str)]) -> (PathBuf, Roots) {
        let top = std::env::temp_dir().join(format!("named-roots-{}-{}", std::process::id(), items.len()));
        let _ = fs::remove_dir_all(&top);
        let bin = top.join(".Trash-1000");
        fs::create_dir_all(bin.join("files")).unwrap();
        fs::create_dir_all(bin.join("info")).unwrap();
        for (in_bin, original) in items {
            fs::create_dir_all(bin.join("files").join(in_bin)).unwrap();
            fs::write(
                bin.join("info").join(format!("{in_bin}.trashinfo")),
                format!("[Trash Info]\nPath={original}\nDeletionDate=2026-10-28T09:30:00\n"),
            )
            .unwrap();
        }
        let roots = Roots::new("/home/me", Some(bin.join("files")));
        (top, roots)
    }

    #[test]
    fn an_item_goes_by_the_name_it_had_before_it_was_deleted() {
        // `Photos.2` is what a clash made of the second `Photos`.
        let (top, roots) = bin_with(&[("Photos", "Photos"), ("Photos.2", "Photos")]);
        let trash = roots.trash_folder().unwrap().to_path_buf();
        let second = Where::Folder(trash.join("Photos.2"));
        assert_eq!(roots.title(&second), "Photos");
        let labels: Vec<_> = roots.crumbs(&Where::Folder(trash.join("Photos.2").join("raw"))).into_iter().map(|stop| stop.label).collect();
        assert_eq!(labels, ["Trash", "Photos", "raw"], "the first name is the original one, the deeper names are as they are");
        // The address stays exact: it names the item in the bin.
        assert_eq!(roots.uri(&second), "trash:///Photos.2");
        assert_eq!(roots.parse("trash:///Photos.2", Path::new("/")).place(), second);
        fs::remove_dir_all(top).unwrap();
    }
}
