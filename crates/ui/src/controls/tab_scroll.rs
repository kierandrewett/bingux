//! Where a strip's tabs are scrolled to, with no widgets in it, so each rule
//! can be tested without a window.
//!
//! An offset is *logical*: how far the tabs are scrolled from the strip's
//! start edge, `0.0` with the first tab at the start and `range` with the last
//! at the end, whichever side the start is on. Only at the end, when a tab is
//! placed on the screen, is it made physical ([`physical`]) and mirrored for a
//! right-to-left strip. So the rules that decide what scrolls where (the wheel,
//! a finger, the selected tab coming into view) are the same in both
//! directions and are written once.
//!
//! * [`wheel`] and [`pan`]: what a wheel notch, a touchpad's two-finger
//!   movement and a finger dragged along the strip move the tabs by.
//! * [`into_view`]: the offset that brings a tab into the region.
//! * [`physical`], [`place`]: logical to screen, mirrored right to left.

use super::direction::Direction;
use super::tab_geometry::Geometry;

/// How far a finger must travel along the strip before it scrolls it rather
/// than taps a tab, in logical pixels.
pub(crate) const PAN_SLOP: f32 = 6.0;

/// `offset` kept in `0..=range`.
pub(crate) fn clamp(offset: f32, range: f32) -> f32 {
    offset.clamp(0.0, range.max(0.0))
}

/// The distance from the region's left edge the tabs are scrolled by, for the
/// logical `offset`. Left to right it is the offset; right to left the start is
/// at the right, so the tabs are scrolled by what is left of the range.
pub(crate) fn physical(offset: f32, range: f32, direction: Direction) -> f32 {
    if direction.is_rtl() { range - offset } else { offset }
}

/// Where a box that starts `start` px into the tabs' content (in reading
/// order) and is `width` wide begins in the content as it is laid out on the
/// screen: the same distance from the left edge, or, right to left, from the
/// right edge.
pub(crate) fn place(start: f32, width: f32, content: f32, direction: Direction) -> f32 {
    if direction.is_rtl() { content - start - width } else { start }
}

/// What a wheel notch or a touchpad movement moves the tabs by, along the
/// reading direction: positive towards the end. `dx` and `dy` are the event's
/// deltas, as Freya reports them: a positive `dy` is the wheel turned down, a
/// positive `dx` is the content going left. A strip has one axis, so the
/// vertical wheel scrolls it too, forwards when turned down in either
/// direction; a sideways movement is physical: it moves the tabs left or right
/// as the fingers did, which is backwards in a right-to-left strip.
pub(crate) fn wheel(dx: f64, dy: f64, direction: Direction) -> f32 {
    dy as f32 + direction.sign() * dx as f32
}

/// The offset a finger dragged along the strip leads to: the tabs follow the
/// finger, so moving it `travelled` px to the right (positive) shows more of
/// what is on the left, that is, earlier in a left-to-right strip and later in
/// a right-to-left one. `from` is the offset when the finger went down.
pub(crate) fn pan(from: f32, travelled: f32, range: f32, direction: Direction) -> f32 {
    clamp(from - direction.sign() * travelled, range)
}

/// The offset, from the start of the tabs, that brings the tab at `index`
/// into a region `region` wide without moving more than it must: `current`
/// is where the region starts now. A tab that is wider than the region cannot
/// be shown whole: its start is shown. Distances
/// are in reading order, so the same offset brings the tab in view on either
/// side.
pub(crate) fn into_view(index: usize, geometry: Geometry, tab: f32, region: f32, current: f32) -> f32 {
    let left = index as f32 * (tab + geometry.gap());
    let right = left + tab;
    if left < current {
        left
    } else if right > current + region {
        (right - region).min(left)
    } else {
        current
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::SizeClass;

    #[test]
    fn a_logical_offset_is_the_screens_from_the_left_and_from_the_right() {
        assert_eq!(physical(30.0, 100.0, Direction::Ltr), 30.0);
        assert_eq!(physical(30.0, 100.0, Direction::Rtl), 70.0);
        for direction in [Direction::Ltr, Direction::Rtl] {
            // Its own inverse: the offset from the other end of the range.
            assert_eq!(physical(physical(30.0, 100.0, direction), 100.0, direction), 30.0);
        }
        // The start is at the start edge: nothing scrolled shows the first tab
        // there, on the left and on the right.
        assert_eq!(physical(0.0, 100.0, Direction::Ltr), 0.0);
        assert_eq!(physical(0.0, 100.0, Direction::Rtl), 100.0, "right to left the content is already at its right end");
    }

    #[test]
    fn a_box_is_placed_from_the_right_edge_right_to_left() {
        assert_eq!(place(10.0, 40.0, 300.0, Direction::Ltr), 10.0);
        assert_eq!(place(10.0, 40.0, 300.0, Direction::Rtl), 250.0);
        // The first box and the last are at opposite ends.
        assert_eq!(place(0.0, 40.0, 300.0, Direction::Rtl), 260.0);
        assert_eq!(place(260.0, 40.0, 300.0, Direction::Rtl), 0.0);
    }

    #[test]
    fn the_vertical_wheel_scrolls_forwards_and_a_sideways_movement_follows_the_fingers() {
        for direction in [Direction::Ltr, Direction::Rtl] {
            assert!(wheel(0.0, 64.0, direction) > 0.0, "{direction:?}: wheel down goes towards the end");
            assert!(wheel(0.0, -64.0, direction) < 0.0, "{direction:?}: wheel up goes back");
        }
        // Content going left (dx > 0) is towards the end of a left-to-right strip
        // and towards the start of a right-to-left one.
        assert!(wheel(40.0, 0.0, Direction::Ltr) > 0.0);
        assert!(wheel(40.0, 0.0, Direction::Rtl) < 0.0);
        assert_eq!(wheel(10.0, 5.0, Direction::Ltr), 15.0);
        assert_eq!(wheel(10.0, 5.0, Direction::Rtl), -5.0);
    }

    #[test]
    fn a_finger_drags_the_tabs_along_with_it_and_stays_in_range() {
        // Dragged to the right: more of the left shows. Left to right that is
        // earlier (a smaller offset), right to left it is later.
        assert_eq!(pan(50.0, 20.0, 100.0, Direction::Ltr), 30.0);
        assert_eq!(pan(50.0, 20.0, 100.0, Direction::Rtl), 70.0);
        assert_eq!(pan(10.0, 500.0, 100.0, Direction::Ltr), 0.0);
        assert_eq!(pan(90.0, -500.0, 100.0, Direction::Ltr), 100.0);
        assert_eq!(pan(50.0, 20.0, 0.0, Direction::Ltr), 0.0, "nothing to scroll");
    }

    #[test]
    fn the_selected_tab_is_scrolled_in_by_the_least_distance() {
        let geometry = Geometry::in_class(SizeClass::Regular);
        // Tabs of 96 with 4 px between, in a region 250 wide: a step is 100.
        let view = |index, current| into_view(index, geometry, 96.0, 250.0, current);
        assert_eq!(view(1, 0.0), 0.0, "a tab in view does not scroll");
        assert_eq!(view(3, 0.0), 3.0 * 100.0 + 96.0 - 250.0, "the tab's far edge comes to the edge");
        assert_eq!(view(0, 200.0), 0.0, "a tab before the region scrolls back to it");
        // A region too narrow for a tab shows the tab's start, not its end.
        assert_eq!(into_view(2, geometry, 96.0, 90.0, 0.0), 2.0 * 100.0);
        assert_eq!(into_view(0, geometry, 96.0, 100.0, 0.0), 0.0);
    }
}
