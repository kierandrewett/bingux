//! A flame graph: a tree of named, weighted calls drawn as stacked bars, each
//! bar as wide as its share of its parent. The root is the top row and its
//! callees hang beneath it, the widest first.
//!
//! Use it wherever a total splits into nested parts: where a program's CPU
//! time goes (the inspector's Performance tab), or which folders hold a disk's
//! bytes (a storage page).
//!
//! ```ignore
//! use bingux_ui::charts::{FlameTree, Unit, flame_graph};
//!
//! let mut tree = FlameTree::new("All");
//! let render = tree.push(0, "render", 70.0);
//! tree.push(render, "layout", 30.0);
//! tree.push(0, "idle", 30.0);
//! tree.add(0, 100.0);
//! flame_graph(Rc::new(tree.sorted())).unit(Unit::Suffix("ms"))
//! ```
//!
//! A bar's label is cut with an ellipsis to fit; the full name and the bar's
//! numbers are in the detail line beneath the graph, which follows the
//! pointer. Click a bar to zoom into it. Keyboard: the graph is one tab stop.
//! Down moves to the widest callee, Up to the caller, Left and Right between
//! siblings, Enter zooms in, Escape zooms out, Home returns to the root.

use std::rc::Rc;

use freya::prelude::*;

use super::format::{Unit, format_value};
use super::geometry::NO_MIN;
use crate::component::ComponentName;
use crate::controls::action_button;
use crate::focus::use_focus_core;
use crate::style::*;
use crate::surface::{is_activation_key, use_tokens};
use crate::typography::{BODY, CAPTION, Ink};

/// The distance from one row of bars to the next: the bar and its gap.
fn row_pitch() -> f32 {
    Dim::FlameRow.px() + GAP
}

/// The gap between neighbouring bars.
const GAP: f32 = 1.0;
/// The narrowest a bar is drawn once it is placed.
const MIN_DRAWN: f32 = 1.0;
/// Bars narrower than this are not drawn, nor are their callees.
const MIN_BAR: f32 = 1.5;
/// Bars narrower than this carry no label.
const MIN_LABEL: f32 = 36.0;
/// A call that holds at least this share of its caller, and is its widest
/// callee, is part of the trunk the view starts past.
const TRUNK_SHARE: f64 = 0.85;
/// The deepest row drawn.
const MAX_DEPTH: usize = 80;
/// The room kept at the right of the bars for the scrollbar.
const SCROLLBAR: f32 = 12.0;
/// Height of the detail line beneath the graph.
const DETAIL_H: f32 = 52.0;
/// Width used before the graph has been measured.
const DEFAULT_WIDTH: f32 = 480.0;

/// The calls of a flame graph. Node 0 is the root.
pub struct FlameTree {
    nodes: Vec<Node>,
}

struct Node {
    name: Rc<str>,
    /// What the bar shows when it differs from `name`.
    label: Option<Rc<str>>,
    value: f64,
    parent: usize,
    highlight: bool,
    children: Vec<usize>,
}

impl FlameTree {
    /// A tree with only its root, named `root` and weighing nothing yet.
    pub fn new(root: impl Into<Rc<str>>) -> Self {
        Self {
            nodes: vec![Node {
                name: root.into(),
                label: None,
                value: 0.0,
                parent: 0,
                highlight: false,
                children: Vec::new(),
            }],
        }
    }

    /// Adds a callee of `parent` weighing `value`, and returns its index.
    pub fn push(&mut self, parent: usize, name: impl Into<Rc<str>>, value: f64) -> usize {
        let index = self.nodes.len();
        self.nodes.push(Node {
            name: name.into(),
            label: None,
            value,
            parent,
            highlight: false,
            children: Vec::new(),
        });
        self.nodes[parent].children.push(index);
        index
    }

    /// Adds `value` to a node's weight. A caller that counts samples pushes a
    /// node once and adds to it for every further sample.
    pub fn add(&mut self, node: usize, value: f64) {
        self.nodes[node].value += value;
    }

    /// What a bar shows in place of the node's name, which it cuts to fit
    /// anyway: a function's last path segment, a file's name without its
    /// folders. The full name stays in the detail line.
    pub fn label(&mut self, node: usize, label: impl Into<Rc<str>>) {
        self.nodes[node].label = Some(label.into());
    }

    /// Picks the node out in the accent colour, such as the application's own
    /// code among the libraries it calls.
    pub fn highlight(&mut self, node: usize) {
        self.nodes[node].highlight = true;
    }

    /// Orders every node's callees from the widest to the narrowest.
    pub fn sorted(mut self) -> Self {
        let values: Vec<f64> = self.nodes.iter().map(|node| node.value).collect();
        for node in &mut self.nodes {
            node.children
                .sort_by(|a, b| values[*b].total_cmp(&values[*a]));
        }
        self
    }

    /// The root's weight.
    pub fn total(&self) -> f64 {
        self.nodes[0].value
    }

    /// A node's weight less its callees': the part spent in the node itself.
    fn own(&self, node: usize) -> f64 {
        let callees: f64 = self.nodes[node]
            .children
            .iter()
            .map(|&child| self.nodes[child].value)
            .sum();
        (self.nodes[node].value - callees).max(0.0)
    }

    fn depth(&self, mut node: usize) -> usize {
        let mut depth = 0;
        while node != 0 {
            node = self.nodes[node].parent;
            depth += 1;
        }
        depth
    }
}

/// A flame graph. Build it with [`flame_graph`].
#[derive(Clone)]
pub struct FlameGraph {
    tree: Rc<FlameTree>,
    unit: Unit,
    label: Option<String>,
}

impl PartialEq for FlameGraph {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.tree, &other.tree) && self.unit == other.unit && self.label == other.label
    }
}

pub fn flame_graph(tree: Rc<FlameTree>) -> FlameGraph {
    FlameGraph {
        tree,
        unit: Unit::Number,
        label: None,
    }
}

impl FlameGraph {
    /// How a weight is written: `Unit::Suffix("samples")` reads `412 samples`.
    pub fn unit(mut self, unit: Unit) -> Self {
        self.unit = unit;
        self
    }

    /// The accessible name.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

/// One bar placed on the graph.
#[derive(Clone, Copy)]
struct Cell {
    node: usize,
    depth: usize,
    x: f32,
    width: f32,
}

/// Places `root` and everything under it in `width` pixels.
fn layout(tree: &FlameTree, root: usize, width: f32) -> Vec<Cell> {
    let mut cells = Vec::new();
    let mut stack = vec![Cell {
        node: root,
        depth: 0,
        x: 0.0,
        width,
    }];
    while let Some(cell) = stack.pop() {
        cells.push(cell);
        let node = &tree.nodes[cell.node];
        if cell.depth + 1 >= MAX_DEPTH || node.value <= 0.0 {
            continue;
        }
        let mut x = cell.x;
        for &child in &node.children {
            let share = (tree.nodes[child].value / node.value) as f32 * cell.width;
            if share >= MIN_BAR {
                stack.push(Cell {
                    node: child,
                    depth: cell.depth + 1,
                    x,
                    width: share,
                });
            }
            x += share;
        }
    }
    cells
}

/// Where the view starts before the user zooms: past the calls at the top
/// that do nothing but call the next one (`main` calling `run` calling `loop`),
/// which would only push the part worth reading down the screen.
fn trunk_end(tree: &FlameTree) -> usize {
    let mut node = 0;
    while let Some(&widest) = tree.nodes[node].children.first() {
        if tree.nodes[widest].value < tree.nodes[node].value * TRUNK_SHARE {
            break;
        }
        node = widest;
    }
    node
}

/// The key that tells a zoom of one tree from a zoom of another.
fn identity(tree: &Rc<FlameTree>) -> usize {
    Rc::as_ptr(tree) as usize
}

impl Component for FlameGraph {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let tree = self.tree.clone();
        let key = identity(&tree);
        let mut viewport = use_state(Area::default);
        let mut active = use_state(|| None::<(usize, usize)>);
        let zoom = use_state(|| None::<(usize, usize)>);
        let mut scroll = use_scroll_controller(ScrollConfig::default);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let focus_core = use_focus_core();

        let chosen = zoom()
            .filter(|(zoomed, node)| *zoomed == key && *node < tree.nodes.len())
            .map(|(_, node)| node);
        let view_root = chosen.unwrap_or_else(|| trunk_end(&tree));
        let measured = viewport.read().width();
        let width = (if measured > 0.0 {
            measured
        } else {
            DEFAULT_WIDTH
        } - SCROLLBAR)
            .max(MIN_LABEL);
        let height = viewport.read().height();
        let cells = layout(&tree, view_root, width);
        let rows = cells
            .iter()
            .map(|cell| cell.depth)
            .max()
            .map_or(1, |deepest| deepest + 1);
        let current = active()
            .filter(|(owner, node)| *owner == key && *node < tree.nodes.len())
            .map(|(_, node)| node);
        let total = tree.total().max(f64::MIN_POSITIVE);

        // The detail line: the name in full, then the numbers.
        let detail: Element = match current {
            Some(node) => {
                let value = tree.nodes[node].value;
                let own = tree.own(node);
                rect()
                    .width(Size::fill())
                    .tw(&tokens, &[flex_col(), gap(S0_5)])
                    .child(
                        BODY.label(&tokens, tree.nodes[node].name.to_string())
                            .max_lines(2)
                            .text_overflow(TextOverflow::Ellipsis),
                    )
                    .child(
                        CAPTION
                            .in_ink(Ink::Muted)
                            .label(
                                &tokens,
                                format!(
                                    "{:.1}% of all  ·  {:.1}% in the call itself  ·  {}  ·  depth {}",
                                    value / total * 100.0,
                                    own / total * 100.0,
                                    format_value(value, self.unit),
                                    tree.depth(node),
                                ),
                            )
                            .max_lines(1),
                    )
                    .into_element()
            }
            None => CAPTION
                .in_ink(Ink::Muted)
                .label(
                    &tokens,
                    if view_root == 0 {
                        "Point at a bar for its full name and numbers. Click it to zoom in; Escape zooms out."
                            .to_string()
                    } else if chosen.is_some() {
                        format!(
                            "Zoomed to {}. Escape zooms out; point at a bar for its full name and numbers.",
                            tree.nodes[view_root]
                                .label
                                .as_ref()
                                .unwrap_or(&tree.nodes[view_root].name)
                        )
                    } else {
                        format!(
                            "Showing from {} calls down, where the work splits. Point at a bar for its full name and numbers; click to zoom in.",
                            tree.depth(view_root)
                        )
                    },
                )
                .max_lines(2)
                .into_element(),
        };

        let bars = cells.iter().map(|cell| {
            let node = &tree.nodes[cell.node];
            Bar {
                node: cell.node,
                tree_key: key,
                label: node.label.clone().unwrap_or_else(|| node.name.clone()),
                row: cell.depth,
                x: cell.x,
                width: cell.width,
                tone: if Some(cell.node) == current {
                    Shade::Hot
                } else if node.highlight {
                    Shade::Own
                } else {
                    Shade::Plain
                },
                zoomable: !node.children.is_empty() && cell.node != view_root,
                active,
                zoom,
            }
            .into_element()
        });

        let all_calls = (view_root != 0).then(|| {
            action_button("All calls", move |()| {
                let mut zoom = zoom;
                zoom.set(Some((key, 0)));
            })
            .compact()
        });

        let key_tree = tree.clone();
        let on_key = move |event: Event<KeyboardEventData>| {
            let mut zoom = zoom;
            let node = (*active.peek())
                .filter(|(owner, _)| *owner == key)
                .map_or(view_root, |(_, node)| node);
            let target = match &event.key {
                Key::Named(NamedKey::ArrowDown) => Some(
                    key_tree.nodes[node]
                        .children
                        .first()
                        .copied()
                        .unwrap_or(node),
                ),
                Key::Named(NamedKey::ArrowUp) => Some(if node == view_root {
                    node
                } else {
                    key_tree.nodes[node].parent
                }),
                Key::Named(key @ (NamedKey::ArrowLeft | NamedKey::ArrowRight)) => {
                    let siblings = &key_tree.nodes[key_tree.nodes[node].parent].children;
                    let at = siblings.iter().position(|&sibling| sibling == node);
                    match (at, key) {
                        (Some(at), NamedKey::ArrowRight) => {
                            siblings.get(at + 1).copied().or(Some(node))
                        }
                        (Some(at), _) => Some(siblings[at.saturating_sub(1)]),
                        (None, _) => Some(node),
                    }
                }
                Key::Named(NamedKey::Home) => Some(view_root),
                Key::Named(NamedKey::Escape) => {
                    event.stop_propagation();
                    active.set(None);
                    if view_root != 0 {
                        zoom.set(Some((key, key_tree.nodes[view_root].parent)));
                    }
                    return;
                }
                pressed if is_activation_key(pressed) => {
                    event.stop_propagation();
                    if node != view_root && !key_tree.nodes[node].children.is_empty() {
                        zoom.set(Some((key, node)));
                    }
                    return;
                }
                _ => return,
            };
            let Some(target) = target else {
                return;
            };
            event.stop_propagation();
            active.set(Some((key, target)));
            // Keep the bar in view.
            let row = (key_tree
                .depth(target)
                .saturating_sub(key_tree.depth(view_root))) as f32
                * row_pitch();
            let shown = -(<(i32, i32)>::from(scroll).1 as f32);
            let pane = viewport.peek().height();
            let want = if row < shown {
                row
            } else if row + row_pitch() > shown + pane {
                row + row_pitch() - pane
            } else {
                shown
            };
            scroll.scroll_to_y(-(want.max(0.0) as i32));
        };

        let content = rect()
            .width(Size::px(width))
            .height(Size::px(rows as f32 * row_pitch()))
            .children(bars);
        let graph: Element = if tree.total() <= 0.0 {
            CAPTION
                .in_ink(Ink::Muted)
                .label(&tokens, "Nothing was sampled in this range.")
                .into_element()
        } else {
            ScrollView::new_controlled(scroll)
                .width(Size::fill())
                .height(Size::fill())
                .drag_scrolling(false)
                .child(content)
                .into_element()
        };

        let outline = super::view::focus_outline(
            &tokens,
            focus_core,
            focus(),
            viewport.read().width().max(1.0),
            height.max(1.0),
        );

        rect()
            .component("FlameGraph")
            .width(Size::fill())
            .height(Size::fill())
            .min_width(Size::px(NO_MIN))
            .tw(&tokens, &[flex_col(), grow_children(), gap(S2)])
            .child(
                rect()
                    .a11y_id(a11y_id)
                    .a11y_focusable(true)
                    .a11y_role(AccessibilityRole::Tree)
                    .a11y_alt(
                        self.label
                            .clone()
                            .unwrap_or_else(|| "Flame graph".to_string()),
                    )
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .on_sized(move |event: Event<SizedEventData>| viewport.set(event.area))
                    .on_key_down(on_key)
                    .child(graph)
                    .child(outline),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::px(DETAIL_H))
                    .tw(&tokens, &[flex_row(), grow_children(), gap(S3)])
                    .child(rect().width(Size::flex(1.0)).child(detail))
                    .maybe_child(all_calls),
            )
    }
}

/// How a bar is painted: by what it is, or that it is the one pointed at.
#[derive(Clone, Copy, PartialEq)]
enum Shade {
    Plain,
    Own,
    Hot,
}

/// One bar. A bar is its own component so that moving the pointer, which
/// changes only two bars, does not rebuild the thousand others.
#[derive(Clone, PartialEq)]
struct Bar {
    node: usize,
    tree_key: usize,
    label: Rc<str>,
    row: usize,
    x: f32,
    width: f32,
    tone: Shade,
    zoomable: bool,
    active: State<Option<(usize, usize)>>,
    zoom: State<Option<(usize, usize)>>,
}

impl Component for Bar {
    fn render_key(&self) -> DiffKey {
        DiffKey::U64(self.node as u64)
    }

    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let (node, key, zoomable) = (self.node, self.tree_key, self.zoomable);
        let (mut active, mut zoom) = (self.active, self.zoom);
        let (fill, ink) = match self.tone {
            Shade::Hot => (Paint::Accent, Ink::OnAccent),
            Shade::Own => (Paint::AccentSoft, Ink::Text),
            Shade::Plain => (Paint::Control, Ink::Text),
        };
        let wide = self.width >= MIN_LABEL;
        rect()
            .position(
                Position::new_absolute()
                    .top(self.row as f32 * row_pitch())
                    .left(self.x),
            )
            .width(Size::px((self.width - GAP).max(MIN_DRAWN)))
            .tw(&tokens, &[h(Dim::FlameRow)])
            .tw(
                &tokens,
                &[
                    flex_row(),
                    items(Align::Center),
                    px(S1),
                    clip(),
                    rounded(Radius::Mark),
                    bg(fill),
                ],
            )
            .on_pointer_enter(move |_| active.set_if_modified(Some((key, node))))
            .on_press(move |event: Event<PressEventData>| {
                event.stop_propagation();
                if zoomable {
                    zoom.set(Some((key, node)));
                }
            })
            .maybe_child(wide.then(|| {
                CAPTION
                    .in_ink(ink)
                    .label(&tokens, self.label.to_string())
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis)
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> FlameTree {
        let mut tree = FlameTree::new("All");
        let render = tree.push(0, "render", 60.0);
        tree.push(render, "layout", 40.0);
        tree.push(0, "idle", 40.0);
        tree.add(0, 100.0);
        tree.sorted()
    }

    #[test]
    fn callees_are_laid_out_by_share_of_the_parent() {
        let tree = sample();
        let cells = layout(&tree, 0, 200.0);
        let at = |node: usize| *cells.iter().find(|cell| cell.node == node).unwrap();
        let near = |a: f32, b: f32| (a - b).abs() < 0.01;
        assert!(near(at(1).width, 120.0));
        assert!(near(at(3).x, 120.0));
        assert!(near(at(3).width, 80.0));
        assert!(near(at(2).width, 80.0));
        assert_eq!(at(2).depth, 2);
    }

    #[test]
    fn the_view_starts_past_calls_that_only_call_one_thing() {
        let mut tree = FlameTree::new("All");
        let main = tree.push(0, "main", 100.0);
        let run = tree.push(main, "run", 100.0);
        tree.push(run, "layout", 60.0);
        tree.push(run, "paint", 40.0);
        tree.add(0, 100.0);
        assert_eq!(trunk_end(&tree.sorted()), run);
    }

    #[test]
    fn a_node_s_own_weight_excludes_its_callees() {
        let tree = sample();
        assert_eq!(tree.own(1), 20.0);
        assert_eq!(tree.own(0), 0.0);
    }

    #[test]
    fn slivers_are_not_drawn() {
        let mut tree = FlameTree::new("All");
        tree.push(0, "big", 999.0);
        tree.push(0, "tiny", 1.0);
        tree.add(0, 1000.0);
        let cells = layout(&tree.sorted(), 0, 300.0);
        assert_eq!(cells.len(), 2);
    }
}
