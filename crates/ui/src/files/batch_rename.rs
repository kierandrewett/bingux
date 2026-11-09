//! The Rename sheet for several items: find and replace, a change of case and
//! a number, with every new name shown as it is typed. Rename on a selection
//! of more than one item (F2, the context menu) opens it; one item is renamed
//! in place, in the list.
//!
//! The rules are [`batch_rename_model`](super::batch_rename_model). Rename is
//! blocked, with the reason in the footer, while two items would share a
//! name, a name is taken in the folder, a name is not allowed, or nothing
//! would change. The whole batch is one step of Undo.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use bingux_files::Entry;
use freya::prelude::*;

use super::actions::{Env, rename_many};
use super::batch_rename_model::{CaseChange, Numbering, Planned, Rule, blocked, plan};
use crate::controls::{dropdown, form_field, stepper};
use crate::group::{Group, caption, form_row};
use crate::motion::reveal_block;
use crate::rows::row;
use crate::shell::{form_dialog, open_form, use_overlays};
use crate::style::*;
use crate::surface::use_tokens;

/// The most rows the preview draws; the rest are counted under it.
const PREVIEW_ROWS: usize = 200;

/// One item of the batch: where it is, its name, whether it is a folder.
#[derive(Clone, PartialEq)]
struct Item {
    path: PathBuf,
    name: String,
    folder: bool,
}

struct Launch {
    env: Env,
    items: Vec<Item>,
}

#[derive(Clone)]
struct Shared(Rc<Launch>);

impl PartialEq for Shared {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// Open the sheet for `selected`, in the order the list shows them.
pub(super) fn show_batch_rename(env: &Env, selected: &[Entry]) {
    let items: Vec<Item> = selected
        .iter()
        .map(|entry| Item {
            path: entry.path.clone(),
            name: entry.name.clone(),
            folder: entry.is_dir(),
        })
        .collect();
    let title = format!("Rename {} items", items.len());
    let focus = env.focus;
    open_form(
        env.overlays,
        title,
        Element::from(BatchRename {
            launch: Shared(Rc::new(Launch {
                env: env.clone(),
                items,
            })),
        }),
        Some(EventHandler::new(move |()| focus.request_focus())),
    );
}

/// Whether `name` is taken, beside any item of the batch, by something that
/// is not in it. Asked of the disk: the batch may come from several folders
/// (a search, Recent).
fn taken_by_others(items: &[Item], name: &str) -> bool {
    items
        .iter()
        .filter_map(|item| item.path.parent())
        .any(|folder| {
            let there = folder.join(name);
            there.symlink_metadata().is_ok() && !items.iter().any(|item| item.path == there)
        })
}

#[derive(PartialEq)]
struct BatchRename {
    launch: Shared,
}

impl Component for BatchRename {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let overlays = use_overlays();
        let launch = self.launch.0.clone();
        let find = use_state(String::new);
        let replace = use_state(String::new);
        let mut match_case = use_state(|| false);
        let mut case = use_state(CaseChange::default);
        let mut numbering = use_state(Numbering::default);
        let mut start = use_state(|| 1_u32);

        let rule = Rule {
            find: find.read().clone(),
            replace: replace.read().clone(),
            match_case: *match_case.read(),
            case: *case.read(),
            numbering: *numbering.read(),
            start: *start.read(),
        };
        let names: Vec<(String, bool)> = launch
            .items
            .iter()
            .map(|item| (item.name.clone(), item.folder))
            .collect();
        let planned: Vec<Planned> =
            plan(&names, &rule, |name| taken_by_others(&launch.items, name));
        let reason = blocked(&planned);

        let go = {
            let launch = launch.clone();
            let planned = planned.clone();
            EventHandler::new(move |()| {
                let pairs: Vec<(PathBuf, String)> = launch
                    .items
                    .iter()
                    .zip(&planned)
                    .filter(|(_, planned)| planned.changes())
                    .map(|(item, planned)| (item.path.clone(), planned.to.clone()))
                    .collect();
                overlays.dismiss_dialog();
                rename_many(&launch.env, pairs);
            })
        };

        let words = Group::titled("Find and replace")
            .row(form_field("Find", find).autofocus().on_submit(go.clone()))
            .row(form_field("Replace with", replace).on_submit(go.clone()))
            .row(crate::rows::switch_row(
                "Match case",
                *match_case.read(),
                move |on: bool| match_case.set(on),
            ));
        let letters = Group::titled("Case").row(form_row(
            "Change to",
            dropdown(
                vec![
                    ("keep".to_string(), "Keep as it is".to_string()),
                    ("lower".to_string(), "lowercase".to_string()),
                    ("upper".to_string(), "UPPERCASE".to_string()),
                    ("title".to_string(), "Title Case".to_string()),
                ],
                match *case.read() {
                    CaseChange::Keep => "keep",
                    CaseChange::Lower => "lower",
                    CaseChange::Upper => "upper",
                    CaseChange::Title => "title",
                },
                move |key: String| {
                    case.set(match key.as_str() {
                        "lower" => CaseChange::Lower,
                        "upper" => CaseChange::Upper,
                        "title" => CaseChange::Title,
                        _ => CaseChange::Keep,
                    });
                },
            )
            .title("Change to"),
        ));
        let numbered = *numbering.read() != Numbering::Off;
        let number = Group::titled("Number").row(form_row(
            "Add a number",
            dropdown(
                vec![
                    ("off".to_string(), "No number".to_string()),
                    ("after".to_string(), "After the name".to_string()),
                    ("before".to_string(), "Before the name".to_string()),
                ],
                match *numbering.read() {
                    Numbering::Off => "off",
                    Numbering::After => "after",
                    Numbering::Before => "before",
                },
                move |key: String| {
                    numbering.set(match key.as_str() {
                        "after" => Numbering::After,
                        "before" => Numbering::Before,
                        _ => Numbering::Off,
                    });
                },
            )
            .title("Add a number"),
        ));
        let first = Group::new(None).row(form_row(
            "Start at",
            stepper(i64::from(*start.read()), move |value: i64| {
                start.set(u32::try_from(value).unwrap_or(0));
            })
            .range(0, 99_999),
        ));

        let mut preview = Group::titled("New names");
        for planned in planned.iter().take(PREVIEW_ROWS) {
            preview = preview.row(
                row(planned.from.clone())
                    .value(planned.to.clone())
                    .maybe_subtitle(
                        planned
                            .problem
                            .as_ref()
                            .map(|problem| problem.text().to_string()),
                    ),
            );
        }
        let more = planned.len().saturating_sub(PREVIEW_ROWS);
        let preview_note = (more > 0).then(|| {
            caption(if more == 1 {
                "1 more item is not shown.".to_string()
            } else {
                format!("{more} more items are not shown.")
            })
        });

        let body = rect()
            .tw(&tokens, &[flex_col(), w_full(), gap(S4)])
            .child(Element::from(words))
            .child(Element::from(letters))
            .child(Element::from(number))
            .child(reveal_block(numbered, S4, Element::from(first)).lazy())
            .child(Element::from(preview))
            .maybe_child(preview_note);
        form_dialog(
            format!("Rename {} items", launch.items.len()),
            body,
            "Rename",
            go,
        )
        .blocked(reason)
        .stays_open()
    }
}

/// The folder an item is in, for tests and callers that group by folder.
#[allow(dead_code)]
fn folder_of(path: &Path) -> Option<&Path> {
    path.parent()
}
