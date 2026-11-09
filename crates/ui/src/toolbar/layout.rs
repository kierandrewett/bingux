//! Which toolbar items fit, apart from anything drawn.

/// What the fit needs to know about an item.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Slot {
    pub width: f32,
    /// How soon the item goes into the overflow menu: the highest number goes
    /// first. `None` for an item that never goes (the prominent action, an
    /// element that cannot be a menu row).
    pub rank: Option<u8>,
    /// The group the item belongs to; neighbours in one group sit closer.
    pub group: usize,
    /// The cluster the item is in; clusters have the title between them, so no
    /// gap is counted from one to the next.
    pub cluster: u8,
}

/// The width the visible `slots` take, with `gap` between items of a group and
/// `group_gap` between groups of a cluster.
fn width_of(slots: &[Slot], visible: &[bool], gap: f32, group_gap: f32) -> f32 {
    let mut total = 0.0;
    let mut previous: Option<&Slot> = None;
    for (slot, _) in slots.iter().zip(visible).filter(|(_, shown)| **shown) {
        if let Some(before) = previous
            && before.cluster == slot.cluster
        {
            total += if before.group == slot.group {
                gap
            } else {
                group_gap
            };
        }
        total += slot.width;
        previous = Some(slot);
    }
    total
}

/// Which of `slots` stay in the bar when it has `available` px. When they all
/// fit, all stay. Otherwise the overflow button (`more` px wide, with `gap` before
/// it) takes its room and items go into the menu, the highest rank first and,
/// among equals, the last first, until the rest fit. An item with no rank
/// stays whatever the room.
pub(super) fn fit(
    slots: &[Slot],
    available: f32,
    gap: f32,
    group_gap: f32,
    more: f32,
) -> Vec<bool> {
    let mut visible = vec![true; slots.len()];
    if width_of(slots, &visible, gap, group_gap) <= available + 0.5 {
        return visible;
    }
    let room = available - more - gap;
    loop {
        let victim = (0..slots.len())
            .filter(|&at| visible[at])
            .filter_map(|at| slots[at].rank.map(|rank| (rank, at)))
            .max();
        match victim {
            Some((_, at)) if width_of(slots, &visible, gap, group_gap) > room + 0.5 => {
                visible[at] = false;
            }
            _ => return visible,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(width: f32, rank: Option<u8>, group: usize, cluster: u8) -> Slot {
        Slot {
            width,
            rank,
            group,
            cluster,
        }
    }

    #[test]
    fn everything_stays_when_it_fits() {
        let slots = [slot(36.0, Some(1), 0, 0), slot(36.0, Some(1), 0, 1)];
        assert_eq!(fit(&slots, 100.0, 4.0, 12.0, 36.0), [true, true]);
    }

    #[test]
    fn secondary_items_go_before_primary_ones() {
        // Back, Forward | Add, Share (secondary), Delete (secondary), Save (never).
        let slots = [
            slot(36.0, Some(0), 0, 0),
            slot(36.0, Some(0), 0, 0),
            slot(36.0, Some(1), 1, 1),
            slot(36.0, Some(2), 2, 1),
            slot(36.0, Some(2), 2, 1),
            slot(80.0, None, 3, 1),
        ];
        // All: 36+4+36 + 36 + 12+36 + 4+36 + 12+80 = 292. At 250 the button takes
        // 40 and the room is 210: both secondaries go, the later first.
        let shown = fit(&slots, 250.0, 4.0, 12.0, 36.0);
        assert_eq!(shown, [true, true, true, false, false, true]);
        // At 150 the room is 110: the primary Add goes, then the navigation
        // items, last first; the one that never goes stays.
        let shown = fit(&slots, 150.0, 4.0, 12.0, 36.0);
        assert_eq!(shown, [false, false, false, false, false, true]);
    }

    #[test]
    fn an_item_that_never_goes_stays_when_nothing_fits() {
        let slots = [slot(36.0, Some(1), 0, 0), slot(120.0, None, 1, 0)];
        assert_eq!(fit(&slots, 60.0, 4.0, 12.0, 36.0), [false, true]);
    }

    #[test]
    fn the_overflow_button_takes_room_before_the_fit_is_judged() {
        // Three 36 px items in 120 px fit (116); four do not, and the button
        // takes 40 px, so only two stay.
        let slots = [slot(36.0, Some(1), 0, 0); 4];
        assert_eq!(
            fit(&slots, 120.0, 4.0, 12.0, 36.0),
            [true, true, false, false]
        );
    }
}
