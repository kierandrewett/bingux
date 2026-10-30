//! The context menu of a row of the places sidebar, and the keys that do the
//! same things.
//!
//! Two steps, as for the file views (`context_menu.rs`): a pure one decides
//! the rows from [`PlaceFacts`], which is what the tests read; the second turns
//! rows into [`MenuItem`]s whose actions send a [`PlaceCmd`] to one handler
//! (the sidebar's), so a menu row and its key cannot disagree.
//!
//! The rows are grouped as the list's menu is: open, then what changes the
//! place (rename, let go of a drive, empty the Trash), then the path and the
//! bookmark, then Properties last.
//!
//! | Place | Rows |
//! | --- | --- |
//! | Recent | Open |
//! | Home, a user folder | Open, Open in new tab, Copy path, Properties |
//! | Trash | Open, Open in new tab, Empty Trash (greyed while empty), Properties |
//! | Bookmark | Open, Open in new tab, Rename…, Copy path, Remove from bookmarks, Properties |
//! | Computer | Open, Open in new tab, Copy path, Properties |
//! | Mounted drive | Open, Open in new tab, Eject (then Unmount) or Unmount, Copy path, Add to or Remove from bookmarks, Properties |
//! | Drive not mounted | Mount |
//!
//! Open in new tab is there only where the window has tabs. Properties is the
//! Properties dialog of a folder in a list; Recent and a drive that is not
//! mounted have no folder, so no Properties.

use freya::prelude::*;

use super::directory::Where;
use super::places_model::{Item, ItemKind, Release, is_trash};
use crate::shell::{MenuItem, menu_item, menu_separator};

/// What a row of the sidebar is, as far as its menu is concerned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Subject {
    Recent,
    /// Home or a user folder.
    Folder,
    Trash,
    Bookmark,
    /// The root of the file system.
    Computer,
    /// A drive that is mounted.
    Drive,
    /// A drive that is plugged in and not mounted.
    Unmounted,
}

impl Subject {
    pub fn of(item: &Item) -> Subject {
        if item.volume.is_some() {
            return Subject::Unmounted;
        }
        match (&item.target, item.kind) {
            (Where::Recent, _) => Subject::Recent,
            _ if is_trash(item) => Subject::Trash,
            (_, ItemKind::Bookmark) => Subject::Bookmark,
            (Where::Folder(path), ItemKind::Drive) if path == std::path::Path::new("/") => {
                Subject::Computer
            }
            (_, ItemKind::Drive) => Subject::Drive,
            (_, ItemKind::Place) => Subject::Folder,
        }
    }

    /// There is a folder to open, copy the path of and show the properties
    /// of.
    fn has_folder(self) -> bool {
        !matches!(self, Subject::Recent | Subject::Unmounted)
    }
}

/// What the menu depends on.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct PlaceFacts {
    pub subject: Subject,
    /// The window has tabs to open a folder in.
    pub new_tab: bool,
    /// The folder is a bookmark now.
    pub bookmarked: bool,
    /// How the drive lets go, if it can.
    pub release: Option<Release>,
    /// The drive is being mounted or let go of.
    pub busy: bool,
    /// The owner can empty the Trash.
    pub can_empty_trash: bool,
    /// The Trash holds something.
    pub trash_full: bool,
}

impl PlaceFacts {
    /// The facts of `item`, which the caller knows more about than the model
    /// does (tabs, the Trash, a drive in use).
    pub fn of(
        item: &Item,
        new_tab: bool,
        release: Option<Release>,
        busy: bool,
        can_empty_trash: bool,
        trash_full: bool,
    ) -> PlaceFacts {
        PlaceFacts {
            subject: Subject::of(item),
            new_tab,
            bookmarked: item.bookmarked,
            release,
            busy,
            can_empty_trash,
            trash_full,
        }
    }
}

/// A thing a person can ask of a place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PlaceCmd {
    /// Go there; for a drive that is not mounted, mount it first.
    Open,
    OpenInNewTab,
    Rename,
    Release(Release),
    EmptyTrash,
    CopyPath,
    AddBookmark,
    RemoveBookmark,
    Properties,
}

/// One row of the menu.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct PlaceRow {
    pub label: &'static str,
    pub shortcut: Option<&'static str>,
    pub enabled: bool,
    pub destructive: bool,
    pub cmd: PlaceCmd,
}

impl PlaceRow {
    fn new(label: &'static str, cmd: PlaceCmd) -> PlaceRow {
        PlaceRow {
            label,
            shortcut: shortcut(cmd),
            enabled: true,
            destructive: false,
            cmd,
        }
    }

    fn enabled(self, enabled: bool) -> PlaceRow {
        PlaceRow { enabled, ..self }
    }
}

/// The shortcut shown beside a command: the one the sidebar answers to (see
/// [`key_command`]), written as the list's menu writes it.
fn shortcut(cmd: PlaceCmd) -> Option<&'static str> {
    match cmd {
        PlaceCmd::Open => Some("Enter"),
        PlaceCmd::Rename => Some("F2"),
        PlaceCmd::CopyPath => Some("Ctrl+Shift+C"),
        PlaceCmd::RemoveBookmark => Some("Delete"),
        PlaceCmd::Properties => Some("Alt+Enter"),
        _ => None,
    }
}

/// The rows, with `None` for a separator.
pub(super) fn place_plan(facts: &PlaceFacts) -> Vec<Option<PlaceRow>> {
    let subject = facts.subject;
    let has_folder = subject.has_folder();

    let open = vec![
        Some(PlaceRow::new(
            if subject == Subject::Unmounted { "Mount" } else { "Open" },
            PlaceCmd::Open,
        )
        .enabled(!(subject == Subject::Unmounted && facts.busy))),
        (facts.new_tab && has_folder).then(|| PlaceRow::new("Open in new tab", PlaceCmd::OpenInNewTab)),
    ];

    let mut change = Vec::new();
    if subject == Subject::Bookmark {
        change.push(Some(PlaceRow::new("Rename…", PlaceCmd::Rename)));
    }
    if let Some(release) = facts.release.filter(|_| matches!(subject, Subject::Drive | Subject::Computer)) {
        change.push(Some(
            PlaceRow::new(release.label(), PlaceCmd::Release(release)).enabled(!facts.busy),
        ));
        if release == Release::Eject {
            change.push(Some(
                PlaceRow::new(Release::Unmount.label(), PlaceCmd::Release(Release::Unmount))
                    .enabled(!facts.busy),
            ));
        }
    }
    if subject == Subject::Trash && facts.can_empty_trash {
        let mut row = PlaceRow::new("Empty Trash", PlaceCmd::EmptyTrash).enabled(facts.trash_full);
        row.destructive = facts.trash_full;
        change.push(Some(row));
    }

    let mut path = Vec::new();
    if has_folder && subject != Subject::Trash {
        path.push(Some(PlaceRow::new("Copy path", PlaceCmd::CopyPath)));
    }
    // A user folder is in the list already, the model hides a bookmark of
    // it, and a bookmark of the root would not show either.
    match subject {
        Subject::Bookmark | Subject::Drive | Subject::Computer if facts.bookmarked => {
            path.push(Some(PlaceRow::new("Remove from bookmarks", PlaceCmd::RemoveBookmark)));
        }
        Subject::Drive => {
            path.push(Some(PlaceRow::new("Add to bookmarks", PlaceCmd::AddBookmark)));
        }
        _ => {}
    }

    let properties = vec![has_folder.then(|| PlaceRow::new("Properties", PlaceCmd::Properties))];

    let mut rows = Vec::new();
    for group in [open, change, path, properties] {
        let group: Vec<PlaceRow> = group.into_iter().flatten().collect();
        if group.is_empty() {
            continue;
        }
        if !rows.is_empty() {
            rows.push(None);
        }
        rows.extend(group.into_iter().map(Some));
    }
    rows
}

/// Whether the menu offers `cmd` and it can be used now.
pub(super) fn offered(facts: &PlaceFacts, cmd: PlaceCmd) -> bool {
    place_plan(facts)
        .into_iter()
        .flatten()
        .any(|row| row.cmd == cmd && row.enabled)
}

/// The command a key on a focused row asks for. Enter, which opens, is the
/// row's own activation. Whether the row offers it is [`offered`].
pub(super) fn key_command(key: &Key, modifiers: Modifiers) -> Option<PlaceCmd> {
    let ctrl = modifiers.contains(Modifiers::CONTROL);
    let alt = modifiers.contains(Modifiers::ALT);
    let shift = modifiers.contains(Modifiers::SHIFT);
    match key {
        Key::Named(NamedKey::Enter) if alt && !ctrl && !shift => Some(PlaceCmd::Properties),
        Key::Named(NamedKey::F2) if !(ctrl || alt || shift) => Some(PlaceCmd::Rename),
        Key::Named(NamedKey::Delete) if !(ctrl || alt || shift) => Some(PlaceCmd::RemoveBookmark),
        Key::Character(text) if ctrl && shift && !alt && text.eq_ignore_ascii_case("c") => {
            Some(PlaceCmd::CopyPath)
        }
        _ => None,
    }
}

/// What a place's menu needs: the facts to choose its rows and the one
/// handler its rows and keys send commands to.
#[derive(Clone)]
pub(super) struct PlaceEnv {
    pub facts: PlaceFacts,
    pub act: EventHandler<PlaceCmd>,
}

/// The menu of a row of the places sidebar.
pub(super) fn place_menu(env: PlaceEnv) -> Vec<MenuItem> {
    place_plan(&env.facts)
        .into_iter()
        .map(|row| {
            let Some(row) = row else {
                return menu_separator();
            };
            let act = env.act.clone();
            let cmd = row.cmd;
            let mut entry = menu_item(row.label, move |_: ()| act.call(cmd));
            if let Some(shortcut) = row.shortcut {
                entry = entry.shortcut(shortcut);
            }
            if !row.enabled {
                entry = entry.disabled();
            }
            if row.destructive {
                entry = entry.destructive();
            }
            entry
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(subject: Subject) -> PlaceFacts {
        PlaceFacts {
            subject,
            new_tab: true,
            bookmarked: false,
            release: None,
            busy: false,
            can_empty_trash: true,
            trash_full: true,
        }
    }

    /// Labels in order, a separator as `---`, a greyed row with a `~`.
    fn shown(facts: &PlaceFacts) -> Vec<String> {
        place_plan(facts)
            .into_iter()
            .map(|row| match row {
                None => "---".to_owned(),
                Some(row) if row.enabled => row.label.to_owned(),
                Some(row) => format!("~{}", row.label),
            })
            .collect()
    }

    #[test]
    fn recent_only_opens() {
        assert_eq!(shown(&facts(Subject::Recent)), ["Open"]);
    }

    #[test]
    fn home_and_user_folders_open_copy_and_have_properties() {
        assert_eq!(
            shown(&facts(Subject::Folder)),
            ["Open", "Open in new tab", "---", "Copy path", "---", "Properties"]
        );
    }

    #[test]
    fn a_window_without_tabs_has_no_open_in_new_tab() {
        for subject in [Subject::Folder, Subject::Bookmark, Subject::Trash, Subject::Drive] {
            let rows = shown(&PlaceFacts { new_tab: false, ..facts(subject) });
            assert!(!rows.iter().any(|row| row == "Open in new tab"), "{subject:?}: {rows:?}");
        }
    }

    #[test]
    fn the_trash_empties_while_it_is_full_and_is_greyed_when_it_is_not() {
        assert_eq!(
            shown(&facts(Subject::Trash)),
            ["Open", "Open in new tab", "---", "Empty Trash", "---", "Properties"]
        );
        let empty = PlaceFacts { trash_full: false, ..facts(Subject::Trash) };
        assert_eq!(
            shown(&empty),
            ["Open", "Open in new tab", "---", "~Empty Trash", "---", "Properties"]
        );
        let row = place_plan(&facts(Subject::Trash)).into_iter().flatten().nth(2).unwrap();
        assert!(row.destructive && row.enabled);
        assert!(!offered(&empty, PlaceCmd::EmptyTrash));
        // Without a handler to empty it, the row is not there.
        let owner = PlaceFacts { can_empty_trash: false, ..facts(Subject::Trash) };
        assert_eq!(shown(&owner), ["Open", "Open in new tab", "---", "Properties"]);
    }

    #[test]
    fn a_bookmark_renames_removes_and_has_properties() {
        let bookmark = PlaceFacts { bookmarked: true, ..facts(Subject::Bookmark) };
        assert_eq!(
            shown(&bookmark),
            [
                "Open",
                "Open in new tab",
                "---",
                "Rename…",
                "---",
                "Copy path",
                "Remove from bookmarks",
                "---",
                "Properties"
            ]
        );
    }

    #[test]
    fn a_mounted_drive_lets_go_by_what_it_is() {
        let eject = PlaceFacts { release: Some(Release::Eject), ..facts(Subject::Drive) };
        assert_eq!(
            shown(&eject),
            [
                "Open",
                "Open in new tab",
                "---",
                "Eject",
                "Unmount",
                "---",
                "Copy path",
                "Add to bookmarks",
                "---",
                "Properties"
            ]
        );
        let unmount = PlaceFacts { release: Some(Release::Unmount), ..facts(Subject::Drive) };
        assert_eq!(
            shown(&unmount),
            [
                "Open",
                "Open in new tab",
                "---",
                "Unmount",
                "---",
                "Copy path",
                "Add to bookmarks",
                "---",
                "Properties"
            ]
        );
        // In use, they are greyed rather than gone.
        let busy = PlaceFacts { busy: true, ..eject };
        assert!(shown(&busy).contains(&"~Eject".to_owned()));
        assert!(shown(&busy).contains(&"~Unmount".to_owned()));
        assert!(!offered(&busy, PlaceCmd::Release(Release::Eject)));
    }

    #[test]
    fn a_drive_the_system_lives_on_has_nothing_to_let_go_of() {
        let system = facts(Subject::Drive);
        assert!(!shown(&system).iter().any(|row| row.contains("ject") || row.contains("Unmount")));
    }

    #[test]
    fn a_bookmarked_drive_offers_remove_and_computer_never_offers_add() {
        let drive = PlaceFacts { bookmarked: true, ..facts(Subject::Drive) };
        assert!(offered(&drive, PlaceCmd::RemoveBookmark));
        assert!(!offered(&drive, PlaceCmd::AddBookmark));
        // A bookmark of the root is hidden by the model, so it cannot be made.
        assert!(!offered(&facts(Subject::Computer), PlaceCmd::AddBookmark));
        assert_eq!(
            shown(&facts(Subject::Computer)),
            ["Open", "Open in new tab", "---", "Copy path", "---", "Properties"]
        );
    }

    #[test]
    fn a_drive_that_is_not_mounted_only_mounts() {
        assert_eq!(shown(&facts(Subject::Unmounted)), ["Mount"]);
        let busy = PlaceFacts { busy: true, ..facts(Subject::Unmounted) };
        assert_eq!(shown(&busy), ["~Mount"]);
    }

    #[test]
    fn properties_is_last_and_on_every_place_that_is_a_folder() {
        for subject in [
            Subject::Folder,
            Subject::Trash,
            Subject::Bookmark,
            Subject::Computer,
            Subject::Drive,
        ] {
            let rows = place_plan(&facts(subject));
            let last = rows.last().unwrap().as_ref().unwrap();
            assert_eq!((last.cmd, last.shortcut), (PlaceCmd::Properties, Some("Alt+Enter")));
            assert!(rows[rows.len() - 2].is_none(), "{subject:?}: a separator before Properties");
        }
        for subject in [Subject::Recent, Subject::Unmounted] {
            assert!(!offered(&facts(subject), PlaceCmd::Properties));
        }
    }

    #[test]
    fn no_separator_leads_trails_or_doubles() {
        for subject in [
            Subject::Recent,
            Subject::Folder,
            Subject::Trash,
            Subject::Bookmark,
            Subject::Computer,
            Subject::Drive,
            Subject::Unmounted,
        ] {
            for release in [None, Some(Release::Eject), Some(Release::Unmount)] {
                let rows = place_plan(&PlaceFacts { release, ..facts(subject) });
                assert!(rows.first().unwrap().is_some() && rows.last().unwrap().is_some());
                assert!(rows.windows(2).all(|pair| pair[0].is_some() || pair[1].is_some()));
            }
        }
    }

    #[test]
    fn a_shortcut_in_the_menu_is_a_key_the_sidebar_answers_to() {
        let keys = [
            ("Alt+Enter", Key::Named(NamedKey::Enter), Modifiers::ALT),
            ("F2", Key::Named(NamedKey::F2), Modifiers::empty()),
            ("Delete", Key::Named(NamedKey::Delete), Modifiers::empty()),
            (
                "Ctrl+Shift+C",
                Key::Character("C".into()),
                Modifiers::CONTROL | Modifiers::SHIFT,
            ),
        ];
        let release = Some(Release::Eject);
        for subject in [
            Subject::Folder,
            Subject::Trash,
            Subject::Bookmark,
            Subject::Computer,
            Subject::Drive,
        ] {
            let facts = PlaceFacts { release, bookmarked: true, ..facts(subject) };
            for row in place_plan(&facts).into_iter().flatten() {
                let Some(hint) = row.shortcut.filter(|hint| *hint != "Enter") else {
                    continue;
                };
                let (_, key, modifiers) = keys.iter().find(|(text, ..)| *text == hint).unwrap();
                assert_eq!(key_command(key, *modifiers), Some(row.cmd), "{hint}");
            }
        }
        // Alt+Enter is Properties, not Open.
        assert_eq!(key_command(&Key::Named(NamedKey::Enter), Modifiers::empty()), None);
    }

    #[test]
    fn menu_items_carry_the_plan() {
        let items = place_menu(PlaceEnv {
            facts: facts(Subject::Folder),
            act: EventHandler::new(|_: PlaceCmd| {}),
        });
        assert_eq!(items.len(), place_plan(&facts(Subject::Folder)).len());
        let labels: Vec<&str> = items
            .iter()
            .filter(|item| !item.separator)
            .map(|item| item.label.as_str())
            .collect();
        assert_eq!(labels, ["Open", "Open in new tab", "Copy path", "Properties"]);
        assert!(items.iter().any(|item| item.shortcut.as_deref() == Some("Alt+Enter")));
    }
}
