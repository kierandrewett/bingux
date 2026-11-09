//! What Preview remembers between runs, in the state file Files keeps its
//! views in (`bingux_ui::files::remember`): how each kind of picture was last
//! wanted (fitted, or at actual size) and the size of the window.

use std::collections::BTreeMap;

use bingux_ui::files;
use bingux_ui::theme::WINDOW_SIZE;
use bingux_ui::zoom_view::FitMode;

const ZOOM_KEY: &str = "preview-zoom";
const WINDOW_KEY: &str = "preview-window";

/// The smallest window size worth coming back to; a phone-sized window is never kept.
const LEAST: [f32; 2] = [480.0, 360.0];
/// The most kinds of picture remembered.
const MOST_KINDS: usize = 64;

/// How pictures of `mime` were last wanted, when the person ever chose.
pub fn zoom_for(mime: &str) -> Option<FitMode> {
    let kinds = files::remembered::<BTreeMap<String, String>>(ZOOM_KEY)?;
    match kinds.get(mime)?.as_str() {
        "actual" => Some(FitMode::Actual),
        "fit" => Some(FitMode::Fit),
        _ => None,
    }
}

/// Keep how pictures of `mime` are wanted. The state file is written half a
/// second after the last change.
pub fn remember_zoom(mime: &str, mode: FitMode) {
    let mut kinds = files::remembered::<BTreeMap<String, String>>(ZOOM_KEY).unwrap_or_default();
    let word = match mode {
        FitMode::Fit => "fit",
        FitMode::Actual => "actual",
    };
    if kinds.get(mime).map(String::as_str) == Some(word) {
        return;
    }
    if kinds.len() >= MOST_KINDS && !kinds.contains_key(mime) {
        return;
    }
    kinds.insert(mime.to_owned(), word.to_owned());
    files::remember(ZOOM_KEY, &kinds);
}

/// The last size, or the default one.
pub fn window_size() -> (f32, f32) {
    match files::remembered::<[f32; 2]>(WINDOW_KEY) {
        Some([width, height]) if width >= LEAST[0] && height >= LEAST[1] => (width, height),
        _ => (WINDOW_SIZE.0 as f32, WINDOW_SIZE.1 as f32),
    }
}

/// Keep `width` by `height`.
pub fn remember_window_size(width: f32, height: f32) {
    if width >= LEAST[0] && height >= LEAST[1] {
        files::remember(WINDOW_KEY, &[width.round(), height.round()]);
    }
}
