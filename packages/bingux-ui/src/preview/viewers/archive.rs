//! An archive: what is in it, listed without extracting anything.

use std::path::Path;
use std::sync::Arc;

use bingux_files::{Kind, display};
use bingux_preview_core::Loaded;
use bingux_preview_core::archive::ArchiveListing;
use freya::prelude::*;

use super::super::subject::Resolved;
use crate::component::ComponentName;
use crate::controls::path_label;
use crate::files::{FileIcon, file_icon_view};
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{BODY, CAPTION, Ink};

#[derive(PartialEq)]
pub(super) struct ArchiveView {
    pub resolved: Arc<Resolved>,
}

/// The listing a row of the list draws from.
#[derive(Clone)]
struct ListData {
    listing: Arc<ArchiveListing>,
    /// The list's width: the rows are laid out at it, as the virtual list
    /// gives its items no width of their own.
    width: f32,
}

impl PartialEq for ListData {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.listing, &other.listing) && self.width == other.width
    }
}

/// The room of a size beside a name: wide enough for "999.9 MB".
fn size_room() -> f32 {
    crate::controls::path_label::measure_text("999.9 MB", CAPTION) + 4.0
}

#[derive(PartialEq)]
struct EntryRow {
    data: ListData,
    index: usize,
}

impl Component for EntryRow {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let entry = &self.data.listing.entries[self.index];
        let icon = if entry.is_dir {
            FileIcon::for_path(Path::new(&entry.path), Kind::Directory)
        } else {
            FileIcon::for_name(entry.path.rsplit('/').next().unwrap_or(&entry.path))
        };
        rect()
            .width(Size::px(self.data.width))
            .tw(&tokens, &[flex_row(), h(Dim::MenuItem), items(Align::Center), gap(S3), px(S4)])
            .child(file_icon_view(&tokens, &icon, IconSize::Row, false))
            .child(rect().tw(&tokens, &[w_flex(), clip()]).child(path_label(entry.path.clone()).style(BODY)))
            .child(
                rect()
                    .width(Size::px(size_room()))
                    .tw(&tokens, &[flex_row(), justify(Align::End)])
                    .maybe_child(entry.size.map(|size| CAPTION.in_ink(Ink::Muted).label(&tokens, display::size(size)).max_lines(1))),
            )
    }
}

impl Component for ArchiveView {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let Loaded::Archive(listing) = &self.resolved.loaded else {
            return Element::from(rect());
        };
        let mut width = use_state(|| 0.0f32);
        let listing = Arc::new(listing.clone());
        let (files, bytes) = listing.totals();
        let summary = format!(
            "{} · {} {}{}{}",
            listing.format.name(),
            files,
            if files == 1 { "file" } else { "files" },
            if listing.truncated { " or more" } else { "" },
            if bytes > 0 { format!(" · {} unpacked", display::size(bytes)) } else { String::new() }
        );
        let data = ListData {
            listing: listing.clone(),
            width: *width.read(),
        };
        let list = VirtualScrollView::new_with_data(data, |item, data: &ListData| {
            Element::from(EntryRow {
                data: data.clone(),
                index: item.index,
            })
        })
        .length(listing.entries.len())
        .item_size(Dim::MenuItem.px())
        .on_sized(move |event: Event<SizedEventData>| width.set_if_modified(event.area.width()));
        rect()
            .component("PreviewArchive")
            .tw(&tokens, &[w_full(), h_full(), flex_col(), grow_children()])
            .child(
                rect()
                    .tw(&tokens, &[w_full(), h(Dim::Control), px(S4), flex_row(), items(Align::Center), bg(Paint::Surface)])
                    .child(CAPTION.in_ink(Ink::Muted).label(&tokens, summary).max_lines(1).text_overflow(TextOverflow::Ellipsis)),
            )
            .child(rect().tw(&tokens, &[w_full(), h(Dim::Hairline), bg(Paint::Separator)]))
            .child(rect().width(Size::fill()).height(Size::flex(1.0)).min_height(Size::px(0.0)).child(list))
            .into_element()
    }
}
