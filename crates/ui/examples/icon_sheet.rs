//! Crispness contact sheets for the bundled icons, drawn by the rasteriser
//! Bingux itself uses (Freya on Skia, through `freya-testing`).
//!
//! ```sh
//! cargo run --manifest-path crates/ui/Cargo.toml --features testing \
//!     --example icon_sheet -- OUT_DIR OURS_DIR [ROW_DIR...]
//! ```
//!
//! For every size (12, 14, 16, 18, 20, 48) and both schemes (white on black,
//! dark on light) it writes `OUT_DIR/sheet-<size>-<scheme>.png` at 1:1: each
//! column is one icon from `OURS_DIR`, with the file of the same name from each
//! `ROW_DIR` (a reference set, or other candidate sets) beneath it. Zoom
//! the PNGs with a nearest-neighbour filter to judge pixels.
//!
//! `ICON_SHEET_SIZES=16,20` limits the sizes. `ICON_SHEET_TOPBAR=FILE` names a
//! text file with one `ours-file reference-file` pair per line (`#` starts a
//! comment) and writes `OUT_DIR/topbar-<scheme>.png`: two 28 px bars of 16 px
//! icons, ours above the first `ROW_DIR`.

use std::fs;
use std::path::{Path, PathBuf};

use bingux_ui::snapshot::snapshot;
use freya::prelude::*;

const SIZES: [u32; 6] = [12, 14, 16, 18, 20, 48];
const COLUMNS: usize = 19;

#[derive(Clone, Copy)]
struct Scheme {
    name: &'static str,
    background: Color,
    ink: Color,
}

const SCHEMES: [Scheme; 2] = [
    Scheme {
        name: "dark",
        background: Color::from_rgb(0, 0, 0),
        ink: Color::from_rgb(255, 255, 255),
    },
    Scheme {
        name: "light",
        background: Color::from_rgb(255, 255, 255),
        ink: Color::from_rgb(34, 34, 34),
    },
];

/// SVG bytes leaked for the life of the process: the viewer wants `'static`.
fn load(path: &Path) -> Option<&'static [u8]> {
    fs::read(path).ok().map(|bytes| &*Vec::leak(bytes))
}

fn glyph(bytes: &'static [u8], size: u32, x: f32, y: f32, ink: Color) -> Element {
    rect()
        .position(Position::new_absolute().left(x).top(y))
        .width(Size::px(size as f32))
        .height(Size::px(size as f32))
        .child(
            SvgViewer::new(bytes)
                .width(Size::px(size as f32))
                .height(Size::px(size as f32))
                .fill(ink),
        )
        .into()
}

fn files(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("read the icon directory")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".svg"))
        .collect();
    names.sort();
    names
}

fn sheet(out: &Path, ours: &Path, others: &[PathBuf], size: u32, scheme: Scheme) {
    let names = files(ours);
    let gap = if size >= 40 { 12 } else { 6 };
    let cell = size + gap;
    let rows_per_icon = 1 + others.len() as u32;
    let band = cell * rows_per_icon + gap;
    let rows = names.len().div_ceil(COLUMNS) as u32;
    let width = COLUMNS as u32 * cell + gap;
    let height = rows * band + gap;
    let entries: Vec<Vec<Option<&'static [u8]>>> = names
        .iter()
        .map(|name| {
            std::iter::once(ours)
                .chain(others.iter().map(PathBuf::as_path))
                .map(|dir| load(&dir.join(name)))
                .collect()
        })
        .collect();
    let path = out.join(format!("sheet-{size}-{}.png", scheme.name));
    snapshot(
        move || {
            let mut root = rect()
                .width(Size::px(width as f32))
                .height(Size::px(height as f32))
                .background(scheme.background);
            for (index, column) in entries.iter().enumerate() {
                let x = (gap + (index % COLUMNS) as u32 * cell) as f32;
                let y = (gap + (index / COLUMNS) as u32 * band) as f32;
                for (row, bytes) in column.iter().enumerate() {
                    if let Some(bytes) = bytes {
                        let y = y + (row as u32 * cell) as f32;
                        root = root.child(glyph(bytes, size, x, y, scheme.ink));
                    }
                }
            }
            Element::from(root)
        },
        (width as f32, height as f32),
        400,
        &path,
    );
    println!("[icon_sheet] wrote {}", path.display());
}

fn topbar(out: &Path, ours: &Path, reference: &Path, list: &Path, scheme: Scheme) {
    let text = fs::read_to_string(list).expect("read the top bar list");
    let pairs: Vec<(Option<&'static [u8]>, Option<&'static [u8]>)> = text
        .lines()
        .map(|line| line.split('#').next().unwrap_or("").trim())
        .filter(|line| !line.is_empty())
        .map(|line| {
            let mut parts = line.split_whitespace();
            let mine = parts.next().unwrap_or("");
            let theirs = parts.next().unwrap_or(mine);
            (load(&ours.join(mine)), load(&reference.join(theirs)))
        })
        .collect();
    let pitch = 32u32;
    let bar = 28u32;
    let width = pitch * pairs.len() as u32 + 16;
    let height = bar * 2;
    let path = out.join(format!("topbar-{}.png", scheme.name));
    snapshot(
        move || {
            let mut root = rect()
                .width(Size::px(width as f32))
                .height(Size::px(height as f32))
                .background(scheme.background);
            for (index, (mine, theirs)) in pairs.iter().enumerate() {
                let x = (16 + index as u32 * pitch) as f32;
                if let Some(bytes) = mine {
                    root = root.child(glyph(bytes, 16, x, 6.0, scheme.ink));
                }
                if let Some(bytes) = theirs {
                    root = root.child(glyph(bytes, 16, x, (bar + 6) as f32, scheme.ink));
                }
            }
            Element::from(root)
        },
        (width as f32, height as f32),
        400,
        &path,
    );
    println!("[icon_sheet] wrote {}", path.display());
}

fn main() {
    let mut args = std::env::args().skip(1);
    let out = PathBuf::from(args.next().expect("OUT_DIR"));
    let ours = PathBuf::from(args.next().expect("OURS_DIR"));
    let others: Vec<PathBuf> = args.map(PathBuf::from).collect();
    let sizes: Vec<u32> = match std::env::var("ICON_SHEET_SIZES") {
        Ok(list) => list
            .split(',')
            .filter_map(|size| size.trim().parse().ok())
            .collect(),
        Err(_) => SIZES.to_vec(),
    };
    let list = std::env::var("ICON_SHEET_TOPBAR").ok().map(PathBuf::from);
    fs::create_dir_all(&out).expect("create the output directory");
    for scheme in SCHEMES {
        for &size in &sizes {
            sheet(&out, &ours, &others, size, scheme);
        }
        if let (Some(reference), Some(list)) = (others.first(), &list) {
            topbar(&out, &ours, reference, list, scheme);
        }
    }
}
