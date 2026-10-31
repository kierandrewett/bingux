//! Mounting a page a little at a time.
//!
//! Mounting a page is work in proportion to what is on it: every row makes its
//! components, its nodes and its text, and a page of a dozen rows costs the
//! frame that mounts it 20 to 30 ms, while a frame has 16. Nothing waits in it,
//! and it still drops frames, in the middle of the slide that brings the page
//! in. [`staged`] splits that frame: the page head and the first group mount
//! with the page, and one more child of the page's column a frame after that,
//! top to bottom, until the page is whole. A group is a few rows, a few
//! milliseconds, so each frame has room for the slide beside it.
//!
//! Nothing moves while it fills in. A child is added under the last one, and
//! nothing is ever mounted below something that is not mounted yet, so there is
//! no gap to hold open and no estimate to be wrong: the page is simply shorter
//! until it is whole. The scroll height is the exception, and it is not left to
//! chance: a page remembers its height from the last time it was whole, and
//! holds that as its minimum height while it fills in, so opening a page you
//! have seen before has its final scroll extent from the first frame.
//!
//! A long group is staged the same way inside the page: its first rows mount
//! with it, and [`use_staged_rows`] adds a few more each frame, while the
//! groups below it wait for it to be whole (so it never grows into something
//! that is already there) and its footer waits too.
//!
//! Something that sits under the page, outside its column (the Related group
//! under a top-level Settings page), would be pushed down by every child that
//! mounts above it, so it waits too: the component that hosts the page calls
//! [`provide_page`] and wraps what goes under the page in [`after_page`], which
//! mounts it once the page is whole, groups that are still adding rows
//! included. A host with no staged page under it shows it at once.
//!
//! A page is mounted whole at once when the location points at a row (the row
//! has to exist to be scrolled to) and while a search is running (the matches
//! are counted by the rows that are mounted). Setting `BINGUX_NO_STAGE` mounts
//! every page whole, to compare.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use freya::prelude::*;

use crate::layout::page_column;
use crate::search_highlight;

/// The children that mount with the page: the head and the first group.
const FIRST: usize = 2;
/// The rows of a long group that mount with it, and the rows added a frame.
/// A card that grows moves its bottom edge, so only a group too long for
/// anyone to take in at once is staged.
pub(crate) const FIRST_ROWS: usize = 24;
pub(crate) const MORE_ROWS: usize = 12;

thread_local! {
    /// The pages (by location) that had a stand-in on screen and have not been mounted since: the
    /// next page that mounts there replaces the stand-in with its data landing, not a page that
    /// slides in. See [`use_provisional`].
    static STAND_INS: RefCell<std::collections::HashSet<String>> = RefCell::new(std::collections::HashSet::new());
    /// The height of each page, by location, the last time it was whole.
    static HEIGHTS: RefCell<HashMap<String, f32>> = RefCell::new(HashMap::new());
}

/// The page's own place: what [`PageLocation`] says, else where the router is.
fn page_key() -> Option<String> {
    try_consume_context::<PageLocation>()
        .map(|place| place.0)
        .or_else(|| crate::nav::router::try_router().map(|router| router.location().path_text()))
}

/// Whether staging is off (`BINGUX_NO_STAGE` is set).
fn disabled() -> bool {
    static OFF: std::sync::LazyLock<bool> =
        std::sync::LazyLock::new(|| std::env::var_os("BINGUX_NO_STAGE").is_some());
    *OFF
}

/// `column` with its children mounted a few at a time, as described in the
/// [module](self). Call it on the page's column, last, whatever else the
/// page does with it afterwards (a veil over it, an opacity) still applies.
/// A column of up to [`FIRST`] children is returned as it is.
pub fn staged(mut column: Rect) -> Rect {
    if disabled() || column.get_children().len() <= FIRST {
        return column;
    }
    let children = std::mem::take(column.get_children());
    column.child(Staged { children })
}

/// The location of the page that is drawn, provided by whoever draws it (the
/// split view gives each page its own, including one that slides out), so what a
/// page remembers about itself is kept under its own place.
#[derive(Clone, PartialEq)]
pub struct PageLocation(pub String);

/// What a page being staged shares with the groups under it: how many of
/// them are still adding rows. The page waits for them before it adds the next
/// child.
#[derive(Clone)]
pub(crate) struct StagedPage {
    pending: Rc<Cell<usize>>,
}

/// What the host of a page shares with the staged column in it and with what
/// sits under the column (see [`provide_page`] and [`after_page`]).
#[derive(Clone, Default)]
struct PageFill {
    /// Staged columns under the host: one, or none for a page too short to stage.
    columns: Rc<Cell<usize>>,
    /// Whether every child of the column is mounted.
    mounted: Rc<Cell<bool>>,
    /// Groups of the column that are still adding rows.
    pending: Rc<Cell<usize>>,
    /// Skeletons mounted under the host: stand-ins for a page whose shape is
    /// not known yet (see [`use_provisional`]).
    provisional: Rc<Cell<usize>>,
}

impl PageFill {
    /// Nothing under the host is still filling in, and no skeleton stands in for
    /// what will be there.
    fn whole(&self) -> bool {
        self.provisional.get() == 0
            && (self.columns.get() == 0 || (self.mounted.get() && self.pending.get() == 0))
    }
}

/// Call from a stand-in for a page whose shape is not known yet (a skeleton, a
/// page hidden until its data lands): while `active`, what goes under the page
/// ([`after_page`]) waits, until the stand-in is gone or inactive and the page
/// that replaces it is whole. A stand-in is not the shape of the loaded page, so
/// a Related group placed under it would be pushed down when the data lands.
/// Does nothing outside a host that called [`provide_page`]. Call it on every
/// render, whatever `active` is: it is a hook.
pub fn use_provisional(active: bool) {
    let fill = use_hook(try_consume_context::<PageFill>);
    let counted = use_hook(|| Rc::new(Cell::new(false)));
    // A page that replaces this stand-in is the data landing in a page that is already on
    // screen. It is mounted whole: staged a group a frame it would show the page's first
    // groups under blank space where the stand-in was, and the rest popping in one by one.
    use_hook(|| {
        if active && let Some(key) = page_key() {
            STAND_INS.with_borrow_mut(|marks| marks.insert(key));
        }
    });
    if let Some(fill) = &fill {
        if active && !counted.get() {
            fill.provisional.set(fill.provisional.get() + 1);
            counted.set(true);
        } else if !active && counted.get() {
            fill.provisional
                .set(fill.provisional.get().saturating_sub(1));
            counted.set(false);
        }
    }
    use_drop({
        let counted = counted.clone();
        move || {
            if let (true, Some(fill)) = (counted.get(), &fill) {
                fill.provisional
                    .set(fill.provisional.get().saturating_sub(1));
            }
        }
    });
}

/// Call in the component that hosts a page and puts something under it, before
/// the page is built. What goes under the page is wrapped in [`after_page`].
pub fn provide_page() {
    let fill = use_hook(PageFill::default);
    provide_context(fill);
}

/// `child`, mounted once the staged page above it is whole: the Related
/// group under a Settings page, a footer. Under a host that called
/// [`provide_page`] with no staged page in it, and anywhere else, it is
/// mounted at once. Once mounted it stays.
pub fn after_page(child: impl Into<Element>) -> Element {
    Element::from(AfterPage {
        child: child.into(),
    })
}

#[derive(PartialEq)]
struct AfterPage {
    child: Element,
}

impl Component for AfterPage {
    fn render(&self) -> impl IntoElement {
        let fill = try_consume_context::<PageFill>();
        // The page above renders first (children render in order), so its
        // column has registered by now.
        let mut shown = use_state(|| fill.as_ref().is_none_or(PageFill::whole));
        use_hook({
            let fill = fill.clone();
            move || {
                let ticker = RenderingTicker::get();
                let window = Platform::get().current_window();
                spawn(async move {
                    while !*shown.peek() {
                        window.request_redraw();
                        ticker.tick().await;
                        if fill.as_ref().is_none_or(PageFill::whole) {
                            shown.set(true);
                        }
                    }
                })
            }
        });
        // A page that was hidden or drawn as a skeleton fades its data in; what sits under it
        // fades in with it, or it would stand on the screen first, with nothing above it.
        let held = use_hook(|| Rc::new(Cell::new(false)));
        if fill.as_ref().is_some_and(|fill| fill.provisional.get() > 0) {
            held.set(true);
        }
        let tokens = crate::surface::tokens_now();
        let fade = crate::motion::use_tween_from(
            if held.get() { 0.0 } else { 1.0 },
            1.0,
            tokens.motion(crate::motion::MOVE_MS),
        );
        if shown() {
            Element::from(rect().opacity(fade).child(self.child.clone()))
        } else {
            Element::from(rect())
        }
    }
}

/// The rows of a group of `total` rows that are mounted now, and whether that
/// is all of them. A group outside a staged page, a short one and a group
/// while `searching` mount every row at once. Call it on every render of the
/// group.
pub(crate) fn use_staged_rows(total: usize, searching: bool) -> (usize, bool) {
    let page = try_consume_context::<StagedPage>();
    let mut shown = use_state(|| FIRST_ROWS);
    let whole = use_hook(|| Rc::new(Cell::new(false)));
    let counted = use_hook(|| Rc::new(Cell::new(false)));
    let known = use_hook(|| Rc::new(Cell::new(total)));
    known.set(total);
    if page.is_none() || searching || total <= FIRST_ROWS || shown() >= total {
        whole.set(true);
    }
    if let Some(page) = &page {
        if !whole.get() && !counted.get() {
            page.pending.set(page.pending.get() + 1);
            counted.set(true);
        } else if whole.get() && counted.get() {
            page.pending.set(page.pending.get().saturating_sub(1));
            counted.set(false);
        }
    }
    use_drop({
        let counted = counted.clone();
        let page = page.clone();
        move || {
            if let (true, Some(page)) = (counted.get(), &page) {
                page.pending.set(page.pending.get().saturating_sub(1));
            }
        }
    });
    use_hook({
        let whole = whole.clone();
        let known = known.clone();
        move || {
            let ticker = RenderingTicker::get();
            let window = Platform::get().current_window();
            spawn(async move {
                while !whole.get() && *shown.peek() < known.get() {
                    window.request_redraw();
                    ticker.tick().await;
                    let next = *shown.peek() + MORE_ROWS;
                    shown.set(next);
                }
            })
        }
    });
    if whole.get() {
        (total, true)
    } else {
        (shown().min(total), false)
    }
}

/// The children of a page's column, shown from the first towards the last.
#[derive(PartialEq)]
struct Staged {
    children: Vec<Element>,
}

impl Component for Staged {
    fn render(&self) -> impl IntoElement {
        let total = self.children.len();
        let mut shown = use_state(|| FIRST);
        let place = use_hook(page_key);
        let replaces_stand_in = use_hook({
            let place = place.clone();
            move || place.is_some_and(|key| STAND_INS.with_borrow_mut(|marks| marks.remove(&key)))
        });
        // Marks left by stand-ins inside this page (a skeleton row in one of its groups) are not for
        // the next page that mounts here.
        use_drop({
            let place = place.clone();
            move || {
                if let Some(key) = &place {
                    STAND_INS.with_borrow_mut(|marks| marks.remove(key));
                }
            }
        });
        let done = use_hook(|| Rc::new(Cell::new(replaces_stand_in)));
        let known = use_hook(|| Rc::new(Cell::new(total)));
        known.set(total);
        // What the host of the page, if there is one, is told (see `provide_page`):
        // a staged column is here until it is whole, groups still adding rows
        // included.
        let fill = use_hook(|| {
            let fill = try_consume_context::<PageFill>().unwrap_or_default();
            fill.columns.set(fill.columns.get() + 1);
            fill.mounted.set(false);
            fill
        });
        use_drop({
            let fill = fill.clone();
            move || fill.columns.set(fill.columns.get().saturating_sub(1))
        });
        let pending = fill.pending.clone();
        provide_context(StagedPage {
            pending: pending.clone(),
        });
        // A location that names a row, and a search, need the whole page.
        let fragment = crate::nav::router::use_fragment().is_some();
        let searching = search_highlight::current().is_some();
        if fragment || searching || shown() >= total {
            done.set(true);
        }
        fill.mounted.set(done.get());
        use_hook({
            let done = done.clone();
            let known = known.clone();
            move || {
                let ticker = RenderingTicker::get();
                let window = Platform::get().current_window();
                spawn(async move {
                    while !done.get() && *shown.peek() < known.get() {
                        window.request_redraw();
                        ticker.tick().await;
                        // The next child waits for the groups above it to be whole.
                        if pending.get() == 0 {
                            let next = *shown.peek() + 1;
                            shown.set(next);
                        }
                    }
                })
            }
        });
        // The page's own place, not the router's: a page that slides out is built again
        // while the router already points at the page that replaced it, and a page
        // that Back reveals is built again before the router points at it for the
        // frame. Keyed by the router, either one found no height kept for it, drew
        // shorter than its scroll offset, and the scroll view cut the offset to fit
        // (the offset it keeps on the page's controller, so Back found the page at
        // the top).
        let key = try_consume_context::<PageLocation>()
            .map(|place| place.0)
            .or_else(|| {
                crate::nav::router::try_router().map(|router| router.location().path_text())
            });
        let reserved = key
            .as_ref()
            .and_then(|key| HEIGHTS.with_borrow(|heights| heights.get(key).copied()));
        let visible = if done.get() {
            total
        } else {
            shown().min(total)
        };
        let whole = done.get();
        page_column()
            .maybe(!whole && reserved.is_some(), |column| {
                column.min_height(Size::px(reserved.unwrap_or(0.0)))
            })
            .on_sized(move |event: Event<SizedEventData>| {
                if let (true, Some(key)) = (done.get(), key.clone()) {
                    HEIGHTS.with_borrow_mut(|heights| {
                        heights.insert(key, event.area.height());
                    });
                }
            })
            .children(self.children.iter().take(visible).cloned())
    }
}
