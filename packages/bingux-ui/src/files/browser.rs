//! The file view as an application's content, not a dialog.
//!
//! [`browse`] is the chooser's body (toolbar, places sidebar, address bar,
//! file view, status pill) hosted in a window. It is the same component, so a
//! fix to one is a fix to the other. What is absent is what only a question
//! has: Cancel and the accept button, the file type filter, the name field and
//! the request's choices. What differs is what a person means by opening:
//! a file opens in its default application, and a folder can open in another
//! tab.
//!
//! The host (the Files application) owns the tabs. Each tab gives its body a
//! [`TabMemory`], where the body keeps the tab's [`NavModel`] (the folder
//! shown and the Back and Forward history, on [`folder_table`]), so a tab
//! that is hidden and shown again, or restored from a session, is where the
//! person left it.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use freya::prelude::*;

use super::chooser::browse_body;
use super::directory::Where;
use super::folder_table::{folder_table, location_of};
use crate::nav::NavModel;

/// The number the next [`TabMemory`] takes.
static NEXT: AtomicU64 = AtomicU64::new(1);

/// A tab's navigation history, shared between the host and the body that
/// shows it.
#[derive(Clone)]
pub struct TabMemory {
    id: u64,
    model: Rc<RefCell<NavModel>>,
    away: Rc<Away>,
}

/// What a tab keeps for a move to another window.
#[derive(Default)]
struct Away {
    /// Reads the states of the body that shows the tab now.
    reader: RefCell<Option<Box<dyn Fn() -> Box<dyn std::any::Any>>>>,
    /// What the window the tab left read from its states, until the window
    /// that has the tab takes it.
    view: RefCell<Option<Box<dyn std::any::Any>>>,
    /// The window that shows the tab holds this alive through its memory. A
    /// move puts a new one in its place, so the old window sees it end.
    resident: RefCell<Rc<()>>,
    /// A file the first view of the tab selects, once; see
    /// [`TabMemory::select_on_open`].
    select: RefCell<Option<PathBuf>>,
}

impl TabMemory {
    pub fn new(at: Where) -> Self {
        let model = NavModel::new(Arc::new(folder_table()), &location_of(&at));
        Self {
            id: NEXT.fetch_add(1, Ordering::Relaxed),
            model: Rc::new(RefCell::new(model)),
            away: Rc::default(),
        }
    }

    /// A new tab's memory that starts where this one is, with its own copy
    /// of the Back and Forward lists: what *Duplicate tab* gives.
    pub fn duplicate(&self) -> Self {
        Self {
            id: NEXT.fetch_add(1, Ordering::Relaxed),
            model: Rc::new(RefCell::new(self.model())),
            away: Rc::default(),
        }
    }

    /// Whether the host still has this tab *in this window*: the handle goes
    /// dead when it drops the tab's memory, or when the tab leaves for another
    /// window ([`TabMemory::depart`]). The body keeps the states of every tab
    /// it has shown, and lets go of a closed or departed tab's through this.
    pub(super) fn downgrade(&self) -> std::rc::Weak<()> {
        Rc::downgrade(&self.away.resident.borrow())
    }

    /// The tab is leaving its window for another, and the host is about to
    /// give the same memory to that window. Records where its view was
    /// scrolled and what it had selected, takes a snapshot of the states its
    /// body keeps for it (what is selected, how it is sorted and zoomed, the
    /// search in the box), and ends its residence here, so this window lets go
    /// of the states it kept. The window that takes the tab shows it from the
    /// snapshot, once: nothing is copied, so the tab is in one window at a
    /// time. The history and the scroll memory move by being the same memory.
    pub fn depart(&self) {
        super::scroll_memory::flush(self.id);
        let snapshot = self.away.reader.borrow_mut().take().map(|read| read());
        if let Some(snapshot) = snapshot {
            *self.away.view.borrow_mut() = Some(snapshot);
        }
        // A new residence: the old one is dropped here, and the window that
        // had the tab sees it end.
        *self.away.resident.borrow_mut() = Rc::new(());
    }

    /// Say how to snapshot this tab's states, for [`TabMemory::depart`]. The
    /// body that shows the tab calls it when it creates the states.
    pub(super) fn publish(&self, read: impl Fn() -> Box<dyn std::any::Any> + 'static) {
        *self.away.reader.borrow_mut() = Some(Box::new(read));
    }

    /// The snapshot a window the tab left made, once: the window that shows
    /// the tab next starts from it.
    pub(super) fn take_view<T: 'static>(&self) -> Option<T> {
        let snapshot = self.away.view.borrow_mut().take()?;
        snapshot.downcast::<T>().ok().map(|snapshot| *snapshot)
    }

    /// A copy of the tab's history now. A host with no body to show the tab
    /// (the pure tests) reads and writes it with [`TabMemory::model`] and
    /// [`TabMemory::set`] and the rules of [`super::history`], as the body
    /// does.
    pub fn model(&self) -> NavModel {
        self.model.borrow().clone()
    }

    pub fn set(&self, model: NavModel) {
        *self.model.borrow_mut() = model;
    }

    /// Have the body select `file` in the folder it shows first: a file
    /// named on the command line opens its folder with the file selected.
    /// Taken once; a tab that is shown again keeps its own selection.
    pub fn select_on_open(&self, file: PathBuf) {
        *self.away.select.borrow_mut() = Some(file);
    }

    /// The file to select when the body first shows this tab, once.
    pub(super) fn take_select(&self) -> Option<PathBuf> {
        self.away.select.borrow_mut().take()
    }

    /// A number no other tab's memory has. The view keys what it remembers
    /// of each folder (where it was scrolled, what was selected) by it, so a
    /// tab shown again comes back to its own place, also when another tab
    /// shows the same folder.
    pub(super) fn id(&self) -> u64 {
        self.id
    }
}

impl PartialEq for TabMemory {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.model, &other.model)
    }
}

/// A tab that was closed: where it stood among its neighbours, the folder it
/// showed, and its memory, so reopening it brings back its Back and Forward
/// lists as well as the folder.
#[derive(Clone)]
pub struct ClosedTab {
    pub index: usize,
    pub at: Where,
    pub memory: TabMemory,
}

/// The tabs closed most recently, newest last: what *Reopen closed tab*
/// (Ctrl+Shift+T) takes from. It keeps the last [`ClosedTabs::LIMIT`]; the
/// oldest is forgotten first.
#[derive(Clone, Default)]
pub struct ClosedTabs {
    stack: Vec<ClosedTab>,
}

impl ClosedTabs {
    /// How many closed tabs are remembered.
    pub const LIMIT: usize = 10;

    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, closed: ClosedTab) {
        self.stack.push(closed);
        if self.stack.len() > Self::LIMIT {
            self.stack.remove(0);
        }
    }

    /// The tab closed last, which is no longer remembered.
    pub fn pop(&mut self) -> Option<ClosedTab> {
        self.stack.pop()
    }

    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    pub fn len(&self) -> usize {
        self.stack.len()
    }
}

/// What a host gives the file view in a window: its tab's memory and what to
/// do with the things the view cannot do itself.
#[derive(Clone, PartialEq)]
pub struct Browse {
    pub(super) memory: TabMemory,
    pub(super) on_location: EventHandler<Where>,
    pub(super) on_new_tab: EventHandler<PathBuf>,
}

impl Browse {
    pub fn new(memory: TabMemory) -> Self {
        Self {
            memory,
            on_location: EventHandler::new(|_: Where| {}),
            on_new_tab: EventHandler::new(|_: PathBuf| {}),
        }
    }

    /// The tab went to another folder (or the Recent place).
    pub fn on_location(mut self, handler: impl Into<EventHandler<Where>>) -> Self {
        self.on_location = handler.into();
        self
    }

    /// A folder was asked for in a tab of its own: a middle click, or
    /// "Open in new tab" in a menu.
    pub fn on_new_tab(mut self, handler: impl Into<EventHandler<PathBuf>>) -> Self {
        self.on_new_tab = handler.into();
        self
    }
}

/// The file view of a window: the chooser's body with no question to answer.
pub fn browse(browse: Browse) -> Element {
    browse_body(browse)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn closed(index: usize, folder: &str) -> ClosedTab {
        let at = Where::Folder(PathBuf::from(folder));
        ClosedTab { index, memory: TabMemory::new(at.clone()), at }
    }

    fn folder_of(tab: &ClosedTab) -> String {
        match &tab.at {
            Where::Folder(path) => path.to_string_lossy().into_owned(),
            Where::Recent => "recent".to_string(),
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
        assert_eq!((last.index, folder_of(&last).as_str()), (2, "/b"));
        let first = stack.pop().expect("then the one before");
        assert_eq!((first.index, folder_of(&first).as_str()), (0, "/a"));
        assert!(stack.is_empty());
    }

    #[test]
    fn the_stack_keeps_only_the_most_recent_ones() {
        let mut stack = ClosedTabs::new();
        for n in 0..ClosedTabs::LIMIT + 3 {
            stack.push(closed(n, &format!("/f{n}")));
        }
        assert_eq!(stack.len(), ClosedTabs::LIMIT);
        let newest = stack.pop().unwrap();
        assert_eq!(folder_of(&newest), format!("/f{}", ClosedTabs::LIMIT + 2));
        let mut oldest = newest;
        while let Some(tab) = stack.pop() {
            oldest = tab;
        }
        assert_eq!(folder_of(&oldest), "/f3", "the three oldest were forgotten");
    }

    #[test]
    fn a_reopened_tab_keeps_its_own_memory() {
        let mut stack = ClosedTabs::new();
        let tab = closed(1, "/a");
        let memory = tab.memory.clone();
        stack.push(tab);
        let back = stack.pop().unwrap();
        assert!(back.memory == memory, "the same history comes back, not a new one");
    }

    #[test]
    fn a_duplicate_has_its_own_number_and_its_own_history() {
        let one = TabMemory::new(Where::Folder(PathBuf::from("/a")));
        let two = one.duplicate();
        assert!(one != two, "a duplicate is a different tab");
        assert_ne!(one.id(), two.id(), "with a number of its own");
        assert_eq!(one.model().location(), two.model().location(), "that starts where the first one is");
    }

    #[test]
    fn a_tab_that_departs_is_the_same_memory_and_hands_its_states_over_once() {
        let memory = TabMemory::new(Where::Folder(PathBuf::from("/a")));
        let (id, location) = (memory.id(), memory.model().location().clone());
        let in_the_old_window = memory.downgrade();
        assert!(in_the_old_window.upgrade().is_some());
        memory.publish(|| Box::new(("sorted", 42_u32)));
        assert!(memory.take_view::<(&str, u32)>().is_none(), "nothing has left yet");

        // The host moves the tab: the same memory, whoever holds it.
        let in_the_new_window = memory.clone();
        memory.depart();
        assert!(in_the_new_window == memory && in_the_new_window.id() == id, "moved, not made again");
        assert_eq!(in_the_new_window.model().location(), &location, "the history is where it was");
        assert!(in_the_old_window.upgrade().is_none(), "the window it left sees it gone, and lets go of its states");
        assert!(in_the_new_window.downgrade().upgrade().is_some(), "the window it joins has its own residence");

        assert_eq!(in_the_new_window.take_view::<(&str, u32)>(), Some(("sorted", 42)));
        assert!(in_the_new_window.take_view::<(&str, u32)>().is_none(), "the snapshot is handed over once");
    }

    #[test]
    fn a_departure_with_no_body_to_ask_has_nothing_to_hand_over() {
        let memory = TabMemory::new(Where::Recent);
        memory.depart();
        assert!(memory.take_view::<u32>().is_none(), "a tab that was never shown has no states");
        memory.publish(|| Box::new(7_u32));
        memory.depart();
        memory.depart();
        assert_eq!(memory.take_view::<u32>(), Some(7), "the second departure does not wipe the first's");
    }

    #[test]
    fn a_snapshot_of_another_kind_is_not_taken_for_this_one() {
        let memory = TabMemory::new(Where::Recent);
        memory.publish(|| Box::new("text"));
        memory.depart();
        assert!(memory.take_view::<u32>().is_none());
    }

    #[test]
    fn departing_records_the_view_on_show_before_anything_else() {
        use std::cell::Cell;
        let memory = TabMemory::new(Where::Recent);
        let recorded = Rc::new(Cell::new(0_u32));
        let seen = recorded.clone();
        let token = super::super::scroll_memory::register(memory.id(), move || seen.set(seen.get() + 1));
        memory.depart();
        assert_eq!(recorded.get(), 1, "the view on show was asked to record where it is scrolled");
        super::super::scroll_memory::unregister(token);
        memory.depart();
        assert_eq!(recorded.get(), 1, "a view that has gone is not asked");
    }
}
