//! The colour icons of files and folders, drawn in the language of the Bingux
//! Files app icon: rounded shapes of one radius ratio, light falling from above
//! (a pale sheen on the top, a soft shade at the foot), a darker back layer, a
//! thin highlight on the top edge. A file is a rounded page with a rounded
//! folded corner and a picture of what it holds; a folder has a tab, a back, the
//! paper standing in it and a lighter front panel.
//!
//! They are a second set beside the symbolic one (`icons.rs`): the sources are
//! `scripts/icons/colour/*.svg`, compiled by `scripts/icons/colour.py` into
//! `icons/colour/<name>-<grid>.svg` for five grids: 16, 20, 24 and 32 are pixel
//! grids (straight edges on whole pixels; curves and diagonals smooth, and the
//! corners turn), 128 is the same drawing as a vector. An icon of 40 px or less
//! is drawn from the pixel grid whose whole multiple is nearest the size asked
//! for ([`colour_grid`]), so 16, 20, 24 and 32 px, and their 2x, land on whole
//! pixels; a larger one (48, 64, 96, 128, 192, ...) is drawn from the vector at
//! any scale. No icon spells its extension: the glyph is a picture at every size.
//!
//! # Colour
//!
//! The SVGs hold roles, not colours: the family colour is the viewer's `fill`
//! (every shape without a fill of its own), the glyph is `currentColor`, and
//! the depth is white and black laid over the body: the sheen at the top
//! ([`LIT_TOP`]), the shade at the foot of a page ([`DEEP_BOTTOM`]), the fold and
//! the back of a folder ([`SHADE`]), and the lift of a folder's front panel
//! ([`FACE_LIGHT`]). The two colours come from the theme, per [`Tone`]:
//!
//! - A **folder and the Trash** are the user's accent (`Tokens::accent`), so
//!   they follow the system accent setting. The front panel (and a bin's can)
//!   shows the accent on the dark scheme (whose accent is already pale) and a
//!   paler lift of it on the light one; the back, the tab, a bin's lid, are the
//!   body under the shade. The glyph is a deep tone of the accent's hue, set into
//!   the panel.
//! - A **file** has one hue per family ([`ColourIcon::hue`]) at a lightness
//!   tuned for the scheme (a light page with a deep ink on the dark scheme, a
//!   deeper page with white ink on the light one, except the yellows, which stay
//!   light and saturated on it and keep a deep ink: a body darkened until it
//!   stands off white at 3:1 turns olive and brown, so the page's silhouette is
//!   held by its **rim** instead).
//!
//! A page has a rim ([`RIM`]): black at 26% over the page along its whole
//! silhouette, one pixel wide inside it (three on the 128 vector). The body only
//! has to stand apart from a ground by [`PAGE_BODY`]; the rim is the edge.
//!
//! The body is moved along lightness until every edge of the picture shows on
//! every surface a row is drawn on (the layers, and each of them under the hover
//! and selection layers), per [`edges`] and [`edge_limits`]: a page's rim, over
//! the body and over its foot, at 3:1 and over its top, under the sheen, at 2:1; a
//! folder's panel at 2.2:1, its top and its back at 1.5:1 (the pale panel holds
//! the folder's silhouette). The ink reads 4.5:1 where a glyph sits
//! ([`glyph_grounds`]). The same tone is drawn whether the row is selected or not.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use freya::prelude::*;

use crate::contrast::{
    AA_LARGE, AA_TEXT, Oklch, composite, ensure_contrast, on_colour_over, worst_ratio,
};
use crate::theme::Tokens;

/// Every colour icon, one per `scripts/icons/colour/<name>.svg`; the table of
/// names is the `colour_icons!` list below.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ColourIcon {
    FilePage,
    FileText,
    FileCode,
    FileConfig,
    FileMarkup,
    FileSpreadsheet,
    FilePresentation,
    FilePdf,
    FileDocument,
    FileEbook,
    FileImage,
    FileAudio,
    FileVideo,
    FileArchive,
    FileDiskImage,
    FileExecutable,
    FileFont,
    FileModel,
    FileDatabase,
    FileCertificate,
    FileTorrent,
    FileUnknown,
    Folder,
    FolderDocuments,
    FolderDownloads,
    FolderMusic,
    FolderPictures,
    FolderVideos,
    FolderDesktop,
    FolderHome,
    FolderTemplates,
    FolderPublic,
    TrashEmpty,
    TrashFull,
}

macro_rules! colour_icons {
    ($($variant:ident => $name:literal,)*) => {
        impl ColourIcon {
            /// Every colour icon, files first, then folders.
            pub const ALL: &[ColourIcon] = &[$(ColourIcon::$variant,)*];

            /// The source name, `file-pdf`.
            pub const fn name(self) -> &'static str {
                match self {
                    $(ColourIcon::$variant => $name,)*
                }
            }

            /// The compiled SVG of this icon for `grid` (one of [`COLOUR_GRIDS`]).
            pub fn bytes(self, grid: u16) -> &'static [u8] {
                match (self, grid) {
                    $(
                        (ColourIcon::$variant, 16) => include_bytes!(concat!("../../icons/colour/", $name, "-16.svg")),
                        (ColourIcon::$variant, 20) => include_bytes!(concat!("../../icons/colour/", $name, "-20.svg")),
                        (ColourIcon::$variant, 24) => include_bytes!(concat!("../../icons/colour/", $name, "-24.svg")),
                        (ColourIcon::$variant, 32) => include_bytes!(concat!("../../icons/colour/", $name, "-32.svg")),
                        (ColourIcon::$variant, _) => include_bytes!(concat!("../../icons/colour/", $name, "-128.svg")),
                    )*
                }
            }
        }
    };
}

// `scripts/icons/colour.py --check` reads this list: keep one `Variant => "name"` per line.
colour_icons! {
    FilePage => "file-page",
    FileText => "file-text",
    FileCode => "file-code",
    FileConfig => "file-config",
    FileMarkup => "file-markup",
    FileSpreadsheet => "file-spreadsheet",
    FilePresentation => "file-presentation",
    FilePdf => "file-pdf",
    FileDocument => "file-document",
    FileEbook => "file-ebook",
    FileImage => "file-image",
    FileAudio => "file-audio",
    FileVideo => "file-video",
    FileArchive => "file-archive",
    FileDiskImage => "file-disk-image",
    FileExecutable => "file-executable",
    FileFont => "file-font",
    FileModel => "file-model",
    FileDatabase => "file-database",
    FileCertificate => "file-certificate",
    FileTorrent => "file-torrent",
    FileUnknown => "file-unknown",
    Folder => "folder",
    FolderDocuments => "folder-documents",
    FolderDownloads => "folder-downloads",
    FolderMusic => "folder-music",
    FolderPictures => "folder-pictures",
    FolderVideos => "folder-videos",
    FolderDesktop => "folder-desktop",
    FolderHome => "folder-home",
    FolderTemplates => "folder-templates",
    FolderPublic => "folder-public",
    TrashEmpty => "trash-empty",
    TrashFull => "trash-full",
}

/// The grids the icons are compiled for: four pixel grids and the vector.
pub const COLOUR_GRIDS: [u16; 5] = [16, 20, 24, 32, VECTOR_GRID];

/// The pixel grids: straight edges on whole pixels, drawn at a whole multiple.
pub const PIXEL_GRIDS: [u16; 4] = [16, 20, 24, 32];

/// The vector grid, for icons larger than [`VECTOR_FROM`] px.
pub const VECTOR_GRID: u16 = 128;

/// The largest edge drawn from a pixel grid; above it the vector is drawn.
pub const VECTOR_FROM: f32 = 40.0;

/// The grid an icon of `edge` logical pixels is drawn from. Up to
/// [`VECTOR_FROM`] px: the pixel grid whose whole multiple is closest to `edge`
/// (relative to the size), the larger on a tie: 16 gives the 16 grid, 20 the 20
/// grid, 24 the 24 grid, 32 the 32 grid, 18 the 20 and 14 the 16. Above it the
/// vector grid, at whatever scale.
pub fn colour_grid(edge: f32) -> u16 {
    if edge > VECTOR_FROM {
        return VECTOR_GRID;
    }
    let misfit = |grid: u16| {
        let ratio = edge / f32::from(grid);
        let whole = ratio.round().max(1.0);
        (((ratio - whole).abs() / ratio) * 10_000.0).round() as i32
    };
    PIXEL_GRIDS
        .iter()
        .copied()
        .min_by_key(|grid| (misfit(*grid), std::cmp::Reverse(*grid)))
        .unwrap_or(16)
}

/// Whether the icon is the accent's (a folder, the Trash) and not a file family's.
impl ColourIcon {
    pub const fn is_folder(self) -> bool {
        matches!(
            self,
            ColourIcon::Folder
                | ColourIcon::FolderDocuments
                | ColourIcon::FolderDownloads
                | ColourIcon::FolderMusic
                | ColourIcon::FolderPictures
                | ColourIcon::FolderVideos
                | ColourIcon::FolderDesktop
                | ColourIcon::FolderHome
                | ColourIcon::FolderTemplates
                | ColourIcon::FolderPublic
                | ColourIcon::TrashEmpty
                | ColourIcon::TrashFull
        )
    }

    /// The family of the icon, as the catalogue names it.
    pub const fn family(self) -> &'static str {
        match self {
            ColourIcon::FilePage => "Generic document",
            ColourIcon::FileText => "Text",
            ColourIcon::FileCode => "Source code",
            ColourIcon::FileConfig => "Configuration and data",
            ColourIcon::FileMarkup => "Markup",
            ColourIcon::FileSpreadsheet => "Spreadsheet",
            ColourIcon::FilePresentation => "Presentation",
            ColourIcon::FilePdf => "PDF",
            ColourIcon::FileDocument => "Word-processing document",
            ColourIcon::FileEbook => "E-book",
            ColourIcon::FileImage => "Image",
            ColourIcon::FileAudio => "Audio",
            ColourIcon::FileVideo => "Video",
            ColourIcon::FileArchive => "Archive",
            ColourIcon::FileDiskImage => "Disk image",
            ColourIcon::FileExecutable => "Program or script",
            ColourIcon::FileFont => "Font",
            ColourIcon::FileModel => "3D model",
            ColourIcon::FileDatabase => "Database",
            ColourIcon::FileCertificate => "Certificate or key",
            ColourIcon::FileTorrent => "Torrent",
            ColourIcon::FileUnknown => "Unknown file",
            ColourIcon::Folder => "Folder",
            ColourIcon::FolderDocuments => "Documents folder",
            ColourIcon::FolderDownloads => "Downloads folder",
            ColourIcon::FolderMusic => "Music folder",
            ColourIcon::FolderPictures => "Pictures folder",
            ColourIcon::FolderVideos => "Videos folder",
            ColourIcon::FolderDesktop => "Desktop folder",
            ColourIcon::FolderHome => "Home folder",
            ColourIcon::FolderTemplates => "Templates folder",
            ColourIcon::FolderPublic => "Public folder",
            ColourIcon::TrashEmpty => "Trash, empty",
            ColourIcon::TrashFull => "Trash, with items",
        }
    }

    /// The hue (OKLCH degrees) and chroma of a file family, `None` for the
    /// accent's folders. One hue per family; the lightness is the scheme's.
    pub const fn hue(self) -> Option<(f32, f32)> {
        Some(match self {
            ColourIcon::FilePage => (260.0, 0.030),
            ColourIcon::FileText => (250.0, 0.050),
            ColourIcon::FileCode => (195.0, 0.100),
            ColourIcon::FileConfig => (80.0, 0.120),
            ColourIcon::FileMarkup => (232.0, 0.105),
            ColourIcon::FileSpreadsheet => (148.0, 0.140),
            ColourIcon::FilePresentation => (50.0, 0.160),
            ColourIcon::FilePdf => (25.0, 0.180),
            ColourIcon::FileDocument => (258.0, 0.160),
            ColourIcon::FileEbook => (64.0, 0.090),
            ColourIcon::FileImage => (335.0, 0.150),
            ColourIcon::FileAudio => (5.0, 0.150),
            ColourIcon::FileVideo => (298.0, 0.150),
            ColourIcon::FileArchive => (92.0, 0.110),
            ColourIcon::FileDiskImage => (240.0, 0.040),
            ColourIcon::FileExecutable => (165.0, 0.110),
            ColourIcon::FileFont => (278.0, 0.140),
            ColourIcon::FileModel => (125.0, 0.130),
            ColourIcon::FileDatabase => (208.0, 0.070),
            ColourIcon::FileCertificate => (102.0, 0.140),
            ColourIcon::FileTorrent => (135.0, 0.160),
            ColourIcon::FileUnknown => (260.0, 0.020),
            _ => return None,
        })
    }
}

/// The two colours a colour icon is drawn in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tone {
    /// The page or the folder: the viewer's `fill`.
    pub body: Color,
    /// The glyph or label: `currentColor`.
    pub ink: Color,
}

/// OKLab lightness of a file page, on the dark and the light scheme.
const PAGE_L_DARK: f32 = 0.76;
const PAGE_L_LIGHT: f32 = 0.52;
/// The yellows and the yellow greens (OKLCH hue 70 to 128) look brown and
/// muddy at the light scheme's deep lightness: their page starts this much
/// lighter there, and this much more saturated (a light yellow at the family's
/// plain chroma goes khaki), and takes dark ink (white no longer reads on it).
/// Its rim holds the edge, so it is not darkened to meet contrast on its own.
const YELLOW_LIGHT_BONUS: f32 = 0.28;
const YELLOW_CHROMA: f32 = 1.3;
const YELLOW_HUES: std::ops::RangeInclusive<f32> = 70.0..=128.0;
/// The ink of a pale page: a near-white tinted with the page's hue.
const WHITE_INK_L: f32 = 0.985;
/// The lightness a deep ink starts from, before it is moved until it reads.
const DEEP_INK_L: f32 = 0.30;

/// The overlays the compiler draws (`scripts/icons/colour.py`: LIT_TOP,
/// DEEP_BOTTOM, SHADE and the LIGHT role; a test reads them back from the
/// SVGs): white at `LIT_TOP` at the top edge, fading to nothing half way down;
/// black from half way down to `DEEP_BOTTOM` at the foot (a page); black at
/// `SHADE` flat, for a folder's back and a page's fold; white at `FACE_LIGHT`
/// flat, which lifts a folder's front panel above its back.
pub const LIT_TOP: f32 = 0.32;
pub const DEEP_BOTTOM: f32 = 0.10;
pub const SHADE: f32 = 0.22;
pub const FACE_LIGHT: f32 = 0.28;
/// How much of the sheen and of the shade lies under a glyph, at most: a page's
/// glyph sits in the lower half (under a little sheen, over the start of the
/// shade); a folder's starts a fifth of the way down its panel.
const GLYPH_LIT_PAGE: f32 = 0.09;
const GLYPH_DEEP_PAGE: f32 = 0.08;
const GLYPH_LIT_FOLDER: f32 = 0.22;
/// The least contrast on a ground, of a page: its rim at the body, under the
/// sheen and over the shade (the rim is the silhouette); of a folder: its front
/// panel lifted (the body of the icon), the top of the panel under the sheen,
/// and the back (the tab and the band above the panel), whose edges the pale
/// panel holds in place.
pub const LIT_EDGE: f32 = 2.0;
pub const FACE_MID: f32 = 2.2;
pub const FACE_EDGE: f32 = 1.5;
pub const BACK_EDGE: f32 = 1.5;
/// The rim of a page (`scripts/icons/colour.py`: RIM; a test reads it back from
/// the SVGs): black at `RIM` over the page along its whole silhouette, inside it.
/// It holds the page's edge on the ground, so the body only has to stand apart
/// from it by `PAGE_BODY`, and a light yellow stays a light yellow.
pub const RIM: f32 = 0.26;
pub const PAGE_BODY: f32 = 1.4;

/// Every ground a row's icon sits on: the opaque layers, and each of them
/// under the hover, press and selection layers.
pub fn grounds(tokens: &Tokens) -> Vec<Color> {
    let layers = [
        tokens.bg,
        tokens.sidebar,
        tokens.card,
        tokens.control,
        tokens.popover,
    ];
    let mut all = layers.to_vec();
    for layer in layers {
        for state in [tokens.hover, tokens.pressed, tokens.selection] {
            all.push(composite(state, layer));
        }
    }
    all
}

/// `white` or `black` at `alpha` over `under`: what the compiled SVG's overlays
/// make of a body colour.
fn over(white: bool, alpha: f32, under: Color) -> Color {
    let v = if white { 255 } else { 0 };
    composite(
        Color::from_argb((alpha * 255.0).round() as u8, v, v, v),
        under,
    )
}

/// `white` over `under` at `alpha` after `then`, as the compiled SVG layers
/// them: the combined opacity of two white layers.
fn white_twice(first: f32, then: f32) -> f32 {
    1.0 - (1.0 - first) * (1.0 - then)
}

/// The colours of `body` that meet a row's ground. A page: its rim, which is its silhouette,
/// over the body, over the sheen at the top and over the shade at the foot. A folder: its
/// front panel lifted, the top of the panel under the sheen too, and the back under the shade.
pub fn edges(body: Color, folder: bool) -> [Color; 3] {
    if folder {
        [
            over(true, FACE_LIGHT, body),
            over(true, white_twice(FACE_LIGHT, LIT_TOP), body),
            over(false, SHADE, body),
        ]
    } else {
        [
            over(false, RIM, body),
            over(false, RIM, over(true, LIT_TOP, body)),
            over(false, RIM, over(false, DEEP_BOTTOM, body)),
        ]
    }
}

/// The least contrast on a ground each of [`edges`] must show.
pub fn edge_limits(folder: bool) -> [f32; 3] {
    if folder {
        [FACE_MID, FACE_EDGE, BACK_EDGE]
    } else {
        [AA_LARGE, LIT_EDGE, AA_LARGE]
    }
}

/// Where a glyph sits: a page's body under a share of the sheen and of the
/// shade; a folder's panel, lifted, under a share of the sheen.
pub fn glyph_grounds(body: Color, folder: bool) -> [Color; 3] {
    if folder {
        [
            over(true, FACE_LIGHT, body),
            over(true, white_twice(FACE_LIGHT, GLYPH_LIT_FOLDER), body),
            over(true, FACE_LIGHT, body),
        ]
    } else {
        [
            body,
            over(true, GLYPH_LIT_PAGE, body),
            over(false, GLYPH_DEEP_PAGE, body),
        ]
    }
}

/// Whether the body, with its overlays, shows on every ground: each of [`edges`]
/// at its [`edge_limits`], a page's body apart from the ground at [`PAGE_BODY`]
/// (inside its rim), and an ink reads on it where a glyph sits.
fn shows(body: Color, grounds: &[Color], folder: bool) -> bool {
    edges(body, folder)
        .iter()
        .zip(edge_limits(folder))
        .all(|(edge, least)| worst_ratio(*edge, grounds) >= least)
        && (folder || worst_ratio(body, grounds) >= PAGE_BODY)
        && worst_ratio(ink_for(body, folder), &glyph_grounds(body, folder)) >= AA_TEXT
}

/// `body` moved along lightness, away from the ground, until [`shows`].
fn settle(body: Color, grounds: &[Color], folder: bool, dark: bool) -> Color {
    let mut at = Oklch::from_colour(body);
    for _ in 0..90 {
        let colour = at.to_colour();
        if shows(colour, grounds, folder) {
            return colour;
        }
        at.l = (at.l + if dark { 0.01 } else { -0.01 }).clamp(0.0, 1.0);
    }
    at.to_colour()
}

/// The ink of `body`: near-white where it reads (a page of the light scheme), else a
/// deep tone of the body's own hue (the pale pages of the dark scheme, a folder's
/// lifted front panel in both), moved until it reads 4.5:1 where a glyph sits.
fn ink_for(body: Color, folder: bool) -> Color {
    let base = Oklch::from_colour(body);
    let ground = glyph_grounds(body, folder);
    let white = Oklch {
        l: WHITE_INK_L,
        c: (base.c * 0.12).min(0.02),
        h: base.h,
    }
    .to_colour();
    if !folder && worst_ratio(white, &ground) >= AA_TEXT {
        return white;
    }
    let mut deep = Oklch {
        l: DEEP_INK_L,
        c: (base.c * 0.55).min(0.09),
        h: base.h,
    };
    for _ in 0..30 {
        if worst_ratio(deep.to_colour(), &ground) >= AA_TEXT {
            return deep.to_colour();
        }
        deep.l = (deep.l - 0.01).max(0.0);
    }
    on_colour_over(&ground)
}

/// The body whose front panel, lifted by [`FACE_LIGHT`] of white, is as light as
/// `shown`, at its hue and a chroma raised to meet the lift's loss of colour.
fn unlifted(shown: Color) -> Color {
    let wanted = Oklch::from_colour(shown);
    let body = |l: f32| Oklch {
        l,
        c: wanted.c * 1.25,
        h: wanted.h,
    };
    let (mut low, mut high) = (0.0_f32, wanted.l);
    for _ in 0..24 {
        let mid = (low + high) / 2.0;
        if Oklch::from_colour(over(true, FACE_LIGHT, body(mid).to_colour())).l < wanted.l {
            low = mid;
        } else {
            high = mid;
        }
    }
    body(high).to_colour()
}

fn derive(icon: ColourIcon, tokens: &Tokens) -> Tone {
    let grounds = grounds(tokens);
    let dark = tokens.scheme.is_dark();
    let start = match icon.hue() {
        Some((hue, chroma)) => {
            let (l, c) = match dark {
                true => (PAGE_L_DARK, chroma),
                false if YELLOW_HUES.contains(&hue) => {
                    (PAGE_L_LIGHT + YELLOW_LIGHT_BONUS, chroma * YELLOW_CHROMA)
                }
                false => (PAGE_L_LIGHT, chroma),
            };
            Oklch { l, c, h: hue }.to_colour()
        }
        // The dark scheme's accent is already lifted towards white: the body is what lifting by
        // FACE_LIGHT turns into the accent, so the front panel shows the accent and the back is
        // its deeper self. The light scheme's accent is the body, and the panel the paler lift.
        None if dark => unlifted(tokens.accent),
        None => tokens.accent,
    };
    // A page's silhouette is held by its rim, not by its body, so the body is only moved as far
    // as `shows` asks: a light yellow stays a light yellow.
    let body = settle(start, &grounds, icon.is_folder(), dark);
    Tone {
        body,
        ink: ink_for(body, icon.is_folder()),
    }
}

type ToneKey = (ColourIcon, bool, [u8; 4]);

/// The tone of `icon` on the theme of `tokens`: the accent's for a folder, the
/// family's for a file. Derived once per icon, scheme and accent.
pub fn colour_tone(icon: ColourIcon, tokens: &Tokens) -> Tone {
    static TONES: LazyLock<Mutex<HashMap<ToneKey, Tone>>> = LazyLock::new(Default::default);
    let accent = [
        tokens.accent.r(),
        tokens.accent.g(),
        tokens.accent.b(),
        tokens.accent.a(),
    ];
    let key = (
        icon,
        tokens.scheme.is_dark(),
        if icon.is_folder() { accent } else { [0; 4] },
    );
    if let Some(found) = TONES.lock().ok().and_then(|map| map.get(&key).copied()) {
        return found;
    }
    let fresh = derive(icon, tokens);
    if let Ok(mut map) = TONES.lock() {
        map.insert(key, fresh);
    }
    fresh
}

/// The picture of `icon` at `edge` logical pixels: the grid it is drawn from,
/// the family colour as `fill`, the glyph colour as `currentColor`. Rasterised
/// on a worker, like every icon, and cached by (icon, size, colours).
pub(crate) fn picture(icon: ColourIcon, edge: f32, tone: Tone) -> SvgViewer {
    let bytes = icon.bytes(colour_grid(edge));
    SvgViewer::new(ImageSource::from((bytes.as_ptr() as usize, bytes)))
        .width(Size::px(edge))
        .height(Size::px(edge))
        .fill(tone.body)
        .color(tone.ink)
        .async_rasterization(true)
        .show_loader(false)
}

/// `icon` drawn at `edge` pixels in the colours of `tokens`; the catalogue's view of one.
pub fn colour_icon_view(tokens: &Tokens, icon: ColourIcon, edge: f32) -> Element {
    picture(icon, edge, colour_tone(icon, tokens)).into()
}
