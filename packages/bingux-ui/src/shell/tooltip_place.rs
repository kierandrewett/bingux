//! Where a hover tooltip sits against its control, decided as one pure step.
//!
//! The decision is a function of the control's rectangle, the bubble's size,
//! the window and the side the bubble has been on, and of nothing else. A
//! bubble that is re-decided every frame therefore stays where it is while the
//! pointer rests, and the side is remembered by the caller, so a window edge
//! cannot make it flip back and forth:
//!
//! - The bubble opens on the first side that has room, in the order below,
//!   above, right, left.
//! - A bubble that is already on a side stays there for as long as that side
//!   has room. It leaves only when staying would put it outside the window.
//! - It sits beside its control, never over it. It is centred on the control
//!   along the control's edge, and held inside the window across it.
//! - A size change (a new label) keeps the edge that touches the control, so
//!   the bubble grows away from the control and about its middle.

use super::panel::{PANEL_EDGE, PANEL_GAP, PanelRect, Side};

/// Where a bubble goes: its top-left corner in window coordinates, and the
/// side of the control it is on.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct Placement {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) side: Side,
}

/// The order a bubble tries sides in when it has none yet.
const ORDER: [Side; 4] = [Side::Below, Side::Above, Side::Right, Side::Left];

/// Where a bubble of `size` goes beside `anchor` in a window of `window`.
/// `previous` is the side the bubble was on at its last placement, if it was
/// placed: it is kept while it fits. With no window size yet, the bubble sits
/// on `previous` or below, unclamped.
///
/// When no side has room, which needs a window smaller than the control and
/// bubble together, the side with the most room is used, and the bubble stays
/// against the control and may run past the window edge: it is never moved
/// over the control.
pub(crate) fn decide(
    anchor: PanelRect,
    size: (f32, f32),
    window: (f32, f32),
    previous: Option<Side>,
) -> Placement {
    let unknown = window.0 <= 0.0 || window.1 <= 0.0;
    let side = if unknown {
        previous.unwrap_or(Side::Below)
    } else {
        previous
            .filter(|&side| fits(side, anchor, size, window))
            .or_else(|| {
                ORDER
                    .into_iter()
                    .find(|&side| fits(side, anchor, size, window))
            })
            .unwrap_or_else(|| roomiest(anchor, size, window))
    };
    let (x, y) = beside(side, anchor, size);
    if unknown {
        return Placement { x, y, side };
    }
    // Hold the bubble inside the window along the control's edge. Across it,
    // a side that fits is already inside; a last-resort side stays put.
    let (x, y) = match side {
        Side::Below | Side::Above => (clamp_inside(x, size.0, window.0), y),
        Side::Right | Side::Left => (x, clamp_inside(y, size.1, window.1)),
    };
    Placement { x, y, side }
}

/// The bubble's top-left corner on `side` of `anchor`, centred on the anchor
/// along its edge and a gap away from it.
fn beside(side: Side, anchor: PanelRect, (width, height): (f32, f32)) -> (f32, f32) {
    let centre_x = anchor.x + anchor.width / 2.0;
    let centre_y = anchor.y + anchor.height / 2.0;
    match side {
        Side::Below => (centre_x - width / 2.0, anchor.bottom() + PANEL_GAP),
        Side::Above => (centre_x - width / 2.0, anchor.y - PANEL_GAP - height),
        Side::Right => (anchor.right() + PANEL_GAP, centre_y - height / 2.0),
        Side::Left => (anchor.x - PANEL_GAP - width, centre_y - height / 2.0),
    }
}

/// Whether a bubble on `side` lies inside the window, with the edge margin,
/// along the axis it moves away from the control on.
fn fits(side: Side, anchor: PanelRect, size: (f32, f32), window: (f32, f32)) -> bool {
    room(side, anchor, window) >= extent(side, size)
}

/// How much space there is on `side` of the anchor, after the gap and the
/// window's edge margin. It is negative when the anchor is closer to the
/// window's edge than that.
fn room(side: Side, anchor: PanelRect, window: (f32, f32)) -> f32 {
    match side {
        Side::Below => window.1 - PANEL_EDGE - (anchor.bottom() + PANEL_GAP),
        Side::Above => anchor.y - PANEL_GAP - PANEL_EDGE,
        Side::Right => window.0 - PANEL_EDGE - (anchor.right() + PANEL_GAP),
        Side::Left => anchor.x - PANEL_GAP - PANEL_EDGE,
    }
}

/// How far a bubble of `size` reaches away from the control on `side`.
fn extent(side: Side, size: (f32, f32)) -> f32 {
    match side {
        Side::Below | Side::Above => size.1,
        Side::Right | Side::Left => size.0,
    }
}

/// The side with the most room, the first in [`ORDER`] on a tie.
fn roomiest(anchor: PanelRect, size: (f32, f32), window: (f32, f32)) -> Side {
    let spare = |side: Side| room(side, anchor, window) - extent(side, size);
    ORDER
        .into_iter()
        .fold((ORDER[0], f32::NEG_INFINITY), |best, side| {
            let spare = spare(side);
            if spare > best.1 { (side, spare) } else { best }
        })
        .0
}

/// Holds a bubble of `size` on one axis inside `limit`, keeping the edge margin.
fn clamp_inside(value: f32, size: f32, limit: f32) -> f32 {
    value.min(limit - size - PANEL_EDGE).max(PANEL_EDGE)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: (f32, f32) = (800.0, 600.0);
    const BUBBLE: (f32, f32) = (80.0, 28.0);

    fn control(x: f32, y: f32) -> PanelRect {
        PanelRect {
            x,
            y,
            width: 28.0,
            height: 28.0,
        }
    }

    fn rect_of(placement: Placement, size: (f32, f32)) -> PanelRect {
        PanelRect {
            x: placement.x,
            y: placement.y,
            width: size.0,
            height: size.1,
        }
    }

    fn overlaps(a: PanelRect, b: PanelRect) -> bool {
        a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom()
    }

    #[test]
    fn opens_below_and_centred_when_there_is_room() {
        let anchor = control(300.0, 200.0);
        let placed = decide(anchor, BUBBLE, WINDOW, None);
        assert_eq!(placed.side, Side::Below);
        assert_eq!(placed.y, anchor.bottom() + PANEL_GAP);
        assert_eq!(placed.x + BUBBLE.0 / 2.0, anchor.x + anchor.width / 2.0);
    }

    #[test]
    fn flips_above_when_below_has_no_room() {
        let anchor = control(300.0, WINDOW.1 - 40.0);
        let placed = decide(anchor, BUBBLE, WINDOW, None);
        assert_eq!(placed.side, Side::Above);
        assert_eq!(placed.y + BUBBLE.1 + PANEL_GAP, anchor.y);
    }

    #[test]
    fn goes_beside_the_control_when_neither_above_nor_below_has_room() {
        // A bubble taller than the space above and below a control mid-window.
        let anchor = control(300.0, 280.0);
        let tall = (120.0, 300.0);
        let placed = decide(anchor, tall, WINDOW, None);
        assert_eq!(placed.side, Side::Right);
        assert_eq!(placed.x, anchor.right() + PANEL_GAP);
        // And left when right has no room either.
        let anchor = control(WINDOW.0 - 40.0, 280.0);
        let placed = decide(anchor, tall, WINDOW, None);
        assert_eq!(placed.side, Side::Left);
        assert_eq!(placed.x + tall.0 + PANEL_GAP, anchor.x);
    }

    #[test]
    fn a_side_it_is_on_is_kept_while_it_fits() {
        // Room on both sides: a bubble that was above stays above, one that was
        // below stays below. Without a side it opens below.
        let anchor = control(300.0, 300.0);
        assert_eq!(decide(anchor, BUBBLE, WINDOW, None).side, Side::Below);
        assert_eq!(decide(anchor, BUBBLE, WINDOW, Some(Side::Above)).side, Side::Above);
        assert_eq!(decide(anchor, BUBBLE, WINDOW, Some(Side::Below)).side, Side::Below);
        assert_eq!(decide(anchor, BUBBLE, WINDOW, Some(Side::Right)).side, Side::Right);
    }

    #[test]
    fn leaves_a_remembered_side_only_when_it_would_leave_the_window() {
        let anchor = control(300.0, WINDOW.1 - 40.0);
        // Below no longer fits: it moves above, as with no side at all.
        assert_eq!(decide(anchor, BUBBLE, WINDOW, Some(Side::Below)).side, Side::Above);
        // Above no longer fits: it moves below.
        let near_top = control(300.0, 20.0);
        assert_eq!(decide(near_top, BUBBLE, WINDOW, Some(Side::Above)).side, Side::Below);
    }

    #[test]
    fn a_control_on_the_boundary_does_not_make_the_side_oscillate() {
        // Slide a control down through the point where below stops fitting and
        // back up through it. Fed its own previous side, the bubble flips once
        // going down and stays above going back up, until above itself fails.
        let mut side = None;
        let mut sides = Vec::new();
        let steps = (100..=(WINDOW.1 as i32 - 30)).chain((100..=(WINDOW.1 as i32 - 30)).rev());
        for y in steps {
            let placed = decide(control(300.0, y as f32), BUBBLE, WINDOW, side);
            side = Some(placed.side);
            if sides.last() != Some(&placed.side) {
                sides.push(placed.side);
            }
        }
        assert_eq!(sides, [Side::Below, Side::Above]);
    }

    #[test]
    fn a_bubble_that_grows_flips_once_and_does_not_come_back_when_it_shrinks() {
        // A control near the bottom: a one-line bubble fits below, a wrapped one
        // does not. It flips above when it grows and stays when it shrinks.
        let anchor = control(300.0, WINDOW.1 - 90.0);
        let one = (100.0, 28.0);
        let wrapped = (100.0, 84.0);
        let first = decide(anchor, one, WINDOW, None);
        assert_eq!(first.side, Side::Below);
        let grown = decide(anchor, wrapped, WINDOW, Some(first.side));
        assert_eq!(grown.side, Side::Above);
        let shrunk = decide(anchor, one, WINDOW, Some(grown.side));
        assert_eq!(shrunk.side, Side::Above);
    }

    #[test]
    fn repeating_the_decision_does_not_move_the_bubble() {
        let anchor = control(120.0, WINDOW.1 - 44.0);
        let first = decide(anchor, BUBBLE, WINDOW, None);
        let mut previous = Some(first.side);
        for _ in 0..8 {
            let again = decide(anchor, BUBBLE, WINDOW, previous);
            assert_eq!(again, first);
            previous = Some(again.side);
        }
    }

    #[test]
    fn is_held_inside_the_window_along_the_controls_edge() {
        let left = decide(control(2.0, 200.0), BUBBLE, WINDOW, None);
        assert_eq!(left.side, Side::Below);
        assert_eq!(left.x, PANEL_EDGE);
        let right = decide(control(WINDOW.0 - 30.0, 200.0), BUBBLE, WINDOW, None);
        assert_eq!(right.side, Side::Below);
        assert_eq!(right.x, WINDOW.0 - BUBBLE.0 - PANEL_EDGE);
        // A bubble beside a control near the top is held down.
        let top = decide(control(300.0, 2.0), (120.0, 560.0), WINDOW, None);
        assert_eq!(top.side, Side::Right);
        assert_eq!(top.y, PANEL_EDGE);
    }

    #[test]
    fn a_new_size_keeps_the_edge_on_the_control_and_the_middle() {
        let sizes = [(40.0, 28.0), (96.0, 28.0), (140.0, 56.0)];
        for side in [Side::Below, Side::Above, Side::Right, Side::Left] {
            let anchor = control(400.0, 300.0);
            for size in sizes {
                let placed = decide(anchor, size, WINDOW, Some(side));
                assert_eq!(placed.side, side);
                let bubble = rect_of(placed, size);
                match side {
                    Side::Below => {
                        assert_eq!(bubble.y, anchor.bottom() + PANEL_GAP);
                        assert_eq!(bubble.x + bubble.width / 2.0, anchor.x + anchor.width / 2.0);
                    }
                    Side::Above => {
                        assert_eq!(bubble.bottom() + PANEL_GAP, anchor.y);
                        assert_eq!(bubble.x + bubble.width / 2.0, anchor.x + anchor.width / 2.0);
                    }
                    Side::Right => {
                        assert_eq!(bubble.x, anchor.right() + PANEL_GAP);
                        assert_eq!(bubble.y + bubble.height / 2.0, anchor.y + anchor.height / 2.0);
                    }
                    Side::Left => {
                        assert_eq!(bubble.right() + PANEL_GAP, anchor.x);
                        assert_eq!(bubble.y + bubble.height / 2.0, anchor.y + anchor.height / 2.0);
                    }
                }
            }
        }
    }

    #[test]
    fn never_covers_the_control_while_any_side_has_room() {
        let sizes = [(40.0, 24.0), (96.0, 28.0), (280.0, 112.0), (160.0, 300.0)];
        for size in sizes {
            // Controls that lie inside the window, from corner to corner.
            for x in (0..=(WINDOW.0 as i32 - 28)).step_by(36) {
                for y in (0..=(WINDOW.1 as i32 - 28)).step_by(36) {
                    let anchor = control(x as f32, y as f32);
                    for previous in [None, Some(Side::Below), Some(Side::Above), Some(Side::Right), Some(Side::Left)] {
                        let placed = decide(anchor, size, WINDOW, previous);
                        let any_fits = ORDER.into_iter().any(|side| fits(side, anchor, size, WINDOW));
                        if any_fits {
                            assert!(
                                !overlaps(rect_of(placed, size), anchor),
                                "{size:?} over a control at ({x}, {y}), previous {previous:?}: {placed:?}"
                            );
                            let bubble = rect_of(placed, size);
                            assert!(
                                bubble.x >= PANEL_EDGE - 0.001
                                    && bubble.y >= PANEL_EDGE - 0.001
                                    && bubble.right() <= WINDOW.0 - PANEL_EDGE + 0.001
                                    && bubble.bottom() <= WINDOW.1 - PANEL_EDGE + 0.001,
                                "{size:?} outside the window for a control at ({x}, {y}): {placed:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn with_no_room_anywhere_it_takes_the_roomiest_side_and_stays_off_the_control() {
        let small = (120.0, 90.0);
        let anchor = control(40.0, 30.0);
        let placed = decide(anchor, small, small, None);
        assert!(!overlaps(rect_of(placed, small), anchor));
        // Same decision again: no oscillation between sides of equal standing.
        assert_eq!(decide(anchor, small, small, Some(placed.side)), placed);
    }

    #[test]
    fn with_no_window_size_it_sits_on_the_previous_side_unclamped() {
        let anchor = control(0.0, 0.0);
        let placed = decide(anchor, BUBBLE, (0.0, 0.0), None);
        assert_eq!(placed.side, Side::Below);
        assert_eq!(placed.x, anchor.x + anchor.width / 2.0 - BUBBLE.0 / 2.0);
        let above = decide(anchor, BUBBLE, (0.0, 0.0), Some(Side::Above));
        assert_eq!(above.side, Side::Above);
    }
}
