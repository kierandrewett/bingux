//! Bingux patch: how long the window spends on a frame, for
//! `BINGUX_FRAME_STATS` (read by bingux-ui's `frame_view::stats`).
//!
//! Off until [`enable`] is called: each probe is then one relaxed load.
//!
//! * `layout`: measuring the node tree, when a frame needed it.
//! * `render`: the render pipeline over the whole tree: recording the draw of
//!   every node (the frame view's image included) into the canvas.
//! * `present`: the driver's whole present: `render`, the flush that rasterises
//!   and uploads, and the swap. With the swap waiting for vertical blank this
//!   sits near the refresh interval whatever the work is.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

static ON: AtomicBool = AtomicBool::new(false);

#[derive(Default)]
struct Samples {
    layout: Vec<f64>,
    render: Vec<f64>,
    present: Vec<f64>,
}

static SAMPLES: Mutex<Samples> = Mutex::new(Samples {
    layout: Vec::new(),
    render: Vec::new(),
    present: Vec::new(),
});

/// Which stage a sample is of.
#[derive(Clone, Copy)]
pub enum Stage {
    Layout,
    Render,
    Present,
}

/// Starts recording.
pub fn enable() {
    ON.store(true, Ordering::Relaxed);
}

/// Whether to time at all.
pub fn enabled() -> bool {
    ON.load(Ordering::Relaxed)
}

/// Records one stage of a frame.
pub fn record(stage: Stage, took: Duration) {
    if !enabled() {
        return;
    }
    let ms = took.as_secs_f64() * 1000.0;
    let mut samples = SAMPLES.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    match stage {
        Stage::Layout => samples.layout.push(ms),
        Stage::Render => samples.render.push(ms),
        Stage::Present => samples.present.push(ms),
    }
}

/// The samples recorded since the last call, in milliseconds: layout, render, present.
pub fn take() -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let mut samples = SAMPLES.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    (
        std::mem::take(&mut samples.layout),
        std::mem::take(&mut samples.render),
        std::mem::take(&mut samples.present),
    )
}
