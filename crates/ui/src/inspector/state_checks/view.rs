//! The interactive nodes of a captured tree: what a person can point at, tab
//! to or press, where they really are on screen, and what to call them.

use freya_core::accessibility::id::AccessibilityId;
use torin::prelude::{Area, Point2D, Size2D};

use crate::inspector::snapshot::{Node, Snapshot as Tree};

/// A node a person can focus, point at or press.
#[derive(Clone, Debug)]
pub(super) struct Target {
    pub id: u64,
    pub focus: AccessibilityId,
    /// The accessible name, or the first text inside, or "".
    pub name: String,
    /// The component that built the node, or the nearest one above it.
    pub component: String,
    /// The components from the outermost down, ` > ` apart.
    pub path: String,
    pub role: String,
    /// The border box: the painted face.
    pub face: Area,
    /// The face inside every clipping ancestor and the window.
    pub visible: Area,
    /// What a finger or pointer can press: the face, and any transparent
    /// absolute box inside it that reaches further (`hit_area`).
    pub hit: Area,
    pub selected: Option<bool>,
}

/// The roles of a control: what a person presses, toggles, picks or types into.
const CONTROL_ROLES: &[&str] = &[
    "Button",
    "CheckBox",
    "ColumnHeader",
    "ComboBox",
    "Link",
    "ListBoxOption",
    "MenuItem",
    "MultilineTextInput",
    "RadioButton",
    "SearchInput",
    "Slider",
    "SpinButton",
    "Splitter",
    "Switch",
    "Tab",
    "TextInput",
    "TreeItem",
];

impl Target {
    /// Whether the whole face is in view: not cut by a clip, the window or a
    /// scroll. A control half out of view is not pointed at or ringed.
    pub fn whole(&self) -> bool {
        (self.visible.width() - self.face.width()).abs() <= 0.5
            && (self.visible.height() - self.face.height()).abs() <= 0.5
    }

    /// Whether it is a control (pointed at, pressed, sized for a finger), as
    /// opposed to a focusable region such as a page of selectable text, a
    /// window or a list that only holds focus for its rows.
    pub fn control(&self) -> bool {
        CONTROL_ROLES.contains(&self.role.as_str())
    }

    pub fn centre(&self) -> (f32, f32) {
        let c = self.visible.center();
        (c.x, c.y)
    }

    /// How a finding names it: the last components of the path, then the name.
    pub fn label(&self) -> String {
        let tail: Vec<&str> = self.path.split(" > ").collect();
        let tail = tail[tail.len().saturating_sub(3)..].join(" > ");
        // A focusable that is not a control says what it is.
        let tail = if self.control() || self.role.is_empty() { tail } else { format!("{tail} ({})", self.role) };
        if self.name.is_empty() {
            tail
        } else {
            format!("{tail} \u{201c}{}\u{201d}", self.name)
        }
    }

    /// The box as `[x, y, width, height]`.
    pub fn rect(&self) -> [f32; 4] {
        [self.face.min_x(), self.face.min_y(), self.face.width(), self.face.height()]
    }
}

pub(super) fn border(node: &Node) -> Option<Area> {
    node.used.border.or(node.area)
}

/// The window's box: the root's.
pub(super) fn window(tree: &Tree) -> Area {
    tree.roots
        .first()
        .and_then(|id| tree.node(*id))
        .and_then(border)
        .unwrap_or_else(|| Area::new(Point2D::new(0.0, 0.0), Size2D::new(0.0, 0.0)))
}

/// The ancestors of `node`, nearest first.
pub(super) fn ancestors<'a>(tree: &'a Tree, node: &'a Node) -> impl Iterator<Item = &'a Node> {
    std::iter::successors(node.parent.and_then(|id| tree.node(id)), |here| {
        here.parent.and_then(|id| tree.node(id))
    })
}

/// The components a node is built in, outermost first.
pub(super) fn path_of(tree: &Tree, node: &Node) -> String {
    let mut names: Vec<&str> = std::iter::once(node)
        .chain(ancestors(tree, node))
        .filter_map(|here| here.a11y.component.as_deref())
        .collect();
    names.reverse();
    names.join(" > ")
}

/// The first text at or below `node`, to name an unnamed control.
pub(super) fn text_in(tree: &Tree, node: &Node) -> Option<String> {
    if let Some(text) = node.text.as_ref().filter(|text| !text.trim().is_empty()) {
        return Some(text.clone());
    }
    node.children
        .iter()
        .filter_map(|id| tree.node(*id))
        .find_map(|child| text_in(tree, child))
}

/// Whether the node, or any ancestor, is not drawn or takes no events.
fn gone(tree: &Tree, node: &Node) -> bool {
    std::iter::once(node).chain(ancestors(tree, node)).any(|here| {
        here.used.hidden || here.props.as_ref().and_then(|props| props.opacity).is_some_and(|o| o <= 0.0)
    })
}

/// A node's box inside every clipping ancestor and the window.
pub(super) fn visible_box(tree: &Tree, node: &Node, window: Area) -> Option<Area> {
    let mut visible = border(node)?.intersection(&window)?;
    for above in ancestors(tree, node) {
        if above.props.as_ref().is_some_and(|props| props.clip)
            && let Some(clip) = border(above)
        {
            visible = visible.intersection(&clip)?;
        }
    }
    (visible.width() >= 1.0 && visible.height() >= 1.0).then_some(visible)
}

/// What a focus is built from, in tree order: every node that takes focus and
/// can be reached. Inert nodes (behind a modal scope), hidden ones and ones
/// scrolled out of view are left out, and so is a node with the same box as
/// the focusable node above it (a wrapper and the control it holds are one
/// stop).
pub(super) fn targets(tree: &Tree) -> Vec<Target> {
    let mut found: Vec<Target> = Vec::new();
    for target in focusables(tree).into_iter().filter(|target| target.visible.width() >= 1.0) {
        let same_box = |other: &Target| {
            (other.face.min_x() - target.face.min_x()).abs() < 0.5
                && (other.face.min_y() - target.face.min_y()).abs() < 0.5
                && (other.face.width() - target.face.width()).abs() < 0.5
                && (other.face.height() - target.face.height()).abs() < 0.5
        };
        if !found.iter().any(same_box) {
            found.push(target);
        }
    }
    found
}

/// Every node that takes focus, in tree order, wherever it is on the page:
/// what Tab can stop on. A node scrolled out of view has an empty `visible`.
pub(super) fn focusables(tree: &Tree) -> Vec<Target> {
    let window = window(tree);
    let mut found: Vec<Target> = Vec::new();
    for node in &tree.nodes {
        if node.a11y.focusable != Some(true) || node.a11y.inert {
            continue;
        }
        let Some(focus) = node.a11y.id else { continue };
        let Some(face) = border(node) else { continue };
        if gone(tree, node) || face.width() <= 0.0 || face.height() <= 0.0 {
            continue;
        }
        let visible = visible_box(tree, node, window)
            .unwrap_or_else(|| Area::new(face.origin, Size2D::new(0.0, 0.0)));
        let name = node
            .a11y
            .name
            .clone()
            .filter(|name| !name.is_empty())
            .or_else(|| text_in(tree, node))
            .unwrap_or_default();
        let component = std::iter::once(node)
            .chain(ancestors(tree, node))
            .find_map(|here| here.a11y.component.clone())
            .unwrap_or_default();
        let hit = hit_box(tree, node, face);
        found.push(Target {
            id: node.id,
            focus,
            name,
            component,
            path: path_of(tree, node),
            role: node.a11y.role.clone(),
            face,
            visible,
            hit,
            selected: node.a11y.selected,
        });
    }
    found
}

/// The box a press lands in: the face, grown by every transparent absolute
/// child that is centred over it and reaches further, which is how a control
/// with a small painted face has a hit region a finger can find.
fn hit_box(tree: &Tree, node: &Node, face: Area) -> Area {
    let mut hit = face;
    for child in node.children.iter().filter_map(|id| tree.node(*id)) {
        let transparent = child.props.as_ref().is_none_or(|props| props.background.is_none_or(|c| c.a() == 0));
        if child.used.out_of_flow
            && transparent
            && let Some(area) = border(child)
            && area.inflate(1.0, 1.0).contains(face.center())
        {
            hit = hit.union(&area);
        }
    }
    hit
}

/// The ids of every focusable node: what must be the same before and after a trial for
/// the window to be the window it was.
pub(super) fn signature(tree: &Tree) -> Vec<u64> {
    let mut ids: Vec<u64> = focusables(tree).iter().map(|target| target.focus.0).collect();
    ids.sort_unstable();
    ids
}

/// `target` as `tree` lays it out. Focus scrolls a control at the edge of its
/// scroller into view, so the box read before the focus is not where the
/// control is drawn after it.
pub(super) fn refreshed(tree: &Tree, target: &Target) -> Target {
    focusables(tree).into_iter().find(|now| now.focus == target.focus).unwrap_or_else(|| target.clone())
}

/// Whether `now` is somewhere else than `before`: the page scrolled between.
pub(super) fn moved(before: &Target, now: &Target) -> bool {
    (before.face.min_x() - now.face.min_x()).abs() > 0.5 || (before.face.min_y() - now.face.min_y()).abs() > 0.5
}

/// Whether an open panel (a menu, a completion list, a popover) is drawn over
/// the middle of `target` and the target is not part of it. A pointer over
/// such a control lands on the panel and the panel hides its ring, so what a
/// state or ring would look like there is not measurable.
pub(super) fn covered(tree: &Tree, target: &Target) -> bool {
    let Some(node) = tree.node(target.id) else { return false };
    let middle = target.face.center();
    tree.nodes.iter().any(|content| {
        panel_of(tree, content).is_some_and(|panel| {
            !within(tree, node, panel)
                && !content.used.hidden
                && border(content).is_some_and(|area| area.width() > 8.0 && area.height() > 8.0 && area.contains(middle))
        })
    })
}

/// The `Panel` that `node` is, or is drawn in. A panel's own node is a box of no size: what it
/// draws (the dismiss shield over the window, the card, its rows) is its children.
fn panel_of<'a>(tree: &'a Tree, node: &'a Node) -> Option<&'a Node> {
    std::iter::once(node).chain(ancestors(tree, node)).find(|here| here.a11y.component.as_deref() == Some("Panel"))
}

/// What is painted over the middle of `target` and is not part of it or around
/// it: the last such node in tree order (painted last), named as a reader would.
/// It is what covers a ring that is drawn and cannot be seen.
pub(super) fn overlying(tree: &Tree, target: &Target) -> Option<String> {
    let at = tree.nodes.iter().position(|node| node.id == target.id)?;
    let middle = target.face.center();
    let here = tree.node(target.id)?;
    tree.nodes[at + 1..]
        .iter()
        .rev()
        .filter(|node| !node.used.hidden && !within(tree, node, here) && !within(tree, here, node))
        .filter(|node| node.props.as_ref().and_then(|props| props.background).is_some_and(|fill| fill.a() >= 128))
        .find(|node| border(node).is_some_and(|area| area.width() >= 8.0 && area.height() >= 8.0 && area.contains(middle)))
        .map(|node| describe_node(tree, node))
}

/// The face of the focus ring that belongs to `target` in `tree`, and whether
/// the ring is drawn inside it. A control's ring may hug a part of it (a
/// checkbox rings its box, not its label), so the ring is the visible one
/// whose face is inside the target's box, or holds it.
pub(super) fn ring_of(tree: &Tree, target: &Target) -> Option<(Area, bool)> {
    let around = target.face.inflate(1.0, 1.0);
    crate::focus::ring_check::probes(tree)
        .into_iter()
        .map(|(_, probe)| probe)
        .filter(|probe| probe.progress > 0.5)
        .map(|probe| {
            let [x, y, w, h] = probe.face;
            (Area::new(Point2D::new(x, y), Size2D::new(w, h)), probe.inside)
        })
        .filter(|(face, _)| around.contains_rect(face) || face.inflate(1.0, 1.0).contains_rect(&target.face))
        .max_by(|a, b| (a.0.width() * a.0.height()).total_cmp(&(b.0.width() * b.0.height())))
}

/// The colour of the line of the ring around `face` in `tree`, when it is
/// opaque.
pub(super) fn ring_colour(tree: &Tree, face: Area) -> Option<[u8; 3]> {
    let same = |box_: [f32; 4]| {
        (box_[0] - face.min_x()).abs() < 0.5
            && (box_[1] - face.min_y()).abs() < 0.5
            && (box_[2] - face.width()).abs() < 0.5
            && (box_[3] - face.height()).abs() < 0.5
    };
    let (id, _) = crate::focus::ring_check::probes(tree).into_iter().find(|(_, probe)| same(probe.face))?;
    let line = tree.node(id)?.props.as_ref()?.borders.first()?.fill;
    (line.a() == 255).then(|| [line.r(), line.g(), line.b()])
}

/// The nearest clipping ancestor of `target` whose box does not hold `outer`,
/// with the box: what cuts a ring off.
pub(super) fn clipping_ancestor(tree: &Tree, target: &Target, outer: Area) -> Option<(String, Area)> {
    let face = tree.node(target.id)?;
    ancestors(tree, face).find_map(|above| {
        let clip = border(above).filter(|_| above.props.as_ref().is_some_and(|props| props.clip))?;
        let holds = clip.min_x() <= outer.min_x() + 0.5
            && clip.min_y() <= outer.min_y() + 0.5
            && clip.max_x() >= outer.max_x() - 0.5
            && clip.max_y() >= outer.max_y() - 0.5;
        (!holds).then(|| (describe_node(tree, above), clip))
    })
}

/// A node as a reader names it: its component path and its first text.
pub(super) fn describe_node(tree: &Tree, node: &Node) -> String {
    let path = path_of(tree, node);
    let tail: Vec<&str> = path.split(" > ").collect();
    let tail = tail[tail.len().saturating_sub(3)..].join(" > ");
    match text_in(tree, node) {
        Some(text) if !tail.is_empty() => format!("{tail} \u{201c}{}\u{201d}", text.chars().take(40).collect::<String>()),
        _ if tail.is_empty() => node.title().to_owned(),
        _ => tail,
    }
}

/// Whether `inner` is `outer` or inside it.
pub(super) fn within(tree: &Tree, inner: &Node, outer: &Node) -> bool {
    inner.id == outer.id || ancestors(tree, inner).any(|above| above.id == outer.id)
}

/// Whether a panel (a menu, a completion list, a popover) is open over the
/// page: a `Panel` with a box.
pub(super) fn panel_open(tree: &Tree) -> bool {
    tree.nodes.iter().any(|node| {
        !node.used.hidden
            && panel_of(tree, node).is_some()
            && border(node).is_some_and(|area| area.width() > 8.0 && area.height() > 8.0)
    })
}
