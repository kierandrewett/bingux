//! The Details tab of the Properties dialog: the facts of a picture or a
//! recording, the file's SHA-256 on demand, what the file system keeps (inode,
//! device, links, block size, disk use) and the extended attributes.
//!
//! The checksum is worked out on a worker thread that reports as it reads, so
//! the bar moves, Stop is heard within a block, and nothing waits on it. Its
//! rows are there before it is asked for, and its two lines of digits are
//! reserved, so the answer moves nothing. The extended attributes come last
//! in the tab, so a file with many of them only makes the tab longer.

use bingux_files::Category;
use bingux_files::ops::{HashOutcome, HashProgress};
use bingux_preview_core::media::{self, AvInfo, ImageInfo};
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
            let mut image = Group::titled(&crate::tr!("ui-files-props-image")).row(property_row(
                crate::tr!("ui-files-props-dimensions"),
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
                    (
                        crate::tr!("ui-files-props-camera"),
                        exif_camera as fn(&ImageInfo) -> Option<String>,
                    ),
                    (crate::tr!("ui-files-props-lens"), exif_lens),
                    (crate::tr!("ui-files-props-taken"), exif_taken),
                    (crate::tr!("ui-files-props-exposure"), exif_exposure),
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
            Group::titled(&if first.category() == Category::Audio {
                crate::tr!("ui-files-props-audio")
            } else {
                crate::tr!("ui-files-props-video")
            })
            .row(property_row(
                crate::tr!("ui-files-props-duration"),
                page.fact(|facts| {
                    facts
                        .av
                        .as_ref()
                        .and_then(|av| av.seconds)
                        .map(media::clock)
                }),
            ))
            .row(property_row(
                crate::tr!("ui-files-props-codec"),
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
        action_button(crate::tr!("ui-files-props-stop"), move |_| stop.call(()))
            .compact()
            .into_element()
    } else if let Some(digest) = digest.clone() {
        let overlays = hash.overlays;
        action_button(crate::tr!("ui-files-props-copy"), move |_| {
            match freya::clipboard::Clipboard::set(digest.clone()) {
                Ok(()) => overlays.toast(crate::tr!("ui-files-props-copied")),
                Err(_) => overlays.toast(crate::tr!("ui-files-props-copy-failed")),
            }
        })
        .compact()
        .into_element()
    } else {
        let start = hash.on_start.clone();
        action_button(crate::tr!("ui-files-props-calculate"), move |_| {
            start.call(())
        })
        .compact()
        .into_element()
    };

    // One line under the bar, always: how far, or why there is no digest.
    let (status, ink) = match (progress, outcome) {
        (None, _) => (" ".to_owned(), Ink::Muted),
        (Some(progress), None) => (read_line(progress.done, progress.total), Ink::Muted),
        (Some(_), Some(HashOutcome::Done(_))) => {
            (crate::tr!("ui-files-props-hash-done"), Ink::Muted)
        }
        (Some(_), Some(HashOutcome::Cancelled)) => {
            (crate::tr!("ui-files-props-hash-stopped"), Ink::Muted)
        }
        (Some(_), Some(HashOutcome::Failed(message))) => (message.clone(), Ink::Danger),
    };
    let fraction = progress.map_or(0.0, HashProgress::fraction);
    let (first, second) = digest
        .as_deref()
        .map_or_else(|| (NONE.to_owned(), " ".to_owned()), digest_lines);
    let mono = TextStyle {
        mono: true,
        ..CAPTION
    };
    let block = rect()
        .tw(&tokens, &[flex_col(), w_full(), gap(S2)])
        .child(progress_bar(fraction))
        .child(crate::controls::select_text(status).style(CAPTION.in_ink(ink)))
        .child(
            crate::controls::select_text(first)
                .style(mono)
                .breaks_anywhere(),
        )
        .child(
            crate::controls::select_text(second)
                .style(mono)
                .breaks_anywhere(),
        );

    Group::titled(&crate::tr!("ui-files-props-checksum"))
        .row(
            row(crate::tr!("ui-files-props-sha-256"))
                .trailing(action)
                .trailing_control(),
        )
        .content(block)
}

/// What the file system keeps about the file.
fn storage(page: &Page) -> Group {
    let details = |of: fn(&bingux_files::ops::FileDetails) -> String| {
        page.fact(move |facts| facts.details.as_ref().map(of))
    };
    Group::titled(&crate::tr!("ui-files-props-storage"))
        .row(property_row(
            crate::tr!("ui-files-props-inode"),
            details(|details| grouped(details.inode)),
        ))
        .row(property_row(
            crate::tr!("ui-files-props-device"),
            details(device_line),
        ))
        .row(property_row(
            crate::tr!("ui-files-props-hard-links"),
            details(|details| grouped(details.hard_links)),
        ))
        .row(property_row(
            crate::tr!("ui-files-props-block-size"),
            details(|details| {
                crate::tr!("ui-files-props-bytes", count = grouped(details.block_size))
            }),
        ))
        .row(property_row(
            crate::tr!("ui-files-props-disk-usage"),
            details(disk_usage),
        ))
}

/// The extended attributes, one row each. Before they are read there is one
/// row that says so; they are the last group, so a long list moves nothing.
fn attributes(page: &Page) -> Group {
    let mut group = Group::titled(&crate::tr!("ui-files-props-extended-attributes"));
    match page.loaded.and_then(|facts| facts.details.as_ref()) {
        Some(details) if !details.xattrs.is_empty() => {
            for attribute in &details.xattrs {
                group = group.row(row(attribute.name.clone()).mono_value(attribute.value.clone()));
            }
        }
        Some(_) => group = group.row(property_row(crate::tr!("ui-files-props-none"), NONE)),
        None => {
            let waiting = if page.loaded.is_some() || page.failed.is_some() {
                NONE
            } else {
                PENDING
            };
            group = group.row(property_row(
                crate::tr!("ui-files-props-attributes"),
                waiting,
            ));
        }
    }
    group
}
