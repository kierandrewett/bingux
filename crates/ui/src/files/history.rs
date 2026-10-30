//! What goes into a tab's Back and Forward history, and what does not.
//!
//! A tab's history is a [`NavModel`] on [`folder_table`](super::folder_table):
//! a linear list of places with one current place, like a browser tab's. These
//! functions are the only way the file view moves it, so the rules are in one
//! place and are tested without a window (`docs/files.md`, "History"):
//!
//! - A place is an absolute folder path or the recent files. Anything else (a
//!   relative path, an empty one, text that was a command line option) is not
//!   a place: it is ignored, never recorded.
//! - A path is cleaned by name (`.`, `..`, repeated and trailing slashes) and
//!   never resolved through links: the entry is the folder under the name the
//!   person used, so a link opens as the link.
//! - Going to the place already shown records nothing.
//! - Any other move (a click, the address bar, a sidebar place, Up, Home, a
//!   folder opened from the list) adds an entry after the current one and
//!   forgets Forward. Refresh, a search, an error and its retry are not moves.
//! - Back, Forward and a choice from a history menu only move along the list.
//! - The list keeps the last [`HISTORY_LIMIT`](crate::nav::HISTORY_LIMIT)
//!   places behind the current one.

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

use super::directory::Where;
use super::folder_table::{location_of, place_of};
use crate::nav::{HistoryItem, Location, NavModel, Outcome};

/// `at` as a place the history can hold: a folder path made clean by name, or
/// the recent files. `None` for a path that is empty or not absolute.
pub fn place(at: &Where) -> Option<Where> {
    match at {
        Where::Recent => Some(Where::Recent),
        Where::Folder(path) => clean(path).map(Where::Folder),
    }
}

/// The absolute `path` with `.` and `..` resolved by name and no repeated or
/// trailing slash. A link is not followed.
fn clean(path: &Path) -> Option<PathBuf> {
    if !path.is_absolute() {
        return None;
    }
    let mut cleaned = PathBuf::from("/");
    for part in path.components() {
        match part {
            Component::Normal(name) => cleaned.push(name),
            Component::ParentDir => {
                cleaned.pop();
            }
            _ => {}
        }
    }
    Some(cleaned)
}

/// Go to `to`: a new entry after the current one, forgetting Forward. Does
/// nothing (`Unchanged`) for a place that is not one (see [`place`]) and for
/// the place already shown.
pub fn visit(model: &mut NavModel, to: &Where) -> Outcome {
    match place(to) {
        Some(to) => model.navigate(&location_of(&to)),
        None => Outcome::Unchanged,
    }
}

/// The place that holds `at`, or `None` for the root, the recent files, the
/// Trash (which is a root of its own) and anything that is not a place.
pub fn parent(at: &Where) -> Option<Where> {
    super::named::Roots::current().parent(&place(at)?)
}

/// Go to the folder that holds the one shown: a move like any other, so Back
/// returns to where Up was pressed.
pub fn up(model: &mut NavModel) -> Outcome {
    match parent(&place_of(model.location())) {
        Some(parent) => visit(model, &parent),
        None => Outcome::Unchanged,
    }
}

/// The places of a history, for a host or a test to read: what Back would
/// reach (oldest first), the place shown and what Forward would reach
/// (nearest first).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trail {
    pub back: Vec<Where>,
    pub current: Where,
    pub forward: Vec<Where>,
}

impl Trail {
    pub fn of(model: &NavModel) -> Self {
        Self {
            back: model.back_entries().iter().map(|entry| place_of(&entry.location)).collect(),
            current: place_of(model.location()),
            forward: model
                .forward_entries()
                .iter()
                .rev()
                .map(|entry| place_of(&entry.location))
                .collect(),
        }
    }
}

/// The rows of the Back and Forward menus, nearest first, with the folders
/// that go by one name told apart as tabs are: `Documents — kieran` and
/// `Documents — work`. The same folder twice in a history is one name. The
/// folder shown counts too, so a row never reads as the place you are in.
pub fn menus(model: &NavModel) -> (Vec<HistoryItem>, Vec<HistoryItem>) {
    let mut back = model.back_menu();
    let mut forward = model.forward_menu();
    let mut places: HashMap<String, HashSet<Location>> = HashMap::new();
    places.entry(model.title(model.current())).or_default().insert(model.location().clone());
    for item in back.iter().chain(&forward) {
        places.entry(item.title.clone()).or_default().insert(item.location.clone());
    }
    for item in back.iter_mut().chain(forward.iter_mut()) {
        if places.get(&item.title).is_some_and(|same| same.len() > 1)
            && let Some(parent) = place_name_above(&item.location)
        {
            item.title = format!("{} \u{2014} {parent}", item.title);
        }
    }
    (back, forward)
}

/// The name of the place that holds `location`'s folder, as a menu row says
/// it: "Home", "Computer" for the root, "Trash" inside the Trash, else the
/// folder's own name.
fn place_name_above(location: &Location) -> Option<String> {
    let roots = super::named::Roots::current();
    let parent = roots.parent(&place_of(location))?;
    Some(roots.title(&parent))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::files::folder_table;
    use crate::nav::HISTORY_LIMIT;

    fn folder(path: &str) -> Where {
        Where::Folder(PathBuf::from(path))
    }

    fn model(at: &Where) -> NavModel {
        NavModel::new(Arc::new(folder_table()), &location_of(at))
    }

    fn trail(model: &NavModel) -> Trail {
        Trail::of(model)
    }

    #[test]
    fn a_new_history_has_the_start_and_nothing_else() {
        let model = model(&folder("/home/me"));
        assert_eq!(trail(&model), Trail { back: vec![], current: folder("/home/me"), forward: vec![] });
        assert!(!model.can_go_back() && !model.can_forward());
    }

    #[test]
    fn a_move_adds_an_entry_and_forgets_forward() {
        let mut model = model(&folder("/a"));
        visit(&mut model, &folder("/b"));
        visit(&mut model, &folder("/c"));
        model.back();
        model.back();
        assert_eq!(trail(&model).forward, vec![folder("/b"), folder("/c")]);
        visit(&mut model, &folder("/d"));
        assert_eq!(
            trail(&model),
            Trail { back: vec![folder("/a")], current: folder("/d"), forward: vec![] }
        );
    }

    #[test]
    fn going_where_you_are_records_nothing() {
        let mut model = model(&folder("/a/b"));
        assert_eq!(visit(&mut model, &folder("/a/b")), Outcome::Unchanged);
        assert_eq!(visit(&mut model, &folder("/a/b/")), Outcome::Unchanged);
        assert_eq!(visit(&mut model, &folder("/a//b/./")), Outcome::Unchanged);
        assert_eq!(visit(&mut model, &folder("/a/c/../b")), Outcome::Unchanged);
        assert!(trail(&model).back.is_empty());
    }

    #[test]
    fn going_where_you_are_keeps_forward() {
        let mut model = model(&folder("/a"));
        visit(&mut model, &folder("/b"));
        model.back();
        visit(&mut model, &folder("/a"));
        assert_eq!(trail(&model).forward, vec![folder("/b")]);
    }

    #[test]
    fn what_is_not_a_place_is_never_recorded() {
        let mut model = model(&folder("/a"));
        for text in ["", "--snapshot", "-h", "rel/path", ".", "..", "~", "~/Documents", "file:///b", "https://x"] {
            assert_eq!(visit(&mut model, &folder(text)), Outcome::Unchanged, "{text:?}");
        }
        assert_eq!(trail(&model), Trail { back: vec![], current: folder("/a"), forward: vec![] });
    }

    #[test]
    fn a_path_is_cleaned_by_name_and_links_are_not_followed() {
        let mut model = model(&folder("/a"));
        visit(&mut model, &folder("/x/y/../z//"));
        assert_eq!(trail(&model).current, folder("/x/z"));
        // Past the root, `..` stays at the root.
        visit(&mut model, &folder("/../../b"));
        assert_eq!(trail(&model).current, folder("/b"));
    }

    #[test]
    fn up_is_a_move_and_the_root_has_no_parent() {
        let mut model = model(&folder("/a/b"));
        up(&mut model);
        assert_eq!(trail(&model), Trail { back: vec![folder("/a/b")], current: folder("/a"), forward: vec![] });
        up(&mut model);
        assert_eq!(trail(&model).current, folder("/"));
        assert_eq!(up(&mut model), Outcome::Unchanged);
        assert_eq!(trail(&model).back.len(), 2);
        // Back from a parent returns to the child, with a Forward that goes up again.
        model.back();
        assert_eq!(trail(&model).current, folder("/a"));
    }

    #[test]
    fn the_recent_files_are_a_place_with_no_parent() {
        let mut model = model(&folder("/a"));
        visit(&mut model, &Where::Recent);
        assert_eq!(trail(&model).current, Where::Recent);
        assert_eq!(up(&mut model), Outcome::Unchanged);
        assert_eq!(visit(&mut model, &Where::Recent), Outcome::Unchanged);
        model.back();
        assert_eq!(trail(&model).current, folder("/a"));
    }

    #[test]
    fn the_trash_is_a_place_of_its_own_with_nothing_above_it() {
        let trash = bingux_files::ops::trash_folder().expect("a data home");
        let mut model = model(&folder("/a"));
        visit(&mut model, &Where::Folder(trash.clone()));
        // Whatever spelling of the real path reaches it, it is the one place.
        assert_eq!(visit(&mut model, &Where::Folder(trash.join("."))), Outcome::Unchanged);
        assert_eq!(visit(&mut model, &Where::Folder(trash.join("x").join(".."))), Outcome::Unchanged);
        // Up from the Trash does not climb into the bin it lives in.
        assert_eq!(up(&mut model), Outcome::Unchanged);
        assert_eq!(trail(&model).current, Where::Folder(trash.clone()));
        // Up from inside returns to the Trash, and Back to where Up was pressed.
        let inside = Where::Folder(trash.join("Old").join("sub"));
        visit(&mut model, &inside);
        up(&mut model);
        assert_eq!(trail(&model).current, Where::Folder(trash.join("Old")));
        up(&mut model);
        assert_eq!(trail(&model).current, Where::Folder(trash.clone()));
        assert_eq!(up(&mut model), Outcome::Unchanged);
    }

    #[test]
    fn the_menus_say_trash_and_not_the_folder_it_is_kept_in() {
        let trash = bingux_files::ops::trash_folder().expect("a data home");
        let mut model = model(&folder("/a"));
        visit(&mut model, &Where::Folder(trash.clone()));
        visit(&mut model, &Where::Folder(trash.join("Old")));
        visit(&mut model, &folder("/b"));
        let (back, forward) = menus(&model);
        let titles: Vec<_> = back.iter().map(|item| item.title.as_str()).collect();
        assert_eq!(titles, ["Old", "Trash", "a"]);
        assert!(forward.is_empty());
        // Two folders of one name are told apart by what holds them: in the
        // Trash that is "Trash".
        visit(&mut model, &Where::Folder(trash.join("b")));
        let (back, _) = menus(&model);
        let rows: Vec<_> = back.iter().map(|item| item.title.clone()).collect();
        assert!(rows.contains(&"b \u{2014} /".to_owned()) || rows.contains(&"b \u{2014} Computer".to_owned()), "{rows:?}");
        assert!(model.title(model.current()) == "b");
    }

    #[test]
    fn the_root_folder_is_a_place_like_any_other() {
        let mut model = model(&folder("/a"));
        visit(&mut model, &folder("/"));
        visit(&mut model, &folder("/b"));
        assert!(model.can_go_back());
        assert_eq!(model.back_menu().len(), 2, "the root is listed");
        model.back();
        assert_eq!(trail(&model).current, folder("/"));
        assert!(model.can_go_back(), "and Back goes on from it");
    }

    #[test]
    fn the_history_keeps_the_last_hundred_places() {
        let mut model = model(&folder("/start"));
        for index in 0..HISTORY_LIMIT + 40 {
            visit(&mut model, &folder(&format!("/n{index}")));
        }
        let trail = trail(&model);
        assert_eq!(trail.back.len(), HISTORY_LIMIT);
        assert_eq!(trail.current, folder(&format!("/n{}", HISTORY_LIMIT + 39)));
        assert_eq!(trail.back.first(), Some(&folder("/n39")), "the oldest are forgotten");
        assert_eq!(model.back_menu().len(), HISTORY_LIMIT);
    }

    #[test]
    fn going_back_and_forward_never_changes_the_length() {
        let mut model = model(&folder("/0"));
        for index in 1..30 {
            visit(&mut model, &folder(&format!("/{index}")));
        }
        for _ in 0..40 {
            model.back();
        }
        let all = |model: &NavModel| model.back_entries().len() + 1 + model.forward_entries().len();
        assert_eq!(all(&model), 30);
        for _ in 0..40 {
            model.forward();
        }
        assert_eq!(all(&model), 30);
    }

    #[test]
    fn a_menu_choice_is_one_move_across_the_entries_between() {
        let mut model = model(&folder("/0"));
        for index in 1..=5 {
            visit(&mut model, &folder(&format!("/{index}")));
        }
        let menu = model.back_menu();
        assert_eq!(menu[0].title, "4", "nearest first");
        assert_eq!(menu[4].title, "0");
        assert_eq!(menu[2].steps, 3);
        model.pop_to(menu[2].entry);
        let now = trail(&model);
        assert_eq!(now.current, folder("/2"));
        assert_eq!(now.forward, vec![folder("/3"), folder("/4"), folder("/5")]);
        let forward = model.forward_menu();
        model.forward_to(forward[1].entry);
        assert_eq!(trail(&model).current, folder("/4"));
        assert_eq!(trail(&model).forward, vec![folder("/5")]);
        assert_eq!(trail(&model).back.len(), 4);
    }

    #[test]
    fn folders_that_share_a_name_are_told_apart_by_their_parent() {
        let mut model = model(&folder("/data/kieran/Documents"));
        visit(&mut model, &folder("/srv/work/Documents"));
        visit(&mut model, &folder("/srv/music"));
        visit(&mut model, &folder("/srv/work/Documents"));
        visit(&mut model, &folder("/"));
        visit(&mut model, &folder("/srv/music/extra/Documents"));
        let (back, forward) = menus(&model);
        let titles: Vec<&str> = back.iter().map(|row| row.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "Computer",
                "Documents \u{2014} work",
                "music",
                "Documents \u{2014} work",
                "Documents \u{2014} kieran",
            ],
            "the same folder twice is one name; different folders with one name carry their parent"
        );
        assert!(forward.is_empty());
        // The row for the folder shown is not confusable with it.
        model.back();
        model.back();
        let (back, forward) = menus(&model);
        assert_eq!(back.len(), 3);
        assert_eq!(forward[0].title, "Computer");
    }
}
