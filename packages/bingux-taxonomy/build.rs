//! Builds the icon table: one row for every SVG in `packages/bingux-ui/icons`.
//!
//! The Icons page shows every icon file bingux-ui ships, so the list is read
//! from the directory, never typed. A row joins:
//!
//! * the SVG file (embedded, so a file with no `Icon` variant can be drawn),
//! * the `Icon` variant that names it, from `Icon::bytes()` in
//!   `packages/bingux-ui/src/icons.rs`,
//! * where the icon is used and which glyph and weight it is drawn from, from
//!   `scripts/icons/labels.tsv`,
//! * the contact-sheet group it belongs to, from `scripts/icons/groups.txt`.
//!
//! A variant added to `Icon` or a file added to `icons/` appears on the page
//! the next time the app is built.

use std::collections::HashMap;
use std::env;
use std::fmt::Write;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let ui = manifest.join("../bingux-ui");
    let icons_dir = ui.join("icons");
    let icons_rs = ui.join("src/icons.rs");
    let labels = manifest.join("../../scripts/icons/labels.tsv");
    let groups = manifest.join("../../scripts/icons/groups.txt");
    for path in [&icons_dir, &icons_rs, &labels, &groups] {
        println!("cargo:rerun-if-changed={}", path.display());
    }

    // `Self::GoPrevious => include_bytes!("../icons/go-previous-symbolic.svg")`,
    // possibly with the call wrapped in braces over several lines.
    let source = fs::read_to_string(&icons_rs).expect("read icons.rs");
    let mut variant_of: HashMap<String, String> = HashMap::new();
    let mut pending: Option<String> = None;
    for line in source.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Self::")
            && let Some((name, tail)) = rest.split_once("=>")
        {
            pending = Some(name.trim().to_string());
            if let Some(file) = svg_file(tail) {
                variant_of.insert(file, pending.take().unwrap_or_default());
            }
        } else if pending.is_some()
            && let Some(file) = svg_file(line)
        {
            variant_of.insert(file, pending.take().unwrap_or_default());
        }
    }

    // `file | where it is used | metaphor | glyph and weight`
    let mut used: HashMap<String, (String, String, String)> = HashMap::new();
    for line in fs::read_to_string(&labels)
        .expect("read labels.tsv")
        .lines()
    {
        if line.starts_with('#') || !line.contains('|') {
            continue;
        }
        let mut cells = line.split('|').map(str::trim);
        let file = cells.next().unwrap_or_default().to_string();
        let place = cells.next().unwrap_or_default().to_string();
        let metaphor = cells.next().unwrap_or_default().to_string();
        let glyph = cells.next().unwrap_or_default().to_string();
        used.insert(file, (place, metaphor, glyph));
    }

    // `[group]` followed by icon names, several to a line.
    let mut group_of: HashMap<String, String> = HashMap::new();
    let mut group = String::new();
    for line in fs::read_to_string(&groups)
        .expect("read groups.txt")
        .lines()
    {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some(name) = line
            .strip_prefix('[')
            .and_then(|name| name.strip_suffix(']'))
        {
            group = name.to_string();
            continue;
        }
        for name in line.split_whitespace() {
            group_of
                .entry(name.to_string())
                .or_insert_with(|| group.clone());
        }
    }

    let mut files: Vec<String> = fs::read_dir(&icons_dir)
        .expect("read icons dir")
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            name.strip_suffix("-symbolic.svg").map(str::to_string)
        })
        .collect();
    files.sort();

    let mut out = String::from("pub const ICON_FILES: &[IconFile] = &[\n");
    for file in &files {
        let variant = variant_of.get(file);
        let (place, metaphor, glyph) = used.get(file).cloned().unwrap_or_default();
        let svg = icons_dir.join(format!("{file}-symbolic.svg"));
        let _ = writeln!(
            out,
            "    IconFile {{ file: {file:?}, variant: {variant}, icon: {icon}, svg: include_bytes!({svg:?}), place: {place:?}, metaphor: {metaphor:?}, glyph: {glyph:?}, group: {group:?} }},",
            variant = match variant {
                Some(name) => format!("Some({name:?})"),
                None => "None".to_string(),
            },
            icon = match variant {
                Some(name) => format!("Some(bingux_ui::icons::Icon::{name})"),
                None => "None".to_string(),
            },
            svg = svg.display().to_string(),
            group = group_of.get(file).cloned().unwrap_or_default(),
        );
    }
    out.push_str("];\n");
    fs::write(
        PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR")).join("icon_table.rs"),
        out,
    )
    .expect("write icon_table.rs");
}

/// The file stem inside `include_bytes!("../icons/<stem>-symbolic.svg")`.
fn svg_file(text: &str) -> Option<String> {
    let start = text.find("../icons/")? + "../icons/".len();
    let end = text[start..].find("-symbolic.svg")? + start;
    Some(text[start..end].to_string())
}
