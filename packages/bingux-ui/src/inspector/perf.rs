//! The Performance tab: a frame timeline of the application and, after a
//! recording, a flame graph of the call stacks sampled while it ran.
//!
//! Frame durations are taken on every application redraw while the plugin
//! hook runs, whatever the tab shows. Stacks are sampled only between
//! [`start`] and [`stop`] (see [`perf_sample`]).

use std::cell::RefCell;
use std::cmp::Reverse;
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use freya::prelude::*;
use freya_winit::plugins::PluginEvent;

use super::a11y_checks::FrameHistory;
use super::perf_sample::{self, Profile};
use crate::controls::action_button;
use crate::style::*;
use crate::surface::use_tokens;
use crate::theme::Tokens;
use crate::typography::{CAPTION, SECTION, styled_label};

/// Frames kept for the timeline, newest last.
const TIMELINE: usize = 200;
/// Height of the timeline plot, in logical pixels.
const TIMELINE_H: f32 = 96.0;
/// One 60 Hz frame budget, in milliseconds. Frames over it are drawn in danger.
const BUDGET_MS: f32 = 1000.0 / 60.0;
/// A frame of this many milliseconds fills the plot.
const FULL_MS: f32 = 2.0 * BUDGET_MS;
/// A gap longer than this between a frame's start and end is idle, not a frame.
const IDLE_GAP_NS: u64 = 1_000_000_000;
/// Height of one flame graph row.
const ROW_H: f32 = 20.0;
/// Deepest flame graph row drawn.
const MAX_DEPTH: usize = 48;
/// Flame graph nodes below `1 / PRUNE` of all samples are not drawn (0.5 %).
const PRUNE: u64 = 200;
/// How often the tab checks the recorder for changes.
const POLL: Duration = Duration::from_millis(200);

#[derive(Default)]
struct Recorder {
    /// Bumped on every change the tab should show.
    version: u64,
    /// Start of the frame in progress, on the `perf_sample` clock.
    frame_start: Option<u64>,
    /// Frame durations in milliseconds, oldest first.
    frames: VecDeque<f32>,
    /// Sequence number of `frames[0]`. Frames are numbered from 0 and never
    /// reused, so a selection keeps its frames as the timeline moves on.
    first_seq: u64,
    /// Start of the recording in progress.
    recording: Option<u64>,
    /// Why the last recording could not start.
    error: Option<&'static str>,
    /// The last finished recording.
    session: Option<Session>,
}

struct Session {
    samples: u64,
    secs: f64,
    truncated: bool,
    flame: Flame,
}

/// A call tree built from samples. `nodes[0]` is a virtual root whose count is
/// the number of samples; its children are the outermost frames.
#[derive(Default)]
struct Flame {
    nodes: Vec<Node>,
}

struct Node {
    name: Box<str>,
    count: u64,
    /// Sorted by count, largest first.
    children: Vec<usize>,
}

thread_local! {
    static RECORDER: RefCell<Recorder> = RefCell::new(Recorder::default());
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
        if self.frames.len() == TIMELINE {
            self.frames.pop_front();
            self.first_seq += 1;
        }
        self.frames.push_back(ns as f32 / 1.0e6);
        self.version += 1;
        Some(ns)
    }
}

impl Flame {
    fn from_profile(profile: &Profile) -> Flame {
        let mut nodes = vec![Node {
            name: "".into(),
            count: 0,
            children: Vec::new(),
        }];
        let mut index: HashMap<(usize, Box<str>), usize> = HashMap::new();
        for sample in &profile.samples {
            let mut current = 0usize;
            nodes[0].count += 1;
            for frame in &sample.stack {
                let key = (current, frame.clone());
                let next = match index.get(&key) {
                    Some(&existing) => existing,
                    None => {
                        let created = nodes.len();
                        nodes.push(Node {
                            name: frame.clone(),
                            count: 0,
                            children: Vec::new(),
                        });
                        nodes[current].children.push(created);
                        index.insert(key, created);
                        created
                    }
                };
                nodes[next].count += 1;
                current = next;
            }
        }
        let counts: Vec<u64> = nodes.iter().map(|node| node.count).collect();
        for node in &mut nodes {
            node.children.sort_by_key(|&child| Reverse(counts[child]));
        }
        Flame { nodes }
    }

    fn total(&self) -> u64 {
        self.nodes.first().map_or(0, |node| node.count)
    }

    /// The children of `parent` that are large enough to draw.
    fn visible(&self, parent: usize) -> impl Iterator<Item = usize> + '_ {
        let total = self.total();
        self.nodes[parent]
            .children
            .iter()
            .copied()
            .filter(move |&child| self.nodes[child].count.saturating_mul(PRUNE) >= total)
    }

    /// One line for the footer: the node's share of all samples.
    fn describe(&self, index: usize) -> Option<String> {
        let node = self.nodes.get(index)?;
        let total = self.total().max(1) as f64;
        let children: u64 = node
            .children
            .iter()
            .map(|&child| self.nodes[child].count)
            .sum();
        let own = node.count.saturating_sub(children);
        Some(format!(
            "{}  ·  {:.1} % total  ·  {:.1} % self  ·  {} samples",
            node.name,
            node.count as f64 / total * 100.0,
            own as f64 / total * 100.0,
            node.count,
        ))
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

/// Starts a recording: samples the stacks of the application until [`stop`].
/// Returns false when sampling is unavailable.
pub fn start() -> bool {
    if !perf_sample::start() {
        write(|r| r.error = Some("Stack sampling is not available here."));
        return false;
    }
    let now = perf_sample::now_ns();
    write(|r| {
        r.recording = Some(now);
        r.error = None;
        r.session = None;
        r.version += 1;
    });
    true
}

/// Ends the recording in progress and builds its flame graph. Does nothing
/// when no recording is running.
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
    let flame = Flame::from_profile(&profile);
    write(|r| {
        let started = r.recording.take().unwrap_or(now);
        r.session = Some(Session {
            samples: profile.samples.len() as u64,
            secs: now.saturating_sub(started) as f64 / 1.0e9,
            truncated: profile.truncated,
            flame,
        });
        r.version += 1;
    });
}

fn version() -> u64 {
    read(|r| r.version)
}

/// The sequence number of the first frame on the timeline, and the frames.
fn timeline() -> (u64, Vec<f32>) {
    read(|r| (r.first_seq, r.frames.iter().copied().collect()))
}

fn with_flame<R>(f: impl FnOnce(&Flame) -> R) -> Option<R> {
    read(|r| r.session.as_ref().map(|session| f(&session.flame)))
}

fn status_line() -> String {
    let now = perf_sample::now_ns();
    read(|r| {
        if let Some(started) = r.recording {
            format!(
                "Recording · {:.1} s",
                now.saturating_sub(started) as f64 / 1.0e9
            )
        } else if let Some(error) = r.error {
            error.to_string()
        } else if let Some(session) = &r.session {
            format!(
                "{} samples over {:.1} s{}",
                session.samples,
                session.secs,
                if session.truncated {
                    " · buffer full, stopped early"
                } else {
                    ""
                },
            )
        } else {
            "Not recording".to_string()
        }
    })
}

/// The Performance tab.
pub fn view() -> Element {
    PerfTab.into()
}

#[derive(PartialEq)]
struct PerfTab;

impl Component for PerfTab {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut seen = use_state(|| 0u64);
        let hovered = use_state::<Option<u64>>(|| None);
        let drag = use_state::<Option<u64>>(|| None);
        let selection = use_state::<Option<(u64, u64)>>(|| None);
        let flame_hover = use_state::<Option<usize>>(|| None);

        // Poll the recorder. A recording changes on every frame, but nothing
        // in this tab's state does, so each poll while recording bumps `tick`
        // to re-render the tab, and asks the window for a frame too.
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

        let (base, frames) = timeline();
        let selected = *selection.read();
        let hovered_seq = *hovered.read();
        let recording = is_recording();

        let record = action_button(if recording { "Stop" } else { "Record" }, move |()| {
            let mut seen = seen;
            if is_recording() {
                stop();
            } else {
                let _ = start();
            }
            seen.set(version());
        });
        let record = if recording {
            record.destructive()
        } else {
            record.suggested()
        };
        let mut header = rect()
            .tw(
                &tokens,
                &[w_full(), flex_row(), items(Align::Center), gap(S2)],
            )
            .child(record)
            .child(styled_label(CAPTION, status_line()));
        if selected.is_some() {
            header = header.child(
                action_button("Clear selection", move |()| {
                    let mut selection = selection;
                    selection.set(None);
                })
                .flat()
                .compact(),
            );
        }

        let footer = flame_hover_text(*flame_hover.read());

        rect()
            .tw(
                &tokens,
                &[w_full(), h_full(), flex_col(), gap(S3), px(S4), py(S3)],
            )
            .child(header)
            .child(styled_label(SECTION, "Frames"))
            .child(timeline_plot(
                &tokens, base, &frames, selected, hovered, drag, selection,
            ))
            .child(styled_label(
                CAPTION,
                readout(base, &frames, selected, hovered_seq),
            ))
            .child(styled_label(SECTION, "Call stacks"))
            .child(flame_section(&tokens, flame_hover))
            .child(styled_label(CAPTION, footer))
    }
}

fn readout(
    base: u64,
    frames: &[f32],
    selected: Option<(u64, u64)>,
    hovered: Option<u64>,
) -> String {
    if let Some((from, to)) = selected {
        let mut count = 0usize;
        let mut sum = 0.0f32;
        let mut worst = 0.0f32;
        let mut slow = 0usize;
        for (index, &ms) in frames.iter().enumerate() {
            let seq = base + index as u64;
            if seq < from || seq > to {
                continue;
            }
            count += 1;
            sum += ms;
            worst = worst.max(ms);
            if ms > BUDGET_MS {
                slow += 1;
            }
        }
        if count == 0 {
            return "No frames in the selection.".to_string();
        }
        return format!(
            "{} frames · average {:.1} ms · worst {:.1} ms · {} over {:.1} ms",
            count,
            sum / count as f32,
            worst,
            slow,
            BUDGET_MS,
        );
    }
    if let Some(seq) = hovered {
        if let Some(&ms) = seq
            .checked_sub(base)
            .and_then(|index| frames.get(index as usize))
        {
            return format!("Frame {seq}: {ms:.1} ms");
        }
    }
    "Hover a frame to read it. Drag across frames to select a range.".to_string()
}

#[allow(clippy::too_many_arguments)]
fn timeline_plot(
    tokens: &Tokens,
    base: u64,
    frames: &[f32],
    selected: Option<(u64, u64)>,
    hovered: State<Option<u64>>,
    drag: State<Option<u64>>,
    selection: State<Option<(u64, u64)>>,
) -> Rect {
    let slot = Size::percent(100.0 / TIMELINE as f32);
    let slots = frames.iter().enumerate().map(|(index, &ms)| {
        let seq = base + index as u64;
        let in_range = selected.is_some_and(|(from, to)| seq >= from && seq <= to);
        let height = (ms / FULL_MS * TIMELINE_H).clamp(2.0, TIMELINE_H);
        let slow = ms > BUDGET_MS;
        rect()
            .width(slot.clone())
            .height(Size::px(TIMELINE_H))
            .tw(tokens, &[flex_col(), justify(Align::End)])
            .maybe(in_range, |element| {
                element.tw(tokens, &[bg(Paint::Selection)])
            })
            .on_pointer_enter(move |_: Event<PointerEventData>| {
                let mut hovered = hovered;
                let mut selection = selection;
                hovered.set(Some(seq));
                if let Some(anchor) = *drag.peek() {
                    selection.set(Some((anchor.min(seq), anchor.max(seq))));
                }
            })
            .on_pointer_down(move |_: Event<PointerEventData>| {
                let mut drag = drag;
                let mut selection = selection;
                drag.set(Some(seq));
                selection.set(Some((seq, seq)));
            })
            .child(
                rect()
                    .width(Size::percent(100.0))
                    .height(Size::px(height))
                    .tw(
                        tokens,
                        &[bg(if slow { Paint::Danger } else { Paint::Accent })],
                    ),
            )
    });
    rect()
        .width(Size::fill())
        .height(Size::px(TIMELINE_H))
        .tw(
            tokens,
            &[
                flex_row(),
                items(Align::End),
                rounded(Radius::Control),
                bg(Paint::Card),
            ],
        )
        .on_global_pointer_up(move |_| {
            let mut drag = drag;
            drag.set(None);
        })
        .children(slots)
}

fn flame_section(tokens: &Tokens, hover: State<Option<usize>>) -> Element {
    let rows =
        with_flame(|flame| (flame.total() > 0).then(|| flame_children(flame, 0, 0, tokens, hover)))
            .flatten();
    match rows {
        Some(rows) => ScrollView::new()
            .height(Size::fill())
            .children(vec![rows])
            .into_element(),
        None => {
            let text = if is_recording() {
                "Stop the recording to see where time goes."
            } else {
                "Record, then stop, to see where time goes."
            };
            styled_label(CAPTION, text)
        }
    }
}

/// The row of cells for the children of `parent`. A cell's width is its share
/// of its parent's samples, and its children sit in a row beneath it.
fn flame_children(
    flame: &Flame,
    parent: usize,
    depth: usize,
    tokens: &Tokens,
    hover: State<Option<usize>>,
) -> Rect {
    let parent_count = flame.nodes[parent].count.max(1) as f32;
    let cells = flame.visible(parent).map(move |child| {
        let node = &flame.nodes[child];
        let share = node.count as f32 / parent_count * 100.0;
        let mut cell = rect()
            .width(Size::percent(share))
            .tw(tokens, &[flex_col()])
            .child(flame_bar(&node.name, child, tokens, hover));
        if depth + 1 < MAX_DEPTH && !node.children.is_empty() {
            cell = cell.child(flame_children(flame, child, depth + 1, tokens, hover));
        }
        cell
    });
    rect()
        .width(Size::percent(100.0))
        .tw(tokens, &[flex_row()])
        .children(cells)
}

fn flame_bar(name: &str, index: usize, tokens: &Tokens, hover: State<Option<usize>>) -> Rect {
    let hot = *hover.read() == Some(index);
    rect()
        .width(Size::percent(100.0))
        .height(Size::px(ROW_H))
        .tw(
            tokens,
            &[
                flex_row(),
                items(Align::Center),
                clip(),
                rounded(Radius::Mark),
                px(S1),
                bg(if hot {
                    Paint::Selection
                } else {
                    Paint::AccentSoft
                }),
            ],
        )
        .on_pointer_enter(move |_: Event<PointerEventData>| {
            let mut hover = hover;
            hover.set(Some(index));
        })
        .child(
            CAPTION
                .label(tokens, name)
                .max_lines(1)
                .text_overflow(TextOverflow::Ellipsis),
        )
}

fn flame_hover_text(hover: Option<usize>) -> String {
    hover
        .and_then(|index| with_flame(|flame| flame.describe(index)).flatten())
        .unwrap_or_else(|| "Hover a call to see its share of the samples.".to_string())
}
