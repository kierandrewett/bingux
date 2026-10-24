//! Applications: every installed application with its icon, a page for each
//! one, and the default application for each kind of file and link.
//!
//! The list comes from desktop entries (see [`crate::apps`]). Defaults are
//! read and written with `gio mime`, the same store GTK and `xdg-open` use.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use freya::prelude::*;

use bingux_ui::prelude::*;
use bingux_ui::runtime::optimistic_then;
use bingux_ui::shell::{MenuItem, menu_item, menu_separator};

use crate::apps::{self, AppEntry, Details, Removal, Source};
use crate::pages::sound::AppSound;
use crate::routes::{Navigator, Route, use_navigator};
use crate::system::{self, CommandError, background_then};

const TIMEOUT: Duration = Duration::from_secs(5);

/// Rows shown before the list offers "Show more".
const PAGE: usize = 40;

/// The row icon size: twice the row size, which is what `AppIcon` asks for.
const ROW_ICON_SIZE: u32 = 64;

/// The other sizes `AppIcon` asks for: the dropdown (40) and the page head (128).
const OTHER_ICON_SIZES: [u32; 2] = [40, 128];

/// Every source, in the order the source filter lists them.
const SOURCES: [Source; 7] = [
    Source::Flatpak,
    Source::Snap,
    Source::AppImage,
    Source::Rpm,
    Source::Nix,
    Source::Local,
    Source::System,
];

/// A kind of file or link and the content types that stand for it. The
/// first type is the one the choices are read from; every type is set.
struct Kind {
    title: &'static str,
    icon: Icon,
    mimes: &'static [&'static str],
}

const KINDS: [Kind; 9] = [
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
        icon: Icon::DocumentEdit,
        mimes: &["text/plain"],
    },
    Kind {
        title: "Documents",
        icon: Icon::DocumentOpen,
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
    /// The dropdown and page-head icons are cached too.
    all_icons: bool,
    /// The default and the registered applications for each kind, once read.
    choices: Option<Result<Vec<Choice>, CommandError>>,
}

fn desktop_id(file: &str) -> String {
    file.trim().trim_end_matches(".desktop").to_string()
}

/// Parse `gio mime TYPE`: the default application and the registered ones.
fn parse_mime(text: &str) -> (Option<String>, Vec<String>) {
    let mut current = None;
    let mut candidates = Vec::new();
    for line in text.lines() {
        if line.starts_with("Default application for") {
            current = line.rsplit_once(": ").map(|(_, file)| desktop_id(file));
        } else if line.starts_with('\t') && line.trim().ends_with(".desktop") {
            candidates.push(desktop_id(line));
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
    /// The dropdown and page-head icons are cached too.
    AllIcons,
    /// The default and registered applications for each kind.
    Choices(Result<Vec<Choice>, CommandError>),
}

/// Counts loads. A load that has been superseded stops writing to the page.
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Load the applications in the background and show each step as it lands.
/// The last model stays on screen until the new one has something to show.
fn start_load(mut model: ModelState) {
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
        Update::AllIcons => current.all_icons = true,
        Update::Choices(choices) => current.choices = Some(choices),
    }
}

/// The loader thread. The cheap list goes first; the details, the icon cache
/// and the defaults follow on their own threads.
fn stream_load(sender: mpsc::Sender<Update>) {
    let mut installed = apps::rescan();
    let _ = sender.send(Update::Names(installed.clone()));

    let choices_sender = sender.clone();
    std::thread::spawn(move || {
        let _ = choices_sender.send(Update::Choices(load_choices()));
    });

    let icon_names: Vec<String> = installed.iter().map(|app| app.icon.clone()).collect();
    let icon_sender = sender.clone();
    std::thread::spawn(move || {
        warm_icons(&icon_names, &[ROW_ICON_SIZE]);
        let _ = icon_sender.send(Update::RowIcons);
        warm_icons(&icon_names, &OTHER_ICON_SIZES);
        let _ = icon_sender.send(Update::AllIcons);
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

fn source_of(app: &AppEntry) -> Option<Source> {
    app.details.as_ref().map(|details| details.source)
}

fn set_limit(mut limit: State<usize>, value: usize) {
    limit.set(value);
}

/// The applications' model: the last load, shown at once, and a fresh load
/// started the first time a page using it renders. Call from `render`.
fn use_apps() -> ModelState {
    let model = system::cached::<Model>("applications");
    use_hook(move || start_load(model));
    model
}

/// Remove `app` on a background thread, then reload. `leave` is set when the
/// removal comes from the application's own page, which closes once it works.
fn run_uninstall(
    app: AppEntry,
    model: ModelState,
    busy: State<bool>,
    overlays: Overlays,
    leave: Option<Navigator>,
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
                    if let Some(navigator) = leave {
                        navigator.pop();
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
    leave: Option<Navigator>,
) {
    let body = match &app.details {
        Some(details) => format!("{} is installed from {}.", app.name, details.source.label()),
        None => format!("{} is installed on this system.", app.name),
    };
    let title = format!("Uninstall {}?", app.name);
    overlays.show_dialog(dialog(title, body).cancel("Cancel").response(
        "Uninstall",
        ResponseKind::Destructive,
        move |_| run_uninstall(app.clone(), model, busy, overlays, leave),
    ));
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
fn kind_menu(app: Option<AppEntry>, overlays: Overlays, navigator: Navigator) -> Vec<MenuItem> {
    let available = app.is_some();
    let route = app.as_ref().map(|app| Route::App {
        id: app.id.clone(),
        name: app.name.clone(),
    });
    let open = menu_item("Open", move |_| {
        if let Some(app) = &app {
            apps::launch(app, move |reason| overlays.toast(reason));
        }
    })
    .icon(Icon::DocumentOpen);
    let details = menu_item("Show details", move |_| {
        if let Some(route) = &route {
            navigator.push(route.clone());
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
    navigator: Navigator,
) -> Vec<MenuItem> {
    let chosen = app.id.clone();
    let open = app.clone();
    let details = Route::App {
        id: app.id.clone(),
        name: app.name.clone(),
    };
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
        menu_item("Show details", move |_| navigator.push(details.clone()))
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
    Source,
}

fn app_row(app: &AppEntry, navigator: Navigator, icons: bool) -> Row {
    let route = Route::App {
        id: app.id.clone(),
        name: app.name.clone(),
    };
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
        .on_press(move |_| navigator.push(route.clone()));
    // Until the row icons are cached, drawing one would block the UI thread.
    if icons {
        base.leading_app(app.icon.clone())
    } else {
        base
    }
}

/// The menu shown on right-clicking an application.
fn app_menu(
    app: &AppEntry,
    model: ModelState,
    busy: State<bool>,
    overlays: Overlays,
    navigator: Navigator,
) -> Vec<MenuItem> {
    let removable = matches!(
        app.details.as_ref().map(|details| &details.removal),
        Some(Removal::Command(_) | Removal::Files(_))
    );
    let open = app.clone();
    let uninstall = app.clone();
    let details = Route::App {
        id: app.id.clone(),
        name: app.name.clone(),
    };
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
        menu_item("Show details", move |_| navigator.push(details.clone()))
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
    navigator: Navigator,
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
                app_row(app, navigator, icons)
                    .context_menu(app_menu(app, model, busy, overlays, navigator)),
            ),
    )
}

/// The source filter: every source with at least one application, with counts.
fn source_options(apps: &[AppEntry]) -> Vec<(String, String)> {
    let mut options = vec![("all".to_string(), format!("All sources ({})", apps.len()))];
    for (index, source) in SOURCES.iter().enumerate() {
        let count = apps
            .iter()
            .filter(|app| source_of(app) == Some(*source))
            .count();
        if count > 0 {
            options.push((index.to_string(), format!("{} ({count})", source.label())));
        }
    }
    options
}

#[derive(PartialEq)]
pub struct ApplicationsPage;

impl Component for ApplicationsPage {
    fn render(&self) -> impl IntoElement {
        let model = use_apps();
        let query = use_state(String::new);
        let source = use_state(|| None::<Source>);
        let sort = use_state(|| Sort::Name);
        let limit = use_state(|| PAGE);
        let busy = use_state(|| false);
        let overlays = use_overlays();
        let navigator = use_navigator();
        use_page_busy(*busy.read());

        let summary = model
            .read()
            .as_ref()
            .map(browser_default)
            .unwrap_or_default();
        let loaded = model.read().clone();
        let (filters, list) = match loaded {
            None => (None, Group::titled("Installed").row(skeleton())),
            Some(data) => {
                let needle = query.read().trim().to_lowercase();
                let wanted = *source.read();
                let order = *sort.read();
                let mut shown: Vec<&AppEntry> = data
                    .apps
                    .iter()
                    .filter(|app| needle.is_empty() || app.name.to_lowercase().contains(&needle))
                    .filter(|app| wanted.is_none_or(|kind| source_of(app) == Some(kind)))
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
                    Sort::Source => shown.sort_by_key(|app| {
                        (
                            source_of(app).map(|kind| kind.label()),
                            app.name.to_lowercase(),
                        )
                    }),
                }

                let total = shown.len();
                let visible = total.min(*limit.read());
                let mut rows: Vec<Element> = shown
                    .iter()
                    .take(visible)
                    .map(|app| app_entry(app, model, busy, overlays, navigator, data.icons))
                    .collect();
                if total > visible {
                    rows.push(Element::from(
                        row(format!("{} more", total - visible)).trailing(
                            action_button("Show more", move |_| set_limit(limit, visible + PAGE))
                                .compact(),
                        ),
                    ));
                }
                let list = if rows.is_empty() {
                    Group::titled("Installed")
                        .row(status(Status::Empty("No application matches.".to_string())))
                } else {
                    Group::titled("Installed").rows(rows)
                };

                let source_id = wanted
                    .and_then(|kind| SOURCES.iter().position(|item| *item == kind))
                    .map_or("all".to_string(), |index| index.to_string());
                let sort_id = match order {
                    Sort::Name => "name",
                    Sort::Size => "size",
                    Sort::Source => "source",
                };
                let filters = Group::new(None)
                    .row(
                        row("Source").trailing(
                            dropdown(source_options(&data.apps), source_id, move |id: String| {
                                let mut source = source;
                                source.set(
                                    id.parse::<usize>()
                                        .ok()
                                        .and_then(|index| SOURCES.get(index).copied()),
                                );
                                set_limit(limit, PAGE);
                            })
                            .min_width(Dim::Field),
                        ),
                    )
                    .row(
                        row("Sort by").trailing(
                            dropdown(
                                vec![
                                    ("name".to_string(), "Name".to_string()),
                                    ("size".to_string(), "Size".to_string()),
                                    ("source".to_string(), "Source".to_string()),
                                ],
                                sort_id,
                                move |id: String| {
                                    let mut sort = sort;
                                    sort.set(match id.as_str() {
                                        "size" => Sort::Size,
                                        "source" => Sort::Source,
                                        _ => Sort::Name,
                                    });
                                    set_limit(limit, PAGE);
                                },
                            )
                            .min_width(Dim::Field),
                        ),
                    );
                (Some(filters), list)
            }
        };

        page_column()
            .child(
                Group::new(None).row(
                    row("Default applications")
                        .value(summary)
                        .navigates()
                        .on_press(move |_| navigator.push(Route::DefaultApps)),
                ),
            )
            .child(search_field(query, move |_| set_limit(limit, PAGE)).fill_width())
            .maybe_child(filters)
            .child(list)
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
        let navigator = use_navigator();
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
                            ask_uninstall(app, model, busy, overlays, Some(navigator));
                        }
                    });
                }
            });
        }

        let data = match model.read().clone() {
            None => return page_column().child(skeleton()),
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
            None => Some(Group::titled("Open by default").row(skeleton())),
            Some(Err(failure)) => {
                Some(Group::titled("Open by default").row(status(Status::Error(failure.0.clone()))))
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
                (!handled.is_empty()).then(|| {
                    let app = app.clone();
                    Group::titled("Open by default").rows(handled.iter().map(
                        |&(kind, is_default)| {
                            let menu =
                                default_menu(&app, kind, is_default, model, overlays, navigator);
                            Element::from(
                                row(KINDS[kind].title)
                                    .leading(KINDS[kind].icon)
                                    .value(if is_default { "Default" } else { "Not default" })
                                    .navigates()
                                    .on_press(move |_| {
                                        navigator.push(Route::DefaultApp {
                                            kind,
                                            name: KINDS[kind].title.to_string(),
                                        })
                                    })
                                    .context_menu(menu),
                            )
                        },
                    ))
                })
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

        let details = app.details.clone().map(|details| {
            let mut rows = vec![
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
            ];
            if !details.origin.is_empty() {
                rows.push(row("Installed from").value(details.origin.clone()));
            }
            if !details.location.is_empty() {
                rows.push(row("Location").value(details.location.clone()));
            }
            if !app.category.is_empty() {
                rows.push(row("Category").value(app.category.clone()));
            }
            Group::titled("Details").rows(rows.into_iter().map(Element::from))
        });

        let uninstall = app.details.as_ref().map(|details| {
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
                        ask_uninstall(target.clone(), model, busy, overlays, Some(navigator))
                    })
                    .destructive()
                    .compact()
                    .enabled(removable),
                ),
            )
        });

        page_column()
            .child(identity)
            .child(AppSound {
                app_id: app.id.clone(),
            })
            .maybe_child(details)
            .maybe_child(defaults)
            .maybe_child(uninstall)
    }
}

/// The default application for each kind of file and link. Each row shows the
/// current default and opens that kind's list of applications, where the
/// choice is made.
#[derive(PartialEq)]
pub struct DefaultAppsPage;

impl Component for DefaultAppsPage {
    fn render(&self) -> impl IntoElement {
        let model = use_apps();
        let query = use_state(String::new);
        let overlays = use_overlays();
        let navigator = use_navigator();

        let Some(data) = model.read().clone() else {
            return page_column().child(skeleton());
        };
        let choices = match &data.choices {
            None => return page_column().child(skeleton()),
            Some(Err(failure)) => {
                return page_column().child(
                    status(Status::Error(failure.0.clone()))
                        .action("Try again", move |_| start_load(model)),
                );
            }
            Some(Ok(choices)) => choices,
        };

        let needle = query.read().trim().to_lowercase();
        let shown: Vec<&Choice> = choices
            .iter()
            .filter(|choice| {
                let current = choice.current.as_ref().map(|id| app_name(&data.apps, id));
                needle.is_empty()
                    || KINDS[choice.kind].title.to_lowercase().contains(&needle)
                    || current.is_some_and(|name| name.to_lowercase().contains(&needle))
            })
            .collect();
        let list = if shown.is_empty() {
            Group::new(None).row(status(Status::Empty("No file type matches.".to_string())))
        } else {
            Group::new(None).rows(shown.into_iter().map(|choice| {
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
                let menu = kind_menu(current, overlays, navigator);
                Element::from(
                    row(KINDS[kind].title)
                        .leading(KINDS[kind].icon)
                        .value(value)
                        .navigates()
                        .on_press(move |_| {
                            navigator.push(Route::DefaultApp {
                                kind,
                                name: KINDS[kind].title.to_string(),
                            })
                        })
                        .context_menu(menu),
                )
            }))
        };

        page_column()
            .child(search_field(query, |_| {}).fill_width())
            .child(list)
    }
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
        let navigator = use_navigator();
        let kind = self.kind;

        let data = match model.read().clone() {
            None => return page_column().child(skeleton()),
            Some(data) => data,
        };
        let choices = match &data.choices {
            None => return page_column().child(skeleton()),
            Some(Err(failure)) => {
                return page_column().child(
                    status(Status::Error(failure.0.clone()))
                        .action("Try again", move |_| start_load(model)),
                );
            }
            Some(Ok(choices)) => choices,
        };
        let Some(choice) = choices.iter().find(|choice| choice.kind == kind) else {
            return page_column().child(status(Status::Empty(
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
            Group::new(None).row(status(Status::Empty("No application matches.".to_string())))
        } else {
            Group::new(None).rows(listed.iter().map(|app| {
                let is_default = current.as_deref() == Some(app.id.as_str());
                let menu = default_menu(app, kind, is_default, model, overlays, navigator);
                let chosen = app.id.clone();
                let base = row(app.name.clone())
                    .on_press(move |_| set_default(model, kind, chosen.clone(), overlays));
                let base = if is_default {
                    base.value("Default")
                } else {
                    base
                };
                let base = if data.icons {
                    base.leading_app(app.icon.clone())
                } else {
                    base
                };
                Element::from(base.context_menu(menu))
            }))
        };

        page_column()
            .child(search_field(query, |_| {}).fill_width())
            .child(list)
    }
}

/// Start loading this page's data before it is opened.
pub fn prefetch() {
    start_load(system::cached::<Model>("applications"));
}
