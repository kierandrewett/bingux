//! The colour icons: every classified type has one, the folders are the accent
//! whether or not their row is selected, the tones read on every ground, and a
//! phone's file row has the desktop's left edge, gap and padding with a bigger
//! icon.
//!
//! The pure tests read the tables; the rendered ones draw an icon or a list in
//! the headless runner and read pixels or layout back.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use bingux_files::{Entry, Kind, Listing, Selection, mime};
use freya::engine::prelude::{SkData, SkImage, raster_n32_premul};
use freya::prelude::*;
use freya_core::element::ElementExt;
use freya_core::elements::label::Label;
use torin::prelude::Area;

use super::colour::{
    COLOUR_GRIDS, ColourIcon, DEEP_BOTTOM, FACE_LIGHT, LIT_TOP, PAGE_BODY, PIXEL_GRIDS, RIM, SHADE,
    VECTOR_GRID, colour_grid, colour_tone, edge_limits, edges, glyph_grounds, grounds,
};
use super::columns::row_padding;
use super::file_icon::{FileIcon, FileKind, Tint, file_icon_view};
use super::view::FileView;
use super::zoom::{COMPACT_DEFAULT_PX, DEFAULT_PX, LADDER, ViewKind, kind_of};
use crate::contrast::{AA_LARGE, AA_TEXT, Oklch, ratio_opaque, worst_ratio};
use crate::shell::OverlayHost;
use crate::snapshot::Snapshot;
use crate::style::*;
use crate::theme::{Scheme, SharedTokens, Tokens, accent_names, freya_theme, provide_tokens};
use crate::window::WindowHost;

const SCHEMES: [Scheme; 2] = [Scheme::PreferDark, Scheme::PreferLight];

fn accents() -> Vec<&'static str> {
    accent_names().collect()
}

// ---- classification -----------------------------------------------------------------

/// Every extension the Files app classifies has a colour icon, drawn for its
/// kind, and no two kinds share one.
#[test]
fn every_classified_extension_has_a_colour_icon() {
    let mut seen = std::collections::HashSet::new();
    for extension in mime::known_extensions() {
        let name = format!("a.{extension}");
        let icon = FileIcon::for_name(&name);
        let colour = icon
            .colour
            .unwrap_or_else(|| panic!("{name} has no colour icon"));
        assert_eq!(colour, icon.kind.colour(), "{name}");
        assert!(!colour.is_folder(), "{name} is drawn as a folder");
        seen.insert(icon.kind);
    }
    // What the table does not name falls to Unknown, and has one too.
    assert_eq!(
        FileIcon::for_name("a.weird").colour,
        Some(ColourIcon::FileUnknown)
    );
    assert_eq!(FileIcon::for_name("Makefile").kind, FileKind::Code);
    // The table reaches every family but the ones a MIME type or the mode decides.
    for kind in [
        FileKind::Image,
        FileKind::Audio,
        FileKind::Video,
        FileKind::Text,
        FileKind::Code,
        FileKind::Config,
        FileKind::Markup,
        FileKind::Pdf,
        FileKind::Archive,
        FileKind::DiskImage,
        FileKind::Spreadsheet,
        FileKind::Presentation,
        FileKind::Document,
        FileKind::Ebook,
        FileKind::Font,
        FileKind::Executable,
        FileKind::Model3d,
        FileKind::Database,
        FileKind::Certificate,
        FileKind::Torrent,
        FileKind::Unknown,
    ] {
        assert!(seen.contains(&kind), "no listed extension is a {kind:?}");
    }
}

#[test]
fn extensions_and_types_land_in_their_families() {
    for (name, kind) in [
        ("a.json", FileKind::Config),
        ("a.yml", FileKind::Config),
        ("a.toml", FileKind::Config),
        ("a.html", FileKind::Markup),
        ("a.md", FileKind::Markup),
        ("a.xml", FileKind::Markup),
        ("a.css", FileKind::Code),
        ("a.rs", FileKind::Code),
        ("a.py", FileKind::Code),
        ("a.sh", FileKind::Code),
        ("a.txt", FileKind::Text),
        ("a.log", FileKind::Text),
        ("a.xlsx", FileKind::Spreadsheet),
        ("a.ods", FileKind::Spreadsheet),
        ("a.csv", FileKind::Spreadsheet),
        ("a.pptx", FileKind::Presentation),
        ("a.odp", FileKind::Presentation),
        ("a.pdf", FileKind::Pdf),
        ("a.docx", FileKind::Document),
        ("a.odt", FileKind::Document),
        ("a.rtf", FileKind::Document),
        ("a.epub", FileKind::Ebook),
        ("a.mobi", FileKind::Ebook),
        ("a.cbz", FileKind::Ebook),
        ("a.png", FileKind::Image),
        ("a.svg", FileKind::Image),
        ("a.flac", FileKind::Audio),
        ("a.mkv", FileKind::Video),
        ("a.zip", FileKind::Archive),
        ("a.tar.gz", FileKind::Archive),
        ("a.deb", FileKind::Archive),
        ("a.iso", FileKind::DiskImage),
        ("a.img", FileKind::DiskImage),
        ("a.qcow2", FileKind::DiskImage),
        ("a.so", FileKind::Executable),
        ("a.appimage", FileKind::Executable),
        ("a.exe", FileKind::Executable),
        ("a.ttf", FileKind::Font),
        ("a.woff2", FileKind::Font),
        ("a.stl", FileKind::Model3d),
        ("a.glb", FileKind::Model3d),
        ("a.blend", FileKind::Model3d),
        ("a.sqlite", FileKind::Database),
        ("a.kdbx", FileKind::Database),
        ("a.pem", FileKind::Certificate),
        ("a.key", FileKind::Certificate),
        ("a.gpg", FileKind::Certificate),
        ("a.p12", FileKind::Certificate),
        ("a.torrent", FileKind::Torrent),
        ("a.nothing", FileKind::Unknown),
    ] {
        assert_eq!(FileIcon::for_name(name).kind, kind, "{name}");
    }
    for (mime, kind) in [
        ("application/postscript", FileKind::Page),
        ("text/calendar", FileKind::Page),
        ("image/png", FileKind::Image),
        ("model/stl", FileKind::Model3d),
        ("application/x-bittorrent", FileKind::Torrent),
        ("application/vnd.sqlite3", FileKind::Database),
        ("application/x-x509-ca-cert", FileKind::Certificate),
        ("application/pgp-keys", FileKind::Certificate),
        ("application/x-iso9660-image", FileKind::DiskImage),
        ("application/epub+zip", FileKind::Ebook),
        (
            "application/vnd.oasis.opendocument.text",
            FileKind::Document,
        ),
        (
            "application/vnd.oasis.opendocument.spreadsheet",
            FileKind::Spreadsheet,
        ),
        ("application/yaml", FileKind::Config),
        ("text/html", FileKind::Markup),
        ("text/markdown", FileKind::Markup),
    ] {
        assert_eq!(FileIcon::for_mime(mime).kind, kind, "{mime}");
    }
}

#[test]
fn every_kind_has_a_colour_icon_of_its_own() {
    let mut seen = Vec::new();
    for kind in FileKind::ALL {
        let colour = kind.colour();
        assert!(!seen.contains(&colour), "{kind:?} repeats {colour:?}");
        seen.push(colour);
        assert_eq!(FileIcon::for_kind(kind).colour, Some(colour));
    }
    // Every file icon is some kind's; the rest are folders.
    for icon in ColourIcon::ALL {
        assert!(
            icon.is_folder() == !seen.contains(icon) || *icon == ColourIcon::Folder,
            "{icon:?} is neither a kind's icon nor a folder's"
        );
    }
    assert_eq!(ColourIcon::ALL.len(), 34);
}

#[test]
fn folders_and_the_trash_are_coloured_and_drives_are_not() {
    use bingux_files::mounts::MountKind;
    use bingux_files::places::SpecialFolder;
    for special in [
        SpecialFolder::Home,
        SpecialFolder::Desktop,
        SpecialFolder::Documents,
        SpecialFolder::Downloads,
        SpecialFolder::Music,
        SpecialFolder::Pictures,
        SpecialFolder::Videos,
        SpecialFolder::Templates,
        SpecialFolder::PublicShare,
        SpecialFolder::Trash,
    ] {
        let colour = FileIcon::for_special(special)
            .colour
            .unwrap_or_else(|| panic!("{special:?}"));
        assert!(colour.is_folder(), "{special:?}");
    }
    assert_eq!(FileIcon::for_special(SpecialFolder::Root).colour, None);
    assert_eq!(FileIcon::for_mount(MountKind::Removable).colour, None);
    // The Trash keeps its two states: the picture and the tint.
    let empty = FileIcon::for_trash(false);
    let full = FileIcon::for_trash(true);
    assert_eq!(empty.colour, Some(ColourIcon::TrashEmpty));
    assert_eq!(full.colour, Some(ColourIcon::TrashFull));
    assert_eq!((empty.tint, full.tint), (Tint::Dim, Tint::Normal));
}

// ---- assets ---------------------------------------------------------------------------

#[test]
fn every_icon_is_compiled_for_every_grid() {
    for icon in ColourIcon::ALL {
        for grid in COLOUR_GRIDS {
            let text = std::str::from_utf8(icon.bytes(grid)).expect("an SVG is text");
            assert!(
                text.contains(&format!("viewBox=\"0 0 {grid} {grid}\"")),
                "{} for the {grid} grid is drawn on another box",
                icon.name()
            );
            assert!(
                !text.contains("fill=\"#fe"),
                "{} holds a stand-in colour",
                icon.name()
            );
        }
    }
}

/// Up to 40 px an icon is drawn from the pixel grid it is a whole multiple of;
/// above it, from the vector.
#[test]
fn an_icon_is_drawn_from_the_grid_whose_multiple_is_its_size() {
    for (edge, grid) in [
        (16.0, 16),
        (20.0, 20),
        (24.0, 24),
        (32.0, 32),
        (40.0, 20),
        (48.0, 128),
        (64.0, 128),
        (96.0, 128),
        (128.0, 128),
        (192.0, 128),
        (256.0, 128),
        (18.0, 20),
        (14.0, 16),
    ] {
        assert_eq!(colour_grid(edge), grid, "{edge} px");
    }
    // Every step of the ladder is a whole multiple of its pixel grid, or the vector.
    for px in LADDER {
        let grid = colour_grid(f32::from(px));
        if px <= 40 {
            assert!(
                PIXEL_GRIDS.contains(&grid) && px % grid == 0,
                "{px} px from the {grid} grid"
            );
        } else {
            assert_eq!(grid, VECTOR_GRID, "{px} px");
        }
    }
}

/// A glyph is a picture on every grid, never letters: each icon but the blank page and the
/// plain folder draws its glyph in the ink on all five, and none spells its extension.
#[test]
fn every_icon_draws_its_glyph_on_every_grid() {
    for icon in ColourIcon::ALL
        .iter()
        .filter(|icon| !matches!(icon, ColourIcon::FilePage | ColourIcon::Folder))
    {
        for grid in COLOUR_GRIDS {
            let text = std::str::from_utf8(icon.bytes(grid)).expect("an SVG is text");
            assert!(
                text.contains("currentColor"),
                "{} has no glyph at {grid}",
                icon.name()
            );
            assert!(
                !text.contains("<text"),
                "{} spells its glyph at {grid}",
                icon.name()
            );
        }
    }
}

/// The overlays the Rust side keeps the tones readable under are the ones the
/// compiler draws.
#[test]
fn the_overlays_are_the_ones_the_compiler_draws() {
    let text = |icon: ColourIcon| {
        String::from_utf8(icon.bytes(VECTOR_GRID).to_vec()).expect("an SVG is text")
    };
    let (folder, page) = (text(ColourIcon::Folder), text(ColourIcon::FilePdf));
    let opacity = |svg: &str, id: &str| -> f32 {
        let gradient = &svg[svg
            .find(&format!("id=\"{id}\""))
            .unwrap_or_else(|| panic!("no {id} gradient"))..];
        let gradient = &gradient[..gradient
            .find("</linearGradient>")
            .expect("a closed gradient")];
        gradient
            .split("stop-opacity=\"")
            .skip(1)
            .filter_map(|rest| rest[..rest.find('"')?].parse::<f32>().ok())
            .fold(0.0, f32::max)
    };
    assert!((opacity(&folder, "lit") - LIT_TOP).abs() < 1e-4);
    assert!((opacity(&page, "deep") - DEEP_BOTTOM).abs() < 1e-4);
    for alpha in [SHADE, FACE_LIGHT] {
        let wanted = format!(
            "fill-opacity=\"{}\"",
            format!("{alpha}").trim_start_matches('0')
        );
        assert!(folder.contains(&wanted), "the folder has no {wanted}");
    }
    assert!(
        page.contains(&format!(
            "fill-opacity=\"{}\"",
            format!("{SHADE}").trim_start_matches('0')
        )),
        "the page's fold"
    );
}

/// Every icon draws the same light: a sheen at the top, and a shade at the foot
/// of a page; the corners are the app icon's rounded ones (no `M..H..V..Z`
/// boxes in the chrome).
#[test]
fn the_chrome_is_rounded_and_lit_from_above() {
    for icon in ColourIcon::ALL
        .iter()
        .filter(|icon| !matches!(icon, ColourIcon::TrashEmpty | ColourIcon::TrashFull))
    {
        for grid in COLOUR_GRIDS {
            let text = std::str::from_utf8(icon.bytes(grid)).expect("an SVG is text");
            assert!(
                text.contains("fill=\"url(#lit)\""),
                "{} at {grid} has no sheen",
                icon.name()
            );
            if !icon.is_folder() {
                assert!(
                    text.contains("fill=\"url(#deep)\""),
                    "{} at {grid} has no shade",
                    icon.name()
                );
            }
            let rounded = text
                .as_bytes()
                .windows(2)
                .any(|pair| pair[0].is_ascii_digit() && pair[1] == b'C');
            assert!(rounded, "{} at {grid} has square corners", icon.name());
        }
    }
}

// ---- tones ------------------------------------------------------------------------------

fn tokens(accent: &'static str, scheme: Scheme) -> Tokens {
    Tokens::new(accent, scheme, false)
}

/// Every edge of the icon shows on every ground at its limit, and the ink reads
/// under the sheen and the shade where a glyph sits.
fn assert_reads(icon: ColourIcon, tone: super::colour::Tone, grounds: &[Color], what: &str) {
    for (edge, least) in edges(tone.body, icon.is_folder())
        .into_iter()
        .zip(edge_limits(icon.is_folder()))
    {
        let shown = worst_ratio(edge, grounds);
        assert!(
            shown >= least - 0.01,
            "{icon:?} {what}: an edge shows {shown:.2}:1, wanted {least}"
        );
    }
    for under in glyph_grounds(tone.body, icon.is_folder()) {
        let read = ratio_opaque(tone.ink, under);
        assert!(
            read >= AA_TEXT - 0.01,
            "{icon:?} {what}: the ink reads {read:.2}:1 under the overlays"
        );
    }
}

/// With `BINGUX_COLOUR_TONES=FILE`, write the tones every icon takes on both schemes as JSON
/// (`{"dark": {"file-pdf": ["#body", "#ink"], ...}, "light": ...}`), for
/// `scripts/icons/colour.py --sheet DIR --tones FILE`: a contact sheet in the colours the app
/// draws, not a stand-in palette. Without it the test does nothing.
#[test]
fn the_tones_can_be_written_for_the_contact_sheet() {
    let Some(path) = std::env::var_os("BINGUX_COLOUR_TONES") else {
        return;
    };
    let hex = |c: Color| format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b());
    let mut out = String::from("{");
    for (at, scheme) in SCHEMES.into_iter().enumerate() {
        let tokens = tokens("blue", scheme);
        out += &format!(
            "{}\"{}\": {{",
            if at > 0 { "," } else { "" },
            if scheme.is_dark() { "dark" } else { "light" }
        );
        for (index, icon) in ColourIcon::ALL.iter().enumerate() {
            let tone = colour_tone(*icon, &tokens);
            out += &format!(
                "{}\"{}\": [\"{}\", \"{}\"]",
                if index > 0 { "," } else { "" },
                icon.name(),
                hex(tone.body),
                hex(tone.ink)
            );
        }
        out += "}";
    }
    out += "}";
    std::fs::write(path, out).expect("the tones file is written");
}

#[test]
fn a_file_tone_shows_on_every_ground_and_its_ink_reads_on_it() {
    for scheme in SCHEMES {
        let tokens = tokens("blue", scheme);
        let grounds = grounds(&tokens);
        for icon in ColourIcon::ALL.iter().filter(|icon| !icon.is_folder()) {
            let tone = colour_tone(*icon, &tokens);
            // The silhouette is the rim (assert_reads); the body inside it stands apart.
            let shown = worst_ratio(tone.body, &grounds);
            assert!(
                shown >= PAGE_BODY - 0.01,
                "{icon:?} on {scheme:?}: the page shows {shown:.2}:1"
            );
            let read = ratio_opaque(tone.ink, glyph_grounds(tone.body, icon.is_folder())[0]);
            assert!(
                read >= AA_TEXT - 0.01,
                "{icon:?} on {scheme:?}: the ink reads {read:.2}:1"
            );
            assert_reads(*icon, tone, &grounds, &format!("on {scheme:?}"));
        }
    }
}

/// The rim the Rust tones are computed through is the one the SVGs draw.
#[test]
fn the_rim_is_the_one_the_compiler_draws() {
    for icon in ColourIcon::ALL.iter().filter(|icon| !icon.is_folder()) {
        for grid in COLOUR_GRIDS {
            let text = std::str::from_utf8(icon.bytes(grid)).expect("an SVG is text");
            let rim = text
                .lines()
                .find(|line| line.contains("clip-path=\"url(#page)\""))
                .unwrap_or_else(|| panic!("{} at {grid} has no rim", icon.name()));
            let opacity: f32 = rim
                .split("stroke-opacity=\"")
                .nth(1)
                .and_then(|rest| rest[..rest.find('"')?].parse().ok())
                .expect("the rim has an opacity");
            assert!(
                (opacity - RIM).abs() < 1e-4,
                "{} at {grid}: rim {opacity}",
                icon.name()
            );
            assert!(
                text.contains("<clipPath id=\"page\">"),
                "{} at {grid}: the rim has no clip",
                icon.name()
            );
        }
    }
    // Only a page has one: a folder and the bin keep their own chrome.
    for icon in [ColourIcon::Folder, ColourIcon::TrashEmpty] {
        assert!(
            !std::str::from_utf8(icon.bytes(VECTOR_GRID))
                .unwrap()
                .contains("url(#page)")
        );
    }
}

/// On the light scheme the yellow families stay yellow: a light, saturated body of the family's
/// hue (the rim holds the edge, the ink is deep), not the olive-brown a body darkened to 3:1 on
/// white turns into. On the dark scheme they are as light as every other family.
#[test]
fn the_yellow_families_stay_yellow_on_the_light_scheme() {
    let light = tokens("blue", Scheme::PreferLight);
    let grounds = grounds(&light);
    for icon in [
        ColourIcon::FileArchive,
        ColourIcon::FileCertificate,
        ColourIcon::FileConfig,
    ] {
        let tone = colour_tone(icon, &light);
        let body = Oklch::from_colour(tone.body);
        let (hue, _) = icon.hue().expect("a file family has a hue");
        assert!(
            body.l >= 0.66,
            "{icon:?} is too dark to read as yellow: L {:.2}",
            body.l
        );
        assert!(
            body.c >= 0.10,
            "{icon:?} is too grey to read as yellow: C {:.3}",
            body.c
        );
        let apart = ((body.h - hue).abs() + 180.0).rem_euclid(360.0) - 180.0;
        assert!(
            apart.abs() < 8.0,
            "{icon:?} has left its hue {hue}: {}",
            body.h
        );
        // The edge limits hold through the rim, and the glyph is a deep ink.
        assert_reads(icon, tone, &grounds, "on the light scheme");
        assert!(
            Oklch::from_colour(tone.ink).l < 0.4,
            "{icon:?}: a deep ink on a light yellow"
        );
    }
    // Every other family stays as deep as before: the change is the yellows'.
    let tone = colour_tone(ColourIcon::FilePdf, &light);
    assert!(Oklch::from_colour(tone.body).l <= 0.54);
}

/// The hue of a file family is its own: a family's page keeps its hue when it
/// is moved to reach contrast, and no two families sit within 10 degrees unless
/// one of them is near-neutral.
#[test]
fn each_family_has_a_hue_of_its_own() {
    let hues: Vec<(ColourIcon, f32, f32)> = ColourIcon::ALL
        .iter()
        .filter_map(|icon| icon.hue().map(|(hue, chroma)| (*icon, hue, chroma)))
        .collect();
    for scheme in SCHEMES {
        let tokens = tokens("blue", scheme);
        for (icon, hue, chroma) in &hues {
            let drawn = Oklch::from_colour(colour_tone(*icon, &tokens).body);
            if *chroma >= 0.08 && drawn.c > 0.02 {
                let apart = ((drawn.h - hue).abs() + 180.0).rem_euclid(360.0) - 180.0;
                assert!(
                    apart.abs() < 12.0,
                    "{icon:?} on {scheme:?} has moved from {hue} to {}",
                    drawn.h
                );
            }
        }
    }
    let strong: Vec<_> = hues
        .iter()
        .filter(|(_, _, chroma)| *chroma >= 0.08)
        .collect();
    for (index, (a, hue_a, _)) in strong.iter().enumerate() {
        for (b, hue_b, _) in &strong[index + 1..] {
            let apart = ((hue_a - hue_b).abs() + 180.0).rem_euclid(360.0) - 180.0;
            assert!(
                apart.abs() >= 8.0,
                "{a:?} and {b:?} share a hue ({hue_a} and {hue_b})"
            );
        }
    }
}

/// A folder and the Trash are the user's accent, in every scheme and for every
/// accent, and the glyph on them is the accent's own ink.
#[test]
fn a_folder_tone_is_the_accent_and_follows_it() {
    for scheme in SCHEMES {
        let mut bodies = Vec::new();
        for accent in accents() {
            let tokens = tokens(accent, scheme);
            let grounds = grounds(&tokens);
            for icon in ColourIcon::ALL.iter().filter(|icon| icon.is_folder()) {
                let tone = colour_tone(*icon, &tokens);
                assert_reads(*icon, tone, &grounds, &format!("{accent} on {scheme:?}"));
                // The accent's hue, as far as the 3:1 move leaves it.
                let (drawn, wanted) = (
                    Oklch::from_colour(tone.body),
                    Oklch::from_colour(tokens.accent),
                );
                if wanted.c > 0.03 {
                    let apart = ((drawn.h - wanted.h).abs() + 180.0).rem_euclid(360.0) - 180.0;
                    assert!(
                        apart.abs() < 8.0,
                        "{icon:?} {accent} {scheme:?}: hue {} for the accent's {}",
                        drawn.h,
                        wanted.h
                    );
                }
            }
            bodies.push(colour_tone(ColourIcon::Folder, &tokens).body);
        }
        bodies.dedup();
        assert!(
            bodies.len() > 3,
            "{scheme:?}: the folder does not follow the accent"
        );
    }
}

// ---- pixels -------------------------------------------------------------------------------

/// One frame as the renderer drew it.
struct Frame {
    bytes: Vec<u8>,
    row: usize,
    width: usize,
    height: usize,
}

impl Frame {
    fn of(shot: &mut Snapshot) -> Self {
        let png = shot.runner().render();
        let image = SkImage::from_encoded(SkData::new_copy(&png)).expect("the frame decodes");
        let mut surface =
            raster_n32_premul((image.width(), image.height())).expect("a surface to read from");
        surface.canvas().draw_image(&image, (0, 0), None);
        let info = surface.canvas().image_info();
        let row = info.min_row_bytes();
        let mut bytes = vec![0u8; info.compute_byte_size(row)];
        assert!(surface.canvas().read_pixels(&info, &mut bytes, row, (0, 0)));
        Self {
            bytes,
            row,
            width: image.width() as usize,
            height: image.height() as usize,
        }
    }

    fn pixels(&self) -> impl Iterator<Item = Color> + '_ {
        (0..self.height).flat_map(move |y| {
            (0..self.width).map(move |x| {
                let at = y * self.row + x * 4;
                // N32 on a little-endian machine is blue, green, red, alpha.
                colour_from_argb(255, self.bytes[at + 2], self.bytes[at + 1], self.bytes[at])
            })
        })
    }
}

/// How far two colours are apart: the sum of their channel differences.
fn apart(a: Color, b: Color) -> u32 {
    let diff = |x: u8, y: u8| u32::from(x.abs_diff(y));
    diff(a.r(), b.r()) + diff(a.g(), b.g()) + diff(a.b(), b.b())
}

/// `icon` alone on the page of `tokens`, drawn at `px`, at `scale`.
fn drawn(spec: FileIcon, tokens: Tokens, px: u16, selected: bool, scale: f64) -> Frame {
    let edge = f32::from(px) + 8.0;
    let mut shot = Snapshot::with_scale(
        move || {
            let shared = use_hook(move || SharedTokens(State::create(tokens)));
            use_provide_root_context(|| shared);
            let _ = use_init_theme(|| freya_theme(&tokens));
            rect()
                .width(Size::fill())
                .height(Size::fill())
                .background(tokens.bg)
                .padding(4.0)
                .child(file_icon_view(&tokens, &spec, px, selected))
                .into()
        },
        (edge, edge),
        scale,
    );
    // The picture is rasterised on a worker: settle until it is on the page (the
    // frame is more than the plain background), and give up after five seconds.
    for _ in 0..50 {
        shot.settle(Duration::from_millis(100));
        let frame = Frame::of(&mut shot);
        if frame.pixels().any(|pixel| apart(pixel, tokens.bg) > 24) {
            shot.settle(Duration::from_millis(100));
            return Frame::of(&mut shot);
        }
    }
    Frame::of(&mut shot)
}

/// How many pixels of `frame` are the accent's hue: near the accent's OKLCH hue
/// and clearly coloured.
fn accent_pixels(frame: &Frame, accent: Color) -> usize {
    let wanted = Oklch::from_colour(accent);
    frame
        .pixels()
        .filter(|pixel| {
            let seen = Oklch::from_colour(*pixel);
            let apart = ((seen.h - wanted.h).abs() + 180.0).rem_euclid(360.0) - 180.0;
            seen.c > 0.06 && apart.abs() < 14.0
        })
        .count()
}

/// The pixels of a folder contain the accent hue when its row is not selected
/// (and when it is), at each size, for each accent, on each scheme; a file's do
/// not, and a file's own hue is not the accent's.
#[test]
fn a_folder_is_drawn_in_the_accent_unselected() {
    for scheme in SCHEMES {
        for accent in ["blue", "green", "pink", "orange"] {
            let tokens = tokens(accent, scheme);
            for px in [16u16, 32, 64, 128] {
                for selected in [false, true] {
                    let frame = drawn(
                        FileIcon::for_kind(FileKind::Folder),
                        tokens,
                        px,
                        selected,
                        1.0,
                    );
                    let found = accent_pixels(&frame, tokens.accent);
                    let wanted = (px as usize * px as usize) / 4;
                    assert!(
                        found >= wanted,
                        "a {px} px folder (selected {selected}) on {scheme:?} with {accent}: {found} accent pixels, wanted {wanted}"
                    );
                }
            }
        }
    }
    // A PDF is red whatever the accent: its pixels are not the blue accent's.
    let tokens = tokens("blue", Scheme::PreferDark);
    let pdf = drawn(FileIcon::for_kind(FileKind::Pdf), tokens, 32, false, 1.0);
    assert!(
        accent_pixels(&pdf, tokens.accent) < 40,
        "a PDF is drawn in the accent"
    );
}

/// The corners are the app icon's rounded ones, on the pixel grid as well as the
/// vector: the pixel at the very corner of a page's box, or of a folder's tab,
/// is the page behind it, and a pixel inside the shape is not.
#[test]
fn the_corners_of_a_page_and_a_folder_are_rounded() {
    for scheme in SCHEMES {
        let tokens = tokens("blue", scheme);
        // (kind, px, the corner pixel and a pixel inside the shape, in icon pixels)
        for (kind, px, corner, inside) in [
            (FileKind::Pdf, 32u16, (4, 2), (12, 14)),
            (FileKind::Pdf, 128, (16, 8), (60, 108)),
            (FileKind::Folder, 32, (3, 4), (12, 20)),
            (FileKind::Folder, 128, (12, 16), (60, 80)),
        ] {
            let frame = drawn(FileIcon::for_kind(kind), tokens, px, false, 1.0);
            let at = |(x, y): (usize, usize)| {
                frame
                    .pixels()
                    .nth((y + 4) * frame.width + x + 4)
                    .expect("a pixel")
            };
            assert!(
                apart(at(corner), tokens.bg) < 24,
                "{kind:?} at {px} px on {scheme:?}: the corner {corner:?} is drawn"
            );
            assert!(
                apart(at(inside), tokens.bg) > 60,
                "{kind:?} at {px} px on {scheme:?}: {inside:?} is not drawn"
            );
        }
    }
}

/// The Trash and every special folder are the accent too, empty or full.
#[test]
fn the_user_folders_and_the_trash_are_the_accent() {
    let tokens = tokens("teal", Scheme::PreferDark);
    for spec in [
        FileIcon::for_trash(true),
        FileIcon::for_trash(false).hidden(false),
        FileIcon::for_special(bingux_files::places::SpecialFolder::Downloads),
        FileIcon::for_special(bingux_files::places::SpecialFolder::Home),
    ] {
        let spec = FileIcon {
            tint: Tint::Normal,
            ..spec
        };
        let frame = drawn(spec, tokens, 32, false, 1.0);
        assert!(
            accent_pixels(&frame, tokens.accent) >= 200,
            "{:?}",
            spec.colour
        );
    }
}

/// A hidden folder is faint and a selected hidden one is not.
#[test]
fn a_hidden_colour_icon_is_faint_until_it_is_selected() {
    let tokens = tokens("blue", Scheme::PreferDark);
    let hidden = FileIcon::for_kind(FileKind::Folder).hidden(true);
    let faint = drawn(hidden, tokens, 32, false, 1.0);
    let full = drawn(hidden, tokens, 32, true, 1.0);
    let best = |frame: &Frame| {
        frame
            .pixels()
            .map(|p| ratio_opaque(p, tokens.bg))
            .fold(0.0_f32, f32::max)
    };
    assert!(
        best(&faint) + 0.5 < best(&full),
        "{} against {}",
        best(&faint),
        best(&full)
    );
}

// ---- the list --------------------------------------------------------------------------------

fn text_of(element: &dyn ElementExt) -> Option<String> {
    Label::try_downcast(element).map(|label| label.text.to_string())
}

/// A listing of `count` files and folders, the first being `Alpha.txt`.
fn listing(count: usize) -> (Arc<Listing>, Arc<Vec<usize>>) {
    let root = PathBuf::from("/nonexistent-bingux-colour-tests");
    let mut listing = Listing::new(root.clone());
    for index in 0..count {
        let (name, kind) = match index {
            0 => ("Alpha.txt".to_owned(), Kind::File),
            1 => ("Beta".to_owned(), Kind::Directory),
            _ => (format!("File {index:03}.pdf"), Kind::File),
        };
        listing
            .entries
            .push(Entry::named(root.join(&name), name, kind, false, false));
    }
    listing.complete = true;
    let order = (0..count).collect();
    (Arc::new(listing), Arc::new(order))
}

#[derive(Clone, PartialEq)]
struct Page {
    count: usize,
    px: Option<u16>,
}

/// The window root: the tokens, then the window host that sets the density
/// from the window's width.
#[derive(PartialEq)]
struct Root(Page);

impl Component for Root {
    fn render(&self) -> impl IntoElement {
        let _tokens = provide_tokens();
        WindowHost::new(self.0.clone())
    }
}

impl Component for Page {
    fn render(&self) -> impl IntoElement {
        let (listing, order) = listing(self.count);
        let level = use_state(|| self.px.unwrap_or(0));
        let selection = use_state(Selection::default);
        let mut view = FileView::new(listing, order, selection).thumbnails(false);
        if self.px.is_some() {
            view = view.icon_px(level);
        }
        OverlayHost::new(view).into_element()
    }
}

/// The list as the desktop (1000 px wide) or the phone (390 px) draws it: the
/// areas of the first row, its icon and its name.
struct Measured {
    window: f32,
    row: Area,
    icon: Area,
    name: Area,
}

fn measure(window: f32, px: Option<u16>) -> Measured {
    let mut shot = Snapshot::new(
        move || Root(Page { count: 60, px }).into_element(),
        (window, 600.0),
    );
    shot.settle(Duration::from_millis(700));
    let by_class = |shot: &mut Snapshot, class: &'static str| -> Vec<Area> {
        let mut found: Vec<Area> = shot.runner().find_many(|node, element| {
            (element.accessibility().builder.class_name() == Some(class))
                .then(|| node.layout().area)
        });
        found.sort_by(|a, b| a.min_y().total_cmp(&b.min_y()));
        found
    };
    let row = *by_class(&mut shot, "FileRow").first().expect("a row");
    let icons = by_class(&mut shot, "FileIcon");
    let icon = *icons
        .iter()
        .find(|area| area.min_y() >= row.min_y() - 0.1)
        .expect("an icon");
    let name = shot
        .runner()
        .find_many(|node, element| (text_of(element)? == "Alpha.txt").then(|| node.layout().area))
        .first()
        .copied()
        .expect("the first name");
    Measured {
        window,
        row,
        icon,
        name,
    }
}

/// The phone's row has the desktop's padding, icon column and gap: the same
/// distance from the row's left edge to the icon and from the icon to the name,
/// whatever the width. Only the icon's size (and so the row's height) differs.
#[test]
fn a_phone_row_has_the_desktops_left_edge_and_gap() {
    let desktop = measure(1000.0, Some(16));
    let phone = measure(390.0, Some(16));
    let inset = |m: &Measured| m.icon.min_x() - m.row.min_x();
    let gap = |m: &Measured| m.name.min_x() - m.icon.max_x();
    // The row's fill is LIST_EDGE in from its box and the icon FILE_ROW_INSET in from the fill.
    assert!(
        (inset(&desktop) - row_padding()).abs() < 0.51,
        "desktop: {} from the row",
        inset(&desktop)
    );
    assert!(
        (inset(&desktop) - inset(&phone)).abs() < 0.01,
        "the icon is {} from the row's edge on a desktop and {} on a phone",
        inset(&desktop),
        inset(&phone)
    );
    assert!(
        (gap(&desktop) - gap(&phone)).abs() < 0.01,
        "the name is {} from the icon on a desktop and {} on a phone",
        gap(&desktop),
        gap(&phone)
    );
    assert_eq!(desktop.icon.width(), phone.icon.width());
}

/// Whatever the scrollbar does, the phone's rows are as far from the list's
/// left side as from its right.
#[test]
fn a_phone_row_is_symmetric_in_a_list_that_scrolls() {
    for window in [1000.0, 390.0] {
        let measured = measure(window, None);
        let left = measured.row.min_x();
        let right = measured.window - measured.row.max_x();
        assert!(
            (left - right).abs() < 0.51,
            "{window} px wide: {left} on the left, {right} on the right"
        );
    }
}

/// A phone opens at the ladder's last list level, 32 px, in a row of 56 px;
/// the desktop keeps 16 px in a row of 40. The name follows the icon.
#[test]
fn the_icon_is_32_px_on_a_phone_and_16_px_on_a_desktop() {
    assert_eq!(kind_of(COMPACT_DEFAULT_PX), ViewKind::List);
    assert!(LADDER.contains(&COMPACT_DEFAULT_PX));
    assert!(COMPACT_DEFAULT_PX > DEFAULT_PX);
    let desktop = measure(1000.0, None);
    let phone = measure(390.0, None);
    assert_eq!(desktop.icon.width(), f32::from(DEFAULT_PX));
    assert_eq!(phone.icon.width(), f32::from(COMPACT_DEFAULT_PX));
    assert_eq!(desktop.row.height(), Dim::NavItem.px_in(SizeClass::Regular));
    assert_eq!(phone.row.height(), Dim::NavItem.px_in(SizeClass::Compact));
    // The icon is centred in its row; the name starts a gutter and a cell's padding after it.
    assert!(
        (phone.icon.min_y() + phone.icon.height() / 2.0
            - (phone.row.min_y() + phone.row.height() / 2.0))
            .abs()
            < 1.0
    );
    let gap = |m: &Measured| m.name.min_x() - m.icon.max_x();
    assert!((gap(&desktop) - gap(&phone)).abs() < 0.01);
}
