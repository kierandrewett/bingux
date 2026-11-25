//! Frame timing, printed once a second to stderr when `BINGUX_FRAME_STATS` is
//! set to anything but `0`.
//!
//! What it measures, all in milliseconds:
//!
//! * `arrival`: the time between two frames reaching the producer's mailbox
//!   (recorded by the producer with [`arrived`]); `dropped` is how many the
//!   view never took because a newer one replaced them.
//! * `paint`: the UI thread's cost of painting the view once: recording the
//!   frame's draw into the canvas. The rasteriser and the GPU upload run after
//!   it, so this is the part the view itself can influence, not the whole
//!   frame.
//! * `shown`: the time between two frames the view put on screen.
//! * `input->frame`: from the last input event the view forwarded to the next
//!   frame that reached the screen.
//! * `layout`, `render` and `present`: the window's own frame, from the vendored
//!   winit driver (`freya_winit::bingux_frame`): measuring the tree, recording the
//!   draw of the whole tree, and the driver's whole present (render, the flush
//!   that rasterises and uploads, the swap). A `present` that sits near the
//!   refresh interval whatever `render` costs is the swap waiting for vertical
//!   blank.
//!
//! Off, every call is one relaxed load. The lines look like
//! `[frame-stats] presented 58/s | arrival 16.7 mean 18.2 p95 (dropped 2) | ...`.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

/// How often a line is printed.
const EVERY: Duration = Duration::from_secs(1);

#[derive(Default)]
struct Series(Vec<f64>);

impl Series {
    fn summary(&mut self) -> String {
        let samples = std::mem::take(&mut self.0);
        if samples.is_empty() {
            return "-".to_string();
        }
        let mut sorted = samples;
        sorted.sort_by(f64::total_cmp);
        let mean = sorted.iter().sum::<f64>() / sorted.len() as f64;
        let p95 = sorted[((sorted.len() as f64 * 0.95) as usize).min(sorted.len() - 1)];
        format!("{mean:.1} mean {p95:.1} p95 {:.1} max", sorted[sorted.len() - 1])
    }
}

struct State {
    layout: Series,
    render: Series,
    present: Series,
    arrival: Series,
    paint: Series,
    shown: Series,
    latency: Series,
    last_arrival: Option<Instant>,
    last_shown: Option<Instant>,
    input_at: Option<Instant>,
    printed: Instant,
    presented: u64,
}

struct Stats {
    state: Mutex<State>,
    dropped: AtomicU64,
}

fn enabled() -> bool {
    static ON: AtomicBool = AtomicBool::new(false);
    static READ: OnceLock<()> = OnceLock::new();
    READ.get_or_init(|| {
        let on = std::env::var("BINGUX_FRAME_STATS").is_ok_and(|value| !value.is_empty() && value != "0");
        ON.store(on, Ordering::Relaxed);
        if on {
            freya_winit::bingux_frame::enable();
        }
    });
    ON.load(Ordering::Relaxed)
}

fn stats() -> &'static Stats {
    static STATS: OnceLock<Stats> = OnceLock::new();
    STATS.get_or_init(|| Stats {
        state: Mutex::new(State {
            layout: Series::default(),
            render: Series::default(),
            present: Series::default(),
            arrival: Series::default(),
            paint: Series::default(),
            shown: Series::default(),
            latency: Series::default(),
            last_arrival: None,
            last_shown: None,
            input_at: None,
            printed: Instant::now(),
            presented: 0,
        }),
        dropped: AtomicU64::new(0),
    })
}

fn with(change: impl FnOnce(&mut State)) {
    let mut state = stats().state.lock().unwrap_or_else(PoisonError::into_inner);
    change(&mut state);
}

fn ms(span: Duration) -> f64 {
    span.as_secs_f64() * 1000.0
}

/// A producer's frame reached its mailbox.
pub fn arrived() {
    if !enabled() {
        return;
    }
    let now = Instant::now();
    with(|state| {
        if let Some(last) = state.last_arrival.replace(now) {
            state.arrival.0.push(ms(now - last));
        }
    });
}

/// A frame was replaced in the mailbox before the view took it.
pub fn dropped() {
    if enabled() {
        stats().dropped.fetch_add(1, Ordering::Relaxed);
    }
}

/// The view forwarded an input event to the producer.
pub(crate) fn input() {
    if !enabled() {
        return;
    }
    with(|state| {
        state.input_at.get_or_insert_with(Instant::now);
    });
}

/// The view painted once, in `took`.
pub(crate) fn painted(took: Duration) {
    if enabled() {
        with(|state| state.paint.0.push(ms(took)));
    }
}

/// Whether a paint is worth timing at all.
pub(crate) fn timing() -> bool {
    enabled()
}

/// A frame reached the screen; prints a line when a second has passed.
pub(crate) fn shown() {
    if !enabled() {
        return;
    }
    let now = Instant::now();
    with(|state| {
        state.presented += 1;
        if let Some(last) = state.last_shown.replace(now) {
            state.shown.0.push(ms(now - last));
        }
        if let Some(at) = state.input_at.take() {
            state.latency.0.push(ms(now - at));
        }
        let span = now - state.printed;
        if span >= EVERY {
            let (layout, render, present) = freya_winit::bingux_frame::take();
            state.layout.0.extend(layout);
            state.render.0.extend(render);
            state.present.0.extend(present);
            let per_second = state.presented as f64 / span.as_secs_f64();
            eprintln!(
                "[frame-stats] presented {per_second:.0}/s | arrival {} (dropped {}) | shown {} | paint {} | input->frame {} | layout {} | render {} | present {}",
                state.arrival.summary(),
                stats().dropped.swap(0, Ordering::Relaxed),
                state.shown.summary(),
                state.paint.summary(),
                state.latency.summary(),
                state.layout.summary(),
                state.render.summary(),
                state.present.summary(),
            );
            state.printed = now;
            state.presented = 0;
        }
    });
}
