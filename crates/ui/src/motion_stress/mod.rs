//! The in-motion stress suite of bingux-ui.
//!
//! Every control that moves is put through its motion one 16 ms frame at a
//! time, and the invariants that must hold on **every** frame are checked:
//! that a box the motion does not own does not move, that a thumb stays on its
//! track, that nothing overshoots, that a colour goes from its start to its end
//! along one path, that text keeps its elision, that an interrupted motion
//! starts from where it is and that the last frame is the frame an
//! uninterrupted run settles on. See `design/ui-motion-coverage.md` for the
//! control-by-control matrix and `docs/bingux-ui.md`, "In-motion tests".
//!
//! The rig steps a headless window with the animation clock at a fixed speed
//! and a wall-clock wait longer than the animation's 34 ms tick cap, so each
//! step moves every animation by exactly 16 ms whatever the load of the machine
//! (the same trick as `shell/page_motion_tests.rs`). The first tick of an
//! animation counts for nothing, so a motion of `ms` is over on step
//! `ms.div_ceil(16) + 1` after the input that started it.
//!
//! Each test runs in [`Env::ALL`]: scale 1.0 and 2.0, light and dark. Tests
//! that are about reduced motion use [`Env::reduced`].

mod frames;
mod series;

mod dialog;
mod dropdown;
mod expander;
mod focus_ring;
mod page_push;
mod scroll;
mod slider;
mod slider_row;
mod switch;
mod toast;
mod tooltip;

use std::collections::HashMap;
use std::time::Duration;

use freya::prelude::*;
use torin::prelude::Area;

use crate::inspector::snapshot::{Kind, Node, Snapshot as Tree};
use crate::snapshot::Snapshot;
use crate::theme::{DEFAULT_ACCENT, Scheme, SharedTokens, Tokens, freya_theme};
use crate::window::WindowHost;

pub(crate) use frames::*;
pub(crate) use series::*;

/// What one step of the animation clock moves every animation by, in milliseconds. The animation
/// counts whole milliseconds: the 34 ms tick cap at [`SPEED`] is 16.5 ms, cut to 16.
pub(crate) const STEP_MS: u64 = 16;
/// The clock's speed that turns the capped 34 ms tick into one [`STEP_MS`] step.
const SPEED: f32 = 0.4853;
/// The wall time of a step: longer than the tick cap, so the cap always decides.
const WALL: Duration = Duration::from_millis(40);

/// One rendering condition: the display scale, the colour scheme, and reduced motion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Env {
    pub scale: f64,
    pub dark: bool,
    pub reduced: bool,
}

impl Env {
    /// The four conditions every test runs in: scale 1.0 and 2.0, light and dark.
    pub(crate) const ALL: [Env; 4] = [
        Env {
            scale: 1.0,
            dark: false,
            reduced: false,
        },
        Env {
            scale: 1.0,
            dark: true,
            reduced: false,
        },
        Env {
            scale: 2.0,
            dark: false,
            reduced: false,
        },
        Env {
            scale: 2.0,
            dark: true,
            reduced: false,
        },
    ];

    /// The same four with reduced motion on.
    pub(crate) const REDUCED: [Env; 4] = [
        Env {
            scale: 1.0,
            dark: false,
            reduced: true,
        },
        Env {
            scale: 1.0,
            dark: true,
            reduced: true,
        },
        Env {
            scale: 2.0,
            dark: false,
            reduced: true,
        },
        Env {
            scale: 2.0,
            dark: true,
            reduced: true,
        },
    ];

    pub(crate) fn tokens(self) -> Tokens {
        let scheme = if self.dark {
            Scheme::PreferDark
        } else {
            Scheme::PreferLight
        };
        Tokens::new(DEFAULT_ACCENT, scheme, self.reduced)
    }

    pub(crate) fn label(self) -> String {
        format!(
            "{}x {}{}",
            self.scale,
            if self.dark { "dark" } else { "light" },
            if self.reduced { " reduced-motion" } else { "" }
        )
    }

    /// Runs `test` in every condition of `envs` in every window of `windows` and fails once, with
    /// every combination's findings.
    pub(crate) fn matrix(
        windows: &[(f32, f32)],
        envs: &[Env],
        mut test: impl FnMut(Env, (f32, f32), &mut Findings),
    ) {
        let mut all = Findings::default();
        for window in windows {
            for env in envs {
                let mut findings = Findings::default();
                test(*env, *window, &mut findings);
                all.merge(
                    format!("{} {}x{}", env.label(), window.0, window.1),
                    findings,
                );
            }
        }
        all.assert_clean();
    }

    /// Runs `test` in every condition of `envs` and fails once, with every condition's findings.
    pub(crate) fn each(envs: &[Env], mut test: impl FnMut(Env, &mut Findings)) {
        let mut all = Findings::default();
        for env in envs {
            let mut findings = Findings::default();
            test(*env, &mut findings);
            all.merge(env.label(), findings);
        }
        all.assert_clean();
    }
}

/// The window root of a scene: the tokens of `env` and Freya's theme. A scene wraps itself in
/// [`WindowHost`] (or in an `OverlayHost`, which holds one) for popups, tooltips and focus.
#[derive(PartialEq)]
struct Root<S: Component> {
    env: Env,
    scene: S,
}

impl<S: Component + Clone> Component for Root<S> {
    fn render(&self) -> impl IntoElement {
        let tokens = self.env.tokens();
        let shared = use_hook(move || SharedTokens(State::create(tokens)));
        use_provide_root_context(move || shared);
        let tokens = *shared.0.read();
        let _ = use_init_theme(|| freya_theme(&tokens));
        self.scene.clone().into_element()
    }
}

/// What a test found wrong, one line each. A test collects them and asserts once, so a failing
/// run lists every frame and every condition that broke an invariant, not only the first.
#[derive(Default)]
pub(crate) struct Findings {
    lines: Vec<String>,
}

impl Findings {
    pub(crate) fn add(&mut self, line: impl Into<String>) {
        self.lines.push(line.into());
    }

    /// Adds `message` when `ok` is false.
    pub(crate) fn check(&mut self, ok: bool, message: impl FnOnce() -> String) {
        if !ok {
            self.lines.push(message());
        }
    }

    /// Adds each of `results` that failed.
    pub(crate) fn extend(&mut self, results: impl IntoIterator<Item = Result<(), String>>) {
        for result in results {
            if let Err(message) = result {
                self.lines.push(message);
            }
        }
    }

    fn merge(&mut self, context: String, other: Findings) {
        self.lines.extend(
            other
                .lines
                .into_iter()
                .map(|line| format!("[{context}] {line}")),
        );
    }

    pub(crate) fn assert_clean(&self) {
        assert!(
            self.lines.is_empty(),
            "{} finding(s):\n  {}",
            self.lines.len(),
            self.lines.join("\n  ")
        );
    }
}

/// One captured frame: the inspector's tree of the window after layout, with the lookups a
/// motion test needs.
pub(crate) struct Frame {
    pub tree: Tree,
    by_id: HashMap<u64, usize>,
    /// The whole-pixel size of the window in logical pixels.
    pub window: (f32, f32),
}

impl Frame {
    fn of(shot: &mut Snapshot, scale: f64, window: (f32, f32)) -> Frame {
        let tree = Tree::capture_headless(shot.runner(), scale as f32);
        let by_id = tree
            .nodes
            .iter()
            .enumerate()
            .map(|(at, node)| (node.id, at))
            .collect();
        Frame {
            tree,
            by_id,
            window,
        }
    }

    pub(crate) fn node(&self, id: u64) -> &Node {
        &self.tree.nodes[self.by_id[&id]]
    }

    pub(crate) fn nodes(&self) -> impl Iterator<Item = &Node> {
        self.tree.nodes.iter()
    }

    /// Every node a component named `name` made, in tree order.
    pub(crate) fn all(&self, name: &str) -> Vec<&Node> {
        self.nodes()
            .filter(|node| node.a11y.component.as_deref() == Some(name))
            .collect()
    }

    /// The first node a component named `name` made.
    pub(crate) fn find(&self, name: &str) -> Option<&Node> {
        self.nodes()
            .find(|node| node.a11y.component.as_deref() == Some(name))
    }

    /// The `index`th node of the component `name`, or a panic that lists what is on screen.
    pub(crate) fn nth(&self, name: &str, index: usize) -> &Node {
        self.all(name).get(index).copied().unwrap_or_else(|| {
            panic!(
                "no {name} #{index} on screen; components: {:?}",
                self.nodes()
                    .filter_map(|node| node.a11y.component.as_deref())
                    .collect::<Vec<_>>()
            )
        })
    }

    pub(crate) fn area(&self, name: &str) -> Area {
        self.nth(name, 0)
            .area
            .unwrap_or_else(|| panic!("{name} has no area"))
    }

    /// The nodes below `id`, depth first, `id` not included.
    pub(crate) fn below(&self, id: u64) -> Vec<&Node> {
        let mut out = Vec::new();
        let mut stack: Vec<u64> = self.node(id).children.iter().rev().copied().collect();
        while let Some(next) = stack.pop() {
            let node = self.node(next);
            out.push(node);
            stack.extend(node.children.iter().rev());
        }
        out
    }

    /// The opacity a node is drawn at: its own `opacity` and every ancestor's multiplied together,
    /// read from the elements' styles (the tree's `used.opacity` does not follow an animation).
    pub(crate) fn opacity(&self, id: u64) -> f32 {
        let mut product = 1.0;
        let mut at = Some(id);
        while let Some(id) = at {
            let node = self.node(id);
            product *= node
                .props
                .as_ref()
                .and_then(|props| props.opacity)
                .unwrap_or(1.0);
            at = node.parent;
        }
        product
    }

    /// The solid fill a node paints, `None` when it has none or it is transparent.
    pub(crate) fn fill(node: &Node) -> Option<Color> {
        node.props
            .as_ref()
            .and_then(|props| props.background)
            .filter(|colour| colour.a() != 0)
    }

    /// Every text on screen: its string, box, and whether it is cut by an ellipsis now.
    pub(crate) fn texts(&self) -> Vec<TextLook> {
        self.nodes()
            .filter(|node| matches!(node.kind, Kind::Label | Kind::Paragraph))
            .filter_map(|node| {
                let text = node.text.clone()?;
                let area = node.area?;
                let used = node.used.text.as_ref()?;
                let intrinsic = used.intrinsic?;
                let lines = used.lines.unwrap_or(1);
                // Text is elided when it asks for more than its box and ends in an ellipsis, or is
                // cut where it is not allowed to wrap on.
                let elided = lines <= used.wrap.max_lines.unwrap_or(usize::MAX)
                    && lines == 1
                    && intrinsic > area.width() + 0.5;
                Some(TextLook {
                    text,
                    area,
                    intrinsic,
                    lines,
                    elided,
                    id: node.id,
                })
            })
            .collect()
    }

    /// What a still frame keeps: each node's box, fill and text, in tree order, rounded to a
    /// hundredth of a pixel. Opacity is left out (a scrollbar fades on its own clock) and so is the
    /// kind of node (an icon is a different element while it loads). Two frames of a settled
    /// window have the same signature; a frame mid-motion differs from both ends.
    pub(crate) fn signature(&self) -> Vec<String> {
        let mut out = Vec::new();
        for node in self.nodes() {
            let area = node.area.map_or_else(
                || "-".to_owned(),
                |a| {
                    format!(
                        "{:.2},{:.2},{:.2},{:.2}",
                        a.min_x(),
                        a.min_y(),
                        a.width(),
                        a.height()
                    )
                },
            );
            let fill = Frame::fill(node).map_or_else(
                || "-".to_owned(),
                |c| format!("{:02x}{:02x}{:02x}{:02x}", c.r(), c.g(), c.b(), c.a()),
            );
            out.push(format!(
                "{area} {fill} {}",
                node.text.as_deref().unwrap_or("")
            ));
        }
        out
    }
}

/// One text of a frame.
#[derive(Clone, Debug)]
pub(crate) struct TextLook {
    pub text: String,
    pub area: Area,
    pub intrinsic: f32,
    pub lines: usize,
    pub elided: bool,
    pub id: u64,
}

/// The running window of a test, stepped 16 ms at a time, with every frame it showed.
pub(crate) struct Rig {
    pub shot: Snapshot,
    pub env: Env,
    pub window: (f32, f32),
    /// Every frame since the rig started, the input frames included.
    pub frames: Vec<Frame>,
}

impl Rig {
    /// Mounts `scene` in a window of `window` logical pixels under `env` and steps it until it is
    /// still.
    pub(crate) fn new<S: Component + Clone>(env: Env, window: (f32, f32), scene: S) -> Rig {
        let mut shot = Snapshot::with_scale(
            move || {
                Root {
                    env,
                    scene: scene.clone(),
                }
                .into_element()
            },
            window,
            env.scale,
        );
        shot.runner().animation_clock().set_speed(SPEED);
        let mut rig = Rig {
            shot,
            env,
            window,
            frames: Vec::new(),
        };
        rig.capture();
        rig.rest();
        rig.frames.clear();
        rig.capture();
        rig
    }

    fn capture(&mut self) {
        let frame = Frame::of(&mut self.shot, self.env.scale, self.window);
        self.frames.push(frame);
    }

    /// The newest frame.
    pub(crate) fn now(&self) -> &Frame {
        self.frames.last().expect("the rig has a frame")
    }

    /// One 16 ms step of every animation.
    pub(crate) fn step(&mut self) -> &Frame {
        self.shot.runner().poll_n(WALL, 1);
        // The tick that `poll_n` ended with is taken by the animations on the next poll: run it, so
        // the frame shows every tick so far and an input that follows meets a settled state.
        self.shot.runner().handle_events_immediately();
        self.shot.runner().sync_and_update();
        self.capture();
        self.now()
    }

    pub(crate) fn steps(&mut self, count: usize) {
        for _ in 0..count {
            self.step();
        }
    }

    /// Steps until `quiet` frames in a row are the same, at most `cap` steps. Returns the number of
    /// steps taken before the window stopped changing (the index of the first frame of the quiet
    /// run, counted from the call).
    pub(crate) fn rest_with(&mut self, quiet: usize, cap: usize) -> usize {
        let mut same = 0;
        for taken in 1..=cap {
            let before = self.now().signature();
            self.step();
            if self.now().signature() == before {
                same += 1;
                if same >= quiet {
                    return taken - same + 1;
                }
            } else {
                same = 0;
            }
        }
        panic!("the window never came to rest in {cap} steps");
    }

    /// Steps until the window is still for 4 frames.
    pub(crate) fn rest(&mut self) -> usize {
        self.rest_with(4, 400)
    }

    /// Records the window as it is after an input, before any time passes.
    pub(crate) fn mark(&mut self) {
        self.capture();
    }

    /// A logical point as the pointer methods read it: physical pixels.
    pub(crate) fn phys(&self, x: f32) -> f32 {
        x * self.env.scale as f32
    }

    /// The centre of the first component `name`, in the pointer methods' units.
    pub(crate) fn centre(&self, name: &str) -> (f32, f32) {
        let area = self.now().area(name);
        (
            self.phys(area.min_x() + area.width() / 2.0),
            self.phys(area.min_y() + area.height() / 2.0),
        )
    }

    pub(crate) fn click(&mut self, name: &str) {
        let (x, y) = self.centre(name);
        self.shot.click(x, y);
        self.mark();
    }

    pub(crate) fn click_at(&mut self, x: f32, y: f32) {
        let (x, y) = (self.phys(x), self.phys(y));
        self.shot.click(x, y);
        self.mark();
    }

    pub(crate) fn hover_at(&mut self, x: f32, y: f32) {
        let (x, y) = (self.phys(x), self.phys(y));
        self.shot.hover(x, y);
        self.mark();
    }

    pub(crate) fn press_at(&mut self, x: f32, y: f32) {
        let (x, y) = (self.phys(x), self.phys(y));
        self.shot.press((x, y));
        self.mark();
    }

    pub(crate) fn move_to(&mut self, x: f32, y: f32) {
        let (x, y) = (self.phys(x), self.phys(y));
        self.shot.move_to((x, y));
        self.mark();
    }

    pub(crate) fn release_at(&mut self, x: f32, y: f32) {
        let (x, y) = (self.phys(x), self.phys(y));
        self.shot.release((x, y));
        self.mark();
    }

    pub(crate) fn key(&mut self, key: Key) {
        self.shot.key(key);
        self.mark();
    }

    pub(crate) fn key_with(&mut self, key: Key, modifiers: Modifiers) {
        self.shot.key_with(key, modifiers);
        self.mark();
    }

    pub(crate) fn key_char(&mut self, c: char) {
        self.key(Key::Character(c.to_string()));
    }

    pub(crate) fn wheel(&mut self, x: f32, y: f32, dy: f32) {
        let (x, y) = (self.phys(x), self.phys(y));
        self.shot.scroll(x, y, dy * self.env.scale as f32);
        self.mark();
    }
}
