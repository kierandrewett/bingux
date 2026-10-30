//! The address bar as a text field: a path typed or pasted, completed from
//! the disk as it is typed, and checked before it is opened.
//!
//! Nothing here waits for the disk on the UI thread. Completions are asked of
//! a worker and the answer of a keystroke that has been overtaken is dropped.
//! A typed location is checked on a thread of its own with a deadline, so a
//! network folder that does not answer gives "Not responding", not a frozen
//! window.
//!
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use bingux_files::listing::Cancel;
use bingux_files::location::{CompleteOptions, Completer};
use freya::prelude::*;

use super::address_complete::{Suggestions, step};
use super::address_list::{Lead, ListRow, NoticeRow, Part, popover, scroll_offset};
use super::address_target::{DEADLINE, Problem, Target, check};
use super::directory::Where;
use super::named::{Roots, Typed};
use super::file_icon::{FileIcon, FileKind};
use bingux_files::Kind;
use crate::component::ComponentName;
use crate::controls::path_label::path_label;
use crate::controls::text_field;
use crate::icons::{self, Icon};
use crate::style::*;
use crate::surface::use_tokens;
use crate::task::spawn_blocking;
use crate::typography::{BODY, Ink};

/// How long an answer may take before the list says it is searching.
const SLOW: Duration = Duration::from_millis(250);
/// How long to wait before asking again of a folder that was not ready, and
/// how many times to ask.
const RETRY: Duration = Duration::from_millis(500);
const RETRIES: u32 = 20;
/// How many suggestions are asked of the model.
const SUGGESTIONS: usize = 50;

/// What is wrong with what was typed.
#[derive(Clone, PartialEq)]
enum Complaint {
    /// The text names a file where a folder is wanted.
    File,
    /// The path cannot be opened. `shown` is the path as a person writes it.
    Path { problem: Problem, shown: String },
}

/// The state of the completion requests of one field. Not part of any
/// render, so it is plain cells.
struct Run {
    /// Requests made so far; the newest is the one whose answer counts.
    generation: Cell<u64>,
    /// The newest request that has been answered.
    answered: Cell<u64>,
    cancel: RefCell<Cancel>,
}

/// The text field, its completions and its checks.
#[derive(PartialEq)]
pub(super) struct Edit {
    pub location: Where,
    pub home: PathBuf,
    pub editing: State<bool>,
    pub draft: State<String>,
    pub folders_only: bool,
    pub accepts_file: bool,
    pub show_hidden: bool,
    pub on_open: EventHandler<Where>,
    pub on_file: EventHandler<PathBuf>,
    pub on_close: Option<EventHandler<()>>,
}

impl Edit {
    /// What a relative path is relative to: the folder you are in.
    fn base(&self) -> PathBuf {
        match &self.location {
            Where::Folder(path) => path.clone(),
            Where::Recent => self.home.clone(),
        }
    }
}

impl Component for Edit {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let compact = is_compact();
        let id = use_a11y();
        let focus = use_focus(id);
        let mut editing = self.editing;
        let mut draft = self.draft;
        let initial = use_hook({
            let location = self.location.clone();
            move || super::address_bar::text_of(&location)
        });
        let mut suggestions = use_state(|| None::<Arc<Suggestions>>);
        let mut highlight = use_state(|| None::<usize>);
        let mut hovered = use_state(|| None::<usize>);
        let mut list_open = use_state(|| true);
        let mut complaint = use_state(|| None::<Complaint>);
        let mut searching = use_state(|| false);
        let mut checking = use_state(|| false);
        let mut field_area = use_state(Area::default);
        let mut had_focus = use_state(|| false);
        // A text set from here puts the caret at its end only in a field that
        // is new, so each such text is a new field.
        let mut fresh = use_state(|| 0_u32);
        let mut scroll = use_scroll_controller(ScrollConfig::default);
        let completer = use_hook(|| Arc::new(Mutex::new(Completer::new())));
        let run = use_hook(|| {
            Rc::new(Run {
                generation: Cell::new(0),
                answered: Cell::new(0),
                cancel: RefCell::new(Cancel::new()),
            })
        });

        let on_close = self.on_close.clone();
        let finish = EventHandler::new(move |_: ()| {
            editing.set(false);
            if let Some(on_close) = &on_close {
                on_close.call(());
            }
        });

        // Ask the model for the completions of `text`. The previous request is
        // told to stop; if its answer arrives anyway it is dropped.
        let request = {
            let run = run.clone();
            let completer = completer.clone();
            let base = self.base();
            let home = self.home.clone();
            let folders_only = self.folders_only;
            let show_hidden = self.show_hidden;
            EventHandler::new(move |text: String| {
                run.cancel.borrow().cancel();
                let cancel = Cancel::new();
                *run.cancel.borrow_mut() = cancel.clone();
                let generation = run.generation.get() + 1;
                run.generation.set(generation);
                searching.set(false);
                if text.is_empty() {
                    run.answered.set(generation);
                    suggestions.set(None);
                    return;
                }
                let slow = run.clone();
                spawn(async move {
                    timer(SLOW).await;
                    if slow.answered.get() < generation && slow.generation.get() == generation {
                        searching.set(true);
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
                        let typed = text.clone();
                        let completer = completer.clone();
                        let base = base.clone();
                        let home = home.clone();
                        let answer = spawn_blocking(move || {
                            // The Trash is typed as `trash:///name`; its
                            // names are completed from its folder, hidden
                            // ones too (a deleted file is what it was).
                            let trash = Roots::at_home(&home).completion_text(&typed);
                            let options = CompleteOptions {
                                dirs_only: folders_only,
                                limit: SUGGESTIONS,
                                show_hidden: show_hidden || trash.is_some(),
                            };
                            let found = completer
                                .lock()
                                .unwrap_or_else(PoisonError::into_inner)
                                .complete(trash.as_deref().unwrap_or(&typed), &base, &home, options, &worker_cancel);
                            Suggestions::of(&typed, &found, folders_only)
                        })
                        .await;
                        if cancel.is_cancelled() {
                            return;
                        }
                        // A slow folder is still being read: say so, and ask
                        // again until it has answered.
                        if answer.incomplete && tries < RETRIES {
                            tries += 1;
                            searching.set(true);
                            timer(RETRY).await;
                            if cancel.is_cancelled() {
                                return;
                            }
                            continue;
                        }
                        break answer;
                    };
                    run.answered.set(generation);
                    searching.set(false);
                    highlight.set(None);
                    hovered.set(None);
                    suggestions.set(Some(Arc::new(answer)));
                });
            })
        };

        // A text that was not made by this bar (a `/` or `~` that started the
        // typing) is completed from the start.
        use_side_effect({
            let request = request.clone();
            let initial = initial.clone();
            move || {
                let text = draft.peek().clone();
                if text != initial {
                    request.call(text);
                }
            }
        });

        // Focus leaving the field is a way out, with or without typed text,
        // unless the pointer went into the list (a press on a row takes the
        // field's focus and gives it back), the field's own menu is open, or
        // a path is being checked.
        use_side_effect_with_deps(&focus().is_focused(), {
            let finish = finish.clone();
            move |focused| {
                if *focused {
                    had_focus.set(true);
                    return;
                }
                if !*had_focus.peek() {
                    return;
                }
                had_focus.set(false);
                let menu_open = try_consume_context::<crate::shell::Overlays>()
                    .is_some_and(|overlays| overlays.menu_owned_by(id));
                let in_list = hovered.peek().is_some();
                if !*checking.peek() && !menu_open && !in_list {
                    finish.call(());
                }
            }
        });

        // Open what was typed, once it has been looked at.
        let submit = {
            let finish = finish.clone();
            let on_open = self.on_open.clone();
            let on_file = self.on_file.clone();
            let accepts_file = self.accepts_file;
            let roots = Roots::at_home(&self.home);
            let base = self.base();
            let run = run.clone();
            EventHandler::new(move |text: String| {
                if *checking.peek() {
                    return;
                }
                run.cancel.borrow().cancel();
                searching.set(false);
                complaint.set(None);
                // `recent:///`, `trash:///` and a real path into the Trash
                // are places by their names: there is nothing to look at on
                // the disk first (the Trash's folder is not there until the
                // first item is binned, and is still the Trash).
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
                        // `Trash` or `Recent` alone: the root of that name,
                        // unless a folder of the name is where you are.
                        Target::Problem(Problem::NotFound) if roots.by_word(&text).is_some() => {
                            finish.call(());
                            on_open.call(roots.by_word(&text).map_or(Where::Recent, Typed::place));
                        }
                        Target::Problem(problem) => complaint.set(Some(Complaint::Path {
                            problem,
                            shown: shown_path(&target, &roots),
                        })),
                    }
                });
            })
        };

        let pick = {
            let request = request.clone();
            EventHandler::new(move |index: usize| {
                let Some(answer) = suggestions.peek().clone() else {
                    return;
                };
                let Some(text) = answer.pick(index) else {
                    return;
                };
                let is_dir = answer.rows[index].is_dir;
                draft.set(text.clone());
                // The old field going away is not the focus leaving.
                had_focus.set(false);
                hovered.set(None);
                *fresh.write() += 1;
                highlight.set(None);
                complaint.set(None);
                list_open.set(is_dir);
                if is_dir {
                    request.call(text);
                }
                // The press on the row took the field's focus; give it back.
                id.request_focus();
            })
        };

        let on_submit = {
            let pick = pick.clone();
            let submit = submit.clone();
            EventHandler::new(move |text: String| {
                // Read first: a borrow held across the match would block the
                // writes a pick makes.
                let open = *list_open.peek();
                let at = *highlight.peek();
                let answered = suggestions.peek().is_some();
                match at {
                    Some(index) if open && answered => pick.call(index),
                    _ => submit.call(text),
                }
            })
        };

        let on_change = {
            let request = request.clone();
            EventHandler::new(move |text: String| {
                // Select all and other commands that leave the text alone
                // report a change too.
                if text == *draft.peek() {
                    return;
                }
                complaint.set(None);
                list_open.set(true);
                highlight.set(None);
                hovered.set(None);
                request.call(text);
            })
        };

        let rows_count = move || {
            suggestions
                .peek()
                .as_ref()
                .map_or(0, |answer| answer.rows.len())
        };
        let move_highlight = EventHandler::new(move |forward: bool| {
            let count = rows_count();
            if !*list_open.peek() {
                list_open.set(true);
            }
            let next = step(*highlight.peek(), count, forward);
            highlight.set(next);
            if let Some(at) = next {
                let current = <(i32, i32)>::from(scroll).1;
                scroll.scroll_to_y(scroll_offset(at, count, current, false));
            }
        });
        let on_arrow = {
            let move_highlight = move_highlight.clone();
            EventHandler::new(move |amount: f64| {
                move_highlight.call(amount < 0.0)
            })
        };

        let on_escape = {
            let finish = finish.clone();
            EventHandler::new(move |_: ()| {
                let showing = complaint.peek().is_some()
                    || (*list_open.peek() && rows_count() > 0)
                    || *checking.peek();
                if showing {
                    // The list goes first; the field keeps the keyboard.
                    list_open.set(false);
                    hovered.set(None);
                    complaint.set(None);
                } else {
                    finish.call(());
                }
            })
        };

        // Tab completes what is certain, then walks the list.
        let on_key_down = {
            let request = request.clone();
            let move_highlight = move_highlight.clone();
            move |event: Event<KeyboardEventData>| {
                // Enter is the field's: the dialog around it must not also
                // take it as its own accept.
                if event.key == Key::Named(NamedKey::Enter) {
                    event.stop_propagation();
                    return;
                }
                if event.key != Key::Named(NamedKey::Tab)
                    || event
                        .modifiers
                        .intersects(Modifiers::CONTROL | Modifiers::ALT)
                {
                    return;
                }
                let Some(answer) = suggestions.peek().clone() else {
                    return;
                };
                if !*list_open.peek() || answer.rows.is_empty() {
                    return;
                }
                let typed = draft.peek().clone();
                if answer.typed != typed {
                    return;
                }
                event.stop_propagation();
                event.prevent_default();
                let backward = event.modifiers.contains(Modifiers::SHIFT);
                if !backward && answer.completed != typed {
                    draft.set(answer.completed.clone());
                    // The old field going away is not the focus leaving.
                    had_focus.set(false);
                    *fresh.write() += 1;
                    request.call(answer.completed.clone());
                } else {
                    move_highlight.call(!backward);
                }
            }
        };

        let selects = *draft.peek() == initial;
        let mut field = text_field(draft, on_submit)
            .fill_width()
            .placeholder("Type a path")
            .leading(FileKind::Folder.glyph())
            .a11y_id(id)
            .autofocus()
            .danger_outline(complaint.read().is_some())
            .on_change(on_change)
            .on_arrow(on_arrow)
            .on_escape(on_escape);
        if selects {
            field = field.select_all_on_open();
        }

        // The list under the field: suggestions, or the one thing to say.
        let rows: Vec<Element> = if checking() {
            vec![NoticeRow::busy("Checking…").into()]
        } else if let Some(complaint) = complaint.read().clone() {
            vec![ComplaintRow { complaint }.into()]
        } else if !list_open() {
            Vec::new()
        } else if searching() {
            vec![NoticeRow::busy("Searching…").into()]
        } else if let Some(answer) = suggestions.read().clone() {
            let mut rows: Vec<Element> = answer
                .rows
                .iter()
                .enumerate()
                .map(|(index, row)| {
                    let pick = pick.clone();
                    let icon = FileIcon::for_path(
                        &answer.dir.join(&row.name),
                        if row.is_dir { Kind::Directory } else { Kind::File },
                    );
                    let after = if row.is_dir { "/" } else { "" };
                    Element::from(ListRow {
                        icon: Lead::File(icon),
                        parts: Part::matched(&row.name, row.matched.clone(), after),
                        current: false,
                        mark: crate::shell::menu_query::Mark::Plain,
                        highlighted: hovered().is_none() && highlight() == Some(index),
                        on_press: EventHandler::new(move |_: ()| pick.call(index)),
                        on_hover: EventHandler::new(move |inside: bool| {
                            hovered.set(inside.then_some(index));
                        }),
                    })
                })
                .collect();
            if answer.truncated {
                rows.push(NoticeRow::quiet("More matches. Keep typing to narrow them.").into());
            }
            rows
        } else {
            Vec::new()
        };
        let open = !rows.is_empty();
        let panel = popover(open, &field_area.read(), rows, scroll);

        // On a phone the pencil of the trail becomes this button, in the same
        // place: the field is the trail's box with its text in it.
        if compact {
            let finish = finish.clone();
            field = field.trailing_action(Icon::WindowClose, "Show the path as folders", move |_: ()| finish.call(()));
        }
        rect()
            .component("AddressEdit")
            .tw(
                &tokens,
                &[
                    flex_row(),
                    grow_children(),
                    items(Align::Center),
                    w_full(),
                ],
            )
            .child(
                rect()
                    .key(*fresh.read())
                    .tw(&tokens, &[w_flex()])
                    .on_sized(move |event: Event<SizedEventData>| {
                        field_area.set_if_modified(event.area)
                    })
                    .on_key_down(on_key_down)
                    .child(field),
            )
            .child(Element::from(panel))
    }
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

/// The row that says why a typed location cannot be opened: the reason, and
/// the path cut in the middle when it is long.
#[derive(PartialEq)]
struct ComplaintRow {
    complaint: Complaint,
}

impl Component for ComplaintRow {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let (reason, shown) = match &self.complaint {
            Complaint::File => ("That is a file, not a folder.", None),
            Complaint::Path { problem, shown } => (problem.reason(), Some(shown.clone())),
        };
        rect()
            .component("AddressComplaint")
            .tw(&tokens, MENU_ITEM)
            .on_pointer_down(|event: Event<PointerEventData>| event.stop_propagation())
            .child(icons::icon(
                Icon::DialogWarning,
                IconSize::Control,
                Ink::Danger.color(&tokens),
            ))
            .child(
                BODY.in_ink(Ink::Danger)
                    .label(&tokens, reason.to_string())
                    .max_lines(1),
            )
            .maybe_child(shown.map(|shown| {
                rect()
                    .tw(&tokens, &[w_flex()])
                    .child(path_label(shown).ink(Ink::Danger).copyable(true))
            }))
    }
}
