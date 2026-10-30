//! The accessibility checks. Turns the application snapshot and the frame
//! history into a list of [`Issue`]s. It knows nothing about rendering, so the
//! rules can be read and tested apart from the tab that shows them.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use freya::prelude::Color;

use crate::contrast;

use super::snapshot::{Kind, Node, Props, Snapshot};

/// A move counts as a layout shift only if no input arrived this recently.
const INPUT_QUIET: Duration = Duration::from_millis(500);
/// Animations longer than this are reported.
const LONG_ANIMATION: Duration = Duration::from_millis(500);
/// Frames longer than this are slow.
const SLOW_FRAME: Duration = Duration::from_millis(50);
/// Smallest side of an interactive target, in logical pixels.
const MIN_TARGET: f32 = 24.0;
/// Nodes this thin are scrollbar thumbs and fades; their moves are not shifts.
const SCROLLBAR_THIN: f32 = 12.0;
/// How long a run of idle frames is sampled before a node is judged.
const IDLE_WINDOW: Duration = Duration::from_secs(2);
/// Fewest idle frames a sample needs before it can flag a node.
const IDLE_MIN_FRAMES: u32 = 30;
/// Frames kept for the slow-frame count.
const FRAME_HISTORY: usize = 120;
/// Layout shifts kept for the report.
const SHIFT_HISTORY: usize = 256;
/// Font size assumed for text that does not set one (the Freya default).
const DEFAULT_FONT_SIZE: f32 = 14.0;
/// Text at or above this size is large text, held to 3:1 instead of 4.5:1.
const LARGE_TEXT: f32 = 24.0;
/// The weight from which text counts as bold (CSS 700).
const BOLD_WEIGHT: i32 = 700;
/// Bold text at or above this size (18.66 px, 14 pt) is also large text.
const LARGE_BOLD_TEXT: f32 = 18.66;

/// Roles that take input from the keyboard or the pointer.
const INTERACTIVE_ROLES: &[&str] = &[
    "Button",
    "CheckBox",
    "ComboBox",
    "Link",
    "ListBoxOption",
    "MenuItem",
    "MultilineTextInput",
    "RadioButton",
    "SearchInput",
    "SpinButton",
    "Slider",
    "Switch",
    "Tab",
    "TextInput",
];

/// The group an issue is listed under.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IssueKind {
    LayoutShift,
    Naming,
    Focus,
    TargetSize,
    Contrast,
    Geometry,
    Motion,
    Performance,
}

impl IssueKind {
    /// In the order the tab lists the groups.
    pub const ALL: [IssueKind; 8] = [
        IssueKind::Naming,
        IssueKind::Focus,
        IssueKind::TargetSize,
        IssueKind::Contrast,
        IssueKind::Geometry,
        IssueKind::LayoutShift,
        IssueKind::Motion,
        IssueKind::Performance,
    ];

    /// Why problems of this kind matter, said once for the group.
    pub fn why(self) -> &'static str {
        match self {
            IssueKind::Naming => {
                "A screen reader announces a control by its role and name. An unnamed control says nothing useful."
            }
            IssueKind::Focus => {
                "Keyboard and switch users can only operate controls that take focus."
            }
            IssueKind::TargetSize => {
                "Small targets are hard to hit with a pointer, a finger or a switch."
            }
            IssueKind::Contrast => "Low contrast text is hard to read, especially with low vision.",
            IssueKind::Geometry => {
                "An element with no size, or outside the window, cannot be seen or reached."
            }
            IssueKind::LayoutShift => {
                "Content that moves under the pointer or focus causes mis-clicks and loses the user's place."
            }
            IssueKind::Motion => {
                "Long animations, and any animation while reduced motion is on, unsettle or delay users."
            }
            IssueKind::Performance => {
                "Work done on every frame, and slow frames, make the interface stutter and drain battery."
            }
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            IssueKind::LayoutShift => "Layout shift",
            IssueKind::Naming => "Naming",
            IssueKind::Focus => "Focus",
            IssueKind::TargetSize => "Target size",
            IssueKind::Contrast => "Contrast",
            IssueKind::Geometry => "Geometry",
            IssueKind::Motion => "Motion",
            IssueKind::Performance => "Performance",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Severity::Error => "Error",
            Severity::Warning => "Warning",
            Severity::Info => "Info",
        }
    }
}

/// One thing wrong with the window.
#[derive(Clone, Debug, PartialEq)]
pub struct Issue {
    pub kind: IssueKind,
    pub severity: Severity,
    /// The node at fault, `None` for a problem of the whole window.
    pub node: Option<u64>,
    /// The node named for a reader: its kind, id and name or text.
    pub target: String,
    /// What is wrong.
    pub what: String,
    /// How to fix it.
    pub fix: &'static str,
    /// The measured colours of a Contrast issue.
    pub contrast: Option<ContrastDetail>,
}

/// An animation the application is running, as the frame loop reports it.
#[derive(Clone, Debug)]
pub struct Animation {
    pub node: Option<u64>,
    pub duration: Duration,
    pub running: bool,
}

/// What the checks need beyond one snapshot: how the window has changed over
/// recent frames and what the application is doing.
///
/// The caller feeds it: [`record_input`](Self::record_input) on every pointer
/// or key event, [`observe`](Self::observe) with each new snapshot, and
/// [`record_frame`](Self::record_frame) once per rendered frame. It also sets
/// `animations`, `reduced_motion` and `window_size` as it learns them.
#[derive(Default)]
pub struct FrameHistory {
    pub animations: Vec<Animation>,
    pub reduced_motion: bool,
    /// The window's size in logical pixels. While `None`, nodes are not
    /// checked against the window edges.
    pub window_size: Option<(f32, f32)>,
    last_input: Option<Instant>,
    /// Each node's area at the last snapshot: `[x, y, width, height]`.
    areas: HashMap<u64, [f32; 4]>,
    /// Size of the first root at the last snapshot. A change means the window
    /// was resized, which moves everything and is not a shift.
    root_size: Option<(f32, f32)>,
    shifts: VecDeque<u64>,
    frames: VecDeque<Duration>,
    idle: IdleWindow,
    /// Nodes that re-rendered on nearly every idle frame of the last window,
    /// with the number of frames they re-rendered on.
    flagged: Vec<(u64, u32)>,
}

#[derive(Default)]
struct IdleWindow {
    start: Option<Instant>,
    frames: u32,
    renders: HashMap<u64, u32>,
}

impl FrameHistory {
    /// A pointer or key event happened at `now`.
    pub fn record_input(&mut self, now: Instant) {
        self.last_input = Some(now);
        self.idle = IdleWindow::default();
    }

    /// A new layout was taken at `now`. Compares each node's area with the
    /// previous snapshot and records the ones that moved while the window
    /// was quiet.
    pub fn observe(&mut self, snapshot: &Snapshot, now: Instant) {
        let quiet = self
            .last_input
            .is_none_or(|last| now.duration_since(last) >= INPUT_QUIET);
        let animating = self.animations.iter().any(|animation| animation.running);
        let root_size = snapshot
            .roots
            .first()
            .and_then(|&id| snapshot.node(id))
            .and_then(|node| bounds(node))
            .map(|area| (area[2], area[3]));
        let resized = root_size != self.root_size;
        self.root_size = root_size;

        let mut next = HashMap::with_capacity(snapshot.nodes.len());
        for node in &snapshot.nodes {
            let Some(area) = bounds(node) else {
                continue;
            };
            if let Some(&before) = self.areas.get(&node.id) {
                let moved = before != area;
                let thin = area[2] <= SCROLLBAR_THIN || area[3] <= SCROLLBAR_THIN;
                if moved && quiet && !animating && !resized && !thin {
                    if self.shifts.len() == SHIFT_HISTORY {
                        self.shifts.pop_front();
                    }
                    self.shifts.push_back(node.id);
                }
            }
            next.insert(node.id, area);
        }
        self.areas = next;
    }

    /// Records one rendered frame at `now`, which took `frame` to render.
    /// `rerendered` lists the nodes whose render ran again in this frame.
    pub fn record_frame(&mut self, now: Instant, frame: Duration, rerendered: &[u64]) {
        if self.frames.len() == FRAME_HISTORY {
            self.frames.pop_front();
        }
        self.frames.push_back(frame);

        let quiet = self
            .last_input
            .is_none_or(|last| now.duration_since(last) >= INPUT_QUIET);
        if !quiet {
            self.idle = IdleWindow::default();
            return;
        }
        let window = &mut self.idle;
        let start = *window.start.get_or_insert(now);
        window.frames += 1;
        for &id in rerendered {
            *window.renders.entry(id).or_insert(0) += 1;
        }
        if now.duration_since(start) >= IDLE_WINDOW {
            if window.frames >= IDLE_MIN_FRAMES {
                let frames = window.frames;
                self.flagged = window
                    .renders
                    .iter()
                    .filter(|(_, count)| **count * 10 >= frames * 9)
                    .map(|(&id, &count)| (id, count))
                    .collect();
                self.flagged.sort_unstable();
            }
            self.idle = IdleWindow::default();
        }
    }
}

/// Runs every check on `snapshot`, using `history` for the checks over time.
/// The result is sorted by kind, then node.
pub fn check(snapshot: &Snapshot, history: &FrameHistory) -> Vec<Issue> {
    let mut out = Vec::new();
    for node in &snapshot.nodes {
        node_checks(snapshot, node, history, &mut out);
    }
    sibling_checks(snapshot, &mut out);
    history_checks(snapshot, history, &mut out);
    out.sort_by_key(|issue| (issue.kind, issue.node));
    out
}

/// Checks that look at one node on its own.
fn node_checks(snapshot: &Snapshot, node: &Node, history: &FrameHistory, out: &mut Vec<Issue>) {
    let role = node.a11y.role.as_str();
    let named = accessible_name(node).is_some();
    let has_role = !matches!(role, "" | "Unknown" | "GenericContainer");
    let interactive = is_interactive(node);
    let area = bounds(node);

    // A focusable node that a reader cannot name or identify.
    if node.a11y.focusable == Some(true) && (!has_role || !named) {
        let (severity, what) = match (has_role, named) {
            (true, false) => (Severity::Error, "Focusable but has no accessible name"),
            (false, true) => (Severity::Warning, "Focusable but has no accessibility role"),
            _ => (
                Severity::Error,
                "Focusable but has neither a role nor a name",
            ),
        };
        out.push(Issue {
            kind: IssueKind::Naming,
            severity,
            node: Some(node.id),
            target: describe(snapshot, Some(node.id)),
            what: what.into(),
            fix: "Give the element a role and an accessible name.",
            contrast: None,
        });
    }

    // A control that the keyboard cannot reach.
    if INTERACTIVE_ROLES.contains(&role) && node.a11y.focusable == Some(false) {
        out.push(Issue {
            kind: IssueKind::Focus,
            severity: Severity::Error,
            node: Some(node.id),
            target: describe(snapshot, Some(node.id)),
            what: "Interactive control cannot take keyboard focus".into(),
            fix: "Make the control focusable and give it a visible focus ring.",
            contrast: None,
        });
    }

    // A button with no name and no text inside it.
    if role == "Button" && !named && !has_text(snapshot, node.id) {
        out.push(Issue {
            kind: IssueKind::Naming,
            severity: Severity::Warning,
            node: Some(node.id),
            target: describe(snapshot, Some(node.id)),
            what: "Button has no name and no text, so it is probably icon-only".into(),
            fix: "Give it an accessible name, or a text label. A tooltip is not read by every reader.",
            contrast: None,
        });
    }

    // An image or icon without alternative text.
    if role == "Image" && !named {
        out.push(Issue {
            kind: IssueKind::Naming,
            severity: Severity::Error,
            node: Some(node.id),
            target: describe(snapshot, Some(node.id)),
            what: "Image has no alternative text".into(),
            fix: "Give the image a name that says what it shows, or hide it from readers if it is decoration.",
            contrast: None,
        });
    }

    if let Some(area) = area.filter(|_| interactive) {
        let (width, height) = (area[2], area[3]);
        if width <= 0.0 || height <= 0.0 {
            out.push(Issue {
                kind: IssueKind::Geometry,
                severity: Severity::Error,
                node: Some(node.id),
                target: describe(snapshot, Some(node.id)),
                what: "Interactive element has zero width or height".into(),
                fix: "Give it a size, or remove it when it is not in use.",
                contrast: None,
            });
        } else {
            if width < MIN_TARGET || height < MIN_TARGET {
                out.push(Issue {
                    kind: IssueKind::TargetSize,
                    severity: Severity::Warning,
                    node: Some(node.id),
                    target: describe(snapshot, Some(node.id)),
                    what: format!(
                        "Target is {width:.0} × {height:.0} px; the minimum is {:.0} × {:.0} px",
                        MIN_TARGET, MIN_TARGET
                    ),
                    fix: "Grow the hit area to at least 24 × 24 px, even if the visible mark is smaller.",
                    contrast: None,
                });
            }
            if let Some((window_width, window_height)) = history.window_size {
                let outside = area[0] + width <= 0.0
                    || area[1] + height <= 0.0
                    || area[0] >= window_width
                    || area[1] >= window_height;
                if outside {
                    out.push(Issue {
                        kind: IssueKind::Geometry,
                        severity: Severity::Warning,
                        node: Some(node.id),
                        target: describe(snapshot, Some(node.id)),
                        what: "Interactive element lies outside the window".into(),
                        fix: "Bring it into view, or check that it is meant to be off screen.",
                        contrast: None,
                    });
                }
            }
        }
    }

    if matches!(node.kind, Kind::Label | Kind::Paragraph) {
        contrast_check(snapshot, node, out);
    }
}

/// What a Contrast issue measured: the colours as drawn, so the tab can show
/// them as chips.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContrastDetail {
    /// The text colour as it reaches the screen: its own alpha and every
    /// ancestor's opacity applied, composited over `background`.
    pub foreground: Color,
    /// The opaque colour under the text: the ancestors' fills composited from
    /// the nearest opaque one down to the text.
    pub background: Color,
    pub ratio: f32,
    pub needed: f32,
    /// APCA Lc, advisory.
    pub apca: f32,
}

/// The foreground and background a label actually draws with, resolved from
/// the tree: the text colour (its own alpha times every ancestor's opacity),
/// and the background composited from the nearest opaque fill above the text
/// down through every translucent one, each at its own cumulative opacity.
/// `None` when no opaque fill is found above the text, so the background is
/// not knowable here.
pub fn resolved_colours(snapshot: &Snapshot, node: &Node) -> Option<(Color, Color)> {
    let foreground = first_in_chain(snapshot, node, |props| props.text_color)?;
    // Walk up, noting each fill with the opacity in force at its box.
    let mut chain = Vec::new();
    let mut current = Some(node);
    while let Some(here) = current {
        chain.push(here);
        current = here.parent.and_then(|parent| snapshot.node(parent));
    }
    // Cumulative opacity at each chain entry: its own times all above it.
    let mut opacity_above = vec![1.0_f32; chain.len() + 1];
    for index in (0..chain.len()).rev() {
        let own = chain[index]
            .props
            .as_ref()
            .and_then(|props| props.opacity)
            .unwrap_or(1.0);
        opacity_above[index] = opacity_above[index + 1] * own;
    }
    let mut fills: Vec<Color> = Vec::new();
    let mut opaque = false;
    for (index, here) in chain.iter().enumerate() {
        let Some(fill) = here.props.as_ref().and_then(|props| props.background) else {
            continue;
        };
        if fill.a() == 0 {
            continue;
        }
        let alpha = f32::from(fill.a()) / 255.0 * opacity_above[index];
        fills.push(fill.with_af32(alpha));
        if fill.a() == 255 && opacity_above[index] >= 0.999 {
            opaque = true;
            break;
        }
    }
    if !opaque {
        return None;
    }
    // Composite from the bottom (the last pushed, the opaque one) upward.
    let mut background = *fills.last()?;
    background = background.with_a(255);
    for fill in fills.iter().rev().skip(1) {
        background = contrast::composite(*fill, background);
    }
    let alpha = f32::from(foreground.a()) / 255.0 * opacity_above[0];
    Some((foreground.with_af32(alpha), background))
}

/// The text colour and the background it is drawn on, both as rendered, must
/// reach 4.5:1 (3:1 for large text), WCAG 1.4.3 AA.
fn contrast_check(snapshot: &Snapshot, node: &Node, out: &mut Vec<Issue>) {
    if node
        .text
        .as_deref()
        .is_none_or(|text| text.trim().is_empty())
    {
        return;
    }
    let Some((foreground, background)) = resolved_colours(snapshot, node) else {
        return;
    };
    let size = first_in_chain(snapshot, node, |props| props.font_size).unwrap_or(DEFAULT_FONT_SIZE);
    let bold = first_in_chain(snapshot, node, |props| props.font_weight)
        .is_some_and(|weight| i32::from(weight) >= BOLD_WEIGHT);
    let needed = if size >= LARGE_TEXT || (bold && size >= LARGE_BOLD_TEXT) {
        contrast::AA_LARGE
    } else {
        contrast::AA_TEXT
    };
    let shown = contrast::composite(foreground, background);
    let ratio = contrast::ratio_opaque(shown, background);
    if ratio < needed {
        let apca = contrast::apca(shown, background);
        out.push(Issue {
            kind: IssueKind::Contrast,
            severity: Severity::Error,
            node: Some(node.id),
            target: describe(snapshot, Some(node.id)),
            what: format!(
                "Text contrast is {ratio:.2}:1; AA needs {needed:.1}:1 (APCA Lc {apca:.0}, advisory)"
            ),
            fix: "Darken the text or lighten the background until the ratio meets the minimum; use the on-colour tokens on a filled surface.",
            contrast: Some(ContrastDetail {
                foreground: shown,
                background,
                ratio,
                needed,
                apca,
            }),
        });
    }
}

/// Checks that compare the children of one parent with each other.
fn sibling_checks(snapshot: &Snapshot, out: &mut Vec<Issue>) {
    for parent in &snapshot.nodes {
        let kids: Vec<&Node> = parent
            .children
            .iter()
            .filter_map(|&id| snapshot.node(id))
            .collect();
        for (index, first) in kids.iter().enumerate() {
            if !is_interactive(first) {
                continue;
            }
            if let Some(name) = accessible_name(first) {
                let twins = kids.iter().filter(|node| named_control(node, name)).count();
                let reported_earlier = kids[..index].iter().any(|node| named_control(node, name));
                if twins > 1 && !reported_earlier {
                    out.push(Issue {
                        kind: IssueKind::Naming,
                        severity: Severity::Warning,
                        node: Some(first.id),
                        target: describe(snapshot, Some(first.id)),
                        what: format!("{twins} sibling controls share the name \"{name}\""),
                        fix: "Give each control a name that says what it does, or group them under a label.",
                        contrast: None,
                    });
                }
            }
            let Some(area) = bounds(first) else {
                continue;
            };
            for second in kids.iter().skip(index + 1) {
                if !is_interactive(second) {
                    continue;
                }
                let Some(other) = bounds(second) else {
                    continue;
                };
                if overlaps(area, other) {
                    out.push(Issue {
                        kind: IssueKind::Geometry,
                        severity: Severity::Warning,
                        node: Some(first.id),
                        target: describe(snapshot, Some(first.id)),
                        what: format!(
                            "Overlaps the interactive sibling {}",
                            describe(snapshot, Some(second.id))
                        ),
                        fix: "Separate the controls, or make one of them a child of the other.",
                        contrast: None,
                    });
                }
            }
        }
    }
}

/// Checks that need the frame history: shifts, animations, idle re-renders and
/// slow frames.
fn history_checks(snapshot: &Snapshot, history: &FrameHistory, out: &mut Vec<Issue>) {
    let mut shifted: Vec<u64> = history.shifts.iter().copied().collect();
    shifted.sort_unstable();
    for run in shifted.chunk_by(|a, b| a == b) {
        let id = run[0];
        out.push(Issue {
            kind: IssueKind::LayoutShift,
            severity: Severity::Warning,
            node: Some(id),
            target: describe(snapshot, Some(id)),
            what: format!(
                "Moved or resized {} time(s) after first paint with no input in the last 500 ms",
                run.len()
            ),
            fix: "Reserve space for content that loads later, and animate with transforms instead of layout.",
            contrast: None,
        });
    }

    for animation in &history.animations {
        let target = animation.node;
        if animation.running && history.reduced_motion {
            out.push(Issue {
                kind: IssueKind::Motion,
                severity: Severity::Error,
                node: target,
                target: describe(snapshot, target),
                what: "Animation runs while reduced motion is on".into(),
                fix: "Skip or shorten the animation when reduced motion is on.",
                contrast: None,
            });
        }
        if animation.duration > LONG_ANIMATION {
            out.push(Issue {
                kind: IssueKind::Motion,
                severity: Severity::Info,
                node: target,
                target: describe(snapshot, target),
                what: format!("Animation lasts {} ms", animation.duration.as_millis()),
                fix: "Keep UI transitions under 500 ms.",
                contrast: None,
            });
        }
    }

    for &(id, count) in &history.flagged {
        out.push(Issue {
            kind: IssueKind::Performance,
            severity: Severity::Warning,
            node: Some(id),
            target: describe(snapshot, Some(id)),
            what: format!("Re-rendered on {count} idle frames while nothing was happening"),
            fix: "Memoise the component, or move the state it reads out of the render path.",
            contrast: None,
        });
    }

    let slow: Vec<f32> = history
        .frames
        .iter()
        .filter(|frame| **frame > SLOW_FRAME)
        .map(|frame| frame.as_secs_f32() * 1000.0)
        .collect();
    if !slow.is_empty() {
        let worst = history
            .frames
            .iter()
            .map(|frame| frame.as_secs_f32() * 1000.0)
            .fold(0.0f32, f32::max);
        out.push(Issue {
            kind: IssueKind::Performance,
            severity: Severity::Warning,
            node: None,
            target: describe(snapshot, None),
            what: format!(
                "{} of the last {} frames took longer than 50 ms (worst {worst:.0} ms)",
                slow.len(),
                history.frames.len()
            ),
            fix: "Profile the slow frames and move the heavy work out of the render path.",
            contrast: None,
        });
    }
}

/// The node's area as `[x, y, width, height]`, `None` if it was not laid out.
fn bounds(node: &Node) -> Option<[f32; 4]> {
    node.area
        .map(|area| [area.min_x(), area.min_y(), area.width(), area.height()])
}

fn accessible_name(node: &Node) -> Option<&str> {
    node.a11y
        .name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
}

fn is_interactive(node: &Node) -> bool {
    INTERACTIVE_ROLES.contains(&node.a11y.role.as_str()) || node.a11y.focusable == Some(true)
}

/// Whether the node is an interactive control with the given accessible name.
fn named_control(node: &Node, name: &str) -> bool {
    is_interactive(node) && accessible_name(node) == Some(name)
}

/// Whether the node has a text label anywhere below it.
fn has_text(snapshot: &Snapshot, id: u64) -> bool {
    snapshot.node(id).is_some_and(|node| {
        node.children.iter().any(|&child| {
            snapshot.node(child).is_some_and(|child_node| {
                child_node
                    .text
                    .as_deref()
                    .is_some_and(|text| !text.trim().is_empty())
                    || has_text(snapshot, child)
            })
        })
    })
}

/// The first value `pick` finds on `node` or its nearest ancestors, the way
/// text colour and background are inherited.
fn first_in_chain<T>(
    snapshot: &Snapshot,
    node: &Node,
    pick: impl Fn(&Props) -> Option<T>,
) -> Option<T> {
    let mut current = Some(node);
    while let Some(here) = current {
        if let Some(value) = here.props.as_ref().and_then(&pick) {
            return Some(value);
        }
        current = here.parent.and_then(|parent| snapshot.node(parent));
    }
    None
}

/// `node`'s description for a reader: the component that built it (else its
/// role), its id, and its name or text.
pub(super) fn describe(snapshot: &Snapshot, id: Option<u64>) -> String {
    let Some(id) = id else {
        return "Application window".into();
    };
    let Some(node) = snapshot.node(id) else {
        return format!("Element #{id}");
    };
    let label = accessible_name(node).or_else(|| {
        node.text
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty())
    });
    match label {
        Some(label) => {
            let shown: String = label.chars().take(40).collect();
            let ellipsis = if label.chars().count() > 40 {
                "…"
            } else {
                ""
            };
            format!("{} #{id} \"{shown}{ellipsis}\"", node.title())
        }
        None => format!("{} #{id}", node.title()),
    }
}

fn overlaps(a: [f32; 4], b: [f32; 4]) -> bool {
    if a[2] <= 0.0 || a[3] <= 0.0 || b[2] <= 0.0 || b[3] <= 0.0 {
        return false;
    }
    let width = (a[0] + a[2]).min(b[0] + b[2]) - a[0].max(b[0]);
    let height = (a[1] + a[3]).min(b[1] + b[3]) - a[1].max(b[1]);
    width > 1.0 && height > 1.0
}
