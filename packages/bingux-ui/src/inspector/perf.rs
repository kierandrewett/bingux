//! The Performance tab: record the application, then read how its frames
//! went and where the CPU time was spent.
//!
//! A recording has two parts. While it runs, every application redraw is
//! timed (the frame timeline), and the application's thread is sampled for its
//! call stacks ([`perf_sample`]). After Stop the tab shows a summary, the
//! frames along a time axis with the 60 Hz budget marked and slow frames in
//! red, and the sampled calls as a flame graph. Dragging across the timeline
//! selects a stretch; the summary and the flame graph then describe only that
//! stretch.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use freya::prelude::*;
use freya_winit::plugins::PluginEvent;

use super::a11y_checks::FrameHistory;
use super::perf_sample::{self, Profile};
use crate::charts::{Bar, FlameTree, Unit, flame_graph, timeline_chart};
use crate::component::ComponentName;
use crate::controls::{SpinnerSize, action_button, spinner, stat_tile};
use crate::icons::Icon;
use crate::layout::boxed;
use crate::shell::status_page::status_page;
use crate::style::*;
use crate::surface::use_tokens;
use crate::theme::Tokens;
use crate::typography::{BODY, CAPTION, EMPHASIS, Ink};

/// One 60 Hz frame budget, in milliseconds. Frames over it are slow.
const BUDGET_MS: f32 = 1000.0 / 60.0;
/// A gap longer than this between a frame's start and end is idle, not a frame.
const IDLE_GAP_NS: u64 = 1_000_000_000;
/// Frames kept for one recording. Past this the oldest are not kept.
const MAX_FRAMES: usize = 30_000;
/// Frames further apart than this are separate bursts of drawing: the frame
/// rate is measured inside a burst, not across idle time.
const BURST_GAP: f64 = 0.25;
/// How often the tab checks the recorder for changes.
const POLL: Duration = Duration::from_millis(150);
/// The shortest span the time axis shows while a recording is young.
const MIN_AXIS: f64 = 2.0;
/// The timeline's height, axes included.
const CHART_H: f32 = 124.0;
/// Below this width the four summary tiles stack two by two.
const NARROW: f32 = 760.0;
/// Below this width the timeline's colour legend is dropped to leave room for
/// its selection.
const LEGEND_MIN: f32 = 560.0;
/// The call graph card is at least this tall; the tab scrolls when the window
/// is too short for it and the rest.
const FLAME_MIN: f32 = 300.0;
/// Heights of the parts above the call graph, to give it what is left. A
/// button row, a stat tile, and a card's padding and heading around its body.
const TOOLBAR_H: f32 = 48.0;
const TILE_H: f32 = 78.0;
const CARD_CHROME: f32 = 2.0 * 16.0 + 36.0 + 18.0;
/// The shortest the introduction is drawn.
const INTRO_MIN: f32 = 420.0;
/// The tallest a frame bar can be before it is cut off, as a multiple of the
/// budget: one terrible frame must not flatten every other.
const CEILING: f32 = 6.0;
/// Calls whose name starts with this are the application's own code.
const SELF_PREFIX: &str = "bingux";

#[derive(Clone, Copy)]
struct Frame {
    /// Seconds from the start of the recording to the start of the frame.
    at: f64,
    ms: f32,
}

#[derive(Default)]
struct Recorder {
    /// Bumped on every change the tab should show.
    version: u64,
    /// Start of the frame in progress, on the `perf_sample` clock.
    frame_start: Option<u64>,
    recording: Option<Recording>,
    /// Why the last recording could not start.
    error: Option<&'static str>,
    session: Option<Rc<Session>>,
    /// Counts recordings, so a cached flame graph knows which it was built from.
    sessions: u64,
}

struct Recording {
    started: u64,
    frames: Vec<Frame>,
}

/// A finished recording.
struct Session {
    id: u64,
    secs: f64,
    truncated: bool,
    frames: Rc<Vec<Frame>>,
    /// Every distinct call name once; stacks refer to them by index.
    names: Vec<Box<str>>,
    samples: Vec<Stamp>,
}

struct Stamp {
    /// Seconds from the start of the recording.
    at: f64,
    /// Root first.
    stack: Box<[u32]>,
}

thread_local! {
    static RECORDER: RefCell<Recorder> = RefCell::new(Recorder::default());
    /// The flame graph last built: recording, range, graph.
    static FLAME: RefCell<Option<(u64, Option<(u64, u64)>, Rc<FlameTree>)>> = const { RefCell::new(None) };
}

fn read<R>(f: impl FnOnce(&Recorder) -> R) -> R {
    RECORDER.with(|r| f(&r.borrow()))
}

fn write<R>(f: impl FnOnce(&mut Recorder) -> R) -> R {
    RECORDER.with(|r| f(&mut r.borrow_mut()))
}

impl Recorder {
    /// Closes the frame in progress. Returns its duration in nanoseconds, or
    /// `None` when no frame was open or it was an idle gap.
    fn end_frame(&mut self, now: u64) -> Option<u64> {
        let start = self.frame_start.take()?;
        let ns = now.saturating_sub(start);
        if ns > IDLE_GAP_NS {
            return None;
        }
        if let Some(recording) = &mut self.recording
            && recording.frames.len() < MAX_FRAMES
        {
            recording.frames.push(Frame {
                at: start.saturating_sub(recording.started) as f64 / 1.0e9,
                ms: ns as f32 / 1.0e6,
            });
            self.version += 1;
        }
        Some(ns)
    }
}

/// Feeds the tab from the plugin hook. Call for every event the hook gets.
/// `inspector` is true when the event belongs to the inspector's own window,
/// whose frames are not the application's. Frame durations are pushed to
/// `history` too, for the accessibility checks.
pub fn on_event(event: &PluginEvent<'_>, inspector: bool, history: &mut FrameHistory) {
    if inspector {
        return;
    }
    match event {
        PluginEvent::BeforeMeasuringLayout { .. } | PluginEvent::BeforeRender { .. } => {
            let now = perf_sample::now_ns();
            write(|r| {
                r.frame_start.get_or_insert(now);
            });
        }
        PluginEvent::AfterRedraw { .. } => {
            let now = perf_sample::now_ns();
            if let Some(ns) = write(|r| r.end_frame(now)) {
                history.record_frame(Instant::now(), Duration::from_nanos(ns), &[]);
            }
            // The sampler stops itself when its buffer fills; keep the tab in step.
            if is_recording() && !perf_sample::is_active() {
                finish();
            }
        }
        _ => {}
    }
}

/// Starts a recording: times every frame and samples the stacks of the
/// application until [`stop`]. Returns false when sampling is unavailable.
pub fn start() -> bool {
    if !perf_sample::start() {
        write(|r| r.error = Some("Stack sampling is not available here."));
        return false;
    }
    let now = perf_sample::now_ns();
    write(|r| {
        r.recording = Some(Recording {
            started: now,
            frames: Vec::new(),
        });
        r.frame_start = None;
        r.error = None;
        r.session = None;
        r.version += 1;
    });
    FLAME.with(|cache| cache.borrow_mut().take());
    true
}

/// Ends the recording in progress and builds its session. Does nothing when
/// no recording is running.
pub fn stop() {
    if is_recording() {
        finish();
    }
}

/// Whether a recording is in progress.
pub fn is_recording() -> bool {
    read(|r| r.recording.is_some())
}

fn finish() {
    let profile = perf_sample::stop();
    let now = perf_sample::now_ns();
    let Some(recording) = write(|r| r.recording.take()) else {
        return;
    };
    let (names, samples) = intern(&profile, recording.started);
    write(|r| {
        r.sessions += 1;
        r.session = Some(Rc::new(Session {
            id: r.sessions,
            secs: now.saturating_sub(recording.started) as f64 / 1.0e9,
            truncated: profile.truncated,
            frames: Rc::new(recording.frames),
            names,
            samples,
        }));
        r.version += 1;
    });
}

/// The profile's stacks with each distinct name stored once.
fn intern(profile: &Profile, started: u64) -> (Vec<Box<str>>, Vec<Stamp>) {
    let mut names: Vec<Box<str>> = Vec::new();
    let mut index: HashMap<&str, u32> = HashMap::new();
    let samples = profile
        .samples
        .iter()
        .map(|sample| Stamp {
            at: sample.t_ns.saturating_sub(started) as f64 / 1.0e9,
            stack: sample
                .stack
                .iter()
                .map(|frame| {
                    *index.entry(frame.as_ref()).or_insert_with(|| {
                        names.push(frame.clone());
                        (names.len() - 1) as u32
                    })
                })
                .collect(),
        })
        .collect();
    (names, samples)
}

fn version() -> u64 {
    read(|r| r.version)
}

/// What the tab shows.
enum View {
    /// Nothing recorded yet, or the last attempt failed.
    Idle(Option<&'static str>),
    Recording {
        elapsed: f64,
        frames: Rc<Vec<Frame>>,
    },
    Done(Rc<Session>),
}

fn view_now() -> View {
    let now = perf_sample::now_ns();
    read(|r| {
        if let Some(recording) = &r.recording {
            View::Recording {
                elapsed: now.saturating_sub(recording.started) as f64 / 1.0e9,
                frames: Rc::new(recording.frames.clone()),
            }
        } else if let Some(session) = &r.session {
            View::Done(session.clone())
        } else {
            View::Idle(r.error)
        }
    })
}

/// Calls whose name starts with the application's own crates are picked out.
fn is_own(name: &str) -> bool {
    name.trim_start_matches('<').starts_with(SELF_PREFIX)
}

/// What a bar shows for a call: its last path segment (two for a closure, so
/// it says which function it is in), without generic arguments.
/// `bingux_ui::app::Application::run::<Page>::{closure#0}` becomes
/// `run::{closure#0}`.
fn short_name(name: &str) -> String {
    // Split on `::` outside <...> and {...}; a turbofish `::<` is not a split.
    let mut depth = 0i32;
    let mut starts = vec![0usize];
    let bytes = name.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'<' | b'{' | b'(' | b'[' => depth += 1,
            b'>' | b'}' | b')' | b']' => depth -= 1,
            b':' if depth == 0
                && bytes.get(i + 1) == Some(&b':')
                && bytes.get(i + 2) != Some(&b'<') =>
            {
                starts.push(i + 2);
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }
    let keep = if name.ends_with('}') && starts.len() > 1 {
        2
    } else {
        1
    };
    let from = starts[starts.len().saturating_sub(keep)];
    without_turbofish(&name[from..])
}

/// `run::<Page>::{closure#0}` as `run::{closure#0}`.
fn without_turbofish(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("::<") {
        out.push_str(&rest[..at]);
        let mut depth = 0i32;
        let mut end = rest.len();
        for (offset, c) in rest[at + 2..].char_indices() {
            match c {
                '<' => depth += 1,
                '>' => {
                    depth -= 1;
                    if depth == 0 {
                        end = at + 2 + offset + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

/// The call tree of the samples taken in `range` (all of them without one).
/// The last tree is kept, so a redraw that changes nothing reuses it.
fn flame(session: &Session, range: Option<(f64, f64)>) -> Rc<FlameTree> {
    let key = range.map(|(a, b)| (a.to_bits(), b.to_bits()));
    if let Some(tree) = FLAME.with(|cache| {
        cache
            .borrow()
            .as_ref()
            .filter(|(id, cached, _)| *id == session.id && *cached == key)
            .map(|(_, _, tree)| tree.clone())
    }) {
        return tree;
    }
    let mut tree = FlameTree::new("All calls");
    // Each node's callees as (name, node): a handful at most, so a scan beats
    // hashing.
    let mut callees: Vec<Vec<(u32, usize)>> = vec![Vec::new()];
    let mut highlighted = vec![None::<bool>; session.names.len()];
    for sample in &session.samples {
        if range.is_some_and(|(from, to)| sample.at < from || sample.at > to) {
            continue;
        }
        tree.add(0, 1.0);
        let mut at = 0;
        for &name in &sample.stack {
            let known = callees[at]
                .iter()
                .find(|(callee, _)| *callee == name)
                .map(|&(_, node)| node);
            at = match known {
                Some(node) => node,
                None => {
                    let full = session.names[name as usize].as_ref();
                    let node = tree.push(at, full, 0.0);
                    tree.label(node, short_name(full));
                    let own = *highlighted[name as usize].get_or_insert_with(|| is_own(full));
                    if own {
                        tree.highlight(node);
                    }
                    callees.push(Vec::new());
                    callees[at].push((name, node));
                    node
                }
            };
            tree.add(at, 1.0);
        }
    }
    let tree = Rc::new(tree.sorted());
    FLAME.with(|cache| *cache.borrow_mut() = Some((session.id, key, tree.clone())));
    tree
}

/// Numbers over a stretch of frames.
struct Stats {
    frames: usize,
    long: usize,
    average: f32,
    worst: f32,
    /// The frame rate while drawing, when two frames came close enough to
    /// measure one.
    fps: Option<f64>,
}

impl Stats {
    fn of(frames: &[Frame], range: Option<(f64, f64)>) -> Stats {
        let mut stats = Stats {
            frames: 0,
            long: 0,
            average: 0.0,
            worst: 0.0,
            fps: None,
        };
        let mut sum = 0.0f32;
        let mut intervals = 0usize;
        let mut interval_sum = 0.0f64;
        let mut previous: Option<f64> = None;
        for frame in frames {
            if range.is_some_and(|(from, to)| frame.at < from || frame.at > to) {
                continue;
            }
            stats.frames += 1;
            sum += frame.ms;
            stats.worst = stats.worst.max(frame.ms);
            if frame.ms > BUDGET_MS {
                stats.long += 1;
            }
            if let Some(before) = previous
                && frame.at - before <= BURST_GAP
            {
                intervals += 1;
                interval_sum += frame.at - before;
            }
            previous = Some(frame.at);
        }
        if stats.frames > 0 {
            stats.average = sum / stats.frames as f32;
        }
        if intervals > 0 && interval_sum > 0.0 {
            stats.fps = Some(intervals as f64 / interval_sum);
        }
        stats
    }
}

/// The tab, for the inspector window.
pub fn view() -> Element {
    PerfTab.into()
}

#[derive(PartialEq)]
struct PerfTab;

impl Component for PerfTab {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut seen = use_state(|| 0u64);
        let mut selection = use_state::<Option<(f64, f64)>>(|| None);
        let mut area = use_state(|| (0.0f32, 0.0f32));

        // Poll the recorder. A recording changes on every frame, but nothing
        // in this tab's state does, so each poll while recording bumps `tick`
        // to re-render the tab (the clock runs even when no frame is drawn),
        // and asks the window for a frame too.
        let mut tick = use_state(|| 0u64);
        let window = Platform::window_id();
        use_hook(move || {
            spawn(async move {
                loop {
                    timer(POLL).await;
                    let current = version();
                    let changed = current != *seen.peek();
                    if changed {
                        seen.set(current);
                    }
                    let recording_now = is_recording();
                    if recording_now {
                        let next = *tick.peek() + 1;
                        tick.set(next);
                    }
                    if changed || recording_now {
                        Platform::get().with_window(window, |w| w.request_redraw());
                    }
                }
            });
        });
        let _ = *seen.read();
        let _ = *tick.read();

        let view = view_now();
        let recording = matches!(view, View::Recording { .. });
        let range = *selection.read();

        let record = action_button(if recording { "Stop" } else { "Record" }, move |()| {
            let mut seen = seen;
            if is_recording() {
                stop();
            } else {
                selection.set(None);
                let _ = start();
            }
            seen.set(version());
        });
        let record = if recording {
            record.destructive()
        } else {
            record.suggested()
        };
        let toolbar = rect()
            .tw(
                &tokens,
                &[w_full(), flex_row(), items(Align::Center), gap(S3)],
            )
            .child(record)
            .child(status(&tokens, &view))
            .maybe_child(matches!(view, View::Done(_)).then(|| {
                action_button("Clear", move |()| {
                    write(|r| {
                        r.session = None;
                        r.error = None;
                        r.version += 1;
                    });
                    let mut selection = selection;
                    selection.set(None);
                })
                .flat()
                .compact()
            }));

        let body: Element = match &view {
            View::Idle(error) => intro(
                &tokens,
                *error,
                (area().1 - TOOLBAR_H - 2.0 * S4.fixed_px() - S3.fixed_px()).max(INTRO_MIN),
            ),
            View::Recording { elapsed, frames } => results(
                &tokens,
                area(),
                frames.clone(),
                None,
                (0.0, elapsed.max(MIN_AXIS)),
                range,
                selection,
                true,
            ),
            View::Done(session) => results(
                &tokens,
                area(),
                session.frames.clone(),
                Some(session.clone()),
                (0.0, session.secs.max(0.5)),
                range,
                selection,
                false,
            ),
        };

        let content = rect()
            .width(Size::fill())
            .tw(&tokens, &[flex_col(), gap(S3), p(S4)])
            .child(toolbar)
            .child(body);
        rect()
            .component("PerformanceTab")
            .tw(&tokens, &[w_full(), h_full()])
            .on_sized(move |event: Event<SizedEventData>| {
                area.set((event.area.width(), event.area.height()));
            })
            .child(
                ScrollView::new()
                    .width(Size::fill())
                    .height(Size::fill())
                    .drag_scrolling(false)
                    .child(content),
            )
    }
}

/// The status beside the Record button: a dot and one line.
fn status(tokens: &Tokens, view: &View) -> Rect {
    let (dot, ink, text) = match view {
        View::Idle(Some(error)) => (Paint::Danger, Ink::Danger, (*error).to_string()),
        View::Idle(None) => (Paint::Muted, Ink::Muted, "Not recording".to_string()),
        View::Recording { elapsed, frames } => (
            Paint::Danger,
            Ink::Text,
            format!(
                "Recording  ·  {elapsed:.1} s  ·  {} frames",
                grouped(frames.len())
            ),
        ),
        View::Done(session) => (
            Paint::Success,
            Ink::Muted,
            format!(
                "Recorded {:.1} s  ·  {} frames  ·  {} samples{}",
                session.secs,
                grouped(session.frames.len()),
                grouped(session.samples.len()),
                if session.truncated {
                    "  ·  buffer full, stopped early"
                } else {
                    ""
                },
            ),
        ),
    };
    rect()
        .tw(
            tokens,
            &[w_flex(), flex_row(), items(Align::Center), gap(S2)],
        )
        .child(rect().tw(
            tokens,
            &[w(Dim::Dot), h(Dim::Dot), rounded(Radius::Pill), bg(dot)],
        ))
        .child(
            CAPTION
                .in_ink(ink)
                .label(tokens, text)
                .max_lines(1)
                .text_overflow(TextOverflow::Ellipsis),
        )
}

/// What the tab says before the first recording.
fn intro(tokens: &Tokens, error: Option<&'static str>, height: f32) -> Element {
    let step = |number: &str, text: &str| {
        rect()
            .tw(tokens, &[w_full(), flex_row(), gap(S3)])
            .child(
                rect()
                    .tw(
                        tokens,
                        &[
                            w(Dim::Box5),
                            h(Dim::Box5),
                            center(),
                            rounded(Radius::Pill),
                            bg(Paint::Control),
                        ],
                    )
                    .child(CAPTION.label(tokens, number.to_string())),
            )
            .child(
                BODY.in_ink(Ink::Muted)
                    .paragraph(tokens, text.to_string())
                    .width(Size::flex(1.0)),
            )
    };
    let steps = rect()
        .tw(tokens, &[w(Dim::Dialog), flex_col(), gap(S2)])
        .child(step("1", "Press Record."))
        .child(step("2", "Use the app: scroll, open pages, type, resize."))
        .child(step(
            "3",
            "Press Stop. Drag across the timeline to pick a stretch; the call graph follows it.",
        ))
        .maybe_child(
            error.map(|error| CAPTION.in_ink(Ink::Danger).label(tokens, error.to_string())),
        );
    rect()
        .tw(tokens, &[w_full()])
        .height(Size::px(height))
        .child(
            status_page(Icon::Clock, "See where your app spends its time")
                .description(
                    "Record while you use the app. The inspector marks the frames that ran long and shows which calls took the time.",
                )
                .child(steps),
        )
        .into_element()
}

/// The summary, the frame timeline and the call graph.
#[allow(clippy::too_many_arguments)]
fn results(
    tokens: &Tokens,
    (width, height): (f32, f32),
    frames: Rc<Vec<Frame>>,
    session: Option<Rc<Session>>,
    axis: (f64, f64),
    range: Option<(f64, f64)>,
    selection: State<Option<(f64, f64)>>,
    live: bool,
) -> Element {
    let stats = Stats::of(&frames, range);
    let tiles = summary(tokens, &stats, width < NARROW);
    let timeline = timeline_card(
        tokens,
        &frames,
        axis,
        range,
        selection,
        live,
        width < LEGEND_MIN,
    );
    // The call graph takes what the window leaves, and no less than its
    // minimum.
    let tile_rows = if width < NARROW { 2.0 } else { 1.0 };
    let above = TOOLBAR_H
        + tile_rows * TILE_H
        + (tile_rows - 1.0) * S3.fixed_px()
        + CARD_CHROME
        + CHART_H
        + 3.0 * S3.fixed_px()
        + 2.0 * S4.fixed_px();
    let calls = calls_card(
        tokens,
        session,
        range,
        live,
        (height - above).max(FLAME_MIN),
        width < LEGEND_MIN,
    );
    rect()
        .tw(tokens, &[w_full(), flex_col(), gap(S3)])
        .child(tiles)
        .child(timeline)
        .child(calls)
        .into_element()
}

/// Four tiles in a row, or two rows of two when narrow.
fn summary(tokens: &Tokens, stats: &Stats, narrow: bool) -> Element {
    let none = || "–".to_string();
    let ms = |value: f32| format!("{value:.1} ms");
    let has = stats.frames > 0;
    let fps = stat_tile(
        stats.fps.map_or_else(none, |fps| format!("{fps:.0} fps")),
        "Frame rate",
    )
    .icon(Icon::Display);
    let average = stat_tile(
        if has { ms(stats.average) } else { none() },
        "Average frame",
    )
    .icon(Icon::Clock);
    let mut worst = stat_tile(if has { ms(stats.worst) } else { none() }, "Worst frame")
        .icon(Icon::DialogWarning);
    if stats.worst > 2.0 * BUDGET_MS {
        worst = worst.tone(Tone::Danger);
    } else if stats.worst > BUDGET_MS {
        worst = worst.tone(Tone::Warning);
    }
    let share = if has {
        stats.long as f32 / stats.frames as f32 * 100.0
    } else {
        0.0
    };
    let mut long = stat_tile(
        if has {
            format!("{}  ({share:.0} %)", stats.long)
        } else {
            none()
        },
        format!("Over {BUDGET_MS:.1} ms"),
    );
    long = if stats.long > 0 {
        long.icon(Icon::DialogError).tone(Tone::Danger)
    } else {
        long.icon(Icon::Check).tone(Tone::Accent)
    };
    let cell = |tile: Element| rect().width(Size::flex(1.0)).child(tile).into_element();
    let tiles: Vec<Element> = vec![
        cell(fps.into()),
        cell(average.into()),
        cell(worst.into()),
        cell(long.into()),
    ];
    let row = || rect().tw(tokens, &[w_full(), flex_row(), grow_children(), gap(S3)]);
    if narrow {
        let mut tiles = tiles.into_iter();
        let mut column = rect().tw(tokens, &[w_full(), flex_col(), gap(S3)]);
        while let Some(first) = tiles.next() {
            column = column.child(row().child(first).maybe_child(tiles.next()));
        }
        column.into_element()
    } else {
        row().children(tiles).into_element()
    }
}

/// A card's heading line: the title, optional extras after it, and a hint at
/// the right.
fn card_head(tokens: &Tokens, title: &str, extra: Vec<Element>, hint: Vec<Element>) -> Rect {
    // As tall as a compact button, so a button appearing in it moves nothing.
    rect()
        .tw(
            tokens,
            &[
                w_full(),
                h(Dim::Control),
                flex_row(),
                items(Align::Center),
                gap(S3),
            ],
        )
        .child(EMPHASIS.label(tokens, title.to_string()).max_lines(1))
        .children(extra)
        .child(
            rect()
                .tw(
                    tokens,
                    &[
                        w_flex(),
                        flex_row(),
                        justify(Align::End),
                        items(Align::Center),
                        gap(S2),
                    ],
                )
                .children(hint),
        )
}

/// A coloured dot and a word, for a chart legend.
fn key(tokens: &Tokens, paint: Paint, text: String) -> Rect {
    rect()
        .tw(tokens, &[flex_row(), items(Align::Center), gap(S1)])
        .child(rect().tw(
            tokens,
            &[w(Dim::Dot), h(Dim::Dot), rounded(Radius::Pill), bg(paint)],
        ))
        .child(CAPTION.in_ink(Ink::Muted).label(tokens, text).max_lines(1))
}

fn timeline_card(
    tokens: &Tokens,
    frames: &Rc<Vec<Frame>>,
    axis: (f64, f64),
    range: Option<(f64, f64)>,
    selection: State<Option<(f64, f64)>>,
    live: bool,
    compact: bool,
) -> Rect {
    let hint: Vec<Element> = match range {
        Some((from, to)) => vec![
            CAPTION
                .in_ink(Ink::Text)
                .label(
                    tokens,
                    format!("{from:.2} – {to:.2} s  ·  {:.2} s", to - from),
                )
                .max_lines(1)
                .into_element(),
            action_button("Clear", move |()| {
                let mut selection = selection;
                selection.set(None);
            })
            .flat()
            .compact()
            .into_element(),
        ],
        None if frames.is_empty() => Vec::new(),
        None => vec![
            CAPTION
                .in_ink(Ink::Muted)
                .label(tokens, "Drag across the frames to select a stretch")
                .max_lines(1)
                .into_element(),
        ],
    };
    let head = card_head(
        tokens,
        "Frames",
        if compact {
            Vec::new()
        } else {
            vec![
                key(tokens, Paint::Accent, "Within budget".to_string()).into_element(),
                key(tokens, Paint::Danger, format!("Over {BUDGET_MS:.1} ms")).into_element(),
            ]
        },
        hint,
    );
    let body: Element = if frames.is_empty() {
        rect()
            .tw(tokens, &[w_full(), center()])
            .height(Size::px(CHART_H))
            .child(
                CAPTION.in_ink(Ink::Muted).label(
                    tokens,
                    if live {
                        "Waiting for the app to draw a frame. Use the app."
                    } else {
                        "The app drew no frames while recording. Use it while recording to see them here."
                    },
                ),
            )
            .into_element()
    } else {
        let worst = frames.iter().map(|frame| frame.ms).fold(0.0f32, f32::max);
        let top = (worst * 1.08).clamp(2.0 * BUDGET_MS, CEILING * BUDGET_MS);
        timeline_chart(
            frames
                .iter()
                .map(|frame| Bar::new(frame.at, f64::from(frame.ms) / 1000.0, f64::from(frame.ms))),
        )
        .x_range(axis.0, axis.1)
        .x_unit(Unit::Suffix("s"))
        .y_unit(Unit::Suffix("ms"))
        .y_max(f64::from(top))
        .limit(f64::from(BUDGET_MS), "Budget")
        .value_name("Frame")
        .selection(range)
        .on_select(move |range: Option<(f64, f64)>| {
            let mut selection = selection;
            selection.set(range);
        })
        .height(CHART_H)
        .label("Frame times over the recording; drag to select a range")
        .into_element()
    };
    boxed(tokens).tw(tokens, &[gap(S2)]).child(head).child(body)
}

fn calls_card(
    tokens: &Tokens,
    session: Option<Rc<Session>>,
    range: Option<(f64, f64)>,
    live: bool,
    height: f32,
    compact: bool,
) -> Rect {
    let scope = match range {
        Some((from, to)) if !compact => format!("Selection {from:.2} – {to:.2} s"),
        Some(_) => "Selection".to_string(),
        None if compact => "All".to_string(),
        None => "Whole recording".to_string(),
    };
    let tree = session.as_ref().map(|session| flame(session, range));
    let samples = tree.as_ref().map_or(0.0, |tree| tree.total());
    let head = card_head(
        tokens,
        "Call stacks",
        Vec::new(),
        vec![
            CAPTION
                .in_ink(Ink::Muted)
                .label(
                    tokens,
                    match &tree {
                        Some(_) => format!("{scope}  ·  {} samples", grouped(samples as usize)),
                        None => String::new(),
                    },
                )
                .max_lines(1)
                .into_element(),
        ],
    );
    let body: Element = match tree {
        Some(tree) => flame_graph(tree)
            .unit(Unit::Suffix("samples"))
            .label("Call stacks sampled during the recording")
            .into_element(),
        None => rect()
            .tw(tokens, &[w_full(), h_flex(), flex_row(), center(), gap(S2)])
            .maybe_child(live.then(|| spinner(SpinnerSize::Small)))
            .child(CAPTION.in_ink(Ink::Muted).label(
                tokens,
                if live {
                    "The call graph appears when you press Stop."
                } else {
                    ""
                },
            ))
            .into_element(),
    };
    boxed(tokens)
        .tw(tokens, &[gap(S2), grow_children()])
        .height(Size::px(height))
        .child(head)
        .child(rect().tw(tokens, &[w_full(), h_flex()]).child(body))
}

/// `1234567` as `1,234,567`.
fn grouped(value: usize) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frames(spec: &[(f64, f32)]) -> Vec<Frame> {
        spec.iter().map(|&(at, ms)| Frame { at, ms }).collect()
    }

    #[test]
    fn stats_count_long_frames_and_find_the_worst() {
        let all = frames(&[(0.0, 10.0), (0.017, 30.0), (0.034, 12.0)]);
        let stats = Stats::of(&all, None);
        assert_eq!(stats.frames, 3);
        assert_eq!(stats.long, 1);
        assert_eq!(stats.worst, 30.0);
    }

    #[test]
    fn frame_rate_ignores_idle_gaps() {
        // Two bursts at 100 fps, seconds apart.
        let all = frames(&[
            (0.0, 5.0),
            (0.01, 5.0),
            (0.02, 5.0),
            (5.0, 5.0),
            (5.01, 5.0),
        ]);
        let fps = Stats::of(&all, None).fps.unwrap();
        assert!((fps - 100.0).abs() < 1.0, "{fps}");
    }

    #[test]
    fn a_range_keeps_only_its_frames() {
        let all = frames(&[(0.0, 10.0), (1.0, 40.0), (2.0, 10.0)]);
        let stats = Stats::of(&all, Some((0.5, 1.5)));
        assert_eq!(stats.frames, 1);
        assert_eq!(stats.worst, 40.0);
    }

    #[test]
    fn bars_show_the_end_of_a_long_name() {
        assert_eq!(short_name("std::rt::lang_start"), "lang_start");
        assert_eq!(short_name("main"), "main");
        assert_eq!(
            short_name("bingux_ui::app::Application::run::<Page>::{closure#0}"),
            "run::{closure#0}"
        );
        assert_eq!(short_name("alloc::vec::Vec::<u8>::push"), "push");
        assert_eq!(short_name("<app::Thing as core::fmt::Debug>::fmt"), "fmt");
        assert_eq!(short_name("alloc::vec::Vec::<u8>"), "Vec");
    }

    #[test]
    fn own_code_is_found_through_a_trait_impl() {
        assert!(is_own("<bingux_ui::Row as freya::Component>::render"));
        assert!(!is_own("std::rt::lang_start"));
    }

    #[test]
    fn thousands_are_grouped() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(1234567), "1,234,567");
    }
}
