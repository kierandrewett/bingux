//! The wallpaper picker: a grid of thumbnails from the system's and the user's
//! picture folders, narrowed by a source filter and by file name.
//!
//! Thumbnails decode on a worker thread and only once they scroll into view.
//! Every tile has the same size, so nothing moves while a picture decodes.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use freya::prelude::*;

use super::{Loaded, choose_wallpaper, file_name, set_wallpaper};
use crate::system::{self, background};
use crate::theme::Tokens;
use crate::ui::typography::BODY;
use crate::ui::{
    Group, Icon, Overlays, Status, dropdown, icon, search_field, skeleton, status, use_overlays,
    use_tokens,
};
use bingux_ui::style::*;

const SOURCE_ALL: &str = "all";
const SOURCE_SYSTEM: &str = "system";
const SOURCE_YOURS: &str = "yours";

/// Folders that hold the system's pictures. Everything else is the user's.
const SYSTEM_DIRS: [&str; 2] = ["/usr/share/backgrounds", "/usr/share/wallpapers"];
/// Folders that hold the user's pictures, under the home directory.
const USER_DIRS: [&str; 2] = ["Pictures/Wallpapers", ".local/share/backgrounds"];
const PICTURE_EXTENSIONS: [&str; 5] = ["png", "jpg", "jpeg", "webp", "bmp"];
/// Folders deeper than this are not searched.
const MAX_DEPTH: usize = 6;
/// Pictures listed in one folder at most, so a huge folder cannot stall the page.
const MAX_PICTURES: usize = 2000;

/// The thumbnail frame, about 16:10 at both phone and desktop widths.
const THUMB_HEIGHT: f32 = 112.0;
const CAPTION_HEIGHT: f32 = 28.0;
/// The whole tile. Fixed, so the grid holds still while pictures decode.
const TILE_HEIGHT: f32 = THUMB_HEIGHT + CAPTION_HEIGHT;
/// Decode size: twice the frame, so a thumbnail stays sharp on a HiDPI screen.
const DECODE_WIDTH: f32 = 360.0;
const DECODE_HEIGHT: f32 = 224.0;

/// The search text the picker starts with. Snapshots set
/// `BINGUX_SETTINGS_SNAPSHOT_WALLPAPER_SEARCH` so the search can be captured
/// without typing; a real window always starts empty.
fn initial_query() -> String {
    #[cfg(feature = "snapshot")]
    {
        std::env::var("BINGUX_SETTINGS_SNAPSHOT_WALLPAPER_SEARCH").unwrap_or_default()
    }
    #[cfg(not(feature = "snapshot"))]
    {
        String::new()
    }
}

/// The source the picker starts with. Snapshots set
/// `BINGUX_SETTINGS_SNAPSHOT_WALLPAPER_SOURCE`; a real window starts on All.
fn initial_source() -> String {
    #[cfg(feature = "snapshot")]
    {
        std::env::var("BINGUX_SETTINGS_SNAPSHOT_WALLPAPER_SOURCE")
            .unwrap_or_else(|_| SOURCE_ALL.to_string())
    }
    #[cfg(not(feature = "snapshot"))]
    {
        SOURCE_ALL.to_string()
    }
}

#[derive(Clone, PartialEq)]
struct Picture {
    path: String,
    name: String,
    /// Whether the picture lives in a system folder rather than the user's.
    system: bool,
}

fn by_name(a: &Picture, b: &Picture) -> Ordering {
    a.name
        .to_lowercase()
        .cmp(&b.name.to_lowercase())
        .then_with(|| a.path.cmp(&b.path))
}

fn is_picture(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            PICTURE_EXTENSIONS
                .iter()
                .any(|known| ext.eq_ignore_ascii_case(known))
        })
}

/// Collects the pictures under `dir`. Hidden entries and symlinked folders are
/// skipped, so a link cannot send the walk round in a loop.
fn walk(dir: &Path, depth: usize, found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if found.len() >= MAX_PICTURES {
            return;
        }
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            if depth < MAX_DEPTH {
                walk(&path, depth + 1, found);
            }
        } else if path.is_file() && is_picture(&path) {
            found.push(path);
        }
    }
}

/// Lists every picture in the system and user folders, by name. A picture
/// that appears in two folders is listed once, as a system picture.
fn scan() -> Vec<Picture> {
    let mut sources: Vec<(PathBuf, bool)> = SYSTEM_DIRS
        .iter()
        .map(|dir| (PathBuf::from(dir), true))
        .collect();
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        sources.extend(USER_DIRS.iter().map(|dir| (home.join(dir), false)));
    }
    let mut found: BTreeMap<String, bool> = BTreeMap::new();
    for (dir, system) in sources {
        let mut paths = Vec::new();
        walk(&dir, 0, &mut paths);
        for path in paths {
            let system_copy = found
                .entry(path.to_string_lossy().into_owned())
                .or_insert(false);
            *system_copy |= system;
        }
    }
    let mut pictures: Vec<Picture> = found
        .into_iter()
        .map(|(path, system)| Picture {
            name: file_name(&path),
            path,
            system,
        })
        .collect();
    pictures.sort_by(by_name);
    pictures
}

/// Adds the chosen picture when no folder listed it, so its tile can show the
/// accent ring.
fn with_current(mut pictures: Vec<Picture>, current: &str) -> Vec<Picture> {
    if !current.is_empty() && pictures.iter().all(|picture| picture.path != current) {
        pictures.push(Picture {
            name: file_name(current),
            path: current.to_string(),
            system: current.starts_with("/usr/"),
        });
        pictures.sort_by(by_name);
    }
    pictures
}

fn matching<'a>(pictures: &'a [Picture], source: &str, query: &str) -> Vec<&'a Picture> {
    let needle = query.trim().to_lowercase();
    pictures
        .iter()
        .filter(|picture| match source {
            SOURCE_SYSTEM => picture.system,
            SOURCE_YOURS => !picture.system,
            _ => true,
        })
        .filter(|picture| needle.is_empty() || picture.name.to_lowercase().contains(&needle))
        .collect()
}

fn source_options() -> Vec<(String, String)> {
    vec![
        (SOURCE_ALL.to_string(), "All".to_string()),
        (SOURCE_SYSTEM.to_string(), "System".to_string()),
        (SOURCE_YOURS.to_string(), "Your pictures".to_string()),
    ]
}

/// The picker below the desktop preview: a filter row, then the grid of
/// pictures. `current` is the chosen path, or empty for the system background.
#[derive(PartialEq)]
pub struct WallpaperPicker {
    pub current: String,
}

impl Component for WallpaperPicker {
    fn render(&self) -> impl IntoElement {
        let model: State<Option<Loaded>> = system::cached("appearance");
        let overlays: Overlays = use_overlays();
        let tokens = use_tokens();
        let pictures = use_state::<Option<Vec<Picture>>>(|| None);
        let source = use_state(initial_source);
        let query = use_state(initial_query);
        use_hook(move || background(pictures, scan));

        let source_key = source.read().clone();
        let query_text = query.read().clone();
        let searching = !query_text.trim().is_empty();
        let current = self.current.clone();

        let toolbar = rect()
            .tw(
                &tokens,
                &[
                    flex_row(),
                    items(Align::Center),
                    gap(S2),
                    w_full(),
                    grow_children(),
                ],
            )
            .child(
                rect().tw(&tokens, &[flex_row(), w_flex()]).child(
                    search_field(query, |_: String| {})
                        .placeholder("Search pictures")
                        .fill_width(),
                ),
            )
            .child(dropdown(
                source_options(),
                source_key.clone(),
                move |key: String| {
                    let mut source = source;
                    source.set(key);
                },
            ));

        let mut rows: Vec<Element> = vec![rect().tw(&tokens, ROW).child(toolbar).into()];
        let Some(loaded) = pictures.read().clone() else {
            rows.push(skeleton());
            return Element::from(Group::titled("Wallpaper").rows(rows));
        };
        let all = with_current(loaded, &current);
        let shown = matching(&all, &source_key, &query_text);

        let mut tiles: Vec<Element> = vec![add_tile(&tokens, model, overlays)];
        if !searching && source_key != SOURCE_YOURS {
            tiles.push(system_tile(&tokens, model, overlays, current.is_empty()));
        }
        tiles.extend(shown.iter().map(|picture| {
            picture_tile(&tokens, picture, picture.path == current, model, overlays)
        }));

        rows.extend(grid(&tokens, tiles, if touch() { 2 } else { 4 }));
        if all.is_empty() {
            rows.push(
                status(Status::Empty(
                    "No pictures found. Add one with Add picture.".to_string(),
                ))
                .into(),
            );
        } else if shown.is_empty() {
            rows.push(status(Status::Empty("Nothing matches.".to_string())).into());
        }

        Element::from(
            Group::titled("Wallpaper")
                .description(format!("{} pictures", shown.len()))
                .rows(rows),
        )
    }
}

/// Lays the tiles out in rows of `columns`. The last row is padded with empty
/// cells so its tiles keep the width of the others.
fn grid(tokens: &Tokens, tiles: Vec<Element>, columns: usize) -> Vec<Element> {
    let mut tiles = tiles.into_iter().peekable();
    let mut rows = Vec::new();
    while tiles.peek().is_some() {
        let mut line = rect().tw(
            tokens,
            &[flex_row(), grow_children(), gap(S3), w_full(), p(S3)],
        );
        for _ in 0..columns {
            line = match tiles.next() {
                Some(tile) => line.child(
                    rect()
                        .width(Size::flex(1.))
                        .height(Size::px(TILE_HEIGHT))
                        .child(tile),
                ),
                None => line.child(rect().tw(tokens, &[w_flex()])),
            };
        }
        rows.push(line.into());
    }
    rows
}

fn add_tile(tokens: &Tokens, model: State<Option<Loaded>>, overlays: Overlays) -> Element {
    let face = rect()
        .tw(tokens, &[w_full(), h_full(), center(), bg(Paint::Control)])
        .child(icon(
            Icon::ListAdd,
            IconSize::Row,
            Paint::Muted.color(tokens),
        ));
    tile(
        tokens,
        thumb(tokens, false, face.into()),
        "Add picture...",
        move |_| choose_wallpaper(model, overlays),
    )
}

fn system_tile(
    tokens: &Tokens,
    model: State<Option<Loaded>>,
    overlays: Overlays,
    selected: bool,
) -> Element {
    let face = rect().tw(tokens, &[w_full(), h_full(), bg(Paint::AccentSoft)]);
    tile(
        tokens,
        thumb(tokens, selected, face.into()),
        "System background",
        move |_| set_wallpaper(model, overlays, None),
    )
}

fn picture_tile(
    tokens: &Tokens,
    picture: &Picture,
    selected: bool,
    model: State<Option<Loaded>>,
    overlays: Overlays,
) -> Element {
    let path = picture.path.clone();
    let muted = Paint::Muted.color(tokens);
    let face = ImageViewer::new(PathBuf::from(&picture.path))
        .decode_mode(DecodeMode::Custom(Size2D::new(DECODE_WIDTH, DECODE_HEIGHT)))
        .width(Size::fill())
        .height(Size::fill())
        .aspect_ratio(AspectRatio::Max)
        .loading_placeholder(rect().tw(tokens, &[w_full(), h_full(), bg(Paint::Control)]))
        .error_renderer(move |_: String| {
            rect()
                .tw_layout(&[center()])
                .child(icon(Icon::ImageGeneric, IconSize::Row, muted))
                .into()
        });
    tile(
        tokens,
        thumb(tokens, selected, face.into()),
        &picture.name,
        move |_| set_wallpaper(model, overlays, Some(path.clone())),
    )
}

/// The picture frame: rounded, clipped, with a ring and a check when chosen.
fn thumb(tokens: &Tokens, selected: bool, face: Element) -> Element {
    rect()
        .tw(
            tokens,
            &[
                w_full(),
                rounded(Radius::Control),
                clip(),
                bg(Paint::Control),
                border(if selected {
                    Line::Stroke(Dim::Ring, Paint::Accent)
                } else {
                    Line::Outline
                }),
            ],
        )
        .height(Size::px(THUMB_HEIGHT))
        .child(face)
        .maybe_child(selected.then(|| check_badge(tokens)))
        .into()
}

fn check_badge(tokens: &Tokens) -> Element {
    rect()
        .position(Position::new_absolute().top(6.).right(6.))
        .tw(
            tokens,
            &[
                w(Dim::Box5),
                h(Dim::Box5),
                rounded(Radius::Pill),
                bg(Paint::Accent),
                center(),
            ],
        )
        .child(icon(
            Icon::Check,
            IconSize::Mark,
            Paint::OnAccent.color(tokens),
        ))
        .into()
}

/// A tile: the frame over a caption. The whole tile is a button.
fn tile(
    tokens: &Tokens,
    thumb: Element,
    name: &str,
    on_press: impl Fn(Event<PressEventData>) + 'static,
) -> Element {
    rect()
        .tw(tokens, &[flex_col(), w_full()])
        .a11y_role(AccessibilityRole::Button)
        .a11y_alt(name.to_string())
        .on_press(on_press)
        .child(thumb)
        .child(
            rect()
                .tw(tokens, &[w_full()])
                .height(Size::px(CAPTION_HEIGHT))
                .child(
                    BODY.label(tokens, name.to_string())
                        .max_lines(1)
                        .text_overflow(TextOverflow::Ellipsis),
                ),
        )
        .into()
}
