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

/// One wheel event as winit delivered it, before Freya's own handling (which
/// drops every phase but `Moved`, scales a line by [`wheel_step`] and a pixel
/// delta by [`touchpad_factor`]). The scroll probe reads these.
#[derive(Clone, Debug, PartialEq)]
pub struct RawWheel {
    pub at: std::time::Instant,
    /// `"line"` (detents) or `"pixel"` (touchpad, free-spin).
    pub kind: &'static str,
    pub x: f64,
    pub y: f64,
    /// winit's phase: `started`, `moved`, `ended` or `cancelled`.
    pub phase: &'static str,
    /// The window's scale factor. A pixel delta is physical pixels.
    pub scale: f64,
}

/// The most events held for [`drain_tap`]; the oldest go first.
const TAP_CAPACITY: usize = 4096;

static TAP_ON: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static TAP_QUEUE: std::sync::Mutex<std::collections::VecDeque<RawWheel>> =
    std::sync::Mutex::new(std::collections::VecDeque::new());
/// `BINGUX_SCROLL_LOG=path`: every raw wheel event, one JSON object per line.
static TAP_LOG: std::sync::LazyLock<Option<std::sync::Mutex<std::fs::File>>> =
    std::sync::LazyLock::new(|| {
        let path = std::env::var_os("BINGUX_SCROLL_LOG")?;
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .ok()
            .map(std::sync::Mutex::new)
    });
static TAP_START: std::sync::LazyLock<std::time::Instant> =
    std::sync::LazyLock::new(std::time::Instant::now);

/// Starts recording raw wheel events for [`drain_tap`].
pub fn enable_tap() {
    std::sync::LazyLock::force(&TAP_START);
    TAP_ON.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Takes the raw wheel events recorded since the last call.
pub fn drain_tap() -> Vec<RawWheel> {
    let mut queue = TAP_QUEUE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    queue.drain(..).collect()
}

pub(crate) fn tap(delta: winit::event::MouseScrollDelta, phase: winit::event::TouchPhase, scale: f64) {
    use std::io::Write as _;
    use std::sync::atomic::Ordering;
    use winit::event::{MouseScrollDelta, TouchPhase};

    let log = TAP_LOG.as_ref();
    if log.is_none() && !TAP_ON.load(Ordering::Relaxed) {
        return;
    }
    let (kind, x, y) = match delta {
        MouseScrollDelta::LineDelta(x, y) => ("line", f64::from(x), f64::from(y)),
        MouseScrollDelta::PixelDelta(position) => ("pixel", position.x, position.y),
    };
    let phase = match phase {
        TouchPhase::Started => "started",
        TouchPhase::Moved => "moved",
        TouchPhase::Ended => "ended",
        TouchPhase::Cancelled => "cancelled",
    };
    let event = RawWheel {
        at: std::time::Instant::now(),
        kind,
        x,
        y,
        phase,
        scale,
    };
    if let Some(log) = log {
        let ms = event.at.saturating_duration_since(*TAP_START).as_secs_f64() * 1000.0;
        let line = format!(
            "{{\"type\":\"winit\",\"ms\":{ms:.3},\"kind\":\"{kind}\",\"x\":{x},\"y\":{y},\"phase\":\"{phase}\",\"scale\":{scale}}}\n"
        );
        let mut file = log.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let _ = file.write_all(line.as_bytes());
    }
    if TAP_ON.load(Ordering::Relaxed) {
        let mut queue = TAP_QUEUE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if queue.len() >= TAP_CAPACITY {
            queue.pop_front();
        }
        queue.push_back(event);
    }
}
