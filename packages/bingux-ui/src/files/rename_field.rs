//! The name of a file as an editable field, in place of its label.
//!
//! The field opens with the name selected up to its extension, so typing
//! replaces the name and keeps `.txt`. Enter renames, Escape leaves the name
//! as it was, and clicking elsewhere renames too, as GNOME Files does. What is
//! wrong with the name shows under the field as you type: empty, a slash,
//! `.` or `..`, too long, or taken by another entry of the folder. A rename
//! the disk refuses (no permission, or the name was taken a moment ago) shows
//! in the same place, and the field stays open.

use std::path::PathBuf;
use std::sync::Arc;

use bingux_files::Entry;
use freya::prelude::*;

use super::actions::{self, Env, check_name, set, stem_chars};
use crate::component::ComponentName;
use crate::controls::text_field;
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{CAPTION, Ink};

/// An entry's name, as a field.
pub(super) struct RenameField {
    env: Env,
    path: PathBuf,
    name: String,
    is_dir: bool,
    grid: bool,
    /// Wider or narrower than its parent; `None` fills it.
    width: Option<f32>,
    /// Where focus goes back to when the field closes.
    list: Option<AccessibilityId>,
}

impl RenameField {
    pub(super) fn new(env: &Env, entry: &Entry) -> Self {
        RenameField {
            env: env.clone(),
            path: entry.path.clone(),
            name: entry.name.clone(),
            is_dir: entry.is_dir(),
            grid: false,
            width: None,
            list: None,
        }
    }

    /// A tile of the grid: a compact field.
    pub(super) fn grid(mut self, grid: bool) -> Self {
        self.grid = grid;
        self
    }

    /// The field is this wide, in logical pixels, whatever its parent is.
    pub(super) fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// The list the field's focus returns to when the rename ends.
    pub(super) fn list(mut self, list: AccessibilityId) -> Self {
        self.list = Some(list);
        self
    }
}

impl PartialEq for RenameField {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path
            && self.name == other.name
            && self.is_dir == other.is_dir
            && self.grid == other.grid
            && self.width == other.width
            && self.list == other.list
            && Arc::ptr_eq(&self.env.listing, &other.env.listing)
    }
}

/// How a rename ended, from the field's point of view.
#[derive(Clone, Copy, PartialEq)]
enum How {
    /// Enter: a problem keeps the field open.
    Enter,
    /// Focus left: a problem closes the field and is reported.
    Blur,
}

/// What is wrong with `text` as the new name of `current`, if anything.
fn problem_with(env: &Env, current: &str, text: &str) -> Option<String> {
    check_name(text, Some(current), |name| {
        env.listing.entries.iter().any(|entry| entry.name == name)
    })
    .err()
    .map(|problem| problem.message().to_owned())
}

impl Component for RenameField {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let env = self.env.clone();
        let path = self.path.clone();
        let current = self.name.clone();
        let value = use_state(|| current.clone());
        let mut refused = use_state::<Option<String>>(|| None);
        let working = use_state(|| false);
        let mut had_focus = use_state(|| false);
        let mut menu_held = use_state(|| false);
        let id = use_a11y();
        let list = self.list;
        let focus = use_focus(id);

        let commit = {
            let env = env.clone();
            let path = path.clone();
            let current = current.clone();
            move |text: String, how: How| {
                if *working.peek() {
                    return;
                }
                // The field's focus goes with it, to the list, unless the
                // person already moved it to something else.
                let end = {
                    let ui = env.ui;
                    move || {
                        set(ui.rename, None);
                        if let Some(list) = list {
                            let now = *Platform::get().current_window().focused_accessibility_id.peek();
                            if now == id || now == freya_core::accessibility::tree::ACCESSIBILITY_ROOT_ID {
                                list.request_focus();
                            }
                        }
                    }
                };
                if text == current {
                    end();
                    return;
                }
                if let Some(message) = problem_with(&env, &current, &text) {
                    match how {
                        How::Enter => set(refused, Some(message)),
                        How::Blur => {
                            end();
                            actions::notify(env.ui, message, true);
                        }
                    }
                    return;
                }
                set(working, true);
                let ui = env.ui;
                actions::rename(&env, path.clone(), text, move |outcome| {
                    set(working, false);
                    match (outcome, how) {
                        (Ok(()), _) => end(),
                        (Err(message), How::Enter) => set(refused, Some(message)),
                        (Err(message), How::Blur) => {
                            end();
                            actions::notify(ui, message, true);
                        }
                    }
                });
            }
        };

        // Leaving the field renames, once it has had focus to leave. Focus
        // moving to the field's own edit menu is not leaving: the menu hands
        // it back when it closes, and the field asks for it if the menu
        // closes some other way, so a right click never ends the edit.
        let focused = focus().is_focused();
        use_side_effect_with_deps(&focused, {
            let commit = commit.clone();
            let overlays = env.overlays;
            move |focused: &bool| {
                if *focused {
                    had_focus.set(true);
                    menu_held.set(false);
                } else if *had_focus.peek() {
                    if overlays.menu_owned_by(id) {
                        menu_held.set(true);
                    } else if *menu_held.peek() {
                        id.request_focus();
                    } else {
                        commit(value.peek().clone(), How::Blur);
                    }
                }
            }
        });

        let text = value.read().clone();
        let message = problem_with(&env, &current, &text).or_else(|| refused.read().clone());
        let field = text_field(value, {
            let commit = commit.clone();
            move |text: String| commit(text, How::Enter)
        })
        .fill_width()
        .autofocus()
        .a11y_id(id)
        .revert_to(current.clone())
        .danger_outline(message.is_some())
        .select_range(0, stem_chars(&current, self.is_dir))
        .on_change(move |_: String| refused.set(None));
        let field = if self.grid { field.dense() } else { field };

        // The message floats under the field, so a row keeps its height.
        let below = if self.grid {
            Dim::DenseControl.px()
        } else {
            Dim::Control.px()
        };
        let note = message.map(|message| {
            rect()
                .layer(crate::layers::INLINE_EDITOR)
                .position(Position::new_absolute().left(0.0).top(below))
                .tw(
                    &tokens,
                    &[
                        px(S2),
                        py(S1),
                        rounded(Radius::Control),
                        bg(Paint::Popover),
                        border(Line::Hairline),
                        shadow(Elevation::Raised),
                    ],
                )
                .a11y_role(AccessibilityRole::Alert)
                .child(CAPTION.in_ink(Ink::Danger).label(&tokens, message).max_lines(2))
        });
        let mut column = rect().component("RenameField");
        column = match self.width {
            Some(width) => column.width(Size::px(width)),
            None => column.width(Size::fill()),
        };
        column.child(field).maybe_child(note)
    }
}
