//! Page zoom. The steps are Firefox's, which are the ones the engine's own
//! Ctrl+wheel uses (`docs/input.md`), so the keyboard and the wheel agree.

/// The zoom levels, in percent.
pub const LEVELS: [u32; 16] = [
    30, 50, 67, 80, 90, 100, 110, 120, 133, 150, 170, 200, 240, 300, 400, 500,
];

/// 100 %.
pub const DEFAULT: f64 = 1.0;

/// `zoom` as a whole percentage.
pub fn percent(zoom: f64) -> u32 {
    (zoom * 100.0).round().max(0.0) as u32
}

/// The next level above `zoom`, or `zoom` at the top.
pub fn zoom_in(zoom: f64) -> f64 {
    let now = percent(zoom);
    LEVELS
        .iter()
        .find(|level| **level > now)
        .map_or(zoom, |level| f64::from(*level) / 100.0)
}

/// The next level below `zoom`, or `zoom` at the bottom.
pub fn zoom_out(zoom: f64) -> f64 {
    let now = percent(zoom);
    LEVELS
        .iter()
        .rev()
        .find(|level| **level < now)
        .map_or(zoom, |level| f64::from(*level) / 100.0)
}

/// Whether `zoom` is the default, to a percent.
pub fn is_default(zoom: f64) -> bool {
    percent(zoom) == 100
}

#[cfg(test)]
mod tests;
