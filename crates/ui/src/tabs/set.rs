//! The tabs of a window as plain data: which there are, which is on show, and
//! which were closed lately. No widget in here, so the rules a person feels
//! (where a new tab opens, who is selected after a close, what Ctrl+Shift+T
//! brings back, what a tab moved to another window keeps) are tested without a
//! window.
//!
//! A tab holds whatever the application shows in it, `P`: a [`TabContent`].
//! Files puts a folder and its Back and Forward lists in one
//! (`bingux_ui::files::FileTab`); an editor would put a document. The set
//! never looks inside it: it asks a tab's content for a copy
//! ([`TabContent::duplicate`]) and tells it when it leaves for another window
//! ([`TabContent::depart`]).

use crate::controls::tab_close::selection_after_close;

/// What a tab shows, as far as the set must know it.
pub trait TabContent: Clone + PartialEq + 'static {
    /// A copy that starts where this one is and goes on on its own: *Duplicate
    /// tab*. It must not share anything that changes with the original (a
    /// history, a scroll position).
    fn duplicate(&self) -> Self;

    /// The tab is being taken out of its window to be put in another's. The
    /// content may write down what its view knows, so the other window shows
    /// it as it was. Not called when a tab is closed.
    fn depart(&self) {}
}

/// One tab: a number that is never reused, and what it shows.
#[derive(Clone, PartialEq)]
pub struct Tab<P> {
    pub id: u64,
    pub content: P,
}

/// What closing a tab came to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Closed {
    /// The tab is gone, and these are the tabs now.
    Tab,
    /// It was the last one: the window is what closes. Nothing changed here.
    Window,
    /// There is no such tab.
    Nothing,
}

/// How many closed tabs a set remembers for *Reopen closed tab*.
pub const CLOSED_TABS_KEPT: usize = 10;

/// A tab that was closed: where it stood among its neighbours and what it
/// showed, so reopening it brings back its lists as well as its place.
#[derive(Clone)]
pub struct ClosedTab<P> {
    pub index: usize,
    pub content: P,
}

/// The tabs closed most recently, newest last: what *Reopen closed tab*
/// (Ctrl+Shift+T) takes from. It keeps the last [`CLOSED_TABS_KEPT`]; the
/// oldest is forgotten first.
#[derive(Clone)]
pub struct ClosedTabs<P> {
    stack: Vec<ClosedTab<P>>,
}

impl<P> Default for ClosedTabs<P> {
    fn default() -> Self {
        Self { stack: Vec::new() }
    }
}

impl<P> ClosedTabs<P> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, closed: ClosedTab<P>) {
        self.stack.push(closed);
        if self.stack.len() > CLOSED_TABS_KEPT {
            self.stack.remove(0);
        }
    }

    /// The tab closed last, which is no longer remembered.
    pub fn pop(&mut self) -> Option<ClosedTab<P>> {
        self.stack.pop()
    }

    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    pub fn len(&self) -> usize {
        self.stack.len()
    }
}

/// The tabs of a window.
pub struct TabSet<P> {
    tabs: Vec<Tab<P>>,
    active: u64,
    next: u64,
    closed: ClosedTabs<P>,
}

impl<P: TabContent> TabSet<P> {
    /// A window with one tab.
    pub fn new(first: P) -> Self {
        Self {
            tabs: vec![Tab {
                id: 1,
                content: first,
            }],
            active: 1,
            next: 2,
            closed: ClosedTabs::new(),
        }
    }

    pub fn tabs(&self) -> &[Tab<P>] {
        &self.tabs
    }

    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub fn active(&self) -> u64 {
        self.active
    }

    /// The tab on show, or the first if the active one is somehow gone.
    pub fn current(&self) -> Option<&Tab<P>> {
        self.tabs
            .iter()
            .find(|tab| tab.id == self.active)
            .or_else(|| self.tabs.first())
    }

    /// The content of tab `id`.
    pub fn content(&self, id: u64) -> Option<&P> {
        self.tabs
            .iter()
            .find(|tab| tab.id == id)
            .map(|tab| &tab.content)
    }

    pub fn ids(&self) -> Vec<u64> {
        self.tabs.iter().map(|tab| tab.id).collect()
    }

    /// Whether *Reopen closed tab* has a tab to bring back.
    pub fn can_reopen(&self) -> bool {
        !self.closed.is_empty()
    }

    fn position(&self, id: u64) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.id == id)
    }

    /// Put a tab at `index` (or at the end) and go to it. Returns its id.
    fn insert(&mut self, index: usize, content: P) -> u64 {
        let id = self.next;
        self.next += 1;
        let index = index.min(self.tabs.len());
        self.tabs.insert(index, Tab { id, content });
        self.active = id;
        id
    }

    /// Open `content` in a new tab beside the one on show and go to it.
    pub fn open(&mut self, content: P) -> u64 {
        let after = self
            .position(self.active)
            .map_or(usize::MAX, |index| index + 1);
        self.insert(after, content)
    }

    /// Open a copy of tab `id` beside it (see [`TabContent::duplicate`]) and
    /// go to it.
    pub fn duplicate(&mut self, id: u64) -> Option<u64> {
        let index = self.position(id)?;
        let copy = self.tabs[index].content.duplicate();
        Some(self.insert(index + 1, copy))
    }

    pub fn select(&mut self, id: u64) {
        if self.active != id && self.position(id).is_some() {
            self.active = id;
        }
    }

    /// Close tab `id`. The tab that takes the selection is the neighbour to
    /// the right, else the one to the left. The last tab is not closed here:
    /// it is the window that closes, and the host decides how.
    pub fn close(&mut self, id: u64) -> Closed {
        let Some(index) = self.position(id) else {
            return Closed::Nothing;
        };
        if self.tabs.len() == 1 {
            return Closed::Window;
        }
        let next = selection_after_close(&self.ids(), &[id], self.active);
        let gone = self.tabs.remove(index);
        self.closed.push(ClosedTab {
            index,
            content: gone.content,
        });
        if let Some(next) = next {
            self.active = next;
        }
        Closed::Tab
    }

    /// Close every tab in `ids` but never the last one standing: a menu's
    /// *Close other tabs* keeps the tab it was opened on. Each is remembered
    /// at its place, the rightmost first, so Reopen brings them back left to
    /// right.
    pub fn close_many(&mut self, ids: &[u64]) {
        let all = self.ids();
        let mut ids: Vec<u64> = ids.iter().copied().filter(|id| all.contains(id)).collect();
        if ids.len() >= all.len() {
            // Never everything: the active tab stays.
            ids.retain(|id| *id != self.active);
        }
        if ids.is_empty() {
            return;
        }
        let next = selection_after_close(&all, &ids, self.active);
        for id in all.iter().rev().filter(|id| ids.contains(id)) {
            if let Some(index) = self.position(*id) {
                let gone = self.tabs.remove(index);
                // The place it had before any of them went.
                let place = all.iter().position(|other| other == id).unwrap_or(index);
                self.closed.push(ClosedTab {
                    index: place,
                    content: gone.content,
                });
            }
        }
        if let Some(next) = next {
            self.active = next;
        }
    }

    /// Bring back the tab closed last, at its place. Returns its new id.
    pub fn reopen(&mut self) -> Option<u64> {
        let tab = self.closed.pop()?;
        Some(self.insert(tab.index, tab.content))
    }

    /// Go `by` tabs along, wrapping at either end.
    pub fn step(&mut self, by: isize) {
        let len = self.tabs.len() as isize;
        let Some(index) = self.position(self.active) else {
            return;
        };
        self.active = self.tabs[(index as isize + by).rem_euclid(len) as usize].id;
    }

    /// Go to the tab at `index` from the start, or the last for `usize::MAX`.
    pub fn nth(&mut self, index: usize) {
        if let Some(tab) = self.tabs.get(index).or_else(|| self.tabs.last()) {
            self.active = tab.id;
        }
    }

    /// A tab was dragged (or moved by its menu) to place `to` in the list,
    /// as if it were removed and inserted there. The active tab stays.
    pub fn reorder(&mut self, id: u64, to: usize) {
        let Some(from) = self.position(id) else {
            return;
        };
        let tab = self.tabs.remove(from);
        let to = to.min(self.tabs.len());
        self.tabs.insert(to, tab);
    }

    /// Change what tab `id` shows, in place: a tab that went to another
    /// folder, a document that was renamed. `edit` says whether it changed
    /// anything; a host asks [`TabSet::content`] first, so an unchanged tab
    /// does not redraw.
    pub fn update(&mut self, id: u64, edit: impl FnOnce(&mut P)) {
        if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == id) {
            edit(&mut tab.content);
        }
    }
}

/// A tab on its way from one window to another: what it shows. It has no
/// number: the window that takes it gives it one.
#[derive(Clone, PartialEq)]
pub struct Moved<P> {
    pub content: P,
}

impl<P: TabContent> TabSet<P> {
    /// A window made of tabs that came from another one, with `on_show` (an
    /// index into `tabs`) selected. `None` when there are none.
    pub fn adopt(tabs: Vec<Moved<P>>, on_show: usize) -> Option<Self> {
        let mut tabs = tabs.into_iter();
        let mut set = Self::new(tabs.next()?.content);
        for moved in tabs {
            set.insert(usize::MAX, moved.content);
        }
        let on_show = on_show.min(set.tabs.len() - 1);
        set.active = set.tabs[on_show].id;
        Some(set)
    }

    /// Take tab `id` out of the set to put it in another window's. Its
    /// content departs (see [`TabContent::depart`]) and is not kept: nothing
    /// remembers the tab here, so it cannot be reopened from this window. The
    /// tab that takes the selection is the one a close would give it. The
    /// last tab is not taken: it is the window that moves.
    pub fn detach(&mut self, id: u64) -> Option<Moved<P>> {
        let index = self.position(id)?;
        if self.tabs.len() == 1 {
            return None;
        }
        let next = selection_after_close(&self.ids(), &[id], self.active);
        let gone = self.tabs.remove(index);
        if let Some(next) = next {
            self.active = next;
        }
        gone.content.depart();
        Some(Moved {
            content: gone.content,
        })
    }

    /// Take every tab but `keep` out, in order, for a window of their own,
    /// with the index among them of the one that was on show (the nearest to
    /// `keep`, on its right, when it was `keep`). `keep` stays, selected.
    pub fn detach_others(&mut self, keep: u64) -> Option<(Vec<Moved<P>>, usize)> {
        self.position(keep)?;
        if self.tabs.len() == 1 {
            return None;
        }
        let ids = self.ids();
        let shown =
            selection_after_close(&ids, &[keep], self.active).filter(|shown| *shown != keep);
        let mut moved = Vec::with_capacity(ids.len() - 1);
        let mut on_show = 0;
        for id in ids.into_iter().filter(|id| *id != keep) {
            if Some(id) == shown {
                on_show = moved.len();
            }
            if let Some(tab) = self.detach_one(id) {
                moved.push(tab);
            }
        }
        self.active = keep;
        Some((moved, on_show))
    }

    /// Take every tab out, in order: the window is about to close and they
    /// go to another. The set is left with one tab it must not show.
    pub fn detach_all(&mut self) -> Vec<Moved<P>> {
        let ids = self.ids();
        let mut moved = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(tab) = self.detach_one(id) {
                moved.push(tab);
            }
        }
        moved
    }

    /// Remove `id` whatever is left, and let its content depart.
    fn detach_one(&mut self, id: u64) -> Option<Moved<P>> {
        let index = self.position(id)?;
        let gone = self.tabs.remove(index);
        gone.content.depart();
        Some(Moved {
            content: gone.content,
        })
    }

    /// Put tabs from another window in at `index` (the end, past it), in
    /// order, and go to the first. Returns the new ids.
    pub fn insert_moved(&mut self, index: usize, tabs: Vec<Moved<P>>) -> Vec<u64> {
        let mut ids = Vec::with_capacity(tabs.len());
        for (offset, moved) in tabs.into_iter().enumerate() {
            ids.push(self.insert(
                index.saturating_add(offset).min(self.tabs.len()),
                moved.content,
            ));
        }
        if let Some(first) = ids.first() {
            self.active = *first;
        }
        ids
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use crate::controls::tab_close;

    use super::*;

    /// A document: a name, and an identity that is what the tab *is* (two
    /// tabs on the same name are two documents, as two Files tabs on one
    /// folder are two histories).
    #[derive(Clone)]
    struct Page {
        name: String,
        identity: Rc<Cell<u32>>,
        departed: Rc<Cell<u32>>,
    }

    impl PartialEq for Page {
        fn eq(&self, other: &Self) -> bool {
            Rc::ptr_eq(&self.identity, &other.identity)
        }
    }

    impl TabContent for Page {
        fn duplicate(&self) -> Self {
            page(&self.name)
        }

        fn depart(&self) {
            self.departed.set(self.departed.get() + 1);
        }
    }

    fn page(name: &str) -> Page {
        Page {
            name: name.into(),
            identity: Rc::default(),
            departed: Rc::default(),
        }
    }

    /// A set with tabs a, b, c, d (ids 1..=4), `d` on show.
    fn four() -> TabSet<Page> {
        let mut set = TabSet::new(page("a"));
        for letter in ["b", "c", "d"] {
            set.open(page(letter));
        }
        set
    }

    fn names(set: &TabSet<Page>) -> Vec<String> {
        set.tabs()
            .iter()
            .map(|tab| tab.content.name.clone())
            .collect()
    }

    fn on_show(set: &TabSet<Page>) -> String {
        set.current().expect("a tab on show").content.name.clone()
    }

    #[test]
    fn a_new_tab_opens_beside_the_one_on_show_and_is_selected() {
        let mut set = four();
        set.select(2);
        let id = set.open(page("x"));
        assert_eq!(names(&set), ["a", "b", "x", "c", "d"]);
        assert_eq!(set.active(), id);
        assert_eq!(set.ids(), [1, 2, 5, 3, 4], "ids are never reused");
    }

    #[test]
    fn closing_the_selected_tab_selects_its_right_neighbour_else_its_left() {
        let mut set = four();
        set.select(2);
        assert_eq!(set.close(2), Closed::Tab);
        assert_eq!(on_show(&set), "c", "the right neighbour");
        set.select(4);
        assert_eq!(set.close(4), Closed::Tab);
        assert_eq!(on_show(&set), "c", "the last tab has only a left neighbour");
        assert_eq!(names(&set), ["a", "c"]);
    }

    #[test]
    fn closing_another_tab_keeps_the_selection() {
        let mut set = four();
        set.select(3);
        set.close(1);
        set.close(4);
        assert_eq!(on_show(&set), "c");
        assert_eq!(names(&set), ["b", "c"]);
    }

    #[test]
    fn closing_the_last_tab_is_the_windows_business() {
        let mut set = TabSet::new(page("a"));
        assert_eq!(
            set.close(1),
            Closed::Window,
            "the window closes, the tab is left alone"
        );
        assert_eq!(set.len(), 1);
        assert!(
            !set.can_reopen(),
            "a window that is closing remembers nothing"
        );
        assert_eq!(set.close(99), Closed::Nothing, "a tab that is not there");
    }

    #[test]
    fn closing_all_the_others_keeps_one_tab_and_it_is_selected() {
        let mut set = four();
        let others = tab_close::others(&set.ids(), 2);
        set.close_many(&others);
        assert_eq!(names(&set), ["b"]);
        assert_eq!(set.active(), 2);
    }

    #[test]
    fn close_to_the_right_selects_the_tab_it_was_opened_on_when_the_selection_went() {
        let mut set = four();
        set.select(4);
        let right = tab_close::to_the_right(&set.ids(), 2);
        set.close_many(&right);
        assert_eq!(names(&set), ["a", "b"]);
        assert_eq!(set.active(), 2, "the nearest tab that stayed");
        let mut set = four();
        set.select(1);
        let right = tab_close::to_the_right(&set.ids(), 2);
        set.close_many(&right);
        assert_eq!(set.active(), 1, "a selection that stays stays");
    }

    #[test]
    fn close_to_the_left_selects_the_tab_it_was_opened_on_when_the_selection_went() {
        let mut set = four();
        set.select(1);
        let left = tab_close::to_the_left(&set.ids(), 3);
        set.close_many(&left);
        assert_eq!(names(&set), ["c", "d"]);
        assert_eq!(set.active(), 3, "the nearest tab to the right that stayed");
        let mut set = four();
        set.select(4);
        let left = tab_close::to_the_left(&set.ids(), 3);
        set.close_many(&left);
        assert_eq!(set.active(), 4, "a selection that stays stays");
        // Nothing is left of the first tab.
        let mut set = four();
        let none = tab_close::to_the_left(&set.ids(), 1);
        set.close_many(&none);
        assert_eq!(set.len(), 4);
    }

    #[test]
    fn closing_the_tabs_to_the_left_reopens_them_left_to_right_at_their_places() {
        let mut set = four();
        let left = tab_close::to_the_left(&set.ids(), 3);
        set.close_many(&left);
        assert_eq!(names(&set), ["c", "d"]);
        assert!(set.can_reopen());
        let first = set.reopen().expect("a comes back first");
        assert_eq!(names(&set), ["a", "c", "d"], "at the start, where it was");
        assert_eq!(set.active(), first);
        set.reopen().expect("then b");
        assert_eq!(
            names(&set),
            ["a", "b", "c", "d"],
            "between a and c, where it was"
        );
        assert!(set.reopen().is_none(), "and nothing more");
    }

    #[test]
    fn closing_many_never_closes_the_last_tab() {
        let mut set = four();
        set.select(3);
        set.close_many(&[1, 2, 3, 4]);
        assert_eq!(set.len(), 1);
        assert_eq!(on_show(&set), "c", "the tab on show stays");
    }

    #[test]
    fn reopen_brings_back_the_last_closed_tab_at_its_content_and_place() {
        let mut set = four();
        set.select(2);
        set.close(2);
        set.close(4);
        assert_eq!(names(&set), ["a", "c"]);
        assert!(set.can_reopen());
        let id = set.reopen().expect("d comes back");
        assert_eq!(names(&set), ["a", "c", "d"], "at the end, where it was");
        assert_eq!(set.active(), id, "and it is selected");
        set.reopen().expect("then b");
        assert_eq!(
            names(&set),
            ["a", "b", "c", "d"],
            "between a and c, where it was"
        );
        assert_eq!(on_show(&set), "b");
        assert!(set.reopen().is_none(), "nothing more to reopen");
        assert!(!set.can_reopen());
    }

    #[test]
    fn reopened_tabs_get_new_ids_and_keep_their_content() {
        let mut set = four();
        let before = set.tabs()[3].content.clone();
        set.close(4);
        let id = set.reopen().unwrap();
        assert!(id > 4, "an id is never reused");
        assert!(
            set.current().unwrap().content == before,
            "what the tab held comes back with it"
        );
    }

    #[test]
    fn closing_several_reopens_them_left_to_right_at_their_places() {
        let mut set = four();
        let others = tab_close::others(&set.ids(), 2);
        set.close_many(&others);
        assert_eq!(names(&set), ["b"]);
        for _ in 0..3 {
            set.reopen().unwrap();
        }
        assert_eq!(names(&set), ["a", "b", "c", "d"]);
    }

    fn closed(index: usize, name: &str) -> ClosedTab<Page> {
        ClosedTab {
            index,
            content: page(name),
        }
    }

    #[test]
    fn the_last_closed_tab_comes_back_first_at_its_place() {
        let mut stack = ClosedTabs::new();
        assert!(stack.is_empty());
        assert!(stack.pop().is_none(), "nothing to reopen");
        stack.push(closed(0, "/a"));
        stack.push(closed(2, "/b"));
        assert_eq!(stack.len(), 2);
        let last = stack.pop().expect("the last closed tab");
        assert_eq!((last.index, last.content.name.as_str()), (2, "/b"));
        let first = stack.pop().expect("then the one before");
        assert_eq!((first.index, first.content.name.as_str()), (0, "/a"));
        assert!(stack.is_empty());
    }

    #[test]
    fn the_closed_stack_keeps_only_the_most_recent_ones() {
        let mut stack = ClosedTabs::new();
        for n in 0..CLOSED_TABS_KEPT + 3 {
            stack.push(closed(n, &format!("/f{n}")));
        }
        assert_eq!(stack.len(), CLOSED_TABS_KEPT);
        let newest = stack.pop().unwrap();
        assert_eq!(newest.content.name, format!("/f{}", CLOSED_TABS_KEPT + 2));
        let mut oldest = newest;
        while let Some(tab) = stack.pop() {
            oldest = tab;
        }
        assert_eq!(
            oldest.content.name, "/f3",
            "the three oldest were forgotten"
        );
    }

    #[test]
    fn a_reopened_tab_keeps_its_own_content() {
        let mut stack = ClosedTabs::new();
        let tab = closed(1, "/a");
        let content = tab.content.clone();
        stack.push(tab);
        let back = stack.pop().unwrap();
        assert!(
            back.content == content,
            "the same tab comes back, not a new one"
        );
    }

    #[test]
    fn the_stack_is_short() {
        let mut set = TabSet::new(page("a"));
        for n in 0..CLOSED_TABS_KEPT + 5 {
            set.open(page(&format!("t{n}")));
        }
        for id in set.ids().into_iter().skip(1) {
            set.close(id);
        }
        let mut brought_back = 0;
        while set.reopen().is_some() {
            brought_back += 1;
        }
        assert_eq!(brought_back, CLOSED_TABS_KEPT);
    }

    #[test]
    fn a_duplicate_opens_beside_its_tab_and_starts_where_it_is() {
        let mut set = four();
        let id = set.duplicate(2).expect("b has a duplicate");
        assert_eq!(names(&set), ["a", "b", "b", "c", "d"]);
        assert_eq!(set.active(), id, "the copy is selected");
        assert!(
            set.tabs()[1].content != set.tabs()[2].content,
            "with a life of its own"
        );
        assert!(set.duplicate(99).is_none());
    }

    #[test]
    fn tabs_step_and_jump() {
        let mut set = four();
        set.step(1);
        assert_eq!(on_show(&set), "a", "wraps past the end");
        set.step(-1);
        assert_eq!(on_show(&set), "d", "and back");
        set.nth(1);
        assert_eq!(on_show(&set), "b");
        set.nth(usize::MAX);
        assert_eq!(on_show(&set), "d", "Alt+9 is the last");
    }

    #[test]
    fn reordering_moves_a_tab_and_leaves_the_selection() {
        let mut set = four();
        set.select(1);
        set.reorder(1, 3);
        assert_eq!(names(&set), ["b", "c", "d", "a"]);
        assert_eq!(set.active(), 1);
    }

    #[test]
    fn a_tab_is_updated_in_place() {
        let mut set = four();
        assert_eq!(set.content(2).map(|page| page.name.as_str()), Some("b"));
        set.update(2, |page| page.name = "z".into());
        assert_eq!(names(&set), ["a", "z", "c", "d"]);
        set.update(99, |page| page.name = "never".into());
        assert_eq!(
            names(&set),
            ["a", "z", "c", "d"],
            "a tab that is not there is left alone"
        );
        assert!(set.content(99).is_none());
    }

    // ---- Moving tabs between windows ----------------------------------------

    fn identities(set: &TabSet<Page>) -> Vec<Page> {
        set.tabs().iter().map(|tab| tab.content.clone()).collect()
    }

    /// How many tabs of `windows` hold exactly this content.
    fn holders(windows: &[&TabSet<Page>], content: &Page) -> usize {
        windows
            .iter()
            .flat_map(|set| set.tabs())
            .filter(|tab| tab.content == *content)
            .count()
    }

    #[test]
    fn a_moved_tab_keeps_its_content_and_is_in_one_window_at_every_step() {
        let mut from = four();
        let mut to = TabSet::new(page("x"));
        let content = from.tabs()[1].content.clone();
        assert_eq!(holders(&[&from, &to], &content), 1);
        let moved = from.detach(2).expect("a tab to move");
        assert!(
            moved.content == content,
            "the tab's content is moved, not made again"
        );
        assert_eq!(content.departed.get(), 1, "and it was told it was leaving");
        assert_eq!(
            holders(&[&from, &to], &content),
            0,
            "in flight it is in neither window"
        );
        to.insert_moved(1, vec![moved]);
        assert_eq!(holders(&[&from, &to], &content), 1, "and then in one only");
        assert_eq!(names(&from), ["a", "c", "d"]);
        assert_eq!(names(&to), ["x", "b"]);
        assert_eq!(
            on_show(&to),
            "b",
            "the moved tab is on show in the window it joined"
        );
        assert!(!from.can_reopen(), "a moved tab is not a closed one");
    }

    #[test]
    fn a_closed_tab_is_not_told_it_departs() {
        let mut set = four();
        let content = set.tabs()[1].content.clone();
        set.close(2);
        assert_eq!(content.departed.get(), 0);
    }

    #[test]
    fn the_window_a_tab_leaves_selects_as_a_close_would() {
        let mut set = four();
        set.select(2);
        set.detach(2);
        assert_eq!(on_show(&set), "c", "the right neighbour takes it");
        set.select(4);
        set.detach(4);
        assert_eq!(
            on_show(&set),
            "c",
            "the left one when there is none on the right"
        );
        set.select(1);
        set.detach(3);
        assert_eq!(on_show(&set), "a", "a tab that stays on show stays on show");
    }

    #[test]
    fn the_last_tab_of_a_window_is_not_taken_out_of_it() {
        let mut set = TabSet::new(page("a"));
        assert!(set.detach(1).is_none(), "it is the window that moves");
        assert_eq!(set.len(), 1);
        assert!(set.detach_others(1).is_none());
        assert!(set.detach(99).is_none(), "a tab that is not there");
    }

    #[test]
    fn tearing_one_tab_off_a_window_leaves_it_alone_and_the_rest_in_order() {
        let mut set = four();
        set.select(3);
        let all = identities(&set);
        let (others, on_show_index) = set.detach_others(3).expect("others to move");
        assert_eq!(names(&set), ["c"]);
        assert_eq!(set.active(), 3);
        assert_eq!(
            others
                .iter()
                .map(|moved| moved.content.name.clone())
                .collect::<Vec<_>>(),
            ["a", "b", "d"]
        );
        assert_eq!(
            on_show_index, 2,
            "the selection was `c`, so the nearest tab on its right takes it: `d`"
        );
        let mut window = TabSet::adopt(others, on_show_index).expect("a window of them");
        assert_eq!(on_show(&window), "d");
        assert_eq!(names(&window), ["a", "b", "d"]);
        for content in &all {
            assert_eq!(
                holders(&[&set, &window], content),
                1,
                "every tab is in one window"
            );
        }
        // What was on show in the other window stays on show there.
        let mut set = four();
        set.select(1);
        let (others, shown) = set.detach_others(3).unwrap();
        window = TabSet::adopt(others, shown).unwrap();
        assert_eq!(on_show(&window), "a");
        // And with the last tab torn off, the one left of it is the nearest.
        let mut set = four();
        set.select(4);
        let (others, shown) = set.detach_others(4).unwrap();
        assert_eq!(
            others
                .iter()
                .map(|moved| moved.content.name.clone())
                .collect::<Vec<_>>(),
            ["a", "b", "c"]
        );
        assert_eq!(on_show(&TabSet::adopt(others, shown).unwrap()), "c");
        assert!(
            TabSet::<Page>::adopt(Vec::new(), 0).is_none(),
            "no tabs, no window"
        );
    }

    #[test]
    fn a_window_dropped_on_another_brings_all_its_tabs_in_order_at_the_marker() {
        let mut dragged = TabSet::new(page("p"));
        dragged.open(page("q"));
        let mut target = four();
        let before: Vec<Page> = identities(&dragged)
            .into_iter()
            .chain(identities(&target))
            .collect();
        let moved = dragged.detach_all();
        assert_eq!(moved.len(), 2);
        let ids = target.insert_moved(2, moved);
        assert_eq!(names(&target), ["a", "b", "p", "q", "c", "d"]);
        assert_eq!(target.active(), ids[0], "the first of them is on show");
        for content in &before {
            assert_eq!(holders(&[&target], content), 1, "no tab lost or duplicated");
        }
        assert_eq!(target.ids().len(), 6);
        let mut sorted = target.ids();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 6, "every tab has a number of its own");
        // Dropped past the end, they go at the end.
        let mut target = four();
        target.insert_moved(99, vec![Moved { content: page("z") }]);
        assert_eq!(names(&target), ["a", "b", "c", "d", "z"]);
    }

    #[test]
    fn tabs_torn_off_and_dropped_back_end_where_they_began_with_their_own_content() {
        let mut window = four();
        window.select(2);
        let all = identities(&window);
        let (others, shown) = window.detach_others(2).unwrap();
        let mut other = TabSet::adopt(others, shown).unwrap();
        // The torn-off tab goes back to the window it left, at its old place.
        let moved = window.detach_all();
        other.insert_moved(1, moved);
        assert_eq!(names(&other), ["a", "b", "c", "d"]);
        for content in &all {
            assert_eq!(holders(&[&other], content), 1);
        }
        assert!(
            identities(&other) == all,
            "the very same content, in the order it had"
        );
    }

    /// A long walk of tab moves between two windows, closes and opens, checking
    /// after every step that each live tab is in exactly one window.
    #[test]
    fn no_tab_is_lost_or_duplicated_by_any_sequence_of_moves() {
        let mut seed = 0x2545_f491_4f6c_dd1d_u64;
        let mut random = move |below: usize| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % below as u64) as usize
        };
        let mut a = four();
        let mut b = TabSet::new(page("m"));
        let mut live: Vec<Page> = identities(&a).into_iter().chain(identities(&b)).collect();
        for step in 0..3000 {
            let (from, to) = if random(2) == 0 {
                (&mut a, &mut b)
            } else {
                (&mut b, &mut a)
            };
            match random(7) {
                0 | 1 => {
                    let ids = from.ids();
                    let id = ids[random(ids.len())];
                    if let Some(moved) = from.detach(id) {
                        let at = random(to.len() + 2);
                        to.insert_moved(at, vec![moved]);
                    }
                }
                2 => {
                    let ids = from.ids();
                    let id = ids[random(ids.len())];
                    if let Some((others, shown)) = from.detach_others(id) {
                        // They open a window of their own: the target stands for it.
                        let adopted = TabSet::adopt(others, shown).expect("tabs");
                        for tab in adopted.tabs() {
                            to.insert_moved(
                                usize::MAX,
                                vec![Moved {
                                    content: tab.content.clone(),
                                }],
                            );
                        }
                    }
                }
                3 => {
                    if from.len() > 1 {
                        let moved = from.detach_all();
                        to.insert_moved(random(to.len() + 1), moved);
                        // A window closes when it is emptied: it starts over with a tab.
                        let fresh = TabSet::new(page("n"));
                        live.push(fresh.tabs()[0].content.clone());
                        *from = fresh;
                    }
                }
                4 => {
                    from.open(page(&format!("t{step}")));
                    live.push(
                        from.tabs()
                            .iter()
                            .find(|tab| tab.id == from.active())
                            .unwrap()
                            .content
                            .clone(),
                    );
                }
                5 => {
                    let ids = from.ids();
                    let id = ids[random(ids.len())];
                    if from.len() > 1 {
                        let content = from
                            .tabs()
                            .iter()
                            .find(|tab| tab.id == id)
                            .unwrap()
                            .content
                            .clone();
                        from.close(id);
                        live.retain(|other| *other != content);
                    }
                }
                _ => {
                    let ids = from.ids();
                    let id = ids[random(ids.len())];
                    from.reorder(id, random(ids.len()));
                }
            }
            // The window that was emptied by `3` was replaced; its old tabs are in `to`.
            for content in &live {
                assert!(
                    holders(&[&a, &b], content) == 1,
                    "step {step}: a tab is in {} windows",
                    holders(&[&a, &b], content)
                );
            }
            assert_eq!(
                a.len() + b.len(),
                live.len(),
                "step {step}: a tab was lost or made up"
            );
            for set in [&a, &b] {
                let mut ids = set.ids();
                ids.sort_unstable();
                ids.dedup();
                assert_eq!(ids.len(), set.len(), "step {step}: two tabs share a number");
                assert!(
                    set.current().is_some(),
                    "step {step}: a window with no tab on show"
                );
            }
        }
    }
}
