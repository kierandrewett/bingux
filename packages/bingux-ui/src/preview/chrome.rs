//! The chrome every preview shares: the header, the footer line and the Info
//! panel. One header, in one order: the file's icon and name, its position,
//! then Open, Info, the mode switch, full screen, and Close at the trailing
//! end where every Bingux header has it.

use bingux_files::Kind;
use bingux_preview_core::PreviewSource;
use freya::prelude::*;

use super::info::InfoRow;
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
            on_close: on_close.into(),
        }
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
pub(super) fn icon_of(source: &PreviewSource) -> FileIcon {
    match source {
        PreviewSource::Path(path) => {
            let kind = if path.is_dir() { Kind::Directory } else { Kind::File };
            FileIcon::for_path(path, kind)
        }
        PreviewSource::Bytes { name, .. } => FileIcon::for_name(name),
    }
}

/// "3 of 22".
pub(super) fn position_text(position: (usize, usize)) -> String {
    format!("{} of {}", position.0, position.1)
}

/// The header: icon, name and position share the room the buttons leave, then
/// the buttons. `compact` (a phone) keeps Open, Info and Close.
pub(super) fn header(
    tokens: &Tokens,
    source: Option<&PreviewSource>,
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
        .maybe_child(source.map(|source| file_icon_view(tokens, &icon_of(source), IconSize::Row, false)))
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
    if let Some(switch) = nav.on_mode.clone().filter(|_| mode != Mode::Pane && !compact) {
        let (icon, tip, to) = if mode == Mode::Fullscreen {
            (Icon::ViewRestore, "Leave full screen (F)", Mode::Popup)
        } else {
            (Icon::ViewFullscreen, "Full screen (F)", Mode::Fullscreen)
        };
        bar = bar.trailing(icon_button(icon, move |_| switch.call(to)).tooltip(tip));
    }
    let close = nav.on_close.clone();
    bar.trailing(
        icon_button(Icon::WindowClose, move |_| close.call(())).tooltip(if mode == Mode::Pane { "Hide the preview" } else { "Close (Space)" }),
    )
}

/// The line under the content.
pub(super) fn footer(tokens: &Tokens, text: String) -> Element {
    rect()
        .tw(tokens, &[w_full(), h(Dim::Header), px(S4), flex_row(), items(Align::Center)])
        .child(CAPTION.in_ink(Ink::Muted).label(tokens, text).max_lines(1).text_overflow(TextOverflow::Ellipsis))
        .into()
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
