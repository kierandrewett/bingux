//! The rows of the Info panel: pure, from what the viewer loaded.

use bingux_files::display;
use bingux_preview_core::{Loaded, PreviewSource};

use super::subject::Resolved;

/// One line of the Info panel: a name and its value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InfoRow {
    pub label: String,
    pub value: String,
}

fn row(label: String, value: impl Into<String>) -> InfoRow {
    InfoRow {
        label,
        value: value.into(),
    }
}

/// The facts of every file, then those the viewer read. Rows that have no
/// value are left out.
pub fn rows(resolved: &Resolved) -> Vec<InfoRow> {
    let facts = &resolved.facts;
    let mut rows = vec![
        row(crate::tr!("ui-info-name"), facts.name.clone()),
        row(crate::tr!("ui-info-type"), facts.description),
    ];
    if let Some(size) = facts.size {
        rows.push(row(crate::tr!("ui-info-size"), display::size(size)));
    }
    if let Some(modified) = facts.modified {
        rows.push(row(crate::tr!("ui-info-modified"), display::full(modified)));
    }
    // Bytes with no file have no folder to name.
    let has_location = !matches!(resolved.source, PreviewSource::Bytes { .. });
    if let Some(parent) = facts
        .path
        .as_deref()
        .and_then(|path| path.parent())
        .filter(|_| has_location)
    {
        rows.push(row(
            crate::tr!("ui-info-location"),
            parent.to_string_lossy(),
        ));
    }
    match &resolved.loaded {
        Loaded::Image(Some(info)) => {
            rows.push(row(crate::tr!("ui-info-dimensions"), info.dimensions()));
            rows.extend(
                info.megapixels()
                    .map(|text| row(crate::tr!("ui-info-resolution"), text)),
            );
            let exif = &info.exif;
            rows.extend(
                exif.camera
                    .clone()
                    .map(|text| row(crate::tr!("ui-info-camera"), text)),
            );
            rows.extend(
                exif.lens
                    .clone()
                    .map(|text| row(crate::tr!("ui-info-lens"), text)),
            );
            rows.extend(
                exif.taken
                    .clone()
                    .map(|text| row(crate::tr!("ui-info-taken"), text)),
            );
            rows.extend(
                exif.exposure
                    .clone()
                    .map(|text| row(crate::tr!("ui-info-exposure"), text)),
            );
            rows.extend(
                exif.software
                    .clone()
                    .map(|text| row(crate::tr!("ui-info-software"), text)),
            );
            rows.extend(
                exif.location
                    .clone()
                    .map(|text| row(crate::tr!("ui-info-place"), text)),
            );
        }
        Loaded::Media(Some(info)) => {
            rows.extend(info.seconds.map(|seconds| {
                row(
                    crate::tr!("ui-info-length"),
                    bingux_preview_core::media::clock(seconds),
                )
            }));
            rows.extend(
                info.codec_line()
                    .map(|text| row(crate::tr!("ui-info-codecs"), text)),
            );
        }
        Loaded::Text(window) => {
            if let Some(note) = window.encoding.note() {
                rows.push(row(crate::tr!("ui-info-encoding"), note));
            } else {
                rows.push(row(crate::tr!("ui-info-encoding"), "UTF-8"));
            }
        }
        Loaded::Archive(listing) => {
            let (files, bytes) = listing.totals();
            rows.push(row(crate::tr!("ui-info-format"), listing.format.name()));
            let contents = if listing.truncated {
                crate::tr!("ui-info-files-at-least", count = files)
            } else {
                crate::tr!("ui-info-files", count = files)
            };
            rows.push(row(crate::tr!("ui-info-contents"), contents));
            if bytes > 0 {
                rows.push(row(crate::tr!("ui-info-unpacked"), display::size(bytes)));
            }
        }
        Loaded::Font(font) => {
            rows.push(row(crate::tr!("ui-info-family"), font.family.clone()));
            rows.extend(
                font.style
                    .clone()
                    .map(|text| row(crate::tr!("ui-info-style"), text)),
            );
            rows.extend(
                font.version
                    .clone()
                    .map(|text| row(crate::tr!("ui-info-version"), text)),
            );
            rows.extend(
                font.designer
                    .clone()
                    .map(|text| row(crate::tr!("ui-info-designer"), text)),
            );
            rows.push(row(crate::tr!("ui-info-glyphs"), font.glyphs.to_string()));
        }
        _ => {}
    }
    rows
}

#[cfg(test)]
mod tests {
    use bingux_preview_core::media::{AvInfo, ImageInfo};
    use bingux_preview_core::{Content, FileFacts, Sniffed, ViewerId};

    use super::*;

    fn resolved(name: &str, size: u64, loaded: Loaded, viewer: ViewerId) -> Resolved {
        let source = PreviewSource::bytes(name, vec![0u8; size as usize]);
        let sniffed = Sniffed {
            mime: bingux_files::mime::for_name(name),
            content: Content::Image,
        };
        Resolved {
            facts: FileFacts::of(&source, &sniffed),
            source,
            sniffed,
            viewer,
            loaded,
            notes: Vec::new(),
        }
    }

    #[test]
    fn a_picture_says_its_size_and_camera() {
        let mut info = ImageInfo {
            width: 4032,
            height: 3024,
            exif: Default::default(),
        };
        info.exif.camera = Some("Canon EOS R5".into());
        let picture = resolved(
            "holiday.png",
            2_500_000,
            Loaded::Image(Some(info)),
            ViewerId::Image,
        );
        let rows = rows(&picture);
        let labels: Vec<_> = rows.iter().map(|row| row.label.clone()).collect();
        assert_eq!(
            labels,
            ["Name", "Type", "Size", "Dimensions", "Resolution", "Camera"]
        );
    }

    #[test]
    fn a_song_says_its_length_and_codec() {
        let song = resolved(
            "song.flac",
            30_000_000,
            Loaded::Media(Some(AvInfo {
                seconds: Some(187.0),
                codecs: vec!["FLAC".into()],
            })),
            ViewerId::Media,
        );
        let values: Vec<String> = rows(&song).into_iter().map(|row| row.value).collect();
        assert!(
            values.iter().any(|value| value.contains("3:07")),
            "{values:?}"
        );
        assert!(
            values.iter().any(|value| value.contains("FLAC")),
            "{values:?}"
        );
    }

    #[test]
    fn an_attachment_has_no_location() {
        let card = resolved("blob.dat", 4, Loaded::Card, ViewerId::Card);
        assert!(rows(&card).iter().all(|row| row.label != "Location"));
    }
}
