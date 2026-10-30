//! The rules of closing tabs, with no widgets in them, so each can be tested
//! without a window.
//!
//! * [`selection_after_close`]: which tab is selected once some are gone. A
//!   browser's rule: the neighbour to the right, else the one to the left.
//! * [`others`], [`to_the_left`] and [`to_the_right`]: the tabs the menu's
//!   *Close other tabs*, *Close tabs to the left* and *Close tabs to the
//!   right* close.
//! * [`menu_rows`]: what a tab's context menu offers, which items are greyed
//!   out, and where the separators fall.
//! * [`RapidClose`]: Chromium's *rapid closure mode*. After a tab is closed
//!   with the pointer the other tabs keep their width until the pointer
//!   leaves the strip, so the next close button comes under the pointer; then
//!   they grow to fill the room. A state machine over a clock the caller
//!   supplies, so a test can run it with a fake one.

use std::time::Duration;

/// The tab that is selected after `closed` are removed from `ids` (in strip
/// order). `selected` stays selected if it is not closed. If it is, the next
/// tab to its right that is not closed takes over, and failing that the
/// nearest one to its left. `None` when nothing is left.
pub fn selection_after_close(ids: &[u64], closed: &[u64], selected: u64) -> Option<u64> {
    let open = |id: &&u64| !closed.contains(id);
    match ids.iter().position(|id| *id == selected) {
        Some(_) if !closed.contains(&selected) => Some(selected),
        Some(at) => ids[at + 1..]
            .iter()
            .find(open)
            .or_else(|| ids[..at].iter().rev().find(open))
            .copied(),
        None => ids.iter().find(open).copied(),
    }
}

/// Every tab but `keep`, in strip order.
pub fn others(ids: &[u64], keep: u64) -> Vec<u64> {
    ids.iter().copied().filter(|id| *id != keep).collect()
}

/// The tabs before `of`, in strip order.
pub fn to_the_left(ids: &[u64], of: u64) -> Vec<u64> {
    match ids.iter().position(|id| *id == of) {
        Some(at) => ids[..at].to_vec(),
        None => Vec::new(),
    }
}

/// The tabs after `of`, in strip order.
pub fn to_the_right(ids: &[u64], of: u64) -> Vec<u64> {
    match ids.iter().position(|id| *id == of) {
        Some(at) => ids[at + 1..].to_vec(),
        None => Vec::new(),
    }
}

// ---- The context menu ------------------------------------------------------

/// What a tab's menu item does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Act {
    Close,
    CloseOthers,
    CloseToTheLeft,
    CloseToTheRight,
    Duplicate,
    Reopen,
    MoveLeft,
    MoveRight,
}

impl Act {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Act::Close => "Close tab",
            Act::CloseOthers => "Close other tabs",
            Act::CloseToTheLeft => "Close tabs to the left",
            Act::CloseToTheRight => "Close tabs to the right",
            Act::Duplicate => "Duplicate tab",
            Act::Reopen => "Reopen closed tab",
            Act::MoveLeft => "Move left",
            Act::MoveRight => "Move right",
        }
    }

    /// The shortcut the menu shows beside the item.
    pub(crate) fn shortcut(self) -> Option<&'static str> {
        match self {
            Act::Close => Some("Ctrl+W"),
            Act::Reopen => Some("Ctrl+Shift+T"),
            _ => None,
        }
    }
}

/// One row of the menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Row {
    Item { act: Act, enabled: bool },
    Separator,
}

/// What the host gave the strip to do, which decides what the menu offers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Offers {
    /// A handler for closing several tabs at once.
    pub close_many: bool,
    pub duplicate: bool,
    pub reopen: bool,
    /// There is a closed tab to bring back.
    pub can_reopen: bool,
    pub reorder: bool,
}

/// The rows of the menu of the tab at `place` among `count`: Close tab, then
/// the three ways of closing more (the others, those to the left, those to the
/// right), a separator, Duplicate and Reopen, a separator, Move left and Move
/// right. An item that cannot do anything here is greyed out, except the
/// moves, which are left out at the ends.
pub(crate) fn menu_rows(place: usize, count: usize, offers: Offers) -> Vec<Row> {
    let item = |act, enabled| Row::Item { act, enabled };
    let mut groups: Vec<Vec<Row>> = Vec::new();
    let mut close = vec![item(Act::Close, true)];
    if offers.close_many {
        close.push(item(Act::CloseOthers, count > 1));
        close.push(item(Act::CloseToTheLeft, place > 0));
        close.push(item(Act::CloseToTheRight, place + 1 < count));
    }
    groups.push(close);
    let mut open = Vec::new();
    if offers.duplicate {
        open.push(item(Act::Duplicate, true));
    }
    if offers.reopen {
        open.push(item(Act::Reopen, offers.can_reopen));
    }
    groups.push(open);
    let mut moves = Vec::new();
    if offers.reorder {
        if place > 0 {
            moves.push(item(Act::MoveLeft, true));
        }
        if place + 1 < count {
            moves.push(item(Act::MoveRight, true));
        }
    }
    groups.push(moves);
    let mut rows = Vec::new();
    for group in groups.into_iter().filter(|group| !group.is_empty()) {
        if !rows.is_empty() {
            rows.push(Row::Separator);
        }
        rows.extend(group);
    }
    rows
}

// ---- Rapid closure ---------------------------------------------------------

/// Where the tab widths are in a rapid close.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Hold {
    /// The widths are what the room gives.
    Idle,
    /// A tab was closed with the pointer: the widths stay `width`, so the
    /// next close button is where the pointer is.
    Held { width: f32 },
    /// The pointer left: the widths grow from `from` to what the room gives,
    /// starting at `since`.
    Releasing { from: f32, since: Duration },
}

/// The widths of the tabs of a strip while the person closes them one after
/// another with the pointer. The strip tells it what happened and asks it how
/// wide a tab is; it never reads a clock itself.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RapidClose {
    hold: Hold,
}

impl Default for RapidClose {
    fn default() -> Self {
        Self { hold: Hold::Idle }
    }
}

impl RapidClose {
    /// A tab was closed with the pointer (its close button, a middle click)
    /// while the tabs were `shown` wide. The widths are held from here until
    /// the pointer leaves the strip. A second close keeps the first one's
    /// width, unless the tabs have since been drawn narrower.
    pub(crate) fn closed_with_pointer(&mut self, shown: f32) {
        self.hold = Hold::Held {
            width: match self.hold {
                Hold::Held { width } => width.min(shown),
                _ => shown,
            },
        };
    }

    /// The pointer entered or left the strip, `now` on the caller's clock.
    /// Leaving lets the held widths go.
    pub(crate) fn pointer_in_strip(&mut self, inside: bool, now: Duration) {
        if let (false, Hold::Held { width }) = (inside, self.hold) {
            self.hold = Hold::Releasing { from: width, since: now };
        }
    }

    /// A tab was added: there is no pointer on a close button now, so the
    /// widths are whatever the room gives.
    pub(crate) fn reset(&mut self) {
        self.hold = Hold::Idle;
    }

    /// The width of a tab, given `natural`, the width the room gives it.
    /// `motion` is how long the growth takes after the pointer has left: no
    /// time at all with reduced motion.
    pub(crate) fn tab_width(&self, natural: f32, now: Duration, motion: Duration) -> f32 {
        match self.hold {
            Hold::Idle => natural,
            // Held tabs are never wider than the room allows: adding a tab
            // takes the room back.
            Hold::Held { width } => width.min(natural),
            Hold::Releasing { from, since } => {
                let from = from.min(natural);
                let progress = Self::progress(now, since, motion);
                let eased = 1.0 - (1.0 - progress) * (1.0 - progress);
                from + (natural - from) * eased
            }
        }
    }

    /// Whether the widths are the room's: nothing held, nothing growing.
    #[cfg(test)]
    pub(crate) fn is_idle(&self) -> bool {
        self.hold == Hold::Idle
    }

    /// Whether the widths are being held for the pointer.
    pub(crate) fn is_held(&self) -> bool {
        matches!(self.hold, Hold::Held { .. })
    }

    /// Whether the growth after the pointer left is still going at `now`.
    pub(crate) fn is_growing(&self, now: Duration, motion: Duration) -> bool {
        matches!(self.hold, Hold::Releasing { since, .. } if Self::progress(now, since, motion) < 1.0)
    }

    /// Return to rest once the growth is over. Whether anything changed.
    pub(crate) fn settle(&mut self, now: Duration, motion: Duration) -> bool {
        if let Hold::Releasing { since, .. } = self.hold {
            if Self::progress(now, since, motion) >= 1.0 {
                self.hold = Hold::Idle;
                return true;
            }
        }
        false
    }

    fn progress(now: Duration, since: Duration, motion: Duration) -> f32 {
        if motion.is_zero() {
            return 1.0;
        }
        (now.saturating_sub(since).as_secs_f32() / motion.as_secs_f32()).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MOTION: Duration = Duration::from_millis(200);

    fn ms(ms: u64) -> Duration {
        Duration::from_millis(ms)
    }

    // ---- Selection

    #[test]
    fn closing_the_selected_tab_selects_the_one_to_its_right() {
        let ids = [1, 2, 3, 4];
        assert_eq!(selection_after_close(&ids, &[2], 2), Some(3));
        assert_eq!(selection_after_close(&ids, &[1], 1), Some(2));
    }

    #[test]
    fn closing_the_last_tab_selects_the_one_to_its_left() {
        let ids = [1, 2, 3, 4];
        assert_eq!(selection_after_close(&ids, &[4], 4), Some(3));
    }

    #[test]
    fn closing_another_tab_keeps_the_selection() {
        let ids = [1, 2, 3, 4];
        for closed in [1, 3, 4] {
            assert_eq!(selection_after_close(&ids, &[closed], 2), Some(2));
        }
    }

    #[test]
    fn closing_the_only_tab_leaves_nothing() {
        assert_eq!(selection_after_close(&[7], &[7], 7), None);
    }

    #[test]
    fn closing_several_picks_the_nearest_tab_that_stays() {
        let ids = [1, 2, 3, 4, 5];
        // Tabs to the right of 2 go: 2 stays selected.
        assert_eq!(selection_after_close(&ids, &to_the_right(&ids, 2), 2), Some(2));
        // The selected tab was one of those to the right: the one it was
        // closed beside is selected, since nothing to its right stays.
        assert_eq!(selection_after_close(&ids, &to_the_right(&ids, 2), 4), Some(2));
        // All but 3 go while 5 is selected: only 3 is left.
        assert_eq!(selection_after_close(&ids, &others(&ids, 3), 5), Some(3));
        // The right neighbour wins over the left one when both stay.
        assert_eq!(selection_after_close(&ids, &[3, 4], 3), Some(5));
    }

    #[test]
    fn the_sets_a_menu_closes_follow_strip_order() {
        let ids = [10, 20, 30, 40];
        assert_eq!(others(&ids, 30), [10, 20, 40]);
        assert_eq!(to_the_right(&ids, 20), [30, 40]);
        assert_eq!(to_the_right(&ids, 40), Vec::<u64>::new(), "nothing is right of the last tab");
        assert_eq!(to_the_right(&ids, 99), Vec::<u64>::new(), "nor of one that is not there");
        assert_eq!(to_the_left(&ids, 30), [10, 20]);
        assert_eq!(to_the_left(&ids, 10), Vec::<u64>::new(), "nothing is left of the first tab");
        assert_eq!(to_the_left(&ids, 99), Vec::<u64>::new(), "nor of one that is not there");
        assert_eq!(others(&[10], 10), Vec::<u64>::new());
        // The two sides and the tab itself are every tab, once.
        for of in ids {
            let mut every = to_the_left(&ids, of);
            every.push(of);
            every.extend(to_the_right(&ids, of));
            assert_eq!(every, ids);
        }
    }

    #[test]
    fn closing_the_tabs_to_the_left_keeps_the_one_it_was_opened_on_selected_when_the_selection_went() {
        let ids = [1, 2, 3, 4, 5];
        // The tab the menu is on stays, and stays selected if it was.
        assert_eq!(selection_after_close(&ids, &to_the_left(&ids, 4), 4), Some(4));
        // The selection was among those that went: the next tab to its right that
        // stays is the one the menu was opened on.
        assert_eq!(selection_after_close(&ids, &to_the_left(&ids, 4), 2), Some(4));
        // The selection was to the right: it stays.
        assert_eq!(selection_after_close(&ids, &to_the_left(&ids, 3), 5), Some(5));
    }

    // ---- The menu

    fn acts(rows: &[Row]) -> Vec<(Act, bool)> {
        rows.iter()
            .filter_map(|row| match row {
                Row::Item { act, enabled } => Some((*act, *enabled)),
                Row::Separator => None,
            })
            .collect()
    }

    const ALL: Offers = Offers { close_many: true, duplicate: true, reopen: true, can_reopen: true, reorder: true };

    #[test]
    fn a_middle_tab_offers_everything_in_the_order_of_a_browsers_menu() {
        let rows = menu_rows(1, 3, ALL);
        assert_eq!(
            acts(&rows),
            [
                (Act::Close, true),
                (Act::CloseOthers, true),
                (Act::CloseToTheLeft, true),
                (Act::CloseToTheRight, true),
                (Act::Duplicate, true),
                (Act::Reopen, true),
                (Act::MoveLeft, true),
                (Act::MoveRight, true),
            ]
        );
        assert_eq!(rows.iter().filter(|row| **row == Row::Separator).count(), 2, "three groups, two separators");
        assert_eq!(rows.first(), Some(&Row::Item { act: Act::Close, enabled: true }));
    }

    #[test]
    fn the_last_tab_has_no_tabs_to_its_right_the_first_none_to_its_left_and_a_lone_tab_has_no_others() {
        let last = acts(&menu_rows(2, 3, ALL));
        assert!(last.contains(&(Act::CloseToTheRight, false)));
        assert!(last.contains(&(Act::CloseToTheLeft, true)));
        assert!(last.contains(&(Act::CloseOthers, true)));
        assert!(!last.iter().any(|(act, _)| *act == Act::MoveRight), "no move past the end");
        let first = acts(&menu_rows(0, 3, ALL));
        assert!(first.contains(&(Act::CloseToTheLeft, false)), "nothing is left of the first tab: the item is greyed out");
        assert!(first.contains(&(Act::CloseToTheRight, true)));
        let lone = acts(&menu_rows(0, 1, ALL));
        assert!(lone.contains(&(Act::CloseOthers, false)));
        assert!(lone.contains(&(Act::CloseToTheLeft, false)));
        assert!(lone.contains(&(Act::CloseToTheRight, false)));
        assert!(lone.contains(&(Act::Close, true)), "a lone tab can still be closed");
        assert!(!lone.iter().any(|(act, _)| matches!(act, Act::MoveLeft | Act::MoveRight)));
    }

    #[test]
    fn reopen_is_greyed_out_until_a_tab_has_been_closed() {
        let none = Offers { can_reopen: false, ..ALL };
        assert!(acts(&menu_rows(0, 2, none)).contains(&(Act::Reopen, false)));
        assert!(acts(&menu_rows(0, 2, ALL)).contains(&(Act::Reopen, true)));
    }

    #[test]
    fn a_strip_with_only_close_offers_close() {
        let rows = menu_rows(0, 3, Offers::default());
        assert_eq!(rows, [Row::Item { act: Act::Close, enabled: true }], "no separators around a lone group");
    }

    #[test]
    fn the_menu_has_no_leading_or_doubled_separators() {
        for place in 0..4 {
            for offers in [
                ALL,
                Offers::default(),
                Offers { duplicate: true, ..Offers::default() },
                Offers { reorder: true, ..Offers::default() },
                Offers { reopen: true, ..Offers::default() },
            ] {
                let rows = menu_rows(place, 4, offers);
                assert_ne!(rows.first(), Some(&Row::Separator));
                assert_ne!(rows.last(), Some(&Row::Separator));
                assert!(!rows.windows(2).any(|pair| pair[0] == Row::Separator && pair[1] == Row::Separator));
            }
        }
    }

    #[test]
    fn labels_and_shortcuts_are_the_browsers() {
        assert_eq!(Act::Close.label(), "Close tab");
        assert_eq!(Act::CloseOthers.label(), "Close other tabs");
        assert_eq!(Act::CloseToTheLeft.label(), "Close tabs to the left");
        assert_eq!(Act::CloseToTheRight.label(), "Close tabs to the right");
        assert_eq!(Act::Duplicate.label(), "Duplicate tab");
        assert_eq!(Act::Reopen.label(), "Reopen closed tab");
        assert_eq!(Act::Close.shortcut(), Some("Ctrl+W"));
        assert_eq!(Act::Reopen.shortcut(), Some("Ctrl+Shift+T"));
        assert_eq!(Act::Duplicate.shortcut(), None);
    }

    // ---- Rapid closure, with a fake clock

    #[test]
    fn widths_follow_the_room_until_a_tab_is_closed_with_the_pointer() {
        let rapid = RapidClose::default();
        assert!(rapid.is_idle());
        assert_eq!(rapid.tab_width(150.0, ms(0), MOTION), 150.0);
        assert_eq!(rapid.tab_width(240.0, ms(5000), MOTION), 240.0);
    }

    #[test]
    fn the_width_is_held_while_the_pointer_stays_in_the_strip() {
        let mut rapid = RapidClose::default();
        rapid.closed_with_pointer(150.0);
        assert!(rapid.is_held());
        // The room now gives 200 (one tab fewer): the tabs stay 150.
        assert_eq!(rapid.tab_width(200.0, ms(0), MOTION), 150.0);
        assert_eq!(rapid.tab_width(240.0, ms(60_000), MOTION), 150.0, "however long the pointer stays");
        rapid.pointer_in_strip(true, ms(10));
        assert_eq!(rapid.tab_width(240.0, ms(60_000), MOTION), 150.0, "entering changes nothing");
    }

    #[test]
    fn a_second_close_keeps_the_first_width() {
        let mut rapid = RapidClose::default();
        rapid.closed_with_pointer(150.0);
        // The strip draws 150 while the room gives 200; the second close
        // reports what is drawn.
        rapid.closed_with_pointer(150.0);
        assert_eq!(rapid.tab_width(240.0, ms(0), MOTION), 150.0);
        // Had the tabs been drawn narrower since (a tab was added), they stay narrower.
        rapid.closed_with_pointer(120.0);
        assert_eq!(rapid.tab_width(240.0, ms(0), MOTION), 120.0);
    }

    #[test]
    fn leaving_the_strip_grows_the_tabs_to_the_room_over_the_motion() {
        let mut rapid = RapidClose::default();
        rapid.closed_with_pointer(150.0);
        rapid.pointer_in_strip(false, ms(1000));
        assert!(!rapid.is_held());
        assert!(rapid.is_growing(ms(1000), MOTION));
        assert_eq!(rapid.tab_width(210.0, ms(1000), MOTION), 150.0, "it starts where it was held");
        let half = rapid.tab_width(210.0, ms(1100), MOTION);
        assert!(half > 150.0 && half < 210.0, "half way through it is in between: {half}");
        assert!(half > 180.0, "ease out: more than half the way at half the time ({half})");
        assert_eq!(rapid.tab_width(210.0, ms(1200), MOTION), 210.0);
        assert_eq!(rapid.tab_width(210.0, ms(9999), MOTION), 210.0);
        assert!(!rapid.is_growing(ms(1200), MOTION));
        assert!(!rapid.settle(ms(1100), MOTION), "not settled before the end");
        assert!(rapid.settle(ms(1200), MOTION));
        assert!(rapid.is_idle());
        assert!(!rapid.settle(ms(1300), MOTION), "settling twice changes nothing");
    }

    #[test]
    fn widths_are_monotonic_while_they_grow() {
        let mut rapid = RapidClose::default();
        rapid.closed_with_pointer(96.0);
        rapid.pointer_in_strip(false, ms(0));
        let mut last = 0.0;
        for t in (0..=250).step_by(10) {
            let width = rapid.tab_width(240.0, ms(t), MOTION);
            assert!(width >= last - 0.001, "the width never shrinks while it grows: {width} after {last}");
            last = width;
        }
        assert_eq!(last, 240.0);
    }

    #[test]
    fn with_reduced_motion_the_tabs_grow_at_once() {
        let mut rapid = RapidClose::default();
        rapid.closed_with_pointer(150.0);
        rapid.pointer_in_strip(false, ms(500));
        assert_eq!(rapid.tab_width(210.0, ms(500), Duration::ZERO), 210.0);
        assert!(!rapid.is_growing(ms(500), Duration::ZERO));
        assert!(rapid.settle(ms(500), Duration::ZERO));
    }

    #[test]
    fn leaving_without_a_close_does_nothing() {
        let mut rapid = RapidClose::default();
        rapid.pointer_in_strip(false, ms(0));
        assert!(rapid.is_idle());
        assert_eq!(rapid.tab_width(170.0, ms(0), MOTION), 170.0);
    }

    #[test]
    fn adding_a_tab_takes_the_held_room_back() {
        let mut rapid = RapidClose::default();
        rapid.closed_with_pointer(150.0);
        // A tab arrives and the room gives 120: the held 150 would not fit.
        assert_eq!(rapid.tab_width(120.0, ms(0), MOTION), 120.0);
        rapid.reset();
        assert!(rapid.is_idle());
        assert_eq!(rapid.tab_width(200.0, ms(0), MOTION), 200.0);
    }

    #[test]
    fn a_close_while_the_widths_are_growing_holds_them_where_they_are() {
        let mut rapid = RapidClose::default();
        rapid.closed_with_pointer(150.0);
        rapid.pointer_in_strip(false, ms(0));
        // The pointer came back to a tab part way through the growth and
        // closed it: the strip draws `shown` wide, and that is held.
        let shown = rapid.tab_width(210.0, ms(50), MOTION);
        rapid.closed_with_pointer(shown);
        assert!(rapid.is_held());
        assert_eq!(rapid.tab_width(240.0, ms(5000), MOTION), shown);
    }
}
