//! The tabs of a window and which one is on show.
//!
//! Pinned tabs are always first and stay in a block of their own: moving a
//! tab never carries it across the line between the two groups. Closing the
//! tab on show goes to the tab it was opened from, if that is still open, and
//! else to the neighbour that takes its place (the one on the right, or the
//! last when the closed tab was).

use bingux_ui::web_view::TabId;

use crate::tabs::Tab;

/// Where a new tab goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// After every tab.
    End,
    /// Right after the tab on show (and after the pinned ones).
    AfterActive,
    /// Right after the tab with this key.
    After(u64),
}

#[derive(Clone, Debug, Default)]
pub struct TabList {
    tabs: Vec<Tab>,
    active: u64,
    next_key: u64,
}

impl TabList {
    pub fn new() -> Self {
        Self {
            tabs: Vec::new(),
            active: 0,
            next_key: 1,
        }
    }

    /// A key no tab of this window has had.
    pub fn next_key(&mut self) -> u64 {
        let key = self.next_key;
        self.next_key += 1;
        key
    }

    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub fn all(&self) -> &[Tab] {
        &self.tabs
    }

    pub fn all_mut(&mut self) -> &mut [Tab] {
        &mut self.tabs
    }

    pub fn active_key(&self) -> u64 {
        self.active
    }

    pub fn active(&self) -> Option<&Tab> {
        self.get(self.active)
    }

    pub fn active_mut(&mut self) -> Option<&mut Tab> {
        let key = self.active;
        self.get_mut(key)
    }

    pub fn get(&self, key: u64) -> Option<&Tab> {
        self.tabs.iter().find(|tab| tab.key == key)
    }

    pub fn get_mut(&mut self, key: u64) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|tab| tab.key == key)
    }

    pub fn by_session(&self, session: TabId) -> Option<&Tab> {
        self.tabs.iter().find(|tab| tab.session == Some(session))
    }

    pub fn by_session_mut(&mut self, session: TabId) -> Option<&mut Tab> {
        self.tabs
            .iter_mut()
            .find(|tab| tab.session == Some(session))
    }

    pub fn position(&self, key: u64) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.key == key)
    }

    /// How many tabs are pinned. They are the first ones.
    pub fn pinned_count(&self) -> usize {
        self.tabs.iter().take_while(|tab| tab.pinned).count()
    }

    /// Put `tab` in the list. It becomes the tab on show when `select` is set
    /// or when it is the first.
    pub fn insert(&mut self, tab: Tab, place: Place, select: bool) {
        let pinned = self.pinned_count();
        let after = match place {
            Place::End => self.tabs.len(),
            Place::AfterActive => self
                .position(self.active)
                .map_or(self.tabs.len(), |at| at + 1),
            Place::After(key) => self.position(key).map_or(self.tabs.len(), |at| at + 1),
        };
        // A tab that is not pinned never goes among the pinned ones, and a
        // pinned one never past them.
        let index = if tab.pinned {
            after.min(pinned)
        } else {
            after.max(pinned)
        };
        let key = tab.key;
        self.tabs.insert(index, tab);
        if select || self.tabs.len() == 1 {
            self.active = key;
        }
    }

    /// Show tab `key`. False when there is no such tab.
    pub fn select(&mut self, key: u64) -> bool {
        if self.get(key).is_some() {
            self.active = key;
            true
        } else {
            false
        }
    }

    /// Take tab `key` out. When it was on show, the tab that takes over is
    /// chosen as the module comment says. Returns the tab and its place.
    pub fn remove(&mut self, key: u64) -> Option<(Tab, usize)> {
        let index = self.position(key)?;
        let tab = self.tabs.remove(index);
        if self.active == key {
            let owner = tab.opener.filter(|opener| self.get(*opener).is_some());
            self.active = match owner {
                Some(owner) => owner,
                None if self.tabs.is_empty() => 0,
                None => self.tabs[index.min(self.tabs.len() - 1)].key,
            };
        }
        // The tabs this one opened are no longer anyone's.
        for other in &mut self.tabs {
            if other.opener == Some(key) {
                other.opener = None;
            }
        }
        Some((tab, index))
    }

    /// Move tab `key` so it is at place `to` once it has been taken out and
    /// put back: the strip's reorder. A pinned tab stays among the pinned
    /// ones and an unpinned tab among the others.
    pub fn move_to(&mut self, key: u64, to: usize) {
        let Some(from) = self.position(key) else {
            return;
        };
        let pinned = self.pinned_count();
        let is_pinned = self.tabs[from].pinned;
        let (low, high) = if is_pinned {
            (0, pinned - 1)
        } else {
            (pinned, self.tabs.len() - 1)
        };
        let to = to.clamp(low, high);
        if to == from {
            return;
        }
        let tab = self.tabs.remove(from);
        self.tabs.insert(to, tab);
    }

    /// Pin or unpin. A pinned tab joins the end of the pinned block; an
    /// unpinned one goes to the front of the others.
    pub fn set_pinned(&mut self, key: u64, pinned: bool) {
        let Some(from) = self.position(key) else {
            return;
        };
        if self.tabs[from].pinned == pinned {
            return;
        }
        let mut tab = self.tabs.remove(from);
        tab.pinned = pinned;
        let block = self.pinned_count();
        self.tabs.insert(block, tab);
    }

    /// The tab `by` places along from the one on show, wrapping at the ends.
    pub fn step(&self, by: isize) -> Option<u64> {
        let len = self.tabs.len() as isize;
        let at = self.position(self.active)? as isize;
        Some(self.tabs[(at + by).rem_euclid(len) as usize].key)
    }

    /// Tab `n` counted from 1, or `None` past the end.
    pub fn nth(&self, n: usize) -> Option<u64> {
        self.tabs.get(n.checked_sub(1)?).map(|tab| tab.key)
    }

    pub fn last(&self) -> Option<u64> {
        self.tabs.last().map(|tab| tab.key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tabs::TabKind;

    fn list(count: usize) -> TabList {
        let mut list = TabList::new();
        for _ in 0..count {
            let key = list.next_key();
            list.insert(Tab::blank(key, TabKind::Web), Place::End, false);
        }
        list
    }

    fn keys(list: &TabList) -> Vec<u64> {
        list.all().iter().map(|tab| tab.key).collect()
    }

    #[test]
    fn the_first_tab_is_on_show_and_keys_are_never_reused() {
        let mut list = list(3);
        assert_eq!(list.active_key(), 1);
        list.remove(3);
        assert_eq!(list.next_key(), 4);
    }

    #[test]
    fn a_new_tab_goes_after_the_one_on_show() {
        let mut list = list(3);
        list.select(1);
        let key = list.next_key();
        list.insert(Tab::blank(key, TabKind::Web), Place::AfterActive, true);
        assert_eq!(keys(&list), [1, 4, 2, 3]);
        assert_eq!(list.active_key(), 4);
    }

    #[test]
    fn closing_the_tab_on_show_goes_to_its_opener_else_its_right_neighbour_else_its_left() {
        let mut list = list(4);
        list.select(2);
        list.remove(2);
        assert_eq!(list.active_key(), 3, "the neighbour that takes its place");
        list.select(4);
        list.remove(4);
        assert_eq!(
            list.active_key(),
            3,
            "the last tab goes to its left neighbour"
        );
        let key = list.next_key();
        let mut child = Tab::blank(key, TabKind::Web);
        child.opener = Some(1);
        list.insert(child, Place::AfterActive, true);
        list.remove(key);
        assert_eq!(list.active_key(), 1, "back to the page that opened it");
    }

    #[test]
    fn closing_a_tab_that_is_not_on_show_leaves_the_selection() {
        let mut list = list(3);
        list.select(3);
        list.remove(1);
        assert_eq!(list.active_key(), 3);
        assert!(list.remove(99).is_none());
    }

    #[test]
    fn closing_the_last_tab_leaves_an_empty_list() {
        let mut list = list(1);
        list.remove(1);
        assert!(list.is_empty());
        assert_eq!(list.active_key(), 0);
        assert!(list.active().is_none());
        assert_eq!(list.step(1), None);
    }

    #[test]
    fn a_closed_tab_is_nobodys_opener() {
        let mut list = list(2);
        list.get_mut(2).unwrap().opener = Some(1);
        list.remove(1);
        assert_eq!(list.get(2).unwrap().opener, None);
    }

    #[test]
    fn pinned_tabs_are_first_and_stay_a_block() {
        let mut list = list(4);
        list.set_pinned(3, true);
        assert_eq!(keys(&list), [3, 1, 2, 4]);
        list.set_pinned(4, true);
        assert_eq!(
            keys(&list),
            [3, 4, 1, 2],
            "a newly pinned tab joins the end of the block"
        );
        assert_eq!(list.pinned_count(), 2);
        list.move_to(3, 3);
        assert_eq!(
            keys(&list),
            [4, 3, 1, 2],
            "a pinned tab cannot leave the block"
        );
        list.move_to(2, 0);
        assert_eq!(keys(&list), [4, 3, 2, 1], "an unpinned tab cannot enter it");
        list.set_pinned(4, false);
        assert_eq!(
            keys(&list),
            [3, 4, 2, 1],
            "an unpinned tab is the first of the others"
        );
    }

    #[test]
    fn a_new_tab_never_goes_among_the_pinned_ones() {
        let mut list = list(2);
        list.set_pinned(2, true);
        list.select(2);
        let key = list.next_key();
        list.insert(Tab::blank(key, TabKind::Web), Place::AfterActive, true);
        assert_eq!(keys(&list), [2, 3, 1]);
        let key = list.next_key();
        let mut pinned = Tab::blank(key, TabKind::Web);
        pinned.pinned = true;
        list.insert(pinned, Place::End, false);
        assert_eq!(
            keys(&list),
            [2, 4, 3, 1],
            "a pinned tab goes to the end of the block"
        );
    }

    #[test]
    fn reordering_is_a_remove_and_an_insert() {
        let mut list = list(4);
        list.move_to(1, 2);
        assert_eq!(keys(&list), [2, 3, 1, 4]);
        list.move_to(4, 0);
        assert_eq!(keys(&list), [4, 2, 3, 1]);
        list.move_to(4, 99);
        assert_eq!(keys(&list), [2, 3, 1, 4], "past the end is the end");
    }

    #[test]
    fn stepping_wraps_and_numbers_count_from_one() {
        let mut list = list(3);
        assert_eq!(list.step(-1), Some(3));
        list.select(3);
        assert_eq!(list.step(1), Some(1));
        assert_eq!(list.nth(2), Some(2));
        assert_eq!(list.nth(0), None);
        assert_eq!(list.nth(4), None);
        assert_eq!(list.last(), Some(3));
    }
}
