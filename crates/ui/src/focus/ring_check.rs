//! The focus ring check: every ring must hug the box that is painted.
//!
//! A control declares its **face** by ending `render` with `.focusable(..)`
//! on the node that carries the fill. The `focusable` wrapper hugs the face,
//! measures it after layout, and draws the ring from that measured box and
//! the declared radius, so the face's padding, content and margin cannot move
//! it. This module checks the result: for every ring in a laid-out tree, the
//! ring's rectangle must equal the face's border box grown by the ring's
//! outset (gap, width and, while it animates, travel), within half a pixel,
//! and each of its four corner radii must be the face's painted radius there
//! plus that outset (nested), or zero where the face's corner is square: a
//! rounded ring around a square face and a square ring around a rounded face
//! both fail.
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
    /// The face's four corner radii as painted: top left, top right, bottom
    /// right, bottom left.
    pub face_radii: [f32; 4],
    /// The ring node's border box.
    pub ring: Box4,
    /// The ring's own opacity, which is its animation progress: 0 unfocused,
    /// 1 settled.
    pub progress: f32,
    /// The ring's four corner radii, in the same order.
    pub ring_radii: [f32; 4],
    /// Whether the ring sits inside the face (a face on a clipped edge)
    /// instead of outside it.
    pub inside: bool,
    /// Whether the ring node carries its blur halo (while it arrives or
    /// leaves; a settled ring is crisp).
    pub blurred: bool,
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

/// The corner radii a ring needs for a face of `face` radii: nested, so the
/// ring's outer edge follows each round corner, and square where the face's
/// corner is square.
pub fn expected_radii(face: [f32; 4], progress: f32, inside: bool) -> [f32; 4] {
    face.map(|radius| {
        if inside || radius == 0.0 {
            radius
        } else {
            radius + outset(progress)
        }
    })
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
        let radii = expected_radii(probe.face_radii, probe.progress, probe.inside);
        for corner in 0..4 {
            let (want, got) = (radii[corner], probe.ring_radii[corner]);
            if (got - want).abs() <= RADIUS_TOLERANCE {
                continue;
            }
            let what = if probe.face_radii[corner] == 0.0 {
                "a rounded ring corner around a square face corner"
            } else if got <= RADIUS_TOLERANCE {
                "a square ring corner around a rounded face corner"
            } else {
                "the ring's corner radius does not follow the face's"
            };
            found.push(Mismatch { label: probe.label.clone(), what, expected: radii, found: probe.ring_radii });
            break;
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
        let ring_radii = ring.props.as_ref().map_or([0.0; 4], |props| props.corner_radii);
        let progress = ring.props.as_ref().and_then(|props| props.opacity).unwrap_or(1.0);
        let painter = painted(snapshot, face, face_box);
        found.push((
            ring.id,
            RingProbe {
                label: path_of(snapshot, face),
                face: face_box,
                // A face that is not a box has no radii of its own to follow.
                face_radii: painter.props.as_ref().map_or_else(
                    || expected_inverse(ring_radii, progress, inside),
                    |props| props.corner_radii,
                ),
                ring: ring_box,
                progress,
                ring_radii,
                inside,
                blurred: ring.props.as_ref().is_some_and(|props| !props.shadows.is_empty()),
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
            || props.corner_radii.iter().any(|radius| *radius > 0.0)
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

/// The face radii a ring of `ring` radii is nested around.
fn expected_inverse(ring: [f32; 4], progress: f32, inside: bool) -> [f32; 4] {
    ring.map(|radius| {
        if inside || radius == 0.0 {
            radius
        } else {
            (radius - outset(progress)).max(0.0)
        }
    })
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
            face_radii: [8.0; 4],
            ring,
            progress,
            ring_radii: [8.0 + outset(progress); 4],
            inside: false,
            blurred: false,
        }
    }

    /// A settled outside ring around `face` with the given face and ring radii.
    fn settled(face_radii: [f32; 4], ring_radii: [f32; 4]) -> RingProbe {
        let face = [20.0, 20.0, 100.0, 40.0];
        let out = outset(1.0);
        let ring = [face[0] - out, face[1] - out, face[2] + 2.0 * out, face[3] + 2.0 * out];
        RingProbe { face_radii, ring_radii, ..probe(face, ring, 1.0) }
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
        inside.ring_radii = [8.0; 4];
        assert!(check(&[inside]).is_empty());
    }

    #[test]
    fn an_inside_ring_takes_the_faces_radii_and_no_others() {
        let face = [0.0, 0.0, 300.0, 56.0];
        let mut row = probe(face, face, 1.0);
        row.inside = true;
        row.face_radii = [0.0, 0.0, 12.0, 0.0];
        row.ring_radii = [0.0, 0.0, 12.0, 0.0];
        assert!(check(&[row.clone()]).is_empty());
        // A rounded ring inside a square face is the old exemption: now wrong.
        row.face_radii = [0.0; 4];
        let found = check(&[row]);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].what, "a rounded ring corner around a square face corner");
    }

    #[test]
    fn a_square_face_gets_a_square_ring() {
        let out = outset(1.0);
        assert!(check(&[settled([0.0; 4], [0.0; 4])]).is_empty());
        let found = check(&[settled([0.0; 4], [12.0; 4])]);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].what, "a rounded ring corner around a square face corner");
        assert_eq!(found[0].expected, [0.0; 4], "{out}");
    }

    #[test]
    fn a_rounded_face_gets_a_rounded_ring() {
        let out = outset(1.0);
        assert!(check(&[settled([8.0; 4], [8.0 + out; 4])]).is_empty());
        let found = check(&[settled([8.0; 4], [0.0; 4])]);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].what, "a square ring corner around a rounded face corner");
        let found = check(&[settled([8.0; 4], [8.0; 4])]);
        assert_eq!(found[0].what, "the ring's corner radius does not follow the face's");
    }

    #[test]
    fn mixed_corners_are_held_one_by_one() {
        let out = outset(1.0);
        // A tab: round top corners, square bottom ones.
        let tab = [10.0, 10.0, 0.0, 0.0];
        let ring = [10.0 + out, 10.0 + out, 0.0, 0.0];
        assert!(check(&[settled(tab, ring)]).is_empty());
        // The same ring on a face whose corners are the other way is wrong at
        // every corner, and the first bad corner is reported.
        assert_eq!(check(&[settled([0.0, 0.0, 10.0, 10.0], ring)]).len(), 1);
        // One corner off in an otherwise exact ring.
        let found = check(&[settled(tab, [10.0 + out, 0.0, 0.0, 0.0])]);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].what, "a square ring corner around a rounded face corner");
    }

    #[test]
    fn a_face_that_draws_nothing_is_square_and_its_ring_is_too() {
        let list = settled([0.0; 4], [12.0; 4]);
        assert_eq!(check(&[list.clone()]).len(), 1);
        // The box is still held to the face.
        let mut moved = settled([0.0; 4], [0.0; 4]);
        moved.ring = [0.0, 0.0, 100.0, 40.0];
        assert_eq!(check(&[moved]).len(), 1);
    }

    #[test]
    fn the_nested_radii_are_each_the_face_radius_plus_the_outset() {
        let out = FOCUS_GAP + FOCUS_WIDTH;
        assert_eq!(expected_radii([8.0; 4], 1.0, false), [8.0 + out; 4]);
        assert_eq!(expected_radii([0.0; 4], 1.0, false), [0.0; 4]);
        assert_eq!(expected_radii([8.0, 0.0, 4.0, 0.0], 1.0, false), [8.0 + out, 0.0, 4.0 + out, 0.0]);
        assert_eq!(expected_radii([8.0; 4], 1.0, true), [8.0; 4]);
    }
}
