//! The route table of a file view: every folder is a place in one linear
//! history.
//!
//! A folder is a `file:///home/me/Documents` location, the recent files are
//! `recent:///`, the Trash is `trash:///` (and `trash:///Folder` inside it),
//! and `bingux-files:` is the application's own name for the folders. The
//! chooser, a Files tab and the Files window's router all use
//! [`folder_table`], so a place has one URI, one title and one history menu
//! row everywhere. See `design/navigation.md`, "Linear apps", and
//! [`named`](super::named) for the named roots.
//!
//! A location is text, but a folder name is bytes: a name that is not valid
//! UTF-8 is percent-encoded byte for byte in the URI, so going back to it
//! opens that exact folder. Only the title shows it with replacement characters.

use std::os::unix::ffi::OsStrExt;
use std::path::{Component, PathBuf};

use super::directory::Where;
use super::named::{RECENT_SCHEME, Root, Roots, TRASH_SCHEME};
use crate::nav::{Location, Mode, Params, RouteTable};

/// Folders are `file:` locations.
pub const FOLDER_SCHEME: &str = "file";
/// The scheme of the application's own name for the folders.
pub const APP_SCHEME: &str = "bingux-files";

/// The table of a file view: one route for every folder, back and forward
/// like a browser's, and moves are not animated.
pub fn folder_table() -> RouteTable {
    RouteTable::new(FOLDER_SCHEME)
        .rooted()
        .mode(Mode::Linear)
        .alias_scheme(APP_SCHEME)
        .also_scheme(RECENT_SCHEME)
        .also_scheme(TRASH_SCHEME)
        .root("*path")
        .title_with(folder_title)
}

/// The name a place goes by in a history menu, a tab and the window title:
/// its own name, "Home" for the home folder, "Computer" for the root,
/// "Recent" for the recent files and "Trash" for the Trash (and the name an
/// item had before it was deleted, inside it).
fn folder_title(params: &Params) -> String {
    let roots = Roots::current();
    let path = params.str("path");
    let at = match params.scheme() {
        RECENT_SCHEME => Where::Recent,
        TRASH_SCHEME => match roots.trash_folder() {
            Some(trash) => Where::Folder(trash.join(path)),
            None => return Root::Trash.name().to_string(),
        },
        _ => Where::Folder(PathBuf::from("/").join(path)),
    };
    roots.title(&at)
}

/// The location of a place. The Trash's folder (and anything inside it) is a
/// `trash:` location, so a real path to it and `trash:///` are one place.
pub fn location_of(at: &Where) -> Location {
    let roots = Roots::current();
    let (scheme, path) = match at {
        Where::Recent => return Location::rooted(RECENT_SCHEME),
        Where::Folder(path) => match roots.below_trash(path) {
            Some(below) => (TRASH_SCHEME, below),
            None => (FOLDER_SCHEME, path.as_path()),
        },
    };
    let mut location = Location::rooted(scheme);
    for part in path.components() {
        if let Component::Normal(name) = part {
            location = location.join_bytes(name.as_bytes());
        }
    }
    location
}

/// The place a location names; a location with no path is the root folder.
pub fn place_of(location: &Location) -> Where {
    match location.scheme() {
        RECENT_SCHEME => Where::Recent,
        TRASH_SCHEME => match Roots::current().trash_folder() {
            Some(trash) => {
                let mut path = trash.to_path_buf();
                path.extend(location.os_path().components().filter_map(|part| match part {
                    Component::Normal(name) => Some(name),
                    _ => None,
                }));
                Where::Folder(path)
            }
            None => Where::Folder(location.os_path()),
        },
        _ => Where::Folder(location.os_path()),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::files::directory::home_folder;
    use crate::nav::NavModel;

    fn folder(path: &str) -> Where {
        Where::Folder(PathBuf::from(path))
    }

    #[test]
    fn a_place_and_its_location_name_each_other() {
        for at in [folder("/"), folder("/etc"), folder("/srv/with space/a#b"), Where::Recent] {
            let location = location_of(&at);
            assert_eq!(place_of(&location), at, "{location}");
            // Through the text form too: the URI is what travels.
            let parsed = Location::parse(&location.to_string()).unwrap();
            assert_eq!(place_of(&parsed), at, "{location}");
        }
        assert_eq!(location_of(&folder("/home/me")).to_string(), "file:///home/me");
        assert_eq!(location_of(&Where::Recent).to_string(), "recent:///");
    }

    #[test]
    fn the_application_scheme_names_the_same_folders() {
        let table = folder_table();
        let own = Location::parse("bingux-files:///home/me").unwrap();
        assert_eq!(table.canonical(&own).to_string(), "file:///home/me");
    }

    #[test]
    fn titles_are_the_names_people_know() {
        let table = folder_table();
        let title = |text: &str| table.title_of(&Location::parse(text).unwrap());
        assert_eq!(title("file:///"), "Computer");
        assert_eq!(title("file:///etc/ssh"), "ssh");
        assert_eq!(title("recent:///"), "Recent");
        let home = location_of(&Where::Folder(home_folder()));
        assert_eq!(table.title_of(&home), "Home");
    }

    #[test]
    fn a_folder_history_has_a_menu_and_up_reaches_the_root() {
        let table = Arc::new(folder_table());
        let mut model = NavModel::new(table.clone(), &location_of(&folder("/srv")));
        for next in ["/srv/media", "/srv/media/films"] {
            model.navigate(&location_of(&folder(next)));
        }
        let titles: Vec<_> = model.back_menu().into_iter().map(|item| item.title).collect();
        assert_eq!(titles, ["media", "srv"]);
        // Up from the root of the file system has nowhere to go.
        assert_eq!(table.up(&location_of(&folder("/"))), None);
        assert_eq!(
            table.up(&location_of(&folder("/srv/media"))),
            Some(location_of(&folder("/srv")))
        );
    }

    /// A real folder whose name is not valid UTF-8, under a scratch home.
    fn odd_folder(name: &[u8]) -> PathBuf {
        let path = PathBuf::from("/var/tmp/NavPaths-home").join(std::ffi::OsStr::from_bytes(name));
        std::fs::create_dir_all(&path).unwrap();
        assert!(path.to_str().is_none());
        path
    }

    #[test]
    fn a_folder_that_is_not_utf8_round_trips_exactly() {
        let path = odd_folder(b"caf\xe9-\xff");
        let at = Where::Folder(path.clone());
        let location = location_of(&at);
        assert!(location.to_string().ends_with("/caf%E9-%FF"), "{location}");
        assert_eq!(place_of(&location), at);
        let parsed = Location::parse(&location.to_string()).unwrap();
        assert_eq!(parsed, location);
        assert_eq!(place_of(&parsed), at);
        assert!(path.is_dir());
        // Another invalid name is another place, though both display alike.
        let other = location_of(&Where::Folder(odd_folder(b"caf\xe9-\xfe")));
        assert_ne!(other, location);
        assert!(!other.same_place(&location));
        // So is the valid text of the replacement characters.
        let text = location_of(&folder("/var/tmp/NavPaths-home/caf\u{fffd}-\u{fffd}"));
        assert_ne!(text, location);
        assert!(!text.same_place(&location));
    }

    #[test]
    fn back_and_forward_reopen_the_exact_folder() {
        let odd = odd_folder(b"caf\xe9-\xff");
        let table = Arc::new(folder_table());
        let mut model = NavModel::new(table, &location_of(&folder("/var/tmp")));
        model.navigate(&location_of(&Where::Folder(odd.clone())));
        model.navigate(&location_of(&folder("/var/tmp/NavPaths-home")));
        // The history menu shows a lossy display name.
        let menu = model.back_menu();
        let titles: Vec<_> = menu.iter().map(|item| item.title.as_str()).collect();
        assert_eq!(titles, ["caf\u{fffd}-\u{fffd}", "tmp"]);
        model.pop_to(menu[0].entry);
        assert_eq!(place_of(model.location()), Where::Folder(odd.clone()));
        let forward = model.forward_menu();
        assert_eq!(forward.len(), 1);
        model.forward();
        assert_eq!(place_of(model.location()), folder("/var/tmp/NavPaths-home"));
        model.back();
        assert_eq!(place_of(model.location()), Where::Folder(odd));
    }

    #[test]
    fn the_trash_is_a_location_of_its_own_and_a_real_path_to_it_is_the_same_place() {
        let table = Arc::new(folder_table());
        let trash = bingux_files::ops::trash_folder().expect("a data home");
        let at = Where::Folder(trash.clone());
        let location = location_of(&at);
        assert_eq!(location.to_string(), "trash:///");
        assert_eq!(place_of(&location), at);
        assert_eq!(place_of(&Location::parse("trash:///").unwrap()), at);
        let inside = Where::Folder(trash.join("Old folder"));
        let deeper = location_of(&inside);
        assert!(deeper.to_string().starts_with("trash:///Old"), "{deeper}");
        assert_eq!(place_of(&Location::parse(&deeper.to_string()).unwrap()), inside);
        // Titles and history rows say Trash, and the name an item had.
        assert_eq!(table.title_of(&location), "Trash");
        assert_eq!(table.title_of(&deeper), "Old folder");
        // The same folder reached as a `file:` location is the Trash too.
        let by_path = Location::parse(&bingux_files::location::file_uri(&trash)).unwrap();
        assert_eq!(place_of(&by_path), at);
        assert_eq!(table.title_of(&by_path), "Trash");
        // Back lists "Trash" and the name of what was in it, never `files`.
        let mut model = NavModel::new(table.clone(), &location_of(&folder("/srv")));
        model.navigate(&location);
        model.navigate(&deeper);
        model.navigate(&location_of(&folder("/srv/media")));
        let titles: Vec<_> = model.back_menu().into_iter().map(|item| item.title).collect();
        assert_eq!(titles, ["Old folder", "Trash", "srv"]);
        // Up is `history::up`, which the Trash answers with nothing above it.
        assert_eq!(crate::files::history::parent(&at), None);
    }

    #[test]
    fn equal_paths_canonicalise_equal() {
        let table = folder_table();
        let odd = odd_folder(b"caf\xe9-\xff");
        let file = location_of(&Where::Folder(odd.clone()));
        let own = Location::parse(&file.to_string().replacen("file:", "bingux-files:", 1)).unwrap();
        assert_eq!(table.canonical(&own), file);
        assert_eq!(table.canonical(&file), file);
        assert_eq!(place_of(&table.canonical(&own)), Where::Folder(odd));
    }
}
