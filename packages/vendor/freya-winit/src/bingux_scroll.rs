//! Bingux patch: scroll speed for freya-winit.
//!
//! Upstream hard-codes 53 px per wheel notch and doubles touchpad movement,
//! which feels fast and jumpy on a desktop. These defaults suit a mouse wheel
//! and a touchpad; both can be changed with environment variables.

use std::sync::OnceLock;

fn read(name: &str, default: f64, min: f64, max: f64) -> f64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite())
        .map_or(default, |value| value.clamp(min, max))
}

/// Pixels per wheel notch (`BINGUX_WHEEL_STEP`, default 64).
pub(crate) fn wheel_step() -> f64 {
    static VALUE: OnceLock<f64> = OnceLock::new();
    *VALUE.get_or_init(|| read("BINGUX_WHEEL_STEP", 64.0, 8.0, 400.0))
}

/// Touchpad movement factor (`BINGUX_TOUCHPAD_FACTOR`, default 1).
pub(crate) fn touchpad_factor() -> f64 {
    static VALUE: OnceLock<f64> = OnceLock::new();
    *VALUE.get_or_init(|| read("BINGUX_TOUCHPAD_FACTOR", 1.0, 0.1, 8.0))
}
