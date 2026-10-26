//! The path bar: where you are as a breadcrumb, or a typed location.

use std::path::{Path, PathBuf};

use bingux_files::location::{self, Candidate, Completion};
use freya::prelude::*;

use super::directory::Where;
use crate::component::ComponentName;
use crate::controls::{breadcrumb, button::icon_button, text_field};
use crate::icons::Icon;
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::BODY;

/// How many suggestions the typed location offers at most.
const SUGGESTIONS: usize = 6;
/// The suggestion list floats above the file list.
const SUGGESTION_LAYER: i16 = 40;

/// The current location as a trail of folders, each a button that opens that
/// folder, with a button at the end that turns the trail into a text field.
/// In the field, a person types or pastes a path (`~`, `..` and `file://`
/// work), sees the folders that match as they type, and presses Enter.
///
/// `editing` and `draft` belong to the caller, so a keyboard shortcut
/// (Ctrl+L) can start typing.
#[derive(PartialEq)]
pub struct PathBar {
    location: Where,
    home: PathBuf,
    editing: State<bool>,
    draft: State<String>,
    on_open: EventHandler<Where>,
    on_submit: EventHandler<String>,
    folders_only: bool,
}

impl PathBar {
    /// `on_open` receives a crumb that was pressed; `on_submit` the text of
    /// the field when Enter is pressed.
    pub fn new(
        location: Where,
        home: PathBuf,
        editing: State<bool>,
        draft: State<String>,
        on_open: impl Into<EventHandler<Where>>,
        on_submit: impl Into<EventHandler<String>>,
    ) -> Self {
        PathBar {
            location,
            home,
            editing,
            draft,
            on_open: on_open.into(),
            on_submit: on_submit.into(),
            folders_only: false,
        }
    }

    /// Suggest only folders, for choosing a folder.
    pub fn folders_only(mut self, folders_only: bool) -> Self {
        self.folders_only = folders_only;
        self
    }

    /// The text a person edits for `location`: its path with a trailing slash.
    pub fn text_of(location: &Where) -> String {
        match location {
            Where::Folder(path) if path == Path::new("/") => "/".into(),
            Where::Folder(path) => format!("{}/", path.display()),
            Where::Recent => String::new(),
        }
    }

    fn base(&self) -> PathBuf {
        match &self.location {
            Where::Folder(path) => path.clone(),
            Where::Recent => self.home.clone(),
        }
    }
}

impl Component for PathBar {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut editing = self.editing;
        let mut draft = self.draft;
        let field = rect().tw(
            &tokens,
            &[
                flex_row(),
                items(Align::Center),
                w_flex(),
                h(Dim::Control),
                rounded(Radius::Control),
                bg(Paint::Control),
                clip(),
            ],
        );
        if *editing.read() {
            let text = draft.read().clone();
            let completion = (!text.is_empty())
                .then(|| location::complete(&text, &self.base(), &self.home, self.folders_only));
            let on_submit = self.on_submit.clone();
            let input = text_field(draft, move |text: String| on_submit.call(text))
                .fill_width()
                .placeholder("Type a path")
                .leading(Icon::Folder)
                .autofocus();
            return rect()
                .component("PathBar")
                .tw(
                    &tokens,
                    &[flex_row(), items(Align::Center), w_full(), gap(S2)],
                )
                .child(
                    rect()
                        .tw(&tokens, &[w_flex()])
                        .child(input)
                        .maybe_child(completion.and_then(|completion| {
                            suggestions(&tokens, &completion, &text, draft)
                        })),
                )
                .child(
                    icon_button(Icon::WindowClose, move |_: ()| editing.set(false))
                        .tooltip("Show the path as folders"),
                );
        }

        let crumbs: Vec<(Where, String)> = match &self.location {
            Where::Recent => vec![(Where::Recent, "Recent".to_string())],
            Where::Folder(path) => location::crumbs(path, &self.home)
                .into_iter()
                .map(|crumb| (Where::Folder(crumb.path), crumb.label))
                .collect(),
        };
        let location = self.location.clone();
        rect()
            .component("PathBar")
            .tw(
                &tokens,
                &[flex_row(), items(Align::Center), w_full(), gap(S2)],
            )
            .child(
                field.child(
                    rect()
                        .tw(&tokens, &[w_flex()])
                        .child(breadcrumb(crumbs, self.on_open.clone()).flat()),
                ),
            )
            .child({
                let start = PathBar::text_of(&location);
                icon_button(Icon::DocumentEdit, move |_: ()| {
                    draft.set(start.clone());
                    editing.set(true);
                })
                .tooltip("Type a location")
            })
    }
}

/// The folders and files that match the half-typed path, under the field.
/// Pressing one completes the text. `None` when nothing matches.
fn suggestions(
    tokens: &crate::theme::Tokens,
    completion: &Completion,
    typed: &str,
    mut draft: State<String>,
) -> Option<Element> {
    if completion.candidates.is_empty() {
        return None;
    }
    // The text up to and including the last slash stays as typed.
    let head = typed[..typed.rfind('/').map_or(0, |at| at + 1)].to_string();
    let rows = completion
        .candidates
        .iter()
        .take(SUGGESTIONS)
        .map(|candidate: &Candidate| {
            let mut completed = format!("{head}{}", candidate.name);
            if candidate.is_dir {
                completed.push('/');
            }
            let icon = if candidate.is_dir {
                Icon::Folder
            } else {
                Icon::DocumentOpen
            };
            Element::from(Suggestion {
                icon,
                label: candidate.name.clone(),
                on_press: EventHandler::new(move |_: ()| draft.set(completed.clone())),
            })
        });
    Some(
        rect()
            .position(
                Position::new_absolute()
                    .top(Dim::Control.px() + S1.px())
                    .left(0.0),
            )
            .layer(Layer::Relative(SUGGESTION_LAYER))
            .tw(tokens, MENU)
            .tw(tokens, &[w_full()])
            .children(rows)
            .into(),
    )
}

#[derive(PartialEq)]
struct Suggestion {
    icon: Icon,
    label: String,
    on_press: EventHandler<()>,
}

impl Component for Suggestion {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let on_press = self.on_press.clone();
        rect()
            .tw(&tokens, MENU_ITEM)
            .tw(
                &tokens,
                &[surface(Paint::Transparent, hovering(), false, false)],
            )
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| hovering.set(false))
            .on_press(move |event: Event<PressEventData>| {
                event.stop_propagation();
                on_press.call(());
            })
            .child(crate::icons::icon(
                self.icon,
                IconSize::Row,
                Paint::Muted.color(&tokens),
            ))
            .child(
                BODY.label(&tokens, self.label.clone())
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            )
    }
}
