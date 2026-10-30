//! The windows of the running Files application: which there are, so a tab can
//! be handed from one to another, and how a window is opened for one.
//!
//! Every window is one process's, on one thread, so the registry is a plain
//! thread-local list of handles to what a window keeps: its tabs, the marker
//! its strip shows for a tab dropped on it, and where its strip is. A window
//! adds itself when it opens and removes itself when it closes.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use bingux_ui::controls::StripProbe;
use freya::prelude::*;
use freya::winit::window::WindowId;

use crate::tabset::{Moved, TabSet};

/// One window of the application, as the others see it.
#[derive(Clone)]
pub(crate) struct Entry {
    pub id: WindowId,
    /// 1 for the first window, and the lowest number no other window has for
    /// the rest. The compositor tells windows apart by title, and two windows
    /// on the same folder have the same one, so a later window's title ends in
    /// it.
    pub ordinal: u32,
    pub set: State<TabSet>,
    /// The place a tab dropped on this window would take, drawn by its strip.
    pub marker: State<Option<usize>>,
    /// Where the strip's tabs are.
    pub probe: StripProbe,
    /// The window is being carried over another window's strip, and is drawn
    /// faded so the strip's marker shows through it.
    pub ghost: State<bool>,
    /// The title the compositor has for the window now.
    pub title: Rc<RefCell<String>>,
    /// The window's size, in logical pixels.
    pub size: Rc<Cell<(f32, f32)>>,
}

thread_local! {
    static WINDOWS: RefCell<Vec<Entry>> = const { RefCell::new(Vec::new()) };
}

/// The number a window opened now takes (see [`Entry::ordinal`]).
pub(crate) fn free_ordinal() -> u32 {
    WINDOWS.with_borrow(|windows| {
        (1..).find(|ordinal| windows.iter().all(|window| window.ordinal != *ordinal)).unwrap_or(1)
    })
}

pub(crate) fn register(entry: Entry) {
    WINDOWS.with_borrow_mut(|windows| {
        windows.retain(|window| window.id != entry.id);
        windows.push(entry);
    });
}

/// The window is gone. A tear-off that was using it ends.
pub(crate) fn unregister(id: WindowId) {
    WINDOWS.with_borrow_mut(|windows| windows.retain(|window| window.id != id));
    crate::tearoff::window_closed(id);
}

pub(crate) fn find(id: WindowId) -> Option<Entry> {
    WINDOWS.with_borrow(|windows| windows.iter().find(|window| window.id == id).cloned())
}

pub(crate) fn all() -> Vec<Entry> {
    WINDOWS.with_borrow(Clone::clone)
}

/// How many windows there are.
pub(crate) fn count() -> usize {
    WINDOWS.with_borrow(Vec::len)
}

/// The title the compositor shows for a window on folder `name`: the app's
/// name after it, and for a later window its number.
pub(crate) fn title(name: &str, ordinal: u32) -> String {
    if ordinal <= 1 {
        format!("{name} \u{2013} Files")
    } else {
        format!("{name} \u{2013} Files ({ordinal})")
    }
}

/// The tabs a new window is born with, handed to it once.
#[derive(Clone)]
pub(crate) struct Birth(Rc<RefCell<Option<(Vec<Moved>, usize)>>>);

impl Birth {
    fn new(tabs: Vec<Moved>, on_show: usize) -> Self {
        Self(Rc::new(RefCell::new(Some((tabs, on_show)))))
    }

    /// The tabs and which of them is on show. Once: the window owns them
    /// from then on, and this holds nothing.
    pub(crate) fn take(&self) -> Option<(Vec<Moved>, usize)> {
        self.0.borrow_mut().take()
    }
}

/// Open a window of its own for `tabs`, `size` big, with `on_show` (an index
/// into them) selected. Returns at once; the window appears when the
/// compositor has made it.
pub(crate) fn open(tabs: Vec<Moved>, on_show: usize, size: (f32, f32)) {
    if tabs.is_empty() {
        return;
    }
    spawn(async move {
        // The router of the new window starts at the home folder: the tabs it
        // shows are its own, whatever it was opened on.
        let _ = bingux_ui::app::open_window(Birth::new(tabs, on_show), None, size).await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_later_windows_title_ends_in_its_number() {
        assert_eq!(title("Home", 1), "Home \u{2013} Files");
        assert_eq!(title("Home", 2), "Home \u{2013} Files (2)");
        assert_ne!(title("Home", 2), title("Home", 3), "two windows on one folder are told apart");
    }

    #[test]
    fn a_window_that_never_got_its_tabs_has_none_to_give() {
        let birth = Birth::new(Vec::new(), 0);
        assert!(birth.take().is_some());
        assert!(birth.take().is_none(), "the tabs are handed over once");
    }
}
