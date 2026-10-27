//! What Quick Look shows for one file, and how each kind loads.
//!
//! Nothing here blocks the window. The stage is a fixed box, so the type's
//! icon is the placeholder at the size the content will have, and nothing
//! shifts when the content arrives:
//!
//! | Kind | Stage | Loaded by |
//! | ---- | ----- | --------- |
//! | picture | the picture, scaled to fit | the image viewer's decoder, off the UI thread |
//! | text, code, markup, JSON | the first 64 KiB, monospace | a pool worker |
//! | document, song, film | the largest thumbnail the desktop has made, else the icon | a pool worker |
//! | folder | its item count and total size, growing as it is counted | a counting thread |
//! | unknown type | the head as text when it is text, else the icon | a pool worker |
//! | archive, device | the icon | |
//!
//! Every load belongs to the component that shows it. Stepping to another
//! file unmounts the component: a load that has not started is dropped, and
//! the folder counter stops after its current entry.

use std::rc::Rc;
use std::time::Duration;

use bingux_files::media::{self, AvInfo, ImageInfo};
use bingux_files::ops::{Counting, Totals};
use bingux_files::preview::{PreviewKind, TEXT_LIMIT, TextError, TextHead, kind_of, read_text};
use bingux_files::thumbs::{self, Bucket};
use bingux_files::{Category, Entry, display};
use freya::prelude::*;
use freya::torin::prelude::Size2D;
use std::path::PathBuf;

use super::view::entry_glyph;
use crate::icons::{self, Icon};
use crate::style::*;
use crate::surface::use_tokens;
use crate::task::use_resource;
use crate::typography::{BODY, CAPTION, CODE, Ink, TITLE};

/// The longest edge a picture is decoded to. The decoder never enlarges, and
/// scales larger pictures down while reading them.
const DECODE_EDGE: f32 = 2048.0;
/// How long a decoded picture stays in the cache once nothing shows it.
const KEEP: Duration = Duration::from_secs(5);
/// How often a folder's running total is read.
const TICK: Duration = Duration::from_millis(150);
/// The most lines of text the stage lays out. A 64 KiB file of short lines
/// has up to 30 000; a screen shows fifty.
const TEXT_LINES: usize = 1500;
/// The icon on the stage, as a multiple of the 48 px hero size.
const ICON_SCALE: f32 = 3.0;

// ---------------------------------------------------------------------------
// Facts
// ---------------------------------------------------------------------------

/// What the footer says, read off the UI thread.
#[derive(Clone, Debug, PartialEq)]
struct Facts {
    /// With its size and date read, when the listing had not yet.
    entry: Entry,
    image: Option<ImageInfo>,
    av: Option<AvInfo>,
}

fn load_facts(entry: Entry) -> Result<Facts, String> {
    let entry = if entry.sized {
        entry
    } else {
        Entry::from_path(&entry.path, false).map_err(|error| error.to_string())?
    };
    let image = (entry.category() == Category::Image)
        .then(|| media::image_info(&entry.path))
        .flatten();
    let av = matches!(entry.category(), Category::Audio | Category::Video)
        .then(|| media::av_info(&entry.path))
        .flatten();
    Ok(Facts { entry, image, av })
}

/// The footer line: type, size, picture size or length, date. `facts` is
/// absent until it is read, and the line shows what the listing knew.
fn summary(entry: &Entry, facts: Option<&Facts>) -> String {
    let entry = facts.map_or(entry, |facts| &facts.entry);
    let mut parts = vec![entry.mime.description.to_owned()];
    if !entry.is_dir() && entry.sized {
        parts.push(display::size(entry.size));
    }
    if let Some(facts) = facts {
        if let Some(image) = &facts.image {
            parts.push(image.dimensions());
        }
        if let Some(av) = &facts.av {
            parts.extend(av.seconds.map(media::clock));
            parts.extend(av.codec_line());
        }
    }
    if let Some(modified) = entry.modified {
        parts.push(format!("Modified {}", display::modified(modified)));
    }
    parts.join(" · ")
}

fn plural(count: u64, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// The two lines of a folder's stage: how many items, and how big. While
/// the count runs they grow and say "so far…"; a folder with an unreadable
/// part says "at least". Before anything is counted, the first line says so
/// and the second is a space, so both lines have their height from the start.
fn folder_lines(totals: Totals) -> (String, String) {
    if totals.items() == 0 && !totals.done {
        return ("Counting…".to_owned(), " ".to_owned());
    }
    let mut size = display::size(totals.bytes);
    if !totals.done {
        size.push_str(" so far…");
    } else if totals.unreadable > 0 {
        size.insert_str(0, "at least ");
    }
    (plural(totals.items(), "item", "items"), size)
}

// ---------------------------------------------------------------------------
// The subject
// ---------------------------------------------------------------------------

/// One file on the stage, with its footer. Keyed by path, so stepping to
/// another file starts every load afresh and drops the last file's.
#[derive(PartialEq)]
pub(super) struct Subject {
    pub entry: Entry,
}

impl Component for Subject {
    fn render_key(&self) -> DiffKey {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(&self.entry.path, &mut hasher);
        DiffKey::U64(std::hash::Hasher::finish(&hasher))
    }

    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let kind = kind_of(self.entry.kind, &self.entry.mime);
        let facts = use_resource(self.entry.clone(), load_facts);

        let stage = match kind {
            PreviewKind::Image => Element::from(PictureStage {
                entry: self.entry.clone(),
            }),
            PreviewKind::Text | PreviewKind::Sniff => Element::from(TextStage {
                entry: self.entry.clone(),
                sniff: kind == PreviewKind::Sniff,
            }),
            PreviewKind::Poster => Element::from(PosterStage {
                entry: self.entry.clone(),
            }),
            PreviewKind::Folder => Element::from(FolderStage {
                entry: self.entry.clone(),
            }),
            PreviewKind::Icon => icon_stage(&self.entry, "There is no preview for this type."),
        };
        let line = facts.with(|facts| summary(&self.entry, facts.value()));

        rect()
            .tw(&tokens, &[flex_col(), grow_children(), w_full(), h_flex()])
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .min_height(Size::px(0.0))
                    .tw(&tokens, &[bg(Paint::Card), clip()])
                    .child(stage),
            )
            .child(rect().tw(&tokens, &[w_full(), h(Dim::Hairline), bg(Paint::Separator)]))
            .child(
                rect()
                    .tw(&tokens, &[w_full(), h(Dim::Header), px(S4), flex_row(), items(Align::Center)])
                    .child(
                        CAPTION
                            .in_ink(Ink::Muted)
                            .label(&tokens, line)
                            .max_lines(1)
                            .text_overflow(TextOverflow::Ellipsis),
                    ),
            )
    }
}

/// Count a folder in the background and return its running total. The count
/// stops when the component goes.
fn use_folder(entry: &Entry) -> State<Totals> {
    let mut totals = use_state(Totals::default);
    let path = entry.path.clone();
    use_hook(move || {
        let counting = Rc::new(Counting::start(vec![path]));
        let reader = counting.clone();
        spawn(async move {
            loop {
                let now = reader.totals();
                totals.set(now);
                if now.done {
                    break;
                }
                timer(TICK).await;
            }
        });
        counting
    });
    totals
}

// ---------------------------------------------------------------------------
// Stages
// ---------------------------------------------------------------------------

/// The type's icon, big and centred, with an optional line under it. It is
/// the placeholder of every stage, at the size the content will have.
fn icon_stage(entry: &Entry, note: &str) -> Element {
    IconStage {
        icon: entry_glyph(entry),
        accent: entry.is_dir(),
        note: (!note.is_empty()).then(|| note.to_owned()),
    }
    .into()
}

#[derive(PartialEq)]
struct IconStage {
    icon: Icon,
    accent: bool,
    note: Option<String>,
}

impl Component for IconStage {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let color = if self.accent {
            Paint::Accent.color(&tokens)
        } else {
            Paint::Muted.color(&tokens)
        };
        rect()
            .tw(&tokens, &[w_full(), h_full(), flex_col(), center(), gap(S4)])
            .child(icons::icon_scaled(self.icon, IconSize::Hero, ICON_SCALE, color))
            .maybe_child(self.note.clone().map(|note| {
                CAPTION
                    .in_ink(Ink::Muted)
                    .paragraph(&tokens, note)
                    .width(Size::fill())
                    .text_align(TextAlign::Center)
            }))
    }
}

/// A picture, scaled to fit with its proportions kept.
#[derive(PartialEq)]
struct PictureStage {
    entry: Entry,
}

impl Component for PictureStage {
    fn render(&self) -> impl IntoElement {
        let entry = self.entry.clone();
        let broken = icon_stage(&entry, "This picture could not be shown.");
        ImageViewer::new(entry.path.clone())
            .decode_mode(DecodeMode::Custom(Size2D::new(DECODE_EDGE, DECODE_EDGE)))
            .asset_age(KEEP)
            .width(Size::fill())
            .height(Size::fill())
            .aspect_ratio(AspectRatio::Min)
            .image_cover(ImageCover::Center)
            .loading_placeholder(icon_stage(&entry, ""))
            .error_renderer(move |_: String| broken.clone())
    }
}

/// The largest thumbnail the desktop has made for a file, if any.
fn find_poster(entry: Entry) -> Result<Option<PathBuf>, String> {
    Ok([Bucket::XXLarge, Bucket::XLarge, Bucket::Large, Bucket::Normal]
        .into_iter()
        .find_map(|bucket| thumbs::lookup(&entry.path, entry.modified, bucket)))
}

/// A document, a song or a film: its thumbnail, else its icon.
#[derive(PartialEq)]
struct PosterStage {
    entry: Entry,
}

impl Component for PosterStage {
    fn render(&self) -> impl IntoElement {
        let poster = use_resource(self.entry.clone(), find_poster);
        let found = poster.with(|poster| poster.value().cloned());
        match found {
            None => icon_stage(&self.entry, ""),
            Some(None) => icon_stage(&self.entry, "No thumbnail has been made for this file."),
            Some(Some(png)) => {
                let fallback = icon_stage(&self.entry, "");
                ImageViewer::new(png)
                    .decode_mode(DecodeMode::Source)
                    .asset_age(KEEP)
                    .width(Size::fill())
                    .height(Size::fill())
                    .aspect_ratio(AspectRatio::Min)
                    .image_cover(ImageCover::Center)
                    .loading_placeholder(fallback.clone())
                    .error_renderer(move |_: String| fallback.clone())
                    .into()
            }
        }
    }
}

/// The head of a file as text, or a note that it is not text.
#[derive(Clone, Debug, PartialEq)]
enum Loaded {
    Text(TextHead, bool),
    Binary,
}

fn load_text(path: PathBuf) -> Result<Loaded, String> {
    match read_text(&path, &bingux_files::Cancel::new()) {
        Ok(head) => {
            let text = head.text.replace('\r', "").replace('\t', "    ");
            // Lines past the cap are cut: a screen shows far fewer.
            let mut cut = false;
            let text = match text.match_indices('\n').nth(TEXT_LINES) {
                Some((at, _)) => {
                    cut = true;
                    text[..at].to_owned()
                }
                None => text,
            };
            Ok(Loaded::Text(
                TextHead {
                    text,
                    truncated: head.truncated,
                },
                cut,
            ))
        }
        Err(TextError::Binary) => Ok(Loaded::Binary),
        Err(TextError::Cancelled) => Err("Cancelled.".to_owned()),
        Err(TextError::Unreadable(message)) => Err(message),
    }
}

/// What the notice under a cut-off text says, or `None` when all is shown.
fn truncation_notice(entry: &Entry, truncated: bool, lines_cut: bool) -> Option<String> {
    if !truncated && !lines_cut {
        return None;
    }
    let shown = if lines_cut {
        format!("the first {TEXT_LINES} lines")
    } else {
        format!("the first {} KiB", TEXT_LIMIT / 1024)
    };
    let of = if entry.sized && entry.size > 0 {
        format!(" of {}", display::size(entry.size))
    } else {
        String::new()
    };
    Some(format!("Showing {shown}{of}. Press Enter to open the whole file."))
}

#[derive(PartialEq)]
struct TextStage {
    entry: Entry,
    /// The type is not known, so a file that is not text shows its icon.
    sniff: bool,
}

impl Component for TextStage {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let text = use_resource(self.entry.path.clone(), load_text);
        let loaded = text.with(|loaded| loaded.value().cloned());
        let failed = text.with(|loaded| loaded.error().map(str::to_owned));
        if let Some(message) = failed {
            return icon_stage(&self.entry, &message);
        }
        let Some(loaded) = loaded else {
            return icon_stage(&self.entry, "");
        };
        match loaded {
            Loaded::Binary => icon_stage(
                &self.entry,
                if self.sniff {
                    "There is no preview for this type."
                } else {
                    "This file does not look like text."
                },
            ),
            Loaded::Text(head, lines_cut) => {
                let notice = truncation_notice(&self.entry, head.truncated, lines_cut);
                rect()
                    .tw(&tokens, &[w_full(), h_full(), flex_col(), grow_children()])
                    .child(
                        rect()
                            .width(Size::fill())
                            .height(Size::flex(1.0))
                            .min_height(Size::px(0.0))
                            .child(
                                ScrollView::new().child(
                                    rect().tw(&tokens, &[w_full(), p(S4)]).child(
                                        CODE.paragraph(&tokens, head.text).width(Size::fill()),
                                    ),
                                ),
                            ),
                    )
                    .maybe_child(notice.map(|notice| {
                        rect()
                            .tw(
                                &tokens,
                                &[w_full(), px(S4), py(S2), bg(Paint::Control)],
                            )
                            .child(CAPTION.in_ink(Ink::Muted).label(&tokens, notice).max_lines(1))
                    }))
                    .into_element()
            }
        }
    }
}

/// A folder: how much is in it, counted while the stage is open.
#[derive(PartialEq)]
struct FolderStage {
    entry: Entry,
}

impl Component for FolderStage {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let totals = use_folder(&self.entry);
        let (items, size) = folder_lines(totals());
        rect()
            .tw(&tokens, &[w_full(), h_full(), flex_col(), center(), gap(S2)])
            .child(icons::icon_scaled(
                entry_glyph(&self.entry),
                IconSize::Hero,
                ICON_SCALE,
                Paint::Accent.color(&tokens),
            ))
            .child(TITLE.label(&tokens, items))
            .child(BODY.in_ink(Ink::Muted).label(&tokens, size))
    }
}

#[cfg(test)]
mod tests {
    use bingux_files::Kind;

    use super::*;

    fn entry(name: &str, size: u64) -> Entry {
        let mut entry = Entry::named(
            PathBuf::from("/tmp").join(name),
            name.to_owned(),
            Kind::File,
            false,
            false,
        );
        entry.size = size;
        entry.sized = true;
        entry
    }

    #[test]
    fn the_footer_names_type_size_and_what_was_read() {
        let picture = entry("holiday.png", 2_500_000);
        assert_eq!(summary(&picture, None), "PNG image · 2.5 MB");
        let facts = Facts {
            entry: picture.clone(),
            image: Some(ImageInfo {
                width: 4032,
                height: 3024,
                exif: Default::default(),
            }),
            av: None,
        };
        assert_eq!(
            summary(&picture, Some(&facts)),
            "PNG image · 2.5 MB · 4032 × 3024"
        );
        let song = entry("song.flac", 30_000_000);
        let facts = Facts {
            entry: song.clone(),
            image: None,
            av: Some(AvInfo {
                seconds: Some(187.0),
                codecs: vec!["FLAC".into()],
            }),
        };
        assert_eq!(
            summary(&song, Some(&facts)),
            "FLAC audio · 30 MB · 3:07 · FLAC"
        );
    }

    #[test]
    fn a_folder_counts_up_then_settles() {
        assert_eq!(
            folder_lines(Totals::default()),
            ("Counting…".to_owned(), " ".to_owned())
        );
        let counting = Totals {
            bytes: 1_500_000,
            files: 3,
            folders: 1,
            ..Totals::default()
        };
        assert_eq!(folder_lines(counting), ("4 items".into(), "1.5 MB so far…".into()));
        let done = Totals {
            done: true,
            files: 1,
            folders: 0,
            ..counting
        };
        assert_eq!(folder_lines(done), ("1 item".into(), "1.5 MB".into()));
        let partial = Totals {
            unreadable: 2,
            ..done
        };
        assert_eq!(folder_lines(partial).1, "at least 1.5 MB");
        let empty = Totals {
            done: true,
            ..Totals::default()
        };
        assert_eq!(folder_lines(empty), ("0 items".into(), "0 bytes".into()));
    }

    #[test]
    fn a_cut_off_text_says_how_much_is_shown() {
        let log = entry("big.log", 3_200_000);
        assert_eq!(truncation_notice(&log, false, false), None);
        assert_eq!(
            truncation_notice(&log, true, false).as_deref(),
            Some("Showing the first 64 KiB of 3.2 MB. Press Enter to open the whole file.")
        );
        assert!(
            truncation_notice(&log, true, true)
                .unwrap()
                .starts_with("Showing the first 1500 lines of 3.2 MB.")
        );
    }

    #[test]
    fn text_is_cut_to_the_line_cap_and_tidied() {
        let dir = std::env::temp_dir().join(format!("bingux-stage-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("lines.txt");
        let body: String = (0..TEXT_LINES + 50).map(|n| format!("line\t{n}\r\n")).collect();
        std::fs::write(&path, body).unwrap();
        let Ok(Loaded::Text(head, cut)) = load_text(path) else {
            panic!("a text file loads as text");
        };
        assert!(cut);
        assert_eq!(head.text.lines().count(), TEXT_LINES + 1);
        assert!(!head.text.contains('\t') && !head.text.contains('\r'));
    }

    #[test]
    fn binary_data_is_reported_not_shown() {
        let dir = std::env::temp_dir().join(format!("bingux-stage-bin-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("data.bin");
        std::fs::write(&path, [0u8, 1, 2, 3]).unwrap();
        assert_eq!(load_text(path), Ok(Loaded::Binary));
    }
}
