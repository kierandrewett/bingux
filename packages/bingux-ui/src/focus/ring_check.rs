//! The focus ring check: every ring must hug the box that is painted.
//!
//! A control declares its **face** by ending `render` with `.focusable(..)`
//! on the node that carries the fill. The `focusable` wrapper hugs the face,
//! measures it after layout, and draws the ring from that measured box and
//! the declared radius, so the face's padding, content and margin cannot move
//! it. This module checks the result: for every ring in a laid-out tree, the
//! ring's rectangle must equal the face's border box grown by the ring's
//! outset (gap, width and, while it animates, travel), within half a pixel,
//! and its corner radius must be the face's plus that outset (nested).
//!
//! The rule is pure: [`check`] takes rectangles. [`collect`] reads them from a
//! headless run, so the same check runs over every Settings page and every
//! taxonomy page (`scripts/focus-ring-check.py`) and the Inspector's Layout
//! tab reports it live.

use crate::inspector::snapshot::{Node, Snapshot};
use crate::motion::{FOCUS_GAP, FOCUS_TRAVEL, FOCUS_WIDTH};

/// A rectangle as `[x, y, width, height]` in logical pixels.
pub type Box4 = [f32; 4];

/// The most a ring may differ from the face it is drawn for, in logical pixels.
pub const TOLERANCE: f32 = 0.5;

/// The most a ring's corner radius may differ from the nested radius.
pub const RADIUS_TOLERANCE: f32 = 1.0;

/// One ring and the face it belongs to, as laid out.
#[derive(Clone, Debug, PartialEq)]
pub struct RingProbe {
    /// What the face is, for the report: the component or role, and its name.
    pub label: String,
    /// The face's border box.
    pub face: Box4,
    /// The face's corner radius.
    pub face_radius: f32,
    /// The ring node's border box.
    pub ring: Box4,
    /// The ring's own opacity, which is its animation progress: 0 unfocused,
    /// 1 settled.
    pub progress: f32,
    /// The ring's corner radius.
    pub ring_radius: f32,
    /// Whether the ring sits inside the face (a face on a clipped edge)
    /// instead of outside it.
    pub inside: bool,
    /// Whether the face, or a box of its size inside it, draws a fill, a border
    /// or rounded corners. A face that draws nothing has no corners to follow.
    pub painted: bool,
}

/// One ring that is not where its face is painted.
#[derive(Clone, Debug, PartialEq)]
pub struct Mismatch {
    pub label: String,
    pub what: &'static str,
    pub expected: Box4,
    pub found: Box4,
}

impl std::fmt::Display for Mismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: {} (ring should be {:?}, is {:?})",
            self.label, self.what, self.expected, self.found
        )
    }
}

/// How far outside its face a ring's box reaches at `progress`: the gap and
/// the ring's width, plus the travel the ring still has to come in.
pub fn outset(progress: f32) -> f32 {
    FOCUS_GAP + FOCUS_WIDTH + FOCUS_TRAVEL * (1.0 - progress.clamp(0.0, 1.0))
}

/// The box the ring node must occupy for `face` at `progress`.
pub fn expected_ring(face: Box4, progress: f32, inside: bool) -> Box4 {
    if inside {
        return face;
    }
    let out = outset(progress);
    [face[0] - out, face[1] - out, face[2] + 2.0 * out, face[3] + 2.0 * out]
}

/// Whether two boxes are the same within `tolerance`.
pub fn same(a: Box4, b: Box4, tolerance: f32) -> bool {
    a.iter().zip(&b).all(|(a, b)| (a - b).abs() <= tolerance)
}

/// The corner radius a ring needs for a face of `face_radius`: nested, so the
/// ring's outer edge follows the face's corner.
pub fn expected_radius(face_radius: f32, progress: f32, inside: bool) -> f32 {
    if inside || face_radius == 0.0 {
        face_radius
    } else {
        face_radius + outset(progress)
    }
}

/// Every ring that is not on the face it is drawn for.
pub fn check(probes: &[RingProbe]) -> Vec<Mismatch> {
    let mut found = Vec::new();
    for probe in probes {
        let expected = expected_ring(probe.face, probe.progress, probe.inside);
        if !same(probe.ring, expected, TOLERANCE) {
            found.push(Mismatch {
                label: probe.label.clone(),
                what: "the ring is not around the face's border box",
                expected,
                found: probe.ring,
            });
            continue;
        }
        // A face that draws nothing (a list, a chart, a row at rest) has no
        // corners for the ring to follow; the box was checked above.
        if !probe.painted {
            continue;
        }
        // A row clipped by its card paints a square that the card rounds at its
        // own corners, so the ring drawn inside it takes the radius it is given.
        if probe.inside && probe.face_radius == 0.0 {
            continue;
        }
        let radius = expected_radius(probe.face_radius, probe.progress, probe.inside);
        if (probe.ring_radius - radius).abs() > RADIUS_TOLERANCE {
            found.push(Mismatch {
                label: probe.label.clone(),
                what: "the ring's corner radius does not follow the face's",
                expected: [radius, 0.0, 0.0, 0.0],
                found: [probe.ring_radius, 0.0, 0.0, 0.0],
            });
        }
    }
    found
}

/// The class name a ring node carries, so a tree walk can find it.
pub const RING_CLASS: &str = "FocusRing";
/// The class of a ring drawn inside its face.
pub const RING_INSIDE_CLASS: &str = "FocusRingInside";

/// Every ring in `snapshot` with the face it belongs to, each with the ring
/// node's id. A ring is the last child of its `focusable` wrapper; the face is
/// the wrapper's first other child.
pub fn probes(snapshot: &Snapshot) -> Vec<(u64, RingProbe)> {
    let mut found = Vec::new();
    for ring in &snapshot.nodes {
        let inside = match ring.a11y.component.as_deref() {
            Some(RING_CLASS) => false,
            Some(RING_INSIDE_CLASS) => true,
            _ => continue,
        };
        let Some(wrapper) = ring.parent.and_then(|id| snapshot.node(id)) else {
            continue;
        };
        let Some(face) = wrapper
            .children
            .iter()
            .filter_map(|id| snapshot.node(*id))
            .find(|node| node.id != ring.id)
        else {
            continue;
        };
        let (Some(face_box), Some(ring_box)) = (border_box(face), border_box(ring)) else {
            continue;
        };
        // A face that was never laid out (or is hidden) has no ring to check.
        if face_box[2] <= 0.0 || face_box[3] <= 0.0 || face.used.hidden {
            continue;
        }
        let ring_radius = ring.props.as_ref().map_or(0.0, |props| props.corner_radius);
        let progress = ring.props.as_ref().and_then(|props| props.opacity).unwrap_or(1.0);
        let painter = painted(snapshot, face, face_box);
        found.push((
            ring.id,
            RingProbe {
                label: path_of(snapshot, face),
                face: face_box,
                // A face that is not a box has no radius of its own to follow.
                face_radius: painter.props.as_ref().map_or_else(
                    || expected_inverse(ring_radius, progress, inside),
                    |props| props.corner_radius,
                ),
                ring: ring_box,
                progress,
                ring_radius,
                inside,
                painted: paints(painter),
            },
        ));
    }
    found
}

/// Whether `node` draws anything of its own: a fill, a border or rounded
/// corners.
fn paints(node: &Node) -> bool {
    node.props.as_ref().is_some_and(|props| {
        props.background.is_some_and(|colour| colour.a() > 0)
            || props.corner_radius > 0.0
            || !props.borders.is_empty()
    })
}

/// The node that paints the face. It is `face` itself when that draws
/// something; otherwise the first descendant with exactly the face's box that
/// does. A text field's wrapper draws nothing and the input inside it holds
/// the fill, so the ring is checked against the input's corners.
fn painted<'a>(snapshot: &'a Snapshot, face: &'a Node, face_box: Box4) -> &'a Node {
    let mut node = face;
    while !paints(node) {
        let inner = node
            .children
            .iter()
            .filter_map(|id| snapshot.node(*id))
            .find(|child| border_box(child).is_some_and(|area| same(area, face_box, TOLERANCE)));
        match inner {
            Some(child) => node = child,
            None => return face,
        }
    }
    node
}

/// The face radius a ring of `ring_radius` is nested around.
fn expected_inverse(ring_radius: f32, progress: f32, inside: bool) -> f32 {
    if inside || ring_radius == 0.0 {
        ring_radius
    } else {
        (ring_radius - outset(progress)).max(0.0)
    }
}

fn border_box(node: &Node) -> Option<Box4> {
    node.used
        .border
        .or(node.area)
        .map(|area| [area.min_x(), area.min_y(), area.width(), area.height()])
}

/// The components a node is built in, outermost first, then the node.
fn path_of(snapshot: &Snapshot, node: &Node) -> String {
    let mut names = Vec::new();
    let mut current = Some(node);
    while let Some(here) = current {
        if let Some(component) = here.a11y.component.as_deref() {
            names.push(component);
        }
        current = here.parent.and_then(|id| snapshot.node(id));
    }
    names.reverse();
    names.join(" > ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe(face: Box4, ring: Box4, progress: f32) -> RingProbe {
        RingProbe {
            label: "Dropdown".to_string(),
            face,
            face_radius: 8.0,
            ring,
            progress,
            ring_radius: 8.0 + outset(progress),
            inside: false,
            painted: true,
        }
    }

    #[test]
    fn a_settled_ring_is_the_face_grown_by_gap_and_width() {
        let face = [100.0, 40.0, 200.0, 36.0];
        let out = FOCUS_GAP + FOCUS_WIDTH;
        let ring = [100.0 - out, 40.0 - out, 200.0 + 2.0 * out, 36.0 + 2.0 * out];
        assert!(check(&[probe(face, ring, 1.0)]).is_empty());
    }

    #[test]
    fn a_ring_that_follows_the_inner_box_of_a_padded_face_is_reported() {
        // The dropdown bug: the ring was laid out against the face's content
        // box, so it started a padding in and ended a padding short.
        let face = [108.0, 60.0, 320.0, 44.0];
        let out = FOCUS_GAP + FOCUS_WIDTH;
        let ring = [108.0 + 12.0 - out, 60.0 - out, 320.0 - 20.0 + 2.0 * out, 44.0 + 2.0 * out];
        let found = check(&[probe(face, ring, 1.0)]);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].expected[0], 108.0 - out);
    }

    #[test]
    fn an_unfocused_ring_is_checked_at_its_travel_outset() {
        let face = [0.0, 0.0, 100.0, 40.0];
        let out = outset(0.0);
        let ring = [-out, -out, 100.0 + 2.0 * out, 40.0 + 2.0 * out];
        assert!(check(&[probe(face, ring, 0.0)]).is_empty());
        // The same box would be wrong for a settled ring.
        assert_eq!(check(&[probe(face, ring, 1.0)]).len(), 1);
    }

    #[test]
    fn an_inside_ring_is_the_face_itself() {
        let face = [0.0, 0.0, 100.0, 40.0];
        let mut inside = probe(face, face, 1.0);
        inside.inside = true;
        inside.ring_radius = 8.0;
        assert!(check(&[inside]).is_empty());
    }

    #[test]
    fn an_inside_ring_may_be_rounded_on_a_square_face_a_card_clips() {
        let face = [0.0, 0.0, 300.0, 56.0];
        let mut row = probe(face, face, 1.0);
        row.inside = true;
        row.face_radius = 0.0;
        row.ring_radius = 12.0;
        assert!(check(&[row]).is_empty());
        // An outside ring on a square face still has to stay square.
        let out = outset(1.0);
        let ring = [-out, -out, 300.0 + 2.0 * out, 56.0 + 2.0 * out];
        let mut outside = probe(face, ring, 1.0);
        outside.face_radius = 0.0;
        outside.ring_radius = 12.0;
        assert_eq!(check(&[outside]).len(), 1);
    }

    #[test]
    fn a_face_that_draws_nothing_has_no_corners_to_follow_but_keeps_its_box() {
        let face = [0.0, 0.0, 100.0, 40.0];
        let out = outset(1.0);
        let ring = [-out, -out, 100.0 + 2.0 * out, 40.0 + 2.0 * out];
        let mut list = probe(face, ring, 1.0);
        list.painted = false;
        list.face_radius = 0.0;
        list.ring_radius = 12.0;
        assert!(check(&[list.clone()]).is_empty());
        // The box is still held to the face.
        list.ring = [0.0, 0.0, 100.0, 40.0];
        assert_eq!(check(&[list]).len(), 1);
    }

    #[test]
    fn a_ring_with_the_wrong_radius_is_reported() {
        let face = [0.0, 0.0, 100.0, 40.0];
        let out = outset(1.0);
        let ring = [-out, -out, 100.0 + 2.0 * out, 40.0 + 2.0 * out];
        let mut wrong = probe(face, ring, 1.0);
        wrong.ring_radius = 8.0;
        let found = check(&[wrong]);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].what, "the ring's corner radius does not follow the face's");
    }

    #[test]
    fn the_nested_radius_is_the_face_radius_plus_the_outset() {
        assert_eq!(expected_radius(8.0, 1.0, false), 8.0 + FOCUS_GAP + FOCUS_WIDTH);
        assert_eq!(expected_radius(0.0, 1.0, false), 0.0);
        assert_eq!(expected_radius(8.0, 1.0, true), 8.0);
    }
}
