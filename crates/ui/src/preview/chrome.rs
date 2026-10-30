//! The chrome every preview shares: one slim title line and the Info panel.
//! The title line has the file's icon and name and its position, then at most
//! four buttons: Open, Info, the mode switch, and Close at the trailing end
//! where every Bingux header has it. Full screen is the F key. There is no
//! footer: the facts of the file are in Info.

use bingux_files::Kind;
use bingux_preview_core::PreviewSource;
use freya::prelude::*;

use super::info::InfoRow;
use super::keys::Keyboard;
use super::mode::Mode;
use super::step::Step;
use crate::component::ComponentName;
use crate::controls::{action_button, icon_button, path_label};
use crate::files::{FileIcon, file_icon_view};
use crate::icons::Icon;
use crate::shell::HeaderBar;
use crate::style::*;
use crate::surface::use_tokens;
use crate::theme::Tokens;
use crate::typography::{BODY, CAPTION, Ink, SECTION};

/// What the owner of a preview gives it: where it is in a list, what the
/// buttons do. Every handler is optional except Close; a button whose handler
/// is absent is not drawn.
#[derive(Clone, PartialEq)]
pub struct PreviewNav {
    /// "3 of 22": the file's place among those the arrows reach.
    pub position: Option<(usize, usize)>,
    /// The files are laid out in rows of tiles, so Up and Down step a line.
    pub grid: bool,
    /// The owner can dock a popup into a pane and back: the header shows the
    /// switch.
    pub dock: bool,
    pub on_step: Option<EventHandler<Step>>,
    /// Open the file as a double click does.
    pub on_open: Option<EventHandler<()>>,
    /// Show the Properties dialog.
    pub on_properties: Option<EventHandler<()>>,
    /// Switch to another frame: the owner closes this one and shows that.
    pub on_mode: Option<EventHandler<Mode>>,
    /// A popup takes the keyboard when it opens (see [`PreviewNav::takes_focus`]).
    pub focus: bool,
    /// Whether the preview's own keys (scroll, zoom, play) are live: always
    /// in a popup; in a pane only after a click in it.
    pub(super) keyboard: Keyboard,
    /// Give the keyboard back to the owner's list (a pane's Escape).
    pub on_release: Option<EventHandler<()>>,
    pub on_close: EventHandler<()>,
}

impl PreviewNav {
    /// A preview that can only be closed.
    pub fn new(on_close: impl Into<EventHandler<()>>) -> Self {
        PreviewNav {
            position: None,
            grid: false,
            dock: false,
            on_step: None,
            on_open: None,
            on_properties: None,
            on_mode: None,
            focus: true,
            keyboard: Keyboard::always(),
            on_release: None,
            on_close: on_close.into(),
        }
    }

    /// Whether a popup takes the keyboard when it opens. It does by default;
    /// a preview shown in the middle of a page (the taxonomy's specimens) must
    /// not, or the page scrolls to wherever the last one is.
    pub fn takes_focus(mut self, takes: bool) -> Self {
        self.focus = takes;
        self
    }

    /// The file is number `at` (counting from 1) of `of`.
    pub fn position(mut self, at: usize, of: usize) -> Self {
        self.position = Some((at, of));
        self
    }

    pub fn grid(mut self, grid: bool) -> Self {
        self.grid = grid;
        self
    }

    /// The owner can dock this preview into a pane beside its list.
    pub fn dockable(mut self, dock: bool) -> Self {
        self.dock = dock;
        self
    }

    /// The preview is a docked pane; `held` is true while the pane has the
    /// keyboard for its own keys (the owner clears it when it takes the
    /// keyboard back). The arrows step the file either way.
    pub fn pane_keys(mut self, held: State<bool>) -> Self {
        self.keyboard = Keyboard::pane(held);
        self
    }

    /// What gives the keyboard back to the owner's list.
    pub fn on_release(mut self, handler: impl Into<EventHandler<()>>) -> Self {
        self.on_release = Some(handler.into());
        self
    }

    pub fn on_step(mut self, handler: impl Into<EventHandler<Step>>) -> Self {
        self.on_step = Some(handler.into());
        self
    }

    pub fn on_open(mut self, handler: impl Into<EventHandler<()>>) -> Self {
        self.on_open = Some(handler.into());
        self
    }

    pub fn on_properties(mut self, handler: impl Into<EventHandler<()>>) -> Self {
        self.on_properties = Some(handler.into());
        self
    }

    pub fn on_mode(mut self, handler: impl Into<EventHandler<Mode>>) -> Self {
        self.on_mode = Some(handler.into());
        self
    }
}

/// The icon of a source: its type's, by the one file icon API.
///
/// `folder` is what the caller already knows. This never asks the disk: it runs
/// while a frame is drawn, and on a network mount that is a wait the whole window
/// shares.
pub(super) fn icon_of(source: &PreviewSource, folder: bool) -> FileIcon {
    match source {
        PreviewSource::Path(path) => FileIcon::for_path(path, if folder { Kind::Directory } else { Kind::File }),
        PreviewSource::Bytes { name, .. } => FileIcon::for_name(name),
    }
}

/// "3 of 22".
pub(super) fn position_text(position: (usize, usize)) -> String {
    format!("{} of {}", position.0, position.1)
}

/// The header: icon, name and position share the room the buttons leave, then
/// the buttons. `compact` (a phone) keeps Open, Info and Close. `folder` is
/// whether the source is a folder, as the owner knows it.
pub(super) fn header(
    tokens: &Tokens,
    source: Option<&PreviewSource>,
    folder: bool,
    mode: Mode,
    nav: &PreviewNav,
    info: State<bool>,
    compact: bool,
) -> HeaderBar {
    // `grow_children` is what lets the name's `w_flex` share the row with the
    // counter. Without it the name takes the whole row and pushes the counter
    // out from under its room, over the buttons.
    let title: Element = rect()
        .tw(tokens, &[flex_row(), grow_children(), items(Align::Center), gap(S3), w_full()])
        .maybe_child(source.map(|source| file_icon_view(tokens, &icon_of(source, folder), IconSize::Row, false)))
        .child(
            rect().tw(tokens, &[w_flex()]).child(
                path_label(source.map_or_else(|| "Nothing to preview".to_owned(), PreviewSource::name))
                    .name()
                    .style(SECTION),
            ),
        )
        .maybe_child(
            nav.position
                .map(|position| CAPTION.in_ink(Ink::Muted).label(tokens, position_text(position)).max_lines(1)),
        )
        .into();
    let mut bar = HeaderBar::new("").draggable(false).title_start(true).title_slot(title);
    let show_info = *info.read();
    if let Some(open) = nav.on_open.clone() {
        bar = bar.trailing(icon_button(Icon::DocumentOpen, move |_| open.call(())).tooltip("Open (Enter)"));
    }
    bar = bar.trailing(
        icon_button(Icon::DialogInformation, move |_| {
            let mut info = info;
            let shown = *info.peek();
            info.set(!shown);
        })
        .active(show_info)
        .tooltip("Info (I)"),
    );
    if let Some(switch) = nav.on_mode.clone().filter(|_| !compact && nav.dock) {
        let (icon, tip, to) = match mode {
            Mode::Pane => (Icon::WindowNew, "Open as a popup (Alt+P)", Mode::Popup),
            Mode::Popup | Mode::Fullscreen => (Icon::SidebarShow, "Dock to the side (Alt+P)", Mode::Pane),
        };
        bar = bar.trailing(icon_button(icon, move |_| switch.call(to)).tooltip(tip));
    }
    let close = nav.on_close.clone();
    bar.trailing(
        icon_button(Icon::WindowClose, move |_| close.call(())).tooltip(if mode == Mode::Pane { "Hide the preview" } else { "Close (Space)" }),
    )
}

/// The Info panel: the facts of the file, a name above each value, and the
/// Properties dialog for everything else.
#[derive(PartialEq)]
pub(super) struct InfoPanel {
    pub rows: Vec<InfoRow>,
    pub on_properties: Option<EventHandler<()>>,
}

impl Component for InfoPanel {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let rows = self.rows.iter().map(|row| {
            Element::from(
                rect()
                    .tw(&tokens, &[flex_col(), w_full(), gap(S1)])
                    .child(CAPTION.in_ink(Ink::Muted).label(&tokens, row.label).max_lines(1))
                    .child(BODY.selectable(&tokens, row.value.clone())),
            )
        });
        rect()
            .component("PreviewInfo")
            .tw(&tokens, &[flex_col(), w_full(), h_full(), bg(Paint::Surface)])
            .child(
                ScrollView::new().child(
                    rect()
                        .tw(&tokens, &[flex_col(), w_full(), p(S4), gap(S3)])
                        .children(rows)
                        .maybe_child(self.on_properties.clone().map(|show| {
                            Element::from(action_button("Properties…", move |_| show.call(())))
                        })),
                ),
            )
    }
}
