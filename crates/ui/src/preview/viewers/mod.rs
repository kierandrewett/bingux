//! The viewers: one Freya component per [`ViewerId`], each drawing what its
//! previewer loaded.
//!
//! # The contract
//!
//! Every viewer fills the box it is given and keeps that box for its whole
//! life: while it loads, the type's icon stands where the content will be, so
//! nothing shifts when the content arrives. A viewer that cannot show its
//! file draws the [`Card`] with the reason.
//!
//! A viewer takes a [`Stage`] and nothing else, and reads how much to do from
//! [`Stage::depth`]:
//!
//! - [`Depth::Glance`]: a popup, a docked pane or a full-screen sheet. Show the
//!   content and the keys that read it. No toolbar, no action on the file: the
//!   preview is a keystroke from being gone, and a pane follows a selection.
//! - [`Depth::Dedicated`]: the Preview application's own window. Add what a
//!   person who came to look at this one file wants: a [`tool_bar`] of
//!   readings and actions (wrap, text size, Copy, Extract all, Install), as
//!   many as the type deserves.
//!
//! A viewer never compares a [`Mode`] to decide this. The one exception is
//! behaviour that belongs to a frame and not to the amount of chrome: a pane
//! that follows the selection must not start a film (`Stage::pane`).
//!
//! # Adding a viewer
//!
//! Four places, each a few lines, and the compiler finds the ones you miss:
//!
//! 1. **`ViewerId`** (`crates/preview-core/src/registry.rs`): a variant, and
//!    its place in `ViewerId::ALL`.
//! 2. **A previewer** in the same file: a `Previewer` that says how well it
//!    claims a sniffed source (`claim`, 1 to 100: the card is 1, a viewer that
//!    can fail and fall back to another sits below it) and reads what the
//!    viewer needs off the UI thread (`load`, cancellable). Put it in
//!    `Registry::standard()`. When the facts need a new `Loaded` variant, add it
//!    there; a viewer that only needs the file's name and bytes uses
//!    `Loaded::Card`-like unit variants.
//! 3. **This directory**: `<name>.rs` with `pub(super) struct <Name>View
//!    { pub stage: Stage }` implementing `Component`, declared `mod <name>;`
//!    below.
//! 4. **The `match` in [`viewer`]**: one arm. It has no wildcard, so a variant
//!    without a viewer does not compile.
//!
//! The registry test `every_viewer_has_a_previewer` fails for a variant that
//! has no previewer. Nothing else changes: the host, the popup, the pane, the
//! Info panel, the Preview application's chooser and playlist all follow the
//! registry. `docs/bingux-ui.md` ("File preview") has the same list with the
//! font viewer as the worked example.
//!
//! Text a viewer shows goes through `tr!` (`ui-preview-*` in
//! `locale/en-US/bingux-ui.ftl`). Work that blocks (reading a file, running a
//! tool) goes through `crate::task`, never the render.

mod archive;
mod bar;
mod card;
mod folder;
mod font;
mod image;
mod image_helper;
mod media;
mod media_controls;
mod media_logic;
mod pdf;
mod pdf_layout;
mod text;

use std::sync::Arc;

use bingux_preview_core::ViewerId;
use freya::prelude::*;

pub(super) use bar::tool_bar;
pub(super) use card::{Card, Placeholder};
pub use media_controls::{ControlAction, ControlState, MediaControls};
pub use pdf::{DocumentAction, DocumentControls, DocumentState, ZoomChoice};

use super::chrome::{PreviewNav, icon_of};
use super::mode::{Depth, Mode};
use super::subject::Resolved;

/// What every viewer is given: the source, loaded, the owner's wiring and the
/// frame it sits in.
#[derive(Clone, PartialEq)]
pub(super) struct Stage {
    pub resolved: Arc<Resolved>,
    pub nav: PreviewNav,
    pub mode: Mode,
}

impl Stage {
    /// How much the viewer does in this frame.
    pub fn depth(&self) -> Depth {
        self.mode.depth()
    }

    /// Whether the viewer adds its own tools (the Preview application).
    pub fn dedicated(&self) -> bool {
        self.depth() == Depth::Dedicated
    }

    /// Whether this is a docked pane, which follows a list's selection.
    pub fn pane(&self) -> bool {
        self.mode == Mode::Pane
    }

    /// The card of the source: its icon, facts and the reasons a better viewer
    /// did not show it.
    pub fn card(&self) -> Card {
        Card::of(&self.resolved, &self.nav)
    }
}

/// The viewer for `resolved`.
pub(super) fn viewer(resolved: Arc<Resolved>, nav: &PreviewNav, mode: Mode) -> Element {
    let id = resolved.viewer;
    let stage = Stage { resolved, nav: nav.clone(), mode };
    match id {
        ViewerId::Image => image::ImageView { stage }.into(),
        ViewerId::Text => text::TextView { stage }.into(),
        ViewerId::Folder => folder::FolderView { stage }.into(),
        ViewerId::Archive => archive::ArchiveView { stage }.into(),
        ViewerId::Media => media::MediaView { stage }.into(),
        ViewerId::Document => pdf::DocumentView { stage }.into(),
        ViewerId::Font => font::FontView { stage }.into(),
        // What no viewer shows: the desktop's thumbnail and the facts.
        ViewerId::Card => stage.card().into(),
    }
}

/// The icon of the file a viewer shows.
fn icon(resolved: &Resolved) -> crate::files::FileIcon {
    icon_of(&resolved.source, resolved.viewer == ViewerId::Folder)
}
