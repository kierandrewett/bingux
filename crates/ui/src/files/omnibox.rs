//! The location field of the file views: a place and a search in one box,
//! with the card of suggestions growing out of it.
//!
//! At rest the address bar is the trail of folders
//! ([`address_crumbs`](super::address_crumbs)). A press on its empty end,
//! Ctrl+L, Ctrl+F, a double click, or a `/`, `~` or a letter typed on the list
//! turns it into this field, in the same box. Editing alone shows nothing
//! more. Once something is typed, the card grows out of the field
//! ([`Omnibox`](crate::controls::Omnibox), which owns the motion and the
//! keys) with what answers the text ([`omnibox_model`](super::omnibox_model)
//! says how it is decided):
//!
//! ```text
//! +------------------------------------------------------+
//! | [folder]  rep                                      x |  the field, as it was
//! | [This folder] [Home] [Everywhere] [Folder...]        |  where to look
//! | [Documents] [Images] [Audio] [Video] [Folders]       |  what to keep
//! | [Today] [This week] [This year] [Under 1 MB] [Over]  |
//! |  In this folder                                      |
//! |   [pdf] report.pdf                                   |  entries of this folder
//! |  Places                                              |
//! |   [dir] Reports                          ~/Reports   |
//! |  Search in Documents                  3 found        |  streams in as it is found
//! |   [pdf] annual-report.pdf    Reports       [Show in folder]
//! +------------------------------------------------------+
//! ```
//!
//! A text that begins like a path (`/usr/sh`, `~/Do`, `../x`, `trash:`) is
//! a place being typed: the card holds the folders and files that complete it,
//! Tab fills in what they share, and Enter opens the place or says why it
//! cannot (not found, no permission, a file where a folder is wanted, not
//! responding). Any other text is a search: the first row is highlighted, so
//! Enter opens it; a row of the search opens its file (or the folder), and the
//! chip on it shows the file in its folder.
//!
//! Nothing here waits for the disk on the UI thread. The completions are
//! asked of a worker and the answer of a keystroke that has been overtaken is
//! dropped; a typed location is checked on a thread of its own with a
//! deadline, so a network folder that does not answer gives "Not responding",
//! not a frozen window; the search is [`use_search_run`], which streams its
//! results and stops when the text, the scope or the filters change, or the
//! field goes away.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use bingux_files::Kind;
use bingux_files::find::{FilterKey, Matcher, Recents, Scope};
use bingux_files::listing::Cancel;
use bingux_files::location::{CompleteOptions, Completer};
use bingux_files::mounts::MountKind;
use bingux_files::{Filter, Places};
use freya::prelude::*;

use super::address_complete::{Suggestions, find_match};
use super::address_target::{DEADLINE, Problem, Target, check};
use super::directory::Where;
use super::file_icon::{FileIcon, file_icon_view};
use super::named::{Roots, Typed};
use super::omnibox_model::{
    KIND_CHIPS, PlaceMatch, SIZE_CHIPS, SearchStatus, WHEN_CHIPS, is_path, kind_name,
    other_filters, place_choices, place_phrase, places_matching, scopes, status_text, toggle_kind,
    toggle_modified, toggle_size,
};
use super::search::{Find, RunKey, use_search_run};
use crate::component::ComponentName;
use crate::controls::omnibox::step;
use crate::controls::{OmniDraft, OmniItem, OmniNotice, OmniPart, OmniRow, Omnibox, omni_chip};
use crate::icons::{self, Icon};
use crate::style::*;
use crate::surface::use_tokens;
use crate::task::spawn_blocking;
use crate::typography::Ink;

/// How long an answer may take before the card says it is searching.
const SLOW: Duration = Duration::from_millis(250);
/// How long to wait before asking again of a folder that was not ready, and
/// how many times to ask.
const RETRY: Duration = Duration::from_millis(500);
const RETRIES: u32 = 20;
/// How many completions are asked of the model.
const SUGGESTIONS: usize = 50;
/// How many entries of this folder a search shows above its own results.
const HERE_ROWS: usize = 5;
/// How many results the card lists; the engine has found them all by then.
pub(super) const RESULT_ROWS: usize = 40;
/// How many recent searches the card offers for a text.
const RECENT_ROWS: usize = 2;

/// What is wrong with what was typed.
#[derive(Clone, PartialEq)]
enum Complaint {
    /// The text names a file where a folder is wanted.
    File,
    /// The path cannot be opened. `shown` is the path as a person writes it.
    Path { problem: Problem, shown: String },
}

impl Complaint {
    fn text(&self) -> String {
        match self {
            Complaint::File => "That is a file, not a folder.".to_string(),
            Complaint::Path { problem, shown } => format!("{}: {shown}", problem.reason()),
        }
    }
}

/// The state of the completion requests of one field. Not part of any render,
/// so it is plain cells.
struct Run {
    /// Requests made so far; the newest is the one whose answer counts.
    generation: Cell<u64>,
    /// The newest request that has been answered.
    answered: Cell<u64>,
    cancel: RefCell<Cancel>,
}

/// Everything the field needs to know about where it is and what it may do.
#[derive(PartialEq)]
pub(super) struct LocationField {
    pub location: Where,
    pub home: PathBuf,
    pub editing: State<bool>,
    pub draft: OmniDraft,
    /// The tab's scope and filters.
    pub find: State<Find>,
    pub folders_only: bool,
    pub accepts_file: bool,
    pub show_hidden: bool,
    /// The chooser's file type filter, which a search leaves what it would hide out for.
    pub filter: Option<Filter>,
    pub mount: MountKind,
    pub places: Places,
    /// Folders the person has been in lately, newest first.
    pub recent: Vec<PathBuf>,
    pub on_open: EventHandler<Where>,
    pub on_file: EventHandler<PathBuf>,
    /// Show a file in the folder that holds it.
    pub on_reveal: EventHandler<PathBuf>,
    pub on_close: Option<EventHandler<()>>,
}

impl LocationField {
    /// What a relative path is relative to: the folder you are in.
    fn base(&self) -> PathBuf {
        match &self.location {
            Where::Folder(path) => path.clone(),
            Where::Recent => self.home.clone(),
        }
    }
}

/// The text a person edits for `location`: its path with a trailing slash,
/// or the address of a named root (`recent:///`, `trash:///`).
pub(super) fn text_of(location: &Where) -> String {
    Roots::current().edit_text(location)
}

/// The folders the search reads for `scope`: only Everywhere has drives.
pub(super) fn search_drives(home: &Path, scope: &Scope) -> Vec<PathBuf> {
    if *scope != Scope::Everywhere {
        return Vec::new();
    }
    bingux_files::mounts::list_mounts()
        .into_iter()
        .filter(|mount| {
            mount.mount_point != Path::new("/")
                && !matches!(mount.kind, MountKind::Network | MountKind::Optical)
                && !home.starts_with(&mount.mount_point)
                && !mount.mount_point.starts_with(home)
        })
        .map(|mount| mount.mount_point)
        .collect()
}

/// The file the recent searches are kept in: the person's own,
/// `Recents::file()`.
///
/// A test thread has a file of its own, so a test never opens with what another one searched for
/// and never writes the person's list.
#[cfg(not(test))]
pub(super) fn recents_file() -> PathBuf {
    Recents::file()
}

#[cfg(test)]
pub(super) fn recents_file() -> PathBuf {
    super::test_state::private_file("recent-searches")
}

/// Write the recent searches where they are kept, off the UI thread. A write
/// that fails is dropped: it is a convenience.
fn save_recents(recents: &Recents) {
    let snapshot = recents.clone();
    // The path is the window's: this thread knows it, the writer's thread may not.
    let file = recents_file();
    let _ = std::thread::Builder::new()
        .name("bingux-recent-searches".into())
        .spawn(move || {
            let _ = snapshot.save_to(&file);
        });
}

/// `path` the way a person writes it: `~` for the home folder, and the
/// `trash:///` address for what is in the Trash.
fn shown_path(path: &Path, roots: &Roots) -> String {
    if roots.below_trash(path).is_some() {
        return roots.uri(&Where::Folder(path.to_path_buf()));
    }
    let home = roots.home();
    match path.strip_prefix(home) {
        Ok(rest) if home != Path::new("/") => {
            if rest.as_os_str().is_empty() {
                "~".to_string()
            } else {
                format!("~/{}", rest.display())
            }
        }
        _ => path.display().to_string(),
    }
}

/// Words that are a search, from what is typed: the completion of a name
/// is asked only of one word.
fn name_to_complete(text: &str) -> Option<&str> {
    let text = text.trim();
    (!text.is_empty() && !text.contains(char::is_whitespace) && !text.contains(':')).then_some(text)
}

impl Component for LocationField {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut editing = self.editing;
        let mut draft = self.draft;
        let mut find = self.find;
        let mut suggestions = use_state(|| None::<Arc<Suggestions>>);
        let mut complaint = use_state(|| None::<Complaint>);
        let mut slow = use_state(|| false);
        let mut checking = use_state(|| false);
        let mut places_open = use_state(|| false);
        let mut recents = use_state(Recents::default);
        let completer = use_hook(|| Arc::new(Mutex::new(Completer::new())));
        let run = use_hook(|| {
            Rc::new(Run {
                generation: Cell::new(0),
                answered: Cell::new(0),
                cancel: RefCell::new(Cancel::new()),
            })
        });
        let initial = use_hook({
            let location = self.location.clone();
            move || text_of(&location)
        });
        // The glyphs the field brings are rasterised now, in the colours drawn now, so the first
        // frame of the field has them and the swap from the trail shows no frame without its icon.
        let text_ink = Paint::Text.color(&tokens);
        icons::warm(Icon::Folder, IconSize::Control, Paint::Muted.color(&tokens));
        for icon in [Icon::WindowClose, Icon::Pencil, Icon::Search] {
            icons::warm(icon, IconSize::Control, text_ink);
        }
        // The recent searches are read once, off the UI thread.
        use_hook(move || {
            let file = recents_file();
            spawn(async move {
                let loaded = spawn_blocking(move || Recents::load_from(&file)).await;
                if recents.peek().is_empty() {
                    recents.set(loaded);
                }
            });
        });

        let text = draft.read();
        let typed = text != initial;
        let path_mode = is_path(&text);
        let base = self.base();
        let here_name = base.file_name().map_or_else(
            || base.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        let scope = find.read().scope.clone();

        let on_close = self.on_close.clone();
        let finish = EventHandler::new(move |_: ()| {
            editing.set(false);
            if let Some(on_close) = &on_close {
                on_close.call(());
            }
        });

        // Ask the model for the completions of `text`. The previous request is told to stop; if
        // its answer arrives anyway it is dropped.
        let request = {
            let run = run.clone();
            let completer = completer.clone();
            let base = base.clone();
            let home = self.home.clone();
            let folders_only = self.folders_only;
            let show_hidden = self.show_hidden;
            EventHandler::new(move |text: String| {
                run.cancel.borrow().cancel();
                let cancel = Cancel::new();
                *run.cancel.borrow_mut() = cancel.clone();
                let generation = run.generation.get() + 1;
                run.generation.set(generation);
                slow.set(false);
                let asked = if is_path(&text) {
                    Some(text.clone())
                } else {
                    name_to_complete(&text).map(str::to_string)
                };
                let Some(asked) = asked else {
                    run.answered.set(generation);
                    suggestions.set(None);
                    return;
                };
                let overdue = run.clone();
                spawn(async move {
                    timer(SLOW).await;
                    if overdue.answered.get() < generation && overdue.generation.get() == generation
                    {
                        slow.set(true);
                    }
                });
                let run = run.clone();
                let completer = completer.clone();
                let base = base.clone();
                let home = home.clone();
                spawn(async move {
                    let mut tries = 0;
                    let answer = loop {
                        let worker_cancel = cancel.clone();
                        let typed = asked.clone();
                        let completer = completer.clone();
                        let base = base.clone();
                        let home = home.clone();
                        let answer = spawn_blocking(move || {
                            // The Trash is typed as `trash:///name`; its names are completed from
                            // its folder, hidden ones too (a deleted file is what it was).
                            let trash = Roots::at_home(&home).completion_text(&typed);
                            let options = CompleteOptions {
                                dirs_only: folders_only,
                                limit: SUGGESTIONS,
                                show_hidden: show_hidden || trash.is_some(),
                            };
                            let found = completer
                                .lock()
                                .unwrap_or_else(PoisonError::into_inner)
                                .complete(
                                    trash.as_deref().unwrap_or(&typed),
                                    &base,
                                    &home,
                                    options,
                                    &worker_cancel,
                                );
                            Suggestions::of(&typed, &found, folders_only)
                        })
                        .await;
                        if cancel.is_cancelled() {
                            return;
                        }
                        // A slow folder is still being read: say so, and ask again until it has answered.
                        if answer.incomplete && tries < RETRIES {
                            tries += 1;
                            slow.set(true);
                            timer(RETRY).await;
                            if cancel.is_cancelled() {
                                return;
                            }
                            continue;
                        }
                        break answer;
                    };
                    run.answered.set(generation);
                    slow.set(false);
                    suggestions.set(Some(Arc::new(answer)));
                });
            })
        };

        // A text that was not made by this field (a `/`, `~` or a letter that started the typing)
        // is completed from the start.
        use_side_effect({
            let request = request.clone();
            let initial = initial.clone();
            move || {
                let text = draft.text();
                if text != initial {
                    request.call(text);
                }
            }
        });

        // What is asked of the search: the words, the scope and the filters.
        let query = (!path_mode && typed)
            .then(|| find.read().query(&text))
            .filter(|query| !query.is_blank());
        let roots_of = Roots::at_home(&self.home);
        let key = query.clone().map(|query| RunKey {
            roots: scope.roots(&base, &self.home, &search_drives(&self.home, &scope)),
            home: self.home.clone(),
            query,
            view_hidden: self.show_hidden,
            include_noise: true,
            filter: self.filter.clone(),
            base: scope.base(&base, &self.home),
        });
        let deep = use_search_run(key, self.mount);
        let results = query.as_ref().and_then(|_| deep.latest());
        // Names are marked with what the query matches: made again only when the query changes.
        let matcher = use_hook(|| {
            Rc::new(RefCell::new(
                None::<(bingux_files::find::Query, Option<Arc<Matcher>>)>,
            ))
        });
        let marks = query.as_ref().and_then(|query| {
            let mut held = matcher.borrow_mut();
            if held.as_ref().is_none_or(|(asked, _)| asked != query) {
                *held = Some((query.clone(), Matcher::new(query).ok().map(Arc::new)));
            }
            held.as_ref().and_then(|(_, matcher)| matcher.clone())
        });

        // Open what was typed, once it has been looked at.
        let submit = {
            let finish = finish.clone();
            let on_open = self.on_open.clone();
            let on_file = self.on_file.clone();
            let accepts_file = self.accepts_file;
            let roots = roots_of.clone();
            let base = base.clone();
            let run = run.clone();
            EventHandler::new(move |text: String| {
                if *checking.peek() {
                    return;
                }
                run.cancel.borrow().cancel();
                slow.set(false);
                complaint.set(None);
                // `recent:///`, `trash:///` and a real path into the Trash are places by their
                // names: there is nothing to look at on the disk first (the Trash's folder is not
                // there until the first item is binned, and is still the Trash).
                let target = match roots.parse(&text, &base) {
                    Typed::Recent => {
                        finish.call(());
                        on_open.call(Where::Recent);
                        return;
                    }
                    Typed::Folder(path) if roots.trash_folder() == Some(path.as_path()) => {
                        finish.call(());
                        on_open.call(Where::Folder(path));
                        return;
                    }
                    Typed::Folder(path) => path,
                };
                let is_place = is_path(&text);
                checking.set(true);
                let finish = finish.clone();
                let on_open = on_open.clone();
                let on_file = on_file.clone();
                let roots = roots.clone();
                spawn(async move {
                    let asked = target.clone();
                    let found = spawn_blocking(move || check(&asked, DEADLINE)).await;
                    checking.set(false);
                    match found {
                        Target::Folder(path) => {
                            finish.call(());
                            on_open.call(Where::Folder(path));
                        }
                        Target::File(path) if accepts_file => {
                            finish.call(());
                            on_file.call(path);
                        }
                        Target::File(_) => complaint.set(Some(Complaint::File)),
                        // `Trash` or `Recent` alone: the root of that name, unless a folder of the
                        // name is where you are.
                        Target::Problem(Problem::NotFound) if roots.by_word(&text).is_some() => {
                            finish.call(());
                            on_open.call(roots.by_word(&text).map_or(Where::Recent, Typed::place));
                        }
                        // Words that name nothing here are a search, which is already on show.
                        Target::Problem(Problem::NotFound) if !is_place => {}
                        Target::Problem(problem) => complaint.set(Some(Complaint::Path {
                            problem,
                            shown: shown_path(&target, &roots),
                        })),
                    }
                });
            })
        };

        let on_change = {
            let request = request.clone();
            EventHandler::new(move |text: String| {
                complaint.set(None);
                // A `key:value` the typing has just closed becomes a chip, and the field keeps the words.
                let lifted = if is_path(&text) {
                    None
                } else {
                    find.write().lift(&text, false)
                };
                match lifted {
                    Some(words) => {
                        // After the field has taken the text it was typed: the words replace it.
                        let request = request.clone();
                        spawn(async move {
                            let mut draft = draft;
                            draft.set(words.clone());
                            request.call(words);
                        });
                    }
                    None => request.call(text),
                }
            })
        };

        // What each row does.
        let open_path = Rc::new({
            let finish = finish.clone();
            let on_open = self.on_open.clone();
            let on_file = self.on_file.clone();
            let on_reveal = self.on_reveal.clone();
            let accepts_file = self.accepts_file;
            let query_text = query.as_ref().map(ToString::to_string);
            move |path: PathBuf, dir: bool, remember: bool| {
                let mut recents = recents;
                if remember && let Some(search) = &query_text {
                    let mut now = recents.peek().clone();
                    if now.push(search) {
                        save_recents(&now);
                        recents.set(now);
                    }
                }
                finish.call(());
                if dir {
                    on_open.call(Where::Folder(path));
                } else if accepts_file {
                    on_file.call(path);
                } else {
                    on_reveal.call(path);
                }
            }
        });
        let reveal = {
            let finish = finish.clone();
            let on_reveal = self.on_reveal.clone();
            move |path: PathBuf| {
                finish.call(());
                on_reveal.call(path);
            }
        };

        let mut cards: Vec<OmniItem> = Vec::new();
        let mut row_names: Vec<String> = Vec::new();
        let mut complete_with = None;

        if checking() {
            cards.push(OmniItem::Notice {
                text: "Checking…".into(),
                kind: OmniNotice::Busy,
            });
        } else if let Some(complaint) = complaint.read().clone() {
            cards.push(OmniItem::Notice {
                text: complaint.text(),
                kind: OmniNotice::Danger,
            });
        }

        let answer = suggestions.read().clone().filter(|answer| {
            // What answers the text on show, not one a keystroke ago.
            path_mode && answer.typed == text
                || !path_mode && name_to_complete(&text) == Some(answer.typed.as_str())
        });
        let completion_rows = |answer: &Suggestions, limit: usize| -> Vec<OmniItem> {
            answer
                .rows
                .iter()
                .enumerate()
                .take(limit)
                .map(|(index, row)| {
                    let path = answer.dir.join(&row.name);
                    let icon = FileIcon::for_path(
                        &path,
                        if row.is_dir {
                            Kind::Directory
                        } else {
                            Kind::File
                        },
                    );
                    let after = if row.is_dir { "/" } else { "" };
                    let open = open_path.clone();
                    let dir = row.is_dir;
                    let target = path.clone();
                    OmniItem::Row(OmniRow {
                        lead: file_icon_view(&tokens, &icon, IconSize::Control, false),
                        parts: OmniPart::matched(&row.name, row.matched.clone(), after),
                        detail: None,
                        chip: None,
                        on_press: EventHandler::new(move |_: ()| open(target.clone(), dir, false)),
                        fill: path_mode.then(|| answer.pick(index)).flatten(),
                        name: row.name.clone(),
                    })
                })
                .collect()
        };

        if path_mode {
            if complaint.read().is_none() && !checking() {
                if slow() {
                    cards.push(OmniItem::Notice {
                        text: "Searching…".into(),
                        kind: OmniNotice::Busy,
                    });
                } else if let Some(answer) = &answer {
                    cards.extend(completion_rows(answer, SUGGESTIONS));
                    if answer.truncated {
                        cards.push(OmniItem::Notice {
                            text: "More matches. Keep typing to narrow them.".into(),
                            kind: OmniNotice::Quiet,
                        });
                    }
                    complete_with = Some(answer.completed.clone());
                }
            }
        } else if typed && !text.trim().is_empty() {
            let find_now = find.read().clone();
            let choices = place_choices(&self.places, &self.home, &find_now.scope);
            // Where to look.
            let scope_chips = rect()
                .tw(
                    &tokens,
                    &[flex_row(), items(Align::Center), gap(S1), px(S2)],
                )
                .children(
                    scopes()
                        .into_iter()
                        .map(|choice| {
                            let selected = choice.scope == find_now.scope;
                            let scope = choice.scope.clone();
                            Element::from(omni_chip(choice.label, selected, move |_: ()| {
                                find.write().scope = scope.clone();
                            }))
                        })
                        .chain([omni_chip(
                            match &find_now.scope {
                                Scope::Place(_) => {
                                    place_phrase(&find_now.scope, &here_name, &choices)
                                }
                                _ => "Folder…".to_string(),
                            },
                            matches!(find_now.scope, Scope::Place(_)),
                            move |_: ()| {
                                let open = *places_open.peek();
                                places_open.set(!open);
                            },
                        )
                        .icon(Icon::Folder)
                        .into()]),
                );
            cards.push(OmniItem::Strip {
                element: scope_chips.into(),
                height: Dim::Box7.px() + S2.px(),
            });
            if places_open() && !choices.is_empty() {
                let place_chips = rect()
                    .tw(
                        &tokens,
                        &[flex_row(), items(Align::Center), gap(S1), px(S2)],
                    )
                    .children(choices.into_iter().map(|choice| {
                        let selected = choice.scope == find_now.scope;
                        let scope = choice.scope.clone();
                        Element::from(omni_chip(choice.label, selected, move |_: ()| {
                            find.write().scope = scope.clone();
                        }))
                    }));
                cards.push(OmniItem::Strip {
                    element: place_chips.into(),
                    height: Dim::Box7.px() + S2.px(),
                });
            }
            // What to keep.
            let kind_chips = rect()
                .tw(
                    &tokens,
                    &[flex_row(), items(Align::Center), gap(S1), px(S2)],
                )
                .children(KIND_CHIPS.into_iter().map(|kind| {
                    let selected = find_now.filters.kinds.contains(&kind);
                    Element::from(omni_chip(kind_name(kind), selected, move |_: ()| {
                        toggle_kind(&mut find.write().filters, kind);
                    }))
                }));
            cards.push(OmniItem::Strip {
                element: kind_chips.into(),
                height: Dim::Box7.px() + S2.px(),
            });
            let when_chips = rect()
                .tw(
                    &tokens,
                    &[flex_row(), items(Align::Center), gap(S1), px(S2)],
                )
                .children(
                    WHEN_CHIPS
                        .into_iter()
                        .map(|(label, when)| {
                            let selected = find_now.filters.modified == Some(when);
                            Element::from(omni_chip(label, selected, move |_: ()| {
                                toggle_modified(&mut find.write().filters, when);
                            }))
                        })
                        .chain(SIZE_CHIPS.into_iter().map(|(label, size)| {
                            let selected = find_now.filters.size == Some(size);
                            Element::from(omni_chip(label, selected, move |_: ()| {
                                toggle_size(&mut find.write().filters, size);
                            }))
                        }))
                        .chain(other_filters(&find_now.filters).into_iter().map(
                            |(key, label): (FilterKey, String)| {
                                Element::from(
                                    omni_chip(label, true, move |_: ()| {
                                        find.write().filters.clear(key);
                                    })
                                    .removable(),
                                )
                            },
                        )),
                );
            cards.push(OmniItem::Strip {
                element: when_chips.into(),
                height: Dim::Box7.px() + S2.px(),
            });

            // What this folder holds by that name.
            // With a filter set the search below is what answers (it covers this folder, kept by the
            // filters); the plain entries would show what the filter leaves out.
            let unfiltered = if find_now.filters.count() == 0 {
                answer.as_ref()
            } else {
                None
            };
            let here: Vec<OmniItem> = unfiltered
                .map(|answer| completion_rows(answer, HERE_ROWS))
                .unwrap_or_default();
            let here_paths: Vec<PathBuf> = unfiltered
                .map(|answer| {
                    answer
                        .rows
                        .iter()
                        .take(HERE_ROWS)
                        .map(|row| answer.dir.join(&row.name))
                        .collect()
                })
                .unwrap_or_default();
            if !here.is_empty() {
                cards.push(OmniItem::Heading {
                    text: "In this folder".into(),
                    detail: Some(here_name.clone()),
                    busy: false,
                });
                cards.extend(here);
            }
            // Places by that name.
            let named = places_matching(&text, &self.places, &self.recent);
            if !named.is_empty() {
                cards.push(OmniItem::Heading {
                    text: "Places".into(),
                    detail: None,
                    busy: false,
                });
                for PlaceMatch {
                    name,
                    path,
                    matched,
                    bookmark,
                } in named
                {
                    let icon = FileIcon::for_path(&path, Kind::Directory);
                    let open = open_path.clone();
                    let target = path.clone();
                    cards.push(OmniItem::Row(OmniRow {
                        lead: file_icon_view(&tokens, &icon, IconSize::Control, false),
                        parts: OmniPart::matched(&name, matched, ""),
                        detail: Some(shown_path(&path, &roots_of)),
                        chip: bookmark
                            .then(|| ("Bookmark".to_string(), EventHandler::new(|_: ()| {})))
                            .filter(|_| false),
                        on_press: EventHandler::new(move |_: ()| open(target.clone(), true, false)),
                        fill: None,
                        name,
                    }));
                }
            }
            // Searches made before.
            let wanted = text.trim().to_lowercase();
            let before: Vec<String> = recents
                .read()
                .items()
                .iter()
                .filter(|search| {
                    let lower = search.to_lowercase();
                    lower != wanted && lower.contains(&wanted)
                })
                .take(RECENT_ROWS)
                .cloned()
                .collect();
            for search in before {
                let fill = search.clone();
                let range = find_match(&search, text.trim());
                cards.push(OmniItem::Row(OmniRow {
                    lead: icons::icon(Icon::Clock, IconSize::Control, Paint::Muted.color(&tokens))
                        .into(),
                    parts: OmniPart::matched(&search, range, ""),
                    detail: Some("Recent search".into()),
                    chip: None,
                    on_press: EventHandler::new(move |_: ()| {
                        let mut draft = draft;
                        draft.set(fill.clone());
                    }),
                    fill: Some(search.clone()),
                    name: format!("Search for {search}"),
                }));
            }
            // The search itself.
            if let Some(query) = &query {
                let phrase = place_phrase(
                    &find_now.scope,
                    &here_name,
                    &place_choices(&self.places, &self.home, &find_now.scope),
                );
                let mut found: Vec<&bingux_files::Entry> = results
                    .as_ref()
                    .map(|results| {
                        results
                            .listing
                            .entries
                            .iter()
                            .filter(|entry| !here_paths.contains(&entry.path))
                            .filter(|entry| !self.folders_only || entry.is_dir())
                            .collect()
                    })
                    .unwrap_or_default();
                let total = found.len();
                found.truncate(RESULT_ROWS);
                let running = deep.running();
                let status = SearchStatus {
                    place: phrase.clone(),
                    running,
                    found: total,
                    folders: results.as_ref().map_or(0, |results| results.folders),
                    truncated: results.as_ref().is_some_and(|results| results.truncated),
                    unreadable: results.as_ref().map_or(0, |results| results.unreadable),
                };
                cards.push(OmniItem::Heading {
                    text: format!("Search in {phrase}"),
                    detail: Some(status_text(&status)),
                    busy: running,
                });
                let invalid = results
                    .as_ref()
                    .and_then(|results| {
                        results
                            .invalid
                            .as_ref()
                            .map(|invalid| invalid.reason.clone())
                    })
                    .or_else(|| {
                        bingux_files::find::Query::parse_checked(
                            bingux_files::find::split_pending(&text).0,
                        )
                        .1
                        .first()
                        .map(|problem| format!("{}: {}", problem.key, problem.reason))
                    });
                let missing = results.as_ref().and_then(|results| results.missing.clone());
                if let Some(reason) = invalid {
                    cards.push(OmniItem::Notice {
                        text: reason,
                        kind: OmniNotice::Danger,
                    });
                } else if let Some(folder) = missing {
                    cards.push(OmniItem::Notice {
                        text: format!("There is no folder “{folder}” to look in"),
                        kind: OmniNotice::Danger,
                    });
                }
                let base_for_detail = scope.base(&base, &self.home);
                for entry in &found {
                    let path = entry.path.clone();
                    let dir = entry.is_dir();
                    let icon =
                        FileIcon::for_path(&path, if dir { Kind::Directory } else { Kind::File });
                    let ranges = marks
                        .as_ref()
                        .map(|matcher| matcher.highlight(&entry.name))
                        .unwrap_or_default();
                    let detail = bingux_files::find::relative_folder(&path, &base_for_detail);
                    let detail = if detail.is_empty() {
                        here_name_of(&base_for_detail)
                    } else if Path::new(&detail).is_absolute() {
                        shown_path(Path::new(&detail), &roots_of)
                    } else {
                        detail
                    };
                    let open = open_path.clone();
                    let show = reveal.clone();
                    let (go, again) = (path.clone(), path.clone());
                    cards.push(OmniItem::Row(OmniRow {
                        lead: file_icon_view(&tokens, &icon, IconSize::Control, false),
                        parts: OmniPart::highlighted(&entry.name, &ranges),
                        detail: Some(detail),
                        chip: Some((
                            "Show in folder".into(),
                            EventHandler::new(move |_: ()| show(again.clone())),
                        )),
                        on_press: EventHandler::new(move |_: ()| open(go.clone(), dir, true)),
                        fill: None,
                        name: entry.name.clone(),
                    }));
                }
                if found.is_empty() && here_paths.is_empty() && !running {
                    cards.push(OmniItem::Notice {
                        text: if query.filters.count() > 0 {
                            "No results. Try fewer filters.".into()
                        } else {
                            "No results".into()
                        },
                        kind: OmniNotice::Quiet,
                    });
                }
            }
        }
        for item in &cards {
            if let OmniItem::Row(row) = item {
                row_names.push(row.name.clone());
            }
        }

        // The scope's keys: Alt+1, 2 and 3 are the first three chips.
        let on_digit = EventHandler::new(move |digit: usize| {
            if let Some(choice) = scopes().into_iter().nth(digit - 1) {
                find.write().scope = choice.scope;
            }
        });

        let phone = is_compact();
        let mut field = Omnibox::new(draft, submit, finish.clone())
            .items(cards)
            .placeholder("Type a path or search")
            .leading(if path_mode {
                Icon::Folder
            } else {
                Icon::Search
            })
            .label("Location and search")
            .danger(complaint.read().is_some())
            .initial(initial.clone())
            .select_all(draft.text() == initial)
            .auto_highlight(!path_mode)
            .complete(complete_with)
            .alt_digit(scopes().len(), on_digit)
            .on_change(on_change);
        if phone {
            field = field.close_action(Icon::WindowClose, "Show the path as folders");
        }
        let _ = (
            step as fn(Option<usize>, usize, bool) -> Option<usize>,
            &row_names,
        );
        rect()
            .component("LocationField")
            .tw(
                &tokens,
                &[flex_row(), grow_children(), items(Align::Center), w_full()],
            )
            .child(rect().tw(&tokens, &[w_flex()]).child(field))
    }
}

/// The name of a folder as a result's place when the result is directly in it.
fn here_name_of(folder: &Path) -> String {
    folder.file_name().map_or_else(
        || folder.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// A text-ink hint for `Ink` users: kept so the colour import has one reader.
#[allow(dead_code)]
fn muted() -> Ink {
    Ink::Muted
}
