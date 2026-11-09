//! The arithmetic of a crop box: no Freya, so it is tested without a window.
//!
//! A [`CropRect`] is a box inside content of a known size, in that content's
//! own pixels. The functions here are what a drag of a handle, a key press or
//! a change of aspect ratio do to it.
//!
//! # What can go wrong, and where each is held
//!
//! Each line names the test in this file that holds it.
//!
//! - The box leaves the content, by a drag past an edge or a nudge at one
//!   (`dragging_past_an_edge_stops_at_it`, `a_move_keeps_its_size_inside_the_content`).
//! - A handle dragged across the opposite edge turns the box inside out
//!   (`a_handle_cannot_cross_the_opposite_edge`).
//! - The box shrinks to nothing, or is bigger than tiny content allows
//!   (`the_box_never_gets_smaller_than_the_least_side`, `tiny_content_has_no_room_for_the_least_side`).
//! - A fixed ratio drifts while a corner or an edge is dragged, or stops
//!   holding at the content's edge (`a_corner_drag_keeps_the_ratio`,
//!   `an_edge_drag_keeps_the_ratio_about_the_middle`, `a_ratio_box_at_an_edge_still_has_the_ratio`).
//! - Choosing a ratio jumps the box somewhere else or makes a box that does
//!   not fit (`a_new_ratio_keeps_the_area_and_the_middle`, `a_ratio_that_does_not_fit_is_shrunk`).
//! - A turned ratio is not the same ratio turned (`swapping_turns_the_ratio`).
//! - The whole box rounds to pixels that fall outside the content, or to
//!   a width of zero (`pixels_stay_inside_the_content_and_are_never_empty`).
//! - A NaN or an infinity from a pointer or an empty layout poisons the
//!   box for good (`nonsense_input_changes_nothing`).

/// The least a box side may be, in content pixels, unless the content is
/// smaller.
pub const MIN_SIDE: f32 = 16.0;

/// A box inside content, in the content's pixels: where its top left corner
/// is, and how wide and tall it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CropRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl CropRect {
    /// The whole of content of `size`.
    pub fn full(size: (f32, f32)) -> CropRect {
        CropRect {
            x: 0.0,
            y: 0.0,
            width: size.0.max(0.0),
            height: size.1.max(0.0),
        }
    }

    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }

    pub fn middle(&self) -> (f32, f32) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    /// Whether nothing is cut off content of `size`.
    pub fn is_full(&self, size: (f32, f32)) -> bool {
        let tolerance = 0.5;
        self.x.abs() < tolerance
            && self.y.abs() < tolerance
            && (self.width - size.0).abs() < tolerance
            && (self.height - size.1).abs() < tolerance
    }

    /// The box as fractions of the content, left, top, width, height: what an
    /// owner whose own decode may differ a pixel or two in size keeps.
    pub fn fractions(&self, size: (f32, f32)) -> (f32, f32, f32, f32) {
        if size.0 <= 0.0 || size.1 <= 0.0 {
            return (0.0, 0.0, 1.0, 1.0);
        }
        (
            self.x / size.0,
            self.y / size.1,
            self.width / size.0,
            self.height / size.1,
        )
    }

    /// The box in whole pixels that are inside content of `size` and are at
    /// least one wide and one tall: left, top, width, height.
    pub fn pixels(&self, size: (f32, f32)) -> (u32, u32, u32, u32) {
        let (full_width, full_height) = (size.0.max(1.0).floor(), size.1.max(1.0).floor());
        let left = self.x.round().clamp(0.0, full_width - 1.0);
        let top = self.y.round().clamp(0.0, full_height - 1.0);
        let right = self.right().round().clamp(left + 1.0, full_width);
        let bottom = self.bottom().round().clamp(top + 1.0, full_height);
        (
            left as u32,
            top as u32,
            (right - left) as u32,
            (bottom - top) as u32,
        )
    }

    fn is_sane(&self) -> bool {
        [self.x, self.y, self.width, self.height]
            .iter()
            .all(|value| value.is_finite())
    }
}

/// The shape a box is held to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Aspect {
    /// Any shape.
    Free,
    /// The shape of the whole content.
    Original,
    /// A shape of this width to this height: `Ratio(16, 9)`.
    Ratio(u32, u32),
}

impl Aspect {
    /// Square.
    pub const SQUARE: Aspect = Aspect::Ratio(1, 1);

    /// The shapes a person usually asks for.
    pub const PRESETS: [Aspect; 8] = [
        Aspect::Free,
        Aspect::Original,
        Aspect::SQUARE,
        Aspect::Ratio(4, 3),
        Aspect::Ratio(3, 2),
        Aspect::Ratio(16, 9),
        Aspect::Ratio(5, 4),
        Aspect::Ratio(21, 9),
    ];

    /// Width over height for content of `size`, or `None` when any shape goes.
    pub fn ratio(self, size: (f32, f32)) -> Option<f32> {
        match self {
            Aspect::Free => None,
            Aspect::Original if size.0 > 0.0 && size.1 > 0.0 => Some(size.0 / size.1),
            Aspect::Original => None,
            Aspect::Ratio(width, height) if width > 0 && height > 0 => {
                Some(width as f32 / height as f32)
            }
            Aspect::Ratio(..) => None,
        }
    }

    /// The same shape on its side: 4:3 becomes 3:4. Free and square stay as
    /// they are; the original is the original, which the content itself says.
    pub fn swapped(self) -> Aspect {
        match self {
            Aspect::Ratio(width, height) => Aspect::Ratio(height, width),
            other => other,
        }
    }

    /// Whether swapping changes anything.
    pub fn can_swap(self) -> bool {
        matches!(self, Aspect::Ratio(width, height) if width != height)
    }

    /// A key that names the shape: "free", "original", "16:9". [`from_id`](Self::from_id)
    /// reads it back, so a dropdown can carry the choice.
    pub fn id(self) -> String {
        match self {
            Aspect::Free => "free".to_owned(),
            Aspect::Original => "original".to_owned(),
            Aspect::Ratio(width, height) => format!("{width}:{height}"),
        }
    }

    /// The shape [`id`](Self::id) named.
    pub fn from_id(id: &str) -> Option<Aspect> {
        match id {
            "free" => Some(Aspect::Free),
            "original" => Some(Aspect::Original),
            _ => {
                let (width, height) = id.split_once(':')?;
                Some(Aspect::Ratio(width.parse().ok()?, height.parse().ok()?))
            }
        }
    }

    /// "Free", "Original", "16:9".
    pub fn label(self) -> String {
        match self {
            Aspect::Free => "Free".to_owned(),
            Aspect::Original => "Original".to_owned(),
            Aspect::Ratio(width, height) => format!("{width}:{height}"),
        }
    }
}

/// Which part of the box a drag holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Grip {
    /// The inside: the whole box moves.
    Move,
    North,
    South,
    East,
    West,
    NorthEast,
    NorthWest,
    SouthEast,
    SouthWest,
}

impl Grip {
    /// The eight handles, corners first.
    pub const HANDLES: [Grip; 8] = [
        Grip::NorthWest,
        Grip::NorthEast,
        Grip::SouthEast,
        Grip::SouthWest,
        Grip::North,
        Grip::East,
        Grip::South,
        Grip::West,
    ];

    fn moves_left(self) -> bool {
        matches!(self, Grip::West | Grip::NorthWest | Grip::SouthWest)
    }

    fn moves_right(self) -> bool {
        matches!(self, Grip::East | Grip::NorthEast | Grip::SouthEast)
    }

    fn moves_top(self) -> bool {
        matches!(self, Grip::North | Grip::NorthWest | Grip::NorthEast)
    }

    fn moves_bottom(self) -> bool {
        matches!(self, Grip::South | Grip::SouthWest | Grip::SouthEast)
    }

    /// Where the handle sits on `rect`, as fractions across it: (0, 0) is the
    /// top left corner.
    pub fn anchor(self) -> (f32, f32) {
        let x = if self.moves_left() {
            0.0
        } else if self.moves_right() {
            1.0
        } else {
            0.5
        };
        let y = if self.moves_top() {
            0.0
        } else if self.moves_bottom() {
            1.0
        } else {
            0.5
        };
        (x, y)
    }

    /// A word for a screen reader: "top left corner", "right edge".
    pub fn name(self) -> &'static str {
        match self {
            Grip::Move => "Crop area",
            Grip::North => "Top edge",
            Grip::South => "Bottom edge",
            Grip::East => "Right edge",
            Grip::West => "Left edge",
            Grip::NorthEast => "Top right corner",
            Grip::NorthWest => "Top left corner",
            Grip::SouthEast => "Bottom right corner",
            Grip::SouthWest => "Bottom left corner",
        }
    }
}

/// The least a side may be in content of `bounds`.
fn least(bounds: (f32, f32)) -> (f32, f32) {
    (MIN_SIDE.min(bounds.0), MIN_SIDE.min(bounds.1))
}

/// `rect` moved to lie inside `bounds`, its size kept (shrunk only when it is
/// larger than the content).
fn kept_inside(mut rect: CropRect, bounds: (f32, f32)) -> CropRect {
    rect.width = rect.width.min(bounds.0);
    rect.height = rect.height.min(bounds.1);
    rect.x = rect.x.clamp(0.0, (bounds.0 - rect.width).max(0.0));
    rect.y = rect.y.clamp(0.0, (bounds.1 - rect.height).max(0.0));
    rect
}

/// The box after the pointer that took `grip` of `start` has moved by `delta`
/// content pixels. `ratio` is the shape to keep, width over height.
pub fn drag(
    start: CropRect,
    grip: Grip,
    delta: (f32, f32),
    bounds: (f32, f32),
    ratio: Option<f32>,
) -> CropRect {
    if !start.is_sane() || !delta.0.is_finite() || !delta.1.is_finite() {
        return start;
    }
    let start = kept_inside(start, bounds);
    if grip == Grip::Move {
        return kept_inside(
            CropRect {
                x: start.x + delta.0,
                y: start.y + delta.1,
                ..start
            },
            bounds,
        );
    }
    let ratio = ratio.filter(|ratio| ratio.is_finite() && *ratio > 0.0);
    let (least_width, least_height) = least(bounds);
    let corner = (grip.moves_left() || grip.moves_right())
        && (grip.moves_top() || grip.moves_bottom());

    // The edges the grip moves, in content pixels, before any limit.
    let mut left = start.x;
    let mut top = start.y;
    let mut right = start.right();
    let mut bottom = start.bottom();
    if grip.moves_left() {
        left = (start.x + delta.0).clamp(0.0, right - least_width);
    }
    if grip.moves_right() {
        right = (start.right() + delta.0).clamp(left + least_width, bounds.0);
    }
    if grip.moves_top() {
        top = (start.y + delta.1).clamp(0.0, bottom - least_height);
    }
    if grip.moves_bottom() {
        bottom = (start.bottom() + delta.1).clamp(top + least_height, bounds.1);
    }
    let free = CropRect {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    };
    let Some(ratio) = ratio else { return free };

    if corner {
        // The corner opposite the grip stays; the box grows from it in the
        // direction the grip points, as large as the pointer asked for and
        // as the content allows.
        let anchor_x = if grip.moves_left() { start.right() } else { start.x };
        let anchor_y = if grip.moves_top() { start.bottom() } else { start.y };
        let room_x = if grip.moves_left() { anchor_x } else { bounds.0 - anchor_x };
        let room_y = if grip.moves_top() { anchor_y } else { bounds.1 - anchor_y };
        let asked_width = free.width.max(free.height * ratio);
        // The most width the room in both directions allows.
        let most_width = room_x.min(room_y * ratio);
        let least_width_for_ratio = least_width.max(least_height * ratio);
        let width = asked_width.min(most_width).max(least_width_for_ratio.min(most_width));
        let height = width / ratio;
        return CropRect {
            x: if grip.moves_left() { anchor_x - width } else { anchor_x },
            y: if grip.moves_top() { anchor_y - height } else { anchor_y },
            width,
            height,
        };
    }

    // An edge: that side sets the size along its axis; the other side follows
    // from the ratio, about the box's middle, kept inside the content.
    let (middle_x, middle_y) = start.middle();
    if grip.moves_left() || grip.moves_right() {
        let room = if grip.moves_left() { start.right() } else { bounds.0 - start.x };
        let room = room.min(bounds.1 * ratio);
        let width = free.width.min(room).max(least_width.max(least_height * ratio).min(room));
        let height = width / ratio;
        let x = if grip.moves_left() { start.right() - width } else { start.x };
        let y = (middle_y - height / 2.0).clamp(0.0, (bounds.1 - height).max(0.0));
        CropRect { x, y, width, height }
    } else {
        let room = if grip.moves_top() { start.bottom() } else { bounds.1 - start.y };
        let room = room.min(bounds.0 / ratio);
        let height = free.height.min(room).max(least_height.max(least_width / ratio).min(room));
        let width = height * ratio;
        let y = if grip.moves_top() { start.bottom() - height } else { start.y };
        let x = (middle_x - width / 2.0).clamp(0.0, (bounds.0 - width).max(0.0));
        CropRect { x, y, width, height }
    }
}

/// The box moved by `delta`, inside `bounds`: a nudge.
pub fn nudge(rect: CropRect, delta: (f32, f32), bounds: (f32, f32)) -> CropRect {
    drag(rect, Grip::Move, delta, bounds, None)
}

/// The box made `by` larger (smaller when negative) on each axis from its
/// bottom right corner: Alt and an arrow. A box held to a ratio grows along
/// the ratio.
pub fn resize(rect: CropRect, by: (f32, f32), bounds: (f32, f32), ratio: Option<f32>) -> CropRect {
    drag(rect, Grip::SouthEast, by, bounds, ratio)
}

/// The box after a new ratio is chosen: as large as it was in area, in the new
/// shape, around the same middle, and no larger than the content.
pub fn fit_ratio(rect: CropRect, ratio: f32, bounds: (f32, f32)) -> CropRect {
    if !rect.is_sane() || !ratio.is_finite() || ratio <= 0.0 || bounds.0 <= 0.0 || bounds.1 <= 0.0 {
        return rect;
    }
    let area = (rect.width * rect.height).max(1.0);
    let mut width = (area * ratio).sqrt();
    let mut height = width / ratio;
    // Too big for the content on either side: scale down to fit, same shape.
    let shrink = (bounds.0 / width).min(bounds.1 / height).min(1.0);
    width *= shrink;
    height *= shrink;
    let (middle_x, middle_y) = rect.middle();
    kept_inside(
        CropRect {
            x: middle_x - width / 2.0,
            y: middle_y - height / 2.0,
            width,
            height,
        },
        bounds,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOUNDS: (f32, f32) = (400.0, 300.0);

    fn rect(x: f32, y: f32, width: f32, height: f32) -> CropRect {
        CropRect { x, y, width, height }
    }

    fn inside(rect: CropRect, bounds: (f32, f32)) -> bool {
        rect.x >= -1e-3
            && rect.y >= -1e-3
            && rect.right() <= bounds.0 + 1e-3
            && rect.bottom() <= bounds.1 + 1e-3
    }

    fn ratio_of(rect: CropRect) -> f32 {
        rect.width / rect.height
    }

    #[test]
    fn dragging_past_an_edge_stops_at_it() {
        let start = rect(100.0, 100.0, 100.0, 100.0);
        let dragged = drag(start, Grip::East, (900.0, 0.0), BOUNDS, None);
        assert_eq!(dragged.right(), BOUNDS.0);
        let dragged = drag(start, Grip::NorthWest, (-900.0, -900.0), BOUNDS, None);
        assert_eq!((dragged.x, dragged.y), (0.0, 0.0));
        assert_eq!((dragged.right(), dragged.bottom()), (200.0, 200.0));
    }

    #[test]
    fn a_move_keeps_its_size_inside_the_content() {
        let start = rect(100.0, 100.0, 100.0, 80.0);
        let moved = nudge(start, (900.0, 900.0), BOUNDS);
        assert_eq!((moved.width, moved.height), (100.0, 80.0));
        assert!(inside(moved, BOUNDS));
        assert_eq!((moved.right(), moved.bottom()), BOUNDS);
        let moved = nudge(start, (-900.0, -900.0), BOUNDS);
        assert_eq!((moved.x, moved.y), (0.0, 0.0));
    }

    #[test]
    fn a_handle_cannot_cross_the_opposite_edge() {
        let start = rect(100.0, 100.0, 100.0, 100.0);
        let dragged = drag(start, Grip::West, (500.0, 0.0), BOUNDS, None);
        assert!(dragged.width >= MIN_SIDE);
        assert_eq!(dragged.right(), 200.0, "the right edge stays where it was");
        let dragged = drag(start, Grip::South, (0.0, -500.0), BOUNDS, None);
        assert!(dragged.height >= MIN_SIDE);
        assert_eq!(dragged.y, 100.0);
    }

    #[test]
    fn the_box_never_gets_smaller_than_the_least_side() {
        let start = rect(10.0, 10.0, 50.0, 50.0);
        for grip in Grip::HANDLES {
            for ratio in [None, Some(1.0), Some(2.0), Some(0.5)] {
                let dragged = drag(start, grip, (-1e4, -1e4), BOUNDS, ratio);
                let shrunk = drag(start, grip, (1e4, 1e4), BOUNDS, ratio);
                for rect in [dragged, shrunk] {
                    assert!(rect.width >= MIN_SIDE - 1e-3 && rect.height >= MIN_SIDE - 1e-3, "{grip:?} {ratio:?} {rect:?}");
                    assert!(inside(rect, BOUNDS), "{grip:?} {ratio:?} {rect:?}");
                }
            }
        }
    }

    #[test]
    fn tiny_content_has_no_room_for_the_least_side() {
        let tiny = (8.0, 6.0);
        let rect = drag(CropRect::full(tiny), Grip::SouthEast, (-100.0, -100.0), tiny, None);
        assert!(rect.width > 0.0 && rect.height > 0.0);
        assert!(inside(rect, tiny));
        let rect = drag(CropRect::full(tiny), Grip::East, (-100.0, 0.0), tiny, Some(1.0));
        assert!(inside(rect, tiny) && rect.width > 0.0);
    }

    #[test]
    fn a_corner_drag_keeps_the_ratio() {
        let start = rect(100.0, 100.0, 160.0, 90.0);
        let ratio = 16.0 / 9.0;
        for grip in [Grip::NorthWest, Grip::NorthEast, Grip::SouthEast, Grip::SouthWest] {
            for delta in [(30.0, 5.0), (-20.0, -40.0), (5.0, 60.0), (-70.0, 3.0)] {
                let dragged = drag(start, grip, delta, BOUNDS, Some(ratio));
                assert!((ratio_of(dragged) - ratio).abs() < 1e-3, "{grip:?} {delta:?} {dragged:?}");
                assert!(inside(dragged, BOUNDS), "{grip:?} {delta:?} {dragged:?}");
            }
        }
        // The corner opposite the one dragged stays put.
        let dragged = drag(start, Grip::SouthEast, (30.0, 30.0), BOUNDS, Some(ratio));
        assert_eq!((dragged.x, dragged.y), (100.0, 100.0));
        let dragged = drag(start, Grip::NorthWest, (-20.0, -20.0), BOUNDS, Some(ratio));
        assert!((dragged.right() - 260.0).abs() < 1e-3 && (dragged.bottom() - 190.0).abs() < 1e-3);
    }

    #[test]
    fn an_edge_drag_keeps_the_ratio_about_the_middle() {
        let start = rect(100.0, 100.0, 80.0, 80.0);
        let dragged = drag(start, Grip::East, (40.0, 0.0), BOUNDS, Some(1.0));
        assert_eq!((dragged.width, dragged.height), (120.0, 120.0));
        assert_eq!(dragged.x, 100.0);
        assert!((dragged.middle().1 - 140.0).abs() < 1e-3, "grown about the middle");
        let dragged = drag(start, Grip::North, (0.0, -30.0), BOUNDS, Some(1.0));
        assert_eq!((dragged.width, dragged.height), (110.0, 110.0));
        assert_eq!(dragged.bottom(), 180.0);
    }

    #[test]
    fn a_ratio_box_at_an_edge_still_has_the_ratio() {
        let start = rect(300.0, 200.0, 100.0, 100.0);
        // At the bottom right corner of the content: nothing to grow into.
        let dragged = drag(start, Grip::SouthEast, (50.0, 50.0), BOUNDS, Some(1.0));
        assert_eq!(dragged, start);
        let dragged = drag(start, Grip::East, (50.0, 0.0), BOUNDS, Some(1.0));
        assert!((ratio_of(dragged) - 1.0).abs() < 1e-3 && inside(dragged, BOUNDS));
        // A wide ratio on a short content: the height limits the width.
        let dragged = drag(rect(0.0, 0.0, 100.0, 50.0), Grip::East, (900.0, 0.0), BOUNDS, Some(2.0));
        assert!((ratio_of(dragged) - 2.0).abs() < 1e-3 && inside(dragged, BOUNDS));
        assert!(dragged.height <= BOUNDS.1);
    }

    #[test]
    fn a_new_ratio_keeps_the_area_and_the_middle() {
        let start = rect(100.0, 100.0, 100.0, 100.0);
        let fitted = fit_ratio(start, 2.0, BOUNDS);
        assert!((ratio_of(fitted) - 2.0).abs() < 1e-3);
        assert!((fitted.width * fitted.height - 10_000.0).abs() < 1.0);
        let (a, b) = (fitted.middle(), start.middle());
        assert!((a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3);
    }

    #[test]
    fn a_ratio_that_does_not_fit_is_shrunk() {
        let whole = CropRect::full(BOUNDS);
        for ratio in [0.2, 1.0, 21.0 / 9.0, 5.0] {
            let fitted = fit_ratio(whole, ratio, BOUNDS);
            assert!((ratio_of(fitted) - ratio).abs() < 1e-3, "{ratio}");
            assert!(inside(fitted, BOUNDS), "{ratio}: {fitted:?}");
        }
        // Near an edge it is pushed in, not cut.
        let corner = rect(350.0, 250.0, 50.0, 50.0);
        let fitted = fit_ratio(corner, 3.0, BOUNDS);
        assert!(inside(fitted, BOUNDS));
    }

    #[test]
    fn swapping_turns_the_ratio() {
        assert_eq!(Aspect::Ratio(4, 3).swapped(), Aspect::Ratio(3, 4));
        assert_eq!(Aspect::SQUARE.swapped(), Aspect::SQUARE);
        assert_eq!(Aspect::Free.swapped(), Aspect::Free);
        assert!(Aspect::Ratio(16, 9).can_swap() && !Aspect::SQUARE.can_swap() && !Aspect::Free.can_swap());
        assert_eq!(Aspect::Original.ratio((300.0, 200.0)), Some(1.5));
        assert_eq!(Aspect::Original.ratio((0.0, 0.0)), None);
        assert_eq!(Aspect::Ratio(0, 3).ratio(BOUNDS), None);
        assert_eq!(Aspect::Ratio(16, 9).label(), "16:9");
        for aspect in Aspect::PRESETS.into_iter().chain([Aspect::Ratio(9, 16)]) {
            assert_eq!(Aspect::from_id(&aspect.id()), Some(aspect));
        }
        assert_eq!(Aspect::from_id("wide"), None);
        assert_eq!(Aspect::from_id("4:x"), None);
    }

    #[test]
    fn pixels_stay_inside_the_content_and_are_never_empty() {
        let size = (400.0, 300.0);
        assert_eq!(CropRect::full(size).pixels(size), (0, 0, 400, 300));
        let (x, y, w, h) = rect(399.9, 299.9, 0.1, 0.1).pixels(size);
        assert!(x + w <= 400 && y + h <= 300 && w >= 1 && h >= 1);
        let (_, _, w, h) = rect(10.2, 10.2, 0.0, 0.0).pixels(size);
        assert!(w >= 1 && h >= 1);
        let (x, y, w, h) = rect(-5.0, -5.0, 1000.0, 1000.0).pixels(size);
        assert_eq!((x, y, w, h), (0, 0, 400, 300));
        assert_eq!(CropRect::full((0.0, 0.0)).pixels((0.0, 0.0)), (0, 0, 1, 1));
    }

    #[test]
    fn fractions_and_fullness() {
        let size = (400.0, 300.0);
        assert!(CropRect::full(size).is_full(size));
        assert!(!rect(0.0, 0.0, 399.0, 300.0).is_full(size));
        assert_eq!(rect(100.0, 150.0, 200.0, 75.0).fractions(size), (0.25, 0.5, 0.5, 0.25));
        assert_eq!(CropRect::full(size).fractions((0.0, 0.0)), (0.0, 0.0, 1.0, 1.0));
    }

    #[test]
    fn nonsense_input_changes_nothing() {
        let start = rect(10.0, 10.0, 100.0, 100.0);
        assert_eq!(drag(start, Grip::East, (f32::NAN, 0.0), BOUNDS, None), start);
        assert_eq!(drag(start, Grip::Move, (0.0, f32::INFINITY), BOUNDS, None), start);
        assert_eq!(fit_ratio(start, f32::NAN, BOUNDS), start);
        assert_eq!(fit_ratio(start, 1.0, (0.0, 0.0)), start);
        let broken = rect(f32::NAN, 0.0, 10.0, 10.0);
        assert!(drag(broken, Grip::Move, (1.0, 1.0), BOUNDS, None).x.is_nan());
        // A ratio of zero or NaN is no ratio.
        let dragged = drag(start, Grip::East, (10.0, 0.0), BOUNDS, Some(f32::NAN));
        assert_eq!(dragged.width, 110.0);
        let dragged = drag(start, Grip::East, (10.0, 0.0), BOUNDS, Some(0.0));
        assert_eq!(dragged.width, 110.0);
    }

    #[test]
    fn resize_grows_along_the_ratio() {
        let start = rect(0.0, 0.0, 160.0, 90.0);
        let grown = resize(start, (16.0, 16.0), BOUNDS, Some(16.0 / 9.0));
        assert!((ratio_of(grown) - 16.0 / 9.0).abs() < 1e-3);
        assert!(grown.width > 160.0);
        let shrunk = resize(start, (-16.0, -16.0), BOUNDS, None);
        assert_eq!((shrunk.width, shrunk.height), (144.0, 74.0));
    }
}
