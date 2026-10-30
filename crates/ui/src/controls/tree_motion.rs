//! The timeline of a list whose rows come and go, such as a tree view when a
//! branch opens or closes. Pure logic with an injected clock: it decides which
//! rows to draw this frame, in order, and how each one is moving. The rows
//! animate their own height; this only keeps a removed row in the list for as
//! long as its closing motion takes, at the place it was, so the rows below it
//! move with its edge instead of jumping.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

/// How a drawn row is moving.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Slot {
    /// In the list and at rest.
    Settled,
    /// Added after the list first showed, and still growing in.
    Entering,
    /// No longer in the list, and still shrinking away.
    Leaving,
}

#[derive(Clone)]
struct Entry<T> {
    id: u64,
    item: T,
    entered: Option<Duration>,
    left: Option<Duration>,
}

/// The rows drawn so far, with the clock time each was added or removed at.
pub(crate) struct Timeline<T> {
    entries: Vec<Entry<T>>,
    primed: bool,
}

impl<T: Clone> Timeline<T> {
    pub(crate) fn new() -> Self {
        Self {
            entries: Vec::new(),
            primed: false,
        }
    }

    /// The rows to draw at `now`: `rows`, in order, with each row removed since
    /// the last call kept in place for `ms` milliseconds more. The first call
    /// adds every row at rest, so a list that mounts never animates. `ms` of 0
    /// (reduced motion) keeps nothing.
    pub(crate) fn sync(
        &mut self,
        rows: impl IntoIterator<Item = (u64, T)>,
        now: Duration,
        ms: u64,
    ) -> Vec<(u64, T, Slot)> {
        let motion = Duration::from_millis(ms);
        let rows: Vec<(u64, T)> = rows.into_iter().collect();
        let current: HashSet<u64> = rows.iter().map(|(id, _)| *id).collect();
        let first = !self.primed;
        self.primed = true;

        // A row that has left stays after the nearest earlier row that is
        // still in the list, or at the start when none is.
        let mut ghosts: HashMap<Option<u64>, Vec<Entry<T>>> = HashMap::new();
        let mut anchor = None;
        let mut before: HashMap<u64, Option<Duration>> = HashMap::new();
        for entry in &self.entries {
            if current.contains(&entry.id) {
                anchor = Some(entry.id);
                before.insert(entry.id, entry.entered);
                continue;
            }
            let mut ghost = entry.clone();
            let since = *ghost.left.get_or_insert(now);
            if ms == 0 || now.saturating_sub(since) >= motion {
                continue;
            }
            ghosts.entry(anchor).or_default().push(ghost);
        }

        let mut entries = Vec::with_capacity(rows.len());
        entries.extend(ghosts.remove(&None).unwrap_or_default());
        for (id, item) in rows {
            let entered = match before.get(&id) {
                Some(entered) => *entered,
                None if first || ms == 0 => None,
                None => Some(now),
            };
            entries.push(Entry {
                id,
                item,
                entered,
                left: None,
            });
            entries.extend(ghosts.remove(&Some(id)).unwrap_or_default());
        }

        let drawn = entries
            .iter()
            .map(|entry| {
                let slot = if entry.left.is_some() {
                    Slot::Leaving
                } else if entry
                    .entered
                    .is_some_and(|since| now.saturating_sub(since) < motion)
                {
                    Slot::Entering
                } else {
                    Slot::Settled
                };
                (entry.id, entry.item.clone(), slot)
            })
            .collect();
        self.entries = entries;
        drawn
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn ids(drawn: &[(u64, char, Slot)]) -> Vec<(u64, Slot)> {
        drawn.iter().map(|(id, _, slot)| (*id, *slot)).collect()
    }

    fn rows(ids: &[u64]) -> Vec<(u64, char)> {
        ids.iter().map(|id| (*id, 'x')).collect()
    }

    #[test]
    fn the_first_rows_are_at_rest() {
        let mut timeline = Timeline::new();
        let drawn = timeline.sync(rows(&[1, 2]), ms(0), 160);
        assert_eq!(ids(&drawn), [(1, Slot::Settled), (2, Slot::Settled)]);
    }

    #[test]
    fn an_added_row_enters_then_settles() {
        let mut timeline = Timeline::new();
        timeline.sync(rows(&[1]), ms(0), 160);
        let drawn = timeline.sync(rows(&[1, 2, 3]), ms(10), 160);
        assert_eq!(
            ids(&drawn),
            [(1, Slot::Settled), (2, Slot::Entering), (3, Slot::Entering)]
        );
        let drawn = timeline.sync(rows(&[1, 2, 3]), ms(170), 160);
        assert!(drawn.iter().all(|(_, _, slot)| *slot == Slot::Settled));
    }

    #[test]
    fn a_removed_row_stays_in_place_for_the_motion() {
        let mut timeline = Timeline::new();
        timeline.sync(rows(&[1, 2, 3, 4]), ms(0), 160);
        let drawn = timeline.sync(rows(&[1, 4]), ms(100), 160);
        assert_eq!(
            ids(&drawn),
            [
                (1, Slot::Settled),
                (2, Slot::Leaving),
                (3, Slot::Leaving),
                (4, Slot::Settled)
            ]
        );
        // Still there part-way, gone at the end.
        let drawn = timeline.sync(rows(&[1, 4]), ms(200), 160);
        assert_eq!(drawn.len(), 4);
        let drawn = timeline.sync(rows(&[1, 4]), ms(260), 160);
        assert_eq!(ids(&drawn), [(1, Slot::Settled), (4, Slot::Settled)]);
    }

    #[test]
    fn a_row_removed_from_the_top_keeps_the_top() {
        let mut timeline = Timeline::new();
        timeline.sync(rows(&[1, 2]), ms(0), 160);
        let drawn = timeline.sync(rows(&[2]), ms(10), 160);
        assert_eq!(ids(&drawn), [(1, Slot::Leaving), (2, Slot::Settled)]);
    }

    #[test]
    fn a_row_that_comes_back_mid_exit_is_live_again() {
        let mut timeline = Timeline::new();
        timeline.sync(rows(&[1, 2]), ms(0), 160);
        timeline.sync(rows(&[1]), ms(50), 160);
        let drawn = timeline.sync(rows(&[1, 2]), ms(90), 160);
        // The same slot, so its own tween turns around from where it is.
        assert_eq!(ids(&drawn), [(1, Slot::Settled), (2, Slot::Settled)]);
    }

    #[test]
    fn reduced_motion_keeps_nothing() {
        let mut timeline = Timeline::new();
        timeline.sync(rows(&[1, 2]), ms(0), 0);
        let drawn = timeline.sync(rows(&[1, 3]), ms(10), 0);
        assert_eq!(ids(&drawn), [(1, Slot::Settled), (3, Slot::Settled)]);
    }
}
