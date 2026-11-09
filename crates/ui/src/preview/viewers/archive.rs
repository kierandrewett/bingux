//! An archive: what is in it, listed without extracting anything.
//!
//! The Preview window adds Extract all beside the totals: the archive is
//! unpacked into a new folder beside it (never over anything), and a toast
//! offers Show in Files. A quick look only lists.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use bingux_files::Kind;

use crate::files::display;
use bingux_preview_core::Loaded;
use bingux_preview_core::archive::ArchiveListing;
use freya::prelude::*;

use super::{Stage, tool_bar};
use crate::component::ComponentName;
use crate::controls::{action_button, path_label};
use crate::files::{FileIcon, file_icon_view};
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{BODY, CAPTION, Ink};

#[derive(PartialEq)]
pub(super) struct ArchiveView {
    pub stage: Stage,
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
            .tw(
                &tokens,
                &[
                    flex_row(),
                    h(Dim::MenuItem),
                    items(Align::Center),
                    gap(S3),
                    px(S4),
                ],
            )
            .child(file_icon_view(&tokens, &icon, IconSize::Row, false))
            .child(
                rect()
                    .tw(&tokens, &[w_flex(), clip()])
                    .child(path_label(entry.path.clone()).style(BODY)),
            )
            .child(
                rect()
                    .width(Size::px(size_room()))
                    .tw(&tokens, &[flex_row(), justify(Align::End)])
                    .maybe_child(entry.size.map(|size| {
                        CAPTION
                            .in_ink(Ink::Muted)
                            .label(&tokens, display::size(size))
                            .max_lines(1)
                    })),
            )
    }
}

/// What the totals line says about an archive: its kind, how many files and
/// how much they unpack to.
fn summary(listing: &ArchiveListing) -> String {
    let (files, bytes) = listing.totals();
    let count = if listing.truncated {
        crate::tr!("ui-preview-archive-files-or-more", count = files)
    } else {
        crate::tr!("ui-preview-archive-files", count = files)
    };
    let mut parts = vec![listing.format.name().to_owned(), count];
    if bytes > 0 {
        parts.push(crate::tr!(
            "ui-preview-archive-unpacked",
            size = display::size(bytes)
        ));
    }
    parts.join(" · ")
}

/// Unpack `archive` into a new folder beside it, then say where it went.
fn extract_all(archive: PathBuf, overlays: Option<crate::shell::Overlays>, mut busy: State<bool>) {
    let Some(folder) = archive.parent().map(Path::to_path_buf) else {
        return;
    };
    busy.set(true);
    spawn(async move {
        let result = crate::task::spawn_blocking(move || bingux_files::ops::extract(&archive, &folder)).await;
        busy.set(false);
        let Some(overlays) = overlays else {
            return;
        };
        match result {
            Ok(made) => {
                let name = made
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                overlays.toast_with_action(
                    crate::tr!("ui-preview-archive-extracted", name = name),
                    crate::tr!("ui-preview-archive-show-in-files"),
                    move |_: ()| {
                        let _ = crate::files::show_in_files(&made);
                    },
                );
            }
            Err(_) => overlays.toast(crate::tr!("ui-preview-archive-extract-failed")),
        }
    });
}

impl Component for ArchiveView {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let Loaded::Archive(listing) = &self.stage.resolved.loaded else {
            return Element::from(rect());
        };
        let mut width = use_state(|| 0.0f32);
        let busy = use_state(|| false);
        let overlays = use_try_consume::<crate::shell::Overlays>();
        let listing = Arc::new(listing.clone());
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
        // Extract all needs the archive on disk and a program that opens it.
        let extractable = self
            .stage
            .resolved
            .source
            .as_path()
            .filter(|path| self.stage.dedicated() && bingux_files::ops::can_extract(path))
            .map(Path::to_path_buf);
        let tools = extractable
            .map(|archive| {
                let busy_now = *busy.read();
                Element::from(
                    action_button(crate::tr!("ui-preview-archive-extract-all"), move |_| {
                        if !*busy.peek() {
                            extract_all(archive.clone(), overlays, busy);
                        }
                    })
                    .compact()
                    .loading(busy_now),
                )
            })
            .into_iter()
            .collect();
        rect()
            .component("PreviewArchive")
            .tw(&tokens, &[w_full(), h_full(), flex_col(), grow_children()])
            .child(tool_bar(&tokens, summary(&listing), tools))
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .min_height(Size::px(0.0))
                    .child(list),
            )
            .into_element()
    }
}
