//! The Compress dialog in a headless window: its states at desktop and phone
//! size, a card that keeps one size whatever the options, and the keys.
//!
//! Set `COMPRESS_PNG_DIR` to also write each state as a PNG, for looking at.

use std::path::{Path, PathBuf};
use std::time::Duration;

use bingux_files::ops::{Estimate, Format, Tools};
use bingux_files::{Entry, Kind};
use freya::prelude::*;
use torin::prelude::Area;

use super::compress::{CompressPreview, compress_preview};
use crate::shell::overlays::{OverlayHost, use_overlays};
use crate::shell::{FORM_HEIGHT, FORM_WIDTH, open_form};
use crate::snapshot::Snapshot;
use crate::theme::provide_tokens;
use crate::window::WindowHost;

const DESKTOP: (f32, f32) = (1100.0, 820.0);
const PHONE: (f32, f32) = (390.0, 800.0);

fn file(name: &str, size: u64) -> Entry {
    let mut entry = Entry::named(
        PathBuf::from("/home/kieran/Pictures").join(name),
        name.to_string(),
        Kind::File,
        false,
        false,
    );
    entry.size = size;
    entry
}

fn folder(name: &str) -> Entry {
    Entry::named(
        PathBuf::from("/home/kieran/Pictures").join(name),
        name.to_string(),
        Kind::Directory,
        false,
        false,
    )
}

/// Every tool and encoder present: what the dialog shows on a full machine.
fn full() -> Tools {
    Tools {
        ffmpeg: Some("/usr/bin/ffmpeg".into()),
        ffprobe: Some("/usr/bin/ffprobe".into()),
        magick: Some("/usr/bin/magick".into()),
        encoders: [
            "libx264", "libx265", "libsvtav1", "libvpx-vp9", "aac", "libopus", "libmp3lame", "flac",
            "libwebp", "mjpeg", "png",
        ]
        .map(String::from)
        .to_vec(),
        magick_formats: ["JPEG", "PNG", "WEBP"].map(String::from).to_vec(),
    }
}

/// Opens the dialog for `preview` as soon as it is mounted.
#[derive(PartialEq)]
struct Open {
    preview: PreviewKey,
}

/// A preview that compares by identity of what built it (its label), since
/// the preview holds entries that are not comparable.
#[derive(Clone)]
struct PreviewKey(std::rc::Rc<CompressPreview>, &'static str);

impl PartialEq for PreviewKey {
    fn eq(&self, other: &Self) -> bool {
        self.1 == other.1
    }
}

impl Component for Open {
    fn render(&self) -> impl IntoElement {
        let overlays = use_overlays();
        let preview = self.preview.0.clone();
        use_hook(move || {
            open_form(overlays, "Compress", compress_preview((*preview).clone()), None);
        });
        rect()
    }
}

#[derive(PartialEq)]
struct Root {
    child: Element,
}

impl Component for Root {
    fn render(&self) -> impl IntoElement {
        let _tokens = provide_tokens();
        WindowHost::new(OverlayHost::new(self.child.clone()))
    }
}

fn open(preview: CompressPreview, label: &'static str, size: (f32, f32)) -> Snapshot {
    let key = PreviewKey(std::rc::Rc::new(preview), label);
    let mut shot = Snapshot::new(
        move || Root { child: Open { preview: key.clone() }.into() }.into_element(),
        size,
    );
    shot.settle(Duration::from_millis(900));
    shot
}

/// The card of the open dialog.
fn card(shot: &mut Snapshot) -> Option<Area> {
    shot.runner()
        .find_many(|node, element| {
            (element.accessibility().builder.role() == AccessibilityRole::Dialog).then(|| node.layout().area)
        })
        .into_iter()
        .next()
}

fn png(shot: &mut Snapshot, name: &str, size: (f32, f32)) {
    let Some(dir) = std::env::var_os("COMPRESS_PNG_DIR").map(PathBuf::from) else {
        return;
    };
    let tag = if size == PHONE { "phone" } else { "desktop" };
    shot.save(Path::new(&dir).join(format!("{name}-{tag}.png")));
}

fn archive(entries: Vec<Entry>) -> CompressPreview {
    let mut preview = CompressPreview::new(entries, full());
    preview.format = Some(Format::Zip);
    preview
}

fn photos() -> Vec<Entry> {
    vec![file("holiday-1.jpg", 4_200_000), file("holiday-2.jpg", 3_900_000), file("holiday-3.png", 7_500_000)]
}

fn estimate_for(entries: &[Entry], ratio: f64) -> Estimate {
    let per_file: Vec<Option<u64>> = entries.iter().map(|entry| Some((entry.size as f64 * ratio) as u64)).collect();
    Estimate {
        original: entries.iter().map(|entry| entry.size).sum(),
        estimated: per_file.iter().flatten().sum(),
        per_file,
    }
}

/// Every state, desktop and phone, as PNGs when asked for, and a card that is
/// there and as big as the form dialog says.
#[test]
fn every_state_renders_in_a_card_of_one_size() {
    for size in [DESKTOP, PHONE] {
        let mut scenes: Vec<(&'static str, CompressPreview)> = Vec::new();
        scenes.push(("archive-valid", archive(vec![folder("Holiday"), file("notes.txt", 1200)])));
        let mut single = archive(vec![folder("Holiday")]);
        single.name = Some("Holiday".into());
        single.taken = vec!["Holiday.zip".into()];
        scenes.push(("archive-conflict", single));
        let mut replace = archive(vec![folder("Holiday")]);
        replace.taken = vec!["Holiday.zip".into()];
        replace.replace = true;
        scenes.push(("archive-replace", replace));
        let mut invalid = archive(vec![folder("Holiday"), file("notes.txt", 10)]);
        invalid.name = Some("a/b".into());
        invalid.excludes = "a[bc".into();
        scenes.push(("archive-invalid", invalid));
        let mut locked = archive(vec![folder("Holiday"), file("notes.txt", 10)]);
        locked.locked = true;
        locked.password = "secret".into();
        locked.confirm = "secre".into();
        scenes.push(("archive-password", locked));
        let mut tar = archive(vec![folder("Holiday"), file("notes.txt", 10)]);
        tar.format = Some(Format::TarXz);
        scenes.push(("archive-tar", tar));
        let images = photos();
        let mut image = CompressPreview::new(images.clone(), full());
        image.estimate = Some(estimate_for(&images, 0.28));
        scenes.push(("media-images", image));
        let films = vec![file("clip-1.mp4", 612_000_000), file("clip-2.mkv", 230_000_000)];
        let mut film = CompressPreview::new(films.clone(), full());
        film.estimate = Some(estimate_for(&films, 0.14));
        scenes.push(("media-videos", film));
        let songs = vec![file("track-1.flac", 31_000_000), file("track-2.wav", 52_000_000)];
        let mut song = CompressPreview::new(songs.clone(), full());
        song.estimate = Some(estimate_for(&songs, 0.1));
        scenes.push(("media-audio", song));
        let mixed = vec![file("a.jpg", 1000), file("b.mp4", 2000), file("c.mp3", 3000)];
        let mut all = CompressPreview::new(mixed.clone(), full());
        all.estimate = Some(estimate_for(&mixed, 0.3));
        scenes.push(("media-mixed", all));
        scenes.push(("media-missing", CompressPreview::new(vec![file("clip.mp4", 1_000_000), file("a.jpg", 2000)], Tools::default())));
        let mut some = full();
        some.encoders.retain(|encoder| encoder != "libx265" && encoder != "libopus");
        some.magick = None;
        some.magick_formats.clear();
        scenes.push(("media-partial", CompressPreview::new(vec![file("clip.mp4", 1_000_000)], some)));
        let mut not_media = CompressPreview::new(vec![file("a.jpg", 1000), file("notes.txt", 10)], full());
        not_media.reduce = None;
        scenes.push(("mixed-selection", not_media));

        for (index, (label, preview)) in scenes.into_iter().enumerate() {
            let label: &'static str = label;
            let mut shot = open(preview, label, size);
            let found = card(&mut shot).unwrap_or_else(|| panic!("{label}: no dialog card"));
            let (want_w, want_h) = if size == PHONE {
                (size.0, size.1)
            } else {
                (FORM_WIDTH, FORM_HEIGHT)
            };
            assert!((found.width() - want_w).abs() < 1.5, "{label} {size:?}: width {}", found.width());
            assert!((found.height() - want_h).abs() < 1.5, "{label} {size:?}: height {}", found.height());
            png(&mut shot, &format!("{index:02}-{label}"), size);
        }
    }
}
