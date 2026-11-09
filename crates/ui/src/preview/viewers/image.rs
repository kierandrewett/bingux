//! Pictures: [`ZoomView`] around the picture, drawn by Freya's codecs where
//! they have one and by the preview helper where they do not.
//!
//! Freya's `ImageViewer` (or `GifViewer`, or `SvgViewer`) decodes and caches off
//! the UI thread and draws PNG, JPEG, WebP, GIF, BMP, ICO and SVG. Skia turns a
//! photograph stored on its side upright by its EXIF orientation as it decodes,
//! so the size the view fits is the size as it is shown. HEIF, AVIF, TIFF, Radiance HDR,
//! OpenEXR and camera RAW files are decoded in the helper process
//! ([`super::image_helper`]), so a codec bug in any of them cannot take the
//! window. How the view moves, zooms and turns is [`crate::zoom_view`].
//!
//! The picture opens at once: a small decode (a JPEG is decoded at an eighth of
//! its size in a few milliseconds) is on screen while the sharp one is made and
//! fades in over it. A picture that is zoomed in is decoded at the size the
//! screen needs, up to 8192 pixels on a side, and never smaller than before.
//!
//! | Key | Does |
//! | --- | ---- |
//! | `+`, `=`, `-` | zoom in and out a quarter of a step each |
//! | `0`, `1` | fit the window, actual size |
//! | `R`, `Shift+R` | turn a quarter right and left |
//! | `H`, `V` | flip left to right, top to bottom |
//! | Ctrl and wheel | zoom at the pointer; the wheel pans a zoomed picture |
//! | drag, Shift and an arrow | pan a zoomed picture |

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use bingux_preview_core::{Loaded, PreviewSource};
use freya::prelude::*;
use freya::torin::prelude::Size2D;

use super::super::chrome::PreviewNav;
use super::super::subject::Resolved;
use super::Stage;
use crate::zoom_view::{FitMode, ZoomView};

/// How long a decoded picture stays in the cache once nothing shows it.
const KEEP: Duration = Duration::from_secs(5);
/// The longest edge of the quick first decode.
const FIRST_PASS: f32 = 512.0;
/// The sizes the sharp decode steps through: the longest edge in device pixels.
const BUCKETS: [f32; 4] = [1024.0, 2048.0, 4096.0, 8192.0];
/// A GIF larger than this shows its first frame only: every frame of an
/// animation is decoded into memory.
const GIF_LIMIT: u64 = 24 * 1024 * 1024;

/// How a picture is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Route {
    /// Freya's own codecs.
    Own,
    /// The preview helper; `raw` for a camera file.
    Helper { raw: bool },
}

const RAW_EXTENSIONS: [&str; 22] = [
    "cr2", "cr3", "crw", "nef", "nrw", "arw", "srf", "sr2", "dng", "orf", "rw2", "raf", "pef", "srw", "x3f", "3fr", "erf", "kdc", "mrw", "mef", "mos", "iiq",
];
const HELPER_EXTENSIONS: [&str; 11] = ["tif", "tiff", "heic", "heif", "hif", "avif", "avifs", "hdr", "exr", "pic", "rgbe"];

/// Which decoder shows `resolved`, by its type and its name.
pub(super) fn route_of(resolved: &Resolved) -> Route {
    let name = resolved.source.name().to_ascii_lowercase();
    let extension = name.rsplit_once('.').map_or("", |(_, extension)| extension);
    if RAW_EXTENSIONS.contains(&extension) {
        return Route::Helper { raw: true };
    }
    let mime = resolved.sniffed.mime.mime;
    let heif = mime.starts_with("image/heif") || mime.starts_with("image/heic") || mime == "image/avif";
    let other = matches!(mime, "image/tiff" | "image/x-exr" | "image/vnd.radiance" | "image/x-hdr");
    if heif || other || HELPER_EXTENSIONS.contains(&extension) {
        return Route::Helper { raw: false };
    }
    // A camera file the extension list does not know, named by the MIME database.
    if mime.starts_with("image/x-") && ["canon", "nikon", "sony", "olympus", "panasonic", "fuji", "pentax", "samsung", "adobe-dng", "leica", "hasselblad", "kodak", "minolta", "sigma", "phaseone"].iter().any(|maker| mime.contains(maker)) {
        return Route::Helper { raw: true };
    }
    Route::Own
}

/// The size of the picture as it is shown. The header already turns the size of
/// a photograph stored on its side, and Skia turns the pixels the same way as it
/// decodes, so what the view fits and what it draws are the same shape. Fitting
/// the stored size instead (and turning the view to match) fits a box of the
/// wrong shape and then turns the upright picture a second time.
fn shown_size(resolved: &Resolved) -> Option<(f32, f32)> {
    match &resolved.loaded {
        Loaded::Image(Some(info)) => Some((info.width as f32, info.height as f32)),
        _ => None,
    }
}

/// The mode a kind of picture was last wanted in, and what to tell the owner
/// when the person chooses another.
pub(super) fn memory_of(nav: &PreviewNav, resolved: &Resolved) -> (FitMode, Option<EventHandler<FitMode>>) {
    let mime = resolved.sniffed.mime.mime.to_owned();
    match &nav.zoom_memory {
        Some(memory) => {
            let start = memory.recall(&mime).unwrap_or(FitMode::Fit);
            let memory = memory.clone();
            (start, Some(EventHandler::new(move |mode: FitMode| memory.remember(&mime, mode))))
        }
        None => (FitMode::Fit, None),
    }
}

#[derive(PartialEq)]
pub(super) struct ImageView {
    pub stage: Stage,
}

impl Component for ImageView {
    fn render(&self) -> impl IntoElement {
        let Stage { resolved, nav, .. } = &self.stage;
        if let Route::Helper { raw } = route_of(&resolved) {
            return super::image_helper::HelperPicture {
                resolved: resolved.clone(),
                nav: nav.clone(),
                raw,
            }
            .into_element();
        }
        let natural = shown_size(&resolved);
        let source = resolved.source.clone();
        let is_gif = resolved.sniffed.mime.mime == "image/gif" && resolved.facts.size.is_some_and(|size| size <= GIF_LIMIT);
        let pause = use_hook(GifPause::new);
        if let Some(playing) = nav.playing {
            pause.set(!*playing.read());
        }
        let display = *Platform::get().current_window().scale_factor.read() as f32;
        // The sharp decode only ever gets larger while the picture is on show.
        let sharpest = use_hook(|| Rc::new(Cell::new(0.0f32)));
        let (start, on_mode) = memory_of(&nav, &resolved);

        let keyboard = nav.keyboard;
        let draw = move |width: f32, height: f32| picture(&source, is_gif, &pause, width, height, display, &sharpest);
        let mut view = ZoomView::new(natural, draw)
            .start(start)
            .label(format!("Picture: {}", resolved.source.name()))
            .keys_when(move || keyboard.held());
        if let Some(handle) = nav.zoom {
            view = view.handle(handle);
        }
        if let Some(on_mode) = on_mode {
            view = view.on_mode(on_mode);
        }
        view.into_element()
    }
}

/// The picture element at `width` by `height`: a quick small decode under the
/// sharp one.
fn picture(source: &PreviewSource, gif: bool, pause: &GifPause, width: f32, height: f32, display: f32, sharpest: &Cell<f32>) -> Element {
    let needed = (width.max(height) * display).max(1.0);
    sharpest.set(sharpest.get().max(needed));
    let edge = BUCKETS.iter().copied().find(|bucket| *bucket >= sharpest.get()).unwrap_or(BUCKETS[3]);
    let Some(path) = source.as_path().map(ToOwned::to_owned) else {
        // Bytes in memory: the viewer's own cache key is the name.
        let PreviewSource::Bytes { name, data } = source else {
            return Element::from(rect());
        };
        return ImageViewer::new((name.clone(), bytes::Bytes::copy_from_slice(data)))
            .decode_mode(DecodeMode::Custom(Size2D::new(edge, edge)))
            .width(Size::px(width))
            .height(Size::px(height))
            .aspect_ratio(AspectRatio::Min)
            .image_cover(ImageCover::Center)
            .into();
    };
    if gif {
        return GifViewer::new(path)
            .asset_age(KEEP)
            .pause(pause.clone())
            .width(Size::px(width))
            .height(Size::px(height))
            .aspect_ratio(AspectRatio::Min)
            .image_cover(ImageCover::Center)
            .into();
    }
    if source.name().to_ascii_lowercase().ends_with(".svg") {
        return SvgViewer::new(path).width(Size::px(width)).height(Size::px(height)).into();
    }
    let layer = |edge: f32, fade: u64| {
        ImageViewer::new(path.clone())
            .decode_mode(DecodeMode::Custom(Size2D::new(edge, edge)))
            .asset_age(KEEP)
            .fade_in(fade)
            .width(Size::px(width))
            .height(Size::px(height))
            .aspect_ratio(AspectRatio::Min)
            .image_cover(ImageCover::Center)
    };
    // The small one costs a few milliseconds; it stands until the sharp one arrives.
    let sharp = layer(edge, 160);
    if edge <= FIRST_PASS * 2.0 {
        return sharp.into();
    }
    rect()
        .width(Size::px(width))
        .height(Size::px(height))
        .child(layer(FIRST_PASS, 0).key("first-pass"))
        .child(rect().position(Position::new_absolute().left(0.0).top(0.0)).width(Size::px(width)).height(Size::px(height)).child(sharp.key("sharp")))
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolved(name: &str, mime: &'static str) -> Resolved {
        use bingux_preview_core::{Content, FileFacts, Sniffed, ViewerId};
        let source = PreviewSource::bytes(name, vec![0u8; 4]);
        let mut sniffed = Sniffed { mime: bingux_files::mime::for_name(name), content: Content::Image };
        sniffed.mime.mime = mime;
        Resolved { facts: FileFacts::of(&source, &sniffed), source, sniffed, viewer: ViewerId::Image, loaded: Loaded::Image(None), notes: Vec::new() }
    }

    #[test]
    fn a_photo_stored_on_its_side_is_fitted_as_it_is_shown() {
        use bingux_preview_core::media::{Exif, ImageInfo};
        let mut photo = resolved("IMG_0009.jpg", "image/jpeg");
        // Stored 1200 by 1600, EXIF 6: the header says 1600 by 1200, which is what Skia draws.
        let exif = Exif { orientation: Some(6), ..Exif::default() };
        photo.loaded = Loaded::Image(Some(ImageInfo { width: 1600, height: 1200, exif }));
        assert_eq!(shown_size(&photo), Some((1600.0, 1200.0)));
        assert_eq!(shown_size(&resolved("a.png", "image/png")), None);
    }

    #[test]
    fn the_formats_freya_cannot_read_go_to_the_helper() {
        assert_eq!(route_of(&resolved("a.png", "image/png")), Route::Own);
        assert_eq!(route_of(&resolved("a.jpg", "image/jpeg")), Route::Own);
        assert_eq!(route_of(&resolved("a.svg", "image/svg+xml")), Route::Own);
        assert_eq!(route_of(&resolved("IMG.HEIC", "image/heic")), Route::Helper { raw: false });
        assert_eq!(route_of(&resolved("a.avif", "image/avif")), Route::Helper { raw: false });
        assert_eq!(route_of(&resolved("scan.TIF", "image/tiff")), Route::Helper { raw: false });
        assert_eq!(route_of(&resolved("sky.exr", "image/x-exr")), Route::Helper { raw: false });
        assert_eq!(route_of(&resolved("IMG_0001.CR2", "image/x-canon-cr2")), Route::Helper { raw: true });
        assert_eq!(route_of(&resolved("DSC_0001.nef", "image/tiff")), Route::Helper { raw: true }, "the extension wins over a TIFF header");
        assert_eq!(route_of(&resolved("x.raw", "image/x-panasonic-raw")), Route::Helper { raw: true });
    }
}
