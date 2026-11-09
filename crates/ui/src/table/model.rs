//! The arithmetic of a table, apart from anything drawn: which columns are
//! shown and how wide, where a dragged heading would land, how the cursor
//! moves over the grid, and how rows are sorted.
//!
//! Everything here is a plain function of numbers and ids, so it is tested
//! without a window. [`ColumnLayout`] is what a table remembers between runs.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

/// The width every column starts at when it asks for no other, in logical
/// pixels.
pub const DEFAULT_WIDTH: f32 = 120.0;
/// The least a column is dragged to, unless its own minimum is more.
pub const MIN_WIDTH: f32 = 48.0;
/// The most a column is dragged to.
pub const MAX_WIDTH: f32 = 1200.0;

/// How a column takes its width.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Width {
    /// The room the fixed columns leave, shared equally by the flexible ones.
    Flex,
    /// A width of its own, in logical pixels.
    Px(f32),
}

/// Which way a column sorts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortDirection {
    #[default]
    Ascending,
    Descending,
}

impl SortDirection {
    pub fn flipped(self) -> Self {
        match self {
            SortDirection::Ascending => SortDirection::Descending,
            SortDirection::Descending => SortDirection::Ascending,
        }
    }

    pub fn is_descending(self) -> bool {
        self == SortDirection::Descending
    }
}

/// The column a table is sorted by and the way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SortOrder {
    pub column: &'static str,
    pub direction: SortDirection,
}

impl SortOrder {
    pub fn new(column: &'static str, direction: SortDirection) -> Self {
        Self { column, direction }
    }

    /// What pressing the heading of `column` does: a new column sorts
    /// ascending, the sorted one turns around.
    pub fn pressed(current: Option<SortOrder>, column: &'static str) -> SortOrder {
        match current {
            Some(order) if order.column == column => SortOrder {
                column,
                direction: order.direction.flipped(),
            },
            _ => SortOrder {
                column,
                direction: SortDirection::Ascending,
            },
        }
    }
}

/// What a column says about itself, apart from how it draws a row. The layout
/// and the plan work on these, so they need no knowledge of the row type.
#[derive(Clone, Debug, PartialEq)]
pub struct ColumnSpec {
    pub id: &'static str,
    pub title: String,
    pub width: Width,
    pub min_width: f32,
    /// 0 is a column that is never dropped for room; a higher number goes
    /// sooner when the table is too narrow for all its columns.
    pub priority: u8,
    pub right: bool,
    pub can_hide: bool,
    pub shown_by_default: bool,
    pub resizable: bool,
    pub sortable: bool,
}

/// One column's saved place: its width when the person changed it, and
/// whether it is shown when they chose.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Slot {
    id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    width: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    shown: Option<bool>,
}

/// The sort as it is saved.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedSort {
    pub column: String,
    #[serde(default)]
    pub direction: SortDirection,
}

/// The order, widths and visibility the person gave a table's columns, and its
/// sort. Columns it has never seen keep their definition's place, after the
/// ones it has; a saved column that is no longer defined is ignored, so an
/// update that adds or removes a column never breaks a saved layout.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ColumnLayout {
    /// Column ids in the order the person left them; empty until they move one.
    #[serde(default)]
    order: Vec<String>,
    #[serde(default)]
    slots: Vec<Slot>,
    #[serde(default)]
    pub sort: Option<SavedSort>,
}

impl ColumnLayout {
    fn slot(&self, id: &str) -> Option<&Slot> {
        self.slots.iter().find(|slot| slot.id == id)
    }

    fn slot_mut(&mut self, id: &str) -> &mut Slot {
        match self.slots.iter().position(|slot| slot.id == id) {
            Some(index) => &mut self.slots[index],
            None => {
                self.slots.push(Slot {
                    id: id.to_string(),
                    width: None,
                    shown: None,
                });
                self.slots.last_mut().expect("just pushed")
            }
        }
    }

    /// Indexes into `specs` in the order the columns are laid out, shown or
    /// not: saved columns in their saved order, then the rest as defined.
    pub fn order(&self, specs: &[ColumnSpec]) -> Vec<usize> {
        let mut order: Vec<usize> = Vec::with_capacity(specs.len());
        for id in &self.order {
            if let Some(index) = specs.iter().position(|spec| spec.id == *id)
                && !order.contains(&index)
            {
                order.push(index);
            }
        }
        for index in 0..specs.len() {
            if !order.contains(&index) {
                order.push(index);
            }
        }
        order
    }

    /// Whether `spec` is shown: a column that cannot be hidden always is.
    pub fn is_shown(&self, spec: &ColumnSpec) -> bool {
        !spec.can_hide
            || self
                .slot(spec.id)
                .and_then(|slot| slot.shown)
                .unwrap_or(spec.shown_by_default)
    }

    /// The width `spec` is laid out at: the person's, or the definition's.
    pub fn width_of(&self, spec: &ColumnSpec) -> Width {
        match self.slot(spec.id).and_then(|slot| slot.width) {
            Some(px) => Width::Px(clamp_width(px, spec.min_width)),
            None => match spec.width {
                Width::Px(px) => Width::Px(clamp_width(px, spec.min_width)),
                Width::Flex => Width::Flex,
            },
        }
    }

    pub fn set_shown(&mut self, id: &str, shown: bool) {
        self.slot_mut(id).shown = Some(shown);
    }

    /// Sets the width of `id`, kept between its minimum and [`MAX_WIDTH`].
    pub fn resize(&mut self, id: &str, px: f32, min: f32) {
        self.slot_mut(id).width = Some(clamp_width(px, min));
    }

    /// Forgets a column's own width, so it takes its definition's again.
    pub fn reset_width(&mut self, id: &str) {
        if let Some(slot) = self.slots.iter_mut().find(|slot| slot.id == id) {
            slot.width = None;
        }
    }

    /// Moves column `id` to just before column `before`, or to the end when
    /// `before` is `None`. `specs` give the order the columns start from.
    pub fn move_before(&mut self, specs: &[ColumnSpec], id: &str, before: Option<&str>) {
        let mut order: Vec<&str> = self.order(specs).iter().map(|&i| specs[i].id).collect();
        let Some(from) = order.iter().position(|known| *known == id) else {
            return;
        };
        order.remove(from);
        let to = before
            .and_then(|before| order.iter().position(|known| *known == before))
            .unwrap_or(order.len());
        order.insert(
            to,
            specs
                .iter()
                .find(|spec| spec.id == id)
                .map_or(id, |spec| spec.id),
        );
        self.order = order.into_iter().map(str::to_string).collect();
    }

    /// Back to the definitions' order, widths and visibility. The sort stays.
    pub fn reset(&mut self) {
        self.order.clear();
        self.slots.clear();
    }

    /// The saved sort, when it names a sortable column that still exists.
    pub fn saved_sort(&self, specs: &[ColumnSpec]) -> Option<SortOrder> {
        let saved = self.sort.as_ref()?;
        let spec = specs
            .iter()
            .find(|spec| spec.id == saved.column && spec.sortable)?;
        Some(SortOrder::new(spec.id, saved.direction))
    }
}

fn clamp_width(px: f32, min: f32) -> f32 {
    px.clamp(min.max(1.0), MAX_WIDTH)
}

/// One column as laid out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    /// Index into the table's columns.
    pub index: usize,
    /// Drawn width in logical pixels.
    pub width: f32,
    pub flex: bool,
}

/// The columns drawn at some width, and the ones left out for want of room.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    pub columns: Vec<Placed>,
    /// Shown columns that did not fit, least important first dropped.
    pub dropped: Vec<usize>,
}

impl Plan {
    pub fn total(&self) -> f32 {
        self.columns.iter().map(|placed| placed.width).sum()
    }

    pub fn widths(&self) -> Vec<f32> {
        self.columns.iter().map(|placed| placed.width).collect()
    }
}

/// Lays out `shown` columns (indexes into `specs`, in order) in `available`
/// pixels. Fixed columns keep their width; flexible ones share what is left,
/// none below its minimum. When even the minimums do not fit, the columns with
/// the highest priority number go first (the rightmost of equals), and never a
/// column of priority 0.
pub fn plan(specs: &[ColumnSpec], shown: &[usize], widths: &[Width], available: f32) -> Plan {
    debug_assert_eq!(shown.len(), widths.len());
    let mut kept: Vec<usize> = (0..shown.len()).collect();
    let needed = |at: usize| match widths[at] {
        Width::Flex => specs[shown[at]].min_width,
        Width::Px(px) => px,
    };
    let mut dropped = Vec::new();
    while kept.iter().map(|&at| needed(at)).sum::<f32>() > available + 0.5 {
        let victim = kept
            .iter()
            .copied()
            .filter(|&at| specs[shown[at]].priority > 0)
            .max_by_key(|&at| (specs[shown[at]].priority, at));
        let Some(victim) = victim else {
            break;
        };
        kept.retain(|&at| at != victim);
        dropped.push(shown[victim]);
    }
    let fixed: f32 = kept
        .iter()
        .filter_map(|&at| match widths[at] {
            Width::Px(px) => Some(px),
            Width::Flex => None,
        })
        .sum();
    let flexible = kept
        .iter()
        .filter(|&&at| matches!(widths[at], Width::Flex))
        .count();
    let share = if flexible == 0 {
        0.0
    } else {
        ((available - fixed) / flexible as f32).max(0.0)
    };
    let columns = kept
        .iter()
        .map(|&at| {
            let spec = &specs[shown[at]];
            match widths[at] {
                Width::Px(px) => Placed {
                    index: shown[at],
                    width: px,
                    flex: false,
                },
                Width::Flex => Placed {
                    index: shown[at],
                    width: share.max(spec.min_width),
                    flex: true,
                },
            }
        })
        .collect();
    Plan { columns, dropped }
}

// ----------------------------------------------------------------------------
// Dragging a heading
// ----------------------------------------------------------------------------

/// Where a heading dragged `dx` pixels lands among the other columns: its
/// place in the order with the dragged one taken out. It lands after every
/// other column whose middle it has passed.
pub fn drop_slot(widths: &[f32], dragged: usize, dx: f32) -> usize {
    let centres = centres(widths);
    let Some(own) = centres.get(dragged) else {
        return 0;
    };
    let centre = own + dx;
    centres
        .iter()
        .enumerate()
        .filter(|(index, other)| *index != dragged && **other < centre)
        .count()
}

fn centres(widths: &[f32]) -> Vec<f32> {
    let mut left = 0.0;
    widths
        .iter()
        .map(|width| {
            let centre = left + width / 2.0;
            left += width;
            centre
        })
        .collect()
}

/// Where the drop line goes for `slot`, from the left edge of the first
/// column: before the `slot`th of the other columns, or after the last.
pub fn line_position(widths: &[f32], dragged: usize, slot: usize) -> f32 {
    let mut left = 0.0;
    let mut edges = Vec::with_capacity(widths.len());
    for (index, width) in widths.iter().enumerate() {
        edges.push((index, left, left + width));
        left += width;
    }
    let others: Vec<_> = edges
        .iter()
        .filter(|(index, ..)| *index != dragged)
        .collect();
    match others.get(slot) {
        Some((_, start, _)) => *start,
        None => others.last().map_or(0.0, |(_, _, end)| *end),
    }
}

/// What a drag on a column's trailing edge changes, and the width it gives it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgeDrag {
    /// Index into the plan's columns of the column that changes.
    pub target: usize,
    pub width: f32,
}

/// The effect of dragging the trailing edge of plan column `edge` by `dx`
/// (positive to the end). A fixed column takes the change itself. A flexible
/// one cannot, as it takes what is left, so its fixed neighbour after it gives
/// and takes the opposite way; with none, the column becomes fixed at its drawn
/// width plus `dx`. `None` when the edge moves nothing.
pub fn drag_edge(plan: &Plan, edge: usize, dx: f32) -> Option<EdgeDrag> {
    let own = plan.columns.get(edge)?;
    if !own.flex {
        return Some(EdgeDrag {
            target: edge,
            width: own.width + dx,
        });
    }
    match plan.columns.get(edge + 1) {
        Some(next) if !next.flex => Some(EdgeDrag {
            target: edge + 1,
            width: next.width - dx,
        }),
        _ => Some(EdgeDrag {
            target: edge,
            width: own.width + dx,
        }),
    }
}

// ----------------------------------------------------------------------------
// The grid cursor
// ----------------------------------------------------------------------------

/// The cell that holds the keyboard: a column of a row, or of the header when
/// `row` is `None`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cursor {
    pub row: Option<usize>,
    pub col: usize,
}

/// A key that moves the cursor over the grid, as the WAI-ARIA grid pattern
/// names them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Left,
    Right,
    Up,
    Down,
    /// First cell of the row.
    Home,
    /// Last cell of the row.
    End,
    /// First cell of the first row.
    First,
    /// Last cell of the last row.
    Last,
    PageUp,
    PageDown,
}

/// Where `step` takes `cursor` in a grid of `rows` rows and `cols` columns,
/// with a header row above when `header` is set. A step off an edge stays at
/// the edge. `page` is how many rows a page key moves.
pub fn step(
    cursor: Cursor,
    step: Step,
    rows: usize,
    cols: usize,
    header: bool,
    page: usize,
) -> Cursor {
    if cols == 0 {
        return cursor;
    }
    let last_col = cols - 1;
    let col = cursor.col.min(last_col);
    let last_row = rows.checked_sub(1);
    let up = |row: Option<usize>, by: usize| match row {
        Some(0) => {
            if header {
                None
            } else {
                Some(0)
            }
        }
        Some(row) => Some(row.saturating_sub(by)),
        None => None,
    };
    let down = |row: Option<usize>, by: usize| {
        last_row.map(|last| row.map_or(by - 1, |row| row + by).min(last))
    };
    let (row, col) = match step {
        Step::Left => (cursor.row, col.saturating_sub(1)),
        Step::Right => (cursor.row, (col + 1).min(last_col)),
        Step::Up => (up(cursor.row, 1), col),
        Step::Down => (down(cursor.row, 1), col),
        Step::PageUp => (up(cursor.row, page.max(1)), col),
        Step::PageDown => (down(cursor.row, page.max(1)), col),
        Step::Home => (cursor.row, 0),
        Step::End => (cursor.row, last_col),
        Step::First => (if header { None } else { last_row.map(|_| 0) }, 0),
        Step::Last => (last_row.or(cursor.row), last_col),
    };
    Cursor { row, col }
}

/// The scroll offset (negative, as the scroll controller stores it) that
/// shows row `index` of rows `row` px tall, given the offset now and the
/// viewport's height. The offset is unchanged when the row is already in view.
pub fn offset_to_show(index: usize, current: i32, count: usize, viewport: f32, row: f32) -> i32 {
    let top = index as f32 * row;
    let furthest = (count as f32 * row - viewport).max(0.0);
    let shown_top = -(current as f32);
    let want = if top < shown_top {
        top
    } else if top + row > shown_top + viewport {
        top + row - viewport
    } else {
        shown_top
    };
    -(want.clamp(0.0, furthest) as i32)
}

// ----------------------------------------------------------------------------
// Sorting and finding
// ----------------------------------------------------------------------------

/// The order `len` rows are shown in when sorted by `compare` on their
/// indexes. The sort is stable, so rows that compare equal keep the order
/// they came in, descending too.
pub fn sorted_order(
    len: usize,
    direction: SortDirection,
    compare: impl Fn(usize, usize) -> Ordering,
) -> Vec<u32> {
    let mut order: Vec<u32> = (0..len as u32).collect();
    match direction {
        SortDirection::Ascending => order.sort_by(|a, b| compare(*a as usize, *b as usize)),
        SortDirection::Descending => {
            order.sort_by(|a, b| compare(*b as usize, *a as usize));
        }
    }
    order
}

/// Compares two texts the way a person reads them: case ignored, and a run of
/// digits by its value, so `item 2` comes before `item 10`.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut a = a.chars().peekable();
    let mut b = b.chars().peekable();
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let mut left = String::new();
                while let Some(digit) = a.next_if(char::is_ascii_digit) {
                    left.push(digit);
                }
                let mut right = String::new();
                while let Some(digit) = b.next_if(char::is_ascii_digit) {
                    right.push(digit);
                }
                let left = left.trim_start_matches('0');
                let right = right.trim_start_matches('0');
                let order = left.len().cmp(&right.len()).then_with(|| left.cmp(right));
                if order != Ordering::Equal {
                    return order;
                }
            }
            (Some(x), Some(y)) => {
                let order = x.to_lowercase().cmp(y.to_lowercase());
                if order != Ordering::Equal {
                    return order;
                }
                a.next();
                b.next();
            }
        }
    }
}

/// The first row at or after `from` (wrapping round) whose text starts with
/// `typed`, ignoring case, for type-ahead. `text` gives a row's text.
pub fn find_prefix(
    typed: &str,
    from: usize,
    count: usize,
    text: impl Fn(usize) -> String,
) -> Option<usize> {
    if typed.is_empty() || count == 0 {
        return None;
    }
    let typed = typed.to_lowercase();
    (0..count)
        .map(|offset| (from + offset) % count)
        .find(|&index| text(index).to_lowercase().starts_with(&typed))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(id: &'static str, width: Width, priority: u8) -> ColumnSpec {
        ColumnSpec {
            id,
            title: id.to_string(),
            width,
            min_width: 60.0,
            priority,
            right: false,
            can_hide: priority > 0,
            shown_by_default: true,
            resizable: true,
            sortable: true,
        }
    }

    fn specs() -> Vec<ColumnSpec> {
        vec![
            spec("name", Width::Flex, 0),
            spec("size", Width::Px(100.0), 2),
            spec("kind", Width::Px(120.0), 3),
            spec("date", Width::Px(140.0), 1),
        ]
    }

    #[test]
    fn flexible_columns_take_what_the_fixed_ones_leave() {
        let specs = specs();
        let shown = [0, 1, 2, 3];
        let widths = [
            Width::Flex,
            Width::Px(100.0),
            Width::Px(120.0),
            Width::Px(140.0),
        ];
        let plan = plan(&specs, &shown, &widths, 800.0);
        assert!(plan.dropped.is_empty());
        assert_eq!(plan.columns[0].width, 440.0);
        assert!((plan.total() - 800.0).abs() < 0.01);
    }

    #[test]
    fn a_narrow_table_drops_the_least_important_column_first() {
        let specs = specs();
        let shown = [0, 1, 2, 3];
        let widths = [
            Width::Flex,
            Width::Px(100.0),
            Width::Px(120.0),
            Width::Px(140.0),
        ];
        // Name needs 60: 60 + 100 + 120 + 140 = 420. At 350 the kind (3) goes.
        let plan = plan(&specs, &shown, &widths, 350.0);
        assert_eq!(plan.dropped, [2]);
        assert_eq!(
            plan.columns.iter().map(|p| p.index).collect::<Vec<_>>(),
            [0, 1, 3]
        );
        // At 150 the size (2) and the date (1) follow; name is never dropped.
        let plan = super::plan(&specs, &shown, &widths, 150.0);
        assert_eq!(
            plan.columns.iter().map(|p| p.index).collect::<Vec<_>>(),
            [0]
        );
    }

    #[test]
    fn a_layout_the_person_changed_is_read_back_and_repaired() {
        let specs = specs();
        let mut layout = ColumnLayout::default();
        layout.resize("size", 5.0, 60.0);
        layout.set_shown("kind", false);
        layout.move_before(&specs, "date", Some("size"));
        assert_eq!(layout.order(&specs), [0, 3, 1, 2]);
        assert_eq!(layout.width_of(&specs[1]), Width::Px(60.0));
        assert!(!layout.is_shown(&specs[2]));
        let json = serde_json::to_string(&layout).unwrap();
        let back: ColumnLayout = serde_json::from_str(&json).unwrap();
        assert_eq!(back, layout);
        // A column that went away is ignored; one that appeared follows.
        let json = r#"{"order":["gone","date"]}"#;
        let layout: ColumnLayout = serde_json::from_str(json).unwrap();
        assert_eq!(layout.order(&specs), [3, 0, 1, 2]);
    }

    #[test]
    fn a_column_that_cannot_hide_is_always_shown() {
        let specs = specs();
        let mut layout = ColumnLayout::default();
        layout.set_shown("name", false);
        assert!(layout.is_shown(&specs[0]));
    }

    #[test]
    fn moving_to_the_end_keeps_widths() {
        let specs = specs();
        let mut layout = ColumnLayout::default();
        layout.resize("name", 200.0, 60.0);
        layout.move_before(&specs, "name", None);
        assert_eq!(layout.order(&specs), [1, 2, 3, 0]);
        assert_eq!(layout.width_of(&specs[0]), Width::Px(200.0));
    }

    #[test]
    fn a_heading_lands_after_the_columns_it_has_passed() {
        let widths = [100.0, 100.0, 100.0];
        assert_eq!(drop_slot(&widths, 0, 40.0), 0);
        assert_eq!(drop_slot(&widths, 0, 120.0), 1);
        assert_eq!(drop_slot(&widths, 0, 260.0), 2);
        assert_eq!(drop_slot(&widths, 2, -150.0), 1);
        assert_eq!(line_position(&widths, 0, 0), 100.0);
        assert_eq!(line_position(&widths, 0, 2), 300.0);
    }

    #[test]
    fn an_edge_drag_gives_to_a_fixed_neighbour_when_the_column_flexes() {
        let specs = specs();
        let shown = [0, 1, 2];
        let widths = [Width::Flex, Width::Px(100.0), Width::Px(120.0)];
        let plan = plan(&specs, &shown, &widths, 600.0);
        // Name flexes: dragging its edge right narrows the size column.
        assert_eq!(
            drag_edge(&plan, 0, 30.0),
            Some(EdgeDrag {
                target: 1,
                width: 70.0
            })
        );
        // A fixed column takes the change itself.
        assert_eq!(
            drag_edge(&plan, 1, 30.0),
            Some(EdgeDrag {
                target: 1,
                width: 130.0
            })
        );
    }

    #[test]
    fn the_cursor_walks_the_grid_and_stops_at_its_edges() {
        let at = |row, col| Cursor { row, col };
        assert_eq!(
            step(at(Some(0), 0), Step::Left, 10, 3, true, 5),
            at(Some(0), 0)
        );
        assert_eq!(
            step(at(Some(0), 2), Step::Right, 10, 3, true, 5),
            at(Some(0), 2)
        );
        assert_eq!(step(at(Some(0), 1), Step::Up, 10, 3, true, 5), at(None, 1));
        assert_eq!(
            step(at(Some(0), 1), Step::Up, 10, 3, false, 5),
            at(Some(0), 1)
        );
        assert_eq!(
            step(at(None, 1), Step::Down, 10, 3, true, 5),
            at(Some(0), 1)
        );
        assert_eq!(
            step(at(Some(8), 0), Step::Down, 10, 3, true, 5),
            at(Some(9), 0)
        );
        assert_eq!(
            step(at(Some(9), 0), Step::Down, 10, 3, true, 5),
            at(Some(9), 0)
        );
        assert_eq!(
            step(at(Some(2), 1), Step::PageDown, 10, 3, true, 5),
            at(Some(7), 1)
        );
        assert_eq!(
            step(at(Some(7), 1), Step::PageDown, 10, 3, true, 5),
            at(Some(9), 1)
        );
        assert_eq!(
            step(at(Some(3), 1), Step::PageUp, 10, 3, true, 5),
            at(Some(0), 1)
        );
        assert_eq!(
            step(at(Some(3), 1), Step::Home, 10, 3, true, 5),
            at(Some(3), 0)
        );
        assert_eq!(
            step(at(Some(3), 1), Step::End, 10, 3, true, 5),
            at(Some(3), 2)
        );
        assert_eq!(
            step(at(Some(3), 1), Step::First, 10, 3, true, 5),
            at(None, 0)
        );
        assert_eq!(
            step(at(Some(3), 1), Step::First, 10, 3, false, 5),
            at(Some(0), 0)
        );
        assert_eq!(
            step(at(Some(3), 1), Step::Last, 10, 3, true, 5),
            at(Some(9), 2)
        );
    }

    #[test]
    fn the_cursor_in_an_empty_table_stays_on_the_header() {
        let at = |row, col| Cursor { row, col };
        assert_eq!(step(at(None, 0), Step::Down, 0, 3, true, 5), at(None, 0));
        assert_eq!(step(at(None, 0), Step::Last, 0, 3, true, 5), at(None, 2));
    }

    #[test]
    fn scrolling_to_a_row_moves_only_as_far_as_needed() {
        // 100 rows of 40 px in a 200 px viewport.
        assert_eq!(offset_to_show(2, 0, 100, 200.0, 40.0), 0);
        assert_eq!(offset_to_show(6, 0, 100, 200.0, 40.0), -80);
        assert_eq!(offset_to_show(1, -200, 100, 200.0, 40.0), -40);
        assert_eq!(offset_to_show(99, 0, 100, 200.0, 40.0), -3800);
    }

    #[test]
    fn sorting_is_stable_both_ways() {
        let keys = [2, 1, 2, 1, 3];
        let up = sorted_order(5, SortDirection::Ascending, |a, b| keys[a].cmp(&keys[b]));
        assert_eq!(up, [1, 3, 0, 2, 4]);
        let down = sorted_order(5, SortDirection::Descending, |a, b| keys[a].cmp(&keys[b]));
        assert_eq!(down, [4, 0, 2, 1, 3]);
    }

    #[test]
    fn numbers_inside_text_sort_by_value() {
        assert_eq!(natural_cmp("item 2", "item 10"), Ordering::Less);
        assert_eq!(natural_cmp("Apple", "apple"), Ordering::Equal);
        assert_eq!(natural_cmp("b", "A"), Ordering::Greater);
        assert_eq!(natural_cmp("file007", "file7"), Ordering::Equal);
        assert_eq!(natural_cmp("a", "a1"), Ordering::Less);
    }

    #[test]
    fn typing_finds_the_next_row_that_starts_with_it() {
        let names = ["Apple", "Banana", "Blueberry", "Cherry"];
        let text = |index: usize| names[index].to_string();
        assert_eq!(find_prefix("b", 0, 4, text), Some(1));
        assert_eq!(find_prefix("bl", 1, 4, text), Some(2));
        assert_eq!(find_prefix("a", 2, 4, text), Some(0));
        assert_eq!(find_prefix("z", 0, 4, text), None);
        assert_eq!(find_prefix("", 0, 4, text), None);
    }

    #[test]
    fn pressing_a_heading_sorts_then_turns_it_around() {
        let first = SortOrder::pressed(None, "name");
        assert_eq!(first.direction, SortDirection::Ascending);
        let second = SortOrder::pressed(Some(first), "name");
        assert_eq!(second.direction, SortDirection::Descending);
        let other = SortOrder::pressed(Some(second), "size");
        assert_eq!(
            (other.column, other.direction),
            ("size", SortDirection::Ascending)
        );
    }
}
