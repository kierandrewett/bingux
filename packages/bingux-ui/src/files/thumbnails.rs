//! Pictures of files: in a grid tile at every grid level, and in the icon
//! slot of a list row at every size. A list row at the default 16 px shows a
//! picture only when the desktop's thumbnail cache already has one, so the
//! default list never decodes an original; from [`LIST_DECODE_PX`] up it
//! decodes images as the grid does.
//!
//! A picture is taken from the desktop's thumbnail cache when it has one
//! (`bingux_files::thumbs`), and for an image decoded from the file
//! otherwise. A video or a document has a picture only when the cache has
//! one; its original is never decoded. Every other file keeps its icon, and
//! so does a folder.
//!
//! A slot that scrolls into view does not decode at once. It waits a moment
//! ([`START_DELAY`]); one that scrolls away before then is unmounted and its
//! picture is never read. Flicking through twenty thousand files reads only
//! the pictures the eye comes to rest on. The slot is a fixed square, so a
//! picture arriving moves nothing.
//!
//! The decode size is a bucket (see `zoom::bucket_px`), not the exact slot
//! size, so a picture is decoded again only when the icon size crosses into
//! the next bucket.
//!
//! A decoded picture stays in the asset cache for [`KEEP`] after its slot is
//! gone, so scrolling back is instant, and no longer, so a long folder does
//! not hold every picture it ever showed.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, SystemTime};

use bingux_files::thumbs::{self, Bucket};
use bingux_files::{Category, Entry};
use freya::prelude::*;
use freya::torin::prelude::Size2D;

use super::view::entry_glyph;
use crate::icons::Icon;
use crate::style::*;

/// How long a slot waits in view before it loads its picture.
const START_DELAY: Duration = Duration::from_millis(90);
/// How long a decoded picture is kept once nothing shows it.
const KEEP: Duration = Duration::from_secs(20);
/// The smallest icon a list row decodes an original image at. Below it a row
/// shows a picture only from the cache: a 16 px smudge is not worth reading
/// the file for, and the icon stays as the placeholder until a cached picture
/// is there.
pub const LIST_DECODE_PX: u16 = 24;

/// Whether a slot shows a picture, and where it may come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preview {
    /// Always the icon.
    Off,
    /// A picture from the thumbnail cache; the icon when it has none.
    Cached,
    /// A picture from the cache, or decoded from an image.
    Full,
}

/// Where a file's picture can come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// The cache, or else the file itself.
    Decode,
    /// The cache only.
    Cache,
}

/// Where the picture of `entry` comes from, if it can have one.
pub fn source_of(entry: &Entry) -> Option<Source> {
    match entry.category() {
        Category::Image => Some(Source::Decode),
        Category::Video | Category::Document => Some(Source::Cache),
        _ => None,
    }
}

/// The icon of `entry` at `px` logical pixels square: its folder or file-type
/// icon, in the accent for a folder and muted otherwise.
pub fn glyph(tokens: &crate::theme::Tokens, entry: &Entry, px: u16) -> SvgViewer {
    glyph_of(entry_glyph(entry), entry.is_dir(), tokens, px)
}

fn glyph_of(icon: Icon, folder: bool, tokens: &crate::theme::Tokens, px: u16) -> SvgViewer {
    crate::icons::glyph(
        icon,
        f32::from(px),
        if folder {
            Paint::Accent.color(tokens)
        } else {
            Paint::Muted.color(tokens)
        },
    )
}

/// What stands for `entry` in a square slot `px` wide: its picture when
/// `preview` allows one and the file can have one (the icon until it loads,
/// and when it cannot be loaded), otherwise its icon. The slot is always `px`
/// square and clips the picture to the small corner radius of the nested
/// box, [`Radius::Mark`].
pub fn face(tokens: &crate::theme::Tokens, entry: &Entry, px: u16, bucket: u32, preview: Preview) -> Element {
    let edge = f32::from(px);
    let icon = glyph(tokens, entry, px);
    let slot = rect().width(Size::px(edge)).height(Size::px(edge));
    let source = match preview {
        Preview::Off => None,
        Preview::Cached => source_of(entry).map(|_| Source::Cache),
        Preview::Full => source_of(entry),
    };
    let Some(source) = source else {
        return slot.child(icon).into();
    };
    slot.tw(tokens, &[rounded(Radius::Mark), clip()])
        .child(Thumb {
            path: entry.path.clone(),
            modified: entry.modified,
            bucket,
            source,
            fallback: rect()
                .tw_layout(&[w_full(), h_full(), center()])
                .child(icon)
                .into(),
        })
        .into()
}

/// A picture that fills its parent, loaded lazily, with `fallback` shown
/// until it is ready and when it cannot be loaded.
#[derive(Clone, PartialEq)]
struct Thumb {
    /// The file to show.
    path: PathBuf,
    /// When the file was last changed, to tell a stale cached picture.
    modified: Option<SystemTime>,
    /// The longest edge to decode, in device pixels.
    bucket: u32,
    source: Source,
    fallback: Element,
}

impl Component for Thumb {
    fn render_key(&self) -> DiffKey {
        // A slot reused for another file starts again.
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(&self.path, &mut hasher);
        DiffKey::U64(std::hash::Hasher::finish(&hasher))
    }

    fn render(&self) -> impl IntoElement {
        let mut ready = use_state(|| false);
        use_hook(move || {
            // Tied to this component: unmounting cancels it.
            spawn(async move {
                timer(START_DELAY).await;
                ready.set(true);
            })
        });
        // The desktop's thumbnail cache first: a small PNG that somebody has
        // already made, read without opening the original. Looked up once,
        // when the slot has waited its turn.
        let lookup = use_hook(|| Rc::new(RefCell::new(None::<Result<Option<PathBuf>, ()>>)));
        if !ready() {
            return self.fallback.clone();
        }
        let cached = {
            let mut slot = lookup.borrow_mut();
            slot.get_or_insert_with(|| {
                if thumbs::is_failed(&self.path, self.modified) {
                    return Err(());
                }
                Ok(thumbs::lookup(
                    &self.path,
                    self.modified,
                    Bucket::for_size(self.bucket as f32),
                ))
            })
            .clone()
        };
        let Ok(cached) = cached else {
            return self.fallback.clone();
        };
        let fallback = self.fallback.clone();
        let viewer = match (cached, self.source) {
            (Some(png), _) => ImageViewer::new(png).decode_mode(DecodeMode::Source),
            (None, Source::Decode) => {
                let edge = self.bucket.max(1) as f32;
                ImageViewer::new(self.path.clone())
                    .decode_mode(DecodeMode::Custom(Size2D::new(edge, edge)))
            }
            (None, Source::Cache) => return self.fallback.clone(),
        };
        // The whole picture, in its own proportions, centred in the slot.
        viewer
            .asset_age(KEEP)
            .width(Size::fill())
            .height(Size::fill())
            .aspect_ratio(AspectRatio::Min)
            .image_cover(ImageCover::Center)
            .loading_placeholder(self.fallback.clone())
            .error_renderer(move |_: String| fallback.clone())
            .into()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use bingux_files::{Entry, Kind};

    use super::*;

    fn file(name: &str) -> Entry {
        Entry::named(PathBuf::from("/x").join(name), name.to_owned(), Kind::File, false, false)
    }

    #[test]
    fn images_decode_documents_and_videos_only_use_the_cache() {
        assert_eq!(source_of(&file("a.png")), Some(Source::Decode));
        assert_eq!(source_of(&file("a.jpg")), Some(Source::Decode));
        assert_eq!(source_of(&file("a.pdf")), Some(Source::Cache));
        assert_eq!(source_of(&file("a.mp4")), Some(Source::Cache));
        assert_eq!(source_of(&file("a.txt")), None);
        assert_eq!(source_of(&file("a.zip")), None);
    }

    #[test]
    fn a_folder_has_no_picture() {
        let folder = Entry::named(PathBuf::from("/x/d"), "d".to_owned(), Kind::Directory, false, false);
        assert_eq!(source_of(&folder), None);
    }
}
