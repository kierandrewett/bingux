//! The Details tab of the Properties dialog: the facts of a picture or a
//! recording, the file's SHA-256 on demand, what the file system keeps (inode,
//! device, links, block size, disk use) and the extended attributes.
//!
//! The checksum is worked out on a worker thread that reports as it reads, so
//! the bar moves, Stop is heard within a block, and nothing waits on it. Its
//! rows are there before it is asked for, and its two lines of digits are
//! reserved, so the answer moves nothing. The extended attributes come last
//! in the tab, so a file with many of them only makes the tab longer.

use bingux_files::ops::{HashOutcome, HashProgress};
use bingux_preview_core::media::{self, AvInfo, ImageInfo};
use bingux_files::Category;
use freya::prelude::*;

use super::properties::Page;
use super::properties_words::{
    NONE, PENDING, device_line, digest_lines, disk_usage, grouped, read_line,
};
use crate::controls::{action_button, progress_bar};
use crate::group::Group;
use crate::rows::{property_row, row};
use crate::shell::Overlays;
use crate::style::*;
use crate::typography::{CAPTION, Ink, TextStyle};

/// The checksum, from the dialog that owns its worker.
pub(super) struct HashView {
    /// `None` until it is asked for.
    pub progress: Option<HashProgress>,
    pub on_start: EventHandler<()>,
    pub on_stop: EventHandler<()>,
    pub overlays: Overlays,
}

pub(super) fn details_tab(page: &Page, hash: &HashView) -> Vec<Element> {
    let mut groups: Vec<Element> = Vec::new();
    if let Some(media) = media_group(page) {
        groups.push(media.into());
    }
    if !page.is_folder() {
        groups.push(checksum(page, hash).into());
    }
    groups.push(storage(page).into());
    groups.push(attributes(page).into());
    groups
}

/// The size, camera and exposure of a picture, or the length and codecs of a
/// recording.
fn media_group(page: &Page) -> Option<Group> {
    let first = page.first();
    match first.category() {
        Category::Image => {
            let mut image = Group::titled("Image").row(property_row(
                "Dimensions",
                page.fact(|facts| {
                    facts.image.as_ref().map(|image| match image.megapixels() {
                        Some(mega) => format!("{} ({mega})", image.dimensions()),
                        None => image.dimensions(),
                    })
                }),
            ));
            // Only JPEG carries the camera's notes that are read.
            if first.mime.mime == "image/jpeg" {
                for (title, of) in [
                    ("Camera", exif_camera as fn(&ImageInfo) -> Option<String>),
                    ("Lens", exif_lens),
                    ("Taken", exif_taken),
                    ("Exposure", exif_exposure),
                ] {
                    image = image.row(property_row(
                        title,
                        page.fact(|facts| facts.image.as_ref().and_then(of)),
                    ));
                }
            }
            Some(image)
        }
        Category::Audio | Category::Video => Some(
            Group::titled(if first.category() == Category::Audio {
                "Audio"
            } else {
                "Video"
            })
            .row(property_row(
                "Duration",
                page.fact(|facts| facts.av.as_ref().and_then(|av| av.seconds).map(media::clock)),
            ))
            .row(property_row(
                "Codec",
                page.fact(|facts| facts.av.as_ref().and_then(AvInfo::codec_line)),
            )),
        ),
        _ => None,
    }
}

fn exif_camera(image: &ImageInfo) -> Option<String> {
    image.exif.camera.clone()
}

fn exif_lens(image: &ImageInfo) -> Option<String> {
    image.exif.lens.clone()
}

fn exif_taken(image: &ImageInfo) -> Option<String> {
    image.exif.taken.clone()
}

fn exif_exposure(image: &ImageInfo) -> Option<String> {
    image.exif.exposure.clone()
}

/// The SHA-256 of the file: asked for with a button, read in the background
/// with a bar and a Stop button, then shown in two lines that can be selected
/// and copied.
fn checksum(page: &Page, hash: &HashView) -> Group {
    let tokens = page.tokens;
    let progress = hash.progress.as_ref();
    let outcome = progress.and_then(|progress| progress.outcome.as_ref());
    let running = progress.is_some() && outcome.is_none();
    let digest = match outcome {
        Some(HashOutcome::Done(digest)) => Some(digest.clone()),
        _ => None,
    };

    let action: Element = if running {
        let stop = hash.on_stop.clone();
        action_button("Stop", move |_| stop.call(())).compact().into_element()
    } else if let Some(digest) = digest.clone() {
        let overlays = hash.overlays;
        action_button("Copy", move |_| {
            match freya::clipboard::Clipboard::set(digest.clone()) {
                Ok(()) => overlays.toast("Copied."),
                Err(_) => overlays.toast("The text could not be copied."),
            }
        })
        .compact()
        .into_element()
    } else {
        let start = hash.on_start.clone();
        action_button("Calculate", move |_| start.call(())).compact().into_element()
    };

    // One line under the bar, always: how far, or why there is no digest.
    let (status, ink) = match (progress, outcome) {
        (None, _) => (" ".to_owned(), Ink::Muted),
        (Some(progress), None) => (read_line(progress.done, progress.total), Ink::Muted),
        (Some(_), Some(HashOutcome::Done(_))) => ("Done".to_owned(), Ink::Muted),
        (Some(_), Some(HashOutcome::Cancelled)) => ("Stopped".to_owned(), Ink::Muted),
        (Some(_), Some(HashOutcome::Failed(message))) => (message.clone(), Ink::Danger),
    };
    let fraction = progress.map_or(0.0, HashProgress::fraction);
    let (first, second) = digest
        .as_deref()
        .map_or_else(|| (NONE.to_owned(), " ".to_owned()), digest_lines);
    let mono = TextStyle { mono: true, ..CAPTION };
    let block = rect()
        .tw(&tokens, &[flex_col(), w_full(), gap(S2)])
        .child(progress_bar(fraction))
        .child(
            CAPTION.in_ink(ink)
                .label(&tokens, status)
                .width(Size::fill())
                .max_lines(1)
                .text_overflow(TextOverflow::Ellipsis),
        )
        .child(mono.selectable(&tokens, first).max_lines(1).width(Size::fill()))
        .child(mono.selectable(&tokens, second).max_lines(1).width(Size::fill()));

    Group::titled("Checksum")
        .row(row("SHA-256").trailing(action).trailing_control())
        .content(block)
}

/// What the file system keeps about the file.
fn storage(page: &Page) -> Group {
    let details = |of: fn(&bingux_files::ops::FileDetails) -> String| {
        page.fact(move |facts| facts.details.as_ref().map(of))
    };
    Group::titled("Storage")
        .row(property_row("Inode", details(|details| grouped(details.inode))))
        .row(property_row("Device", details(device_line)))
        .row(property_row("Hard links", details(|details| grouped(details.hard_links))))
        .row(property_row(
            "Block size",
            details(|details| format!("{} bytes", grouped(details.block_size))),
        ))
        .row(property_row("Disk usage", details(disk_usage)))
}

/// The extended attributes, one row each. Before they are read there is one
/// row that says so; they are the last group, so a long list moves nothing.
fn attributes(page: &Page) -> Group {
    let mut group = Group::titled("Extended attributes");
    match page.loaded.and_then(|facts| facts.details.as_ref()) {
        Some(details) if !details.xattrs.is_empty() => {
            for attribute in &details.xattrs {
                group = group.row(property_row(attribute.name.clone(), attribute.value.clone()));
            }
        }
        Some(_) => group = group.row(property_row("None", NONE)),
        None => {
            let waiting = if page.loaded.is_some() || page.failed.is_some() { NONE } else { PENDING };
            group = group.row(property_row("Attributes", waiting));
        }
    }
    group
}
