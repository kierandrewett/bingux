//! Bingux patch: mouse wheel acceleration, and the switch between wheel feels.
//!
//! A ratcheted wheel (a Logitech MX Master in ratchet mode, most office mice)
//! sends one event per detent. Neither the kernel nor a Wayland compositor
//! accelerates those events, so an application that wants the quick scrolling
//! of Firefox or GTK's kinetic wheel has to derive it from the event stream.
//! [`step`] does that, as a pure function:
//!
//! * A wheel event is a number of detents (`1.0` per click; a hi-res wheel
//!   reports fractions, `value120 / 120`). Its distance is
//!   `detents * px_per_detent * gain`.
//! * The gain comes from the detent *rate* over a sliding window of the last
//!   [`WINDOW`]: the detents after the window's first event, divided by the
//!   time they span. A single click, or clicks more than a few hundred
//!   milliseconds apart, have no rate and a gain of exactly 1, so a precise
//!   click always moves one step. From [`RATE_FROM`] to [`RATE_FULL`] detents
//!   per second the gain rises smoothly (smoothstep) to [`GAIN_MAX`], and
//!   it builds over the first three intervals of a burst.
//! * The gain attacks at once and releases exponentially (time constant
//!   [`RELEASE_TAU`]); after [`REST`] of stillness it is exactly 1 again.
//! * A change of direction resets the axis: whoever turns the wheel back wants
//!   control, not the momentum of the last flick.
//! * Pixel events (a touchpad, or a wheel in free-spin) are passed through at
//!   1x and reset the line state. A touchpad has kinetic scrolling from the
//!   compositor and a free-spinning wheel has its own flywheel; neither needs
//!   a gain on top.
//! * Each axis has its own state, so a thumb wheel (horizontal) accelerates
//!   on its own. Shift+wheel is swapped to the other axis by the scroll view
//!   before it gets here.
//!
//! [`apply`] is the glue for the scroll views: it picks the feel
//! ([`WheelFeel`], from `BINGUX_WHEEL_FEEL` or [`set_feel`]), runs [`step`] for
//! [`WheelFeel::Accelerated`] and records a [`Trace`] of what it did for the
//! scroll probe (`bingux-ui`'s `scroll_probe` example).

use std::cell::RefCell;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, Instant};

use freya_core::prelude::WheelSource;

/// How far back the detent rate looks.
pub const WINDOW: Duration = Duration::from_millis(150);
/// A gap this long between detents puts the gain back to exactly 1.
pub const REST: Duration = Duration::from_millis(250);
/// Time constant of the gain's release, in seconds: the excess over 1 is 1.5 %
/// of its value after [`REST`].
pub const RELEASE_TAU: f64 = 0.06;
/// The rate (detents per second) below which the gain is 1.
pub const RATE_FROM: f64 = 6.0;
/// The rate at which the gain reaches [`GAIN_MAX`].
pub const RATE_FULL: f64 = 24.0;
/// The most the gain multiplies a detent.
pub const GAIN_MAX: f64 = 4.0;
/// Intervals between detents a burst needs before its gain is the full one
/// for its rate.
const RAMP_INTERVALS: f64 = 3.0;
/// The shortest span a rate is measured over, so two events delivered in the
/// same frame do not read as an infinite rate.
const MIN_SPAN: Duration = Duration::from_millis(40);

/// What kind of wheel event this is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Detents, as a mouse wheel reports them.
    Line,
    /// Pixels, as a touchpad or a free-spinning wheel reports them.
    Pixel,
}

/// One wheel event. For [`Kind::Line`] `dx` and `dy` count detents (a click
/// is 1.0, a hi-res wheel sends fractions); for [`Kind::Pixel`] they are
/// pixels. The sign is the scroll direction, as the scroll view reads it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WheelEvent {
    pub kind: Kind,
    pub dx: f64,
    pub dy: f64,
}

/// What [`step`] decided for one event.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Delta {
    /// The distance to scroll, in logical pixels.
    pub x: f64,
    pub y: f64,
    /// The gain applied to each axis.
    pub gain_x: f64,
    pub gain_y: f64,
    /// The detent rate each axis was measured at, per second.
    pub rate_x: f64,
    pub rate_y: f64,
}

#[derive(Clone, Copy, Debug)]
struct Tick {
    at: Instant,
    detents: f64,
}

#[derive(Clone, Debug)]
struct Axis {
    ticks: Vec<Tick>,
    last: Option<Instant>,
    sign: i8,
    gain: f64,
}

impl Default for Axis {
    fn default() -> Self {
        Self {
            ticks: Vec::new(),
            last: None,
            sign: 0,
            gain: 1.0,
        }
    }
}

impl Axis {
    fn reset(&mut self) {
        self.ticks.clear();
        self.last = None;
        self.sign = 0;
        self.gain = 1.0;
    }

    /// Detents per second over the window, from the events after its first.
    fn rate(&self, now: Instant) -> f64 {
        let Some(first) = self.ticks.first() else {
            return 0.0;
        };
        if self.ticks.len() < 2 {
            return 0.0;
        }
        let detents: f64 = self.ticks[1..].iter().map(|tick| tick.detents).sum();
        let span = now.saturating_duration_since(first.at).max(MIN_SPAN);
        detents / span.as_secs_f64()
    }

    /// Takes `detents` at `now`; returns the detents to move by (gain applied),
    /// the gain and the rate.
    fn advance(&mut self, detents: f64, now: Instant) -> (f64, f64, f64) {
        if detents == 0.0 || !detents.is_finite() {
            return (0.0, 1.0, 0.0);
        }
        let sign = if detents > 0.0 { 1 } else { -1 };
        if self.sign != 0 && self.sign != sign {
            self.reset();
        }
        self.sign = sign;

        let released = match self.last {
            Some(last) => {
                let gap = now.saturating_duration_since(last);
                if gap >= REST {
                    1.0
                } else {
                    1.0 + (self.gain - 1.0) * (-gap.as_secs_f64() / RELEASE_TAU).exp()
                }
            }
            None => 1.0,
        };
        self.ticks
            .retain(|tick| now.saturating_duration_since(tick.at) <= WINDOW);
        self.ticks.push(Tick {
            at: now,
            detents: detents.abs(),
        });
        let rate = self.rate(now);
        let t = ((rate - RATE_FROM) / (RATE_FULL - RATE_FROM)).clamp(0.0, 1.0);
        // The gain builds over the first few detents of a burst rather than
        // jumping on the second.
        let confidence = ((self.ticks.len() - 1) as f64 / RAMP_INTERVALS).min(1.0);
        let wanted = 1.0 + (GAIN_MAX - 1.0) * t * t * (3.0 - 2.0 * t) * confidence;
        let gain = released.max(wanted).clamp(1.0, GAIN_MAX);
        self.gain = gain;
        self.last = Some(now);
        (detents * gain, gain, rate)
    }
}

/// The acceleration state of one scroll view: both axes.
#[derive(Clone, Debug)]
pub struct State {
    x: Axis,
    y: Axis,
    px_per_detent: f64,
}

impl State {
    /// A state for a wheel whose detent is `px_per_detent` logical pixels.
    pub fn new(px_per_detent: f64) -> Self {
        Self {
            x: Axis::default(),
            y: Axis::default(),
            px_per_detent,
        }
    }
}

/// Turns one wheel event into a distance, and the state that follows it.
pub fn step(mut state: State, event: WheelEvent, now: Instant) -> (Delta, State) {
    let delta = match event.kind {
        Kind::Pixel => {
            state.x.reset();
            state.y.reset();
            Delta {
                x: event.dx,
                y: event.dy,
                gain_x: 1.0,
                gain_y: 1.0,
                rate_x: 0.0,
                rate_y: 0.0,
            }
        }
        Kind::Line => {
            let (x, gain_x, rate_x) = state.x.advance(event.dx, now);
            let (y, gain_y, rate_y) = state.y.advance(event.dy, now);
            Delta {
                x: x * state.px_per_detent,
                y: y * state.px_per_detent,
                gain_x,
                gain_y,
                rate_x,
                rate_y,
            }
        }
    };
    (delta, state)
}

/// How a scroll view turns wheel events into movement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WheelFeel {
    /// As shipped: a fixed distance per detent, glided by the adaptive spring.
    Current,
    /// The event's distance, at once, with no smoothing.
    Raw,
    /// [`step`]'s gain on top of the adaptive spring.
    Accelerated,
}

const UNSET: u8 = u8::MAX;
static FEEL: AtomicU8 = AtomicU8::new(UNSET);

impl WheelFeel {
    fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::Raw,
            2 => Self::Accelerated,
            _ => Self::Current,
        }
    }

    /// The feel's name, as `BINGUX_WHEEL_FEEL` and the probe's log spell it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Raw => "raw",
            Self::Accelerated => "accelerated",
        }
    }
}

/// The wheel feel in force: [`set_feel`]'s, else `BINGUX_WHEEL_FEEL`
/// (`current`, `raw` or `accelerated`), else [`WheelFeel::Current`].
pub fn feel() -> WheelFeel {
    let value = FEEL.load(Ordering::Relaxed);
    if value != UNSET {
        return WheelFeel::from_u8(value);
    }
    let from_env = match std::env::var("BINGUX_WHEEL_FEEL").as_deref() {
        Ok("raw") => WheelFeel::Raw,
        Ok("accelerated") => WheelFeel::Accelerated,
        _ => WheelFeel::Current,
    };
    set_feel(from_env);
    from_env
}

/// Switches every scroll view of the process to `feel`, at once.
pub fn set_feel(feel: WheelFeel) {
    FEEL.store(feel as u8, Ordering::Relaxed);
}

/// Pixels per detent: `BINGUX_WHEEL_STEP`, default 64, the same setting
/// freya-winit scales a line event by.
pub fn px_per_detent() -> f64 {
    static VALUE: std::sync::LazyLock<f64> = std::sync::LazyLock::new(|| {
        std::env::var("BINGUX_WHEEL_STEP")
            .ok()
            .and_then(|value| value.parse::<f64>().ok())
            .filter(|value| value.is_finite())
            .map_or(64.0, |value| value.clamp(8.0, 400.0))
    });
    *VALUE
}

/// What the scroll views did with the last wheel event, for the scroll probe.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Trace {
    /// Counts events; a new value is a new event.
    pub seq: u64,
    pub feel: WheelFeel,
    pub line: bool,
    /// The event's distance as it arrived, and what the feel made of it.
    pub raw: (f32, f32),
    pub out: (f32, f32),
    pub gain: (f64, f64),
    pub rate: (f64, f64),
}

static TRACE: Mutex<Option<Trace>> = Mutex::new(None);

/// The last [`Trace`], if a wheel event has arrived.
pub fn last_trace() -> Option<Trace> {
    *TRACE.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn record(feel: WheelFeel, line: bool, raw: (f32, f32), out: (f32, f32), delta: Option<Delta>) {
    let mut slot = TRACE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let seq = slot.map_or(1, |trace| trace.seq + 1);
    *slot = Some(Trace {
        seq,
        feel,
        line,
        raw,
        out,
        gain: delta.map_or((1.0, 1.0), |delta| (delta.gain_x, delta.gain_y)),
        rate: delta.map_or((0.0, 0.0), |delta| (delta.rate_x, delta.rate_y)),
    });
}

/// What a scroll view does with a wheel event.
pub struct Applied {
    pub x: f32,
    pub y: f32,
    /// Whether the content glides to its new target (a wheel's detents) or
    /// jumps to it (pixel deltas, and the raw feel).
    pub animate: bool,
}

/// The scroll views' wheel handlers call this with the movement they read
/// from the event (already swapped for Shift), and scroll by what it returns.
pub fn apply(state: &RefCell<State>, source: WheelSource, x: f32, y: f32) -> Applied {
    let feel = feel();
    let line = source == WheelSource::Line;
    let (applied, delta) = match feel {
        WheelFeel::Current => (
            Applied {
                x,
                y,
                animate: line,
            },
            None,
        ),
        WheelFeel::Raw => (
            Applied {
                x,
                y,
                animate: false,
            },
            None,
        ),
        WheelFeel::Accelerated => {
            let kind = if line { Kind::Line } else { Kind::Pixel };
            // A line event arrives scaled by freya-winit; detents are what
            // the rate is measured in.
            let per_detent = px_per_detent();
            let (dx, dy) = if line {
                (f64::from(x) / per_detent, f64::from(y) / per_detent)
            } else {
                (f64::from(x), f64::from(y))
            };
            let mut state = state.borrow_mut();
            state.px_per_detent = per_detent;
            let (delta, next) = step(
                state.clone(),
                WheelEvent { kind, dx, dy },
                Instant::now(),
            );
            *state = next;
            (
                Applied {
                    x: delta.x as f32,
                    y: delta.y as f32,
                    animate: line,
                },
                Some(delta),
            )
        }
    };
    record(feel, line, (x, y), (applied.x, applied.y), delta);
    applied
}

/// Where a gliding wheel scroll continues from: the content's rendered
/// position plus the travel still pending towards `target`, at most
/// `max_lag` of it, and none when the wheel turned the other way.
pub fn trail_base(target: f32, rendered: f32, movement: f32, max_lag: f32) -> f32 {
    let pending = target - rendered;
    if (movement > 0.0 && pending < 0.0) || (movement < 0.0 && pending > 0.0) {
        return rendered;
    }
    rendered + pending.clamp(-max_lag, max_lag)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PX: f64 = 64.0;

    fn line(dy: f64) -> WheelEvent {
        WheelEvent {
            kind: Kind::Line,
            dx: 0.0,
            dy,
        }
    }

    /// Feeds `dy` detents at each of `times` (ms after `t0`), returning the deltas.
    fn feed(state: State, t0: Instant, dy: f64, times: &[u64]) -> (Vec<Delta>, State) {
        let mut state = state;
        let mut out = Vec::new();
        for &ms in times {
            let (delta, next) = step(state, line(dy), t0 + Duration::from_millis(ms));
            out.push(delta);
            state = next;
        }
        (out, state)
    }

    #[test]
    fn single_detent_is_one_x() {
        let (delta, _) = step(State::new(PX), line(-1.0), Instant::now());
        assert_eq!(delta.gain_y, 1.0);
        assert_eq!(delta.y, -PX);
        assert_eq!(delta.x, 0.0);
    }

    #[test]
    fn slow_detents_stay_one_x() {
        let t0 = Instant::now();
        let (deltas, _) = feed(State::new(PX), t0, 1.0, &[0, 400, 800, 1250, 1700]);
        for delta in deltas {
            assert_eq!(delta.gain_y, 1.0);
            assert_eq!(delta.y, PX);
        }
    }

    #[test]
    fn burst_grows_and_clamps() {
        let t0 = Instant::now();
        // 40 detents a second: well over the full-gain rate.
        let times: Vec<u64> = (0..30).map(|i| i * 25).collect();
        let (deltas, _) = feed(State::new(PX), t0, 1.0, &times);
        assert_eq!(deltas[0].gain_y, 1.0);
        let gains: Vec<f64> = deltas.iter().map(|delta| delta.gain_y).collect();
        assert!(
            gains.windows(2).all(|pair| pair[1] >= pair[0] - 1e-9),
            "gain must not fall during a burst: {gains:?}"
        );
        assert!(gains[3] > 1.5, "grows within a few detents: {gains:?}");
        let last = *gains.last().unwrap();
        assert_eq!(last, GAIN_MAX, "reaches the clamp: {gains:?}");
        assert!(gains.iter().all(|gain| *gain <= GAIN_MAX));
        assert_eq!(deltas.last().unwrap().y, PX * GAIN_MAX);
    }

    #[test]
    fn moderate_rate_gives_moderate_gain() {
        let t0 = Instant::now();
        // 12 detents a second: between the thresholds.
        let times: Vec<u64> = (0..12).map(|i| i * 83).collect();
        let (deltas, _) = feed(State::new(PX), t0, 1.0, &times);
        let gain = deltas.last().unwrap().gain_y;
        assert!(gain > 1.2 && gain < 3.0, "gain {gain}");
    }

    #[test]
    fn absurd_rate_is_clamped() {
        let t0 = Instant::now();
        let times: Vec<u64> = (0..50).collect();
        let (deltas, _) = feed(State::new(PX), t0, 1.0, &times);
        assert!(deltas.iter().all(|delta| delta.gain_y <= GAIN_MAX));
    }

    #[test]
    fn gain_decays_after_stillness() {
        let t0 = Instant::now();
        let times: Vec<u64> = (0..20).map(|i| i * 25).collect();
        let (deltas, state) = feed(State::new(PX), t0, 1.0, &times);
        let peak = deltas.last().unwrap().gain_y;
        assert_eq!(peak, GAIN_MAX);
        let end = *times.last().unwrap();

        // Soon after: still raised, but released from the peak.
        let (soon, _) = step(state.clone(), line(1.0), t0 + Duration::from_millis(end + 100));
        assert!(soon.gain_y > 1.0 && soon.gain_y < peak, "gain {}", soon.gain_y);

        // The longer the pause, the lower.
        let (later, _) = step(state.clone(), line(1.0), t0 + Duration::from_millis(end + 180));
        assert!(later.gain_y < soon.gain_y);

        // After the rest: exactly 1 again, and a single detent is one step.
        let (rested, _) = step(state, line(1.0), t0 + Duration::from_millis(end + 300));
        assert_eq!(rested.gain_y, 1.0);
        assert_eq!(rested.y, PX);
    }

    #[test]
    fn high_res_fractions_total_the_same_distance() {
        let t0 = Instant::now();
        // A detent delivered as eight value120 slices of 15/120, slowly: one
        // detent's distance in all, at 1x.
        let slow: Vec<u64> = (0..8).map(|i| i * 400).collect();
        let (deltas, _) = feed(State::new(PX), t0, 0.125, &slow);
        let total: f64 = deltas.iter().map(|delta| delta.y).sum();
        assert!((total - PX).abs() < 1e-9, "total {total}");

        // The same slices in a quick flick are accelerated, and never
        // reversed or lost.
        let quick: Vec<u64> = (0..24).map(|i| i * 10).collect();
        let (deltas, _) = feed(State::new(PX), t0, 0.125, &quick);
        let total: f64 = deltas.iter().map(|delta| delta.y).sum();
        assert!(total > 3.0 * PX && total <= 3.0 * PX * GAIN_MAX, "total {total}");
        assert!(deltas.iter().all(|delta| delta.y > 0.0));
    }

    #[test]
    fn reversal_resets() {
        let t0 = Instant::now();
        let times: Vec<u64> = (0..20).map(|i| i * 25).collect();
        let (deltas, state) = feed(State::new(PX), t0, 1.0, &times);
        assert_eq!(deltas.last().unwrap().gain_y, GAIN_MAX);
        let end = *times.last().unwrap();

        let (back, state) = step(state, line(-1.0), t0 + Duration::from_millis(end + 25));
        assert_eq!(back.gain_y, 1.0);
        assert_eq!(back.y, -PX);

        // The new direction builds its own rate from there.
        let (next, _) = step(state, line(-1.0), t0 + Duration::from_millis(end + 50));
        assert!(next.gain_y >= 1.0 && next.gain_y < GAIN_MAX);
        assert!(next.y < 0.0);
    }

    #[test]
    fn pixel_events_pass_through_and_reset() {
        let t0 = Instant::now();
        let times: Vec<u64> = (0..20).map(|i| i * 25).collect();
        let (_, state) = feed(State::new(PX), t0, 1.0, &times);
        let end = *times.last().unwrap();

        let touch = WheelEvent {
            kind: Kind::Pixel,
            dx: 3.5,
            dy: -11.25,
        };
        let (delta, state) = step(state, touch, t0 + Duration::from_millis(end + 10));
        assert_eq!((delta.x, delta.y), (3.5, -11.25));
        assert_eq!((delta.gain_x, delta.gain_y), (1.0, 1.0));

        // A burst of pixel events never builds a gain.
        let mut state = state;
        for i in 0..40 {
            let (delta, next) = step(
                state,
                touch,
                t0 + Duration::from_millis(end + 20 + i * 5),
            );
            assert_eq!((delta.x, delta.y), (3.5, -11.25));
            state = next;
        }

        // And the line state started over: the next click is one step.
        let (click, _) = step(state, line(1.0), t0 + Duration::from_millis(end + 300));
        assert_eq!(click.gain_y, 1.0);
    }

    #[test]
    fn axes_accelerate_independently() {
        let t0 = Instant::now();
        let mut state = State::new(PX);
        let mut last = None;
        // The thumb wheel spins fast while the vertical wheel is still.
        for i in 0..20 {
            let event = WheelEvent {
                kind: Kind::Line,
                dx: 1.0,
                dy: 0.0,
            };
            let (delta, next) = step(state, event, t0 + Duration::from_millis(i * 25));
            state = next;
            last = Some(delta);
        }
        let last = last.unwrap();
        assert_eq!(last.gain_x, GAIN_MAX);
        assert_eq!(last.x, PX * GAIN_MAX);
        assert_eq!((last.y, last.gain_y), (0.0, 1.0));

        // A single vertical click is still 1x.
        let (click, _) = step(state, line(1.0), t0 + Duration::from_millis(20 * 25));
        assert_eq!(click.gain_y, 1.0);
        assert_eq!(click.y, PX);
    }

    #[test]
    fn trail_base_drops_pending_travel_on_reversal() {
        // Pending 300 px down (target below rendered), wheel turns up.
        assert_eq!(trail_base(-500.0, -200.0, 64.0, 1200.0), -200.0);
        // Same direction: pending kept, capped.
        assert_eq!(trail_base(-500.0, -200.0, -64.0, 1200.0), -500.0);
        assert_eq!(trail_base(-5000.0, -200.0, -64.0, 1200.0), -1400.0);
    }
}
