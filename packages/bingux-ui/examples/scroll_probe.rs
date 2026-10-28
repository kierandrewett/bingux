//! A window to scroll in, showing what the mouse wheel sends and what the
//! scroll views do with it.
//!
//! ```sh
//! cargo run --example scroll_probe
//! ```
//!
//! The list on the right is the same scroll component the apps use: Files'
//! `VirtualScrollView` (20,000 rows) or Settings' `ScrollView` (2,000 rows).
//! The panel on the left is pinned and lists the last 30 raw wheel events as
//! winit delivered them (`line` is detents, `pixel` is a touchpad or a
//! free-spinning wheel; the phase and the interval to the previous event), and
//! the readouts above the list show the scroll controller: its target, its
//! rendered offset, the spring's velocity and the gain and distance the wheel
//! feel gave the last event. Two charts plot the offset and the wheel's
//! distance per event against time.
//!
//! Three wheel feels switch live, from the segmented control or the keys
//! `1`, `2` and `3` (`V` switches the list):
//!
//! * **Current**: as shipped, a fixed 64 px per detent glided by the spring.
//! * **Raw**: every event's distance at once, no smoothing, no gain: what "no
//!   acceleration" looks like.
//! * **Accelerated**: the candidate, `wheel_accel::step`: a gain of 1x to 4x
//!   from the detent rate over the last 150 ms, on top of the same spring.
//!
//! Everything is appended as JSON lines to `/var/tmp/ScrollProbe/events.jsonl`
//! (`winit` raw events, `applied` per scroll-view decision, `frame` per
//! animation frame, `feel` switches) and a summary per feel is written to
//! `/var/tmp/ScrollProbe/summary.json` when the window closes (and a second
//! after the wheel goes quiet, so a killed probe still leaves one).

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, VecDeque};
use std::io::Write as _;
use std::time::{Duration, Instant};

use bingux_ui::charts::{Series, Unit, line_chart};
use bingux_ui::prelude::{
    Application, OverlayHost, WindowHost, boxed, hstack, segmented, use_tokens, vstack,
};
use bingux_ui::style::*;
use bingux_ui::theme::{SharedTokens, Tokens, freya_theme, load_tokens};
use bingux_ui::typography;
use freya::prelude::wheel_accel::{self, Trace, WheelFeel};
use freya::prelude::*;
use serde_json::json;

const DIR: &str = "/var/tmp/ScrollProbe";
const VIRTUAL_ROWS: usize = 20_000;
const PLAIN_ROWS: usize = 2_000;
const LOG_LINES: usize = 30;
/// How much history the charts keep.
const CHART_SECONDS: f64 = 8.0;

// --- What the probe records ---------------------------------------------------

/// One frame of the scroll controller.
#[derive(Clone, Copy, Default)]
struct Sample {
    target: (f32, f32),
    rendered: (f32, f32),
    velocity: (f32, f32),
    animating: bool,
}

#[derive(Default)]
struct FeelStats {
    winit_line: u64,
    winit_pixel: u64,
    /// Events whose phase Freya drops (anything but `moved`).
    winit_dropped_phase: u64,
    intervals_ms: Vec<f64>,
    peak_events_per_s: usize,
    applied: u64,
    applied_px: f64,
    max_event_px: f64,
    gain_sum: f64,
    max_gain: f64,
    max_rate: f64,
    scrolled_px: f64,
    max_speed: f64,
}

#[derive(Clone, Default, PartialEq)]
struct Charts {
    target: Vec<(f64, f64)>,
    rendered: Vec<(f64, f64)>,
    raw_px: Vec<(f64, f64)>,
    applied_px: Vec<(f64, f64)>,
}

#[derive(Default)]
struct Update {
    log: Option<Vec<String>>,
    readouts: Option<Vec<String>>,
    charts: Option<Charts>,
}

struct Recorder {
    start: Instant,
    file: Option<std::fs::File>,
    feel: WheelFeel,
    view: &'static str,
    log: VecDeque<String>,
    last_raw: Option<Instant>,
    recent: VecDeque<Instant>,
    scale: f64,
    seq: u64,
    last_trace: Option<(Trace, f64)>,
    stats: BTreeMap<&'static str, FeelStats>,
    offsets: VecDeque<(f64, f64, f64)>,
    deltas: VecDeque<(f64, f64, f64)>,
    last_offset_at: f64,
    last_sample: Sample,
    speed_window: VecDeque<(f64, f64)>,
    last_charts: Instant,
    charts_dirty: bool,
    readouts: Vec<String>,
    last_activity: Option<Instant>,
    summary_written: bool,
}

impl Recorder {
    fn new() -> Self {
        let _ = std::fs::create_dir_all(DIR);
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(format!("{DIR}/events.jsonl"))
            .map_err(|error| eprintln!("scroll_probe: no event log: {error}"))
            .ok();
        let mut recorder = Self {
            start: Instant::now(),
            file,
            feel: wheel_accel::feel(),
            view: "virtual",
            log: VecDeque::new(),
            last_raw: None,
            recent: VecDeque::new(),
            scale: 1.0,
            seq: 0,
            last_trace: None,
            stats: BTreeMap::new(),
            offsets: VecDeque::new(),
            deltas: VecDeque::new(),
            last_offset_at: 0.0,
            last_sample: Sample::default(),
            speed_window: VecDeque::new(),
            last_charts: Instant::now(),
            charts_dirty: false,
            readouts: Vec::new(),
            last_activity: None,
            summary_written: true,
        };
        let session = json!({
            "type": "session",
            "unix_ms": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |since| since.as_millis() as u64),
            "wayland_display": std::env::var("WAYLAND_DISPLAY").ok(),
            "feel": recorder.feel.name(),
            "px_per_detent": wheel_accel::px_per_detent(),
            "window_s": wheel_accel::WINDOW.as_secs_f64(),
            "rate_from": wheel_accel::RATE_FROM,
            "rate_full": wheel_accel::RATE_FULL,
            "gain_max": wheel_accel::GAIN_MAX,
        });
        recorder.write(&session);
        recorder
    }

    fn ms(&self, at: Instant) -> f64 {
        at.saturating_duration_since(self.start).as_secs_f64() * 1000.0
    }

    fn write(&mut self, value: &serde_json::Value) {
        if let Some(file) = &mut self.file {
            let mut line = value.to_string();
            line.push('\n');
            // Flushed with every line: the file is the evidence if the probe dies.
            let _ = file.write_all(line.as_bytes()).and_then(|()| file.flush());
        }
    }

    fn stats(&mut self) -> &mut FeelStats {
        self.stats.entry(self.feel.name()).or_default()
    }

    fn switch_feel(&mut self, feel: WheelFeel) {
        self.feel = feel;
        self.last_raw = None;
        self.recent.clear();
        let ms = self.ms(Instant::now());
        self.write(&json!({ "type": "feel", "ms": ms, "feel": feel.name() }));
    }

    fn on_raw(&mut self, raw: freya_winit::bingux_scroll::RawWheel) {
        let ms = self.ms(raw.at);
        let dt = self
            .last_raw
            .map(|last| raw.at.saturating_duration_since(last).as_secs_f64() * 1000.0);
        self.last_raw = Some(raw.at);
        self.last_activity = Some(raw.at);
        self.summary_written = false;
        self.scale = raw.scale;
        self.recent.push_back(raw.at);
        while self
            .recent
            .front()
            .is_some_and(|at| raw.at.saturating_duration_since(*at) > Duration::from_secs(1))
        {
            self.recent.pop_front();
        }
        let last_second = self.recent.len();
        let instantaneous = dt.filter(|dt| *dt > 0.0).map(|dt| 1000.0 / dt);

        let stats = self.stats();
        match raw.kind {
            "line" => stats.winit_line += 1,
            _ => stats.winit_pixel += 1,
        }
        if raw.phase != "moved" {
            stats.winit_dropped_phase += 1;
        }
        if let Some(dt) = dt.filter(|dt| *dt > 0.0 && *dt < 2000.0) {
            stats.intervals_ms.push(dt);
        }
        stats.peak_events_per_s = stats.peak_events_per_s.max(last_second);

        self.log.push_front(format!(
            "{ms:>8.0} {kind:<5} {x:>+7.2} {y:>+8.2} {phase:<7} {dt:>6} {eps:>6}",
            kind = raw.kind,
            x = raw.x,
            y = raw.y,
            phase = raw.phase,
            dt = dt.map_or("-".to_string(), |dt| format!("{dt:.1}")),
            eps = instantaneous.map_or("-".to_string(), |eps| format!("{eps:.1}")),
        ));
        self.log.truncate(LOG_LINES);

        let feel = self.feel.name();
        self.write(&json!({
            "type": "winit", "ms": ms, "feel": feel, "kind": raw.kind,
            "x": raw.x, "y": raw.y, "phase": raw.phase, "scale": raw.scale,
            "dt_ms": dt, "events_per_s": instantaneous, "events_last_1s": last_second,
        }));
    }

    fn on_trace(&mut self, trace: Trace, sample: Sample, now: Instant) {
        let ms = self.ms(now);
        let per_detent = wheel_accel::px_per_detent();
        let out = f64::from(trace.out.1.abs().max(trace.out.0.abs()));
        let raw_px = f64::from(trace.raw.1.abs().max(trace.raw.0.abs()));
        let gain = trace.gain.1.max(trace.gain.0);
        let rate = trace.rate.1.max(trace.rate.0);
        // The distance one detent moved the content by: 64 x the gain.
        let px_per_detent = (trace.line && raw_px > 0.0).then(|| out / (raw_px / per_detent));
        let stats = self.stats();
        stats.applied += 1;
        stats.applied_px += out;
        stats.max_event_px = stats.max_event_px.max(out);
        stats.gain_sum += gain;
        stats.max_gain = stats.max_gain.max(gain);
        stats.max_rate = stats.max_rate.max(rate);
        self.last_trace = Some((trace, px_per_detent.unwrap_or(0.0)));
        self.deltas.push_back((ms / 1000.0, raw_px, out));
        self.charts_dirty = true;
        self.write(&json!({
            "type": "applied", "ms": ms, "feel": trace.feel.name(),
            "src": if trace.line { "line" } else { "pixel" },
            "raw_px": [trace.raw.0, trace.raw.1], "out_px": [trace.out.0, trace.out.1],
            "gain": [trace.gain.0, trace.gain.1], "rate_per_s": [trace.rate.0, trace.rate.1],
            "px_per_detent": px_per_detent,
            "target": [sample.target.0, sample.target.1],
            "rendered": [sample.rendered.0, sample.rendered.1],
        }));
    }

    /// One animation frame: reads the tap, the trace and the controller, and
    /// returns what the panels must show now.
    fn frame(&mut self, now: Instant, feel: WheelFeel, view: &'static str, sample: Sample) -> Update {
        if feel != self.feel {
            self.switch_feel(feel);
        }
        self.view = view;
        let mut update = Update::default();

        let raws = freya_winit::bingux_scroll::drain_tap();
        let had_raw = !raws.is_empty();
        for raw in raws {
            self.on_raw(raw);
        }
        if had_raw {
            update.log = Some(self.log.iter().cloned().collect());
        }
        if let Some(trace) = wheel_accel::last_trace()
            && trace.seq != self.seq
        {
            self.seq = trace.seq;
            self.on_trace(trace, sample, now);
        }

        // The controller, per frame while it moves.
        let t = self.ms(now) / 1000.0;
        let moved = sample.animating
            || sample.rendered != self.last_sample.rendered
            || sample.target != self.last_sample.target;
        if moved {
            let step = f64::from((sample.rendered.1 - self.last_sample.rendered.1).abs());
            let speed_window = &mut self.speed_window;
            speed_window.push_back((t, f64::from(sample.rendered.1)));
            while speed_window.front().is_some_and(|(at, _)| t - at > 0.1) {
                speed_window.pop_front();
            }
            let speed = match (speed_window.front(), speed_window.back()) {
                (Some((t0, y0)), Some((t1, y1))) if t1 > t0 => ((y1 - y0) / (t1 - t0)).abs(),
                _ => 0.0,
            };
            let stats = self.stats();
            stats.scrolled_px += step;
            stats.max_speed = stats.max_speed.max(speed);
            self.last_activity = Some(now);
            self.summary_written = false;
            self.write(&json!({
                "type": "frame", "ms": t * 1000.0,
                "target": [sample.target.0, sample.target.1],
                "rendered": [sample.rendered.0, sample.rendered.1],
                "velocity": [sample.velocity.0, sample.velocity.1],
                "animating": sample.animating,
            }));
            if t - self.last_offset_at >= 0.033 {
                self.last_offset_at = t;
                self.offsets
                    .push_back((t, f64::from(-sample.target.1), f64::from(-sample.rendered.1)));
                self.charts_dirty = true;
            }
        } else {
            self.speed_window.clear();
        }
        self.last_sample = sample;

        let readouts = self.readouts(sample);
        if readouts != self.readouts {
            self.readouts.clone_from(&readouts);
            update.readouts = Some(readouts);
        }

        if self.charts_dirty && now.saturating_duration_since(self.last_charts) >= Duration::from_millis(100) {
            self.last_charts = now;
            self.charts_dirty = false;
            update.charts = Some(self.charts(t));
        }

        // A summary a second after the wheel goes quiet.
        if !self.summary_written
            && self
                .last_activity
                .is_some_and(|at| now.saturating_duration_since(at) > Duration::from_secs(1))
        {
            self.write_summary(false);
        }
        update
    }

    fn readouts(&self, sample: Sample) -> Vec<String> {
        let mut lines = Vec::new();
        lines.push(format!(
            "feel {:<12} list {:<8} scale {:.2}",
            self.feel.name(),
            self.view,
            self.scale
        ));
        lines.push(format!(
            "target {:>10.1} px   rendered {:>10.1} px   velocity {:>8.0} px/s",
            sample.target.1, sample.rendered.1, sample.velocity.1
        ));
        lines.push(match self.last_trace {
            Some((trace, per_detent)) => {
                let detent = f64::from(trace.raw.1) / wheel_accel::px_per_detent();
                format!(
                    "last  {:<5} {:+.2} detent   gain {:.2}x   rate {:>5.1}/s   moved {:+.1} px   ({:.1} px per detent)",
                    if trace.line { "line" } else { "pixel" },
                    if trace.line { detent } else { f64::from(trace.raw.1) },
                    trace.gain.1.max(trace.gain.0),
                    trace.rate.1.max(trace.rate.0),
                    trace.out.1,
                    per_detent,
                )
            }
            None => "last  no wheel event on the list yet".to_string(),
        });
        let stats = self.stats.get(self.feel.name());
        let median = stats.map_or(0.0, |stats| percentile(&stats.intervals_ms, 0.5));
        lines.push(format!(
            "events last 1 s {:>3}   peak {:>3}   median interval {:>6.1} ms",
            self.recent.len(),
            stats.map_or(0, |stats| stats.peak_events_per_s),
            median,
        ));
        lines.push(format!(
            "scrolled {:>8.0} px in this feel   peak speed {:>7.0} px/s",
            stats.map_or(0.0, |stats| stats.scrolled_px),
            stats.map_or(0.0, |stats| stats.max_speed),
        ));
        lines
    }

    fn charts(&mut self, t: f64) -> Charts {
        for series in [&mut self.offsets, &mut self.deltas] {
            while series.front().is_some_and(|(at, ..)| t - at > CHART_SECONDS) {
                series.pop_front();
            }
        }
        Charts {
            target: self.offsets.iter().map(|(t, target, _)| (*t, *target)).collect(),
            rendered: self.offsets.iter().map(|(t, _, rendered)| (*t, *rendered)).collect(),
            raw_px: self.deltas.iter().map(|(t, raw, _)| (*t, *raw)).collect(),
            applied_px: self.deltas.iter().map(|(t, _, out)| (*t, *out)).collect(),
        }
    }

    fn summary(&self) -> serde_json::Value {
        let mut feels = serde_json::Map::new();
        for (name, stats) in &self.stats {
            let applied = stats.applied.max(1) as f64;
            feels.insert(
                (*name).to_string(),
                json!({
                    "winit_line_events": stats.winit_line,
                    "winit_pixel_events": stats.winit_pixel,
                    "winit_events_with_phase_freya_drops": stats.winit_dropped_phase,
                    "interval_ms": {
                        "p10": percentile(&stats.intervals_ms, 0.1),
                        "median": percentile(&stats.intervals_ms, 0.5),
                        "p90": percentile(&stats.intervals_ms, 0.9),
                        "min": percentile(&stats.intervals_ms, 0.0),
                    },
                    "peak_events_in_1s": stats.peak_events_per_s,
                    "applied_events": stats.applied,
                    "applied_px_total": stats.applied_px,
                    "applied_px_per_event_mean": stats.applied_px / applied,
                    "applied_px_per_event_max": stats.max_event_px,
                    "gain_mean": stats.gain_sum / applied,
                    "gain_max": stats.max_gain,
                    "rate_per_s_max": stats.max_rate,
                    "scrolled_px_rendered": stats.scrolled_px,
                    "peak_speed_px_per_s": stats.max_speed,
                }),
            );
        }
        json!({ "type": "summary", "ms": self.ms(Instant::now()), "feels": feels })
    }

    fn write_summary(&mut self, closing: bool) {
        let summary = self.summary();
        let _ = std::fs::write(
            format!("{DIR}/summary.json"),
            serde_json::to_string_pretty(&summary).unwrap_or_default(),
        );
        if closing {
            self.write(&summary);
        }
        self.summary_written = true;
    }
}

/// The `fraction` quantile of `values` (nearest rank), 0 for no values.
fn percentile(values: &[f64], fraction: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let index = ((sorted.len() - 1) as f64 * fraction).round() as usize;
    sorted[index]
}

thread_local! {
    static RECORDER: RefCell<Recorder> = RefCell::new(Recorder::new());
    static TOKENS: Cell<Option<Tokens>> = const { Cell::new(None) };
}

// --- The window ---------------------------------------------------------------

fn main() {
    // Read before the window exists: the tokens come from the desktop's settings.
    TOKENS.with(|tokens| tokens.set(Some(load_tokens())));
    freya_winit::bingux_scroll::enable_tap();
    RECORDER.with(|_| ());
    Application::new("bingux-scroll-probe")
        .title("Scroll probe")
        .size(1500.0, 940.0)
        .min_size(1100.0, 700.0)
        .run(|| Root);
    RECORDER.with(|recorder| recorder.borrow_mut().write_summary(true));
}

/// The window root: it provides the tokens the window host reads. They are
/// loaded once; the probe does not follow live appearance changes, so it runs
/// no `gsettings monitor` helper.
#[derive(PartialEq)]
struct Root;

impl Component for Root {
    fn render(&self) -> impl IntoElement {
        let shared = use_hook(|| {
            SharedTokens(State::create(
                TOKENS
                    .with(Cell::get)
                    .expect("main loads the tokens before the window opens"),
            ))
        });
        use_provide_root_context(move || shared);
        let tokens = *shared.0.read();
        let _ = use_init_theme(|| freya_theme(&tokens));
        WindowHost::new(OverlayHost::new(Probe))
    }
}

#[derive(Clone, Copy, PartialEq)]
enum View {
    Virtual,
    Plain,
}

impl View {
    fn name(self) -> &'static str {
        match self {
            Self::Virtual => "virtual",
            Self::Plain => "plain",
        }
    }
}

fn feel_of(name: &str) -> WheelFeel {
    match name {
        "raw" => WheelFeel::Raw,
        "accelerated" => WheelFeel::Accelerated,
        _ => WheelFeel::Current,
    }
}

#[derive(PartialEq)]
struct Probe;

impl Component for Probe {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let scroll = use_scroll_controller(ScrollConfig::default);
        let feel = use_state(wheel_accel::feel);
        let view = use_state(|| View::Virtual);
        let log = use_state(Vec::<String>::new);
        let readouts = use_state(Vec::<String>::new);
        let charts = use_state(Charts::default);

        use_hook(move || {
            let ticker = RenderingTicker::get();
            let window = Platform::get().current_window();
            let (mut log, mut readouts, mut charts) = (log, readouts, charts);
            spawn(async move {
                loop {
                    // The probe samples every frame, so frames keep coming.
                    window.request_redraw();
                    ticker.tick().await;
                    let (target_x, target_y): (i32, i32) = scroll.into();
                    let rendered = scroll.animated_position(torin::prelude::Point2D::new(target_x as f32, target_y as f32));
                    let (velocity, animating) = scroll.motion();
                    let sample = Sample {
                        target: (target_x as f32, target_y as f32),
                        rendered: (rendered.x, rendered.y),
                        velocity: (velocity.x, velocity.y),
                        animating,
                    };
                    let update = RECORDER.with(|recorder| {
                        recorder.borrow_mut().frame(
                            Instant::now(),
                            wheel_accel::feel(),
                            view.peek().name(),
                            sample,
                        )
                    });
                    if let Some(lines) = update.log {
                        log.set(lines);
                    }
                    if let Some(lines) = update.readouts {
                        readouts.set(lines);
                    }
                    if let Some(data) = update.charts {
                        charts.set(data);
                    }
                }
            });
        });

        let switch_feel = move |next: WheelFeel| {
            let mut feel = feel;
            wheel_accel::set_feel(next);
            feel.set(next);
        };
        let switch_view = move |next: View| {
            let mut view = view;
            let mut scroll = scroll;
            view.set(next);
            scroll.scroll_to_y(0);
        };
        let toggle_view = move || {
            let next = if *view.peek() == View::Virtual { View::Plain } else { View::Virtual };
            switch_view(next);
        };

        rect()
            .tw(&tokens, &[flex_row(), grow_children(), w_full(), h_full(), bg(Paint::Bg), ink(Paint::Text)])
            .font_family(bingux_ui::theme::font_family())
            .on_global_key_down(move |event: Event<KeyboardEventData>| {
                if !event.modifiers.is_empty() {
                    return;
                }
                if let Key::Character(text) = &event.key {
                    match text.as_str() {
                        "1" => switch_feel(WheelFeel::Current),
                        "2" => switch_feel(WheelFeel::Raw),
                        "3" => switch_feel(WheelFeel::Accelerated),
                        "v" | "V" => toggle_view(),
                        _ => {}
                    }
                }
            })
            .child(
                Panel {
                    feel,
                    view,
                    switch_feel: (move |next: WheelFeel| switch_feel(next)).into(),
                    switch_view: (move |next: View| switch_view(next)).into(),
                    log,
                }
                .into_element(),
            )
            .child(
                rect()
                    .tw(&tokens, &[flex_col(), grow_children(), w_flex(), h_full()])
                    .child(Readouts { readouts })
                    .child(ChartRow { charts })
                    .child(List { scroll, view }),
            )
    }
}

/// The pinned panel: the controls and the last 30 raw events.
#[derive(PartialEq)]
struct Panel {
    feel: State<WheelFeel>,
    view: State<View>,
    switch_feel: EventHandler<WheelFeel>,
    switch_view: EventHandler<View>,
    log: State<Vec<String>>,
}

impl Component for Panel {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let feel = *self.feel.read();
        let view = *self.view.read();
        let on_feel = self.switch_feel.clone();
        let on_view = self.switch_view.clone();
        let lines = self.log.read().clone();

        let header = format!(
            "{:>8} {:<5} {:>7} {:>8} {:<7} {:>6} {:>6}",
            "t ms", "kind", "x", "y", "phase", "dt ms", "ev/s"
        );
        let mut log = vstack(S0);
        log = log.child(typography::CODE.in_ink(typography::Ink::Muted).label(&tokens, header));
        for index in 0..LOG_LINES {
            let text = lines.get(index).cloned().unwrap_or_default();
            log = log.child(typography::CODE.label(&tokens, if text.is_empty() { " ".to_string() } else { text }));
        }

        rect()
            .tw(&tokens, &[flex_col(), w(Dim::PopupWidth), h_full(), p(S4), gap(S3), bg(Paint::Surface)])
            .child(typography::HEADING.label(&tokens, "Scroll probe"))
            .child(typography::CAPTION.label(&tokens, "Scroll in the list with the pointer over it. Keys: 1 2 3 feel, V list."))
            .child(segmented(
                vec![
                    ("current".into(), "Current".into()),
                    ("raw".into(), "Raw".into()),
                    ("accelerated".into(), "Accelerated".into()),
                ],
                feel.name(),
                move |name: String| on_feel.call(feel_of(&name)),
            ))
            .child(segmented(
                vec![
                    ("virtual".into(), "Virtual 20,000".into()),
                    ("plain".into(), "Plain 2,000".into()),
                ],
                view.name(),
                move |name: String| on_view.call(if name == "plain" { View::Plain } else { View::Virtual }),
            ))
            .child(typography::CAPTION.label(&tokens, "Raw wheel events as winit delivers them, newest first"))
            .child(log)
    }
}

#[derive(PartialEq)]
struct Readouts {
    readouts: State<Vec<String>>,
}

impl Component for Readouts {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut column = vstack(S1);
        for line in self.readouts.read().iter() {
            column = column.child(typography::CODE.label(&tokens, line.clone()));
        }
        boxed(&tokens).child(column)
    }
}

#[derive(PartialEq)]
struct ChartRow {
    charts: State<Charts>,
}

impl Component for ChartRow {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let charts = self.charts.read().clone();
        let offset = line_chart([
            Series::new("target", charts.target),
            Series::new("rendered", charts.rendered),
        ])
        .x_unit(Unit::Number)
        .y_unit(Unit::Suffix("px"))
        .height(Dim::Chart)
        .label("Scrolled distance against time (s)");
        let wheel = line_chart([
            Series::new("event", charts.raw_px),
            Series::new("moved", charts.applied_px),
        ])
        .x_unit(Unit::Number)
        .y_unit(Unit::Suffix("px"))
        .curve(bingux_ui::charts::Curve::Step)
        .height(Dim::Chart)
        .label("Wheel distance per event against time (s)");
        hstack(S2)
            .tw_layout(&[grow_children(), w_full()])
            .child(boxed(&tokens).tw_layout(&[w_flex()]).child(offset))
            .child(boxed(&tokens).tw_layout(&[w_flex()]).child(wheel))
    }
}

/// The list under test: the scroll component the apps use.
#[derive(PartialEq)]
struct List {
    scroll: ScrollController,
    view: State<View>,
}

impl Component for List {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let row_height = Dim::MenuItem.px();
        let list: Element = match *self.view.read() {
            View::Virtual => VirtualScrollView::new_controlled(
                |item, _: &()| Element::from(ProbeRow { index: item.index }),
                self.scroll,
            )
            .length(VIRTUAL_ROWS)
            .item_size(row_height)
            .into(),
            View::Plain => ScrollView::new_controlled(self.scroll)
                .children((0..PLAIN_ROWS).map(|index| rect().key(index).child(ProbeRow { index }).into_element()))
                .into(),
        };
        rect().tw(&tokens, &[w_full(), h_flex()]).child(list)
    }
}

#[derive(PartialEq)]
struct ProbeRow {
    index: usize,
}

impl Component for ProbeRow {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let fill = if self.index % 2 == 0 { Paint::Card } else { Paint::Bg };
        rect()
            .tw(&tokens, &[flex_row(), items(Align::Center), gap(S4), w_full(), h(Dim::MenuItem), px(S4), bg(fill)])
            .child(typography::CODE.label(&tokens, format!("Row {:>6}", self.index)))
            .child(typography::CAPTION.label(
                &tokens,
                format!("{} px from the top", self.index * Dim::MenuItem.px() as usize),
            ))
    }
}
