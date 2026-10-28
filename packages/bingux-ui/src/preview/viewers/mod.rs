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
mod text;

use std::sync::Arc;

use bingux_preview_core::ViewerId;
use freya::prelude::*;

pub(super) use card::{Card, Placeholder};

use super::chrome::{PreviewNav, icon_of};
use super::mode::Mode;
use super::subject::Resolved;

/// The viewer for `resolved`.
pub(super) fn viewer(resolved: Arc<Resolved>, nav: &PreviewNav, mode: Mode) -> Element {
    let _ = mode;
    match resolved.viewer {
        ViewerId::Image => image::ImageView { resolved }.into(),
        ViewerId::Text => text::TextView { resolved }.into(),
        ViewerId::Folder => folder::FolderView { resolved }.into(),
        ViewerId::Archive => archive::ArchiveView { resolved }.into(),
        // Audio, video and documents: the desktop's thumbnail and the facts.
        ViewerId::Media | ViewerId::Document | ViewerId::Font | ViewerId::Card => Card::of(&resolved, nav).into(),
    }
}

/// The icon of the file a viewer shows.
fn icon(resolved: &Resolved) -> crate::files::FileIcon {
    icon_of(&resolved.source)
}
