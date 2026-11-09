//! Where events go: columns for events that overlap in time, lanes for bars
//! that span days, and the maths of a drag. Plain data, so each rule is
//! tested without a window.

use super::date::Date;

/// The least an event takes of a day column for overlap purposes, in minutes.
/// A shorter event is still drawn this tall, so it must not be drawn under
/// its neighbour.
pub const MIN_SPAN: u32 = 30;

/// A minute range of one day.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

/// Where one event sits among those it overlaps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    /// The column it starts in, from 0.
    pub column: usize,
    /// How many columns it covers: more than one when the columns to its right
    /// are free for as long as it lasts.
    pub span: usize,
    /// How many columns its group has.
    pub columns: usize,
}

/// Lays out events that may overlap, side by side. The result is in the order
/// of `spans`. Events that overlap in time share a group; in a group each
/// event takes the first column that is free when it starts, and then widens
/// to the right while the columns there stay free until it ends.
pub fn pack(spans: &[Span]) -> Vec<Slot> {
    let mut order: Vec<usize> = (0..spans.len()).collect();
    order.sort_by(|&a, &b| {
        spans[a]
            .start
            .cmp(&spans[b].start)
            .then(spans[b].end.cmp(&spans[a].end))
            .then(a.cmp(&b))
    });
    let reach = |span: &Span| span.end.max(span.start + MIN_SPAN);
    let mut slots = vec![
        Slot {
            column: 0,
            span: 1,
            columns: 1,
        };
        spans.len()
    ];
    let mut at = 0;
    while at < order.len() {
        // A group runs while each event starts before the group's latest end.
        let mut group = vec![order[at]];
        let mut latest = reach(&spans[order[at]]);
        let mut next = at + 1;
        while next < order.len() && spans[order[next]].start < latest {
            latest = latest.max(reach(&spans[order[next]]));
            group.push(order[next]);
            next += 1;
        }
        // Columns: each holds events one after another.
        let mut ends: Vec<u32> = Vec::new();
        let mut columns_of: Vec<usize> = Vec::new();
        for &index in &group {
            let span = &spans[index];
            let column = match ends.iter().position(|&end| end <= span.start) {
                Some(column) => column,
                None => {
                    ends.push(0);
                    ends.len() - 1
                }
            };
            ends[column] = reach(span);
            columns_of.push(column);
        }
        let columns = ends.len();
        for (member, &index) in group.iter().enumerate() {
            let column = columns_of[member];
            let mut span_columns = 1;
            'widen: while column + span_columns < columns {
                let probe = column + span_columns;
                for (other, &other_index) in group.iter().enumerate() {
                    if columns_of[other] == probe
                        && spans[other_index].start < reach(&spans[index])
                        && reach(&spans[other_index]) > spans[index].start
                    {
                        break 'widen;
                    }
                }
                span_columns += 1;
            }
            slots[index] = Slot {
                column,
                span: span_columns,
                columns,
            };
        }
        at = next;
    }
    slots
}

/// A bar over a run of days of one row (a week).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Run {
    /// Which bar, by position in the caller's list.
    pub index: usize,
    /// The first and last column the bar covers in this row, 0-based.
    pub first: usize,
    pub last: usize,
    /// The bar starts before this row's first day / ends after its last.
    pub continues_before: bool,
    pub continues_after: bool,
}

/// A bar in its lane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placed {
    pub run: Run,
    pub lane: usize,
}

/// The part of each bar (`first..=last`, in days) that falls in the week
/// starting at `week` (seven columns), as runs.
pub fn runs_in_week(week: Date, bars: &[(Date, Date)]) -> Vec<Run> {
    let week_end = week.add_days(6);
    let mut runs = Vec::new();
    for (index, (first, last)) in bars.iter().enumerate() {
        if *last < week || *first > week_end {
            continue;
        }
        let from = (*first).max(week);
        let to = (*last).min(week_end);
        runs.push(Run {
            index,
            first: (from.days() - week.days()) as usize,
            last: (to.days() - week.days()) as usize,
            continues_before: *first < week,
            continues_after: *last > week_end,
        });
    }
    runs
}

/// Gives each run a lane so that runs that share a column never share a
/// lane. Longer runs and earlier ones go first, so a bar that spans the week
/// keeps one lane across it. `fixed` lanes are those already taken in each
/// column by something the caller placed first.
pub fn lanes(runs: &[Run], columns: usize) -> Vec<Placed> {
    let mut order: Vec<usize> = (0..runs.len()).collect();
    order.sort_by(|&a, &b| {
        let (ra, rb) = (&runs[a], &runs[b]);
        ra.first
            .cmp(&rb.first)
            .then((rb.last - rb.first).cmp(&(ra.last - ra.first)))
            .then(ra.index.cmp(&rb.index))
    });
    let mut taken: Vec<Vec<bool>> = Vec::new();
    let mut placed = Vec::with_capacity(runs.len());
    for index in order {
        let run = runs[index];
        let last = run.last.min(columns.saturating_sub(1));
        let mut lane = 0;
        loop {
            if taken.len() <= lane {
                taken.push(vec![false; columns]);
            }
            if (run.first..=last).all(|column| !taken[lane][column]) {
                for column in run.first..=last {
                    taken[lane][column] = true;
                }
                break;
            }
            lane += 1;
        }
        placed.push(Placed { run, lane });
    }
    placed.sort_by_key(|placed| (placed.lane, placed.run.first));
    placed
}

/// The least a drag has to move, in pixels, before it is a drag and not a
/// press that wobbled.
pub const DRAG_SLOP: f32 = 5.0;

/// Rounds `minutes` to the nearest multiple of `step`.
pub fn snap(minutes: f32, step: u32) -> u32 {
    let step = step.max(1) as f32;
    ((minutes / step).round() * step).max(0.0) as u32
}

/// Rounds `minutes` down to a multiple of `step`.
pub fn snap_down(minutes: f32, step: u32) -> u32 {
    let step = step.max(1) as f32;
    ((minutes / step).floor() * step).max(0.0) as u32
}

/// The range a drag from `anchor` to `now` (minutes) makes: at least `step`
/// long, ordered, and inside the day.
pub fn drag_range(anchor: f32, now: f32, step: u32) -> Span {
    let (low, high) = if now >= anchor {
        (anchor, now)
    } else {
        (now, anchor)
    };
    let start = snap_down(low, step).min(24 * 60 - step);
    let end = snap(high, step).clamp(start + step, 24 * 60);
    Span { start, end }
}

/// Where a block goes when it is carried: its start follows the pointer less
/// where it was grabbed, and stays inside the day.
pub fn carried_start(pointer: f32, grabbed: f32, length: u32, step: u32) -> u32 {
    let latest = (24u32 * 60).saturating_sub(length);
    snap(pointer - grabbed, step).min(latest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(start: u32, end: u32) -> Span {
        Span { start, end }
    }

    fn d(y: i32, m: u8, day: u8) -> Date {
        Date::new(y, m, day).unwrap()
    }

    #[test]
    fn events_that_do_not_overlap_each_take_the_whole_width() {
        let slots = pack(&[span(540, 600), span(600, 660), span(720, 780)]);
        assert!(slots.iter().all(|s| s.columns == 1 && s.span == 1));
    }

    #[test]
    fn two_overlapping_events_sit_side_by_side() {
        let slots = pack(&[span(540, 660), span(600, 720)]);
        assert_eq!(slots[0].columns, 2);
        assert_eq!((slots[0].column, slots[1].column), (0, 1));
    }

    #[test]
    fn a_free_column_is_reused() {
        // A runs 9-12, B 9-10, C 10-11: B and C share the second column.
        let slots = pack(&[span(540, 720), span(540, 600), span(600, 660)]);
        assert_eq!(slots[0].column, 0);
        assert_eq!(slots[1].column, 1);
        assert_eq!(slots[2].column, 1);
        assert!(slots.iter().all(|s| s.columns == 2));
    }

    #[test]
    fn an_event_beside_two_others_is_in_the_third_column() {
        // B 9-12, A 9-10, C 9:30-10: three columns, B first.
        let slots = pack(&[span(540, 600), span(540, 720), span(570, 600)]);
        assert!(slots.iter().all(|s| s.columns == 3));
        assert_eq!(
            (slots[1].column, slots[0].column, slots[2].column),
            (0, 1, 2)
        );
        // None of them has a free column to widen into.
        assert!(slots.iter().all(|s| s.span == 1));
    }

    #[test]
    fn an_event_widens_over_columns_that_are_free_while_it_lasts() {
        // X 9-12, Y 9-10, Z 9-9:30 make three columns. W 11-12 takes Y's
        // column again and may widen over Z's, which is empty by then.
        let slots = pack(&[
            span(540, 720),
            span(540, 600),
            span(540, 570),
            span(660, 720),
        ]);
        assert!(slots.iter().all(|s| s.columns == 3));
        assert_eq!((slots[3].column, slots[3].span), (1, 2));
        // Y has Z beside it for as long as it lasts, so it stays one wide.
        assert_eq!(slots[1].span, 1);
    }

    #[test]
    fn a_very_short_event_is_given_room() {
        // A 5 minute event right after another 5 minute event overlaps its
        // minimum height, so they sit side by side.
        let slots = pack(&[span(600, 605), span(610, 615)]);
        assert_eq!(slots[0].columns, 2);
    }

    #[test]
    fn a_chain_of_overlaps_is_one_group() {
        // A 9-10, B 9:30-10:30, C 10-11: C starts as A ends, B joins them.
        let slots = pack(&[span(540, 600), span(570, 630), span(600, 660)]);
        assert!(slots.iter().all(|s| s.columns == 2));
        assert_eq!(slots[2].column, 0);
    }

    #[test]
    fn bars_are_cut_to_the_week() {
        // Week of Monday 10 March 2025.
        let week = d(2025, 3, 10);
        let runs = runs_in_week(
            week,
            &[
                (d(2025, 3, 8), d(2025, 3, 11)),
                (d(2025, 3, 12), d(2025, 3, 12)),
                (d(2025, 3, 15), d(2025, 3, 20)),
                (d(2025, 3, 1), d(2025, 3, 2)),
            ],
        );
        assert_eq!(runs.len(), 3);
        assert_eq!((runs[0].first, runs[0].last), (0, 1));
        assert!(runs[0].continues_before && !runs[0].continues_after);
        assert_eq!((runs[1].first, runs[1].last), (2, 2));
        assert_eq!((runs[2].first, runs[2].last), (5, 6));
        assert!(runs[2].continues_after);
    }

    #[test]
    fn bars_that_share_a_day_take_different_lanes() {
        let runs = [
            Run {
                index: 0,
                first: 0,
                last: 3,
                continues_before: false,
                continues_after: false,
            },
            Run {
                index: 1,
                first: 2,
                last: 4,
                continues_before: false,
                continues_after: false,
            },
            Run {
                index: 2,
                first: 4,
                last: 6,
                continues_before: false,
                continues_after: false,
            },
        ];
        let placed = lanes(&runs, 7);
        let lane_of = |index| placed.iter().find(|p| p.run.index == index).unwrap().lane;
        assert_eq!(lane_of(0), 0);
        assert_eq!(lane_of(1), 1);
        // The third shares column 4 with the second, so not its lane, and
        // shares nothing with the first, so lane 0.
        assert_eq!(lane_of(2), 0);
    }

    #[test]
    fn a_drag_makes_an_ordered_range_of_at_least_one_step() {
        assert_eq!(drag_range(600.0, 690.0, 15), span(600, 690));
        assert_eq!(drag_range(690.0, 600.0, 15), span(600, 690));
        assert_eq!(drag_range(600.0, 601.0, 15), span(600, 615));
        assert_eq!(drag_range(1439.0, 1500.0, 15), span(1425, 1440));
    }

    #[test]
    fn a_carried_block_stays_in_the_day() {
        assert_eq!(carried_start(700.0, 20.0, 60, 15), 675);
        assert_eq!(carried_start(1430.0, 0.0, 60, 15), 1380);
        assert_eq!(carried_start(5.0, 30.0, 60, 15), 0);
    }
}
