//! The address bar of the file views: where you are as a trail of folders,
//! or a path you type.
//!
//! The trail ([`address_crumbs`](super::address_crumbs)) is the resting
//! state. A press on its empty end, a double click, Ctrl+L or a typed `/` or
//! `~` turns it into a text field ([`address_edit`](super::address_edit)) with
//! completions. On a phone the trail is one row with the folder's name.
//!
//! # Two states, one frame
//!
//! The bar is one row of one height, and it is the same size in both of its
//! states, so nothing around it moves when it changes:
//!
//! | State | Shows | Entered by |
//! | ----- | ----- | ---------- |
//! | Trail | crumbs | resting; Escape, or a folder opened |
//! | Path | the text field, with folder completions | Ctrl+L, a press on the trail's empty end, `/` or `~` typed on the list |
//!
//! Search is not here: it has its own bar under the toolbar
//! ([`search_bar`](super::search_bar)), so the address bar only ever holds a
//! place.

use std::path::PathBuf;

use freya::prelude::*;

use super::address_crumbs::{CompactTrail, Crumbs};
use super::address_edit::Edit;
use super::directory::Where;
use super::file_icon::FileKind;
use super::named::Roots;
use crate::component::ComponentName;
use crate::icons::{self, Icon};
use crate::style::*;
use crate::surface::use_tokens;

/// The text a person edits for `location`: its path with a trailing slash,
/// or the address of a named root (`recent:///`, `trash:///`; see
/// [`Roots::edit_text`]).
pub(super) fn text_of(location: &Where) -> String {
    Roots::current().edit_text(location)
}

/// The address bar. It shows `location` as a trail. Each crumb opens its
/// folder, and the chevron after it lists the folders inside. In the text
/// field a person types or pastes a path (`~`, `..`, relative paths and
/// `file://` work), sees the folders that match, and presses Enter. The bar
/// looks at what was typed itself, off the UI thread, and calls `on_open`
/// with a folder, `on_file` with a file, or says what is wrong.
///
/// `editing` and `draft` belong to the caller, so a shortcut (Ctrl+L, or `/`
/// and `~` typed over the list) can start typing.
#[derive(PartialEq)]
pub struct AddressBar {
    location: Where,
    home: PathBuf,
    editing: State<bool>,
    draft: State<String>,
    on_open: EventHandler<Where>,
    on_file: EventHandler<PathBuf>,
    folders_only: bool,
    accepts_file: bool,
    show_hidden: bool,
    on_close_edit: Option<EventHandler<()>>,
    on_bookmarks: Option<EventHandler<()>>,
}

impl AddressBar {
    /// `on_open` receives a folder a person opened, by a crumb, a list or the
    /// field; `on_file` a file typed in full, when the bar accepts one.
    pub fn new(
        location: Where,
        home: PathBuf,
        editing: State<bool>,
        draft: State<String>,
        on_open: impl Into<EventHandler<Where>>,
        on_file: impl Into<EventHandler<PathBuf>>,
    ) -> Self {
        AddressBar {
            location,
            home,
            editing,
            draft,
            on_open: on_open.into(),
            on_file: on_file.into(),
            folders_only: false,
            accepts_file: false,
            show_hidden: false,
            on_close_edit: None,
            on_bookmarks: None,
        }
    }

    /// Suggest only folders, for choosing a folder.
    pub fn folders_only(mut self, folders_only: bool) -> Self {
        self.folders_only = folders_only;
        self
    }

    /// A typed path that is a file is handed to `on_file`. Without this it is
    /// an error: "That is a file, not a folder."
    pub fn accepts_file(mut self, accepts: bool) -> Self {
        self.accepts_file = accepts;
        self
    }

    /// List hidden folders in a crumb's list.
    pub fn show_hidden(mut self, show: bool) -> Self {
        self.show_hidden = show;
        self
    }

    /// Called when the text field closes, by Escape, by leaving it untouched
    /// or by opening something, so the caller can return the keyboard to its
    /// list.
    pub fn on_close_edit(mut self, on_close: impl Into<EventHandler<()>>) -> Self {
        self.on_close_edit = Some(on_close.into());
        self
    }

    /// Called after a crumb's menu added or removed a bookmark, so a sidebar
    /// can read the bookmarks again.
    pub fn on_bookmarks_changed(mut self, on_changed: impl Into<EventHandler<()>>) -> Self {
        self.on_bookmarks = Some(on_changed.into());
        self
    }

    /// The text a person edits for `location`: its path with a trailing slash.
    pub fn text_of(location: &Where) -> String {
        text_of(location)
    }
}

impl Component for AddressBar {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut editing = self.editing;
        let mut draft = self.draft;
        // The glyphs the field brings (its folder or search mark, its close
        // button) are rasterised now, in the colours drawn now, so the first
        // frame of the field has them and the swap from the trail shows no
        // frame without its icon. A glyph already cached costs a lookup.
        let text = Paint::Text.color(&tokens);
        let muted = Paint::Muted.color(&tokens);
        icons::warm(FileKind::Folder.glyph(), IconSize::Control, muted);
        for icon in [Icon::WindowClose, Icon::Pencil] {
            icons::warm(icon, IconSize::Control, text);
        }
        let content: Element = if *editing.read() {
            Edit {
                location: self.location.clone(),
                home: self.home.clone(),
                editing: self.editing,
                draft: self.draft,
                folders_only: self.folders_only,
                accepts_file: self.accepts_file,
                show_hidden: self.show_hidden,
                on_open: self.on_open.clone(),
                on_file: self.on_file.clone(),
                on_close: self.on_close_edit.clone(),
            }
            .into()
        } else {
            let location = self.location.clone();
            let on_edit = EventHandler::new(move |_: ()| {
                draft.set(text_of(&location));
                editing.set(true);
            });
            if is_compact() {
                CompactTrail {
                    location: self.location.clone(),
                    home: self.home.clone(),
                    on_open: self.on_open.clone(),
                    on_edit,
                }
                .into()
            } else {
                Crumbs {
                    location: self.location.clone(),
                    home: self.home.clone(),
                    show_hidden: self.show_hidden,
                    on_open: self.on_open.clone(),
                    on_edit,
                    on_bookmarks: self.on_bookmarks.clone(),
                }
                .into()
            }
        };
        rect()
            .component("AddressBar")
            .tw(
                &tokens,
                &[
                    flex_row(),
                    grow_children(),
                    items(Align::Center),
                    w_full(),
                    gap(S2),
                ],
            )
            .child(content)
    }
}
