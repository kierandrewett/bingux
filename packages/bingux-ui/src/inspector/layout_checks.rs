//! The Layout checks: visual defects found on the measured tree that have
//! nothing to do with assistive technology. A card with more space under its
//! content than above it, a button that is not centred in its field, two
//! buttons of one row at different heights, a panel with an empty area below
//! its last row are all things a person sees at once, and none of them is an
//! accessibility problem, so they are listed apart from the Accessibility
//! checks, in their own tab.
//!
//! Every rule is a pure function over a [`Snapshot`], so each can be tested on
//! a hand-made tree. [`check`] runs them all and is what the tab and
//! `scripts/layout-lint.py` use.
//!
//! The rules look for ink: text, images, and boxes that paint a fill or a
//! border. A box that only holds space, such as an empty line kept free for a
//! message, is not ink, so space that nothing will ever fill shows up as the
//! difference between where the ink is and where the padding says it should
//! be.

use std::collections::HashMap;

use super::a11y_checks::describe;
use super::snapshot::{Kind, Node, Snapshot};

/// The accuracy of the measurements, in logical pixels. Insets that differ
/// by no more than this are the same.
pub const TOLERANCE: f32 = 1.0;
/// How much empty room under the last child a filled box may have, in logical
/// pixels, before it is reported as taller than its content.
pub const DEAD_SPACE: f32 = 32.0;
/// How much two boxes must overlap, in logical pixels on each axis.
const OVERLAP_MIN: f32 = 2.0;
/// How much spare room text that is cut off must have beside it, in logical
/// pixels, before the cut is reported.
const TRUNCATED_ROOM: f32 = 4.0;
/// How far apart two same-size texts of a row may sit vertically.
const BASELINE_TOLERANCE: f32 = 1.5;
/// Boxes thinner than this are rules, scrollbars and tracks: they are not
/// content, so they do not count as ink.
const THIN: f32 = 8.0;

/// An area as `[x, y, width, height]` in logical pixels.
type Box4 = [f32; 4];

/// What a finding says is wrong.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Rule {
    UnevenPadding,
    OffCentre,
    UnequalSiblings,
    DeadSpace,
    Clipped,
    Overlap,
    Truncated,
    Baseline,
    Wrapped,
    Overflow,
    OutsideParent,
    HorizontalScroll,
    HiddenLabel,
    TextCollision,
    FocusRing,
}

impl Rule {
    /// In the order the tab lists the groups.
    pub const ALL: [Rule; 15] = [
        Rule::UnevenPadding,
        Rule::OffCentre,
        Rule::UnequalSiblings,
        Rule::DeadSpace,
        Rule::Clipped,
        Rule::Overlap,
        Rule::Truncated,
        Rule::Baseline,
        Rule::Wrapped,
        Rule::Overflow,
        Rule::OutsideParent,
        Rule::HorizontalScroll,
        Rule::HiddenLabel,
        Rule::TextCollision,
        Rule::FocusRing,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Rule::UnevenPadding => "Uneven padding",
            Rule::OffCentre => "Off centre",
            Rule::UnequalSiblings => "Unequal siblings",
            Rule::DeadSpace => "Dead space",
            Rule::Clipped => "Clipped",
            Rule::Overlap => "Overlap",
            Rule::Truncated => "Truncated",
            Rule::Baseline => "Baseline",
            Rule::Wrapped => "Broken text",
            Rule::Overflow => "Text overflow",
            Rule::OutsideParent => "Outside its parent",
            Rule::HorizontalScroll => "Sideways scroll",
            Rule::HiddenLabel => "Hidden label",
            Rule::TextCollision => "Text collision",
            Rule::FocusRing => "Focus ring off its face",
        }
    }

    /// The rule's key in the lint's output and its allowlist.
    pub fn key(self) -> &'static str {
        match self {
            Rule::UnevenPadding => "uneven-padding",
            Rule::OffCentre => "off-centre",
            Rule::UnequalSiblings => "unequal-siblings",
            Rule::DeadSpace => "dead-space",
            Rule::Clipped => "clipped",
            Rule::Overlap => "overlap",
            Rule::Truncated => "truncated",
            Rule::Baseline => "baseline",
            Rule::Wrapped => "wrapped",
            Rule::Overflow => "overflow",
            Rule::OutsideParent => "outside-parent",
            Rule::HorizontalScroll => "horizontal-scroll",
            Rule::HiddenLabel => "hidden-label",
            Rule::TextCollision => "text-collision",
            Rule::FocusRing => "focus-ring",
        }
    }

    /// What the rule holds a container to, said once for the group.
    pub fn why(self) -> &'static str {
        match self {
            Rule::UnevenPadding => {
                "A box whose padding is the same on both sides, with its content nearer one side, looks lopsided."
            }
            Rule::OffCentre => {
                "A control in a row that centres its items must sit on the row's centre line."
            }
            Rule::UnequalSiblings => {
                "Controls of one kind side by side stand on one height."
            }
            Rule::DeadSpace => {
                "A box with a fixed height and a lot of empty room under its last child looks unfinished."
            }
            Rule::Clipped => "Content outside the box that clips it is cut off.",
            Rule::Overlap => "Controls and text that overlap each other cannot all be read or hit.",
            Rule::Truncated => {
                "Text that is cut off while there is room beside it should have used the room."
            }
            Rule::Baseline => "Text of one size in one row shares a baseline.",
            Rule::Wrapped => {
                "Text breaks between words, never inside one and never a letter to a line, and keeps to the lines it is given."
            }
            Rule::Overflow => {
                "Text that does not fit its box ends in an ellipsis, so it is never cut off in the middle of a letter."
            }
            Rule::OutsideParent => {
                "A box stays inside the box that holds it, or it covers its neighbours."
            }
            Rule::HorizontalScroll => {
                "Content fits the width of the window and of its scroll view, so no one has to scroll sideways to read it."
            }
            Rule::HiddenLabel => "A control shows its label, so a person can tell what it does.",
            Rule::TextCollision => "Texts do not lie on each other, or neither can be read.",
            Rule::FocusRing => "A focus ring is drawn around the box the control paints, with the face's corner radius nested, so it always shows what has focus.",
        }
    }

    /// How to fix a finding of this rule.
    pub fn fix(self) -> &'static str {
        match self {
            Rule::UnevenPadding => {
                "Remove what takes space on one side (an empty line kept for a message, a margin) or give both sides the same inset token."
            }
            Rule::OffCentre => "Centre it on the cross axis, or size it to the row's content height.",
            Rule::UnequalSiblings => "Give them one height token.",
            Rule::DeadSpace => "Size the box to its content, or let it fill the space with what belongs there.",
            Rule::Clipped => "Make the box big enough, let it scroll, or shorten the content.",
            Rule::Overlap => "Lay the two out in a row or a column, or give them their own space.",
            Rule::Truncated => "Let the text take the free room (flex it) or give it a larger fixed width.",
            Rule::Baseline => "Align the texts to one cross-axis position, or lay them out in one text run.",
            Rule::Wrapped => {
                "Give the label its natural width (or stack it above its control) and let the control shrink first; set max_lines with an ellipsis for text that may be long."
            }
            Rule::Overflow => {
                "Let the text wrap, set max_lines with TextOverflow::Ellipsis, or give it the room it needs."
            }
            Rule::OutsideParent => {
                "Let the parent grow with its content, wrap or truncate the content, or stack the layout when it cannot fit."
            }
            Rule::HorizontalScroll => {
                "Find the child that is wider than its container and let it shrink, wrap or elide; a long unbroken string needs max_lines and an ellipsis."
            }
            Rule::HiddenLabel => "Give the control room for its label, or shorten the label or use an icon with a tooltip.",
            Rule::TextCollision => "Lay the two out in a row or a column, or give the first one a width limit.",
            Rule::FocusRing => "End the control's render with .focusable(..) on the node that paints its fill and its corner radii.",
        }
    }
}

/// One defect on the measured tree.
#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    pub rule: Rule,
    /// The node at fault; clicking the finding selects it.
    pub node: u64,
    /// The node named for a reader: its kind, id and name or text.
    pub target: String,
    /// The components the node is built in, outermost first, for the lint's
    /// allowlist.
    pub path: String,
    /// What was measured, in words.
    pub what: String,
    /// The same numbers, by name, in logical pixels.
    pub numbers: Vec<(&'static str, f32)>,
    /// The node's area, `[x, y, width, height]`.
    pub area: Box4,
}

impl Finding {
    /// The finding as the lint reads it: the rule's key, the node, where it
    /// is built, what was measured and where it is.
    #[cfg(feature = "testing")]
    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "rule": self.rule.key(),
            "node": self.node,
            "target": self.target,
            "path": self.path,
            "what": self.what,
            "numbers": self.numbers.iter().map(|(name, value)| (name.to_string(), *value)).collect::<std::collections::BTreeMap<_, _>>(),
            "area": self.area,
        })
    }
}

/// Runs every rule on `snapshot`. The result is sorted by rule, then node.
pub fn check(snapshot: &Snapshot) -> Vec<Finding> {
    let cx = Context::new(snapshot);
    let mut out = Vec::new();
    uneven_padding(&cx, &mut out);
    off_centre(&cx, &mut out);
    unequal_siblings(&cx, &mut out);
    dead_space(&cx, &mut out);
    clipped(&cx, &mut out);
    overlap(&cx, &mut out);
    truncated(&cx, &mut out);
    baseline(&cx, &mut out);
    wrapped(&cx, &mut out);
    overflow(&cx, &mut out);
    outside_parent(&cx, &mut out);
    horizontal_scroll(&cx, &mut out);
    hidden_label(&cx, &mut out);
    text_collision(&cx, &mut out);
    focus_ring(&cx, &mut out);
    out.sort_by_key(|finding| (finding.rule, finding.node));
    out
}

/// What the rules share: the tree with each node's index, the box of the ink
/// below it, and whether a scroll view holds it.
struct Context<'a> {
    snapshot: &'a Snapshot,
    index: HashMap<u64, usize>,
    /// The union of the ink boxes of everything below the node, itself not
    /// included.
    below: Vec<Option<Box4>>,
    /// The union of the node's own ink and the ink below it.
    within: Vec<Option<Box4>>,
    /// Whether the node or one of its ancestors is a scroll view, so its
    /// content moves and is cut by design.
    scrolled: Vec<bool>,
    /// The tree's first root, which is the window.
    window: Option<Box4>,
    /// The nearest ancestor (the node included) that the layout takes out of
    /// the flow, an overlay such as a menu or a popup, or 0 for the page
    /// itself. Nodes of different layers lie on each other by design.
    layer: Vec<u64>,
}

impl<'a> Context<'a> {
    fn new(snapshot: &'a Snapshot) -> Self {
        let nodes = &snapshot.nodes;
        let index: HashMap<u64, usize> = nodes
            .iter()
            .enumerate()
            .map(|(at, node)| (node.id, at))
            .collect();
        let count = nodes.len();
        let mut below: Vec<Option<Box4>> = vec![None; count];
        let mut within: Vec<Option<Box4>> = vec![None; count];
        // Preorder puts a parent before its children, so walking backwards
        // finishes every child before its parent.
        for at in (0..count).rev() {
            let own = is_ink(&nodes[at]).then(|| border_box(&nodes[at])).flatten();
            within[at] = union(own, below[at]);
            if let Some(parent) = nodes[at].parent.and_then(|id| index.get(&id)) {
                below[*parent] = union(below[*parent], within[at]);
            }
        }
        let mut scrolled = vec![false; count];
        for at in 0..count {
            let parent_scrolled = nodes[at]
                .parent
                .and_then(|id| index.get(&id))
                .is_some_and(|parent| scrolled[*parent]);
            scrolled[at] = parent_scrolled || nodes[at].a11y.role == "ScrollView";
        }
        let window = snapshot
            .roots
            .first()
            .and_then(|id| snapshot.node(*id))
            .and_then(border_box);
        let mut layer = vec![0u64; count];
        for at in 0..count {
            layer[at] = if nodes[at].used.out_of_flow {
                nodes[at].id
            } else {
                nodes[at]
                    .parent
                    .and_then(|id| index.get(&id))
                    .map_or(0, |parent| layer[*parent])
            };
        }
        Context {
            snapshot,
            index,
            below,
            within,
            scrolled,
            window,
            layer,
        }
    }

    fn parent_of(&self, node: &Node) -> Option<&'a Node> {
        node.parent.and_then(|id| self.snapshot.node(id))
    }

    fn children_of(&self, node: &Node) -> impl Iterator<Item = &'a Node> + use<'a> {
        let snapshot = self.snapshot;
        let ids = node.children.clone();
        ids.into_iter().filter_map(move |id| snapshot.node(id))
    }

    /// The names of the components the node is built in, outermost first.
    fn path_of(&self, node: &Node) -> String {
        let mut names = Vec::new();
        let mut current = Some(node);
        while let Some(here) = current {
            if let Some(component) = here.a11y.component.as_deref() {
                names.push(component);
            }
            current = self.parent_of(here);
        }
        names.reverse();
        names.join(" > ")
    }

    fn finding(
        &self,
        rule: Rule,
        node: &Node,
        what: String,
        numbers: Vec<(&'static str, f32)>,
    ) -> Finding {
        Finding {
            rule,
            node: node.id,
            target: describe(self.snapshot, Some(node.id)),
            path: self.path_of(node),
            what,
            numbers,
            area: border_box(node).unwrap_or([0.0; 4]),
        }
    }
}

/// The node's border box as `[x, y, width, height]`.
fn border_box(node: &Node) -> Option<Box4> {
    node.used
        .border
        .or(node.area)
        .map(|area| [area.min_x(), area.min_y(), area.width(), area.height()])
}

fn right(area: Box4) -> f32 {
    area[0] + area[2]
}

fn bottom(area: Box4) -> f32 {
    area[1] + area[3]
}

fn union(a: Option<Box4>, b: Option<Box4>) -> Option<Box4> {
    match (a, b) {
        (Some(a), Some(b)) => {
            let (x, y) = (a[0].min(b[0]), a[1].min(b[1]));
            let (r, d) = (right(a).max(right(b)), bottom(a).max(bottom(b)));
            Some([x, y, r - x, d - y])
        }
        (a, b) => a.or(b),
    }
}

/// The padding the layout applied, top, right, bottom and left.
fn padding_of(node: &Node) -> [f32; 4] {
    node.used
        .padding
        .or_else(|| node.props.as_ref().map(|props| props.padding))
        .unwrap_or([0.0; 4])
}

fn is_visible(node: &Node) -> bool {
    !node.used.hidden
        && node.used.opacity.is_none_or(|opacity| opacity > 0.0)
        && node
            .props
            .as_ref()
            .and_then(|props| props.opacity)
            .is_none_or(|opacity| opacity > 0.0)
        && border_box(node).is_some_and(|area| area[2] > 0.0 && area[3] > 0.0)
}

/// Whether the node paints a fill or a border.
fn paints(node: &Node) -> bool {
    node.props.as_ref().is_some_and(|props| {
        props.background.is_some_and(|colour| colour.a() != 0) || !props.borders.is_empty()
    })
}

/// Whether the node is something to look at: text, an image or an icon, or a
/// box that paints a fill or a border. Rules and scrollbars are too thin to
/// count.
fn is_ink(node: &Node) -> bool {
    if !is_visible(node) {
        return false;
    }
    let Some(area) = border_box(node) else {
        return false;
    };
    if area[2].min(area[3]) < THIN {
        return false;
    }
    match node.kind {
        Kind::Label | Kind::Paragraph => node
            .text
            .as_deref()
            .is_some_and(|text| !text.trim().is_empty()),
        Kind::Other => true,
        Kind::Rect => paints(node),
    }
}

fn is_row(node: &Node) -> bool {
    node.props
        .as_ref()
        .is_some_and(|props| props.direction == "horizontal")
}

fn cross_centred(node: &Node) -> bool {
    node.props
        .as_ref()
        .is_some_and(|props| props.cross_align == "center")
}

fn is_text(node: &Node) -> bool {
    matches!(node.kind, Kind::Label | Kind::Paragraph)
        && node
            .text
            .as_deref()
            .is_some_and(|text| !text.trim().is_empty())
}

/// A declared pixel height, `None` for `auto`, `fill`, a share or a flex.
fn fixed_height(node: &Node) -> Option<f32> {
    node.props
        .as_ref()
        .and_then(|props| props.height.parse::<f32>().ok())
}

/// A box that has the same padding on both sides of an axis says the content
/// is meant to sit in the middle of it. When the box is as big as its
/// content and its padding, nothing else decides where the content sits, so
/// the ink should have the same room on both sides.
fn uneven_padding(cx: &Context, out: &mut Vec<Finding>) {
    for (at, node) in cx.snapshot.nodes.iter().enumerate() {
        if node.kind != Kind::Rect || node.children.is_empty() || cx.scrolled[at] {
            continue;
        }
        let (Some(area), Some(ink), Some((inner_width, inner_height))) =
            (border_box(node), cx.below[at], node.used.inner_sizes)
        else {
            continue;
        };
        if !is_visible(node) {
            continue;
        }
        let [top, right_pad, bottom_pad, left] = padding_of(node);
        // Vertical.
        if top > 0.0
            && (top - bottom_pad).abs() <= TOLERANCE
            && (area[3] - (inner_height + top + bottom_pad)).abs() <= TOLERANCE
        {
            let above = ink[1] - area[1];
            let below = bottom(area) - bottom(ink);
            if (above - below).abs() > TOLERANCE {
                out.push(cx.finding(
                    Rule::UnevenPadding,
                    node,
                    format!(
                        "Padding is {top:.0} px above and below, but the content is {above:.0} px from the top and {below:.0} px from the bottom: {:.0} px uneven",
                        (above - below).abs()
                    ),
                    vec![
                        ("padding", top),
                        ("inset-top", above),
                        ("inset-bottom", below),
                        ("difference", (above - below).abs()),
                    ],
                ));
            }
        }
        // Horizontal.
        if left > 0.0
            && (left - right_pad).abs() <= TOLERANCE
            && (area[2] - (inner_width + left + right_pad)).abs() <= TOLERANCE
        {
            let before = ink[0] - area[0];
            let after = right(area) - right(ink);
            if (before - after).abs() > TOLERANCE {
                out.push(cx.finding(
                    Rule::UnevenPadding,
                    node,
                    format!(
                        "Padding is {left:.0} px left and right, but the content is {before:.0} px from the left and {after:.0} px from the right: {:.0} px uneven",
                        (before - after).abs()
                    ),
                    vec![
                        ("padding", left),
                        ("inset-left", before),
                        ("inset-right", after),
                        ("difference", (before - after).abs()),
                    ],
                ));
            }
        }
    }
}

/// In a row that centres its items, each item sits on the row's centre line
/// (and in a column that does, on its centre column).
fn off_centre(cx: &Context, out: &mut Vec<Finding>) {
    for (at, node) in cx.snapshot.nodes.iter().enumerate() {
        if node.kind != Kind::Rect || !cross_centred(node) || cx.scrolled[at] {
            continue;
        }
        let Some(content) = node
            .used
            .content
            .map(|area| [area.min_x(), area.min_y(), area.width(), area.height()])
        else {
            continue;
        };
        let row = is_row(node);
        for child in cx.children_of(node) {
            let Some(area) = border_box(child) else {
                continue;
            };
            let at_child = cx.index[&child.id];
            if cx.within[at_child].is_none() || !is_visible(child) {
                continue;
            }
            let (centre, line, name) = if row {
                (area[1] + area[3] / 2.0, content[1] + content[3] / 2.0, "centre-y")
            } else {
                (area[0] + area[2] / 2.0, content[0] + content[2] / 2.0, "centre-x")
            };
            let off = centre - line;
            if off.abs() > TOLERANCE {
                out.push(cx.finding(
                    Rule::OffCentre,
                    child,
                    format!(
                        "Its centre is {:.0} px {} the {} of its container, which centres its items",
                        off.abs(),
                        match (row, off > 0.0) {
                            (true, true) => "below",
                            (true, false) => "above",
                            (false, true) => "right of",
                            (false, false) => "left of",
                        },
                        if row { "centre line" } else { "centre column" },
                    ),
                    vec![(name, centre), ("container", line), ("off", off.abs())],
                ));
            }
        }
    }
}

/// In a row, siblings of one component that paint a fill or take focus (the
/// buttons of a toolbar, the fields of a form line) have one height.
fn unequal_siblings(cx: &Context, out: &mut Vec<Finding>) {
    for (at, node) in cx.snapshot.nodes.iter().enumerate() {
        if node.kind != Kind::Rect || !is_row(node) || cx.scrolled[at] {
            continue;
        }
        let mut kinds: Vec<(&str, Vec<(&Node, f32)>)> = Vec::new();
        for child in cx.children_of(node) {
            let Some(name) = child.a11y.component.as_deref() else {
                continue;
            };
            let Some(area) = border_box(child) else {
                continue;
            };
            if !is_visible(child) || !(paints(child) || child.a11y.focusable == Some(true)) {
                continue;
            }
            match kinds.iter_mut().find(|(known, _)| *known == name) {
                Some((_, members)) => members.push((child, area[3])),
                None => kinds.push((name, vec![(child, area[3])])),
            }
        }
        for (name, members) in kinds {
            let tallest = members.iter().map(|(_, height)| *height).fold(0.0, f32::max);
            for (child, height) in &members {
                if tallest - height > TOLERANCE {
                    out.push(cx.finding(
                        Rule::UnequalSiblings,
                        child,
                        format!(
                            "{name} is {height:.0} px tall beside a {name} that is {tallest:.0} px"
                        ),
                        vec![("height", *height), ("tallest", tallest)],
                    ));
                }
            }
        }
    }
}

/// A box that paints, has a height of its own and has far more room under
/// its last child than that child needs.
fn dead_space(cx: &Context, out: &mut Vec<Finding>) {
    for (at, node) in cx.snapshot.nodes.iter().enumerate() {
        if node.kind != Kind::Rect || node.parent.is_none() || cx.scrolled[at] {
            continue;
        }
        let (Some(area), Some(ink), Some(declared)) =
            (border_box(node), cx.below[at], fixed_height(node))
        else {
            continue;
        };
        if !paints(node) || !is_visible(node) {
            continue;
        }
        // A box as big as the window is the window's own surface.
        if cx
            .window
            .is_some_and(|window| area[2] >= window[2] * 0.98 && area[3] >= window[3] * 0.98)
        {
            continue;
        }
        let padding_below = padding_of(node)[2];
        let empty = bottom(area) - padding_below - bottom(ink);
        if empty > DEAD_SPACE {
            out.push(cx.finding(
                Rule::DeadSpace,
                node,
                format!(
                    "{empty:.0} px empty under the last content: {declared:.0} px tall, content ends at {:.0} px",
                    bottom(ink) - area[1]
                ),
                vec![
                    ("height", declared),
                    ("content-bottom", bottom(ink) - area[1]),
                    ("empty", empty),
                ],
            ));
        }
    }
}

/// Ink that reaches outside a box that clips it, with no scroll view between.
fn clipped(cx: &Context, out: &mut Vec<Finding>) {
    for (at, node) in cx.snapshot.nodes.iter().enumerate() {
        if !is_ink(node) || cx.scrolled[at] {
            continue;
        }
        let Some(area) = border_box(node) else {
            continue;
        };
        let mut current = cx.parent_of(node);
        while let Some(clip) = current {
            if clip.a11y.role == "ScrollView" {
                break;
            }
            if clip.props.as_ref().is_some_and(|props| props.clip)
                && let Some(frame) = border_box(clip)
            {
                let cut = [
                    frame[0] - area[0],
                    frame[1] - area[1],
                    right(area) - right(frame),
                    bottom(area) - bottom(frame),
                ];
                let worst = cut.iter().copied().fold(0.0, f32::max);
                if worst > TOLERANCE {
                    let side = ["left", "top", "right", "bottom"][cut
                        .iter()
                        .position(|edge| *edge == worst)
                        .unwrap_or(0)];
                    out.push(cx.finding(
                        Rule::Clipped,
                        node,
                        format!(
                            "{worst:.0} px of it is outside {} on the {side}, and cut off",
                            describe(cx.snapshot, Some(clip.id))
                        ),
                        vec![("cut", worst)],
                    ));
                    break;
                }
            }
            current = cx.parent_of(clip);
        }
    }
}

/// Two siblings with ink that overlap each other partly. One lying wholly
/// on the other is a layer (a fill, a ring, a scrim), not a collision.
fn overlap(cx: &Context, out: &mut Vec<Finding>) {
    for (at, node) in cx.snapshot.nodes.iter().enumerate() {
        if node.kind != Kind::Rect || node.children.len() < 2 || cx.scrolled[at] {
            continue;
        }
        let siblings: Vec<(&Node, Box4)> = cx
            .children_of(node)
            .filter(|child| {
                cx.within[cx.index[&child.id]].is_some()
                    && is_visible(child)
                    && !child.used.out_of_flow
            })
            .filter_map(|child| border_box(child).map(|area| (child, area)))
            .collect();
        for (first, (a, area_a)) in siblings.iter().enumerate() {
            for (b, area_b) in &siblings[first + 1..] {
                let width = right(*area_a).min(right(*area_b)) - area_a[0].max(area_b[0]);
                let height = bottom(*area_a).min(bottom(*area_b)) - area_a[1].max(area_b[1]);
                if width <= OVERLAP_MIN || height <= OVERLAP_MIN {
                    continue;
                }
                if contains(*area_a, *area_b) || contains(*area_b, *area_a) {
                    continue;
                }
                out.push(cx.finding(
                    Rule::Overlap,
                    b,
                    format!(
                        "Overlaps {} by {width:.0} × {height:.0} px",
                        describe(cx.snapshot, Some(a.id))
                    ),
                    vec![("width", width), ("height", height)],
                ));
            }
        }
    }
}

/// Whether `outer` holds `inner`, give or take the tolerance.
fn contains(outer: Box4, inner: Box4) -> bool {
    inner[0] >= outer[0] - TOLERANCE
        && inner[1] >= outer[1] - TOLERANCE
        && right(inner) <= right(outer) + TOLERANCE
        && bottom(inner) <= bottom(outer) + TOLERANCE
}

/// One-line text that is narrower than it needs to be while its container has
/// room to spare.
fn truncated(cx: &Context, out: &mut Vec<Finding>) {
    for node in &cx.snapshot.nodes {
        if !is_text(node) || !is_visible(node) {
            continue;
        }
        let (Some(text), Some(area), Some(parent)) =
            (node.used.text.as_ref(), border_box(node), cx.parent_of(node))
        else {
            continue;
        };
        let (Some(needed), Some(lines)) = (text.intrinsic, text.lines) else {
            continue;
        };
        if lines != 1 || needed <= area[2] + TOLERANCE {
            continue;
        }
        let Some(content) = parent.used.content else {
            continue;
        };
        let room = if is_row(parent) {
            let used: f32 = cx
                .children_of(parent)
                .filter_map(border_box)
                .map(|child| child[2])
                .sum();
            let count = cx.children_of(parent).count().saturating_sub(1) as f32;
            let gap = parent.props.as_ref().map_or(0.0, |props| props.gap);
            content.width() - used - gap * count
        } else {
            content.width() - area[2]
        };
        if room > TRUNCATED_ROOM {
            out.push(cx.finding(
                Rule::Truncated,
                node,
                format!(
                    "The text needs {needed:.0} px and has {:.0} px, while its container has {room:.0} px to spare",
                    area[2]
                ),
                vec![("needed", needed), ("width", area[2]), ("spare", room)],
            ));
        }
    }
}

/// The text of a row's child: the child itself if it is text, else the only
/// text below it. A child with several texts stacks them, and a stack is
/// centred as a block, not by its baseline.
fn lead_text<'a>(cx: &Context<'a>, child: &'a Node) -> Option<&'a Node> {
    if is_text(child) {
        return Some(child);
    }
    let mut found = None;
    let mut stack = vec![child];
    while let Some(here) = stack.pop() {
        for below in cx.children_of(here) {
            if is_text(below) {
                if found.is_some() {
                    return None;
                }
                found = Some(below);
            } else {
                stack.push(below);
            }
        }
    }
    found
}

/// In a row, the single-line texts of one size sit at one height.
fn baseline(cx: &Context, out: &mut Vec<Finding>) {
    for (at, node) in cx.snapshot.nodes.iter().enumerate() {
        if node.kind != Kind::Rect || !is_row(node) || cx.scrolled[at] {
            continue;
        }
        let texts: Vec<(&Node, Box4, f32)> = cx
            .children_of(node)
            .filter(|child| is_visible(child))
            .filter_map(|child| lead_text(cx, child))
            .filter(|text| {
                text.used
                    .text
                    .as_ref()
                    .is_some_and(|style| style.lines == Some(1))
            })
            .filter_map(|text| {
                let line = text.used.text.as_ref()?.measured?.1;
                Some((text, border_box(text)?, line))
            })
            .collect();
        for (first, (a, area_a, line_a)) in texts.iter().enumerate() {
            for (b, area_b, line_b) in &texts[first + 1..] {
                if (line_a - line_b).abs() > 0.5 {
                    continue;
                }
                let off = (area_a[1] - area_b[1]).abs();
                if off > BASELINE_TOLERANCE {
                    out.push(cx.finding(
                        Rule::Baseline,
                        b,
                        format!(
                            "Its text sits {off:.1} px {} the text {} of the same size in its row",
                            if area_b[1] > area_a[1] { "lower than" } else { "higher than" },
                            describe(cx.snapshot, Some(a.id))
                        ),
                        vec![("off", off), ("line-height", *line_a)],
                    ));
                }
            }
        }
    }
}

/// A text with more characters than this is long enough for a line of one or
/// two letters to mean the text was squeezed.
const LONG_TEXT: usize = 8;
/// A line this short or shorter, before the last one of a long text, is a
/// letter-to-a-line symptom.
const SHORT_LINE: usize = 2;
/// A text box narrower than this many average glyphs cannot show a word.
const NARROW_GLYPHS: f32 = 2.0;
/// A control keeps its label when at least this much of it shows.
const LABEL_SHOWN: f32 = 0.5;

/// The width of a text node's content box: its border box less its padding.
fn content_width(node: &Node) -> f32 {
    let [_, right_pad, _, left_pad] = padding_of(node);
    border_box(node).map_or(0.0, |area| (area[2] - right_pad - left_pad).max(0.0))
}

/// Whether the text ends in an ellipsis when it does not fit.
fn ellipsised(node: &Node) -> bool {
    node.used.text.as_ref().is_some_and(|text| text.wrap.ellipsis)
}

/// The box the text's glyphs cover: its own box narrowed to the width of its
/// longest line, placed by the alignment.
fn ink_box(node: &Node) -> Option<Box4> {
    let area = border_box(node)?;
    let [top, right_pad, bottom_pad, left_pad] = padding_of(node);
    let inner = [
        area[0] + left_pad,
        area[1] + top,
        (area[2] - left_pad - right_pad).max(0.0),
        (area[3] - top - bottom_pad).max(0.0),
    ];
    let Some(text) = node.used.text.as_ref() else {
        return Some(inner);
    };
    let Some((line, height)) = text.measured else {
        return Some(inner);
    };
    let width = line.min(inner[2]);
    let x = match text.align {
        "center" => inner[0] + (inner[2] - width) / 2.0,
        "right" | "end" => inner[0] + inner[2] - width,
        _ => inner[0],
    };
    Some([x, inner[1], width, height.min(inner[3])])
}

/// Text that the layout broke badly: a letter to a line, a box narrower than
/// two glyphs, a word split in the middle without an ellipsis, or more lines
/// than the element allows. A label whose box is a few pixels wide beside a
/// control with a fixed width is the usual cause.
fn wrapped(cx: &Context, out: &mut Vec<Finding>) {
    for node in &cx.snapshot.nodes {
        if !is_text(node) || !is_visible(node) {
            continue;
        }
        let (Some(text), Some(string)) = (node.used.text.as_ref(), node.text.as_deref()) else {
            continue;
        };
        let Some(lines) = text.lines else { continue };
        let chars = string.chars().count();
        let width = content_width(node);
        let average = text.intrinsic.map_or(0.0, |whole| whole / chars.max(1) as f32);
        let hard_breaks = string.contains('\n');
        let letter_lines = text.wrap.line_chars.len().saturating_sub(1);
        let shortest = text.wrap.line_chars.iter().take(letter_lines).copied().min();
        let mut numbers = vec![("lines", lines as f32), ("chars", chars as f32), ("width", width)];
        let what = if let Some(max) = text.wrap.max_lines.filter(|max| lines > *max) {
            numbers.push(("max-lines", max as f32));
            format!("The text has {lines} lines but is allowed {max}")
        } else if chars > LONG_TEXT
            && !hard_breaks
            && let Some(short) = shortest.filter(|short| *short <= SHORT_LINE)
        {
            numbers.push(("shortest-line", short as f32));
            format!(
                "A letter to a line: {chars} characters in {lines} lines, one of {short} characters, in a box {width:.0} px wide"
            )
        } else if chars > LONG_TEXT && average > 0.0 && width < NARROW_GLYPHS * average {
            numbers.push(("glyph", average));
            format!(
                "The box is {width:.0} px wide, less than two glyphs ({average:.1} px each) of {chars} characters"
            )
        } else if lines > 1
            && !ellipsised(node)
            && let Some(word) = text.wrap.longest_word.filter(|word| *word > width + TOLERANCE)
        {
            numbers.push(("longest-word", word));
            format!(
                "A word is broken in the middle: the longest word needs {word:.0} px and the text has {width:.0} px"
            )
        } else {
            continue;
        };
        out.push(cx.finding(Rule::Wrapped, node, what, numbers));
    }
}

/// One-line text without an ellipsis that is wider than its own box, and so is
/// cut off in the middle of a letter. Text that runs past a box that clips it
/// is the Clipped rule's.
fn overflow(cx: &Context, out: &mut Vec<Finding>) {
    for node in &cx.snapshot.nodes {
        if !is_text(node) || !is_visible(node) || ellipsised(node) {
            continue;
        }
        let Some(text) = node.used.text.as_ref() else {
            continue;
        };
        let (Some(needed), Some(1)) = (text.intrinsic, text.lines) else {
            continue;
        };
        let width = content_width(node);
        if needed > width + TOLERANCE {
            out.push(cx.finding(
                Rule::Overflow,
                node,
                format!(
                    "The text needs {needed:.0} px, its box is {width:.0} px wide, and it has no ellipsis"
                ),
                vec![("needed", needed), ("width", width)],
            ));
        }
    }
}

/// A box that reaches outside the box that holds it, so it covers what is
/// next to its parent. Overlays and boxes a scroll view holds are left out
/// along the scroll axis.
fn outside_parent(cx: &Context, out: &mut Vec<Finding>) {
    for node in &cx.snapshot.nodes {
        if !is_ink(node) || node.used.out_of_flow {
            continue;
        }
        let (Some(area), Some(parent)) = (border_box(node), cx.parent_of(node)) else {
            continue;
        };
        let Some(frame) = border_box(parent) else {
            continue;
        };
        if parent.kind != Kind::Rect || !is_visible(parent) {
            continue;
        }
        let scrolls = parent.a11y.role == "ScrollView";
        let cut = [
            frame[0] - area[0],
            if scrolls { 0.0 } else { frame[1] - area[1] },
            right(area) - right(frame),
            if scrolls { 0.0 } else { bottom(area) - bottom(frame) },
        ];
        let worst = cut.iter().copied().fold(0.0, f32::max);
        if worst > OVERLAP_MIN {
            let side = ["left", "top", "right", "bottom"][cut
                .iter()
                .position(|edge| *edge == worst)
                .unwrap_or(0)];
            out.push(cx.finding(
                Rule::OutsideParent,
                node,
                format!(
                    "{worst:.0} px of it lies outside {} on the {side}",
                    describe(cx.snapshot, Some(parent.id))
                ),
                vec![("outside", worst)],
            ));
        }
    }
}

/// Content wider than the window, or than the scroll view that holds it, so a
/// person has to scroll sideways.
fn horizontal_scroll(cx: &Context, out: &mut Vec<Finding>) {
    for (at, node) in cx.snapshot.nodes.iter().enumerate() {
        if !is_visible(node) {
            continue;
        }
        if node.a11y.role == "ScrollView" {
            let (Some(area), Some((inner, _))) = (border_box(node), node.used.inner_sizes) else {
                continue;
            };
            let [_, right_pad, _, left_pad] = padding_of(node);
            let view = area[2] - right_pad - left_pad;
            if inner > view + OVERLAP_MIN {
                out.push(cx.finding(
                    Rule::HorizontalScroll,
                    node,
                    format!("Its content is {inner:.0} px wide in a view {view:.0} px wide, so it scrolls sideways"),
                    vec![("content", inner), ("view", view)],
                ));
            }
            continue;
        }
        // Outside any scroll view, the outermost box that is wider than the
        // window.
        let (Some(window), Some(area)) = (cx.window, border_box(node)) else {
            continue;
        };
        if cx.scrolled[at] || node.used.out_of_flow || cx.layer[at] != 0 || !is_ink(node) {
            continue;
        }
        let past = right(area) - right(window);
        let parent_past = cx
            .parent_of(node)
            .and_then(border_box)
            .map_or(0.0, |parent| right(parent) - right(window));
        if past > OVERLAP_MIN && parent_past <= OVERLAP_MIN {
            out.push(cx.finding(
                Rule::HorizontalScroll,
                node,
                format!("It ends {past:.0} px past the right edge of the window"),
                vec![("past", past)],
            ));
        }
    }
}

/// A control whose text is out of sight: hidden, collapsed, or outside the
/// box that clips it.
fn hidden_label(cx: &Context, out: &mut Vec<Finding>) {
    for node in &cx.snapshot.nodes {
        if node.a11y.focusable != Some(true) || !is_visible(node) {
            continue;
        }
        let Some(frame) = border_box(node) else {
            continue;
        };
        let mut shown = Vec::new();
        let mut stack: Vec<&Node> = cx.children_of(node).collect();
        while let Some(here) = stack.pop() {
            if is_text(here) {
                shown.push(shown_of(cx, here, frame));
            } else {
                stack.extend(cx.children_of(here));
            }
        }
        if shown.is_empty() || shown.iter().any(|fraction| *fraction >= LABEL_SHOWN) {
            continue;
        }
        let best = shown.iter().copied().fold(0.0, f32::max);
        out.push(cx.finding(
            Rule::HiddenLabel,
            node,
            format!(
                "None of its {} text{} shows: the most visible is {:.0}% of it",
                shown.len(),
                if shown.len() == 1 { "" } else { "s" },
                best * 100.0
            ),
            vec![("shown", best), ("texts", shown.len() as f32)],
        ));
    }
}

/// How much of a text's width shows inside `frame` and the boxes that clip
/// it, as a fraction of the width it needs. Text that is hidden or has no
/// width shows none of it.
fn shown_of(cx: &Context, text: &Node, frame: Box4) -> f32 {
    if !is_visible(text) {
        return 0.0;
    }
    let Some(area) = border_box(text) else {
        return 0.0;
    };
    let needed = text
        .used
        .text
        .as_ref()
        .filter(|used| used.lines == Some(1))
        .and_then(|used| used.intrinsic)
        .unwrap_or(area[2])
        .max(1.0);
    let (mut left, mut edge) = (area[0], right(area));
    let mut narrow = |frame: Box4| {
        left = left.max(frame[0]);
        edge = edge.min(right(frame));
    };
    narrow(frame);
    let mut current = cx.parent_of(text);
    while let Some(clip) = current {
        if clip.props.as_ref().is_some_and(|props| props.clip)
            && let Some(clip_frame) = border_box(clip)
        {
            narrow(clip_frame);
        }
        current = cx.parent_of(clip);
    }
    ((edge - left).max(0.0) / needed).min(1.0)
}

/// Two texts in unrelated boxes whose glyphs lie on each other: a label that
/// overflowed into the next row. Texts of one parent are the Overlap rule's.
fn text_collision(cx: &Context, out: &mut Vec<Finding>) {
    let texts: Vec<(usize, &Node, Box4)> = cx
        .snapshot
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| is_text(node) && is_visible(node))
        .filter_map(|(at, node)| ink_box(node).map(|ink| (at, node, ink)))
        .collect();
    for (first, (at_a, a, ink_a)) in texts.iter().enumerate() {
        for (at_b, b, ink_b) in &texts[first + 1..] {
            if cx.layer[*at_a] != cx.layer[*at_b] || a.parent == b.parent {
                continue;
            }
            let width = right(*ink_a).min(right(*ink_b)) - ink_a[0].max(ink_b[0]);
            let height = bottom(*ink_a).min(bottom(*ink_b)) - ink_a[1].max(ink_b[1]);
            if width <= OVERLAP_MIN || height <= OVERLAP_MIN {
                continue;
            }
            out.push(cx.finding(
                Rule::TextCollision,
                b,
                format!(
                    "Its text lies on the text {} by {width:.0} × {height:.0} px",
                    describe(cx.snapshot, Some(a.id))
                ),
                vec![("width", width), ("height", height)],
            ));
        }
    }
}

/// A focus ring that is not drawn around the box its control paints. The ring
/// is measured from its face after layout, so a ring that is off its face
/// means the control wrapped the wrong node, or its wrapper is not the face's
/// size. The same rule is `scripts/focus-ring-check.py`, through
/// [`crate::focus::ring_check`].
fn focus_ring(cx: &Context, out: &mut Vec<Finding>) {
    use crate::focus::ring_check::{check, probes};
    for (id, probe) in probes(cx.snapshot) {
        let Some(ring) = cx.snapshot.node(id) else {
            continue;
        };
        for mismatch in check(std::slice::from_ref(&probe)) {
            let [ex, ey, ew, eh] = mismatch.expected;
            let [fx, fy, fw, fh] = mismatch.found;
            out.push(cx.finding(
                Rule::FocusRing,
                ring,
                format!("{}: {}", probe.label, mismatch.what),
                vec![
                    ("expected x", ex),
                    ("expected y", ey),
                    ("expected width", ew),
                    ("expected height", eh),
                    ("found x", fx),
                    ("found y", fy),
                    ("found width", fw),
                    ("found height", fh),
                ],
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use freya::torin::prelude::{Area, Point2D, Size2D};

    use super::*;
    use crate::inspector::snapshot::{Props, UsedText};

    /// A hand-made tree: nodes are added with the box the layout would give
    /// them, in preorder, and linked to their parent.
    #[derive(Default)]
    struct Tree {
        nodes: Vec<Node>,
    }

    fn area(x: f32, y: f32, w: f32, h: f32) -> Area {
        Area::new(Point2D::new(x, y), Size2D::new(w, h))
    }

    impl Tree {
        /// A rect with the given box. `inner` is what its children add up
        /// to, `padding` is top, right, bottom and left.
        fn rect(&mut self, parent: Option<u64>, (x, y, w, h): (f32, f32, f32, f32)) -> u64 {
            let id = self.nodes.len() as u64 + 1;
            let mut node = Node {
                id,
                parent,
                kind: Kind::Rect,
                text: None,
                area: Some(area(x, y, w, h)),
                used: Default::default(),
                children: Vec::new(),
                props: Some(Props {
                    direction: "vertical",
                    ..Props::default()
                }),
                a11y: Default::default(),
                repeats_fill: false,
            };
            node.used.border = node.area;
            node.used.set_padding([0.0; 4]);
            node.used.inner_sizes = Some((w, h));
            self.nodes.push(node);
            if let Some(parent) = parent {
                self.nodes[parent as usize - 1].children.push(id);
            }
            id
        }

        fn at(&mut self, id: u64) -> &mut Node {
            &mut self.nodes[id as usize - 1]
        }

        fn row(&mut self, id: u64) -> u64 {
            self.at(id).props.as_mut().unwrap().direction = "horizontal";
            id
        }

        fn centred(&mut self, id: u64) -> u64 {
            self.at(id).props.as_mut().unwrap().cross_align = "center";
            id
        }

        fn fill(&mut self, id: u64) -> u64 {
            self.at(id).props.as_mut().unwrap().background =
                Some(0xFF28_2828_u32.into());
            id
        }

        fn pad(&mut self, id: u64, padding: [f32; 4]) -> u64 {
            self.at(id).used.set_padding(padding);
            id
        }

        fn inner(&mut self, id: u64, width: f32, height: f32) -> u64 {
            self.at(id).used.inner_sizes = Some((width, height));
            id
        }

        fn fixed_height(&mut self, id: u64, height: f32) -> u64 {
            self.at(id).props.as_mut().unwrap().height = format!("{height}");
            id
        }

        fn text(&mut self, parent: u64, text: &str, (x, y, w, h): (f32, f32, f32, f32), intrinsic: f32) -> u64 {
            let id = self.rect(Some(parent), (x, y, w, h));
            let node = self.at(id);
            node.kind = Kind::Label;
            node.text = Some(text.to_string());
            node.used.text = Some(UsedText {
                families: Vec::new(),
                size: 14.0,
                weight: 400,
                slant: "upright",
                width: 5,
                letter_spacing: 0.0,
                colour: None,
                align: "left",
                lines: Some(1),
                measured: Some((w.min(intrinsic), h)),
                intrinsic: Some(intrinsic),
                wrap: Default::default(),
            });
            id
        }

        /// A text that broke into `line_chars.len()` lines, the longest word
        /// of which is `longest_word` px wide.
        fn broken_text(
            &mut self,
            parent: u64,
            text: &str,
            rect: (f32, f32, f32, f32),
            intrinsic: f32,
            line_chars: &[usize],
            longest_word: f32,
        ) -> u64 {
            let id = self.text(parent, text, rect, intrinsic);
            let used = self.at(id).used.text.as_mut().unwrap();
            used.lines = Some(line_chars.len());
            used.wrap.line_chars = line_chars.to_vec();
            used.wrap.longest_word = Some(longest_word);
            id
        }

        fn clip(&mut self, id: u64) -> u64 {
            self.at(id).props.as_mut().unwrap().clip = true;
            id
        }

        fn out_of_flow(&mut self, id: u64) -> u64 {
            self.at(id).used.out_of_flow = true;
            id
        }

        fn focusable(&mut self, id: u64) -> u64 {
            self.at(id).a11y.focusable = Some(true);
            id
        }

        fn snapshot(self) -> Snapshot {
            Snapshot::from_nodes(vec![1], self.nodes)
        }
    }

    fn rules(snapshot: &Snapshot) -> Vec<Rule> {
        check(snapshot).iter().map(|finding| finding.rule).collect()
    }

    /// The Wi-Fi password row as it was: padding of 12 above and below, the
    /// field at the top and an empty line kept free for a message under it.
    fn form_row(reserved: f32) -> Snapshot {
        let mut tree = Tree::default();
        let height = 12.0 + 36.0 + reserved + 12.0;
        let outer = tree.rect(None, (0.0, 0.0, 400.0, height));
        tree.pad(outer, [12.0, 0.0, 12.0, 0.0]);
        tree.inner(outer, 400.0, 36.0 + reserved);
        let field = tree.rect(Some(outer), (0.0, 12.0, 400.0, 36.0));
        tree.fill(field);
        if reserved > 0.0 {
            tree.rect(Some(outer), (0.0, 48.0, 400.0, reserved));
        }
        tree.snapshot()
    }

    #[test]
    fn a_reserved_empty_line_makes_the_padding_uneven() {
        let findings = check(&form_row(20.0));
        let [finding] = findings.as_slice() else {
            panic!("expected one finding, got {findings:?}");
        };
        assert_eq!(finding.rule, Rule::UnevenPadding);
        assert_eq!(finding.node, 1);
        assert!(finding.numbers.contains(&("inset-top", 12.0)));
        assert!(finding.numbers.contains(&("inset-bottom", 32.0)));
        assert!(finding.what.contains("20 px uneven"), "{}", finding.what);
    }

    #[test]
    fn even_padding_is_clean() {
        assert_eq!(rules(&form_row(0.0)), vec![]);
    }

    #[test]
    fn a_fixed_height_box_with_room_under_its_content_is_dead_space() {
        let mut tree = Tree::default();
        let root = tree.rect(None, (0.0, 0.0, 400.0, 600.0));
        let panel = tree.rect(Some(root), (0.0, 0.0, 400.0, 340.0));
        tree.fill(panel);
        tree.fixed_height(panel, 340.0);
        let hint = tree.text(panel, "Passwords are case sensitive.", (16.0, 190.0, 300.0, 16.0), 300.0);
        let _ = hint;
        let findings = check(&tree.snapshot());
        let dead: Vec<_> = findings
            .iter()
            .filter(|finding| finding.rule == Rule::DeadSpace)
            .collect();
        assert_eq!(dead.len(), 1, "{findings:?}");
        assert_eq!(dead[0].node, panel);
        assert_eq!(dead[0].numbers[2], ("empty", 134.0));
    }

    #[test]
    fn a_box_that_fits_its_content_has_no_dead_space() {
        let mut tree = Tree::default();
        let root = tree.rect(None, (0.0, 0.0, 400.0, 600.0));
        let panel = tree.rect(Some(root), (0.0, 0.0, 400.0, 222.0));
        tree.fill(panel);
        tree.fixed_height(panel, 222.0);
        tree.text(panel, "Passwords are case sensitive.", (16.0, 190.0, 300.0, 16.0), 300.0);
        assert!(!rules(&tree.snapshot()).contains(&Rule::DeadSpace));
    }

    #[test]
    fn a_control_below_the_centre_line_of_a_centred_row_is_off_centre() {
        let mut tree = Tree::default();
        let field = tree.rect(None, (0.0, 0.0, 300.0, 36.0));
        tree.row(field);
        tree.centred(field);
        let text = tree.text(field, "Password", (12.0, 10.0, 200.0, 16.0), 80.0);
        let button = tree.rect(Some(field), (264.0, 8.0, 28.0, 28.0));
        tree.fill(button);
        let _ = text;
        let findings = check(&tree.snapshot());
        let off: Vec<_> = findings
            .iter()
            .filter(|finding| finding.rule == Rule::OffCentre)
            .collect();
        assert_eq!(off.len(), 1, "{findings:?}");
        assert_eq!(off[0].node, button);
        assert!(off[0].numbers.contains(&("off", 4.0)));
    }

    #[test]
    fn a_control_on_the_centre_line_is_centred() {
        let mut tree = Tree::default();
        let field = tree.rect(None, (0.0, 0.0, 300.0, 36.0));
        tree.row(field);
        tree.centred(field);
        let button = tree.rect(Some(field), (268.0, 4.0, 28.0, 28.0));
        tree.fill(button);
        assert!(!rules(&tree.snapshot()).contains(&Rule::OffCentre));
    }

    #[test]
    fn buttons_of_one_kind_in_a_row_must_share_a_height() {
        let mut tree = Tree::default();
        let row = tree.rect(None, (0.0, 0.0, 300.0, 40.0));
        tree.row(row);
        let first = tree.rect(Some(row), (0.0, 0.0, 80.0, 36.0));
        let second = tree.rect(Some(row), (88.0, 0.0, 80.0, 32.0));
        for button in [first, second] {
            tree.fill(button);
            tree.at(button).a11y.component = Some("ActionButton".into());
        }
        let findings = check(&tree.snapshot());
        let unequal: Vec<_> = findings
            .iter()
            .filter(|finding| finding.rule == Rule::UnequalSiblings)
            .collect();
        assert_eq!(unequal.len(), 1, "{findings:?}");
        assert_eq!(unequal[0].node, second);
        assert_eq!(unequal[0].numbers, vec![("height", 32.0), ("tallest", 36.0)]);
    }

    #[test]
    fn content_outside_a_clipping_box_is_clipped_unless_a_scroll_view_holds_it() {
        let mut clipped_tree = Tree::default();
        let panel = clipped_tree.rect(None, (0.0, 0.0, 200.0, 100.0));
        clipped_tree.at(panel).props.as_mut().unwrap().clip = true;
        clipped_tree.text(panel, "Too long for the panel", (10.0, 10.0, 260.0, 16.0), 260.0);
        let findings = check(&clipped_tree.snapshot());
        assert_eq!(
            findings.iter().filter(|f| f.rule == Rule::Clipped).count(),
            1,
            "{findings:?}"
        );

        let mut scrolled_tree = Tree::default();
        let panel = scrolled_tree.rect(None, (0.0, 0.0, 200.0, 100.0));
        scrolled_tree.at(panel).props.as_mut().unwrap().clip = true;
        let scroller = scrolled_tree.rect(Some(panel), (0.0, 0.0, 200.0, 100.0));
        scrolled_tree.at(scroller).a11y.role = "ScrollView".into();
        scrolled_tree.text(scroller, "Row", (10.0, 140.0, 100.0, 16.0), 100.0);
        assert!(!rules(&scrolled_tree.snapshot()).contains(&Rule::Clipped));
    }

    #[test]
    fn partly_overlapping_siblings_are_reported() {
        let mut tree = Tree::default();
        let parent = tree.rect(None, (0.0, 0.0, 300.0, 100.0));
        let a = tree.rect(Some(parent), (0.0, 0.0, 120.0, 40.0));
        let b = tree.rect(Some(parent), (100.0, 10.0, 120.0, 40.0));
        for node in [a, b] {
            tree.fill(node);
        }
        let findings = check(&tree.snapshot());
        let overlaps: Vec<_> = findings
            .iter()
            .filter(|finding| finding.rule == Rule::Overlap)
            .collect();
        assert_eq!(overlaps.len(), 1, "{findings:?}");
        assert_eq!(overlaps[0].node, b);
        assert_eq!(overlaps[0].numbers, vec![("width", 20.0), ("height", 30.0)]);
    }

    #[test]
    fn a_box_lying_wholly_on_another_is_a_layer_not_an_overlap() {
        let mut tree = Tree::default();
        let parent = tree.rect(None, (0.0, 0.0, 300.0, 100.0));
        let face = tree.rect(Some(parent), (0.0, 0.0, 120.0, 40.0));
        let ring = tree.rect(Some(parent), (-2.0, -2.0, 124.0, 44.0));
        for node in [face, ring] {
            tree.fill(node);
        }
        assert!(!rules(&tree.snapshot()).contains(&Rule::Overlap));
    }

    #[test]
    fn cut_off_text_with_room_beside_it_is_truncated() {
        let mut tree = Tree::default();
        let row = tree.rect(None, (0.0, 0.0, 400.0, 36.0));
        tree.row(row);
        let label = tree.text(row, "A long network name", (0.0, 10.0, 100.0, 16.0), 160.0);
        let findings = check(&tree.snapshot());
        let cut: Vec<_> = findings
            .iter()
            .filter(|finding| finding.rule == Rule::Truncated)
            .collect();
        assert_eq!(cut.len(), 1, "{findings:?}");
        assert_eq!(cut[0].node, label);
        assert!(cut[0].numbers.contains(&("spare", 300.0)));
    }

    #[test]
    fn cut_off_text_that_fills_its_container_is_not_reported() {
        let mut tree = Tree::default();
        let row = tree.rect(None, (0.0, 0.0, 100.0, 36.0));
        tree.row(row);
        tree.text(row, "A long network name", (0.0, 10.0, 100.0, 16.0), 160.0);
        assert!(!rules(&tree.snapshot()).contains(&Rule::Truncated));
    }

    #[test]
    fn single_line_texts_of_one_size_in_a_row_share_a_baseline() {
        let mut tree = Tree::default();
        let row = tree.rect(None, (0.0, 0.0, 400.0, 36.0));
        tree.row(row);
        tree.text(row, "Name", (0.0, 10.0, 80.0, 16.0), 80.0);
        let value = tree.text(row, "Value", (200.0, 13.0, 80.0, 16.0), 80.0);
        let findings = check(&tree.snapshot());
        let off: Vec<_> = findings
            .iter()
            .filter(|finding| finding.rule == Rule::Baseline)
            .collect();
        assert_eq!(off.len(), 1, "{findings:?}");
        assert_eq!(off[0].node, value);
        assert!(off[0].numbers.contains(&("off", 3.0)));
    }

    fn of(findings: &[Finding], rule: Rule) -> Vec<&Finding> {
        findings.iter().filter(|finding| finding.rule == rule).collect()
    }

    /// The 'Add a network' popup at its narrowest, as Kieran's screenshot
    /// had it: the label 'Network name' squeezed to 10 px beside a 240 px
    /// field, a letter to a line, taller than its row, lying on the text of
    /// the row below.
    fn add_a_network(squeezed: bool) -> (Snapshot, u64, u64) {
        let mut tree = Tree::default();
        let root = tree.rect(None, (0.0, 0.0, 330.0, 300.0));
        let group = tree.rect(Some(root), (16.0, 16.0, 298.0, 113.0));
        tree.fill(group);
        let first = tree.rect(Some(group), (16.0, 16.0, 298.0, 56.0));
        tree.row(first);
        let name = if squeezed {
            tree.broken_text(first, "Network name", (32.0, 16.0, 10.0, 140.0), 90.0, &[1; 12], 60.0)
        } else {
            tree.text(first, "Network name", (32.0, 27.0, 90.0, 18.0), 90.0)
        };
        let field = tree.rect(Some(first), (58.0, 26.0, 240.0, 36.0));
        tree.fill(field);
        let second = tree.rect(Some(group), (16.0, 73.0, 298.0, 56.0));
        tree.row(second);
        let auto = tree.text(second, "Connect automatically", (32.0, 92.0, 150.0, 18.0), 150.0);
        let check_box = tree.rect(Some(second), (274.0, 92.0, 20.0, 20.0));
        tree.fill(check_box);
        (tree.snapshot(), name, auto)
    }

    #[test]
    fn a_label_squeezed_to_a_letter_a_line_is_found_by_every_rule_that_sees_it() {
        let (snapshot, name, auto) = add_a_network(true);
        let findings = check(&snapshot);
        let wrapped = of(&findings, Rule::Wrapped);
        assert_eq!(wrapped.len(), 1, "{findings:?}");
        assert_eq!(wrapped[0].node, name);
        assert!(wrapped[0].what.starts_with("A letter to a line"), "{}", wrapped[0].what);
        assert!(wrapped[0].numbers.contains(&("shortest-line", 1.0)));
        let outside = of(&findings, Rule::OutsideParent);
        assert_eq!(outside.len(), 1, "{findings:?}");
        assert_eq!(outside[0].node, name);
        assert_eq!(outside[0].numbers, vec![("outside", 84.0)]);
        let collision = of(&findings, Rule::TextCollision);
        assert_eq!(collision.len(), 1, "{findings:?}");
        assert_eq!(collision[0].node, auto);
        assert_eq!(collision[0].numbers, vec![("width", 10.0), ("height", 18.0)]);
    }

    #[test]
    fn the_same_popup_with_the_label_at_its_natural_width_is_clean() {
        let (snapshot, _, _) = add_a_network(false);
        let findings = check(&snapshot);
        for rule in [
            Rule::Wrapped,
            Rule::Overflow,
            Rule::OutsideParent,
            Rule::HorizontalScroll,
            Rule::HiddenLabel,
            Rule::TextCollision,
        ] {
            assert!(of(&findings, rule).is_empty(), "{rule:?}: {findings:?}");
        }
    }

    #[test]
    fn a_box_narrower_than_two_glyphs_is_broken_text_even_on_one_line() {
        let mut tree = Tree::default();
        let row = tree.rect(None, (0.0, 0.0, 300.0, 56.0));
        tree.row(row);
        let text = tree.broken_text(row, "Network name", (0.0, 10.0, 10.0, 16.0), 96.0, &[12], 96.0);
        let findings = check(&tree.snapshot());
        let wrapped = of(&findings, Rule::Wrapped);
        assert_eq!(wrapped.len(), 1, "{findings:?}");
        assert_eq!(wrapped[0].node, text);
        assert!(wrapped[0].what.contains("less than two glyphs"), "{}", wrapped[0].what);
    }

    #[test]
    fn a_word_split_in_the_middle_is_broken_text_unless_an_ellipsis_ends_it() {
        let mut split = Tree::default();
        let root = split.rect(None, (0.0, 0.0, 200.0, 80.0));
        let text = split.broken_text(root, "Hotspot-Wi-Fi-network", (0.0, 0.0, 40.0, 54.0), 140.0, &[6, 6, 8], 80.0);
        let findings = check(&split.snapshot());
        let wrapped = of(&findings, Rule::Wrapped);
        assert_eq!(wrapped.len(), 1, "{findings:?}");
        assert_eq!(wrapped[0].node, text);
        assert!(wrapped[0].numbers.contains(&("longest-word", 80.0)));

        let mut cut = Tree::default();
        let root = cut.rect(None, (0.0, 0.0, 200.0, 80.0));
        let text = cut.broken_text(root, "Hotspot-Wi-Fi-network", (0.0, 0.0, 40.0, 54.0), 140.0, &[6, 6, 8], 80.0);
        cut.at(text).used.text.as_mut().unwrap().wrap.ellipsis = true;
        assert!(of(&check(&cut.snapshot()), Rule::Wrapped).is_empty());
    }

    #[test]
    fn a_text_with_more_lines_than_it_may_have_is_broken_text() {
        let mut tree = Tree::default();
        let root = tree.rect(None, (0.0, 0.0, 300.0, 100.0));
        let text = tree.broken_text(root, "A long caption that goes on", (0.0, 0.0, 200.0, 54.0), 260.0, &[10, 10, 7], 40.0);
        tree.at(text).used.text.as_mut().unwrap().wrap.max_lines = Some(2);
        let findings = check(&tree.snapshot());
        let wrapped = of(&findings, Rule::Wrapped);
        assert_eq!(wrapped.len(), 1, "{findings:?}");
        assert!(wrapped[0].what.contains("3 lines but is allowed 2"), "{}", wrapped[0].what);
    }

    #[test]
    fn normal_wrapping_at_word_boundaries_is_clean() {
        let mut tree = Tree::default();
        let root = tree.rect(None, (0.0, 0.0, 300.0, 100.0));
        tree.broken_text(root, "Connect to the network", (0.0, 0.0, 120.0, 36.0), 170.0, &[15, 7], 52.0);
        assert!(of(&check(&tree.snapshot()), Rule::Wrapped).is_empty());
    }

    #[test]
    fn one_line_text_wider_than_its_box_needs_an_ellipsis() {
        let mut tree = Tree::default();
        let row = tree.rect(None, (0.0, 0.0, 400.0, 36.0));
        tree.row(row);
        let text = tree.text(row, "A long network name", (0.0, 10.0, 100.0, 16.0), 160.0);
        let findings = check(&tree.snapshot());
        let cut = of(&findings, Rule::Overflow);
        assert_eq!(cut.len(), 1, "{findings:?}");
        assert_eq!(cut[0].node, text);
        assert_eq!(cut[0].numbers, vec![("needed", 160.0), ("width", 100.0)]);

        let mut shown = Tree::default();
        let row = shown.rect(None, (0.0, 0.0, 400.0, 36.0));
        shown.row(row);
        let text = shown.text(row, "A long network name", (0.0, 10.0, 100.0, 16.0), 160.0);
        shown.at(text).used.text.as_mut().unwrap().wrap.ellipsis = true;
        assert!(of(&check(&shown.snapshot()), Rule::Overflow).is_empty());
    }

    #[test]
    fn a_box_reaching_outside_its_parent_is_found_unless_it_is_an_overlay() {
        let build = |overlay: bool| {
            let mut tree = Tree::default();
            let parent = tree.rect(None, (0.0, 0.0, 200.0, 56.0));
            tree.fill(parent);
            let child = tree.rect(Some(parent), (150.0, 8.0, 80.0, 40.0));
            tree.fill(child);
            if overlay {
                tree.out_of_flow(child);
            }
            (tree.snapshot(), child)
        };
        let (snapshot, child) = build(false);
        let findings = check(&snapshot);
        let outside = of(&findings, Rule::OutsideParent);
        assert_eq!(outside.len(), 1, "{findings:?}");
        assert_eq!(outside[0].node, child);
        assert_eq!(outside[0].numbers, vec![("outside", 30.0)]);
        assert!(of(&check(&build(true).0), Rule::OutsideParent).is_empty());
    }

    #[test]
    fn siblings_that_overlap_are_not_reported_when_one_is_out_of_flow() {
        let mut tree = Tree::default();
        let parent = tree.rect(None, (0.0, 0.0, 300.0, 100.0));
        let a = tree.rect(Some(parent), (0.0, 0.0, 120.0, 40.0));
        let b = tree.rect(Some(parent), (100.0, 10.0, 120.0, 40.0));
        for node in [a, b] {
            tree.fill(node);
        }
        tree.out_of_flow(b);
        assert!(!rules(&tree.snapshot()).contains(&Rule::Overlap));
    }

    #[test]
    fn content_wider_than_a_scroll_view_or_the_window_scrolls_sideways() {
        let mut tree = Tree::default();
        let root = tree.rect(None, (0.0, 0.0, 390.0, 800.0));
        let scroller = tree.rect(Some(root), (0.0, 0.0, 390.0, 600.0));
        tree.at(scroller).a11y.role = "ScrollView".into();
        tree.inner(scroller, 520.0, 1400.0);
        let wide = tree.rect(Some(root), (300.0, 620.0, 200.0, 40.0));
        tree.fill(wide);
        let findings = check(&tree.snapshot());
        let sideways = of(&findings, Rule::HorizontalScroll);
        assert_eq!(sideways.len(), 2, "{findings:?}");
        assert_eq!(sideways[0].node, scroller);
        assert_eq!(sideways[0].numbers, vec![("content", 520.0), ("view", 390.0)]);
        assert_eq!(sideways[1].node, wide);
        assert_eq!(sideways[1].numbers, vec![("past", 110.0)]);

        let mut fits = Tree::default();
        let root = fits.rect(None, (0.0, 0.0, 390.0, 800.0));
        let scroller = fits.rect(Some(root), (0.0, 0.0, 390.0, 600.0));
        fits.at(scroller).a11y.role = "ScrollView".into();
        fits.inner(scroller, 390.0, 1400.0);
        assert!(of(&check(&fits.snapshot()), Rule::HorizontalScroll).is_empty());
    }

    #[test]
    fn a_control_whose_label_lies_outside_it_has_a_hidden_label() {
        let build = |at: f32| {
            let mut tree = Tree::default();
            let root = tree.rect(None, (0.0, 0.0, 400.0, 100.0));
            let button = tree.rect(Some(root), (0.0, 0.0, 100.0, 36.0));
            tree.fill(button);
            tree.focusable(button);
            tree.clip(button);
            tree.text(button, "Connect automatically", (at, 8.0, 150.0, 18.0), 150.0);
            (tree.snapshot(), button)
        };
        let (snapshot, button) = build(120.0);
        let findings = check(&snapshot);
        let hidden = of(&findings, Rule::HiddenLabel);
        assert_eq!(hidden.len(), 1, "{findings:?}");
        assert_eq!(hidden[0].node, button);
        // With the text inside the button, two thirds of it show.
        assert!(of(&check(&build(0.0).0), Rule::HiddenLabel).is_empty());
    }

    #[test]
    fn texts_in_different_layers_do_not_collide() {
        let mut tree = Tree::default();
        let root = tree.rect(None, (0.0, 0.0, 300.0, 100.0));
        tree.text(root, "Network name", (10.0, 10.0, 100.0, 18.0), 100.0);
        let overlay = tree.rect(Some(root), (0.0, 0.0, 300.0, 100.0));
        tree.out_of_flow(overlay);
        tree.text(overlay, "Add a network", (10.0, 10.0, 100.0, 18.0), 100.0);
        assert!(of(&check(&tree.snapshot()), Rule::TextCollision).is_empty());
    }

    #[test]
    fn every_rule_has_a_unique_key_a_reason_and_a_fix() {
        let mut keys: Vec<&str> = Rule::ALL.iter().map(|rule| rule.key()).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), Rule::ALL.len());
        for rule in Rule::ALL {
            assert!(!rule.why().is_empty() && !rule.fix().is_empty() && !rule.label().is_empty());
        }
    }
}
