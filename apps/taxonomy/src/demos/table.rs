//! The data table, the list view and the toolbar.

use std::rc::Rc;

use bingux_ui::prelude::*;
use freya::prelude::*;

use crate::model::Spec;

/// A row of the sample tables: an installed app.
#[derive(Clone, Debug, PartialEq)]
pub struct AppRow {
    id: u32,
    name: String,
    kind: &'static str,
    bytes: u64,
    opened: &'static str,
}

const NAMES: [&str; 12] = [
    "Calculator",
    "Calendar",
    "Camera",
    "Clocks",
    "Contacts",
    "Files",
    "Maps",
    "Music",
    "Notes",
    "Photos",
    "Terminal",
    "Weather",
];
const KINDS: [&str; 4] = ["Utility", "Productivity", "Media", "System"];
const OPENED: [&str; 5] = ["Today", "Yesterday", "This week", "Last month", "Never"];

fn apps(count: u32) -> Vec<AppRow> {
    (0..count)
        .map(|id| {
            let at = id as usize;
            let round = at / NAMES.len();
            AppRow {
                id,
                name: if round == 0 {
                    NAMES[at % NAMES.len()].to_string()
                } else {
                    format!("{} {}", NAMES[at % NAMES.len()], round + 1)
                },
                kind: KINDS[(at * 7) % KINDS.len()],
                bytes: 4_000_000 + (u64::from(id) * 48_271_913) % 2_400_000_000,
                opened: OPENED[(at * 3) % OPENED.len()],
            }
        })
        .collect()
}

/// `1.2 GB`, `340 MB`: the size as a person reads it.
fn size_text(bytes: u64) -> String {
    let value = bytes as f64;
    if value >= 1e9 {
        format!("{:.1} GB", value / 1e9)
    } else {
        format!("{:.0} MB", value / 1e6)
    }
}

fn app_columns() -> Rc<Vec<Column<AppRow>>> {
    Rc::new(vec![
        Column::text("name", "Name", |app: &AppRow| app.name.clone())
            .flex()
            .min_width(140.0)
            .priority(0)
            .sort_by_text()
            .editable(),
        Column::text("kind", "Kind", |app: &AppRow| app.kind.to_string())
            .width(120.0)
            .priority(3)
            .muted()
            .sort_by_text(),
        Column::text("size", "Size", |app: &AppRow| size_text(app.bytes))
            .width(96.0)
            .numeric()
            .priority(1)
            .sort_by_key(|app| app.bytes),
        Column::text("opened", "Last opened", |app: &AppRow| {
            app.opened.to_string()
        })
        .width(130.0)
        .priority(2)
        .muted()
        .hidden_by_default()
        .sort_by_text(),
    ])
}

const TABLE_HEIGHT: f32 = 460.0;

pub const TABLE: &[Spec] = &[
    Spec::live("Sort, resize, reorder and select", || {
        let rows = use_hook(|| Rc::new(apps(40)));
        let columns = use_hook(app_columns);
        let selection = use_state(|| Selection::of([2_u32]));
        data_table(&rows, columns, |app: &AppRow| app.id)
            .id("taxonomy.apps")
            .label("Installed apps")
            .selection(selection)
            .sorted_by("name", SortDirection::Ascending)
            .on_activate(|_: u32| {})
            .into_element()
    })
    .page(TABLE_HEIGHT)
    .script("wait 400; click-text Size; wait 300; save {out}"),
    Spec::live("A hundred thousand rows", || {
        let rows = use_hook(|| Rc::new(apps(100_000)));
        let columns = use_hook(app_columns);
        data_table(&rows, columns, |app: &AppRow| app.id)
            .label("Every app")
            .sorted_by("size", SortDirection::Descending)
            .into_element()
    })
    .page(TABLE_HEIGHT)
    .script("wait 600; scroll 300,300,-40000; wait 400; save {out}"),
    Spec::live("Marks, batch actions and a row menu", || {
        let rows = use_state(|| Rc::new(apps(14)));
        let columns = use_hook(app_columns);
        let selection = use_state(|| Selection::of([1_u32, 3, 4]));
        let remove = move |keys: Vec<u32>| {
            let mut rows = rows;
            let kept: Vec<AppRow> = rows
                .peek()
                .iter()
                .filter(|app| !keys.contains(&app.id))
                .cloned()
                .collect();
            rows.set(Rc::new(kept));
        };
        let menu_remove = remove;
        data_table(&rows.read().clone(), columns, |app: &AppRow| app.id)
            .label("Installed apps")
            .marks()
            .selection(selection)
            .batch_actions(vec![
                batch_action("Copy names", |_: Vec<u32>| {}).icon(Icon::EditCopy),
                batch_action("Uninstall", remove)
                    .icon(Icon::Delete)
                    .destructive(),
            ])
            .row_menu(move |keys: &[u32]| {
                let keys = keys.to_vec();
                vec![
                    menu_item("Open", |()| {}),
                    menu_separator(),
                    menu_item(
                        if keys.len() == 1 {
                            "Uninstall".to_string()
                        } else {
                            format!("Uninstall {} apps", keys.len())
                        },
                        move |()| menu_remove(keys.clone()),
                    )
                    .destructive(),
                ]
            })
            .into_element()
    })
    .page(TABLE_HEIGHT),
    Spec::live("Edit a name in place (F2 or a double click)", || {
        let rows = use_state(|| Rc::new(apps(12)));
        let columns = use_hook(app_columns);
        data_table(&rows.read().clone(), columns, |app: &AppRow| app.id)
            .label("Installed apps")
            .density(RowDensity::Compact)
            .on_edit(move |edit: CellEdit<u32>| {
                let mut rows = rows;
                let renamed: Vec<AppRow> = rows
                    .peek()
                    .iter()
                    .cloned()
                    .map(|mut app| {
                        if app.id == edit.key && edit.column == "name" && !edit.text.is_empty() {
                            app.name = edit.text.clone();
                        }
                        app
                    })
                    .collect();
                rows.set(Rc::new(renamed));
            })
            .into_element()
    })
    .page(TABLE_HEIGHT - 80.0),
    Spec::forced("Loading", || {
        let columns = use_hook(app_columns);
        data_table(Vec::<AppRow>::new(), columns, |app: &AppRow| app.id)
            .status(TableStatus::Loading)
            .into_element()
    })
    .page(300.0),
    Spec::forced("Empty", || {
        let columns = use_hook(app_columns);
        data_table(Vec::<AppRow>::new(), columns, |app: &AppRow| app.id)
            .empty_state(
                status_page(Icon::Applications, "No apps installed")
                    .description("Apps you install appear here.")
                    .action("Open Software", |()| {}),
            )
            .into_element()
    })
    .page(300.0),
    Spec::forced("Failed", || {
        let columns = use_hook(app_columns);
        data_table(Vec::<AppRow>::new(), columns, |app: &AppRow| app.id)
            .status(
                TableStatus::failed("Could not load the apps")
                    .detail("Check that the app service is running, then try again.")
                    .retry(|()| {}),
            )
            .into_element()
    })
    .page(300.0),
];

/// A person in the sample lists.
#[derive(Clone, Debug, PartialEq)]
pub struct Person {
    id: u32,
    name: String,
    role: &'static str,
}

const FIRST: [&str; 10] = [
    "Ada", "Bea", "Carl", "Dina", "Eli", "Faye", "Gus", "Hana", "Ivo", "June",
];
const LAST: [&str; 6] = ["Adams", "Brook", "Cole", "Diaz", "Evans", "Frost"];
const ROLES: [&str; 4] = ["Design", "Engineering", "Support", "Research"];

fn people(count: u32) -> Vec<Person> {
    let mut people: Vec<Person> = (0..count)
        .map(|id| {
            let at = id as usize;
            Person {
                id,
                name: format!(
                    "{} {}",
                    FIRST[at % FIRST.len()],
                    LAST[(at / FIRST.len()) % LAST.len()]
                ),
                role: ROLES[(at * 5) % ROLES.len()],
            }
        })
        .collect();
    people.sort_by(|a, b| a.name.cmp(&b.name));
    people
}

const LIST_HEIGHT: f32 = 440.0;

pub const LIST: &[Spec] = &[
    Spec::live("Sections, marks and a multiple selection", || {
        let rows = use_hook(|| Rc::new(people(40)));
        let selection = use_state(|| Selection::of([3_u32, 4]));
        list_view(
            &rows,
            |person: &Person| list_row(person.name.clone()).icon(Icon::Users).value(person.role),
            |person: &Person| person.id,
        )
        .label("People")
        .selection_mode(SelectionMode::Multiple)
        .selection(selection)
        .marks()
        .sections(|person: &Person| person.name.chars().next().map(|initial| initial.to_string()))
        .into_element()
    })
    .page(LIST_HEIGHT)
    .script("wait 400; click-text Bea Adams; key shift+down; key shift+down; wait 300; save {out}"),
    Spec::live("One selected, two lines", || {
        let rows = use_hook(|| Rc::new(people(12)));
        let selection = use_state(|| Selection::of([5_u32]));
        list_view(
            &rows,
            |person: &Person| {
                list_row(person.name.clone())
                    .subtitle(person.role)
                    .icon(Icon::Users)
                    .chevron()
            },
            |person: &Person| person.id,
        )
        .two_line()
        .selection_mode(SelectionMode::Single)
        .selection(selection)
        .into_element()
    })
    .page(LIST_HEIGHT - 120.0),
    Spec::live("Swipe a row on a phone, or open its menu", || {
        let rows = use_state(|| Rc::new(people(10)));
        let remove = move |id: u32| {
            let mut rows = rows;
            let kept: Vec<Person> = rows.peek().iter().filter(|person| person.id != id).cloned().collect();
            rows.set(Rc::new(kept));
        };
        list_view(
            &rows.read().clone(),
            move |person: &Person| {
                let id = person.id;
                list_row(person.name.clone())
                    .icon(Icon::Users)
                    .value(person.role)
                    .actions(vec![
                        swipe_action("Archive", |()| {}).icon(Icon::DocumentOpen),
                        swipe_action("Delete", move |()| remove(id)).icon(Icon::Delete).destructive(),
                    ])
            },
            |person: &Person| person.id,
        )
        .into_element()
    })
    .page(LIST_HEIGHT - 120.0)
    .script("wait 300; touch_down 300,60; touch_move 200,60; touch_move 100,60; touch_up 100,60; wait 400; save {out}"),
    Spec::live("Reorder by the grip or Alt+Up and Alt+Down", || {
        let rows = use_state(|| Rc::new(people(6)));
        list_view(
            &rows.read().clone(),
            |person: &Person| list_row(person.name.clone()).icon(Icon::Users).value(person.role),
            |person: &Person| person.id,
        )
        .on_reorder(move |(from, to): (usize, usize)| {
            let mut rows = rows;
            let mut moved: Vec<Person> = rows.peek().iter().cloned().collect();
            let person = moved.remove(from);
            moved.insert(to, person);
            rows.set(Rc::new(moved));
        })
        .into_element()
    })
    .page(LIST_HEIGHT - 160.0),
    Spec::live("A hundred thousand rows", || {
        let rows = use_hook(|| Rc::new(people(100_000)));
        list_view(
            &rows,
            |person: &Person| list_row(person.name.clone()).icon(Icon::Users).value(person.role),
            |person: &Person| person.id,
        )
        .selection_mode(SelectionMode::Multiple)
        .into_element()
    })
    .page(LIST_HEIGHT - 120.0),
    Spec::forced("Empty", || {
        list_view(
            Vec::<Person>::new(),
            |person: &Person| list_row(person.name.clone()),
            |person: &Person| person.id,
        )
        .empty_state(
            status_page(Icon::Users, "No people yet")
                .description("People you add appear here.")
                .action("Add Person", |()| {}),
        )
        .into_element()
    })
    .page(280.0),
];

pub const TOOLBAR: &[Spec] = &[
    Spec::live(
        "Groups, overflow and customising (right click the bar)",
        || {
            let sidebar = use_state(|| true);
            let grid = use_state(|| false);
            toolbar()
                .id("taxonomy.main")
                .customizable()
                .group(
                    toolbar_group(vec![
                        toolbar_item("back", "Back", Icon::GoPrevious, |()| {}).leading(),
                        toolbar_item("forward", "Forward", Icon::GoNext, |()| {}).leading(),
                    ])
                    .linked(),
                )
                .group(toolbar_group(vec![
                    toolbar_item("sidebar", "Sidebar", Icon::SidebarShow, move |()| {
                        let mut sidebar = sidebar;
                        sidebar.set(!sidebar());
                    })
                    .toggled(sidebar())
                    .leading(),
                ]))
                .group(toolbar_group(vec![
                    toolbar_item("new", "New folder", Icon::ListAdd, |()| {})
                        .shortcut("Ctrl+Shift+N"),
                    toolbar_item("copy", "Copy", Icon::EditCopy, |()| {}).secondary(),
                    toolbar_item("delete", "Move to Trash", Icon::Delete, |()| {}).secondary(),
                ]))
                .group(
                    toolbar_group(vec![
                        toolbar_item("list", "List", Icon::ViewList, move |()| {
                            let mut grid = grid;
                            grid.set(false);
                        })
                        .toggled(!grid()),
                        toolbar_item("grid", "Grid", Icon::ViewGrid, move |()| {
                            let mut grid = grid;
                            grid.set(true);
                        })
                        .toggled(grid()),
                    ])
                    .linked(),
                )
                .group(toolbar_group(vec![
                    toolbar_item("open", "Open", Icon::DocumentOpen, |()| {}).prominent(),
                ]))
                .into_element()
        },
    )
    .wide(),
    Spec::live("Title and subtitle in the header bar", || {
        toolbar()
            .header()
            .title("Photos")
            .subtitle("1,204 items")
            .window_controls()
            .group(
                toolbar_group(vec![
                    toolbar_item("back", "Back", Icon::GoPrevious, |()| {}).leading(),
                    toolbar_item("forward", "Forward", Icon::GoNext, |()| {}).leading(),
                ])
                .linked(),
            )
            .group(toolbar_group(vec![
                toolbar_item("add", "Add", Icon::ListAdd, |()| {}),
                toolbar_item("share", "Copy", Icon::EditCopy, |()| {}).secondary(),
                toolbar_item("trash", "Move to Trash", Icon::Delete, |()| {}).secondary(),
            ]))
            .into_element()
    })
    .wide(),
    Spec::live("A linked button group on its own", || {
        let grid = use_state(|| false);
        button_group(vec![
            group_button(Icon::ViewList, "List", move |()| {
                let mut grid = grid;
                grid.set(false);
            })
            .active(!grid()),
            group_button(Icon::ViewGrid, "Grid", move |()| {
                let mut grid = grid;
                grid.set(true);
            })
            .active(grid()),
        ])
        .into_element()
    }),
];
