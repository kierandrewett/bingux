//! The viewers: one Freya component per [`ViewerId`], each drawing what its
//! previewer loaded.
//!
//! Every viewer fills the box it is given and keeps that box for its whole
//! life: while it loads, the type's icon stands where the content will be, so
//! nothing shifts when the content arrives. A viewer that cannot show its
//! file draws the [`Card`] with the reason.

mod archive;
mod card;
mod folder;
mod image;
mod media;
mod media_controls;
mod media_logic;
mod pdf;
mod pdf_layout;
mod text;

use std::sync::Arc;

use bingux_preview_core::ViewerId;
use freya::prelude::*;

pub(super) use card::{Card, Placeholder};
pub use media_controls::{ControlAction, ControlState, MediaControls};
pub use pdf::{DocumentAction, DocumentControls, DocumentState, ZoomChoice};

use super::chrome::{PreviewNav, icon_of};
use super::mode::Mode;
use super::subject::Resolved;

/// The viewer for `resolved`.
pub(super) fn viewer(resolved: Arc<Resolved>, nav: &PreviewNav, mode: Mode) -> Element {
    match resolved.viewer {
        ViewerId::Image => image::ImageView { resolved }.into(),
        ViewerId::Text => text::TextView { resolved }.into(),
        ViewerId::Folder => folder::FolderView { resolved }.into(),
        ViewerId::Archive => archive::ArchiveView { resolved }.into(),
        ViewerId::Media => media::MediaView { resolved, nav: nav.clone(), mode }.into(),
        ViewerId::Document => pdf::DocumentView { resolved, nav: nav.clone(), mode }.into(),
        // Fonts and the rest: the desktop's thumbnail and the facts.
        ViewerId::Font | ViewerId::Card => Card::of(&resolved, nav).into(),
    }
}

/// The icon of the file a viewer shows.
fn icon(resolved: &Resolved) -> crate::files::FileIcon {
    icon_of(&resolved.source)
}
