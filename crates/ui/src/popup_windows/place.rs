//! Where a popup window goes: the request the compositor places it from, and a reference
//! implementation of how it answers.
//!
//! A popup window is an `xdg_popup`. The client does not say where it goes; it says what it is
//! next to: an *anchor rectangle* of its parent, which point of that rectangle it attaches to, which
//! way it grows from there, and what the compositor may do when the result would not fit on the
//! output. [`request`] turns what opened a panel ([`PanelTrigger`]) into that. The compositor
//! decides the rest: it knows the outputs, the client does not.
//!
//! [`resolve`] is the compositor's side, written from the `xdg_positioner` description in
//! `xdg-shell.xml`, for the tests and for the geometry the menu state machine assumes before the
//! compositor has answered.

use crate::controls::direction::Direction;
use crate::shell::panel::{PANEL_GAP, PanelRect, PanelTrigger, Side};

/// A point of a rectangle, or a direction: `xdg_positioner` anchor and gravity.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Edge {
    Centre,
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Edge {
    /// This edge on each axis: -1 for the start (left, top), 1 for the end, 0 for neither.
    fn axes(self) -> (i8, i8) {
        match self {
            Self::Centre => (0, 0),
            Self::Top => (0, -1),
            Self::Bottom => (0, 1),
            Self::Left => (-1, 0),
            Self::Right => (1, 0),
            Self::TopLeft => (-1, -1),
            Self::TopRight => (1, -1),
            Self::BottomLeft => (-1, 1),
            Self::BottomRight => (1, 1),
        }
    }

    fn from_axes(axes: (i8, i8)) -> Self {
        match axes {
            (0, 0) => Self::Centre,
            (0, -1) => Self::Top,
            (0, 1) => Self::Bottom,
            (-1, 0) => Self::Left,
            (1, 0) => Self::Right,
            (-1, -1) => Self::TopLeft,
            (1, -1) => Self::TopRight,
            (-1, 1) => Self::BottomLeft,
            _ => Self::BottomRight,
        }
    }

    fn flipped_x(self) -> Self {
        let (x, y) = self.axes();
        Self::from_axes((-x, y))
    }

    fn flipped_y(self) -> Self {
        let (x, y) = self.axes();
        Self::from_axes((x, -y))
    }
}

/// What the compositor may do when the popup would run off the output, per axis: flip it to the
/// other side of the anchor, or slide it along the edge until it fits. A flip is tried first.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) struct Adjust {
    pub(crate) flip_x: bool,
    pub(crate) flip_y: bool,
    pub(crate) slide_x: bool,
    pub(crate) slide_y: bool,
}

impl Adjust {
    /// Flip and slide on both axes: a context menu opened at a point.
    pub(crate) const ALL: Self = Self {
        flip_x: true,
        flip_y: true,
        slide_x: true,
        slide_y: true,
    };
}

/// A popup's place, asked for from the compositor. `anchor` is in the coordinates of the
/// parent's window geometry; `size` is the popup's card, which the compositor keeps on the output.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct PopupRequest {
    pub(crate) anchor: PanelRect,
    /// The point of `anchor` the popup attaches to.
    pub(crate) edge: Edge,
    /// The direction the popup grows from that point.
    pub(crate) gravity: Edge,
    pub(crate) adjust: Adjust,
    pub(crate) size: (f32, f32),
}

impl PopupRequest {
    /// The same request for a parent whose geometry starts at `origin` in the coordinates the
    /// anchor was given in.
    pub(crate) fn relative_to(self, origin: (f32, f32)) -> Self {
        Self {
            anchor: PanelRect {
                x: self.anchor.x - origin.0,
                y: self.anchor.y - origin.1,
                ..self.anchor
            },
            ..self
        }
    }
}

/// The request for a popup of `size` opened by `trigger`, in the coordinates of the trigger.
///
/// The gap between a popup and what opened it is part of the anchor rectangle, not an offset:
/// the rectangle grows by the gap on the side the popup opens to and on the side it flips to, so a
/// flipped popup keeps its gap whatever the compositor does with an offset.
pub(crate) fn request(trigger: PanelTrigger, size: (f32, f32), direction: Direction) -> PopupRequest {
    let rtl = direction.is_rtl();
    let anchor = trigger.anchor();
    match trigger {
        PanelTrigger::Pointer(_) => PopupRequest {
            // The menu's corner is at the pointer: the start corner, below it.
            anchor: PanelRect { width: 1.0, height: 1.0, ..anchor },
            edge: if rtl { Edge::TopRight } else { Edge::TopLeft },
            gravity: if rtl { Edge::BottomLeft } else { Edge::BottomRight },
            adjust: Adjust::ALL,
            size,
        },
        PanelTrigger::Row(_, side) => {
            let rect = PanelRect {
                x: anchor.x - PANEL_GAP,
                width: anchor.width + 2.0 * PANEL_GAP,
                ..anchor
            };
            let (edge, gravity) = match side {
                Side::Left => (Edge::TopLeft, Edge::BottomLeft),
                _ => (Edge::TopRight, Edge::BottomRight),
            };
            PopupRequest {
                anchor: rect,
                edge,
                gravity,
                adjust: Adjust { flip_x: true, flip_y: false, slide_x: true, slide_y: true },
                size,
            }
        }
        PanelTrigger::Control(_) | PanelTrigger::Keyboard(_) | PanelTrigger::Text { .. } => {
            let rect = PanelRect {
                y: anchor.y - PANEL_GAP,
                height: anchor.height + 2.0 * PANEL_GAP,
                ..anchor
            };
            let (edge, gravity) = if rtl {
                (Edge::BottomRight, Edge::BottomLeft)
            } else {
                (Edge::BottomLeft, Edge::BottomRight)
            };
            PopupRequest {
                anchor: rect,
                edge,
                gravity,
                adjust: Adjust { flip_x: false, flip_y: true, slide_x: true, slide_y: true },
                size,
            }
        }
    }
}

/// Where the compositor puts the popup of `request` when the parent's geometry has its top left at
/// `parent` and the output is `output` (both in the same global coordinates). The returned
/// rectangle is the popup's card in those coordinates.
///
/// Anchor and gravity place it; if it runs off the output on an axis and the request allows, the
/// anchor and gravity flip on that axis, kept only if the flipped place fits; if it still runs off
/// and the request allows, it slides along the axis until it fits.
pub(crate) fn resolve(request: &PopupRequest, parent: (f32, f32), output: PanelRect) -> PanelRect {
    let anchor = PanelRect {
        x: request.anchor.x + parent.0,
        y: request.anchor.y + parent.1,
        ..request.anchor
    };
    let (mut x, mut y) = placed(anchor, request.edge, request.gravity, request.size);

    if constrained_x(x, request.size.0, output) && request.adjust.flip_x {
        let (flipped, _) =
            placed(anchor, request.edge.flipped_x(), request.gravity.flipped_x(), request.size);
        if !constrained_x(flipped, request.size.0, output) {
            x = flipped;
        }
    }
    if constrained_y(y, request.size.1, output) && request.adjust.flip_y {
        let (_, flipped) =
            placed(anchor, request.edge.flipped_y(), request.gravity.flipped_y(), request.size);
        if !constrained_y(flipped, request.size.1, output) {
            y = flipped;
        }
    }
    if request.adjust.slide_x {
        x = slid(x, request.size.0, output.x, output.right());
    }
    if request.adjust.slide_y {
        y = slid(y, request.size.1, output.y, output.bottom());
    }
    PanelRect { x, y, width: request.size.0, height: request.size.1 }
}

/// The top left of a popup of `size` attached by `edge` of `anchor`, growing `gravity`-wards.
fn placed(anchor: PanelRect, edge: Edge, gravity: Edge, size: (f32, f32)) -> (f32, f32) {
    let (ex, ey) = edge.axes();
    let point = (
        anchor.x + anchor.width * (f32::from(ex) + 1.0) / 2.0,
        anchor.y + anchor.height * (f32::from(ey) + 1.0) / 2.0,
    );
    // Growing towards the end (right, down) puts the start corner on the point; towards the
    // start the end corner; neither centres it.
    let (gx, gy) = gravity.axes();
    (
        point.0 - size.0 * (1.0 - f32::from(gx)) / 2.0,
        point.1 - size.1 * (1.0 - f32::from(gy)) / 2.0,
    )
}

fn constrained_x(x: f32, width: f32, output: PanelRect) -> bool {
    x < output.x || x + width > output.right()
}

fn constrained_y(y: f32, height: f32, output: PanelRect) -> bool {
    y < output.y || y + height > output.bottom()
}

/// `start` moved the least that puts `[start, start + size]` inside `[low, high]`: the start edge
/// wins when it does not fit at all.
fn slid(start: f32, size: f32, low: f32, high: f32) -> f32 {
    if start + size > high {
        (high - size).max(low)
    } else if start < low {
        low
    } else {
        start
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OUT: PanelRect = PanelRect { x: 0.0, y: 0.0, width: 1000.0, height: 800.0 };

    fn pointer(at: (f32, f32)) -> PanelTrigger {
        PanelTrigger::Pointer(at)
    }

    fn card(trigger: PanelTrigger, size: (f32, f32), parent: (f32, f32), output: PanelRect) -> PanelRect {
        // The trigger is in the coordinates of a parent at `parent`.
        let asked = request(trigger, size, Direction::Ltr);
        resolve(&asked, parent, output)
    }

    #[test]
    fn a_context_menu_opens_below_and_after_the_pointer() {
        let placed = card(pointer((100.0, 200.0)), (180.0, 240.0), (0.0, 0.0), OUT);
        assert_eq!((placed.x, placed.y), (100.0, 200.0));
    }

    #[test]
    fn a_context_menu_in_a_right_to_left_window_opens_towards_the_start() {
        let asked = request(pointer((500.0, 200.0)), (180.0, 240.0), Direction::Rtl);
        let placed = resolve(&asked, (0.0, 0.0), OUT);
        assert_eq!((placed.x, placed.y), (501.0 - 180.0, 200.0));
        assert_eq!(placed.right(), 501.0);
    }

    #[test]
    fn near_the_bottom_edge_it_flips_above_the_pointer() {
        let placed = card(pointer((100.0, 700.0)), (180.0, 240.0), (0.0, 0.0), OUT);
        // Flipped: the bottom edge, not the top, sits on the pointer (the anchor is 1 px tall).
        assert_eq!(placed.y, 701.0 - 240.0);
        assert_eq!(placed.x, 100.0);
    }

    #[test]
    fn near_the_right_edge_it_flips_before_the_pointer() {
        let placed = card(pointer((900.0, 100.0)), (180.0, 240.0), (0.0, 0.0), OUT);
        // The anchor is 1 px wide: the flipped card ends on its far side.
        assert_eq!(placed.x, 901.0 - 180.0);
    }

    #[test]
    fn in_the_corner_it_flips_on_both_axes() {
        let placed = card(pointer((950.0, 780.0)), (180.0, 240.0), (0.0, 0.0), OUT);
        assert_eq!((placed.x, placed.y), (771.0, 541.0));
    }

    #[test]
    fn a_parent_off_the_output_origin_is_placed_in_global_coordinates() {
        // The window starts at 600,500: a menu at its (300,300) is at 900,800 on the output.
        // The flipped card would end at 801, one pixel past the output (the anchor is 1 px
        // tall), so the flip is refused and it slides up to the edge instead.
        let placed = card(pointer((300.0, 300.0)), (180.0, 240.0), (600.0, 500.0), OUT);
        assert_eq!((placed.x, placed.y), (901.0 - 180.0, 800.0 - 240.0));
    }

    #[test]
    fn it_flips_only_when_the_other_side_fits() {
        // A menu taller than either side of the anchor: the flipped place is no better, so the
        // first one is kept and then slid up to fit.
        let tall = (180.0, 700.0);
        let placed = card(pointer((100.0, 400.0)), tall, (0.0, 0.0), OUT);
        assert_eq!(placed.y, 100.0);
        assert!(placed.bottom() <= OUT.bottom());
    }

    #[test]
    fn a_popup_too_big_for_the_output_keeps_its_start_edge() {
        let huge = (1200.0, 900.0);
        let placed = card(pointer((100.0, 100.0)), huge, (0.0, 0.0), OUT);
        assert_eq!((placed.x, placed.y), (0.0, 0.0));
    }

    #[test]
    fn an_output_left_of_the_origin_has_negative_coordinates() {
        let left = PanelRect { x: -1920.0, y: 0.0, width: 1920.0, height: 1080.0 };
        // A window on that output; the menu goes past the output's right edge (x = 0).
        let placed = card(pointer((1800.0, 100.0)), (180.0, 240.0), (-1920.0, 0.0), left);
        // The anchor is 1 px wide, so the flipped card ends on its far side: -119.
        assert_eq!(placed.x, -119.0 - 180.0);
        // And near its left edge nothing moves.
        let placed = card(pointer((10.0, 100.0)), (180.0, 240.0), (-1920.0, 0.0), left);
        assert_eq!(placed.x, -1910.0);
    }

    #[test]
    fn a_menu_below_a_control_sits_a_gap_under_it_and_flips_above_it() {
        let control = PanelRect { x: 40.0, y: 100.0, width: 120.0, height: 36.0 };
        let asked = request(PanelTrigger::Control(control), (200.0, 160.0), Direction::Ltr);
        let placed = resolve(&asked, (0.0, 0.0), OUT);
        assert_eq!((placed.x, placed.y), (40.0, 136.0 + PANEL_GAP));

        let low = PanelRect { x: 40.0, y: 700.0, width: 120.0, height: 36.0 };
        let asked = request(PanelTrigger::Control(low), (200.0, 160.0), Direction::Ltr);
        let placed = resolve(&asked, (0.0, 0.0), OUT);
        assert_eq!(placed.y, 700.0 - PANEL_GAP - 160.0);
        assert_eq!(placed.x, 40.0);
    }

    #[test]
    fn a_menu_under_a_control_slides_when_it_is_wider_than_the_room() {
        let control = PanelRect { x: 900.0, y: 100.0, width: 80.0, height: 36.0 };
        let asked = request(PanelTrigger::Control(control), (200.0, 160.0), Direction::Ltr);
        let placed = resolve(&asked, (0.0, 0.0), OUT);
        // No flip on x for a control's menu: it slides back inside.
        assert_eq!(placed.x, 800.0);
    }

    #[test]
    fn a_submenu_opens_beside_its_row_and_flips_to_the_other_side() {
        let row = PanelRect { x: 100.0, y: 220.0, width: 200.0, height: 32.0 };
        let asked = request(PanelTrigger::Row(row, Side::Right), (180.0, 120.0), Direction::Ltr);
        let placed = resolve(&asked, (0.0, 0.0), OUT);
        assert_eq!((placed.x, placed.y), (300.0 + PANEL_GAP, 220.0));

        // The row's panel is at the right edge: the submenu goes to its left.
        let edge_row = PanelRect { x: 760.0, y: 220.0, width: 200.0, height: 32.0 };
        let asked = request(PanelTrigger::Row(edge_row, Side::Right), (180.0, 120.0), Direction::Ltr);
        let placed = resolve(&asked, (0.0, 0.0), OUT);
        assert_eq!(placed.x, 760.0 - PANEL_GAP - 180.0);
        assert_eq!(placed.y, 220.0);
    }

    #[test]
    fn a_submenu_near_the_bottom_slides_up_instead_of_flipping() {
        let row = PanelRect { x: 100.0, y: 740.0, width: 200.0, height: 32.0 };
        let asked = request(PanelTrigger::Row(row, Side::Right), (180.0, 120.0), Direction::Ltr);
        let placed = resolve(&asked, (0.0, 0.0), OUT);
        assert_eq!(placed.y, 800.0 - 120.0);
        assert_eq!(placed.x, 300.0 + PANEL_GAP);
    }

    #[test]
    fn a_request_is_relative_to_the_parent_it_is_made_from() {
        let asked = request(pointer((130.0, 90.0)), (100.0, 50.0), Direction::Ltr);
        let from_popup = asked.relative_to((30.0, 40.0));
        assert_eq!((from_popup.anchor.x, from_popup.anchor.y), (100.0, 50.0));
        // Same card whichever frame the anchor was given in.
        assert_eq!(
            resolve(&from_popup, (30.0, 40.0), OUT),
            resolve(&asked, (0.0, 0.0), OUT)
        );
    }

    #[test]
    fn flipping_swaps_the_edge_and_the_gravity_on_one_axis() {
        assert_eq!(Edge::TopLeft.flipped_x(), Edge::TopRight);
        assert_eq!(Edge::TopLeft.flipped_y(), Edge::BottomLeft);
        assert_eq!(Edge::Left.flipped_x(), Edge::Right);
        assert_eq!(Edge::Top.flipped_x(), Edge::Top);
        assert_eq!(Edge::Centre.flipped_y(), Edge::Centre);
    }
}
