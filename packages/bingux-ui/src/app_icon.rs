//! Full-colour application icons, found the way the shell finds them.
//!
//! The dock, search and the window switcher resolve an icon name through the
//! GTK icon theme. This module does the same lookup from the Icon Theme
//! Specification, so a row in Settings shows the icon the dock shows: the
//! current GTK icon theme and the themes it inherits, then `hicolor`, then
//! `/usr/share/pixmaps`. A name that is an absolute path is used as it is.
//! An icon that cannot be found is drawn as a tile with the generic
//! application glyph, never as an empty gap.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use bytes::Bytes;
use freya::prelude::*;

use crate::icons::{self, Icon};
use crate::runtime::background_then;
use crate::style::*;
use crate::surface::use_tokens;

/// An icon file and whether it is a vector image. A vector icon is read
/// once, and carries its text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IconFile {
    pub path: PathBuf,
    pub svg: Option<Bytes>,
}

impl IconFile {
    fn new(path: PathBuf, vector: bool) -> Option<Self> {
        let svg = if vector {
            Some(Bytes::from(with_view_box(crate::runtime::read_file(&path).ok()?)))
        } else {
            None
        };
        Some(Self { path, svg })
    }
}

/// Freya scales an SVG through its `viewBox`. Inkscape and other tools can
/// write only `width` and `height`, and Freya then draws the top-left corner
/// at full size. Add the `viewBox` those two numbers describe.
fn with_view_box(source: Vec<u8>) -> Vec<u8> {
    let Ok(text) = std::str::from_utf8(&source) else {
        return source;
    };
    let Some(start) = text.find("<svg") else {
        return source;
    };
    let Some(length) = text[start..].find('>') else {
        return source;
    };
    let tag = &text[start..start + length];
    if tag.contains("viewBox") {
        return source;
    }
    let number = |name: &str| -> Option<f32> {
        let at = tag.find(&format!("{name}=\""))? + name.len() + 2;
        let value = tag[at..].split('"').next()?;
        value.trim_end_matches("px").parse().ok()
    };
    let (Some(width), Some(height)) = (number("width"), number("height")) else {
        return source;
    };
    let mut patched = text.to_string();
    patched.insert_str(start + 4, &format!(" viewBox=\"0 0 {width} {height}\""));
    patched.into_bytes()
}

/// One size directory of a theme, as `index.theme` describes it.
#[derive(Clone, Debug)]
struct SizeDir {
    path: PathBuf,
    size: u32,
    min: u32,
    max: u32,
    threshold: u32,
    kind: Kind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Fixed,
    Scalable,
    Threshold,
}

impl SizeDir {
    /// The distance between this directory and `size`, zero when it fits.
    fn distance(&self, size: u32) -> u32 {
        match self.kind {
            Kind::Fixed => self.size.abs_diff(size),
            Kind::Scalable => {
                if size < self.min {
                    self.min - size
                } else {
                    size.saturating_sub(self.max)
                }
            }
            Kind::Threshold => {
                let low = self.size.saturating_sub(self.threshold);
                let high = self.size + self.threshold;
                if size < low {
                    low - size
                } else {
                    size.saturating_sub(high)
                }
            }
        }
    }
}

/// A theme: the directories that hold icons and the themes it falls back to.
#[derive(Clone, Debug, Default)]
struct Theme {
    dirs: Vec<SizeDir>,
    inherits: Vec<String>,
}

/// Directories that hold `icons/<theme>` folders, most specific first.
fn icon_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if let Some(home) = &home {
        roots.push(home.join(".icons"));
    }
    match std::env::var_os("XDG_DATA_HOME") {
        Some(dir) if !dir.is_empty() => roots.push(PathBuf::from(dir).join("icons")),
        _ => {
            if let Some(home) = &home {
                roots.push(home.join(".local/share/icons"));
            }
        }
    }
    let dirs = std::env::var("XDG_DATA_DIRS").unwrap_or_default();
    let dirs = if dirs.is_empty() {
        "/usr/local/share:/usr/share".to_string()
    } else {
        dirs
    };
    roots.extend(
        dirs.split(':')
            .filter(|dir| !dir.is_empty())
            .map(|dir| Path::new(dir).join("icons")),
    );
    roots
}

/// A theme's size directories and parents. The `index.theme` in the first
/// root that has one describes the layout; every root is then searched for
/// those directories, so icons a user adds to `~/.local/share/icons/hicolor`
/// are found although that folder has no index of its own.
fn parse_theme(name: &str, roots: &[PathBuf]) -> Theme {
    let mut theme = Theme::default();
    let Some(text) = roots
        .iter()
        .find_map(|root| crate::runtime::read_to_string(root.join(name).join("index.theme")).ok())
    else {
        return theme;
    };
    let mut sections: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut current = String::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(header) = line
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            current = header.to_string();
        } else if let Some((key, value)) = line.split_once('=') {
            sections
                .entry(current.clone())
                .or_default()
                .insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    let Some(main) = sections.get("Icon Theme") else {
        return theme;
    };
    theme.inherits = main
        .get("Inherits")
        .map(|list| {
            list.split(',')
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty())
                .collect()
        })
        .unwrap_or_default();
    for dir in main
        .get("Directories")
        .map(String::as_str)
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|dir| !dir.is_empty())
    {
        let Some(section) = sections.get(dir) else {
            continue;
        };
        let number = |key: &str| section.get(key).and_then(|value| value.parse::<u32>().ok());
        // Skip HiDPI directories: the lookup asks for pixels, not scale.
        if number("Scale").unwrap_or(1) != 1 {
            continue;
        }
        let Some(size) = number("Size") else { continue };
        let kind = match section.get("Type").map(String::as_str) {
            Some("Fixed") => Kind::Fixed,
            Some("Scalable") => Kind::Scalable,
            _ => Kind::Threshold,
        };
        for root in roots {
            theme.dirs.push(SizeDir {
                path: root.join(name).join(dir),
                size,
                min: number("MinSize").unwrap_or(size),
                max: number("MaxSize").unwrap_or(size),
                threshold: number("Threshold").unwrap_or(2),
                kind,
            });
        }
    }
    theme
}

/// The icon theme GTK applications use, from GSettings; Adwaita otherwise.
fn current_theme() -> String {
    static NAME: LazyLock<String> = LazyLock::new(|| {
        crate::runtime::assert_off_ui_thread("gsettings icon-theme");
        std::process::Command::new("gsettings")
            .args(["get", "org.gnome.desktop.interface", "icon-theme"])
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .map(|text| text.trim().trim_matches('\'').to_string())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "Adwaita".to_string())
    });
    NAME.clone()
}

/// The theme search order: the current theme, what it inherits, `hicolor`.
fn theme_chain(roots: &[PathBuf]) -> Vec<(String, Theme)> {
    let mut chain: Vec<(String, Theme)> = Vec::new();
    let mut queue = vec![current_theme()];
    while let Some(name) = (!queue.is_empty()).then(|| queue.remove(0)) {
        if chain.iter().any(|(seen, _)| *seen == name) {
            continue;
        }
        let theme = parse_theme(&name, roots);
        queue.extend(theme.inherits.clone());
        chain.push((name, theme));
    }
    if !chain.iter().any(|(name, _)| name == "hicolor") {
        chain.push(("hicolor".to_string(), parse_theme("hicolor", roots)));
    }
    chain
}

fn file_in(dir: &Path, name: &str) -> Option<IconFile> {
    for (extension, vector) in [("svg", true), ("png", false)] {
        let path = dir.join(format!("{name}.{extension}"));
        if path.is_file() {
            return IconFile::new(path, vector);
        }
    }
    None
}

fn find(name: &str, size: u32) -> Option<IconFile> {
    if name.starts_with('/') {
        let path = PathBuf::from(name);
        // An AppImage icon often has no extension: look at the first bytes.
        let vector = match path.extension() {
            Some(ext) => ext == "svg",
            None => crate::runtime::read_file(&path)
                .map(|bytes| bytes.starts_with(b"<?xml") || bytes.starts_with(b"<svg"))
                .unwrap_or(false),
        };
        return if path.is_file() {
            IconFile::new(path, vector)
        } else {
            None
        };
    }
    let roots = icon_roots();
    for (_, theme) in theme_chain(&roots) {
        let mut best: Option<(u32, IconFile)> = None;
        for dir in &theme.dirs {
            let distance = dir.distance(size);
            if best.as_ref().is_some_and(|(found, _)| *found <= distance) {
                continue;
            }
            if let Some(file) = file_in(&dir.path, name) {
                best = Some((distance, file));
            }
        }
        if let Some((_, file)) = best {
            return Some(file);
        }
    }
    let pixmaps = Path::new("/usr/share/pixmaps");
    file_in(pixmaps, name)
}

type Cache = Mutex<HashMap<(String, u32), Option<IconFile>>>;
static CACHE: LazyLock<Cache> = LazyLock::new(Default::default);

/// The cached lookup for icon `name` at `size`, without scanning: `None`
/// when it has not been looked up yet. Cheap enough for render.
fn cached(name: &str, size: u32) -> Option<Option<IconFile>> {
    CACHE.lock().ok()?.get(&(name.to_string(), size)).cloned()
}

/// Find the file for icon `name` at about `size` physical pixels.
pub fn resolve(name: &str, size: u32) -> Option<IconFile> {
    if let Some(found) = cached(name, size) {
        return found;
    }
    let found = find(name, size);
    if let Ok(mut cache) = CACHE.lock() {
        cache.insert((name.to_string(), size), found.clone());
    }
    found
}

/// The file for icon `name` at `size`. A cached result is used at once; an
/// uncached one is looked up on a background thread, and the result is `None`
/// while it runs. `Some(file)` holds `None` when the theme has no such icon.
/// The lookup scans icon directories, so it must never run in render.
fn use_resolved(name: &str, size: u32) -> Option<Option<IconFile>> {
    let key = (name.to_string(), size);
    let found = use_state::<Option<((String, u32), Option<IconFile>)>>(|| None);
    use_side_effect_with_deps(&key, move |key: &(String, u32)| {
        if cached(&key.0, key.1).is_some() {
            return;
        }
        let mut found = found;
        let looked_up = key.clone();
        let done = key.clone();
        background_then(
            move || resolve(&looked_up.0, looked_up.1),
            move |file| found.set(Some((done, file))),
        );
    });
    if let Some(file) = cached(name, size) {
        return Some(file);
    }
    let current = found.read();
    match &*current {
        Some((looked_up, file)) if *looked_up == key => Some(file.clone()),
        _ => None,
    }
}

/// An application icon of `size` for the icon name `name`.
pub fn app_icon(name: impl Into<String>, size: IconSize) -> AppIcon {
    AppIcon {
        name: name.into(),
        size,
    }
}

#[derive(PartialEq)]
pub struct AppIcon {
    name: String,
    size: IconSize,
}

impl Component for AppIcon {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let size = self.size.px();
        // Ask for twice the size so the icon stays sharp on a HiDPI output.
        let found = use_resolved(&self.name, (size * 2.0).ceil() as u32);
        match found {
            Some(Some(IconFile {
                path,
                svg: Some(bytes),
            })) => SvgViewer::new((path, bytes))
                .width(Size::px(size))
                .height(Size::px(size))
                .into_element(),
            Some(Some(IconFile { path, svg: None })) => ImageViewer::new(path)
                .width(Size::px(size))
                .height(Size::px(size))
                .into_element(),
            // The lookup is still running: an empty tile of the same size, so
            // the row does not jump when the icon arrives.
            None => rect()
                .width(Size::px(size))
                .height(Size::px(size))
                .tw(
                    &tokens,
                    &[
                        bg(Paint::Control),
                        rounded(if self.size == IconSize::AppLarge {
                            Radius::Card
                        } else {
                            Radius::Control
                        }),
                    ],
                )
                .into_element(),
            // No icon in the theme: a plain tile with the generic glyph.
            Some(None) => rect()
                .width(Size::px(size))
                .height(Size::px(size))
                .tw(
                    &tokens,
                    &[
                        bg(Paint::Control),
                        rounded(if self.size == IconSize::AppLarge {
                            Radius::Card
                        } else {
                            Radius::Control
                        }),
                        center(),
                    ],
                )
                .child(icons::icon(
                    Icon::Application,
                    if self.size == IconSize::AppLarge {
                        IconSize::Hero
                    } else {
                        IconSize::Control
                    },
                    Paint::Muted.color(&tokens),
                ))
                .into_element(),
        }
    }
}
