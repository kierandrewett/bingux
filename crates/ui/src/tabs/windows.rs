//! The windows of the running application: which there are, so a tab can be
//! handed from one to another, and how a window is opened for one.
//!
//! Every window is one process's, on one thread, so the registry is a plain
//! thread-local list of handles to what a window keeps: its tabs, the marker
//! its strip shows for a tab dropped on it, and where its strip is. A window
//! adds itself when it opens and removes itself when it closes
//! ([`use_tab_window`] does both).
//!
//! The registry does not know what a tab shows. A window's tabs are reached
//! through a [`Carrier`], which moves them as they are, so a window of one
//! application can take the tabs of another window of the same application and
//! no one else's.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use freya::prelude::*;
use freya::winit::window::WindowId;

use super::set::{Moved, TabContent, TabSet};
use super::tearoff;
use crate::controls::StripProbe;
use crate::shell::with_window;

/// The tabs of one window, as another window moves them: erased, because the
/// registry holds the windows of one application, whatever its tabs show.
pub(crate) trait Carrier {
    /// Take every tab out, in order (`Vec<Moved<P>>`, boxed). `None` when the
    /// window has none to give.
    fn detach_all(&self) -> Option<Box<dyn Any>>;

    /// Put tabs (`Vec<Moved<P>>`, boxed) in at `at`. False when they are not
    /// this window's kind.
    fn insert_moved(&self, at: usize, tabs: Box<dyn Any>) -> bool;
}

impl<P: TabContent> Carrier for State<TabSet<P>> {
    fn detach_all(&self) -> Option<Box<dyn Any>> {
        let mut set = *self;
        let moved = set.write().detach_all();
        (!moved.is_empty()).then(|| Box::new(moved) as Box<dyn Any>)
    }

    fn insert_moved(&self, at: usize, tabs: Box<dyn Any>) -> bool {
        let Ok(tabs) = tabs.downcast::<Vec<Moved<P>>>() else {
            return false;
        };
        let mut set = *self;
        set.write().insert_moved(at, *tabs);
        true
    }
}

/// One window of the application, as the others see it.
#[derive(Clone)]
pub(crate) struct Entry {
    pub id: WindowId,
    /// 1 for the first window, and the lowest number no other window has for
    /// the rest. The compositor tells windows apart by title, and two windows
    /// on the same folder have the same one, so a later window's title ends in
    /// it.
    pub ordinal: u32,
    pub tabs: Rc<dyn Carrier>,
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
        (1..)
            .find(|ordinal| windows.iter().all(|window| window.ordinal != *ordinal))
            .unwrap_or(1)
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
    tearoff::window_closed(id);
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

/// The title the compositor shows for a window on `name` (a folder, a
/// document): the application's name after it, and for a later window its
/// number.
pub(crate) fn title(name: &str, app: &str, ordinal: u32) -> String {
    match (app.is_empty(), ordinal <= 1) {
        (true, true) => name.to_string(),
        (true, false) => format!("{name} ({ordinal})"),
        (false, true) => format!("{name} \u{2013} {app}"),
        (false, false) => format!("{name} \u{2013} {app} ({ordinal})"),
    }
}

/// The tabs a new window is born with, handed to it once. The new window
/// finds it with `bingux_ui::app::use_window_seed::<Birth<P>>()` and builds
/// its [`TabSet`] from [`Birth::take`].
#[derive(Clone)]
pub struct Birth<P>(Rc<RefCell<Option<(Vec<Moved<P>>, usize)>>>);

impl<P> Birth<P> {
    fn new(tabs: Vec<Moved<P>>, on_show: usize) -> Self {
        Self(Rc::new(RefCell::new(Some((tabs, on_show)))))
    }

    /// The tabs and which of them is on show. Once: the window owns them
    /// from then on, and this holds nothing.
    pub fn take(&self) -> Option<(Vec<Moved<P>>, usize)> {
        self.0.borrow_mut().take()
    }
}

impl<P: TabContent> Birth<P> {
    /// The tab set of a window born with these tabs, or `None` when it was
    /// not born with any (a window opened by launching the application).
    pub fn into_set(&self) -> Option<TabSet<P>> {
        self.take()
            .and_then(|(tabs, on_show)| TabSet::adopt(tabs, on_show))
    }
}

/// Open a window of its own for `tabs`, `size` big, with `on_show` (an index
/// into them) selected. Returns at once; the window appears when the
/// compositor has made it. The new window's router starts at the home
/// location: the tabs it shows are its own, whatever it was opened on.
pub(crate) fn open<P: TabContent>(tabs: Vec<Moved<P>>, on_show: usize, size: (f32, f32)) {
    if tabs.is_empty() {
        return;
    }
    spawn(async move {
        let _ = crate::app::open_window(Birth::new(tabs, on_show), None, size).await;
    });
}

/// How faint a window is while it is carried over another window's strip.
pub const GHOST_OPACITY: f32 = 0.4;

/// What a window with tabs keeps for the other windows: [`use_tab_window`]
/// makes it, the window's root and its strip use it.
#[derive(Clone)]
pub struct TabWindow {
    marker: State<Option<usize>>,
    ghost: State<bool>,
    probe: StripProbe,
    size: Rc<Cell<(f32, f32)>>,
}

impl TabWindow {
    /// The place in the tab list a tab being dropped on this window would
    /// take: the strip draws a marker there (`TabStrip::insertion`).
    pub fn insertion(&self) -> Option<usize> {
        *self.marker.read()
    }

    /// The strip reports where its tabs are through this
    /// (`TabStrip::probe`), so another window can ask where a dropped tab
    /// would land.
    pub fn probe(&self) -> &StripProbe {
        &self.probe
    }

    /// The window is being carried over another window's strip.
    pub fn ghosted(&self) -> bool {
        *self.ghost.read()
    }

    /// Say how big the window's content is now, in logical pixels. Call it
    /// from the root's `on_sized`: the pointer's place inside the window is
    /// measured against it.
    pub fn sized(&self, width: f32, height: f32) {
        self.size.set((width, height));
    }

    /// The window's root, dressed for tab tear-off: faded while it is carried
    /// over another window's strip, and listening for the pointer coming back
    /// when the compositor has finished moving it. While the compositor moves
    /// a window no window gets the pointer; the first event any gets is the
    /// pointer coming back: the button is up.
    pub fn root(&self, root: Rect) -> Rect {
        root.maybe(self.ghosted(), |root| root.opacity(GHOST_OPACITY))
            .on_global_pointer_move(move |_: Event<PointerEventData>| tearoff::pointer_arrived())
    }
}

/// Make this window one of the application's windows with tabs: every window
/// is known to the others, so a tab can be handed over, and the compositor's
/// title follows `name` (the tab on show) with the application's name and, for
/// a later window, its number. The window is removed when it closes. Call it
/// once per window, from the component that owns `set`.
pub fn use_tab_window<P: TabContent>(set: State<TabSet<P>>, name: &str) -> TabWindow {
    let window_id = Platform::window_id();
    let marker = use_state(|| None::<usize>);
    let ghost = use_state(|| false);
    let probe = use_hook(StripProbe::new);
    let title_cell = use_hook(|| Rc::new(RefCell::new(String::new())));
    let size = use_hook(|| Rc::new(Cell::new((0.0_f32, 0.0_f32))));
    let ordinal = use_hook(free_ordinal);
    use_hook({
        let entry = Entry {
            id: window_id,
            ordinal,
            tabs: Rc::new(set),
            marker,
            probe: probe.clone(),
            ghost,
            title: title_cell.clone(),
            size: size.clone(),
        };
        move || register(entry)
    });
    use_drop(move || unregister(window_id));
    // The compositor's title (the dock, the switcher) follows the tab. It
    // also tells this window from another on the same folder.
    use_side_effect_with_deps(&(name.to_string(), ordinal), {
        move |(name, ordinal): &(String, u32)| {
            let title = title(name, crate::app::app_title(), *ordinal);
            *title_cell.borrow_mut() = title.clone();
            with_window(move |window| window.set_title(&title));
        }
    });
    TabWindow {
        marker,
        ghost,
        probe,
        size,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_later_windows_title_ends_in_its_number() {
        assert_eq!(title("Home", "Files", 1), "Home \u{2013} Files");
        assert_eq!(title("Home", "Files", 2), "Home \u{2013} Files (2)");
        assert_ne!(
            title("Home", "Files", 2),
            title("Home", "Files", 3),
            "two windows on one folder are told apart"
        );
    }

    #[test]
    fn an_application_with_no_name_yet_titles_windows_by_their_tab() {
        assert_eq!(title("Notes", "", 1), "Notes");
        assert_eq!(title("Notes", "", 2), "Notes (2)");
    }

    #[test]
    fn a_window_that_never_got_its_tabs_has_none_to_give() {
        let birth = Birth::<u8>::new(Vec::new(), 0);
        assert!(birth.take().is_some());
        assert!(birth.take().is_none(), "the tabs are handed over once");
    }
}
