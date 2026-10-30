//! The size of the window, kept between runs in the same state file as the
//! views of each folder (`bingux_ui::files::remember`).

use bingux_ui::files;
use bingux_ui::theme::WINDOW_SIZE;

const KEY: &str = "files-window";

/// The smallest size worth coming back to; a phone-sized window is never kept.
const LEAST: [f32; 2] = [640.0, 420.0];

/// The last size, or the default one.
pub fn remembered() -> (f32, f32) {
    match files::remembered::<[f32; 2]>(KEY) {
        Some([width, height]) if width >= LEAST[0] && height >= LEAST[1] => (width, height),
        _ => (WINDOW_SIZE.0 as f32, WINDOW_SIZE.1 as f32),
    }
}

/// Keep `width` by `height`. The state file is written half a second after
/// the last change, so dragging a window edge does not write on every frame.
pub fn remember_size(width: f32, height: f32) {
    if width >= LEAST[0] && height >= LEAST[1] {
        files::remember(KEY, &[width.round(), height.round()]);
    }
}
