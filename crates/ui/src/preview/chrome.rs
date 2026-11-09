//! The chrome every preview shares: one slim title line and the Info panel.
//! The title line has the file's icon and name and its position, then at most
//! four buttons: Open, Info, the mode switch, and Close at the trailing end
//! where every Bingux header has it. Full screen is the F key. There is no
//! footer: the facts of the file are in Info.

use bingux_files::Kind;
use bingux_preview_core::PreviewSource;
use freya::prelude::*;

use std::rc::Rc;

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
use crate::zoom_view::{FitMode, ZoomHandle};

/// What a person's last choice of fit or actual size was for a kind of
/// picture, kept by the owner: pixel art and icons are wanted at actual size,
/// photographs fitted. The picture viewer asks `recall` with the file's MIME
/// type when it opens and tells `remember` when the person chooses.
#[derive(Clone)]
pub struct ZoomMemory {
    recall: Rc<dyn Fn(&str) -> Option<FitMode>>,
    remember: EventHandler<(String, FitMode)>,
}

impl ZoomMemory {
    pub fn new(
        recall: impl Fn(&str) -> Option<FitMode> + 'static,
        remember: impl Into<EventHandler<(String, FitMode)>>,
    ) -> Self {
        ZoomMemory {
            recall: Rc::new(recall),
            remember: remember.into(),
        }
    }

    pub(super) fn recall(&self, mime: &str) -> Option<FitMode> {
        (self.recall)(mime)
    }

    pub(super) fn remember(&self, mime: &str, mode: FitMode) {
        self.remember.call((mime.to_owned(), mode));
    }
}

impl PartialEq for ZoomMemory {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.recall, &other.recall) && self.remember == other.remember
    }
}

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
    /// The owner drives the picture's zoom, turn and flip (its toolbar, its
    /// menu), through this handle.
    pub zoom: Option<ZoomHandle>,
    /// The owner remembers fit or actual size for each kind of picture.
    pub zoom_memory: Option<ZoomMemory>,
    /// The owner shows the Info panel: it holds the state, and the title line
    /// it draws has the button.
    pub info: Option<State<bool>>,
    /// The owner shows the page thumbnails and outline of a document: it holds
    /// the state, and its title line has the button.
    pub sidebar: Option<State<bool>>,
    /// Whether an animated picture is playing, kept by the owner so its title
    /// line can show Play and Pause.
    pub playing: Option<State<bool>>,
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
            zoom: None,
            zoom_memory: None,
            info: None,
            sidebar: None,
            playing: None,
        }
    }

    /// Drive the picture's zoom, turn and flip from outside the viewer.
    pub fn zoom(mut self, handle: ZoomHandle) -> Self {
        self.zoom = Some(handle);
        self
    }

    /// Keep each kind of picture's choice of fit or actual size.
    pub fn zoom_memory(mut self, memory: ZoomMemory) -> Self {
        self.zoom_memory = Some(memory);
        self
    }

    /// Own the Info panel's state.
    pub fn info(mut self, shown: State<bool>) -> Self {
        self.info = Some(shown);
        self
    }

    /// Own the state of a document's page thumbnails and outline.
    pub fn sidebar(mut self, shown: State<bool>) -> Self {
        self.sidebar = Some(shown);
        self
    }

    /// Own whether an animated picture is playing.
    pub fn playing(mut self, playing: State<bool>) -> Self {
        self.playing = Some(playing);
        self
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
        PreviewSource::Path(path) => {
            FileIcon::for_path(path, if folder { Kind::Directory } else { Kind::File })
        }
        PreviewSource::Bytes { name, .. } => FileIcon::for_name(name),
    }
}

/// "3 of 22", as one unit: in a right-to-left interface it does not read
/// "of 22 3" (`format::isolate`).
pub(super) fn position_text(position: (usize, usize)) -> String {
    crate::format::isolate(format!("{} of {}", position.0, position.1))
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
        .tw(
            tokens,
            &[
                flex_row(),
                grow_children(),
                items(Align::Center),
                gap(S3),
                w_full(),
            ],
        )
        .maybe_child(
            source.map(|source| {
                file_icon_view(tokens, &icon_of(source, folder), IconSize::Row, false)
            }),
        )
        .child(
            rect().tw(tokens, &[w_flex()]).child(
                path_label(
                    source.map_or_else(|| crate::tr!("ui-preview-nothing"), PreviewSource::name),
                )
                .name()
                .style(SECTION),
            ),
        )
        .maybe_child(nav.position.map(|position| {
            CAPTION
                .in_ink(Ink::Muted)
                .label(tokens, position_text(position))
                .max_lines(1)
        }))
        .into();
    let mut bar = HeaderBar::new("")
        .draggable(false)
        .title_start(true)
        .title_slot(title);
    let show_info = *info.read();
    if let Some(open) = nav.on_open.clone() {
        bar = bar.trailing(
            icon_button(Icon::DocumentOpen, move |_| open.call(()))
                .tooltip(&crate::tr!("ui-preview-open.tooltip")),
        );
    }
    bar = bar.trailing(
        icon_button(Icon::DialogInformation, move |_| {
            let mut info = info;
            let shown = *info.peek();
            info.set(!shown);
        })
        .active(show_info)
        .tooltip(&crate::tr!("ui-preview-info.tooltip")),
    );
    if let Some(switch) = nav.on_mode.clone().filter(|_| !compact && nav.dock) {
        let (icon, tip, to) = match mode {
            Mode::Pane => (
                Icon::WindowNew,
                crate::tr!("ui-preview-popup.tooltip"),
                Mode::Popup,
            ),
            Mode::Popup | Mode::Fullscreen | Mode::Window => (
                Icon::SidebarShow,
                crate::tr!("ui-preview-dock.tooltip"),
                Mode::Pane,
            ),
        };
        bar = bar.trailing(icon_button(icon, move |_| switch.call(to)).tooltip(&tip));
    }
    let close = nav.on_close.clone();
    bar.trailing(
        icon_button(Icon::WindowClose, move |_| close.call(())).tooltip(&if mode == Mode::Pane {
            crate::tr!("ui-preview-hide.tooltip")
        } else {
            crate::tr!("ui-preview-close.tooltip")
        }),
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
                    .child(
                        CAPTION
                            .in_ink(Ink::Muted)
                            .label(&tokens, row.label.clone())
                            .max_lines(1),
                    )
                    .child(BODY.selectable(&tokens, row.value.clone())),
            )
        });
        rect()
            .component("PreviewInfo")
            .tw(
                &tokens,
                &[flex_col(), w_full(), h_full(), bg(Paint::Surface)],
            )
            .child(
                ScrollView::new().child(
                    rect()
                        .tw(&tokens, &[flex_col(), w_full(), p(S4), gap(S3)])
                        .children(rows)
                        .maybe_child(self.on_properties.clone().map(|show| {
                            Element::from(action_button(
                                crate::tr!("ui-preview-properties"),
                                move |_| show.call(()),
                            ))
                        })),
                ),
            )
    }
}
