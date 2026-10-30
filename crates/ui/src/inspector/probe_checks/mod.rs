//! The probe: the facts about a capture that a check needs besides its PNG.
//!
//! The checks that read pixels (`scripts/contrast-pixels.py`,
//! `scripts/popup-perimeter.py`) need to know *where* to look: which rectangle
//! is text, which is a popup card. The checks that compare two captures of one
//! window (`scripts/structure-stability.py` for layout stability,
//! `scripts/scroll-continuity.py`, `scripts/number-roll-check.py`) need the
//! shape of the tree and the scroll offsets. With `BINGUX_UI_PROBE=FILE` set,
//! every capture the run saves appends one JSON line to FILE:
//!
//! ```text
//! capture   the PNG's path
//! scale     the render scale (areas below are logical pixels, the PNG physical)
//! window    [width, height] of the window in logical pixels
//! titles    the table of node titles (the component, else the role, else row/column/text/element)
//! strings   the table of texts
//! nodes     one [parent, title, x, y, width, height, flags, text] per node in
//!           preorder; parent and text are -1 when there is none, flags is the
//!           sum of VISIBLE 1, PAINTS 2, TEXT 4, MODAL 8, SCROLLER 16, WRAPPER 32,
//!           OUT_OF_FLOW 64, INERT 128 and CLIP 256 (the box clips its children)
//! texts     per text node: [node, [r, g, b, a], size, weight, opacity, lines]
//! modals    per modal scope (a popup, dialog, menu or sheet): the node of the
//!           card it draws as (the scope's own, or the painted node that fills or
//!           holds it), the card's radii, how many borders and shadows it paints
//!           and whether every header bar that touches its top edge is inside it
//! scrollers per scroll container: its node, its offset and the size of what it scrolls
//! ```
//!
//! It runs after the PNG is written and changes nothing in the window, so it
//! may be left on for every run. The rule IDs a check cites are those of
//! `design/rules.json`.

use std::collections::HashMap;
use std::path::Path;

use serde_json::{Value, json};

use super::snapshot::{Kind, Node, Snapshot as Tree};
use crate::snapshot::Snapshot;

const VISIBLE: u32 = 1;
const PAINTS: u32 = 2;
const TEXT: u32 = 4;
const MODAL: u32 = 8;
const SCROLLER: u32 = 16;
const WRAPPER: u32 = 32;
const OUT_OF_FLOW: u32 = 64;
const INERT: u32 = 128;
const CLIP: u32 = 256;

fn border_box(node: &Node) -> [f32; 4] {
    node.used
        .border
        .or(node.area)
        .map_or([0.0; 4], |area| [area.min_x(), area.min_y(), area.width(), area.height()])
}

fn is_visible(node: &Node) -> bool {
    let area = border_box(node);
    !node.used.hidden
        && node.used.opacity.is_none_or(|opacity| opacity > 0.0)
        && node
            .props
            .as_ref()
            .and_then(|props| props.opacity)
            .is_none_or(|opacity| opacity > 0.0)
        && area[2] > 0.0
        && area[3] > 0.0
}

fn paints(node: &Node) -> bool {
    node.props.as_ref().is_some_and(|props| {
        props.background.is_some_and(|colour| colour.a() != 0) || !props.borders.is_empty()
    })
}

/// A string table: each distinct string once, by index.
#[derive(Default)]
struct Table {
    items: Vec<String>,
    index: HashMap<String, usize>,
}

impl Table {
    fn put(&mut self, text: &str) -> usize {
        if let Some(at) = self.index.get(text) {
            return *at;
        }
        self.items.push(text.to_owned());
        self.index.insert(text.to_owned(), self.items.len() - 1);
        self.items.len() - 1
    }
}

/// The probe of a captured tree, as the JSON a capture appends.
pub(crate) fn probe(tree: &Tree, capture: &str) -> Value {
    let positions: HashMap<u64, usize> = tree
        .nodes
        .iter()
        .enumerate()
        .map(|(at, node)| (node.id, at))
        .collect();
    let mut titles = Table::default();
    let mut strings = Table::default();
    let mut nodes = Vec::with_capacity(tree.nodes.len());
    let mut texts = Vec::new();
    let mut modals = Vec::new();
    let mut scrollers = Vec::new();
    // The layout's opacity states are not resolved in a headless capture, so the
    // opacity of a node is the product of its own and its ancestors' declared ones.
    let mut opacity = vec![1.0_f32; tree.nodes.len()];
    for (at, node) in tree.nodes.iter().enumerate() {
        let above = node
            .parent
            .and_then(|id| positions.get(&id))
            .map_or(1.0, |parent| opacity[*parent]);
        opacity[at] = above * node.props.as_ref().and_then(|props| props.opacity).unwrap_or(1.0);
        let parent = node.parent.and_then(|id| positions.get(&id).copied());
        let area = border_box(node);
        let is_text = matches!(node.kind, Kind::Label | Kind::Paragraph) && node.text.is_some();
        // A headless capture does not resolve the effect states that count a
        // node's scrollers, so a scroll container is a node a scroll view made
        // or one whose children the layout has moved.
        let scrolls = node.title().contains("ScrollView") || node.used.offset != (0.0, 0.0);
        let mut flags = 0;
        if is_visible(node) {
            flags |= VISIBLE;
        }
        if paints(node) {
            flags |= PAINTS;
        }
        if is_text {
            flags |= TEXT;
        }
        if node.a11y.modal {
            flags |= MODAL;
        }
        if scrolls {
            flags |= SCROLLER;
        }
        if tree.is_wrapper(node.id) {
            flags |= WRAPPER;
        }
        if node.used.out_of_flow {
            flags |= OUT_OF_FLOW;
        }
        if node.a11y.inert {
            flags |= INERT;
        }
        if node.props.as_ref().is_some_and(|props| props.clip) {
            flags |= CLIP;
        }
        let text = node
            .text
            .as_deref()
            .filter(|_| is_text)
            .map_or(-1, |text| strings.put(text) as i64);
        nodes.push(json!([
            parent.map_or(-1, |parent| parent as i64),
            titles.put(node.title()),
            round(area[0]),
            round(area[1]),
            round(area[2]),
            round(area[3]),
            flags,
            text,
        ]));
        if is_text {
            // What the element declares; a style it inherits is not resolved in a
            // headless capture, so a colour or size that is not declared is
            // reported as zero and a check reads the pixels instead.
            let used = node.used.text.as_ref();
            let props = node.props.as_ref();
            let colour = props
                .and_then(|props| props.text_color)
                .or_else(|| used.and_then(|used| used.colour))
                .map_or([0, 0, 0, 0], |c| [c.r(), c.g(), c.b(), c.a()]);
            let size = props
                .and_then(|props| props.font_size)
                .or_else(|| used.map(|used| used.size))
                .unwrap_or(0.0);
            let weight = props
                .and_then(|props| props.font_weight)
                .map(i32::from)
                .or_else(|| used.map(|used| used.weight))
                .unwrap_or(400);
            texts.push(json!([
                at,
                colour,
                size,
                weight,
                opacity[at],
                used.and_then(|used| used.lines).unwrap_or(1),
            ]));
        }
        if scrolls {
            let (inner_w, inner_h) = node.used.inner_sizes.unwrap_or((0.0, 0.0));
            scrollers.push(json!({
                "node": at,
                "offset": [round(node.used.offset.0), round(node.used.offset.1)],
                "inner": [round(inner_w), round(inner_h)],
            }));
        }
        if node.a11y.modal && is_visible(node) {
            modals.push(modal(tree, &positions, node));
        }
    }
    let window = tree
        .roots
        .first()
        .and_then(|id| tree.node(*id))
        .map_or([0.0, 0.0], |root| {
            let area = border_box(root);
            [area[2], area[3]]
        });
    json!({
        "capture": capture,
        "scale": tree.scale,
        "window": window,
        "titles": titles.items,
        "strings": strings.items,
        "nodes": nodes,
        "texts": texts,
        "modals": modals,
        "scrollers": scrollers,
    })
}

/// The painted box a modal scope draws as: the outermost rounded, painted ancestor
/// that holds it (a popup's scrolling body is inside the card that carries the
/// border), else the scope itself when it paints, else the painted descendant
/// that fills it (an alert dialog's card is a child of its scope), else the scope.
fn card_of<'a>(tree: &'a Tree, node: &'a Node) -> &'a Node {
    let area = border_box(node);
    let window = tree
        .roots
        .first()
        .and_then(|id| tree.node(*id))
        .map_or([f32::MAX; 2], |root| {
            let root = border_box(root);
            [root[2], root[3]]
        });
    // The outermost painted ancestor that holds the scope and is a card, not the
    // window or a scrim: a popup's scrolling body is inside the card that has
    // the border and the header.
    let mut best: Option<&Node> = None;
    let mut up = node.parent;
    for _ in 0..10 {
        let Some(parent) = up.and_then(|id| tree.node(id)) else { break };
        let box4 = border_box(parent);
        let holds = box4[0] <= area[0] + 1.0
            && box4[1] <= area[1] + 1.0
            && box4[0] + box4[2] >= area[0] + area[2] - 1.0
            && box4[1] + box4[3] >= area[1] + area[3] - 1.0;
        let smaller = box4[2] < window[0] - 2.0 || box4[3] < window[1] - 2.0;
        let rounded = parent
            .props
            .as_ref()
            .is_some_and(|props| props.corner_radii.iter().any(|radius| *radius > 0.0) || !props.borders.is_empty());
        if paints(parent) && holds && smaller && rounded {
            best = Some(parent);
        }
        up = parent.parent;
    }
    if let Some(card) = best {
        return card;
    }
    if paints(node) {
        return node;
    }
    // An alert dialog's card is a child that fills its scope.
    let mut queue: Vec<&Node> = vec![node];
    let mut depth = 0;
    while !queue.is_empty() && depth < 4 {
        let mut next = Vec::new();
        for parent in queue {
            for id in &parent.children {
                let Some(child) = tree.node(*id) else { continue };
                let box4 = border_box(child);
                let same = (0..4).all(|i| (box4[i] - area[i]).abs() <= 1.5);
                if same && paints(child) {
                    return child;
                }
                next.push(child);
            }
        }
        queue = next;
        depth += 1;
    }
    node
}

/// One modal scope: the card it draws as, what the card paints, and whether the
/// header bars that sit on the card's top edge are inside it (BR-03: the border
/// is on the outermost container, header included).
fn modal(tree: &Tree, positions: &HashMap<u64, usize>, scope: &Node) -> Value {
    let node = card_of(tree, scope);
    let at = positions.get(&node.id).copied().unwrap_or(0);
    let area = border_box(node);
    let props = node.props.as_ref();
    // A header bar whose box starts within 2 px of the card's top edge and
    // lies inside its width is the card's header: it must be a descendant.
    let mut inside = 0;
    let mut outside = 0;
    for other in &tree.nodes {
        if other.a11y.component.as_deref() != Some("HeaderBar") || !is_visible(other) {
            continue;
        }
        let header = border_box(other);
        let touches = (header[1] - area[1]).abs() <= 2.0
            && header[0] >= area[0] - 2.0
            && header[0] + header[2] <= area[0] + area[2] + 2.0
            && header[2] >= area[2] * 0.5;
        if !touches || other.id == node.id {
            continue;
        }
        let mut up = other.parent;
        let mut descendant = false;
        while let Some(id) = up {
            if id == node.id {
                descendant = true;
                break;
            }
            up = tree.node(id).and_then(|parent| parent.parent);
        }
        if descendant {
            inside += 1;
        } else {
            outside += 1;
        }
    }
    json!({
        "node": at,
        "scope": positions.get(&scope.id).copied().unwrap_or(0),
        "title": node.title(),
        "radii": props.map_or([0.0; 4], |props| props.corner_radii),
        "borders": props.map_or(0, |props| props.borders.len()),
        "shadows": props.map_or(0, |props| props.shadows.len()),
        "headers_inside": inside,
        "headers_outside": outside,
    })
}

fn round(value: f32) -> f64 {
    (f64::from(value) * 100.0).round() / 100.0
}

impl Snapshot {
    /// With `BINGUX_UI_PROBE=FILE` set, appends the probe of this capture to
    /// FILE as a JSON line, after the PNG is written.
    pub(crate) fn lint_probe(&mut self, shot: &Path) {
        use std::io::Write;
        let Some(file) = std::env::var_os("BINGUX_UI_PROBE") else {
            return;
        };
        let scale = crate::snapshot::scale_factor() as f32;
        let tree = Tree::capture_headless(self.runner(), scale);
        let line = probe(&tree, &shot.to_string_lossy());
        if let Ok(mut out) = std::fs::OpenOptions::new().create(true).append(true).open(file) {
            let _ = writeln!(out, "{line}");
        }
    }
}
