//! The rows of the Info panel: pure, from what the viewer loaded.

use bingux_files::display;
use bingux_preview_core::{Loaded, PreviewSource};

use super::subject::Resolved;

/// One line of the Info panel: a name and its value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InfoRow {
    pub label: &'static str,
    pub value: String,
}

fn row(label: &'static str, value: impl Into<String>) -> InfoRow {
    InfoRow { label, value: value.into() }
}

/// The facts of every file, then those the viewer read. Rows that have no
/// value are left out.
pub fn rows(resolved: &Resolved) -> Vec<InfoRow> {
    let facts = &resolved.facts;
    let mut rows = vec![row("Name", facts.name.clone()), row("Type", facts.description)];
    if let Some(size) = facts.size {
        rows.push(row("Size", display::size(size)));
    }
    if let Some(modified) = facts.modified {
        rows.push(row("Modified", display::full(modified)));
    }
    if let Some(parent) = facts.path.as_deref().and_then(|path| path.parent()) {
        rows.push(row("Location", parent.to_string_lossy()));
    }
    match &resolved.loaded {
        Loaded::Image(Some(info)) => {
            rows.push(row("Dimensions", info.dimensions()));
            rows.extend(info.megapixels().map(|text| row("Resolution", text)));
            let exif = &info.exif;
            rows.extend(exif.camera.clone().map(|text| row("Camera", text)));
            rows.extend(exif.lens.clone().map(|text| row("Lens", text)));
            rows.extend(exif.taken.clone().map(|text| row("Taken", text)));
            rows.extend(exif.exposure.clone().map(|text| row("Exposure", text)));
        }
        Loaded::Media(Some(info)) => {
            rows.extend(info.seconds.map(|seconds| row("Length", bingux_preview_core::media::clock(seconds))));
            rows.extend(info.codec_line().map(|text| row("Codecs", text)));
        }
        Loaded::Text(window) => {
            if let Some(note) = window.encoding.note() {
                rows.push(row("Encoding", note));
            } else {
                rows.push(row("Encoding", "UTF-8"));
            }
        }
        Loaded::Archive(listing) => {
            let (files, bytes) = listing.totals();
            rows.push(row("Format", listing.format.name()));
            rows.push(row(
                "Contents",
                format!("{files} {}{}", if files == 1 { "file" } else { "files" }, if listing.truncated { " or more" } else { "" }),
            ));
            if bytes > 0 {
                rows.push(row("Unpacked", display::size(bytes)));
            }
        }
        Loaded::Font(font) => {
            rows.push(row("Family", font.family.clone()));
            rows.extend(font.style.clone().map(|text| row("Style", text)));
            rows.extend(font.version.clone().map(|text| row("Version", text)));
            rows.extend(font.designer.clone().map(|text| row("Designer", text)));
            rows.push(row("Glyphs", font.glyphs.to_string()));
        }
        _ => {}
    }
    if let PreviewSource::Bytes { .. } = resolved.source {
        rows.retain(|row| row.label != "Location");
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
        let picture = resolved("holiday.png", 2_500_000, Loaded::Image(Some(info)), ViewerId::Image);
        let rows = rows(&picture);
        let labels: Vec<_> = rows.iter().map(|row| row.label).collect();
        assert_eq!(labels, ["Name", "Type", "Size", "Dimensions", "Resolution", "Camera"]);
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
        assert!(values.iter().any(|value| value.contains("3:07")), "{values:?}");
        assert!(values.iter().any(|value| value.contains("FLAC")), "{values:?}");
    }

    #[test]
    fn an_attachment_has_no_location() {
        let card = resolved("blob.dat", 4, Loaded::Card, ViewerId::Card);
        assert!(rows(&card).iter().all(|row| row.label != "Location"));
    }
}
