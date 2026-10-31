//! Applications: every installed application with its icon, a page for each
//! one, and the default application for each kind of file and link.
//!
//! The list comes from desktop entries (see [`crate::apps`]). Defaults are
//! read and written with `gio mime`, the same store GTK and `xdg-open` use.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
#[cfg(feature = "snapshot")]
use std::sync::atomic::AtomicBool;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use freya::prelude::*;

use bingux_ui::prelude::*;
use bingux_ui::runtime::{optimistic, optimistic_then};
use bingux_ui::shell::{MenuItem, menu_item, menu_separator};

use crate::apps::{self, AppEntry, Details, Removal, Source};
use crate::pages::sound::AppSound;
use crate::routes;
use crate::system::{self, CommandError, background_then};
use bingux_ui::prelude::{Router, use_router};

mod file_types;

use file_types::{Category, OtherFileTypes};

const TIMEOUT: Duration = Duration::from_secs(5);

/// Rows loaded at a time; scrolling to the end of a list loads the next page.
const PAGE: usize = 40;

/// The row icon size: twice the row size, which is what `AppIcon` asks for.
const ROW_ICON_SIZE: u32 = 64;

/// The other sizes `AppIcon` asks for: the dropdown (40) and the page head (128).
const OTHER_ICON_SIZES: [u32; 2] = [40, 128];

/// A kind of file or link and the content types that stand for it. The
/// first type is the one the choices are read from; every type is set.
pub(crate) struct Kind {
    pub(crate) title: &'static str,
    icon: Icon,
    mimes: &'static [&'static str],
}

pub(crate) const KINDS: [Kind; 9] = [
    Kind {
        title: "Web",
        icon: Icon::WebBrowser,
        mimes: &[
            "x-scheme-handler/http",
            "x-scheme-handler/https",
            "text/html",
        ],
    },
    Kind {
        title: "Mail",
        icon: Icon::MailUnread,
        mimes: &["x-scheme-handler/mailto"],
    },
    Kind {
        title: "Calendar",
        icon: Icon::XOfficeCalendar,
        mimes: &["text/calendar"],
    },
    Kind {
        title: "Files",
        icon: Icon::Folder,
        mimes: &["inode/directory"],
    },
    Kind {
        title: "Text",
        icon: Icon::DefaultAppText,
        mimes: &["text/plain"],
    },
    Kind {
        title: "Documents",
        icon: Icon::DefaultAppDocuments,
        mimes: &["application/pdf"],
    },
    Kind {
        title: "Photos",
        icon: Icon::ImageGeneric,
        mimes: &["image/png", "image/jpeg"],
    },
    Kind {
        title: "Music",
        icon: Icon::ApplicationsMultimedia,
        mimes: &["audio/mpeg", "audio/flac", "audio/ogg"],
    },
    Kind {
        title: "Video",
        icon: Icon::VideoGeneric,
        mimes: &["video/mp4", "video/x-matroska", "video/webm"],
    },
];

/// The choice for one kind: who handles it now and who could.
#[derive(Clone, PartialEq)]
struct Choice {
    kind: usize,
    /// Desktop id of the current default, without `.desktop`.
    current: Option<String>,
    /// Desktop ids of every application registered for the kind's first type.
    candidates: Vec<String>,
}

/// What the page shows. It fills in as the loader reports: the names first,
/// then the details, the icons and the defaults.
#[derive(Clone, PartialEq, Default)]
struct Model {
    apps: Vec<AppEntry>,
    /// The row icons are cached, so rows may draw them without a stall.
    icons: bool,
    /// The default and the registered applications for each kind, once read.
    choices: Option<Result<Vec<Choice>, CommandError>>,
    /// The default and the registered applications for each other content type.
    file_types: Option<Result<Vec<FileType>, CommandError>>,
}

fn desktop_id(file: &str) -> String {
    file.trim().trim_end_matches(".desktop").to_string()
}

/// Parse `gio mime TYPE`: the default application and the registered ones.
/// `gio` lists each registered application under both "Registered" and
/// "Recommended", so each id is kept once, in the order first seen.
fn parse_mime(text: &str) -> (Option<String>, Vec<String>) {
    let mut current = None;
    let mut candidates: Vec<String> = Vec::new();
    for line in text.lines() {
        if line.starts_with("Default application for") {
            current = line.rsplit_once(": ").map(|(_, file)| desktop_id(file));
        } else if line.starts_with('\t') && line.trim().ends_with(".desktop") {
            let id = desktop_id(line);
            if !candidates.contains(&id) {
                candidates.push(id);
            }
        }
    }
    (current, candidates)
}

type ModelState = State<Option<Model>>;

/// One step of a load, sent from the loader thread to the page.
enum Update {
    /// The installed applications, without their details yet.
    Names(Vec<AppEntry>),
    /// The same applications with their details.
    Details(Vec<AppEntry>),
    /// The row icons are cached.
    RowIcons,
    /// The default and registered applications for each kind.
    Choices(Result<Vec<Choice>, CommandError>),
    /// The default and registered applications for each other content type.
    FileTypes(Result<Vec<FileType>, CommandError>),
}

/// When the latest load started. A page that opens within [`FRESH_FOR`] of it
/// shows what is loaded: a load runs a `gio mime` process for every content
/// type, so flipping between the application pages must not repeat it.
static LAST_LOAD: std::sync::Mutex<Option<Instant>> = std::sync::Mutex::new(None);
const FRESH_FOR: Duration = Duration::from_secs(15);

/// Counts loads. A load that has been superseded stops writing to the page.
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Load the applications in the background and show each step as it lands.
/// The last model stays on screen until the new one has something to show.
fn start_load(mut model: ModelState) {
    *LAST_LOAD.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let mut current = model.read().clone().unwrap_or_default();
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || stream_load(sender));
    spawn(async move {
        loop {
            let mut changed = false;
            let mut finished = false;
            loop {
                match receiver.try_recv() {
                    Ok(update) => {
                        apply(&mut current, update);
                        changed = true;
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        finished = true;
                        break;
                    }
                }
            }
            if GENERATION.load(Ordering::SeqCst) != generation {
                return;
            }
            if changed {
                model.set(Some(current.clone()));
            }
            if finished {
                return;
            }
            timer(Duration::from_millis(100)).await;
        }
    });
}

/// Fold one step into the model.
fn apply(current: &mut Model, update: Update) {
    match update {
        Update::Names(mut apps) => {
            // A reload keeps the details it already had, so no badge blanks out.
            let known: HashMap<PathBuf, Details> = current
                .apps
                .iter()
                .filter_map(|app| Some((app.desktop_file.clone(), app.details.clone()?)))
                .collect();
            for app in apps.iter_mut().filter(|app| app.details.is_none()) {
                app.details = known.get(&app.desktop_file).cloned();
            }
            current.apps = apps;
        }
        Update::Details(apps) => current.apps = apps,
        Update::RowIcons => current.icons = true,
        Update::Choices(choices) => current.choices = Some(choices),
        Update::FileTypes(types) => current.file_types = Some(types),
    }
}

/// The loader thread. The cheap list goes first; the details, the icon cache
/// and the defaults follow on their own threads.
fn stream_load(sender: mpsc::Sender<Update>) {
    let mut installed = apps::rescan();
    let types = other_types(&installed);
    let _ = sender.send(Update::Names(installed.clone()));

    let choices_sender = sender.clone();
    std::thread::spawn(move || {
        let _ = choices_sender.send(Update::Choices(load_choices()));
        let _ = choices_sender.send(Update::FileTypes(load_file_types(&types)));
    });

    let icon_names: Vec<String> = installed.iter().map(|app| app.icon.clone()).collect();
    let icon_sender = sender.clone();
    std::thread::spawn(move || {
        warm_icons(&icon_names, &[ROW_ICON_SIZE]);
        let _ = icon_sender.send(Update::RowIcons);
        warm_icons(&icon_names, &OTHER_ICON_SIZES);
    });

    apps::with_details(&mut installed);
    // Two desktop files can describe one installed program, such as an AppImage
    // with a launcher entry in two places. Keep the first of each name and location.
    let mut seen = HashSet::new();
    installed.retain(|app| {
        let location = app.details.as_ref().map(|details| details.location.clone());
        seen.insert((app.name.clone(), location))
    });
    let _ = sender.send(Update::Details(installed));
}

/// Resolve each icon name at each size, so the icon cache is full before the
/// UI draws it. Each distinct name is resolved once, over a few threads.
fn warm_icons(names: &[String], sizes: &[u32]) {
    let mut unique: Vec<&str> = names.iter().map(String::as_str).collect();
    unique.sort_unstable();
    unique.dedup();
    let threads = std::thread::available_parallelism()
        .map_or(1, |count| count.get())
        .min(4);
    let chunk = unique.len().div_ceil(threads).max(1);
    std::thread::scope(|scope| {
        for part in unique.chunks(chunk) {
            scope.spawn(move || {
                for name in part {
                    for &size in sizes {
                        let _ = bingux_ui::app_icon::resolve(name, size);
                    }
                }
            });
        }
    });
}

/// The default and registered applications for each kind, from `gio mime`.
fn load_choices() -> Result<Vec<Choice>, CommandError> {
    KINDS
        .iter()
        .enumerate()
        .map(|(index, kind)| -> Result<Choice, CommandError> {
            let text = system::run("gio", &["mime", kind.mimes[0]], TIMEOUT)?;
            let (current, candidates) = parse_mime(&text);
            Ok(Choice {
                kind: index,
                current,
                candidates,
            })
        })
        .collect()
}

/// Bytes as a decimal size, such as `512.0 MB` or `12 GB`.
fn size_text(bytes: u64) -> String {
    const UNITS: [(&str, f64); 4] = [("TB", 1e12), ("GB", 1e9), ("MB", 1e6), ("KB", 1e3)];
    let value = bytes as f64;
    for (index, (unit, scale)) in UNITS.iter().enumerate() {
        if value >= *scale {
            let amount = value / *scale;
            // Whole numbers from 10 GB up, one decimal place below that.
            return if index < 2 && amount >= 10.0 {
                format!("{amount:.0} {unit}")
            } else {
                format!("{amount:.1} {unit}")
            };
        }
    }
    format!("{bytes} B")
}

/// How far below the viewport the last row is mounted-and-watched, so the next
/// page is ready before the list ends.
const LOAD_MORE_MARGIN: f32 = 360.0;

/// The last row of a list that has more to show. When it comes near the
/// viewport the next page loads, as the Fonts page does, so there is no
/// "show more" button.
#[derive(PartialEq)]
struct LoadMore {
    row: Element,
    on_more: EventHandler<()>,
}

impl Component for LoadMore {
    fn render(&self) -> impl IntoElement {
        let on_more = self.on_more.clone();
        let probe_height = Dim::Row.px() + LOAD_MORE_MARGIN;
        // An empty box reaching below the row, out of flow, reports when the
        // end of the list comes within the margin of the viewport.
        let probe = rect()
            .position(Position::new_absolute().top(0.0).left(0.0))
            .width(Size::fill())
            .height(Size::px(probe_height))
            .on_visible(move |_| on_more.call(()));
        rect()
            .width(Size::fill())
            .child(self.row.clone())
            .child(probe)
    }
}

fn set_limit(mut limit: State<usize>, value: usize) {
    limit.set(value);
}

/// The applications' model: the last load, shown at once, and a fresh load
/// started the first time a page using it renders. Call from `render`.
fn use_apps() -> ModelState {
    let model = system::cached::<Model>("applications");
    use_hook(move || {
        let fresh = LAST_LOAD
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some_and(|at| at.elapsed() < FRESH_FOR);
        if !fresh {
            start_load(model);
        }
    });
    model
}

/// Remove `app` on a background thread, then reload. `leave` is set when the
/// removal comes from the application's own page, which closes once it works.
fn run_uninstall(
    app: AppEntry,
    model: ModelState,
    busy: State<bool>,
    overlays: Overlays,
    leave: Option<Router>,
) {
    let mut busy = busy;
    busy.set(true);
    let name = app.name.clone();
    background_then(
        move || apps::uninstall(&app),
        move |result| {
            busy.set(false);
            match result {
                Ok(()) => {
                    overlays.toast(format!("{name} was uninstalled."));
                    if let Some(router) = leave {
                        router.pop();
                    }
                }
                Err(error) => overlays.toast(format!("Could not uninstall {name}. {error}")),
            }
            start_load(model);
        },
    );
}

/// Ask before removing `app`. The dialog says where the application came from.
fn ask_uninstall(
    app: AppEntry,
    model: ModelState,
    busy: State<bool>,
    overlays: Overlays,
    leave: Option<Router>,
) {
    let body = match &app.details {
        Some(details) => format!("{} is installed from {}.", app.name, details.source.label()),
        None => format!("{} is installed on this system.", app.name),
    };
    let title = format!("Uninstall {}?", app.name);
    overlays.show_dialog(
        dialog(title, body)
            .cancel("Cancel")
            .default_action(CANCEL_ID)
            .response("Uninstall", ResponseKind::Destructive, move |_| {
                run_uninstall(app.clone(), model, busy, overlays, leave)
            }),
    );
}

/// Make `app` the default for every type of kind `kind`, with `gio mime`.
fn write_default(kind: usize, app: &str) -> Result<(), String> {
    let file = format!("{app}.desktop");
    for mime in KINDS[kind].mimes {
        system::run("gio", &["mime", mime, &file], TIMEOUT).map_err(|error| error.0)?;
    }
    Ok(())
}

/// The Web kind is first in `KINDS`. Its default is the one the summary row shows.
const BROWSER_KIND: usize = 0;

/// The name of application `id`, or the id itself if it is no longer installed.
fn app_name(apps: &[AppEntry], id: &str) -> String {
    apps.iter()
        .find(|app| app.id == id)
        .map_or_else(|| id.to_string(), |app| app.name.clone())
}

/// The value on the Default applications row: the browser's default, which is
/// what most people look for first. Blank until the defaults have loaded.
fn browser_default(model: &Model) -> String {
    let Some(Ok(choices)) = &model.choices else {
        return String::new();
    };
    match choices.iter().find(|choice| choice.kind == BROWSER_KIND) {
        Some(Choice {
            current: Some(id), ..
        }) => app_name(&model.apps, id),
        Some(_) => "Not set".to_string(),
        None => String::new(),
    }
}

/// The right-click menu for a file type's row. It acts on the application the
/// row shows, and is greyed out when there is none.
fn kind_menu(app: Option<AppEntry>, overlays: Overlays, router: Router) -> Vec<MenuItem> {
    let available = app.is_some();
    let route = app
        .as_ref()
        .map(|app| routes::app(&app.id.clone(), &app.name.clone()));
    let open = menu_item("Open", move |_| {
        if let Some(app) = &app {
            apps::launch(app, move |reason| overlays.toast(reason));
        }
    })
    .icon(Icon::DocumentOpen);
    let details = menu_item("Show details", move |_| {
        if let Some(route) = &route {
            router.open(route.clone());
        }
    })
    .icon(Icon::DialogInformation);
    [open, details]
        .into_iter()
        .map(|item| if available { item } else { item.disabled() })
        .collect()
}

/// The right-click menu for one application in a kind's list. Making it the
/// default is greyed out when it already is.
fn default_menu(
    app: &AppEntry,
    kind: usize,
    is_default: bool,
    model: ModelState,
    overlays: Overlays,
    router: Router,
) -> Vec<MenuItem> {
    let chosen = app.id.clone();
    let open = app.clone();
    let details = routes::app(&app.id.clone(), &app.name.clone());
    let make = menu_item("Make default", move |_| {
        set_default(model, kind, chosen.clone(), overlays)
    });
    let make = if is_default { make.disabled() } else { make };
    vec![
        make,
        menu_separator(),
        menu_item("Open", move |_| {
            apps::launch(&open, move |reason| overlays.toast(reason))
        })
        .icon(Icon::DocumentOpen),
        menu_item("Show details", move |_| router.open(details.clone()))
            .icon(Icon::DialogInformation),
    ]
}

/// Make application `app` the default for kind `kind`. The model shows the new
/// choice at once and the write runs in the background. A failed write puts the
/// previous choice back and shows the reason. A reload then confirms the result.
fn set_default(model: ModelState, kind: usize, app: String, overlays: Overlays) {
    let previous = model.read().clone();
    let Some(mut value) = previous.clone() else {
        return;
    };
    if let Some(Ok(choices)) = &mut value.choices {
        if let Some(choice) = choices.iter_mut().find(|choice| choice.kind == kind) {
            choice.current = Some(app.clone());
        }
    }
    optimistic_then(
        model,
        Some(overlays),
        previous,
        Some(value),
        move || write_default(kind, &app),
        move || start_load(model),
    );
}

/// The order of the installed list.
#[derive(Clone, Copy, PartialEq)]
enum Sort {
    Name,
    Size,
}

fn app_row(app: &AppEntry, router: Router, icons: bool) -> Row {
    let route = routes::app(&app.id.clone(), &app.name.clone());
    let subtitle = match &app.details {
        Some(details) => match details.size {
            Some(size) => format!("{}, {}", details.source.label(), size_text(size)),
            None => details.source.label().to_string(),
        },
        None => app.category.clone(),
    };
    let base = row(app.name.clone())
        .subtitle(subtitle)
        .navigates()
        .on_press(move |_| router.open(route.clone()));
    // The icon column is reserved while the row icons are still loading, so the
    // text does not move when they arrive.
    if icons {
        base.leading_app(app.icon.clone())
    } else {
        base.leading_space()
    }
}

/// The menu shown on right-clicking an application.
fn app_menu(
    app: &AppEntry,
    model: ModelState,
    busy: State<bool>,
    overlays: Overlays,
    router: Router,
) -> Vec<MenuItem> {
    let removable = matches!(
        app.details.as_ref().map(|details| &details.removal),
        Some(Removal::Command(_) | Removal::Files(_))
    );
    let open = app.clone();
    let uninstall = app.clone();
    let details = routes::app(&app.id.clone(), &app.name.clone());
    let uninstall_item = menu_item("Uninstall", move |_| {
        ask_uninstall(uninstall.clone(), model, busy, overlays, None)
    })
    .icon(Icon::UserTrash)
    .destructive();
    vec![
        menu_item("Open", move |_| {
            apps::launch(&open, move |reason| overlays.toast(reason))
        })
        .icon(Icon::DocumentOpen),
        menu_item("Show details", move |_| router.open(details.clone()))
            .icon(Icon::DialogInformation),
        menu_separator(),
        if removable {
            uninstall_item
        } else {
            uninstall_item.disabled()
        },
    ]
}

/// One application in the list, with its right-click menu.
fn app_entry(
    app: &AppEntry,
    model: ModelState,
    busy: State<bool>,
    overlays: Overlays,
    router: Router,
    icons: bool,
) -> Element {
    // A row keyed on whether its details have arrived. Row only calls its
    // subtitle hooks when a subtitle exists, so a row whose subtitle appears
    // on re-render would change hook count; a new key remounts it instead.
    Element::from(
        rect()
            .key((&app.desktop_file, app.details.is_some(), icons))
            .tw_layout(&[w_full()])
            .child(
                app_row(app, router, icons)
                    .context_menu(app_menu(app, model, busy, overlays, router)),
            ),
    )
}

/// Sets the installed list's order and shows the first page again.
fn set_sort(mut sort: State<Sort>, limit: State<usize>, order: Sort) {
    sort.set(order);
    set_limit(limit, PAGE);
}

/// The source kinds the filter offers, in menu order. "All" is no filter.
const SOURCE_CHOICES: [Source; 7] = [
    Source::Rpm,
    Source::Flatpak,
    Source::AppImage,
    Source::Snap,
    Source::Nix,
    Source::Local,
    Source::System,
];

/// Sets the source filter and shows the first page again.
fn set_source(mut source: State<Option<Source>>, limit: State<usize>, kind: Option<Source>) {
    source.set(kind);
    set_limit(limit, PAGE);
}

/// The Installed group's header: a filter menu for source and sort order,
/// then the search toggle. Closing the search also clears its text, so no
/// hidden filter stays on.
fn installed_tools(
    searching: State<bool>,
    query: State<String>,
    sort: State<Sort>,
    source: State<Option<Source>>,
    limit: State<usize>,
    order: Sort,
    filter: Option<Source>,
) -> Element {
    let search = icon_button(Icon::Search, move |_| {
        let mut searching = searching;
        let mut query = query;
        let open = *searching.read();
        if open {
            query.set(String::new());
            set_limit(limit, PAGE);
        }
        searching.set(!open);
    })
    .tooltip("Search");
    let mut sources =
        vec![menu_item("All", move |_| set_source(source, limit, None)).checked(filter.is_none())];
    sources.extend(SOURCE_CHOICES.into_iter().map(|kind| {
        menu_item(kind.label(), move |_| set_source(source, limit, Some(kind)))
            .checked(filter == Some(kind))
    }));
    let sort_items = vec![
        menu_item("Name", move |_| set_sort(sort, limit, Sort::Name)).checked(order == Sort::Name),
        menu_item("Size", move |_| set_sort(sort, limit, Sort::Size)).checked(order == Sort::Size),
    ];
    let filter_menu = menu_button(
        "Filter and sort",
        vec![submenu("Source", sources), submenu("Sort by", sort_items)],
    )
    .icon(Icon::Filter);
    hstack(S2).child(filter_menu).child(search).into()
}

#[derive(PartialEq)]
pub struct ApplicationsPage;

impl Component for ApplicationsPage {
    fn render(&self) -> impl IntoElement {
        let model = use_apps();
        let query = use_state(String::new);
        let searching = use_state(|| false);
        let sort = use_state(|| Sort::Name);
        let source = use_state(|| None::<Source>);
        let limit = use_state(|| PAGE);
        let busy = use_state(|| false);
        let overlays = use_overlays();
        let router = use_router();
        use_page_busy(*busy.read());

        let summary = model
            .read()
            .as_ref()
            .map(browser_default)
            .unwrap_or_default();
        let loaded = model.read().clone();
        // The search field opens between the Installed header and its card.
        let search = reveal(
            *searching.read(),
            search_field(query, move |_| set_limit(limit, PAGE)).fill_width(),
        );
        let list = match loaded {
            None => Group::titled("Installed")
                .trailing(installed_tools(
                    searching,
                    query,
                    sort,
                    source,
                    limit,
                    *sort.read(),
                    *source.read(),
                ))
                .below_title(search)
                .rows((0..PLACEHOLDER_ROWS).map(|_| skeleton_row())),
            Some(data) => {
                let needle = query.read().trim().to_lowercase();
                let order = *sort.read();
                let filter = *source.read();
                let mut shown: Vec<&AppEntry> = data
                    .apps
                    .iter()
                    .filter(|app| needle.is_empty() || app.name.to_lowercase().contains(&needle))
                    .filter(|app| {
                        filter.is_none_or(|kind| {
                            app.details
                                .as_ref()
                                .is_some_and(|details| details.source == kind)
                        })
                    })
                    .collect();
                match order {
                    Sort::Name => shown.sort_by_key(|app| app.name.to_lowercase()),
                    Sort::Size => shown.sort_by_key(|app| {
                        let size = app.details.as_ref().and_then(|details| details.size);
                        (
                            std::cmp::Reverse(size.unwrap_or(0)),
                            app.name.to_lowercase(),
                        )
                    }),
                }

                let total = shown.len();
                let visible = total.min(*limit.read());
                let mut rows: Vec<Element> = shown
                    .iter()
                    .take(visible)
                    .map(|app| app_entry(app, model, busy, overlays, router, data.icons))
                    .collect();
                // Scrolling the last row into view loads the next page.
                if total > visible {
                    if let Some(last) = rows.pop() {
                        rows.push(Element::from(LoadMore {
                            row: last,
                            on_more: (move |()| set_limit(limit, visible + PAGE)).into(),
                        }));
                    }
                }
                let group = Group::titled("Installed")
                    .trailing(installed_tools(
                        searching, query, sort, source, limit, order, filter,
                    ))
                    .below_title(search);
                if rows.is_empty() {
                    group.row(status(Status::Empty("No applications match.".to_string())))
                } else {
                    group.rows(rows)
                }
            }
        };

        bingux_ui::stage::staged(
            page_column()
                .child(PageHead::new())
                .child(
                    Group::new(None).row(
                        row("Default applications")
                            .value(summary)
                            .navigates()
                            .on_press(move |_| router.open(routes::default_apps())),
                    ),
                )
                .child(list),
        )
    }
}

/// Rows drawn while the application list loads, and the rows drawn for an
/// application whose details have not arrived.
const PLACEHOLDER_ROWS: usize = 8;
const DETAIL_ROWS: usize = 6;

/// A detail's text, or `Unknown` while it is empty.
fn unknown_if_empty(text: &str) -> String {
    if text.is_empty() {
        "Unknown".to_string()
    } else {
        text.to_string()
    }
}

/// One application: its icon and name, what it does with sound, where it came
/// from, the kinds of file it opens by default, and how to remove it.
#[derive(PartialEq)]
pub struct AppPage {
    pub id: String,
}

impl Component for AppPage {
    fn render(&self) -> impl IntoElement {
        let model = use_apps();
        let busy = use_state(|| false);
        let overlays = use_overlays();
        let router = use_router();
        // Whether the application starts at login and which notification keys
        // the shell keeps for it come from a file and from gsettings: read on
        // a worker once the page is open, never in `render`.
        let autostart = use_state(|| false);
        let notifications = use_state(|| None::<Notifications>);
        let startup_read = use_state(|| false);
        {
            let id = self.id.clone();
            use_hook(move || {
                background_then(
                    move || (autostart_enabled(&id), read_notifications(&id)),
                    move |(starts, notes)| {
                        let (mut autostart, mut notifications, mut startup_read) =
                            (autostart, notifications, startup_read);
                        autostart.set(starts);
                        notifications.set(notes);
                        startup_read.set(true);
                    },
                );
            });
        }
        use_page_busy(*busy.read());
        // Snapshot only: BINGUX_SETTINGS_SNAPSHOT_OPEN=uninstall asks to
        // uninstall this application once the page has loaded, so the dialog
        // can be captured.
        #[cfg(feature = "snapshot")]
        {
            let id = self.id.clone();
            use_hook(move || {
                if std::env::var("BINGUX_SETTINGS_SNAPSHOT_OPEN").is_ok_and(|v| v == "uninstall") {
                    spawn(async move {
                        timer(Duration::from_millis(2500)).await;
                        let found = model
                            .read()
                            .as_ref()
                            .and_then(|data| data.apps.iter().find(|app| app.id == id).cloned());
                        if let Some(app) = found {
                            ask_uninstall(app, model, busy, overlays, Some(router));
                        }
                    });
                }
            });
        }

        let data = match model.read().clone() {
            None => return page_column().child(skeleton_shape(&[1, 1])),
            Some(data) => data,
        };
        let Some(app) = data.apps.iter().find(|app| app.id == self.id).cloned() else {
            return page_column().child(status(Status::Empty(
                "This application is no longer installed.".to_string(),
            )));
        };

        // The kinds this application is registered for. Each row shows whether
        // it is the default and opens the Default applications page, where the
        // choice is made, so no control is repeated here. The group is a
        // skeleton until `gio mime` has answered, and says so if it fails.
        let defaults = match &data.choices {
            None => Group::titled("Open by default").row(skeleton_row()),
            Some(Err(failure)) => {
                Group::titled("Open by default").row(status(Status::Error(failure.0.clone())))
            }
            Some(Ok(choices)) => {
                let handled: Vec<(usize, bool)> = choices
                    .iter()
                    .filter(|choice| choice.candidates.contains(&app.id))
                    .map(|choice| {
                        (
                            choice.kind,
                            choice.current.as_deref() == Some(app.id.as_str()),
                        )
                    })
                    .collect();
                if handled.is_empty() {
                    Group::titled("Open by default").row(status(Status::Empty(
                        "Not registered for any of these.".to_string(),
                    )))
                } else {
                    let app = app.clone();
                    Group::titled("Open by default").rows(handled.iter().map(
                        |&(kind, is_default)| {
                            let menu =
                                default_menu(&app, kind, is_default, model, overlays, router);
                            Element::from(
                                row(KINDS[kind].title)
                                    .leading(KINDS[kind].icon)
                                    .value(if is_default { "Default" } else { "Not default" })
                                    .navigates()
                                    .on_press(move |_| {
                                        router.open(routes::default_app(
                                            kind,
                                            &KINDS[kind].title.to_string(),
                                        ))
                                    })
                                    .context_menu(menu),
                            )
                        },
                    ))
                }
            }
        };

        let launch = app.clone();
        let identity = vstack(S2)
            .tw_layout(&[w_full(), items(Align::Center), py(S2)])
            .child(app_icon(app.icon.clone(), IconSize::AppLarge))
            .child(heading(app.name.clone()))
            .maybe_child((!app.comment.is_empty()).then(|| centered_caption(app.comment.clone())))
            .child(
                primary_button("Open", move |_| {
                    apps::launch(&launch, move |reason| overlays.toast(reason))
                })
                .compact(),
            );

        let details = match app.details.clone() {
            None => Group::titled("Details").rows((0..DETAIL_ROWS).map(|_| skeleton_row())),
            Some(details) => {
                let rows = vec![
                    row("Source").value(details.source.label().to_string()),
                    row("Version").value(if details.version.is_empty() {
                        "Unknown".to_string()
                    } else {
                        details.version.clone()
                    }),
                    row("Size").value(
                        details
                            .size
                            .map_or_else(|| "Unknown".to_string(), size_text),
                    ),
                    row("Installed from").value(unknown_if_empty(&details.origin)),
                    row("Location").value(unknown_if_empty(&details.location)),
                    row("Category").value(unknown_if_empty(&app.category)),
                ];
                Group::titled("Details").rows(rows.into_iter().map(Element::from))
            }
        };

        // Whether the application starts at login. Writing the switch copies
        // the entry into the user's autostart directory, or hides it there.
        let startup = {
            let (id, source) = (app.id.clone(), app.desktop_file.clone());
            let on = *autostart.read();
            let read = *startup_read.read();
            Group::titled("Startup").row(switch_row("Start at login", on, move |next: bool| {
                // Before the read has landed the switch shows a guess, and a
                // press must not write on top of the real value.
                if !read {
                    return;
                }
                let (id, source) = (id.clone(), source.clone());
                optimistic(autostart, Some(overlays), !next, next, move || {
                    write_autostart(&id, &source, next).map_err(|error| error.to_string())
                });
            }))
        };

        // GNOME's per-application notification switches. The group appears only
        // for applications the shell keeps settings for; until the read has
        // answered it is three skeleton rows, the height of the real group.
        let notifications_group = if *startup_read.read() {
            (*notifications.read()).map(|current| {
                let switches = [
                    ("Show notifications", NotifyKey::Enable, current.enable),
                    (
                        "Show on the lock screen",
                        NotifyKey::LockScreen,
                        current.lock_screen,
                    ),
                    ("Play notification sounds", NotifyKey::Sound, current.sound),
                ];
                Group::titled("Notifications").rows(switches.into_iter().map(|(title, key, on)| {
                    let id = app.id.clone();
                    Element::from(switch_row(title, on, move |next: bool| {
                        set_notification(notifications, overlays, id.clone(), current, key, next);
                    }))
                }))
            })
        } else {
            Some(Group::titled("Notifications").rows((0..3).map(|_| skeleton_row())))
        };

        let uninstall = match app.details.as_ref() {
            None => Group::titled("Uninstall").row(skeleton_row()),
            Some(details) => {
                let (reason, removable) = match &details.removal {
                    Removal::Command(_) => (
                        "Removes the package this application came from.".to_string(),
                        true,
                    ),
                    Removal::Files(_) => (
                        "Deletes the files this application installed.".to_string(),
                        true,
                    ),
                    Removal::Unavailable(reason) => (reason.to_string(), false),
                };
                let target = app.clone();
                Group::titled("Uninstall").row(
                    row("Uninstall this application").subtitle(reason).trailing(
                        action_button("Uninstall", move |_| {
                            ask_uninstall(target.clone(), model, busy, overlays, Some(router))
                        })
                        .destructive()
                        .compact()
                        .enabled(removable),
                    ),
                )
            }
        };

        // The file types the desktop entry says the application opens. Read
        // from the entry only; changing them means editing the entry itself.
        let file_types = (!app.mime_types.is_empty()).then(|| {
            Group::titled("File types")
                .row(row("Supported file types").subtitle(app.mime_types.join(", ")))
        });

        page_column()
            .child(identity)
            .child(AppSound {
                app_id: app.id.clone(),
            })
            .child(details)
            .child(defaults)
            .child(startup)
            .maybe_child(notifications_group)
            .maybe_child(file_types)
            .child(uninstall)
    }
}

#[cfg(not(feature = "snapshot"))]
/// The file the user's autostart entry for `id` lives in.
fn autostart_file(id: &str) -> PathBuf {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_default();
    config.join("autostart").join(format!("{id}.desktop"))
}

/// Whether the application starts at login. Only the user's autostart
/// directory is read, so an entry shipped system-wide is not reported.
#[cfg(not(feature = "snapshot"))]
fn autostart_enabled(id: &str) -> bool {
    crate::system::read_to_string(autostart_file(id)).is_ok_and(|text| {
        !text.lines().any(|line| {
            let line = line.trim();
            line == "Hidden=true" || line == "X-GNOME-Autostart-enabled=false"
        })
    })
}

/// Snapshot builds keep the switch in memory and never touch the user's files.
#[cfg(feature = "snapshot")]
static AUTOSTART_FAKE: AtomicBool = AtomicBool::new(false);

#[cfg(feature = "snapshot")]
fn autostart_enabled(_id: &str) -> bool {
    AUTOSTART_FAKE.load(Ordering::SeqCst)
}

/// Makes the user's autostart entry for `id` start (`on`) or not. The entry is
/// the user's copy if there is one, otherwise the application's own desktop file.
#[cfg(not(feature = "snapshot"))]
fn write_autostart(id: &str, source: &std::path::Path, on: bool) -> std::io::Result<()> {
    let path = autostart_file(id);
    let base = if path.exists() {
        path.clone()
    } else {
        source.to_path_buf()
    };
    let text = crate::system::read_to_string(&base)?;
    let mut kept: Vec<&str> = text
        .lines()
        .filter(|line| {
            let key = line.trim();
            !key.starts_with("Hidden=") && !key.starts_with("X-GNOME-Autostart-enabled=")
        })
        .collect();
    // GNOME's own switch: the key is `false` while disabled and absent when enabled.
    if !on {
        kept.push("X-GNOME-Autostart-enabled=false");
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, format!("{}\n", kept.join("\n")))
}

#[cfg(feature = "snapshot")]
fn write_autostart(_id: &str, _source: &std::path::Path, on: bool) -> std::io::Result<()> {
    AUTOSTART_FAKE.store(on, Ordering::SeqCst);
    Ok(())
}

/// GNOME's per-application notification settings. The shell keeps one
/// relocatable schema per application, keyed by its desktop id in lower case
/// with dots as hyphens. Only applications listed in `application-children`
/// have one.
const NOTIFICATIONS: &str = "org.gnome.desktop.notifications";
const NOTIFICATION_APP: &str = "org.gnome.desktop.notifications.application";

/// The per-application keys the page shows.
#[derive(Clone, Copy, PartialEq)]
struct Notifications {
    enable: bool,
    lock_screen: bool,
    sound: bool,
}

/// Which key of [`Notifications`] a switch changes.
#[derive(Clone, Copy)]
enum NotifyKey {
    Enable,
    LockScreen,
    Sound,
}

impl NotifyKey {
    #[cfg(not(feature = "snapshot"))]
    fn key(self) -> &'static str {
        match self {
            NotifyKey::Enable => "enable",
            NotifyKey::LockScreen => "show-in-lock-screen",
            NotifyKey::Sound => "enable-sound-alerts",
        }
    }
}

impl Notifications {
    fn set(&mut self, key: NotifyKey, on: bool) {
        match key {
            NotifyKey::Enable => self.enable = on,
            NotifyKey::LockScreen => self.lock_screen = on,
            NotifyKey::Sound => self.sound = on,
        }
    }
}

fn notification_child(app_id: &str) -> String {
    app_id.to_lowercase().replace('.', "-")
}

fn notification_schema(child: &str) -> String {
    format!("{NOTIFICATION_APP}:/org/gnome/desktop/notifications/application/{child}/")
}

/// The application's notification settings, or `None` when the shell keeps
/// none for it. Reads only.
fn read_notifications(app_id: &str) -> Option<Notifications> {
    let child = notification_child(app_id);
    let listed = system::gsettings_get(NOTIFICATIONS, "application-children").ok()?;
    if !listed.contains(&format!("'{child}'")) {
        return None;
    }
    let schema = notification_schema(&child);
    let flag = |key: &str| {
        system::gsettings_get(&schema, key)
            .ok()
            .map(|text| text.trim() == "true")
    };
    Some(Notifications {
        enable: flag("enable")?,
        lock_screen: flag("show-in-lock-screen")?,
        sound: flag("enable-sound-alerts")?,
    })
}

/// Writes one key. Snapshot builds never touch the user's settings.
#[cfg(not(feature = "snapshot"))]
fn write_notification(app_id: &str, key: NotifyKey, on: bool) -> Result<(), String> {
    let value = if on { "true" } else { "false" };
    system::gsettings_set(
        &notification_schema(&notification_child(app_id)),
        key.key(),
        value,
    )
    .map(|_| ())
    .map_err(|error| error.0)
}

#[cfg(feature = "snapshot")]
fn write_notification(_app_id: &str, _key: NotifyKey, _on: bool) -> Result<(), String> {
    Ok(())
}

/// Shows the switch at once, writes the key in the background, and reverts
/// with a toast if the write fails.
fn set_notification(
    shown: State<Option<Notifications>>,
    overlays: Overlays,
    app_id: String,
    previous: Notifications,
    key: NotifyKey,
    on: bool,
) {
    let mut next = previous;
    next.set(key, on);
    optimistic(
        shown,
        Some(overlays),
        Some(previous),
        Some(next),
        move || write_notification(&app_id, key, on),
    );
}

/// The default application for each kind of file and link. Each row shows the
/// current default and opens that kind's list of applications, where the
/// choice is made.
#[derive(PartialEq)]
pub struct DefaultAppsPage;

/// The file-type rows at their final height, before `gio mime` has answered.
/// One row per kind, with blank values and no navigation.
fn kind_placeholder() -> Group {
    Group::new(None).rows(
        KINDS
            .iter()
            .map(|kind| Element::from(row(kind.title).leading(kind.icon).value("").enabled(false))),
    )
}

impl Component for DefaultAppsPage {
    fn render(&self) -> impl IntoElement {
        let model = use_apps();
        let overlays = use_overlays();
        let router = use_router();

        let Some(data) = model.read().clone() else {
            return page_column()
                .child(PageHead::new())
                .child(kind_placeholder());
        };
        let choices = match &data.choices {
            None => {
                return page_column()
                    .child(PageHead::new())
                    .child(kind_placeholder());
            }
            Some(Err(failure)) => {
                return page_column().child(PageHead::new()).child(
                    status(Status::Error(failure.0.clone()))
                        .action("Try again", move |_| start_load(model)),
                );
            }
            Some(Ok(choices)) => choices,
        };

        let shown: Vec<&Choice> = choices.iter().collect();
        let other = match &data.file_types {
            None => None,
            Some(Err(failure)) => Some(Element::from(
                Group::titled("Other file types").row(
                    status(Status::Error(failure.0.clone()))
                        .action("Try again", move |_| start_load(model)),
                ),
            )),
            Some(Ok(types)) => (!types.is_empty()).then(|| {
                Element::from(OtherFileTypes {
                    model,
                    overlays,
                    apps: data.apps.clone(),
                    types: types.clone(),
                })
            }),
        };
        let kinds = (!shown.is_empty()).then(|| {
            Element::from(Group::new(None).rows(shown.into_iter().map(|choice| {
                let kind = choice.kind;
                let current = choice
                    .current
                    .as_ref()
                    .and_then(|id| data.apps.iter().find(|app| app.id == *id))
                    .cloned();
                let value = choice
                    .current
                    .as_ref()
                    .map_or_else(|| "Not set".to_string(), |id| app_name(&data.apps, id));
                let menu = kind_menu(current, overlays, router);
                Element::from(
                    row(KINDS[kind].title)
                        .leading(KINDS[kind].icon)
                        .value(value)
                        .navigates()
                        .on_press(move |_| {
                            router.open(routes::default_app(kind, &KINDS[kind].title.to_string()))
                        })
                        .context_menu(menu),
                )
            })))
        });
        page_column()
            .child(PageHead::new())
            .maybe_child(kinds)
            .maybe_child(other)
    }
}

/// A content type that no kind covers, with its default and the applications
/// registered for it.
#[derive(Clone, PartialEq)]
struct FileType {
    mime: String,
    /// What a person calls it, such as "Bzip2 archive".
    title: String,
    category: Category,
    /// Its first extensions, such as ".csv  .xls".
    extensions: String,
    current: Option<String>,
    candidates: Vec<String>,
}

/// Content types that applications declare but no kind covers, sorted. Scheme
/// handlers belong to the kinds that show them.
fn other_types(apps: &[AppEntry]) -> Vec<String> {
    let covered: HashSet<&str> = KINDS
        .iter()
        .flat_map(|kind| kind.mimes.iter().copied())
        .collect();
    let mut types: Vec<String> = apps
        .iter()
        .flat_map(|app| app.mime_types.iter())
        .filter(|mime| !covered.contains(mime.as_str()) && !mime.starts_with("x-scheme-handler/"))
        .cloned()
        .collect::<HashSet<String>>()
        .into_iter()
        .collect();
    types.sort();
    types
}

/// The default and registered applications for each other content type, from `gio mime`.
fn load_file_types(types: &[String]) -> Result<Vec<FileType>, CommandError> {
    types
        .iter()
        .map(|mime| -> Result<FileType, CommandError> {
            let text = system::run("gio", &["mime", mime], TIMEOUT)?;
            let (current, candidates) = parse_mime(&text);
            let (title, category, extensions) = file_types::describe(mime);
            Ok(FileType {
                mime: mime.clone(),
                title,
                category,
                extensions,
                current,
                candidates,
            })
        })
        .collect()
}

/// Make `app` the default for content type `mime`, with `gio mime`.
fn write_file_type(mime: &str, app: &str) -> Result<(), String> {
    let file = format!("{app}.desktop");
    system::run("gio", &["mime", mime, &file], TIMEOUT)
        .map(|_| ())
        .map_err(|error| error.0)
}

/// Make `app` the default for content type `mime`. The row shows the choice at
/// once and the write runs in the background. A failed write puts the previous
/// choice back and shows the reason. A reload then confirms the result.
fn set_file_type(model: ModelState, overlays: Overlays, mime: String, app: String) {
    let previous = model.read().clone();
    let Some(mut value) = previous.clone() else {
        return;
    };
    if let Some(Ok(types)) = &mut value.file_types {
        if let Some(file) = types.iter_mut().find(|file| file.mime == mime) {
            file.current = Some(app.clone());
        }
    }
    optimistic_then(
        model,
        Some(overlays),
        previous,
        Some(value),
        move || write_file_type(&mime, &app),
        move || start_load(model),
    );
}

/// The applications that can open one kind of file or link. Choosing one makes
/// it the default for every type in the kind. The list is searchable.
#[derive(PartialEq)]
pub struct DefaultKindPage {
    pub kind: usize,
}

impl Component for DefaultKindPage {
    fn render(&self) -> impl IntoElement {
        let model = use_apps();
        let query = use_state(String::new);
        let overlays = use_overlays();
        let kind = self.kind;

        let data = match model.read().clone() {
            None => return page_column().child(PageHead::new()).child(skeleton()),
            Some(data) => data,
        };
        let choices = match &data.choices {
            None => return page_column().child(PageHead::new()).child(skeleton()),
            Some(Err(failure)) => {
                return page_column().child(PageHead::new()).child(
                    status(Status::Error(failure.0.clone()))
                        .action("Try again", move |_| start_load(model)),
                );
            }
            Some(Ok(choices)) => choices,
        };
        let Some(choice) = choices.iter().find(|choice| choice.kind == kind) else {
            return page_column()
                .child(PageHead::new())
                .child(status(Status::Empty(
                    "This file type is not known.".to_string(),
                )));
        };

        let needle = query.read().trim().to_lowercase();
        let current = choice.current.clone();
        let listed: Vec<AppEntry> = choice
            .candidates
            .iter()
            .filter_map(|id| data.apps.iter().find(|app| app.id == *id).cloned())
            .filter(|app| needle.is_empty() || app.name.to_lowercase().contains(&needle))
            .collect();
        let list = if listed.is_empty() {
            radio_rows(None::<String>, Vec::new())
                .row(status(Status::Empty("No applications match.".to_string())))
        } else {
            // One radio per application: the selected one is the current default.
            let selected = current.clone().unwrap_or_default();
            let rows: Vec<Row> = listed
                .iter()
                .map(|app| {
                    let chosen = app.id.clone();
                    let row = radio_row(app.name.clone(), app.id == selected, move |_: ()| {
                        set_default(model, kind, chosen.clone(), overlays)
                    });
                    // Until the row icons are cached, drawing one would block the UI thread.
                    if data.icons {
                        row.leading_app(app.icon.clone())
                    } else {
                        row
                    }
                })
                .collect();
            radio_rows(None::<String>, rows)
        };

        page_column()
            .child(PageHead::new())
            .child(search_field(query, |_| {}).fill_width())
            .child(list)
    }
}

/// Start loading this page's data before it is opened.
pub fn prefetch() {
    start_load(system::cached::<Model>("applications"));
}
