//! What is selected in a list or a table, and how a click or a key changes it.
//!
//! [`Selection`] holds the keys of the selected rows and the anchor a Shift
//! range grows from. It is the value a [`data_table`](crate::table::data_table)
//! or a [`list_view`](crate::list::list_view) reads and writes, so an app owns
//! it as `State<Selection<K>>` and sees every change. The keys are the rows'
//! own identity (an id, a path), never their place in the list, so a sort, a
//! filter or a reload keeps the selection on the same rows.
//!
//! [`Gesture`] names what the person did and [`Selection::apply`] carries it
//! out for a [`SelectionMode`]. It is one function for the pointer and the
//! keyboard, so a click with Shift and an arrow with Shift agree:
//!
//! | Gesture | Meaning |
//! | --- | --- |
//! | [`Gesture::Replace`] | A plain click or arrow: only this row is selected. |
//! | [`Gesture::Toggle`] | Ctrl+click or Space: this row joins or leaves. |
//! | [`Gesture::Extend`] | Shift+click or Shift+arrow: the rows from the anchor to this one. |
//! | [`Gesture::ExtendAdd`] | Ctrl+Shift: the same range, added to what is selected. |
//!
//! The set is behind an `Rc`, so handing a copy to every visible row costs
//! nothing and a change copies the set once.

use std::collections::HashSet;
use std::hash::Hash;
use std::rc::Rc;

use freya::prelude::Modifiers;

/// How many rows can be selected.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SelectionMode {
    /// Rows cannot be selected; a press only activates.
    None,
    /// One row at a time.
    Single,
    /// Any number, with Shift for a range and Ctrl for single rows.
    #[default]
    Multiple,
}

/// What the person did to the selection with a click or a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gesture {
    Replace,
    Toggle,
    Extend,
    ExtendAdd,
}

impl Gesture {
    /// The gesture a press or a key makes with `held` modifiers: Ctrl toggles,
    /// Shift extends, both add a range.
    pub fn from_modifiers(held: Modifiers) -> Self {
        let shift = held.contains(Modifiers::SHIFT);
        let ctrl = held.contains(Modifiers::CONTROL) || held.contains(Modifiers::META);
        match (shift, ctrl) {
            (true, true) => Gesture::ExtendAdd,
            (true, false) => Gesture::Extend,
            (false, true) => Gesture::Toggle,
            (false, false) => Gesture::Replace,
        }
    }
}

/// The selected rows' keys and the anchor of a range.
#[derive(Clone, Debug)]
pub struct Selection<K> {
    keys: Rc<HashSet<K>>,
    anchor: Option<K>,
}

impl<K> Default for Selection<K> {
    fn default() -> Self {
        Self {
            keys: Rc::new(HashSet::new()),
            anchor: None,
        }
    }
}

impl<K: Eq + Hash> PartialEq for Selection<K> {
    fn eq(&self, other: &Self) -> bool {
        (Rc::ptr_eq(&self.keys, &other.keys) || self.keys == other.keys)
            && self.anchor == other.anchor
    }
}

impl<K: Clone + Eq + Hash> Selection<K> {
    /// Nothing selected.
    pub fn new() -> Self {
        Self::default()
    }

    /// Exactly `keys` selected, the last as the anchor.
    pub fn of(keys: impl IntoIterator<Item = K>) -> Self {
        let mut anchor = None;
        let set: HashSet<K> = keys
            .into_iter()
            .inspect(|key| anchor = Some(key.clone()))
            .collect();
        Self {
            keys: Rc::new(set),
            anchor,
        }
    }

    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    pub fn contains(&self, key: &K) -> bool {
        self.keys.contains(key)
    }

    /// The selected keys, in no particular order. Sort them by their place in
    /// the list when order matters.
    pub fn iter(&self) -> impl Iterator<Item = &K> {
        self.keys.iter()
    }

    /// The row a Shift range grows from: the last one clicked.
    pub fn anchor(&self) -> Option<&K> {
        self.anchor.as_ref()
    }

    pub fn clear(&mut self) {
        if !self.keys.is_empty() {
            self.keys = Rc::new(HashSet::new());
        }
        self.anchor = None;
    }

    /// Selects only `key` and makes it the anchor.
    pub fn select_only(&mut self, key: K) {
        if self.keys.len() != 1 || !self.keys.contains(&key) {
            self.keys = Rc::new(HashSet::from([key.clone()]));
        }
        self.anchor = Some(key);
    }

    /// Adds `key` when it is not selected and removes it when it is; either
    /// way it becomes the anchor.
    pub fn toggle(&mut self, key: K) {
        let keys = Rc::make_mut(&mut self.keys);
        if !keys.remove(&key) {
            keys.insert(key.clone());
        }
        self.anchor = Some(key);
    }

    /// Selects every key in `keys`; the anchor stays where it was.
    pub fn select_all(&mut self, keys: impl IntoIterator<Item = K>) {
        self.keys = Rc::new(keys.into_iter().collect());
    }

    /// Selects `range`, in place of what was selected, or added to it when
    /// `additive`. The anchor stays where it was.
    pub fn select_range(&mut self, range: impl IntoIterator<Item = K>, additive: bool) {
        if additive {
            Rc::make_mut(&mut self.keys).extend(range);
        } else {
            self.keys = Rc::new(range.into_iter().collect());
        }
    }

    /// Drops every selected key for which `keep` is false, for rows that left
    /// the list. Returns whether any went.
    pub fn retain(&mut self, mut keep: impl FnMut(&K) -> bool) -> bool {
        if self.keys.iter().all(&mut keep) {
            return false;
        }
        Rc::make_mut(&mut self.keys).retain(|key| keep(key));
        if self
            .anchor
            .as_ref()
            .is_some_and(|anchor| !self.keys.contains(anchor))
        {
            self.anchor = None;
        }
        true
    }

    /// Carries out `gesture` on the row at place `at` of a list whose row `i`
    /// has the key `key_at(i)`. `anchor_at` is where the anchor is in the list
    /// now (it may have moved with a sort), or `None` when it is not there.
    /// Returns whether the selection changed.
    pub fn apply(
        &mut self,
        mode: SelectionMode,
        gesture: Gesture,
        at: usize,
        key_at: impl Fn(usize) -> Option<K>,
        anchor_at: Option<usize>,
    ) -> bool {
        let before = self.clone();
        let Some(key) = key_at(at) else {
            return false;
        };
        match (mode, gesture) {
            (SelectionMode::None, _) => {}
            (SelectionMode::Single, Gesture::Toggle) => {
                if self.contains(&key) {
                    self.clear();
                } else {
                    self.select_only(key);
                }
            }
            (SelectionMode::Single, _) => self.select_only(key),
            (SelectionMode::Multiple, Gesture::Replace) => self.select_only(key),
            (SelectionMode::Multiple, Gesture::Toggle) => self.toggle(key),
            (SelectionMode::Multiple, Gesture::Extend | Gesture::ExtendAdd) => {
                // With no anchor, a range starts at this row.
                let from = anchor_at.unwrap_or(at);
                let (first, last) = if from <= at { (from, at) } else { (at, from) };
                let range: Vec<K> = (first..=last).filter_map(&key_at).collect();
                let additive = gesture == Gesture::ExtendAdd;
                let anchor = self.anchor.clone().or(Some(key));
                self.select_range(range, additive);
                self.anchor = anchor;
            }
        }
        *self != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(index: usize) -> Option<u32> {
        (index < 10).then_some(index as u32 * 10)
    }

    fn sorted(selection: &Selection<u32>) -> Vec<u32> {
        let mut keys: Vec<u32> = selection.iter().copied().collect();
        keys.sort_unstable();
        keys
    }

    #[test]
    fn a_plain_click_selects_one_row_and_anchors_it() {
        let mut selection = Selection::new();
        selection.apply(SelectionMode::Multiple, Gesture::Replace, 2, key, None);
        selection.apply(SelectionMode::Multiple, Gesture::Replace, 4, key, Some(2));
        assert_eq!(sorted(&selection), [40]);
        assert_eq!(selection.anchor(), Some(&40));
    }

    #[test]
    fn shift_selects_from_the_anchor_in_either_direction() {
        let mut selection = Selection::new();
        selection.apply(SelectionMode::Multiple, Gesture::Replace, 5, key, None);
        selection.apply(SelectionMode::Multiple, Gesture::Extend, 7, key, Some(5));
        assert_eq!(sorted(&selection), [50, 60, 70]);
        // The anchor holds, so the range turns around it.
        selection.apply(SelectionMode::Multiple, Gesture::Extend, 3, key, Some(5));
        assert_eq!(sorted(&selection), [30, 40, 50]);
        assert_eq!(selection.anchor(), Some(&50));
    }

    #[test]
    fn ctrl_shift_adds_a_range_to_what_is_selected() {
        let mut selection = Selection::of([0_u32]);
        selection.apply(SelectionMode::Multiple, Gesture::Toggle, 4, key, Some(0));
        selection.apply(SelectionMode::Multiple, Gesture::ExtendAdd, 6, key, Some(4));
        assert_eq!(sorted(&selection), [0, 40, 50, 60]);
    }

    #[test]
    fn ctrl_toggles_one_row_without_touching_the_others() {
        let mut selection = Selection::new();
        selection.apply(SelectionMode::Multiple, Gesture::Replace, 1, key, None);
        selection.apply(SelectionMode::Multiple, Gesture::Toggle, 3, key, Some(1));
        assert_eq!(sorted(&selection), [10, 30]);
        selection.apply(SelectionMode::Multiple, Gesture::Toggle, 1, key, Some(3));
        assert_eq!(sorted(&selection), [30]);
    }

    #[test]
    fn single_mode_never_holds_more_than_one() {
        let mut selection = Selection::new();
        selection.apply(SelectionMode::Single, Gesture::Replace, 1, key, None);
        selection.apply(SelectionMode::Single, Gesture::Extend, 6, key, Some(1));
        assert_eq!(sorted(&selection), [60]);
        selection.apply(SelectionMode::Single, Gesture::Toggle, 6, key, Some(6));
        assert!(selection.is_empty());
    }

    #[test]
    fn none_mode_changes_nothing() {
        let mut selection = Selection::new();
        assert!(!selection.apply(SelectionMode::None, Gesture::Replace, 1, key, None));
        assert!(selection.is_empty());
    }

    #[test]
    fn a_gesture_that_changes_nothing_says_so() {
        let mut selection = Selection::of([20_u32]);
        assert!(!selection.apply(SelectionMode::Multiple, Gesture::Replace, 2, key, Some(2)));
    }

    #[test]
    fn rows_that_left_the_list_leave_the_selection() {
        let mut selection = Selection::of([10_u32, 20, 30]);
        assert!(selection.retain(|key| *key != 20));
        assert_eq!(sorted(&selection), [10, 30]);
        assert!(!selection.retain(|_| true));
    }

    #[test]
    fn modifiers_name_the_gesture() {
        assert_eq!(
            Gesture::from_modifiers(Modifiers::empty()),
            Gesture::Replace
        );
        assert_eq!(Gesture::from_modifiers(Modifiers::SHIFT), Gesture::Extend);
        assert_eq!(Gesture::from_modifiers(Modifiers::CONTROL), Gesture::Toggle);
        assert_eq!(
            Gesture::from_modifiers(Modifiers::SHIFT | Modifiers::CONTROL),
            Gesture::ExtendAdd
        );
    }
}
