//! Applications: every installed application with its icon, a page for each
//! one, and the default application for each kind of file and link.
//!
//! The list comes from desktop entries (see [`crate::apps`]). Defaults are
//! read and written with `gio mime`, the same store GTK and `xdg-open` use.

use std::time::Duration;

use freya::prelude::*;

use bingux_ui::prelude::*;

use crate::apps::{self, AppEntry};
use crate::pages::sound::AppSound;
use crate::routes::{Route, use_navigator};
use crate::system::{self, CommandError, background_then};

const TIMEOUT: Duration = Duration::from_secs(5);

/// A kind of file or link and the content types that stand for it. The
/// first type is the one the choices are read from; every type is set.
struct Kind {
    title: &'static str,
    mimes: &'static [&'static str],
}

const KINDS: [Kind; 9] = [
    Kind {
        title: "Web",
        mimes: &[
            "x-scheme-handler/http",
            "x-scheme-handler/https",
            "text/html",
        ],
    },
    Kind {
        title: "Mail",
        mimes: &["x-scheme-handler/mailto"],
    },
    Kind {
        title: "Calendar",
        mimes: &["text/calendar"],
    },
    Kind {
        title: "Files",
        mimes: &["inode/directory"],
    },
    Kind {
        title: "Text",
        mimes: &["text/plain"],
    },
    Kind {
        title: "Documents",
        mimes: &["application/pdf"],
    },
    Kind {
        title: "Photos",
        mimes: &["image/png", "image/jpeg"],
    },
    Kind {
        title: "Music",
        mimes: &["audio/mpeg", "audio/flac", "audio/ogg"],
    },
    Kind {
        title: "Video",
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

#[derive(Clone, PartialEq)]
struct Model {
    apps: Vec<AppEntry>,
    choices: Vec<Choice>,
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

fn load() -> Result<Model, CommandError> {
    let installed = apps::rescan();
    let mut choices = Vec::new();
    for (index, kind) in KINDS.iter().enumerate() {
        let text = system::run("gio", &["mime", kind.mimes[0]], TIMEOUT)?;
        let (current, candidates) = parse_mime(&text);
        choices.push(Choice {
            kind: index,
            current,
            candidates,
        });
    }
    Ok(Model {
        apps: installed,
        choices,
    })
}

/// Set `app` as the default for every type of kind `kind`, then reload.
fn set_default(
    model: State<Option<Result<Model, CommandError>>>,
    error: State<Option<String>>,
    kind: usize,
    app: String,
) {
    let mut error = error;
    background_then(
        move || {
            let file = format!("{app}.desktop");
            for mime in KINDS[kind].mimes {
                system::run("gio", &["mime", mime, &file], TIMEOUT)?;
            }
            Ok::<(), CommandError>(())
        },
        move |result| {
            match result {
                Ok(()) => error.set(None),
                Err(failure) => error.set(Some(failure.0)),
            }
            system::background(model, load);
        },
    );
}

fn app_row(app: &AppEntry, open: impl Fn(Route) + 'static) -> Row {
    let route = Route::App {
        id: app.id.clone(),
        name: app.name.clone(),
    };
    row(app.name.clone())
        .leading_app(app.icon.clone())
        .navigates()
        .on_press(move |_| open(route.clone()))
}

#[derive(PartialEq)]
pub struct ApplicationsPage;

impl Component for ApplicationsPage {
    fn render(&self) -> impl IntoElement {
        let model = system::use_cached("applications", load);
        let query = use_state(String::new);
        let navigator = use_navigator();

        let Some(loaded) = model.read().clone() else {
            return page_column().child(skeleton());
        };
        let data = match loaded {
            Ok(data) => data,
            Err(failure) => {
                return page_column().child(
                    status(Status::Error(failure.0))
                        .action("Try again", move |_| system::background(model, load)),
                );
            }
        };

        let needle = query.read().trim().to_lowercase();
        let shown: Vec<&AppEntry> = data
            .apps
            .iter()
            .filter(|app| needle.is_empty() || app.name.to_lowercase().contains(&needle))
            .collect();

        let list = if shown.is_empty() {
            Group::titled("Installed")
                .row(status(Status::Empty("No application matches.".to_string())))
        } else {
            Group::titled("Installed").rows(
                shown
                    .iter()
                    .map(|app| Element::from(app_row(app, move |route| navigator.push(route)))),
            )
        };

        page_column()
            .child(
                Group::new(None).row(
                    row("Default applications")
                        .navigates()
                        .on_press(move |_| navigator.push(Route::DefaultApps)),
                ),
            )
            .child(search_field(query, |_| {}).fill_width())
            .child(list)
    }
}

/// One application: its icon and name, what it does with sound, and the
/// kinds of file it opens by default.
#[derive(PartialEq)]
pub struct AppPage {
    pub id: String,
}

impl Component for AppPage {
    fn render(&self) -> impl IntoElement {
        let model = system::use_cached("applications", load);
        let error = use_state::<Option<String>>(|| None);
        let Some(Ok(data)) = model.read().clone() else {
            return page_column().child(skeleton());
        };
        let Some(app) = data.apps.iter().find(|app| app.id == self.id).cloned() else {
            return page_column().child(status(Status::Empty(
                "This application is no longer installed.".to_string(),
            )));
        };

        // The kinds this application is registered for, and whether it is
        // already the default for each.
        let handled: Vec<(usize, bool)> = data
            .choices
            .iter()
            .filter(|choice| choice.candidates.contains(&app.id))
            .map(|choice| {
                (
                    choice.kind,
                    choice.current.as_deref() == Some(app.id.as_str()),
                )
            })
            .collect();

        let launch = app.clone();
        let identity = rect()
            .width(Size::fill())
            .cross_align(Alignment::Center)
            .spacing(S2)
            .padding((S2, 0.0, S2, 0.0))
            .child(app_icon(app.icon.clone(), ICON_APP_LG))
            .child(heading(app.name.clone()))
            .maybe_child((!app.comment.is_empty()).then(|| centered_caption(app.comment.clone())))
            .child(primary_button("Open", move |_| apps::launch(&launch)).compact());

        page_column()
            .child(identity)
            .child(AppSound {
                app_id: app.id.clone(),
            })
            .maybe_child((!handled.is_empty()).then(|| {
                Group::titled("Open by default").rows(handled.iter().map(|&(kind, is_default)| {
                    let id = app.id.clone();
                    let item = row(KINDS[kind].title);
                    Element::from(if is_default {
                        item.value("Default")
                    } else {
                        item.trailing(
                            action_button("Make default", move |_| {
                                set_default(model, error, kind, id.clone())
                            })
                            .compact(),
                        )
                    })
                }))
            }))
            .maybe_child(
                error
                    .read()
                    .clone()
                    .map(|message| status(Status::Error(message))),
            )
    }
}

/// The default application for each kind of file and link, chosen from the
/// applications registered for it.
#[derive(PartialEq)]
pub struct DefaultAppsPage;

impl Component for DefaultAppsPage {
    fn render(&self) -> impl IntoElement {
        let model = system::use_cached("applications", load);
        let error = use_state::<Option<String>>(|| None);

        let Some(loaded) = model.read().clone() else {
            return page_column().child(skeleton());
        };
        let data = match loaded {
            Ok(data) => data,
            Err(failure) => {
                return page_column().child(status(Status::Error(failure.0)));
            }
        };

        let mut page = page_column();
        if let Some(message) = error.read().clone() {
            page = page.child(status(Status::Error(message)));
        }
        page.child(Group::new(None).rows(data.choices.iter().map(|choice| {
            let kind = choice.kind;
            let listed: Vec<&AppEntry> = choice
                .candidates
                .iter()
                .filter_map(|id| data.apps.iter().find(|app| app.id == *id))
                .collect();
            if listed.is_empty() {
                return Element::from(row(KINDS[kind].title).value("No application"));
            }
            let options = listed
                .iter()
                .map(|app| (app.id.clone(), app.name.clone()))
                .collect();
            let icons = listed.iter().map(|app| app.icon.clone()).collect();
            let selected = choice.current.clone().unwrap_or_default();
            Element::from(
                row(KINDS[kind].title).trailing(
                    dropdown(options, selected, move |id: String| {
                        set_default(model, error, kind, id)
                    })
                    .app_icons(icons)
                    .width(FIELD_WIDTH_APPS),
                ),
            )
        })))
    }
}

/// Start loading this page's data before it is opened.
pub fn prefetch() {
    system::prefetch("applications", load);
}
