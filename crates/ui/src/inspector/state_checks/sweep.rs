//! The state sweep (CK-03, CK-14): for every interactive node, render rest,
//! hover, pressed and focus, and ask of the pixels in its box what a person
//! asks of the screen.
//!
//! - **Hover** differs from rest (ST-06).
//! - **Pressed** differs from hover (ST-06).
//! - **Focus** draws a ring on all four sides, not cut off by a clipping
//!   ancestor or the window edge (FC-01, FC-04).
//! - A **selected** item differs from the unselected ones beside it, and
//!   pointing at a neighbour does not change it (ST-05, ST-09).
//! - A row that is **not activatable** shows no hover fill (ST-01).
//! - A **disabled** control is dim and does not react (ST-04).
//!
//! Each node is tried in the window as it was: the pointer is parked and the
//! focus dropped between trials, and a press is let go off the control, so no
//! click lands. A control that acts on the press itself (a tab, a dropdown)
//! changes the window; the sweep then presses Escape, and if the window is
//! still not the one it was, stops with a note rather than measure a
//! different page.

use std::collections::{HashMap, HashSet};

use torin::prelude::{Area, Point2D, Size2D};

use super::frame::{Frame, SIDE_DRAWN, Side, ring_sides};
use super::rig::Rig;
use super::view::{self, Target, border, clipping_ancestor, ring_of};
use super::{Report, Rule};
use crate::inspector::snapshot::{Node, Snapshot as Tree};
use crate::motion::{FOCUS_GAP, FOCUS_WIDTH};

/// The effective opacity of a node: its own times every ancestor's.
fn opacity_of(tree: &Tree, node: &Node) -> f32 {
    std::iter::once(node)
        .chain(view::ancestors(tree, node))
        .filter_map(|here| here.props.as_ref().and_then(|props| props.opacity))
        .product()
}

/// Focuses `target`, renders it and checks its ring against the `rest` frame.
/// Returns whether focus landed on it. `rule` is [`Rule::Focus`] for a trial
/// of one control and [`Rule::TabRing`] at a stop of the Tab walk.
pub(super) fn focus_trial(
    rig: &mut Rig,
    rest: &Frame,
    target: &Target,
    rule: Rule,
    report: &mut Report,
) -> bool {
    rig.park();
    // A popup holds focus on its first field and parking does not drop it: that field is then
    // at rest already ringed, and the ring is judged against a frame with focus elsewhere.
    let held;
    let rest = if rig.focused() == target.focus {
        held = rig.rest_for(target.focus);
        &held
    } else {
        rest
    };
    rig.focus(target.focus);
    let landed = rig.focused() == target.focus;
    let frame = rig.frame();
    let tree = rig.tree();
    rig.park();
    if !landed {
        report.note(format!("focus did not land on {}", target.label()));
        return false;
    }
    // A control at the edge of its scroller is scrolled into view by the focus,
    // and the page moves under the strips: the ring is held against the page at
    // the place it now is, with focus dropped.
    let again;
    let rest = if view::moved(target, &view::refreshed(&tree, target)) {
        again = rig.rest_for(target.focus);
        &again
    } else {
        rest
    };
    ring_check(rig, rest, &frame, &tree, target, rule, report);
    true
}

/// Checks the ring of `target` in `frame` (rendered with the control focused,
/// `tree` captured then) against the `rest` frame: it is drawn on all four
/// sides and is not cut off.
pub(super) fn ring_check(
    rig: &Rig,
    rest: &Frame,
    frame: &Frame,
    tree: &Tree,
    target: &Target,
    rule: Rule,
    report: &mut Report,
) {
    // Where the control is drawn now: Tab and focus may have scrolled the page.
    let target = &view::refreshed(tree, target);
    // A control under an open panel (a completion list, a menu) is behind it:
    // its ring is the panel's to hide, not the control's to draw.
    if view::covered(tree, target) {
        report.note(format!("{} is under an open panel: its ring is not measured", target.label()));
        return;
    }
    // A control cut by the viewport, or half under the floating header after a
    // scroll, has its ring cut with it: that is the scroll position, not the
    // control. A ring is held to its face only when the whole face is in view.
    let cut = !target.whole();
    let under_header = tree
        .nodes
        .iter()
        .filter(|node| node.a11y.component.as_deref() == Some("HeaderBar"))
        .filter_map(view::border)
        .any(|header| header.intersects(&target.face.inflate(4.0, 4.0)) && !header.contains_rect(&target.face));
    if cut || under_header {
        return;
    }
    report.measure(rule);
    let ring = ring_of(tree, target);
    let inside = ring.is_some_and(|(_, inside)| inside);
    let face = ring.map_or(target.face, |(face, _)| face);
    let sides = ring_sides(rest, frame, face, inside, view::ring_colour(tree, face));
    let drawn: Vec<Side> = sides.iter().filter(|(_, share)| *share >= SIDE_DRAWN).map(|(side, _)| *side).collect();
    if drawn.is_empty() {
        let rings: Vec<String> = crate::focus::ring_check::probes(tree)
            .iter()
            .map(|(_, probe)| format!("{:?} at {:.2}", probe.face, probe.progress))
            .collect();
        let why = if ring.is_none() {
            format!(
                "the face has no FocusRing node: it does not end its render with .focusable(..) (the frame has {} rings: {rings:?})",
                rings.len()
            )
        } else if let Some(over) = view::overlying(tree, target) {
            format!("the ring node exists but nothing is drawn around the face: {over} is painted over it")
        } else {
            "the ring node exists but nothing is drawn around the face".to_owned()
        };
        let at = target.rect();
        let tag = format!("{:.0}x{:.0}-{}", at[0], at[1], target.label());
        frame.keep(&format!("ring-{tag}"));
        rest.keep(&format!("ring-rest-{tag}"));
        report.push(rule, target.label(), format!("no ring appears at focus: {why}"), target.rect());
    } else if drawn.len() < 4 {
        let missing: Vec<&str> = Side::ALL.iter().filter(|side| !drawn.contains(side)).map(|side| side.name()).collect();
        let out = FOCUS_GAP + FOCUS_WIDTH;
        let outer = Area::new(
            Point2D::new(face.min_x() - out, face.min_y() - out),
            Size2D::new(face.width() + 2.0 * out, face.height() + 2.0 * out),
        );
        let culprit = if let (true, Some(over)) = (inside, view::overlying(tree, target)) {
            format!("an inside ring, with {over} painted over it")
        } else if inside {
            "an inside ring".to_owned()
        } else if let Some((name, clip)) = clipping_ancestor(tree, target, outer) {
            format!(
                "{name} clips it at x {:.0}..{:.0}, y {:.0}..{:.0}",
                clip.min_x(),
                clip.max_x(),
                clip.min_y(),
                clip.max_y()
            )
        } else if outer.min_x() < 0.0
            || outer.min_y() < 0.0
            || outer.max_x() > rig.window.max_x()
            || outer.max_y() > rig.window.max_y()
        {
            "the window edge cuts it: use focusable_inside".to_owned()
        } else if let Some(over) = view::overlying(tree, target) {
            format!("{over} is painted over it")
        } else {
            "something drawn over it covers it".to_owned()
        };
        let at = target.rect();
        let tag = format!("{:.0}x{:.0}-{}", at[0], at[1], target.label());
        frame.keep(&format!("clipped-{tag}"));
        rest.keep(&format!("clipped-rest-{tag}"));
        report.push(
            Rule::RingClipped,
            target.label(),
            format!("the ring is missing on the {} side{}: {culprit}", missing.join(" and "), if missing.len() > 1 { "s" } else { "" }),
            target.rect(),
        );
    }

}

pub(super) fn run(rig: &mut Rig, ids: &[u64], limit: usize, report: &mut Report) -> HashSet<u64> {
    let mut trialed = HashSet::new();
    rig.park();
    let rest = rig.frame();
    let tree = rig.tree();
    let targets = view::targets(&tree);
    selected_distinct(&tree, &targets, &rest, report);

    // A few of each kind of control, the ones that act on the press last: a
    // press that opens or switches something ends the sweep.
    let mut chosen: Vec<&Target> = Vec::new();
    let mut per_kind: HashMap<String, usize> = HashMap::new();
    for target in targets.iter().filter(|target| target.control() && target.whole() && !view::covered(&tree, target)) {
        let count = per_kind.entry(format!("{} {}", target.role, target.component)).or_default();
        if *count < rig.per_kind && chosen.len() < limit {
            *count += 1;
            chosen.push(target);
        }
    }
    // A window control acts on the press too (it minimises, maximises or closes the window the sweep
    // is in), so it goes with them.
    chosen.sort_by_key(|target| {
        matches!(target.role.as_str(), "Tab" | "ComboBox" | "MenuItem" | "TreeItem") || target.component == "WindowControl"
    });
    let controls = targets.iter().filter(|target| target.control()).count();

    for (index, target) in chosen.iter().enumerate() {
        // A control that ends the sweep and is the last of it leaves nothing unswept.
        let last = index + 1 == chosen.len();
        if rig.over_budget() {
            report.note(format!(
                "sweep stopped at the time budget: {} of {} controls ({} chosen by kind)",
                report.swept,
                controls,
                chosen.len()
            ));
            break;
        }
        report.swept += 1;
        rig.park();
        let at = target.centre();

        // Hover.
        rig.hover(at);
        let hover = rig.frame();
        let change = rest.change(&hover, target.visible);
        report.measure(Rule::Hover);
        if !change.visible() {
            hover.keep(&format!("hover-{}", target.label()));
            report.push(
                Rule::Hover,
                target.label(),
                format!("hovering it changes nothing you can see ({})", change.describe()),
                target.rect(),
            );
        }
        // Pointing at it leaves its selected neighbours alone.
        for other in targets
            .iter()
            .filter(|other| other.focus != target.focus && other.selected == Some(true))
            .filter(|other| same_parent(&tree, target, other))
        {
            report.measure(Rule::Collateral);
            let moved = rest.change(&hover, other.visible);
            if moved.changed > 0 && !touching(other.visible, target.visible) {
                report.push(
                    Rule::Collateral,
                    other.label(),
                    format!("pointing at {} changes it ({})", target.label(), moved.describe()),
                    other.rect(),
                );
            }
        }

        // Focus.
        if focus_trial(rig, &rest, target, Rule::Focus, report) {
            trialed.insert(target.focus.0);
        }
        if !rig.restore(ids, 3) {
            if !last {
                report.note(format!("stopped after focusing {}: the window changed", target.label()));
            }
            return trialed;
        }

        // Pressed, against hovered.
        rig.hover(at);
        let hover = rig.frame();
        rig.press(at);
        let pressed = rig.frame();
        rig.release_away();
        let change = hover.change(&pressed, target.visible);
        report.measure(Rule::Press);
        if !change.visible() {
            pressed.keep(&format!("press-{}", target.label()));
            report.push(
                Rule::Press,
                target.label(),
                format!("pressing it looks the same as hovering it ({})", change.describe()),
                target.rect(),
            );
        }
        // A press that opens a list or a menu (a crumb's chevron) leaves it up, over the
        // controls the next trials point at: close it while the control still holds focus.
        if view::panel_open(&rig.tree()) {
            rig.named(freya::prelude::NamedKey::Escape);
        }
        rig.park();
        if !rig.restore(ids, 3) {
            if !last {
                report.note(format!(
                    "stopped after pressing {}: the press changed the window (it acts on the press); {} of {} controls swept",
                    target.label(),
                    report.swept,
                    chosen.len()
                ));
            }
            return trialed;
        }
    }
    rig.park();
    plain_rows(rig, &tree, &targets, report);
    disabled(rig, &tree, report);
    rig.park();
    trialed
}

fn same_parent(tree: &Tree, a: &Target, b: &Target) -> bool {
    match (tree.node(a.id), tree.node(b.id)) {
        (Some(a), Some(b)) => a.parent == b.parent,
        _ => false,
    }
}

/// Whether two boxes are within a hairline of each other or overlap.
fn touching(a: Area, b: Area) -> bool {
    a.inflate(1.0, 1.0).intersects(&b)
}

/// The mean colour of a box.
fn mean(frame: &Frame, area: Area) -> [f32; 3] {
    let mut sum = [0.0f32; 3];
    let mut n = 0.0;
    let (x0, x1) = (area.min_x().ceil() as i32, area.max_x().floor() as i32);
    let (y0, y1) = (area.min_y().ceil() as i32, area.max_y().floor() as i32);
    for y in (y0..y1).step_by(1) {
        for x in (x0..x1).step_by(1) {
            let p = frame.pixel(x as f32, y as f32);
            for c in 0..3 {
                sum[c] += p[c] as f32;
            }
            n += 1.0;
        }
    }
    if n == 0.0 { sum } else { sum.map(|c| c / n) }
}

/// ST-09: in a group of siblings of one kind where some are selected and some
/// are not, the selected one is painted differently at rest.
fn selected_distinct(tree: &Tree, targets: &[Target], rest: &Frame, report: &mut Report) {
    let mut groups: Vec<(Option<u64>, String, Vec<&Target>)> = Vec::new();
    for target in targets.iter().filter(|target| target.selected.is_some()) {
        let parent = tree.node(target.id).and_then(|node| node.parent);
        match groups.iter_mut().find(|(p, component, _)| *p == parent && *component == target.component) {
            Some((_, _, members)) => members.push(target),
            None => groups.push((parent, target.component.clone(), vec![target])),
        }
    }
    for (_, _, members) in groups {
        let on: Vec<&&Target> = members.iter().filter(|target| target.selected == Some(true)).collect();
        let off: Vec<&&Target> = members.iter().filter(|target| target.selected == Some(false)).collect();
        let (Some(on), Some(off)) = (on.first(), off.first()) else { continue };
        report.measure(Rule::Selected);
        let (a, b) = (mean(rest, on.visible), mean(rest, off.visible));
        let spread = (0..3).map(|c| (a[c] - b[c]).abs()).fold(0.0, f32::max);
        if spread < 3.0 {
            report.push(
                Rule::Selected,
                on.label(),
                format!(
                    "selected, but its box is the same colour as {}, which is not (mean colours differ by {spread:.1})",
                    off.label()
                ),
                on.rect(),
            );
        }
    }
}

/// The fill a node paints in two captures of the tree, when it differs: read
/// from the declared fill, not the pixels, so a tooltip that opens on the
/// control (instantly, with the clock off) is not taken for a fill.
fn fill_change(before: &Tree, after: &Tree, id: u64) -> Option<(String, String)> {
    let node = before.node(id)?;
    let area = border(node)?;
    let fill_of = |node: &Node| node.props.as_ref().and_then(|props| props.background);
    // Node ids are by position in the tree, which an overlay that opens can
    // shift: the same node after is the one with the same accessibility id (a
    // control has one), else the same component and box. A box shared with its
    // wrapper (a focus wrapper hugs its face) is not a twin by box alone.
    let same_box = |other: &Node| {
        border(other).is_some_and(|b| {
            (b.min_x() - area.min_x()).abs() < 0.5
                && (b.min_y() - area.min_y()).abs() < 0.5
                && (b.width() - area.width()).abs() < 0.5
                && (b.height() - area.height()).abs() < 0.5
        })
    };
    let twin = match node.a11y.id {
        Some(own) => after.nodes.iter().find(|other| other.a11y.id == Some(own) && same_box(other)),
        None => after.nodes.iter().find(|other| {
            other.a11y.id.is_none() && other.a11y.component == node.a11y.component && other.kind == node.kind && same_box(other)
        }),
    }?;
    let (a, b) = (fill_of(node), fill_of(twin));
    (a != b).then(|| {
        let show = |colour: Option<freya::prelude::Color>| {
            colour.map_or_else(|| "none".to_owned(), |c| format!("#{:02x}{:02x}{:02x}/{}", c.r(), c.g(), c.b(), c.a()))
        };
        (show(a), show(b))
    })
}

/// ST-01: a row that is not activatable paints no hover fill.
fn plain_rows(rig: &mut Rig, tree: &Tree, targets: &[Target], report: &mut Report) {
    let rows: Vec<&Node> = tree
        .nodes
        .iter()
        .filter(|node| node.a11y.component.as_deref() == Some("Row"))
        .filter(|node| node.a11y.role == "GenericContainer" && !node.a11y.inert)
        .collect();
    let mut tried = 0;
    for row in rows {
        if tried >= rig.per_kind * 2 || rig.over_budget() {
            break;
        }
        let Some(visible) = view::visible_box(tree, row, rig.window) else { continue };
        if visible.height() < 20.0 {
            continue;
        }
        // A point of the row's own, off every control inside it.
        let y = visible.center().y;
        let free = [0.15f32, 0.3, 0.45, 0.6]
            .iter()
            .map(|share| visible.min_x() + visible.width() * share)
            .find(|x| {
                !targets.iter().any(|target| target.visible.inflate(2.0, 2.0).contains(Point2D::new(*x, y)))
            });
        let Some(x) = free else { continue };
        tried += 1;
        rig.park();
        rig.hover((x, y));
        let now = rig.tree();
        report.measure(Rule::RowHover);
        if let Some((before, after)) = fill_change(tree, &now, row.id) {
            report.push(
                Rule::RowHover,
                view::describe_node(tree, row),
                format!("the row has nothing to activate yet its fill goes from {before} to {after} under the pointer"),
                [visible.min_x(), visible.min_y(), visible.width(), visible.height()],
            );
        }
    }
}

/// ST-04: a control that cannot be used is dim and does not react.
fn disabled(rig: &mut Rig, tree: &Tree, report: &mut Report) {
    const ROLES: [&str; 6] = ["Button", "Switch", "Slider", "ComboBox", "CheckBox", "RadioButton"];
    let mut tried = 0;
    for node in &tree.nodes {
        if tried >= rig.per_kind * 2 || rig.over_budget() {
            break;
        }
        if !node.a11y.disabled
            || node.a11y.inert
            || !ROLES.contains(&node.a11y.role.as_str())
            || border(node).is_none()
        {
            continue;
        }
        let Some(visible) = view::visible_box(tree, node, rig.window) else { continue };
        let label = view::describe_node(tree, node);
        report.measure(Rule::Disabled);
        tried += 1;
        let opacity = opacity_of(tree, node);
        if opacity > crate::style::DISABLED_OPACITY + 0.15 {
            report.push(
                Rule::Disabled,
                label.clone(),
                format!("is disabled but drawn at opacity {opacity:.2}; a disabled control is dimmed to {}", crate::style::DISABLED_OPACITY),
                [visible.min_x(), visible.min_y(), visible.width(), visible.height()],
            );
        }
        rig.park();
        let c = visible.center();
        rig.hover((c.x, c.y));
        let now = rig.tree();
        if let Some((before, after)) = fill_change(tree, &now, node.id) {
            report.push(
                Rule::Disabled,
                label,
                format!("a disabled control reacts to the pointer: its fill goes from {before} to {after}"),
                [visible.min_x(), visible.min_y(), visible.width(), visible.height()],
            );
        }
    }
}
