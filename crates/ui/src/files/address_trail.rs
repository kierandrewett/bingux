//! Which crumbs of a path the address bar shows, and what each says.
//!
//! The decision is made on widths, not on counts of characters: the caller
//! measures real text and passes the widths in, so a trail of `iii` crumbs
//! fits where a trail of `WWW` crumbs does not.
//!
//! The rules, in order:
//!
//! 1. A crumb whose name is wider than `Metrics::widest` is cut in the
//!    middle (`final-rep…2026`).
//! 2. A trail that fits is shown whole.
//! 3. Otherwise the first and the last crumb stay, and the middle ones give
//!    way to one `…` crumb that stands for them. The crumbs nearest the end
//!    stay first, then the ones nearest the start, as many as fit.
//! 4. When even the first, the `…` and the last do not fit, the last name is
//!    cut in the middle.

use std::ops::Range;

use crate::controls::path_label::elide_middle;

/// One thing in the trail.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Item {
    /// The crumb at `index` of the full trail, with the `text` it shows.
    Crumb { index: usize, text: String },
    /// One `…` crumb for the crumbs of `hidden`.
    Overflow { hidden: Range<usize> },
}

/// The widths a crumb takes besides its text.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Metrics {
    /// The crumb button's padding, both sides.
    pub crumb: f32,
    /// What the first crumb adds, such as its icon.
    pub first_extra: f32,
    /// The chevron button after a crumb.
    pub chevron: f32,
    /// The widest a name gets before it is cut.
    pub widest: f32,
}

/// The crumbs to draw for `labels` in `room`, measured with `measure`.
pub(super) fn layout(
    labels: &[String],
    metrics: Metrics,
    room: f32,
    measure: &dyn Fn(&str) -> f32,
) -> Vec<Item> {
    let texts: Vec<String> = labels
        .iter()
        .map(|label| elide_middle(label, metrics.widest, measure))
        .collect();
    let widths: Vec<f32> = texts
        .iter()
        .enumerate()
        .map(|(index, text)| {
            let extra = if index == 0 { metrics.first_extra } else { 0.0 };
            measure(text) + metrics.crumb + metrics.chevron + extra
        })
        .collect();
    let overflow = measure("…") + metrics.crumb + metrics.chevron;
    let hidden = hidden_range(&widths, overflow, room);

    let mut items = Vec::new();
    for (index, text) in texts.into_iter().enumerate() {
        if hidden.contains(&index) {
            if index == hidden.start {
                items.push(Item::Overflow {
                    hidden: hidden.clone(),
                });
            }
            continue;
        }
        items.push(Item::Crumb { index, text });
    }

    // Still too wide: the last name gives up its middle.
    let used: f32 = items
        .iter()
        .map(|item| match item {
            Item::Crumb { index, .. } => widths[*index],
            Item::Overflow { .. } => overflow,
        })
        .sum();
    if used > room
        && let Some(Item::Crumb { index, text }) = items.last_mut()
    {
        let others = used - widths[*index];
        let extra = if *index == 0 {
            metrics.first_extra
        } else {
            0.0
        };
        let text_room = room - others - metrics.crumb - metrics.chevron - extra;
        *text = elide_middle(&labels[*index], text_room.max(0.0), measure);
    }
    items
}

/// The crumbs that give way to the `…` crumb: a run in the middle, empty when
/// everything fits. `widths[i]` is crumb `i` with its chevron; `overflow` is
/// the `…` crumb with its chevron.
pub(super) fn hidden_range(widths: &[f32], overflow: f32, room: f32) -> Range<usize> {
    let count = widths.len();
    let total: f32 = widths.iter().sum();
    if count <= 2 || total <= room {
        return 0..0;
    }
    let mut budget = room - widths[0] - widths[count - 1] - overflow;
    // `start..=end` is what is left to decide; crumbs are kept from the end
    // and from the start in turn, the end first.
    let (mut start, mut end) = (1, count - 2);
    let mut from_end = true;
    loop {
        let next = if from_end { end } else { start };
        if budget < widths[next] {
            break;
        }
        budget -= widths[next];
        if from_end {
            if end == start {
                // Everything was kept: only possible when the budget allowed
                // it, which means the trail fits and was returned above.
                return 0..0;
            }
            end -= 1;
        } else {
            if start == end {
                return 0..0;
            }
            start += 1;
        }
        from_end = !from_end;
    }
    start..end + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    const METRICS: Metrics = Metrics {
        crumb: 16.0,
        first_extra: 20.0,
        chevron: 20.0,
        widest: 100.0,
    };

    fn eight(text: &str) -> f32 {
        text.chars().count() as f32 * 8.0
    }

    fn labels(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    fn shown(items: &[Item]) -> Vec<String> {
        items
            .iter()
            .map(|item| match item {
                Item::Crumb { text, .. } => text.clone(),
                Item::Overflow { .. } => "…".into(),
            })
            .collect()
    }

    #[test]
    fn a_trail_that_fits_is_whole() {
        let items = layout(&labels(&["Home", "a", "b"]), METRICS, 600.0, &eight);
        assert_eq!(shown(&items), ["Home", "a", "b"]);
    }

    #[test]
    fn the_middle_gives_way_and_the_ends_stay() {
        let names = labels(&["Home", "one", "two", "three", "four", "five"]);
        // Home 72, each other crumb 36 + 8 * len. Room for Home, …, the last.
        let items = layout(&names, METRICS, 220.0, &eight);
        let text = shown(&items);
        assert_eq!(text.first().unwrap(), "Home");
        assert_eq!(text.last().unwrap(), "five");
        assert!(text.contains(&"…".to_string()));
        let Some(Item::Overflow { hidden }) =
            items.iter().find(|i| matches!(i, Item::Overflow { .. }))
        else {
            panic!("no overflow");
        };
        // The hidden run is contiguous and never holds the first or last.
        assert!(hidden.start >= 1 && hidden.end <= names.len() - 1);
    }

    #[test]
    fn the_nearest_to_the_end_are_kept_first() {
        let names = labels(&["Home", "aaa", "bbb", "ccc", "ddd", "eee"]);
        let widths: Vec<f32> = names
            .iter()
            .map(|n| eight(n) + 36.0 + if n == "Home" { 20.0 } else { 0.0 })
            .collect();
        let overflow = eight("…") + 36.0;
        // Room for the ends, the overflow and exactly one more crumb.
        let room = widths[0] + widths[5] + overflow + widths[4];
        assert_eq!(hidden_range(&widths, overflow, room), 1..4);
        // One more crumb's room keeps the one nearest the start too.
        let room = room + widths[1];
        assert_eq!(hidden_range(&widths, overflow, room), 2..4);
    }

    #[test]
    fn more_room_never_hides_more() {
        let widths = [90.0, 50.0, 70.0, 60.0, 80.0, 55.0, 100.0];
        let overflow = 40.0;
        let mut previous = widths.len();
        let mut room = 10.0;
        while room < 600.0 {
            let hidden = hidden_range(&widths, overflow, room);
            assert!(hidden.len() <= previous, "room {room}");
            previous = hidden.len();
            room += 5.0;
        }
        assert!(previous == 0);
    }

    #[test]
    fn two_crumbs_are_never_collapsed() {
        let widths = [300.0, 300.0];
        assert_eq!(hidden_range(&widths, 40.0, 50.0), 0..0);
    }

    #[test]
    fn too_little_room_hides_the_whole_middle_and_cuts_the_last() {
        let names = labels(&["Home", "one", "two", "a-very-long-folder-name-indeed"]);
        let items = layout(&names, METRICS, 250.0, &eight);
        let text = shown(&items);
        assert_eq!(text[0], "Home");
        assert_eq!(text[1], "…");
        let last = text.last().unwrap();
        assert!(last.contains('…') && last.starts_with("a-v"));
        assert!(last.ends_with('e') || last.ends_with('d'));
    }

    #[test]
    fn a_long_name_is_cut_in_the_middle() {
        let long = "x".repeat(40);
        let items = layout(&[long.clone()], METRICS, 600.0, &eight);
        let text = shown(&items);
        assert!(text[0].contains('…'));
        assert!(eight(&text[0]) <= METRICS.widest);
    }
}
