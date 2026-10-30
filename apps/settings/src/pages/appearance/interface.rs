//! The GNOME interface keys that Appearance shows beside the wallpaper and
//! the font: the text size, the icon theme and the cursor theme.
//!
//! They live in `org.gnome.desktop.interface`, so GTK applications, the portal
//! and the compositor all see a change. Each control shows its new value at
//! once, the write runs in the background, and a failed write puts the old
//! value back with a toast (see `appearance::change`). The two theme lists are
//! read from the icon directories, so only themes that are installed are
//! offered.

use std::path::{Path, PathBuf};

use crate::system::{self, CommandError};
use crate::ui::{Group, dropdown, row};
use bingux_ui::style::*;
use freya::prelude::*;

const INTERFACE: &str = "org.gnome.desktop.interface";
/// Text sizes offered, in percent. The current one is added when it is not
/// among them, so a value set elsewhere still shows.
const TEXT_SIZES: [u32; 8] = [80, 90, 100, 110, 125, 150, 175, 200];

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Theme {
    /// The directory name, which is what the key stores.
    id: String,
    /// `Name` from `index.theme`, or the directory name.
    name: String,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Interface {
    text_scale: f64,
    cursor_theme: String,
    icon_theme: String,
    cursor_themes: Vec<Theme>,
    icon_themes: Vec<Theme>,
}

/// One change to the interface keys.
#[derive(Clone, Debug, PartialEq)]
pub enum Setting {
    TextScale(f64),
    CursorTheme(String),
    IconTheme(String),
}

impl Interface {
    /// Reads every key and the installed themes. `None` without gsettings or
    /// when a key cannot be read.
    pub fn read() -> Option<Self> {
        let text = |schema: &str, key: &str| system::gsettings_get(schema, key).ok();
        Some(Self {
            text_scale: text(INTERFACE, "text-scaling-factor")?
                .trim()
                .parse()
                .ok()?,
            cursor_theme: system::unquote(&text(INTERFACE, "cursor-theme")?),
            icon_theme: system::unquote(&text(INTERFACE, "icon-theme")?),
            cursor_themes: themes(ThemeKind::Cursor),
            icon_themes: themes(ThemeKind::Icon),
        })
    }

    /// Shows `setting` before the system has confirmed it.
    pub fn apply_local(&mut self, setting: &Setting) {
        match setting {
            Setting::TextScale(scale) => self.text_scale = *scale,
            Setting::CursorTheme(id) => self.cursor_theme = id.clone(),
            Setting::IconTheme(id) => self.icon_theme = id.clone(),
        }
    }
}

impl Setting {
    /// Writes the key.
    pub fn write(&self) -> Result<(), CommandError> {
        let (schema, key, value) = match self {
            Setting::TextScale(scale) => (INTERFACE, "text-scaling-factor", format!("{scale:.2}")),
            Setting::CursorTheme(id) => (INTERFACE, "cursor-theme", quoted(id)),
            Setting::IconTheme(id) => (INTERFACE, "icon-theme", quoted(id)),
        };
        system::gsettings_set(schema, key, &value).map(|_| ())
    }
}

/// A GVariant string. Names with a quote are never offered, so this does not
/// need to escape.
fn quoted(text: &str) -> String {
    format!("'{text}'")
}

#[derive(Clone, Copy, PartialEq)]
enum ThemeKind {
    Icon,
    Cursor,
}

/// The directories GTK looks in for icon and cursor themes.
fn theme_roots() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| home.as_ref().map(|home| home.join(".local/share")));
    let data_dirs = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|dirs| !dirs.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".to_string());
    let mut roots: Vec<PathBuf> = Vec::new();
    roots.extend(home.map(|home| home.join(".icons")));
    roots.extend(data_home.map(|dir| dir.join("icons")));
    roots.extend(data_dirs.split(':').map(|dir| Path::new(dir).join("icons")));
    roots
}

/// A value from the `[Icon Theme]` section of an `index.theme`.
fn index_value(index: &str, key: &str) -> Option<String> {
    let mut in_section = false;
    for line in index.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_section = line == "[Icon Theme]";
        } else if in_section
            && let Some(value) = line
                .strip_prefix(key)
                .and_then(|rest| rest.strip_prefix('='))
        {
            return Some(value.trim().to_string());
        }
    }
    None
}

fn themes(kind: ThemeKind) -> Vec<Theme> {
    let mut found: Vec<Theme> = Vec::new();
    for root in theme_roots() {
        let Ok(entries) = crate::system::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let id = entry.file_name().to_string_lossy().into_owned();
            // hicolor and default are fallbacks, not choices.
            if id.contains('\'') || id == "hicolor" || id == "default" || !path.is_dir() {
                continue;
            }
            let index = crate::system::read_to_string(path.join("index.theme")).unwrap_or_default();
            let wanted = match kind {
                ThemeKind::Cursor => path.join("cursors").is_dir(),
                ThemeKind::Icon => {
                    // Adwaita is marked Hidden in its index, but it is the
                    // default icon theme, so it must stay offered.
                    index_value(&index, "Directories").is_some()
                        && (id == "Adwaita"
                            || index_value(&index, "Hidden").as_deref() != Some("true"))
                }
            };
            if !wanted || found.iter().any(|theme| theme.id == id) {
                continue;
            }
            let name = index_value(&index, "Name").unwrap_or_else(|| id.clone());
            found.push(Theme { id, name });
        }
    }
    found.sort_by_key(|theme| theme.name.to_lowercase());
    found
}

/// Options for a theme dropdown. The current theme is kept when it is not
/// installed, so the control never shows a blank.
fn theme_options(themes: &[Theme], current: &str) -> Vec<(String, String)> {
    let mut options: Vec<(String, String)> = themes
        .iter()
        .map(|theme| (theme.id.clone(), theme.name.clone()))
        .collect();
    if !current.is_empty() && !options.iter().any(|(id, _)| id == current) {
        options.push((current.to_string(), current.to_string()));
    }
    options
}

fn text_size_options(current: u32) -> Vec<(String, String)> {
    let mut sizes = TEXT_SIZES.to_vec();
    if !sizes.contains(&current) {
        sizes.push(current);
        sizes.sort_unstable();
    }
    sizes
        .into_iter()
        .map(|percent| (percent.to_string(), format!("{percent}%")))
        .collect()
}

/// The text scale as a whole percent, for the dropdown's key.
fn percent(scale: f64) -> u32 {
    (scale * 100.0).round() as u32
}

/// The "Text size" row for the Font group.
pub fn text_size_row(
    interface: &Interface,
    on_change: impl Into<EventHandler<Setting>>,
) -> Element {
    let on_change: EventHandler<Setting> = on_change.into();
    let current = percent(interface.text_scale);
    row("Text size")
        .trailing(
            dropdown(
                text_size_options(current),
                current.to_string(),
                move |key: String| {
                    if let Ok(percent) = key.parse::<u32>() {
                        on_change.call(Setting::TextScale(f64::from(percent) / 100.0));
                    }
                },
            )
            .min_width(Dim::Field),
        )
        .into()
}

/// "Themes": the icon theme and the cursor theme.
pub fn theme_group(interface: &Interface, on_change: impl Into<EventHandler<Setting>>) -> Group {
    let on_change: EventHandler<Setting> = on_change.into();
    let icons = theme_options(&interface.icon_themes, &interface.icon_theme);
    let cursors = theme_options(&interface.cursor_themes, &interface.cursor_theme);
    let on_icons = on_change.clone();
    let on_cursors = on_change;
    Group::titled("Themes")
        .row(
            row("Icons").trailing(
                dropdown(icons, interface.icon_theme.clone(), move |id: String| {
                    on_icons.call(Setting::IconTheme(id))
                })
                .min_width(Dim::Field),
            ),
        )
        .row(
            row("Cursor").trailing(
                dropdown(
                    cursors,
                    interface.cursor_theme.clone(),
                    move |id: String| on_cursors.call(Setting::CursorTheme(id)),
                )
                .min_width(Dim::Field),
            ),
        )
}
