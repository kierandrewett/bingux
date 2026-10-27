//! The geometry behind pointer gestures in the file views: which rows a
//! rubber band covers, where the scroll goes so the content under the
//! pointer stays put while the size changes, and how far a press must move
//! before it is a drag.
//!
//! All pure, so the gestures can be tested without a window.

/// How far a press moves, in logical pixels, before it is a drag and not a
/// click.
pub(super) const DRAG_SLOP: f32 = 6.0;

/// A rectangle in the view's content space (the scrolled surface, not the
/// window): `x0 <= x1` and `y0 <= y1`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Span {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl Span {
    /// The rectangle between two corners, in either order.
    pub fn between(a: (f32, f32), b: (f32, f32)) -> Span {
        Span {
            x0: a.0.min(b.0),
            y0: a.1.min(b.1),
            x1: a.0.max(b.0),
            y1: a.1.max(b.1),
        }
    }
}

/// Where the items of a view sit: a single column for the list, several for
/// the grid. Items are laid out line by line, left to right.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Lattice {
    pub columns: usize,
    pub count: usize,
    /// Space left of the first column and above the first line.
    pub pad_x: f32,
    pub pad_y: f32,
    /// The width and height of one item.
    pub item_w: f32,
    pub item_h: f32,
    /// Space between items across and down.
    pub gap_x: f32,
    pub gap_y: f32,
}

impl Lattice {
    fn pitch_x(&self) -> f32 {
        self.item_w + self.gap_x
    }

    fn pitch_y(&self) -> f32 {
        self.item_h + self.gap_y
    }

    fn lines(&self) -> usize {
        self.count.div_ceil(self.columns.max(1))
    }

    /// The items a band covers: a block of lines and columns.
    pub fn hits(&self, band: Span) -> Option<Hits> {
        if self.count == 0 || self.columns == 0 {
            return None;
        }
        // The first line or column whose far edge is past the band's start,
        // and the last whose near edge is before the band's end.
        let first = |start: f32, pad: f32, size: f32, pitch: f32| -> Option<usize> {
            let at = start - pad;
            if at < 0.0 {
                return Some(0);
            }
            let index = (at / pitch).floor();
            let into = at - index * pitch;
            // Inside the gap after the item: the next one is the first.
            Some(if into >= size { index as usize + 1 } else { index as usize })
        };
        let last = |end: f32, pad: f32, pitch: f32| -> Option<usize> {
            let at = end - pad;
            (at >= 0.0).then(|| (at / pitch).floor() as usize)
        };
        let line0 = first(band.y0, self.pad_y, self.item_h, self.pitch_y())?;
        let line1 = last(band.y1, self.pad_y, self.pitch_y())?.min(self.lines().saturating_sub(1));
        let col0 = first(band.x0, self.pad_x, self.item_w, self.pitch_x())?;
        let col1 = last(band.x1, self.pad_x, self.pitch_x())?.min(self.columns - 1);
        if line0 > line1 || col0 > col1 {
            return None;
        }
        Some(Hits {
            lines: (line0, line1),
            columns: (col0, col1),
            lattice_columns: self.columns,
            count: self.count,
        })
    }
}

/// A block of items a band covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Hits {
    lines: (usize, usize),
    columns: (usize, usize),
    lattice_columns: usize,
    count: usize,
}

impl Hits {
    /// The item indices covered, in order.
    pub fn indices(self) -> impl Iterator<Item = usize> {
        (self.lines.0..=self.lines.1).flat_map(move |line| {
            (self.columns.0..=self.columns.1)
                .map(move |column| line * self.lattice_columns + column)
                .filter(move |index| *index < self.count)
        })
    }
}

/// The scroll offset (positive, from the top) that keeps the content under
/// `pointer_y` (measured from the viewport's top) where it is when the items
/// change from a pitch of `old_pitch` in `old_columns` columns to `new_pitch`
/// in `new_columns`. Items are the anchor, not pixels, so a grid that
/// reflows into more columns keeps the same files under the pointer.
pub(super) fn anchored_offset(
    old_offset: f32,
    pointer_y: f32,
    old_pitch: f32,
    old_columns: usize,
    new_pitch: f32,
    new_columns: usize,
) -> f32 {
    if old_pitch <= 0.0 || new_pitch <= 0.0 || old_columns == 0 || new_columns == 0 {
        return old_offset;
    }
    // Items above the pointer, as a fraction.
    let items = (old_offset + pointer_y) / old_pitch * old_columns as f32;
    items / new_columns as f32 * new_pitch - pointer_y
}

/// The offset that shows `line` fully, nudging the view as little as
/// possible. `offset` is the present positive offset.
pub(super) fn reveal_offset(line: usize, pitch: f32, offset: f32, content: f32, viewport: f32) -> f32 {
    let top = line as f32 * pitch;
    let furthest = (content - viewport).max(0.0);
    let want = if top < offset {
        top
    } else if top + pitch > offset + viewport {
        top + pitch - viewport
    } else {
        offset
    };
    want.clamp(0.0, furthest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(count: usize) -> Lattice {
        Lattice {
            columns: 1,
            count,
            pad_x: 0.0,
            pad_y: 0.0,
            item_w: 400.0,
            item_h: 40.0,
            gap_x: 0.0,
            gap_y: 0.0,
        }
    }

    fn grid() -> Lattice {
        Lattice {
            columns: 4,
            count: 10,
            pad_x: 12.0,
            pad_y: 12.0,
            item_w: 100.0,
            item_h: 90.0,
            gap_x: 12.0,
            gap_y: 12.0,
        }
    }

    fn picked(lattice: &Lattice, band: Span) -> Vec<usize> {
        lattice
            .hits(band)
            .map(|hits| hits.indices().collect())
            .unwrap_or_default()
    }

    #[test]
    fn a_band_over_list_rows_covers_the_rows_it_touches() {
        let band = Span::between((10.0, 50.0), (30.0, 130.0));
        assert_eq!(picked(&list(10), band), vec![1, 2, 3]);
    }

    #[test]
    fn a_band_past_the_last_row_stops_at_it() {
        let band = Span::between((0.0, 300.0), (10.0, 5000.0));
        assert_eq!(picked(&list(10), band), vec![7, 8, 9]);
    }

    #[test]
    fn a_band_dragged_upwards_is_the_same_as_downwards() {
        let down = Span::between((0.0, 50.0), (10.0, 130.0));
        let up = Span::between((10.0, 130.0), (0.0, 50.0));
        assert_eq!(down, up);
    }

    #[test]
    fn a_band_in_the_gap_between_tiles_covers_neither() {
        // Tile 0 spans x 12..112, tile 1 spans 124..224; the gap is 112..124.
        let band = Span::between((114.0, 20.0), (120.0, 40.0));
        assert!(picked(&grid(), band).is_empty());
    }

    #[test]
    fn a_band_over_a_block_of_tiles_covers_the_block() {
        // Columns 1 and 2 of lines 0 and 1.
        let band = Span::between((130.0, 20.0), (240.0, 150.0));
        assert_eq!(picked(&grid(), band), vec![1, 2, 5, 6]);
    }

    #[test]
    fn a_band_over_a_short_last_line_skips_the_missing_tiles() {
        let band = Span::between((0.0, 220.0), (500.0, 260.0));
        assert_eq!(picked(&grid(), band), vec![8, 9]);
    }

    #[test]
    fn an_empty_view_has_no_hits() {
        assert!(list(0).hits(Span::between((0.0, 0.0), (9.0, 9.0))).is_none());
    }

    #[test]
    fn anchoring_keeps_the_item_under_the_pointer() {
        // A list: the row at 130 px under the pointer stays at 130 px.
        let offset = anchored_offset(100.0, 30.0, 40.0, 1, 80.0, 1);
        assert!((offset - 230.0).abs() < 0.01);
    }

    #[test]
    fn anchoring_follows_the_items_when_the_grid_reflows() {
        // 4 columns at 100 px lines become 2 columns at 200 px lines: the
        // item two lines down (index 8) is now four lines down.
        let offset = anchored_offset(200.0, 0.0, 100.0, 4, 200.0, 2);
        assert!((offset - 800.0).abs() < 0.01);
    }

    #[test]
    fn revealing_a_line_moves_the_view_only_when_needed() {
        assert_eq!(reveal_offset(2, 40.0, 0.0, 4000.0, 200.0), 0.0);
        assert_eq!(reveal_offset(10, 40.0, 0.0, 4000.0, 200.0), 240.0);
        assert_eq!(reveal_offset(1, 40.0, 100.0, 4000.0, 200.0), 40.0);
        assert_eq!(reveal_offset(99, 40.0, 0.0, 4000.0, 200.0), 3800.0);
    }
}
