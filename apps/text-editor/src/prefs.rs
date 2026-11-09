//! The editor's preferences, kept in `$XDG_CONFIG_HOME/bingux/text-editor.json`.
//! A missing or unreadable file is the defaults. A file's `.editorconfig`
//! wins over the indentation set here.

use std::path::PathBuf;

use bingux_text::Indent;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub line_numbers: bool,
    pub wrap: bool,
    pub show_whitespace: bool,
    pub highlight_line: bool,
    pub auto_close: bool,
    pub trim_on_save: bool,
    pub final_newline: bool,
    pub tabs: bool,
    pub indent_width: u8,
    pub tab_width: usize,
    pub zoom: f32,
    pub markdown_preview: bool,
    pub window: Option<(f32, f32)>,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            line_numbers: true,
            wrap: true,
            show_whitespace: false,
            highlight_line: true,
            auto_close: true,
            trim_on_save: false,
            final_newline: true,
            tabs: false,
            indent_width: 4,
            tab_width: 4,
            zoom: 1.0,
            markdown_preview: true,
            window: None,
        }
    }
}

/// Zoom steps, as a person expects from a browser.
pub const ZOOMS: [f32; 11] = [0.5, 0.67, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0];

impl Prefs {
    pub fn indent(&self) -> Indent {
        if self.tabs {
            Indent::Tabs
        } else {
            Indent::Spaces(self.indent_width.clamp(1, 16))
        }
    }

    pub fn zoom_step(&self, step: bingux_ui::text_editor::ZoomStep) -> f32 {
        use bingux_ui::text_editor::ZoomStep;
        let index = ZOOMS
            .iter()
            .position(|zoom| (*zoom - self.zoom).abs() < 0.01)
            .unwrap_or(5);
        match step {
            ZoomStep::In => ZOOMS[(index + 1).min(ZOOMS.len() - 1)],
            ZoomStep::Out => ZOOMS[index.saturating_sub(1)],
            ZoomStep::Reset => 1.0,
        }
    }
}

fn path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home().join(".config"));
    base.join("bingux").join("text-editor.json")
}

pub fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// Reads the preferences. One small file: allowed before the first frame.
pub fn load() -> Prefs {
    std::fs::read(path())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Writes the preferences on a worker thread.
pub fn save(prefs: &Prefs) {
    let Ok(text) = serde_json::to_vec_pretty(prefs) else {
        return;
    };
    bingux_ui::task::spawn_detached("text-editor-prefs", move || {
        let path = path();
        if let Some(folder) = path.parent() {
            let _ = std::fs::create_dir_all(folder);
        }
        let temp = path.with_extension("json.part");
        if std::fs::write(&temp, text).is_ok() {
            let _ = std::fs::rename(&temp, &path);
        }
    });
}
