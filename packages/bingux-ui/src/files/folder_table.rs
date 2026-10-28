//! The route table of a file view: every folder is a place in one linear
//! history.
//!
//! A folder is a `file:///home/me/Documents` location, the recent files are
//! `recent:///`, and `bingux-files:` is the application's own name for the
//! folders. The chooser, a Files tab and the Files window's router all use
//! [`folder_table`], so a folder has one URI, one title and one history menu
//! everywhere. See `design/navigation.md`, "Linear apps".
//!
//! A location is text, so a folder whose name is not valid UTF-8 is recorded
//! with a lossy name; going back to it opens a folder that is not there.

use std::path::{Component, PathBuf};

use super::directory::{Where, home_folder};
use crate::nav::{Location, Mode, Params, RouteTable};

/// Folders are `file:` locations.
pub const FOLDER_SCHEME: &str = "file";
/// The scheme of the application's own name for the folders.
pub const APP_SCHEME: &str = "bingux-files";
/// The scheme of the recent files, `recent:///`.
pub const RECENT_SCHEME: &str = "recent";

/// The table of a file view: one route for every folder, back and forward
/// like a browser's, and moves are not animated.
pub fn folder_table() -> RouteTable {
    RouteTable::new(FOLDER_SCHEME)
        .rooted()
        .mode(Mode::Linear)
        .alias_scheme(APP_SCHEME)
        .also_scheme(RECENT_SCHEME)
        .root("*path")
        .title_with(folder_title)
}

/// The name a folder goes by in a history menu, a tab and the window title:
/// its own name, "Home" for the home folder, "Computer" for the root, and
/// "Recent" for the recent files.
fn folder_title(params: &Params) -> String {
    if params.scheme() == RECENT_SCHEME {
        return "Recent".to_string();
    }
    let path = params.str("path");
    if path.is_empty() {
        return "Computer".to_string();
    }
    if home_folder() == PathBuf::from("/").join(path) {
        return "Home".to_string();
    }
    path.rsplit('/').next().unwrap_or(path).to_string()
}

/// The location of a place.
pub fn location_of(at: &Where) -> Location {
    match at {
        Where::Recent => Location::rooted(RECENT_SCHEME),
        Where::Folder(path) => {
            let mut location = Location::rooted(FOLDER_SCHEME);
            for part in path.components() {
                if let Component::Normal(name) = part {
                    location = location.join(name.to_string_lossy().into_owned());
                }
            }
            location
        }
    }
}

/// The place a location names; a location with no path is the root folder.
pub fn place_of(location: &Location) -> Where {
    if location.scheme() == RECENT_SCHEME {
        return Where::Recent;
    }
    let mut path = PathBuf::from("/");
    for segment in location.segments() {
        path.push(segment);
    }
    Where::Folder(path)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
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
}
