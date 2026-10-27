//! The context menus of the file views: a file or folder, several items,
//! empty space, the column header and a place of the sidebar.
//!
//! Each menu is built in two steps. A pure step decides which rows there are,
//! in the GNOME Files grouping, from [`Facts`] about the selection and the
//! view; it is what the tests read. The second step turns rows into
//! [`MenuItem`]s whose actions call [`actions::run`], so a menu and the
//! keyboard cannot disagree.
//!
//! The menus carry no icons: a menu row reserves its icon column only when it
//! has an icon, so a half-iconed menu would not line up, and the shortcut
//! hints are what the eye reads here.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use bingux_files::places::{SpecialFolder, system_user_dirs};
use bingux_files::{Entry, Place, PlaceKind, Sort, SortKey};
use freya::prelude::*;

use super::actions::{
    self, App, Availability, Capabilities, Command, Env, Facts, availability, shortcut,
};
use super::columns::columns_menu_items;
use super::directory::Where;
use super::zoom::{LADDER, ViewKind, kind_of};
use crate::shell::{MenuItem, Overlays, menu_item, menu_separator, submenu};


/// A change the menu makes to the view rather than to files.
#[derive(Clone, Debug, PartialEq)]
enum Act {
    Do(Command),
    SortKey(SortKey),
    Descending(bool),
    FoldersFirst,
    Show(ViewKind),
    /// The icon size, a level of the zoom ladder.
    IconSize(u16),
    ShowHidden(bool),
}

#[derive(Clone, Debug, PartialEq)]
struct Item {
    label: String,
    shortcut: Option<&'static str>,
    enabled: bool,
    destructive: bool,
    checked: Option<bool>,
    act: Act,
}

#[derive(Clone, Debug, PartialEq)]
enum Row {
    Item(Item),
    Sub(String, Vec<Row>),
    Separator,
}

impl Row {
    /// A row for `command` when it can be used on the selection. A command
    /// that cannot be used here at all is left out, not greyed.
    fn command(label: impl Into<String>, facts: &Facts, command: Command) -> Option<Row> {
        match availability(facts, &command) {
            Availability::Enabled => Self::command_greyed(label, facts, command),
            Availability::Disabled(_) | Availability::Hidden => None,
        }
    }

    /// A row for `command` that stays in the menu, greyed, while it cannot
    /// be used yet.
    fn command_greyed(label: impl Into<String>, facts: &Facts, command: Command) -> Option<Row> {
        let state = availability(facts, &command);
        if !state.is_shown() {
            return None;
        }
        Some(Row::Item(Item {
            label: label.into(),
            shortcut: shortcut(&command),
            enabled: state.is_enabled(),
            destructive: false,
            checked: None,
            act: Act::Do(command),
        }))
    }

    fn destructive(self) -> Row {
        match self {
            Row::Item(item) => Row::Item(Item {
                destructive: true,
                ..item
            }),
            other => other,
        }
    }

    fn choice(label: &str, checked: bool, act: Act) -> Row {
        Row::Item(Item {
            label: label.to_owned(),
            shortcut: None,
            enabled: true,
            destructive: false,
            checked: Some(checked),
            act,
        })
    }
}

/// Groups of rows with a separator between the groups that are not empty.
fn join(groups: Vec<Vec<Option<Row>>>) -> Vec<Row> {
    let mut rows = Vec::new();
    for group in groups {
        let group: Vec<Row> = group.into_iter().flatten().collect();
        if group.is_empty() {
            continue;
        }
        if !rows.is_empty() {
            rows.push(Row::Separator);
        }
        rows.extend(group);
    }
    rows
}

// ---------------------------------------------------------------------------
// The plans
// ---------------------------------------------------------------------------

/// Whether a menu offers Delete permanently. It is not in the ordinary menu,
/// where one slip would lose a file for good: a person opens the menu with
/// Shift held (Shift and right click, as in GNOME Files and Windows) to get
/// it. Shift+F10 and the Menu key are the keyboard's way to open a menu, so
/// Shift being down there says nothing; Shift+Delete is the keyboard's way to
/// this command. Either way the command asks before it deletes.
pub(super) fn permanent_delete_offered(shift_held: bool, from_keyboard: bool) -> bool {
    shift_held && !from_keyboard
}

/// The rows for a file, a folder, or several selected items. `folder` is the
/// one selected folder, for Paste into folder; `apps` open the one selected
/// file; `permanent` adds Delete permanently (see
/// [`permanent_delete_offered`]).
fn entries_plan(facts: &Facts, folder: Option<&Path>, apps: &[App], permanent: bool) -> Vec<Row> {
    // With several files selected there is nothing to offer, so the row is
    // left out rather than shown empty.
    let open_with = match availability(facts, &Command::OpenWith(placeholder_app())) {
        // Nothing opens the file, so there is no row to offer.
        Availability::Enabled if apps.is_empty() => None,
        Availability::Enabled => Some(Row::Sub(
            "Open with".to_owned(),
            apps.iter()
                .map(|app| {
                    Row::Item(Item {
                        label: app.name.clone(),
                        shortcut: None,
                        enabled: true,
                        destructive: false,
                        checked: None,
                        act: Act::Do(Command::OpenWith(app.clone())),
                    })
                })
                .collect(),
        )),
        Availability::Disabled(_) | Availability::Hidden => None,
    };
    let paste_into = folder.and_then(|path| {
        Row::command("Paste into folder", facts, Command::PasteInto(path.to_path_buf()))
    });
    join(vec![
        vec![
            Row::command("Open", facts, Command::Open),
            Row::command("Open in new window", facts, Command::OpenInNewWindow),
            open_with,
            Row::command("Preview", facts, Command::Preview),
        ],
        vec![
            Row::command("Cut", facts, Command::Cut),
            Row::command("Copy", facts, Command::Copy),
            paste_into,
        ],
        vec![
            Row::command("Rename", facts, Command::Rename),
            Row::command("Move to Trash", facts, Command::Trash),
            Row::command("Delete permanently", facts, Command::DeletePermanently)
                .filter(|_| permanent)
                .map(Row::destructive),
        ],
        vec![
            Row::command("Compress", facts, Command::Compress),
            Row::command("Extract", facts, Command::Extract),
            Row::command("Copy path", facts, Command::CopyPath),
            Row::command("Copy location as URI", facts, Command::CopyUri),
            Row::command("Add to bookmarks", facts, Command::AddBookmark),
            Row::command("Remove from bookmarks", facts, Command::RemoveBookmark),
        ],
        vec![Row::command("Properties", facts, Command::Properties)],
    ])
}

/// An application with nothing in it, to ask [`availability`] whether Open
/// with is on offer at all.
fn placeholder_app() -> App {
    App {
        name: String::new(),
        exec: String::new(),
        mimes: Vec::new(),
    }
}

/// How the view looks now, for the empty-space menu.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Looks {
    show_hidden: bool,
    kind: ViewKind,
    sort: Sort,
    /// The icon size now, one of [`LADDER`].
    icon_px: u16,
}

/// The choices of the Sort by submenu: the key and the direction are radio
/// groups, Folders first is a check.
fn sort_rows(sort: Sort) -> Vec<Row> {
    let key = |label: &str, key: SortKey| Row::choice(label, sort.key == key, Act::SortKey(key));
    vec![
        key("Name", SortKey::Name),
        key("Size", SortKey::Size),
        key("Type", SortKey::Kind),
        key("Modified", SortKey::Modified),
        Row::Separator,
        Row::choice("Ascending", !sort.descending, Act::Descending(false)),
        Row::choice("Descending", sort.descending, Act::Descending(true)),
        Row::Separator,
        Row::choice("Folders first", sort.folders_first, Act::FoldersFirst),
    ]
}

/// The icon sizes of the view kind on show, as radio choices. Zooming walks
/// one ladder (see `zoom.rs`): the list has the small sizes and the grid the
/// large ones, and picking one from the other kind's menu would change the
/// kind, which List and Grid above do.
fn size_rows(looks: Looks) -> Vec<Row> {
    LADDER
        .iter()
        .copied()
        .filter(|px| kind_of(*px) == looks.kind)
        .map(|px| Row::choice(&format!("{px} px"), px == looks.icon_px, Act::IconSize(px)))
        .collect()
}

/// The View submenu: how the files show, in the order of how often it is
/// changed. List or Grid, the icon size and the sort sit above the one switch.
fn view_rows(looks: Looks) -> Vec<Row> {
    vec![
        Row::choice("List", looks.kind == ViewKind::List, Act::Show(ViewKind::List)),
        Row::choice("Grid", looks.kind == ViewKind::Grid, Act::Show(ViewKind::Grid)),
        Row::Separator,
        Row::Sub("Icon size".to_owned(), size_rows(looks)),
        Row::Sub("Sort by".to_owned(), sort_rows(looks.sort)),
        Row::Separator,
        Row::Item(Item {
            label: "Show hidden files".to_owned(),
            shortcut: Some("Ctrl+H"),
            enabled: true,
            destructive: false,
            checked: Some(looks.show_hidden),
            act: Act::ShowHidden(!looks.show_hidden),
        }),
    ]
}

/// A file in the user's Templates folder.
#[derive(Clone, Debug, PartialEq)]
struct Template {
    /// The name without its extension.
    label: String,
    path: PathBuf,
}

/// The most templates the New file submenu lists.
const MAX_TEMPLATES: usize = 30;

/// The files of `dir` as templates, by name. Folders and hidden files are
/// not templates.
fn templates_in(dir: &Path) -> Vec<Template> {
    let Ok(listing) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<Template> = listing
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_name()?.to_string_lossy().into_owned();
            if name.starts_with('.') || !path.is_file() {
                return None;
            }
            let label = path.file_stem()?.to_string_lossy().into_owned();
            Some(Template { label, path })
        })
        .collect();
    found.sort_by_key(|template| (template.label.to_lowercase(), template.path.clone()));
    found.truncate(MAX_TEMPLATES);
    found
}

/// The templates of this system: the files of the XDG Templates folder, when
/// there is one.
fn user_templates() -> Vec<Template> {
    let dirs = system_user_dirs();
    dirs.path(SpecialFolder::Templates)
        .map(templates_in)
        .unwrap_or_default()
}

/// The New file row: plain when there are no templates, otherwise a submenu
/// of an empty file and each template.
fn new_file_row(facts: &Facts, templates: &[Template]) -> Option<Row> {
    if templates.is_empty() {
        return Row::command("New file", facts, Command::NewFile);
    }
    let empty = Row::command("Empty file", facts, Command::NewFile)?;
    let mut rows = vec![empty, Row::Separator];
    rows.extend(templates.iter().filter_map(|template| {
        Row::command(
            template.label.clone(),
            facts,
            Command::NewFromTemplate(template.path.clone()),
        )
    }));
    Some(Row::Sub("New file".to_owned(), rows))
}

/// The menu of empty space, grouped by task: make something, paste and take
/// back, select, change the view, then refresh and properties. A row that
/// cannot apply here (nothing to select in an empty folder, Create in Recent,
/// Undo with nothing done) is left out rather than greyed; Paste stays in a
/// folder, greyed until something is copied.
fn empty_plan(facts: &Facts, looks: Looks, templates: &[Template]) -> Vec<Row> {
    let paste = facts
        .in_folder
        .then(|| Row::command_greyed("Paste", facts, Command::Paste))
        .flatten();
    let undo = facts
        .undo
        .and_then(|label| Row::command(label, facts, Command::Undo));
    join(vec![
        vec![
            Row::command("New folder", facts, Command::NewFolder),
            new_file_row(facts, templates),
        ],
        vec![paste, undo],
        vec![
            Row::command("Select all", facts, Command::SelectAll),
            Row::command("Invert selection", facts, Command::InvertSelection),
        ],
        vec![Some(Row::Sub("View".to_owned(), view_rows(looks)))],
        vec![Row::command("Empty Trash", facts, Command::EmptyTrash).map(Row::destructive)],
        vec![
            Row::command("Refresh", facts, Command::Refresh),
            Row::command("Properties", facts, Command::Properties),
        ],
    ])
}

/// The sort after choosing `act`, for the menu items that change the sort.
fn sorted(sort: Sort, act: &Act) -> Sort {
    match act {
        Act::SortKey(key) if sort.key == *key => sort,
        Act::SortKey(key) => Sort {
            key: *key,
            descending: false,
            ..sort
        },
        Act::Descending(descending) => Sort {
            descending: *descending,
            ..sort
        },
        Act::FoldersFirst => Sort {
            folders_first: !sort.folders_first,
            ..sort
        },
        _ => sort,
    }
}

// ---------------------------------------------------------------------------
// Rows to menu items
// ---------------------------------------------------------------------------

/// What the menu items need to run their action.
#[derive(Clone)]
struct Target {
    env: Env,
    selected: Arc<Vec<Entry>>,
}

fn realise(target: &Target, rows: Vec<Row>) -> Vec<MenuItem> {
    rows.into_iter()
        .map(|row| match row {
            Row::Separator => menu_separator(),
            Row::Sub(label, rows) => submenu(label, realise(target, rows)),
            Row::Item(item) => {
                let Item {
                    label,
                    shortcut,
                    enabled,
                    destructive,
                    checked,
                    act,
                } = item;
                let target = target.clone();
                let mut entry = menu_item(label, move |_| perform(&target, &act));
                if let Some(shortcut) = shortcut {
                    entry = entry.shortcut(shortcut);
                }
                if let Some(checked) = checked {
                    entry = entry.checked(checked);
                }
                if !enabled {
                    entry = entry.disabled();
                }
                if destructive {
                    entry = entry.destructive();
                }
                entry
            }
        })
        .collect()
}

fn perform(target: &Target, act: &Act) {
    let env = &target.env;
    match act {
        Act::Do(command) => actions::run(env, &target.selected, command.clone()),
        Act::SortKey(_) | Act::Descending(_) | Act::FoldersFirst => {
            env.view.set_sort.call(sorted(env.view.sort, act));
        }
        Act::Show(kind) => env.view.zoom.show(*kind),
        Act::IconSize(px) => env.view.zoom.set_px(*px),
        Act::ShowHidden(show) => env.view.set_show_hidden.call(*show),
    }
}

fn looks_of(env: &Env) -> Looks {
    Looks {
        show_hidden: env.view.show_hidden,
        kind: env.view.zoom.kind(),
        sort: env.view.sort,
        icon_px: env.view.zoom.px_now(),
    }
}

// ---------------------------------------------------------------------------
// The menus
// ---------------------------------------------------------------------------

/// The menu of one item or several: the commands that apply to them.
/// `shift_held` is whether Shift was down when the menu was asked for, and
/// `keyboard` whether it came from the Menu key or Shift+F10.
pub(super) fn menu_for_entries(
    env: &Env,
    selected: &[Entry],
    shift_held: bool,
    keyboard: bool,
) -> Vec<MenuItem> {
    let facts = Facts::of(env, selected, true);
    let folder = match selected {
        [entry] if entry.is_dir() => Some(entry.path.as_path()),
        _ => None,
    };
    let apps = match selected {
        [entry] if !entry.is_dir() => actions::apps_for(entry.mime.mime),
        _ => Vec::new(),
    };
    let target = Target {
        env: env.clone(),
        selected: Arc::new(selected.to_vec()),
    };
    let permanent = permanent_delete_offered(shift_held, keyboard);
    realise(&target, entries_plan(&facts, folder, &apps, permanent))
}

/// The menu of empty space in the list or grid.
pub(super) fn menu_for_empty(env: &Env) -> Vec<MenuItem> {
    let facts = Facts::of(env, &[], true);
    let target = Target {
        env: env.clone(),
        selected: Arc::new(Vec::new()),
    };
    realise(
        &target,
        empty_plan(&facts, looks_of(env), &user_templates()),
    )
}

/// The menu of a column header: which columns show, then how to sort.
pub(super) fn menu_for_header(env: &Env) -> Vec<MenuItem> {
    let target = Target {
        env: env.clone(),
        selected: Arc::new(Vec::new()),
    };
    let mut items = columns_menu_items(env.view.layout, env.folder.is_none());
    items.push(menu_separator());
    items.push(submenu(
        "Sort by",
        realise(&target, sort_rows(env.view.sort)),
    ));
    items
}

// ---------------------------------------------------------------------------
// Places
// ---------------------------------------------------------------------------

/// What a place's menu needs from the sidebar.
#[derive(Clone)]
pub struct PlaceEnv {
    pub overlays: Overlays,
    pub caps: Capabilities,
    pub on_open: EventHandler<Where>,
    pub on_open_new_window: EventHandler<Where>,
    pub on_remove_bookmark: EventHandler<PathBuf>,
    pub on_properties: EventHandler<PathBuf>,
}

#[derive(Clone, Debug, PartialEq)]
enum PlaceAct {
    Open,
    OpenNewWindow,
    RemoveBookmark,
    Properties,
}

/// The rows of a place's menu, as `(label, action)` with separators as
/// `None`.
fn place_plan(caps: Capabilities, kind: PlaceKind, has_folder: bool) -> Vec<Option<(&'static str, PlaceAct)>> {
    let mut groups: Vec<Vec<(&'static str, PlaceAct)>> = vec![vec![("Open", PlaceAct::Open)]];
    if caps.new_window && has_folder {
        groups[0].push(("Open in new window", PlaceAct::OpenNewWindow));
    }
    if caps.bookmarks && kind == PlaceKind::Bookmark && has_folder {
        groups.push(vec![("Remove from bookmarks", PlaceAct::RemoveBookmark)]);
    }
    if caps.properties && has_folder {
        groups.push(vec![("Properties", PlaceAct::Properties)]);
    }
    let mut rows = Vec::new();
    for group in groups {
        if !rows.is_empty() {
            rows.push(None);
        }
        rows.extend(group.into_iter().map(Some));
    }
    rows
}

/// The menu of a row of the places sidebar. A volume's Unmount and Eject are
/// added by the caller with [`place_menu_with`].
pub fn place_menu(env: PlaceEnv, place: &Place) -> Vec<MenuItem> {
    place_menu_with(env, place, Vec::new())
}

/// [`place_menu`] with `extra` items after the standard ones, behind a
/// separator: the extension point for what only some places have, such as
/// Unmount for a mounted volume.
pub fn place_menu_with(env: PlaceEnv, place: &Place, extra: Vec<MenuItem>) -> Vec<MenuItem> {
    let target = match &place.path {
        Some(path) => Where::Folder(path.clone()),
        None => Where::Recent,
    };
    let mut items: Vec<MenuItem> = place_plan(env.caps, place.kind, place.path.is_some())
        .into_iter()
        .map(|row| {
            let Some((label, act)) = row else {
                return menu_separator();
            };
            let env = env.clone();
            let target = target.clone();
            let path = place.path.clone();
            menu_item(label, move |_| match (&act, &path) {
                (PlaceAct::Open, _) => env.on_open.call(target.clone()),
                (PlaceAct::OpenNewWindow, _) => env.on_open_new_window.call(target.clone()),
                (PlaceAct::RemoveBookmark, Some(path)) => env.on_remove_bookmark.call(path.clone()),
                (PlaceAct::Properties, Some(path)) => env.on_properties.call(path.clone()),
                _ => {}
            })
        })
        .collect();
    if !extra.is_empty() {
        items.push(menu_separator());
        items.extend(extra);
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(caps: Capabilities, selected: usize, folders: usize) -> Facts {
        Facts {
            caps,
            in_folder: true,
            selected,
            folders,
            can_paste: true,
            bookmarked: false,
            extractable: false,
            can_compress: true,
            shown: 10,
            undo: None,
            in_trash: false,
        }
    }

    /// The labels of the top level, `---` for a separator.
    fn labels(rows: &[Row]) -> Vec<String> {
        rows.iter()
            .map(|row| match row {
                Row::Item(item) => item.label.clone(),
                Row::Sub(label, _) => label.clone(),
                Row::Separator => "---".to_owned(),
            })
            .collect()
    }

    fn app(name: &str) -> App {
        App {
            name: name.to_owned(),
            exec: "x".to_owned(),
            mimes: Vec::new(),
        }
    }

    /// The ordinary menu: what a plain right click gives.
    fn ordinary(facts: &Facts, folder: Option<&Path>, apps: &[App]) -> Vec<Row> {
        entries_plan(facts, folder, apps, false)
    }

    /// The menu of a Shift+right click.
    fn shifted(facts: &Facts) -> Vec<Row> {
        entries_plan(facts, None, &[], true)
    }

    #[test]
    fn one_file_in_the_files_app() {
        let rows = ordinary(&facts(Capabilities::FILES_APP, 1, 0), None, &[app("Viewer")]);
        assert_eq!(
            labels(&rows),
            [
                "Open", "Open with", "Preview", "---", "Cut", "Copy", "---", "Rename", "Move to Trash",
                "---", "Compress", "Copy path", "Copy location as URI", "---", "Properties",
            ]
        );
        let Row::Sub(_, apps) = &rows[1] else {
            panic!("a submenu");
        };
        assert_eq!(labels(apps), ["Viewer"]);
    }

    #[test]
    fn one_folder_in_the_files_app() {
        let rows = ordinary(
            &facts(Capabilities::FILES_APP, 1, 1),
            Some(Path::new("/home/a")),
            &[],
        );
        let labels = labels(&rows);
        for wanted in [
            "Open",
            "Open in new window",
            "Paste into folder",
            "Add to bookmarks",
            "Properties",
        ] {
            assert!(labels.iter().any(|label| label == wanted), "{wanted}");
        }
        assert!(!labels.iter().any(|label| label.starts_with("Open with")));
        assert!(!labels.iter().any(|label| label == "Remove from bookmarks"));
    }

    #[test]
    fn a_bookmarked_folder_offers_removal() {
        let mut bookmarked = facts(Capabilities::FILES_APP, 1, 1);
        bookmarked.bookmarked = true;
        let rows = ordinary(&bookmarked, None, &[]);
        let labels = labels(&rows);
        assert!(labels.iter().any(|label| label == "Remove from bookmarks"));
        assert!(!labels.iter().any(|label| label == "Add to bookmarks"));
    }

    #[test]
    fn several_items_get_the_applicable_subset() {
        let rows = ordinary(&facts(Capabilities::FILES_APP, 3, 1), None, &[]);
        let labels = labels(&rows);
        assert!(!labels.iter().any(|label| label == "Rename"));
        assert!(!labels.iter().any(|label| label == "Add to bookmarks"));
        for wanted in ["Cut", "Copy", "Move to Trash", "Compress", "Properties"] {
            assert!(labels.iter().any(|label| label == wanted), "{wanted}");
        }
    }

    #[test]
    fn a_chooser_leaves_out_what_leaves_the_dialog() {
        let file = labels(&ordinary(&facts(Capabilities::CHOOSER, 1, 0), None, &[]));
        assert!(file.iter().any(|label| label == "Rename"));
        assert!(!file.iter().any(|label| label.starts_with("Open with")));
        let folder = labels(&ordinary(&facts(Capabilities::CHOOSER, 1, 1), None, &[]));
        assert!(!folder.iter().any(|label| label == "Open in new window"));
        assert!(folder.iter().any(|label| label == "Add to bookmarks"));
    }

    #[test]
    fn a_read_only_view_keeps_only_the_harmless() {
        let caps = Capabilities {
            modify: false,
            ..Capabilities::FILES_APP
        };
        for rows in [
            ordinary(&facts(caps, 1, 0), None, &[]),
            shifted(&facts(caps, 1, 0)),
        ] {
            let labels = labels(&rows);
            for gone in ["Cut", "Rename", "Move to Trash", "Delete permanently", "Compress"] {
                assert!(!labels.iter().any(|label| label == gone), "{gone}");
            }
            assert!(labels.iter().any(|label| label == "Copy"));
        }
    }

    /// The labels of every item of a menu, flattened, for "is this offered?".
    fn offers(rows: &[Row], wanted: &str) -> bool {
        rows.iter().any(|row| match row {
            Row::Item(item) => item.label == wanted,
            Row::Sub(_, inner) => offers(inner, wanted),
            Row::Separator => false,
        })
    }

    #[test]
    fn delete_permanently_is_not_in_the_ordinary_menu() {
        for (selected, folders) in [(1, 0), (1, 1), (3, 1)] {
            let facts = facts(Capabilities::FILES_APP, selected, folders);
            assert!(!offers(&ordinary(&facts, None, &[]), "Delete permanently"));
            assert!(offers(&ordinary(&facts, None, &[]), "Move to Trash"));
        }
    }

    #[test]
    fn delete_permanently_appears_with_shift_and_is_the_destructive_one() {
        let rows = shifted(&facts(Capabilities::FILES_APP, 1, 0));
        assert!(offers(&rows, "Move to Trash"), "Trash stays the first choice");
        let destructive: Vec<String> = rows
            .iter()
            .filter_map(|row| match row {
                Row::Item(item) if item.destructive => Some(item.label.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(destructive, ["Delete permanently"]);
        let labels = labels(&rows);
        let trash = labels.iter().position(|l| l == "Move to Trash").unwrap();
        assert_eq!(labels[trash + 1], "Delete permanently", "next to Trash, after it");
    }

    #[test]
    fn shift_opens_it_for_the_pointer_but_not_for_the_keyboard_menu() {
        assert!(permanent_delete_offered(true, false), "Shift + right click");
        assert!(!permanent_delete_offered(false, false), "right click");
        // Shift+F10 opens a menu from the keyboard with Shift down.
        assert!(!permanent_delete_offered(true, true), "Shift+F10");
        assert!(!permanent_delete_offered(false, true), "the Menu key");
    }

    #[test]
    fn empty_trash_is_offered_in_the_trash_only() {
        let mut in_trash = facts(Capabilities::FILES_APP, 0, 0);
        assert!(!offers(&plan(&in_trash), "Empty Trash"));
        in_trash.in_trash = true;
        let rows = plan(&in_trash);
        assert!(offers(&rows, "Empty Trash"));
        let destructive: Vec<String> = rows
            .iter()
            .filter_map(|row| match row {
                Row::Item(item) if item.destructive => Some(item.label.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(destructive, ["Empty Trash"]);
    }

    #[test]
    fn shortcuts_are_shown() {
        let rows = shifted(&facts(Capabilities::FILES_APP, 1, 0));
        let hint = |wanted: &str| {
            rows.iter().find_map(|row| match row {
                Row::Item(item) if item.label == wanted => item.shortcut,
                _ => None,
            })
        };
        assert_eq!(hint("Rename"), Some("F2"));
        assert_eq!(hint("Move to Trash"), Some("Delete"));
        assert_eq!(hint("Copy"), Some("Ctrl+C"));
        assert_eq!(hint("Delete permanently"), Some("Shift+Delete"));
    }

    fn looks() -> Looks {
        Looks {
            show_hidden: false,
            kind: ViewKind::List,
            sort: Sort::default(),
            icon_px: 16,
        }
    }

    fn plan(facts: &Facts) -> Vec<Row> {
        empty_plan(facts, looks(), &[])
    }

    /// The rows of the submenu called `label`.
    fn submenu_of<'a>(rows: &'a [Row], label: &str) -> &'a [Row] {
        rows.iter()
            .find_map(|row| match row {
                Row::Sub(name, rows) if name == label => Some(rows.as_slice()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("a submenu called {label}"))
    }

    /// `(label, checked)` of every row that is an item.
    fn ticks(rows: &[Row]) -> Vec<(String, Option<bool>)> {
        rows.iter()
            .filter_map(|row| match row {
                Row::Item(item) => Some((item.label.clone(), item.checked)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn empty_space_menu_is_grouped_by_task() {
        let rows = plan(&facts(Capabilities::FILES_APP, 0, 0));
        assert_eq!(
            labels(&rows),
            [
                "New folder", "New file", "---", "Paste", "---", "Select all", "Invert selection",
                "---", "View", "---", "Refresh", "Properties",
            ]
        );
    }

    #[test]
    fn empty_space_menu_has_no_undo_until_there_is_something_to_undo() {
        let none = labels(&plan(&facts(Capabilities::FILES_APP, 0, 0)));
        assert!(!none.iter().any(|label| label.starts_with("Undo")));
        let mut with_undo = facts(Capabilities::FILES_APP, 0, 0);
        with_undo.undo = Some("Undo Rename");
        let rows = plan(&with_undo);
        assert_eq!(
            labels(&rows)[3..5],
            ["Paste".to_owned(), "Undo Rename".to_owned()]
        );
        let hint = rows.iter().find_map(|row| match row {
            Row::Item(item) if item.label == "Undo Rename" => Some((item.shortcut, item.enabled)),
            _ => None,
        });
        assert_eq!(hint, Some((Some("Ctrl+Z"), true)));
    }

    #[test]
    fn paste_stays_in_a_folder_greyed_until_something_is_copied() {
        let mut empty_clipboard = facts(Capabilities::FILES_APP, 0, 0);
        empty_clipboard.can_paste = false;
        let rows = plan(&empty_clipboard);
        let paste = rows.iter().find_map(|row| match row {
            Row::Item(item) if item.label == "Paste" => Some(item.enabled),
            _ => None,
        });
        assert_eq!(paste, Some(false));
    }

    #[test]
    fn empty_space_menu_in_recent_leaves_out_what_cannot_apply() {
        let mut recent = facts(Capabilities::FILES_APP, 0, 0);
        recent.in_folder = false;
        assert_eq!(
            labels(&plan(&recent)),
            ["Select all", "Invert selection", "---", "View", "---", "Refresh"]
        );
    }

    #[test]
    fn an_empty_folder_has_nothing_to_select() {
        let mut empty = facts(Capabilities::FILES_APP, 0, 0);
        empty.shown = 0;
        let labels = labels(&plan(&empty));
        assert!(!labels.iter().any(|label| label == "Select all"));
        assert!(!labels.iter().any(|label| label == "Invert selection"));
        assert!(labels.iter().any(|label| label == "New folder"));
    }

    #[test]
    fn a_view_that_cannot_modify_has_no_create_or_paste() {
        let caps = Capabilities {
            modify: false,
            ..Capabilities::FILES_APP
        };
        assert_eq!(
            labels(&plan(&facts(caps, 0, 0))),
            ["Select all", "Invert selection", "---", "View", "---", "Refresh", "Properties"]
        );
    }

    #[test]
    fn new_file_is_plain_without_templates_and_a_submenu_with_them() {
        let rows = plan(&facts(Capabilities::FILES_APP, 0, 0));
        assert!(matches!(&rows[1], Row::Item(item) if item.label == "New file"));
        let templates = [
            Template {
                label: "Letter".to_owned(),
                path: PathBuf::from("/t/Letter.odt"),
            },
            Template {
                label: "Notes".to_owned(),
                path: PathBuf::from("/t/Notes.md"),
            },
        ];
        let rows = empty_plan(&facts(Capabilities::FILES_APP, 0, 0), looks(), &templates);
        assert_eq!(labels(&rows)[..2], ["New folder".to_owned(), "New file".to_owned()]);
        let new_file = submenu_of(&rows, "New file");
        assert_eq!(labels(new_file), ["Empty file", "---", "Letter", "Notes"]);
        let acts: Vec<&Act> = new_file
            .iter()
            .filter_map(|row| match row {
                Row::Item(item) => Some(&item.act),
                _ => None,
            })
            .collect();
        assert_eq!(
            acts,
            [
                &Act::Do(Command::NewFile),
                &Act::Do(Command::NewFromTemplate(PathBuf::from("/t/Letter.odt"))),
                &Act::Do(Command::NewFromTemplate(PathBuf::from("/t/Notes.md"))),
            ]
        );
    }

    #[test]
    fn new_file_is_left_out_where_nothing_can_be_made_even_with_templates() {
        let mut recent = facts(Capabilities::FILES_APP, 0, 0);
        recent.in_folder = false;
        let templates = [Template {
            label: "Letter".to_owned(),
            path: PathBuf::from("/t/Letter.odt"),
        }];
        let rows = empty_plan(&recent, looks(), &templates);
        assert!(!labels(&rows).iter().any(|label| label == "New file"));
    }

    #[test]
    fn templates_are_the_visible_files_of_the_folder_by_name() {
        let dir = std::env::temp_dir().join(format!("bingux-templates-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("a folder")).unwrap();
        for name in ["b letter.odt", "A notes.md", ".hidden"] {
            std::fs::write(dir.join(name), "").unwrap();
        }
        let found = templates_in(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        let names: Vec<&str> = found.iter().map(|t| t.label.as_str()).collect();
        assert_eq!(names, ["A notes", "b letter"]);
        assert!(templates_in(&dir).is_empty(), "a missing folder has none");
    }

    #[test]
    fn there_is_one_view_submenu_holding_the_view_choices() {
        let rows = plan(&facts(Capabilities::FILES_APP, 0, 0));
        let view = submenu_of(&rows, "View");
        assert_eq!(
            labels(view),
            ["List", "Grid", "---", "Icon size", "Sort by", "---", "Show hidden files"]
        );
        assert_eq!(
            ticks(view),
            [
                ("List".to_owned(), Some(true)),
                ("Grid".to_owned(), Some(false)),
                ("Show hidden files".to_owned(), Some(false)),
            ]
        );
        // Nothing about the view is in the top level.
        let top = labels(&rows);
        for gone in ["Sort by", "View as", "Zoom", "Show hidden files"] {
            assert!(!top.iter().any(|label| label == gone), "{gone}");
        }
    }

    #[test]
    fn icon_sizes_follow_the_zoom_ladder_of_the_current_view() {
        let sizes = |kind, px| {
            let rows = size_rows(Looks {
                kind,
                icon_px: px,
                ..looks()
            });
            ticks(&rows)
        };
        let list: Vec<(String, Option<bool>)> = [("16 px", true), ("20 px", false), ("24 px", false), ("32 px", false)]
            .map(|(label, on)| (label.to_owned(), Some(on)))
            .to_vec();
        assert_eq!(sizes(ViewKind::List, 16), list);
        let grid = sizes(ViewKind::Grid, 128);
        assert_eq!(
            grid.iter().map(|(label, _)| label.as_str()).collect::<Vec<_>>(),
            ["48 px", "64 px", "96 px", "128 px", "192 px"]
        );
        assert_eq!(
            grid.iter().filter(|(_, on)| *on == Some(true)).map(|(label, _)| label.as_str()).collect::<Vec<_>>(),
            ["128 px"]
        );
    }

    #[test]
    fn sort_submenu_ticks_the_current_order() {
        let rows = sort_rows(Sort {
            key: SortKey::Size,
            descending: true,
            folders_first: false,
        });
        assert_eq!(
            labels(&rows),
            [
                "Name", "Size", "Type", "Modified", "---", "Ascending", "Descending", "---",
                "Folders first",
            ]
        );
        assert_eq!(
            ticks(&rows),
            [
                ("Name".to_owned(), Some(false)),
                ("Size".to_owned(), Some(true)),
                ("Type".to_owned(), Some(false)),
                ("Modified".to_owned(), Some(false)),
                ("Ascending".to_owned(), Some(false)),
                ("Descending".to_owned(), Some(true)),
                ("Folders first".to_owned(), Some(false)),
            ]
        );
    }

    #[test]
    fn choosing_sort_items_changes_the_sort() {
        let sort = Sort::default();
        assert_eq!(sorted(sort, &Act::SortKey(SortKey::Size)).key, SortKey::Size);
        assert!(sorted(sort, &Act::Descending(true)).descending);
        assert!(!sorted(sort, &Act::FoldersFirst).folders_first);
        // A new key starts ascending.
        let descending = Sort {
            descending: true,
            ..sort
        };
        assert!(!sorted(descending, &Act::SortKey(SortKey::Modified)).descending);
        assert!(sorted(descending, &Act::SortKey(SortKey::Name)).descending);
        assert!(!sorted(descending, &Act::Descending(false)).descending);
    }

    fn place(kind: PlaceKind, folder: bool) -> Vec<String> {
        place_plan(Capabilities::FILES_APP, kind, folder)
            .into_iter()
            .map(|row| row.map_or_else(|| "---".to_owned(), |(label, _)| label.to_owned()))
            .collect()
    }

    #[test]
    fn place_menus() {
        assert_eq!(place(PlaceKind::Recent, false), ["Open"]);
        assert_eq!(
            place(PlaceKind::Home, true),
            ["Open", "Open in new window", "---", "Properties"]
        );
        assert_eq!(
            place(PlaceKind::Bookmark, true),
            [
                "Open",
                "Open in new window",
                "---",
                "Remove from bookmarks",
                "---",
                "Properties"
            ]
        );
        let chooser = place_plan(Capabilities::CHOOSER, PlaceKind::Bookmark, true);
        assert!(chooser.iter().flatten().all(|(label, _)| *label != "Open in new window"));
    }
}
