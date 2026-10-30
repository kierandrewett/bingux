//! A strip of closable tabs for an application whose window holds several
//! views of the same kind: the folders of a file manager, the documents of an
//! editor. It speaks the same language as the rest of the shell: a tab is a
//! rounded rectangle of [`Radius::Control`], like a sidebar row or a button,
//! and the selected one takes the fill of a selected sidebar row
//! ([`Paint::Selection`]). The tabs are separate from the content under them,
//! as a segmented control is from the page it switches; nothing joins them and
//! the page has no corner of its own to match. See `design/tabs.md`.
//!
//! The strip is the title of a header bar ([`HeaderBar::tab_slot`]): the bar is
//! the window's title bar, on [`Paint::Surface`] like every other bar, with the
//! window's buttons in the same row, and the page starts at its bottom edge.
//!
//! * **Shape.** The strip is [`Dim::TabBar`] tall (40 px; the whole 44 px touch
//!   target at compact density). A tab is [`FIELD_INSET`] from its top and bottom
//!   edges, so it is [`Dim::TabButton`] tall (32 px; 44 px at compact density,
//!   where there is no inset), and the band above and below it still drags the
//!   window. Two tabs are [`S1`] apart. An unselected tab has no fill and muted
//!   ink; the pointer's fill ([`Paint::Hover`]) fades in under it; pressed is a
//!   stronger fill ([`Paint::Pressed`]) at once; the selected tab has the
//!   selection fill and text ink. A drop target takes [`Paint::AccentSoft`].
//! * **Width.** Tabs share one width. It is [`Dim::Field`] at most; as tabs
//!   are added they shrink together, down to [`Dim::TabMin`], and past that the
//!   strip scrolls sideways instead. The strip hugs its tabs and never grows
//!   past its container.
//! * **A tab** is an optional 16 px icon, a title in the small type step that
//!   takes what room is left, and a close button. A title that does not fit
//!   does not end in an ellipsis: its last [`Dim::TextFade`] (24 px) fade out
//!   ([`fade_text`](super::fade_text)), at the end of the reading direction,
//!   and the close button beside it is never touched. The close button has a
//!   slot of its own that is always laid out, so a tab does not change width
//!   when the button appears. It is a standard icon button, 24 px (the whole
//!   44 px at compact density), whose corner nests in the tab's, and shows on
//!   the selected tab, on a tab the pointer is over or the keyboard focus is
//!   in, and on every tab when the strip is touched. Every tab can be closed,
//!   a lone one too: the host decides what that means (Files closes the window).
//! * **The end of the strip** is fixed: the New tab button (drawn when
//!   [`TabStrip::on_new`] is set) follows the last tab and never scrolls away.
//!   It is a [`Dim::TabButton`] square, a tab's height, with the corner of a
//!   tab and the same distance from the strip's top, its bottom, the last tab
//!   and the strip's trailing edge ([`FIELD_INSET`]; flush at compact
//!   density). When the tabs overflow, a tab list button beside it opens a
//!   menu of every tab (its icon, its name, the selected one ticked), and the
//!   tabs fade into the bar at the edge they continue past. The selected tab is
//!   scrolled into view.
//! * **Scrolling.** The wheel over the strip scrolls it (the vertical wheel
//!   too, as a strip has one axis), so does a touchpad's two-finger movement,
//!   and a finger dragged along it. A wheel notch glides, a touchpad and a
//!   finger are followed at once. Dragging a tab to the edge scrolls too.
//! * **The pointer.** The tab it is on is worked out from where the pointer
//!   and the tabs are, so exactly one tab is hovered, and it stays right when
//!   the tabs move under a pointer that does not (a wheel notch, a tab
//!   closing, a rapid close). The hover fill fades in and out on the standard
//!   motion; pressed is a stronger fill at once; a tab in the hand of a drag is
//!   lifted over its neighbours.
//! * **Adding and closing** animate the widths, so the tabs make room and
//!   close ranks without a jump. After a tab is closed with its button or a
//!   middle click the others keep their width until the pointer leaves the
//!   strip, so the next close button is under the pointer (rapid closure,
//!   [`tab_close::RapidClose`]); they then grow to fill the room.
//! * **Pointer.** A press selects, a middle click closes, and with
//!   [`TabStrip::on_reorder`] set a tab can be dragged: it follows the
//!   pointer and the others slide aside ([`crate::reorder`]), and it settles
//!   into its place when let go. Escape, or letting go far above or below the
//!   strip, puts it back. A right click, or a touch held on a tab, opens its
//!   menu: Close tab, Close other tabs, Close tabs to the left and Close tabs
//!   to the right ([`TabStrip::on_close_many`]; the left one is greyed out on
//!   the first tab, the right one on the last), Duplicate tab
//!   ([`TabStrip::on_duplicate`]), Reopen closed tab
//!   ([`TabStrip::on_reopen`]), and Move left and Move right for a reorderable
//!   strip, which is how touch reorders.
//! * **Keyboard.** Only the selected tab is in the tab order. Left, Right,
//!   Home and End move the selection and the focus; Ctrl+Shift+Left and Right
//!   move the selected tab; Delete or Ctrl+W closes the selected tab, Ctrl+T
//!   asks for a new one and Ctrl+Shift+T for the last closed one. The focus
//!   ring is the tab's own rounded rectangle.
//! * **Reading direction.** The strip is built in reading order and the layout
//!   engine mirrors it ([`Direction`](super::direction::Direction)): in a
//!   right-to-left interface the first tab is at the right, a tab's icon is at
//!   its right and its close button at its left, the New tab button follows the
//!   last tab on its left, and the strip scrolls forward leftwards. What the
//!   engine cannot know is physical and is converted here
//!   ([`tab_scroll`](super::tab_scroll)): the pointer's distances (a tab dragged
//!   to a later place goes left), the wheel's and a finger's sideways movement,
//!   the arrow keys (the screen's), and the place a tab torn from another
//!   window would land, counted from the right ([`StripLayout`]).
//! * [`TabStrip::drop_target`] marks the tab a drag would drop on with the
//!   accent, so a host can accept files on a tab.
//! * [`TabStrip::on_detach`] lets a tab be torn off: dragged past the strip by
//!   a strip's height ([`tab_tear`]) it leaves, and the host takes it to a
//!   window of its own. [`TabStrip::insertion`] draws the marker of where a
//!   tab brought from another window would land, and [`TabStrip::probe`]
//!   publishes where the tabs are so the host can work out that place.
//!
//! At compact density the strip has no [`FIELD_INSET`]: each tab, and the
//! close, tab list and New tab buttons, is the whole 44 px touch target.
//!
//! The strip keeps no state of its own beyond what is on screen: the host
//! owns the list and the selected id. When a tab closes, the host picks the
//! tab that takes the selection: [`tab_close::selection_after_close`] is the
//! browser's rule.
//!
//! [`HeaderBar::tab_slot`]: crate::shell::HeaderBar::tab_slot

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use freya::animation::*;
use freya::prelude::*;

use super::button::icon_button;
use super::direction::{Direction, use_direction};
use super::fade_text::fade_text;
use super::press::use_press;
use super::tab_close::{self, Act, Offers, RapidClose, Row};
use super::tab_geometry::Geometry;
use super::tab_scroll;
use super::tab_tear::{self, Detach, Pull, StripLayout, StripProbe, TearGate};
use crate::component::ComponentName;
use crate::drag::{Frames, PointerDrag, use_frames, use_pointer_drag};
use crate::icons::{self, Icon};
use crate::motion::{Focusable, MOVE_MS, use_tween};
use crate::reorder::{self, Done, Indicate, Reorder, Stack};
use crate::shell::menu::{MenuItem, menu_item, menu_separator};
use crate::shell::overlays::Overlays;
use crate::shell::panel::{PanelRect, PanelTrigger};
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{CAPTION, Ink};

/// How far the pointer travels before a press on a tab becomes a drag.
const DRAG_SLOP: f32 = 6.0;

/// The most a pointer may leave the strip, up and down, and still drop a
/// dragged tab: beyond this the drag is cancelled.
const DRAG_REACH: Dim = Dim::Header;

/// The layer the tab in the hand of a drag is lifted by (see `Tab`): more than
/// a tab's contents are deep, so all of the held tab paints over the tabs it
/// slides across.
const HELD_LAYER: i16 = crate::layers::TAB_HELD;

/// What the strip stands on and what the fades at its edges dissolve into: the
/// header bar's own fill, which is also the page's.
pub(crate) const BAR: Paint = Paint::Surface;

/// The marker of where a dragged-in tab would land is above all of them.
const MARKER_LAYER: i16 = HELD_LAYER + 1;

/// How faint a held tab is drawn once going on would tear it off the strip.
const ARMED_OPACITY: f32 = 0.7;

/// One tab. `id` names it for as long as it exists, whatever its place.
#[derive(Clone, Debug, PartialEq)]
pub struct TabItem {
    pub id: u64,
    pub title: String,
    pub icon: Option<Icon>,
}

impl TabItem {
    pub fn new(id: u64, title: impl Into<String>) -> Self {
        Self {
            id,
            title: title.into(),
            icon: None,
        }
    }

    /// An icon before the title.
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }
}

/// Build a strip of `tabs` with `selected` raised.
pub fn tab_strip(tabs: Vec<TabItem>, selected: u64) -> TabStrip {
    TabStrip {
        tabs,
        selected,
        on_select: EventHandler::new(|_: u64| {}),
        on_close: EventHandler::new(|_: u64| {}),
        on_close_many: None,
        on_duplicate: None,
        on_reopen: None,
        can_reopen: false,
        on_new: None,
        on_hover: EventHandler::new(|_: (u64, bool)| {}),
        on_reorder: None,
        drop_target: None,
        on_detach: None,
        insertion: None,
        probe: None,
    }
}

#[derive(Clone, PartialEq)]
pub struct TabStrip {
    tabs: Vec<TabItem>,
    selected: u64,
    on_select: EventHandler<u64>,
    on_close: EventHandler<u64>,
    on_close_many: Option<EventHandler<Vec<u64>>>,
    on_duplicate: Option<EventHandler<u64>>,
    on_reopen: Option<EventHandler<()>>,
    can_reopen: bool,
    on_new: Option<EventHandler<()>>,
    on_hover: EventHandler<(u64, bool)>,
    on_reorder: Option<EventHandler<(u64, usize)>>,
    drop_target: Option<u64>,
    on_detach: Option<EventHandler<Detach>>,
    insertion: Option<usize>,
    probe: Option<StripProbe>,
}

impl TabStrip {
    pub fn on_select(mut self, handler: impl Into<EventHandler<u64>>) -> Self {
        self.on_select = handler.into();
        self
    }

    /// A close button, a middle click, Delete, Ctrl+W or the menu asked to
    /// close this tab. Every tab can ask, a lone one too: the host decides
    /// what closing the last tab means, and which tab takes the selection
    /// ([`selection_after_close`](super::tab_close::selection_after_close)).
    pub fn on_close(mut self, handler: impl Into<EventHandler<u64>>) -> Self {
        self.on_close = handler.into();
        self
    }

    /// Offer *Close other tabs* and *Close tabs to the right* in a tab's
    /// menu. The handler gets the ids to close, in strip order.
    pub fn on_close_many(mut self, handler: impl Into<EventHandler<Vec<u64>>>) -> Self {
        self.on_close_many = Some(handler.into());
        self
    }

    /// Offer *Duplicate tab* in a tab's menu.
    pub fn on_duplicate(mut self, handler: impl Into<EventHandler<u64>>) -> Self {
        self.on_duplicate = Some(handler.into());
        self
    }

    /// Offer *Reopen closed tab* in a tab's menu, and bind Ctrl+Shift+T on
    /// the strip to it. `available` is whether there is a closed tab to bring
    /// back: the item is greyed out and the key does nothing without one.
    pub fn on_reopen(mut self, available: bool, handler: impl Into<EventHandler<()>>) -> Self {
        self.on_reopen = Some(handler.into());
        self.can_reopen = available;
        self
    }

    /// Show the New tab button and call `handler` when it is pressed, or
    /// when Ctrl+T is pressed on a tab.
    pub fn on_new(mut self, handler: impl Into<EventHandler<()>>) -> Self {
        self.on_new = Some(handler.into());
        self
    }

    /// The pointer entered (`true`) or left (`false`) a tab. A host that takes
    /// a drag onto its tabs learns where the pointer is from this.
    pub fn on_hover(mut self, handler: impl Into<EventHandler<(u64, bool)>>) -> Self {
        self.on_hover = handler.into();
        self
    }

    /// Let the person move tabs: the handler gets the id of the tab that was
    /// dragged (or moved by its menu or the keyboard) and the place it now
    /// belongs at in the list, as if it were removed and inserted there. The
    /// strip does not reorder itself; the host reorders its list and passes
    /// it back.
    pub fn on_reorder(mut self, handler: impl Into<EventHandler<(u64, usize)>>) -> Self {
        self.on_reorder = Some(handler.into());
        self
    }

    /// Mark this tab as the one a drag would drop on.
    pub fn drop_target(mut self, id: Option<u64>) -> Self {
        self.drop_target = id;
        self
    }

    /// Let a tab be torn off. A tab dragged past the strip by a strip's
    /// height (see [`tab_tear`]), or released there, or, if it is the only one,
    /// dragged at all, is reported here and is the host's: the strip puts it
    /// back where it was and ends its own drag, and the host takes the tab out
    /// of the list it passes back. Without a handler a tab never leaves.
    pub fn on_detach(mut self, handler: impl Into<EventHandler<Detach>>) -> Self {
        self.on_detach = Some(handler.into());
        self
    }

    /// Draw the marker of a tab that would be dropped at place `at` of the
    /// list (`0` before the first tab, the number of tabs after the last).
    /// For a host that takes a tab dragged in from another window.
    pub fn insertion(mut self, at: Option<usize>) -> Self {
        self.insertion = at;
        self
    }

    /// Publish where the tabs are, every time the strip draws, to `probe`.
    pub fn probe(mut self, probe: &StripProbe) -> Self {
        self.probe = Some(probe.clone());
        self
    }
}

// ---- Layout ----------------------------------------------------------------

/// How the strip fills `available` pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Fit {
    /// The width of one tab's body that is fully present.
    tab: f32,
    /// The width of the strip.
    rail: f32,
    /// The width of the region the tabs show in.
    region: f32,
    /// Whether the tabs do not fit at their narrowest: they scroll, and the
    /// tab list button is drawn.
    overflow: bool,
}

impl Fit {
    /// The same fit with the tabs `tab` wide instead of what the room gives,
    /// while a rapid close holds the width. A scrolling strip is as it was.
    fn held_at(self, geometry: Geometry, tab: f32, effective: f32, add: bool) -> Fit {
        if self.overflow || (tab - self.tab).abs() < f32::EPSILON {
            return self;
        }
        let region = geometry.content(effective, tab);
        Fit {
            tab,
            rail: region + geometry.cluster(usize::from(add)),
            region,
            overflow: false,
        }
    }
}

/// The widths for `target` tabs that are staying and `effective` tabs' worth of
/// width right now (a tab that is arriving or leaving counts for part of one),
/// in `available` pixels, with or without the New tab button.
fn fit(geometry: Geometry, available: f32, target: usize, effective: f32, add: bool) -> Fit {
    let room = (available - geometry.cluster(usize::from(add))).max(0.0);
    if geometry.content(target as f32, geometry.min) > room + 0.01 {
        // Past the narrowest width: the strip is as wide as its container, and
        // the tab list button joins the New tab button after the tabs.
        return Fit {
            tab: geometry.min,
            rail: available.max(0.0),
            region: (available - geometry.cluster(usize::from(add) + 1)).max(0.0),
            overflow: true,
        };
    }
    let count = effective.max(1.0);
    let tab = geometry.tab_in(room, count).clamp(0.0, geometry.max);
    let region = geometry.content(effective, tab);
    Fit {
        tab,
        rail: region + geometry.cluster(usize::from(add)),
        region,
        overflow: false,
    }
}

// ---- What is on screen -----------------------------------------------------

/// Where a tab is in its life on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Life {
    Live,
    /// Growing from nothing to its width.
    Entering,
    /// Shrinking to nothing, after the host removed it.
    Leaving,
}

#[derive(Clone, Debug, PartialEq)]
struct Entry {
    item: TabItem,
    life: Life,
}

impl Entry {
    /// How much of its width the tab has, 0 to 1, `progress` of the way
    /// through the strip's motion.
    fn presence(&self, progress: f32) -> f32 {
        match self.life {
            Life::Live => 1.0,
            Life::Entering => progress,
            Life::Leaving => 1.0 - progress,
        }
    }
}

/// Merge the host's `tabs` into what is on screen. A tab the host removed
/// stays as a [`Life::Leaving`] entry where it was and a new one comes in as
/// [`Life::Entering`], so the widths can move. Returns whether any tab
/// entered or left, which starts a new motion: tabs still arriving from the
/// last one are then settled, and tabs still leaving are gone.
fn merge(entries: &mut Vec<Entry>, tabs: &[TabItem], animate: bool) -> bool {
    let previous = std::mem::take(entries);
    let mut changed = false;
    let mut merged: Vec<Entry> = tabs
        .iter()
        .map(|tab| {
            match previous
                .iter()
                .find(|entry| entry.item.id == tab.id && entry.life != Life::Leaving)
            {
                Some(entry) => Entry {
                    item: tab.clone(),
                    life: entry.life,
                },
                None => {
                    changed |= animate;
                    Entry {
                        item: tab.clone(),
                        life: if animate { Life::Entering } else { Life::Live },
                    }
                }
            }
        })
        .collect();
    let mut leaving = Vec::new();
    for (index, entry) in previous.iter().enumerate() {
        if animate && tabs.iter().all(|tab| tab.id != entry.item.id) {
            changed |= entry.life != Life::Leaving;
            leaving.push((index, entry.clone()));
        }
    }
    if changed {
        // A new motion: what was still arriving has arrived, what was still
        // leaving is gone.
        let arriving: Vec<u64> = merged
            .iter()
            .filter(|entry| {
                entry.life == Life::Entering
                    && previous.iter().any(|old| old.item.id == entry.item.id)
            })
            .map(|entry| entry.item.id)
            .collect();
        for entry in &mut merged {
            if arriving.contains(&entry.item.id) {
                entry.life = Life::Live;
            }
        }
        leaving.retain(|(_, entry)| entry.life != Life::Leaving);
    }
    for (index, mut entry) in leaving {
        entry.life = Life::Leaving;
        merged.insert(index.min(merged.len()), entry);
    }
    *entries = merged;
    changed
}

/// The tabs on screen and the motion they are in.
#[derive(Default)]
struct Memory {
    entries: Vec<Entry>,
    /// Counts the motions; 0 until a tab has entered or left.
    epoch: u64,
    /// Whether the strip has drawn once, so its first tabs do not grow in.
    mounted: bool,
}

// ---- Drag to reorder, scrolling, and what the pointer is on ----------------

/// What a drag draws: which tab is held, how far each live tab is from its
/// place, and how far the held one is lifted. Idle is the default.
#[derive(Clone, Default, PartialEq)]
struct View {
    held: Option<usize>,
    offsets: Vec<f32>,
    /// The held tab is far enough from the strip that going on tears it off.
    armed: bool,
}

/// Everything a drag works on that is not drawn directly. It lives for the
/// life of the strip and handlers read it, so none of it re-renders.
struct Shared {
    reorder: RefCell<Reorder>,
    /// The tab a press went down on, until the pointer has travelled far
    /// enough for it to be picked up.
    pending: Cell<Option<usize>>,
    /// The ids of the live tabs, in order.
    ids: RefCell<Vec<u64>>,
    on_reorder: RefCell<Option<EventHandler<(u64, usize)>>>,
    /// One tab's body width, and the gap between bodies.
    tab: Cell<f32>,
    gap: Cell<f32>,
    /// How far the tabs are scrolled now (logical: from the start edge), and
    /// when the tab was picked up.
    offset: Cell<f32>,
    offset_start: Cell<f32>,
    /// The most the tabs scroll.
    range: Cell<f32>,
    /// The part of a pixel the strip has not scrolled yet.
    carry: Cell<f32>,
    /// Where the region of tabs is in the window, left and right.
    region: Cell<(f32, f32)>,
    /// Where the strip is in the window, top and bottom.
    bounds: Cell<(f32, f32)>,
    reduced: Cell<bool>,
    /// Where the pointer is against the tear thresholds, for this drag.
    gate: Cell<TearGate>,
    on_detach: RefCell<Option<EventHandler<Detach>>>,
    /// Which way the strip reads, and how wide its content is.
    direction: Cell<Direction>,
    content: Cell<f32>,
    /// The next move of the scroll target is a jump (a finger, a drag's edge,
    /// the first placing) and not a glide.
    jump: Cell<bool>,
    /// The bodies as the strip last drew them, window x, and the vertical
    /// band they fill: what the pointer is hit-tested against.
    spans: RefCell<Vec<(f32, f32, u64)>>,
    band: Cell<(f32, f32)>,
    /// Where the pointer last was in the strip's row, window x and y.
    pointer: Cell<Option<(f32, f32)>>,
    /// A finger that may be scrolling the strip: where it went down and where
    /// the tabs were then; and whether it has gone far enough to scroll.
    pan: Cell<Option<(f32, f32)>>,
    panned: Cell<bool>,
}

/// The handles every handler of one strip needs. Clone it into each.
#[derive(Clone)]
struct Env {
    shared: Rc<Shared>,
    drag: PointerDrag,
    frames: Frames,
    view: State<View>,
    /// Where the tabs are scrolled to (logical): the strip draws a glide to it.
    target: State<f32>,
    /// The tab the pointer is on, as the last pointer event saw it. It says the
    /// strip must draw again; what is drawn is worked out from where the pointer
    /// and the tabs are *now*.
    hover: State<Option<u64>>,
    /// The tab a press went down on that has not come up.
    pressed: State<Option<u64>>,
}

impl Env {
    fn busy(&self) -> bool {
        self.shared.reorder.borrow().is_busy()
    }

    // ---- Scrolling

    /// Scroll the tabs to the logical offset `to` (clamped to the range): at once
    /// with `jump`, else gliding. Returns whether they moved.
    fn scroll_to(&self, to: f32, jump: bool) -> bool {
        let to = tab_scroll::clamp(to, self.shared.range.get());
        let mut target = self.target;
        if (*target.peek() - to).abs() < 0.01 {
            return false;
        }
        self.shared.jump.set(jump);
        target.set(to);
        true
    }

    /// A wheel notch or a touchpad movement over the strip: the vertical wheel
    /// scrolls it along, a sideways movement moves it as the fingers did. A
    /// notch glides; a touchpad's pixels are followed at once. Nothing is taken
    /// when the strip has nowhere to go.
    fn wheel(&self, event: &Event<WheelEventData>) {
        if self.shared.range.get() <= 0.0 {
            return;
        }
        let delta = tab_scroll::wheel(event.delta_x, event.delta_y, self.shared.direction.get());
        if delta == 0.0 {
            return;
        }
        let from = *self.target.peek();
        let to = tab_scroll::clamp(from + delta, self.shared.range.get());
        if (to - from).abs() < 0.01 {
            return;
        }
        event.stop_propagation();
        self.scroll_to(to, event.source != WheelSource::Line);
    }

    /// Whether the window point `at` is on the region the tabs show in.
    fn in_region(&self, at: (f32, f32)) -> bool {
        let (left, right) = self.shared.region.get();
        let (top, bottom) = self.shared.bounds.get();
        at.0 >= left && at.0 < right && at.1 >= top && at.1 < bottom
    }

    /// A finger went down at `at`: it scrolls the strip if it goes on to move
    /// along it before it lifts.
    fn pan_down(&self, at: (f32, f32)) {
        let begins = self.shared.range.get() > 0.0 && self.in_region(at);
        self.shared.pan.set(begins.then(|| (at.0, self.shared.offset.get())));
        self.shared.panned.set(false);
    }

    /// The finger moved: past the slop, the tabs follow it.
    fn pan_move(&self, at: (f32, f32)) {
        let Some((from_x, from_offset)) = self.shared.pan.get() else {
            return;
        };
        let travelled = at.0 - from_x;
        if !self.shared.panned.get() && travelled.abs() < tab_scroll::PAN_SLOP {
            return;
        }
        self.shared.panned.set(true);
        let to = tab_scroll::pan(from_offset, travelled, self.shared.range.get(), self.shared.direction.get());
        self.scroll_to(to, true);
    }

    fn pan_end(&self) {
        self.shared.pan.set(None);
        self.shared.panned.set(false);
    }

    // ---- What the pointer is on

    /// The pointer is at `at` in the strip's row. No tab is hovered while one is
    /// pressed down for a drag: the drag has the pointer.
    fn pointer_at(&self, at: (f32, f32)) {
        self.shared.pointer.set(Some(at));
        let now = if self.drag.is_active_now() { None } else { tab_at(&self.shared.spans.borrow(), self.shared.band.get(), at) };
        let mut hover = self.hover;
        hover.set_if_modified(now);
    }

    /// The pointer left the strip's row.
    fn pointer_gone(&self) {
        self.shared.pointer.set(None);
        let mut hover = self.hover;
        hover.set_if_modified(None);
    }

    /// The press is over, wherever it went up.
    fn press_ended(&self) {
        let mut pressed = self.pressed;
        pressed.set_if_modified(None);
    }

    // ---- Dragging

    /// A press went down on live tab `index`: the drag begins when the
    /// pointer has gone far enough.
    fn arm(&self, index: usize, at: (f32, f32)) {
        let lone_is_dragged = self.tearable();
        if self.busy() || (self.shared.ids.borrow().len() < 2 && !lone_is_dragged) {
            return;
        }
        let (top, bottom) = self.shared.bounds.get();
        self.shared.gate.set(TearGate::new(bottom - top));
        self.shared.pending.set(Some(index));
        self.drag.begin_at(at, false);
        self.start_frames();
    }

    /// Pick the pressed tab up once the pointer has gone [`DRAG_SLOP`] from
    /// where it went down (sideways only, unless a vertical drag tears the
    /// tab off). A lone tab is not picked up: it is torn off to move its window.
    fn pick_if_moved(&self, released: bool) {
        let Some(index) = self.shared.pending.get() else {
            return;
        };
        if !self.drag.is_active_now() || self.busy() {
            return;
        }
        if self.shared.ids.borrow().len() < 2 {
            self.check_tear(released);
            return;
        }
        let (dx, dy) = self.drag.travelled();
        let moved = if self.tearable() { dx.hypot(dy) } else { dx.abs() };
        if moved >= DRAG_SLOP {
            self.shared.pending.set(None);
            self.pick(index);
        }
    }

    fn pick(&self, index: usize) {
        let count = self.shared.ids.borrow().len();
        let stack = Stack::new(vec![self.shared.tab.get(); count], self.shared.gap.get());
        self.shared.offset_start.set(self.shared.offset.get());
        self.shared.carry.set(0.0);
        self.shared.reorder.borrow_mut().pick(stack, index);
    }

    /// The pointer went up: let the tab settle, or put it back when the
    /// pointer is far from the strip. With a host that takes torn tabs, a tab
    /// let go far enough from the strip is torn off instead.
    fn release(&self, at: (f32, f32)) {
        // A drag that was quicker than a frame is still a drag.
        self.pick_if_moved(true);
        self.shared.pending.set(None);
        self.follow(0.0);
        // The drag took the release and the pointer's moves: where it went up is
        // where the pointer is, for the tab it is on now.
        self.shared.pointer.set(Some(at));
        self.press_ended();
        if self.check_tear(true) {
            return;
        }
        let (top, bottom) = self.shared.bounds.get();
        let reach = DRAG_REACH.px();
        if at.1 < top - reach || at.1 > bottom + reach {
            self.shared.reorder.borrow_mut().cancel();
        } else {
            self.shared.reorder.borrow_mut().drop();
        }
        self.drag.end();
        self.publish();
        self.start_frames();
    }

    /// Escape: take the tab home.
    fn cancel(&self) {
        self.shared.pending.set(None);
        self.shared.reorder.borrow_mut().cancel();
        self.press_ended();
        self.drag.end();
        self.publish();
        self.start_frames();
    }

    /// Whether this tab, dragged, is one that can leave: a host takes it, and
    /// the strip is a strip (a lone tab is dragged as a window).
    fn tearable(&self) -> bool {
        self.shared.on_detach.borrow().is_some()
    }

    /// Run the drag against the tear thresholds, and tear the tab off if it
    /// has gone far enough. `released` is whether the pointer is already up.
    /// Returns whether the tab was torn off.
    fn check_tear(&self, released: bool) -> bool {
        if !self.tearable() || !self.drag.is_active_now() {
            return false;
        }
        let (top, bottom) = self.shared.bounds.get();
        let mut gate = self.shared.gate.get();
        let lone = self.shared.ids.borrow().len() < 2;
        let id = if lone {
            self.shared.ids.borrow().first().copied()
        } else {
            let held = self.shared.reorder.borrow().held();
            held.and_then(|held| self.shared.ids.borrow().get(held).copied())
        };
        let Some(id) = id else {
            return false;
        };
        let pointer = self.drag.pointer();
        let pull = if lone {
            gate.update_lone(self.drag.travelled())
        } else if self.shared.reorder.borrow().is_held() {
            gate.update(tab_tear::away(pointer.1, top, bottom))
        } else {
            return false;
        };
        self.shared.gate.set(gate);
        if pull != Pull::Torn {
            return false;
        }
        let (press_x, press_y) = self.drag.start();
        let index = self.shared.ids.borrow().iter().position(|other| *other == id).unwrap_or(0);
        let scrolled = if lone { self.shared.offset.get() } else { self.shared.offset_start.get() };
        // The body's left edge in the window, from where the tabs were scrolled
        // when it was picked up, in the strip's own direction.
        let direction = self.shared.direction.get();
        let start = index as f32 * (self.shared.tab.get() + self.shared.gap.get());
        let body_left = self.shared.region.get().0
            - tab_scroll::physical(scrolled, self.shared.range.get(), direction)
            + tab_scroll::place(start, self.shared.tab.get(), self.shared.content.get(), direction);
        let detach = Detach {
            id,
            pointer,
            press: (press_x, press_y),
            grab: (press_x - body_left, press_y - top),
            released,
        };
        // The strip's drag is over: the tab goes back to its place (the host
        // takes it out of the list when it likes) and nothing is held.
        self.shared.pending.set(None);
        self.shared.reorder.borrow_mut().cancel();
        self.shared.gate.set(TearGate::new(bottom - top));
        self.press_ended();
        self.drag.end();
        self.publish();
        self.start_frames();
        let handler = self.shared.on_detach.borrow().clone();
        if let Some(handler) = handler {
            handler.call(detach);
        }
        true
    }

    /// Carry the held tab to where the pointer is, and scroll the strip when
    /// the pointer is at the edge of the region. `dt` is the seconds since the
    /// last frame.
    fn follow(&self, dt: f32) {
        if !self.shared.reorder.borrow().is_held() {
            return;
        }
        let direction = self.shared.direction.get();
        let (pointer, _) = self.drag.pointer();
        let (left, right) = self.shared.region.get();
        // The edge's speed is on the screen: towards the right edge is forwards in
        // a left-to-right strip and backwards in a right-to-left one.
        let speed = reorder::edge_speed(pointer, left, right) * direction.sign();
        if speed != 0.0 && dt > 0.0 {
            let want = self.shared.carry.get() + speed * dt;
            let whole = want.trunc();
            self.shared.carry.set(want - whole);
            if whole != 0.0 {
                let next = tab_scroll::clamp(self.shared.offset.get() + whole, self.shared.range.get());
                self.shared.offset.set(next);
                self.scroll_to(next, true);
            }
        }
        // Distances are in the content, along the reading direction: the strip
        // scrolling under the pointer moves the held tab with it.
        let scrolled = self.shared.offset.get() - self.shared.offset_start.get();
        let delta = direction.sign() * (pointer - self.drag.start().0) + scrolled;
        let count = self.shared.ids.borrow().len();
        let step = self.shared.tab.get() + self.shared.gap.get();
        let mut model = self.shared.reorder.borrow_mut();
        let from = model.held().unwrap_or(0);
        let low = -(from as f32) * step;
        let high = (count.saturating_sub(1 + from)) as f32 * step;
        model.drag(delta.clamp(low, high));
    }

    /// One frame: pick the tab up once the pointer has moved, follow the
    /// pointer, step the springs, draw, and finish what came to rest. Returns
    /// whether there is more to do.
    fn frame(&self, dt: f32) -> bool {
        self.pick_if_moved(false);
        if !self.drag.is_active_now() {
            self.shared.pending.set(None);
        }
        self.follow(dt);
        self.check_tear(false);
        let done = self
            .shared
            .reorder
            .borrow_mut()
            .step(dt, self.shared.reduced.get());
        self.publish();
        if let Some(done) = done {
            self.finish(done);
        }
        self.busy() || self.shared.pending.get().is_some()
    }

    fn finish(&self, done: Done) {
        let Done::Moved { from, to } = done else {
            return;
        };
        let id = self.shared.ids.borrow().get(from).copied();
        let handler = self.shared.on_reorder.borrow().clone();
        if let (Some(id), Some(handler)) = (id, handler) {
            handler.call((id, to));
        }
    }

    /// Write what is drawn, if it changed: the one state write of a frame.
    fn publish(&self) {
        let model = self.shared.reorder.borrow();
        let count = self.shared.ids.borrow().len();
        let next = if model.is_busy() {
            View {
                held: model.held(),
                offsets: (0..count).map(|index| model.offset(index)).collect(),
                armed: self.shared.gate.get().pull() == Pull::Armed,
            }
        } else {
            View::default()
        };
        drop(model);
        let mut view = self.view;
        view.set_if_modified(next);
    }

    /// Run the frame clock until everything is at rest. With reduced motion
    /// the settling finishes in the same call.
    fn start_frames(&self) {
        if self.shared.reduced.get() && !self.shared.reorder.borrow().is_held() && self.shared.pending.get().is_none() {
            while self.busy() {
                self.frame(0.0);
            }
            return;
        }
        let env = self.clone();
        self.frames.run(move |dt| env.frame(dt));
    }
}

// ---- The strip -------------------------------------------------------------

/// What a tab's menu items call.
struct MenuHandlers {
    close: EventHandler<u64>,
    close_many: Option<EventHandler<Vec<u64>>>,
    duplicate: Option<EventHandler<u64>>,
    reopen: Option<EventHandler<()>>,
    reorder: Option<EventHandler<(u64, usize)>>,
}

/// The menu of the tab `id`, which is at `place` among `ids`.
fn tab_menu(id: u64, place: usize, ids: &[u64], offers: Offers, handlers: &MenuHandlers) -> Vec<MenuItem> {
    tab_close::menu_rows(place, ids.len(), offers)
        .into_iter()
        .map(|row| match row {
            Row::Separator => menu_separator(),
            Row::Item { act, enabled } => {
                let handlers = MenuHandlers {
                    close: handlers.close.clone(),
                    close_many: handlers.close_many.clone(),
                    duplicate: handlers.duplicate.clone(),
                    reopen: handlers.reopen.clone(),
                    reorder: handlers.reorder.clone(),
                };
                let ids = ids.to_vec();
                let item = menu_item(act.label(), move |_| match act {
                    Act::Close => handlers.close.call(id),
                    Act::CloseOthers => {
                        if let Some(close) = &handlers.close_many {
                            close.call(tab_close::others(&ids, id));
                        }
                    }
                    Act::CloseToTheLeft => {
                        if let Some(close) = &handlers.close_many {
                            close.call(tab_close::to_the_left(&ids, id));
                        }
                    }
                    Act::CloseToTheRight => {
                        if let Some(close) = &handlers.close_many {
                            close.call(tab_close::to_the_right(&ids, id));
                        }
                    }
                    Act::Duplicate => {
                        if let Some(duplicate) = &handlers.duplicate {
                            duplicate.call(id);
                        }
                    }
                    Act::Reopen => {
                        if let Some(reopen) = &handlers.reopen {
                            reopen.call(());
                        }
                    }
                    Act::MoveLeft => {
                        if let Some(reorder) = &handlers.reorder {
                            reorder.call((id, place.saturating_sub(1)));
                        }
                    }
                    Act::MoveRight => {
                        if let Some(reorder) = &handlers.reorder {
                            reorder.call((id, place + 1));
                        }
                    }
                });
                let item = match act.shortcut() {
                    Some(shortcut) => item.shortcut(shortcut),
                    None => item,
                };
                if enabled { item } else { item.disabled() }
            }
        })
        .collect()
}

impl Component for TabStrip {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let direction = use_direction();
        let rtl = direction.is_rtl();
        let geometry = Geometry::current();
        let mut available = use_state(|| None::<f32>);
        let mut rail_area = use_state(Area::default);
        let mut region_area = use_state(Area::default);
        let mut button_area = use_state(Area::default);
        let hover = use_state(|| None::<u64>);
        let pressed = use_state(|| None::<u64>);
        let target = use_state(|| 0.0_f32);
        let seen = use_state(|| 0_u64);
        let tick = use_state(|| 0_u64);
        let overlays = use_try_consume::<Overlays>();
        let memory = use_hook(|| Rc::new(RefCell::new(Memory::default())));
        // Whether the person closed a tab from the keyboard, so the focus can
        // follow the selection to the tab that takes its place.
        let refocus = use_hook(|| Rc::new(Cell::new(false)));
        // One accessibility id per tab, kept for the life of the strip, so
        // the arrow keys can move focus to a neighbouring tab.
        let ids = use_hook(|| Rc::new(RefCell::new(HashMap::<u64, AccessibilityId>::new())));
        let drag = use_pointer_drag();
        let frames = use_frames();
        let view = use_state(View::default);
        // Rapid closure: the widths held for the pointer, on a clock the
        // strip reads (the machine reads none), and the animation that
        // redraws the strip while they grow back.
        let rapid = use_hook(|| Rc::new(RefCell::new(RapidClose::default())));
        let started = use_hook(Instant::now);
        // The tab the host was last told the pointer is on, and what the
        // scroll-into-view last did, so each change is reported once.
        let told = use_hook(|| Rc::new(Cell::new(None::<u64>)));
        let placed = use_hook(|| Rc::new(Cell::new(false)));
        let last_view = use_hook(|| Rc::new(Cell::new(None::<usize>)));
        let shared = use_hook(|| {
            Rc::new(Shared {
                reorder: RefCell::new(Reorder::new(Indicate::Slide)),
                pending: Cell::new(None),
                ids: RefCell::new(Vec::new()),
                on_reorder: RefCell::new(None),
                tab: Cell::new(0.0),
                gap: Cell::new(0.0),
                offset: Cell::new(0.0),
                offset_start: Cell::new(0.0),
                range: Cell::new(0.0),
                carry: Cell::new(0.0),
                region: Cell::new((0.0, 0.0)),
                bounds: Cell::new((0.0, 0.0)),
                reduced: Cell::new(false),
                gate: Cell::new(TearGate::new(1.0)),
                on_detach: RefCell::new(None),
                direction: Cell::new(Direction::Ltr),
                content: Cell::new(0.0),
                jump: Cell::new(true),
                spans: RefCell::new(Vec::new()),
                band: Cell::new((0.0, 0.0)),
                pointer: Cell::new(None),
                pan: Cell::new(None),
                panned: Cell::new(false),
            })
        });
        let _ = tick();
        // The strip draws again when the pointer's tab changes; what it draws
        // is worked out below from where the pointer and the tabs are now.
        let _ = hover();
        shared.reduced.set(tokens.reduced_motion);
        shared.direction.set(direction);
        *shared.on_reorder.borrow_mut() = self.on_reorder.clone();
        *shared.on_detach.borrow_mut() = self.on_detach.clone();

        let entries = {
            let mut memory = memory.borrow_mut();
            let animate = memory.mounted;
            memory.mounted = true;
            if merge(&mut memory.entries, &self.tabs, animate) {
                memory.epoch += 1;
            }
            memory.entries.clone()
        };
        let epoch = memory.borrow().epoch;
        let a11y_ids: Vec<AccessibilityId> = {
            let mut map = ids.borrow_mut();
            map.retain(|id, _| entries.iter().any(|entry| entry.item.id == *id));
            entries
                .iter()
                .map(|entry| {
                    *map.entry(entry.item.id)
                        .or_insert_with(AccessibilityId::new_unique)
                })
                .collect()
        };

        // The motion's clock runs from 0 to 1 once for each epoch. Until the
        // effect below has started it for this epoch, an arriving tab has no
        // width and a leaving one all of it.
        let motion = tokens.motion(MOVE_MS).max(1);
        let clock = use_animation(move |_| {
            AnimNum::new(0.0, 1.0)
                .time(motion)
                .function(Function::Quad)
                .ease(Ease::Out)
        });
        let growing = use_animation(move |_| {
            AnimNum::new(0.0, 1.0)
                .time(motion)
                .function(Function::Quad)
                .ease(Ease::Out)
        });
        // The widths read the clock when they are drawn, so the strip is
        // drawn again on every frame of the growth.
        let _ = growing.get().value();
        {
            let memory = memory.clone();
            use_side_effect_with_deps(&epoch, move |&epoch: &u64| {
                if epoch == 0 {
                    return;
                }
                let mut clock = clock;
                let mut seen = seen;
                let mut tick = tick;
                clock.start();
                seen.set(epoch);
                let memory = memory.clone();
                spawn(async move {
                    timer(Duration::from_millis(motion + 40)).await;
                    let mut memory = memory.borrow_mut();
                    if memory.epoch != epoch {
                        return;
                    }
                    memory.entries.retain(|entry| entry.life != Life::Leaving);
                    for entry in &mut memory.entries {
                        entry.life = Life::Live;
                    }
                    drop(memory);
                    let next = tick.peek().wrapping_add(1);
                    tick.set(next);
                });
            });
        }
        let progress = if seen() == epoch {
            clock.get().value()
        } else {
            0.0
        };

        let add = self.on_new.is_some();
        let staying = entries
            .iter()
            .filter(|entry| entry.life != Life::Leaving)
            .count();
        let effective: f32 = entries.iter().map(|entry| entry.presence(progress)).sum();
        let natural = fit(
            geometry,
            available().unwrap_or(Dim::Page.px()),
            staying,
            effective,
            add,
        );
        // After a tab was closed with the pointer the tabs keep their width
        // until the pointer leaves the strip, then grow to the room.
        let reaction = Duration::from_millis(tokens.motion(MOVE_MS));
        let held = rapid.borrow().tab_width(natural.tab, started.elapsed(), reaction);
        let fit = natural.held_at(geometry, held, effective, add);
        let content = geometry.content(effective, fit.tab).max(0.0);
        let range = (content - fit.region).max(0.0);
        shared.range.set(range);
        shared.content.set(content);
        // How far the tabs are scrolled, from the strip's start edge: it glides to
        // where the wheel, the selection or a finger wants it, or jumps there when
        // the cause is a drag of the pointer, a finger or the first placing.
        let glide = if shared.jump.replace(false) { 0 } else { tokens.motion(MOVE_MS) };
        let offset = tab_scroll::clamp(use_tween(target(), glide), range);
        // And on the screen: from the left edge of the region, which a strip that
        // reads from the right counts from the other end.
        let scrolled = tab_scroll::physical(offset, range, direction);
        shared.tab.set(fit.tab);
        shared.gap.set(geometry.gap());
        if !drag.is_active() {
            shared.offset.set(offset);
        }
        let region_bounds = *region_area.peek();
        shared.region.set((region_bounds.min_x(), region_bounds.max_x()));
        let rail_bounds = *rail_area.peek();
        shared.bounds.set((rail_bounds.min_y(), rail_bounds.max_y()));
        // The window x of the content's left edge.
        let origin = region_bounds.min_x() - scrolled;

        // Scroll the selected tab into view when the selection moves, and again
        // when the tabs have settled, because the content only has its final
        // width then. It does not read what it writes: the target is peeked.
        let selected_index = entries
            .iter()
            .position(|entry| entry.item.id == self.selected && entry.life != Life::Leaving);
        {
            let (shared, placed, last_view) = (shared.clone(), placed.clone(), last_view.clone());
            let deps = (
                selected_index,
                fit.tab.round() as i32,
                fit.region.round() as i32,
                fit.overflow,
                tick(),
            );
            use_side_effect_with_deps(&deps, move |deps| {
                let (index, tab, region, overflow, _) = *deps;
                let Some(index) = index else { return };
                if !overflow {
                    return;
                }
                // The first placing and a width settling go straight there; a change
                // of selection glides.
                let first = !placed.replace(true);
                let unchanged = last_view.replace(Some(index)) == Some(index);
                let mut target = target;
                let to = tab_scroll::into_view(index, geometry, tab as f32, region as f32, *target.peek());
                let to = tab_scroll::clamp(to, shared.range.get());
                if (*target.peek() - to).abs() >= 0.01 {
                    shared.jump.set(first || unchanged);
                    target.set(to);
                }
            });
        }
        // After a close from the keyboard, the selection's new tab takes the focus.
        {
            let wants = refocus.clone();
            let deps = (self.selected, selected_index.map(|index| a11y_ids[index]));
            use_side_effect_with_deps(&deps, move |&(_, focus_id): &(u64, Option<AccessibilityId>)| {
                if wants.replace(false) {
                    if let Some(focus_id) = focus_id {
                        focus_id.request_focus();
                    }
                }
            });
        }

        let live: Vec<&Entry> = entries
            .iter()
            .filter(|entry| entry.life != Life::Leaving)
            .collect();
        let live_ids: Vec<u64> = live.iter().map(|entry| entry.item.id).collect();
        *shared.ids.borrow_mut() = live_ids.clone();
        let reorderable = self.on_reorder.is_some();
        let env = Env {
            shared: shared.clone(),
            drag: drag.clone(),
            frames,
            view,
            target,
            hover,
            pressed,
        };
        let current_view = view.read().clone();

        // A close with the pointer holds the widths where they are drawn now.
        let pointer_close = {
            let rapid = rapid.clone();
            let on_close = self.on_close.clone();
            let shown = fit.tab;
            EventHandler::new(move |id: u64| {
                rapid.borrow_mut().closed_with_pointer(shown);
                on_close.call(id);
            })
        };
        let offers = Offers {
            close_many: self.on_close_many.is_some(),
            duplicate: self.on_duplicate.is_some(),
            reopen: self.on_reopen.is_some(),
            can_reopen: self.can_reopen,
            reorder: reorderable,
        };
        let menu_handlers = MenuHandlers {
            close: self.on_close.clone(),
            close_many: self.on_close_many.clone(),
            duplicate: self.on_duplicate.clone(),
            reopen: self.on_reopen.clone(),
            reorder: self.on_reorder.clone(),
        };

        // ---- Where every tab is: its body's start in the content (reading order)
        // and its width, and the body on the screen. The tab the pointer is on is
        // the one whose body it is over now, wherever the tabs have moved to under
        // a pointer that did not: scrolling, a tab closing, a rapid close.
        let mut x = 0.0;
        let mut placement: Vec<(f32, f32)> = Vec::with_capacity(entries.len());
        let mut spans: Vec<(f32, f32)> = Vec::with_capacity(entries.len());
        let mut bodies: Vec<(f32, f32, u64)> = Vec::with_capacity(entries.len());
        for entry in &entries {
            let presence = entry.presence(progress);
            let body = fit.tab * presence;
            placement.push((x, body));
            if entry.life != Life::Leaving {
                let left = origin + tab_scroll::place(x, body, content, direction);
                spans.push((left, left + body));
                bodies.push((left, left + body, entry.item.id));
            }
            x += (fit.tab + geometry.gap()) * presence;
        }
        let band = (rail_bounds.min_y() + geometry.top, rail_bounds.min_y() + geometry.top + geometry.tab);
        shared.band.set(band);
        let under = if drag.is_active() || current_view.held.is_some() {
            None
        } else {
            shared.pointer.get().and_then(|at| tab_at(&bodies, band, at))
        };
        *shared.spans.borrow_mut() = bodies;
        // The host hears of each change of the tab the pointer is on, once.
        {
            let told = told.clone();
            let on_hover = self.on_hover.clone();
            use_side_effect_with_deps(&under, move |&now: &Option<u64>| {
                let before = told.replace(now);
                if before == now {
                    return;
                }
                if let Some(before) = before {
                    on_hover.call((before, false));
                }
                if let Some(now) = now {
                    on_hover.call((now, true));
                }
            });
        }

        // ---- Tabs
        let mut row_children: Vec<Element> = Vec::with_capacity(entries.len());
        let mut held_child: Option<Element> = None;
        let pressed_now = pressed();
        for (position, entry) in entries.iter().enumerate() {
            let (start, body) = placement[position];
            let presence = entry.presence(progress);
            let id = entry.item.id;
            let place = live_ids.iter().position(|live| *live == id);
            let slide = place
                .and_then(|place| current_view.offsets.get(place).copied())
                .unwrap_or(0.0);
            let held = place.is_some() && place == current_view.held;
            let menu = match place {
                Some(place) if entry.life != Life::Leaving => {
                    tab_menu(id, place, &live_ids, offers, &menu_handlers)
                }
                _ => Vec::new(),
            };
            // Its place in the content in reading order: the engine mirrors an absolute
            // `left` of a box that reads from the right, a drag's slide included. A tab
            // dragged along the reading direction is dragged the other way on the
            // screen when the strip reads from the right.
            let left = start + slide;
            let is_selected = id == self.selected;
            let tab = Element::from(Tab {
                item: entry.item.clone(),
                geometry,
                width: body,
                left,
                presence,
                selected: is_selected,
                held,
                armed: held && current_view.armed,
                drop_target: self.drop_target == Some(id),
                hover: under == Some(id),
                pressed: pressed_now == Some(id),
                menu,
                on_select: self.on_select.clone(),
                on_close: pointer_close.clone(),
                on_down: {
                    let mut pressed = pressed;
                    EventHandler::new(move |tab: u64| pressed.set_if_modified(Some(tab)))
                },
                on_arm: ((reorderable || self.on_detach.is_some()) && place.is_some()).then(|| {
                    let env = env.clone();
                    EventHandler::new(move |(_, at): (u64, (f32, f32))| {
                        if let Some(place) = place {
                            env.arm(place, at);
                        }
                    })
                }),
                a11y_id: a11y_ids[position],
            });
            if held {
                held_child = Some(tab);
            } else {
                row_children.push(tab);
            }
        }
        // The tab in hand is drawn last, over the ones that slide aside.
        row_children.extend(held_child);

        // Where a tab brought from another window would land: the boundary
        // between the tabs it would stand between, in the row's own x.
        let marker = self.insertion.and_then(|at| {
            StripLayout { rail: (0.0, 0.0, 0.0, 0.0), spans: spans.clone(), rtl }
                .boundary(at)
                .map(|x| x - origin)
        });
        // Where the bodies are in the window, for a host that asks where a tab
        // brought from another window would land.
        if let Some(probe) = &self.probe {
            probe.set(StripLayout {
                rail: (rail_bounds.min_x(), rail_bounds.min_y(), rail_bounds.max_x(), rail_bounds.max_y()),
                spans,
                rtl,
            });
        }

        let key_ids: Vec<(u64, AccessibilityId)> = entries
            .iter()
            .zip(&a11y_ids)
            .filter(|(entry, _)| entry.life != Life::Leaving)
            .map(|(entry, a11y)| (entry.item.id, *a11y))
            .collect();
        let (on_select, on_close, on_new, on_reorder, on_reopen, can_reopen) = (
            self.on_select.clone(),
            self.on_close.clone(),
            self.on_new.clone(),
            self.on_reorder.clone(),
            self.on_reopen.clone(),
            self.can_reopen,
        );
        let selected = self.selected;
        let wants_focus = refocus.clone();
        let new_rapid = rapid.clone();
        let row = rect()
            .height(Size::px(geometry.strip))
            .width(Size::px(content))
            .on_key_down(move |event: Event<KeyboardEventData>| {
                let Some(index) = key_ids.iter().position(|(id, _)| *id == selected) else {
                    return;
                };
                let last = key_ids.len() - 1;
                let ctrl = event.modifiers.contains(Modifiers::CONTROL);
                let shift = event.modifiers.contains(Modifiers::SHIFT);
                // The arrows are the screen's: the key towards the strip's start is
                // the one that goes back, which is Right when the strip reads from the right.
                let (back, forward) = if rtl {
                    (NamedKey::ArrowRight, NamedKey::ArrowLeft)
                } else {
                    (NamedKey::ArrowLeft, NamedKey::ArrowRight)
                };
                let next = match &event.key {
                    Key::Named(arrow @ (NamedKey::ArrowLeft | NamedKey::ArrowRight)) if ctrl && shift => {
                        event.stop_propagation();
                        let to = if *arrow == back {
                            index.checked_sub(1)
                        } else {
                            (index < last).then_some(index + 1)
                        };
                        if let (Some(to), Some(on_reorder)) = (to, &on_reorder) {
                            on_reorder.call((selected, to));
                        }
                        return;
                    }
                    Key::Named(arrow) if *arrow == back => index.saturating_sub(1),
                    Key::Named(arrow) if *arrow == forward => (index + 1).min(last),
                    Key::Named(NamedKey::Home) => 0,
                    Key::Named(NamedKey::End) => last,
                    Key::Named(NamedKey::Delete) => {
                        event.stop_propagation();
                        wants_focus.set(true);
                        on_close.call(selected);
                        return;
                    }
                    Key::Character(text) if ctrl && !shift && text.eq_ignore_ascii_case("w") => {
                        event.stop_propagation();
                        wants_focus.set(true);
                        on_close.call(selected);
                        return;
                    }
                    Key::Character(text) if ctrl && !shift && text.eq_ignore_ascii_case("t") => {
                        event.stop_propagation();
                        if let Some(on_new) = &on_new {
                            new_rapid.borrow_mut().reset();
                            wants_focus.set(true);
                            on_new.call(());
                        }
                        return;
                    }
                    Key::Character(text) if ctrl && shift && text.eq_ignore_ascii_case("t") => {
                        if let (Some(on_reopen), true) = (&on_reopen, can_reopen) {
                            event.stop_propagation();
                            wants_focus.set(true);
                            on_reopen.call(());
                        }
                        return;
                    }
                    _ => return,
                };
                event.stop_propagation();
                if next != index {
                    on_select.call(key_ids[next].0);
                }
                key_ids[next].1.request_focus();
            })
            .children(row_children)
            .maybe_child(marker.map(|at| {
                let width = Dim::Indicator.px();
                rect()
                    .key("insertion")
                    .component("TabInsertion")
                    .position(Position::new_absolute().top(geometry.top).left(tab_scroll::place((at - width / 2.0).clamp(0.0, (content - width).max(0.0)), width, content, direction)))
                    .layer(Layer::Relative(MARKER_LAYER))
                    .interactive(false)
                    .height(Size::px(geometry.tab))
                    .tw(&tokens, &[w(Dim::Indicator), rounded(Radius::Mark), bg(Paint::Accent)])
            }));

        // ---- The region the tabs scroll in, with fades where they continue
        // A fade is written at the start or the end edge as for left to right; the
        // engine puts it on the right edge, and draws its gradient the other way
        // round, when the strip reads from the right.
        let fade_width = Dim::Box5.px();
        let fade_height = geometry.strip;
        let fade = |at_start: bool| {
            let solid = BAR.color(&tokens);
            let (from, to) = if at_start {
                (solid, solid.with_a(0))
            } else {
                (solid.with_a(0), solid)
            };
            let position = if at_start {
                Position::new_absolute().top(0.0).left(0.0)
            } else {
                Position::new_absolute().top(0.0).right(0.0)
            };
            gradient_fill(
                rect()
                    .position(position)
                    .interactive(false)
                    .width(Size::px(fade_width))
                    .height(Size::px(fade_height)),
                LinearGradient::new()
                    .angle(270.0)
                    .stop(GradientStop::new(from, 0.0))
                    .stop(GradientStop::new(to, 100.0)),
            )
        };
        // The fades need room of their own: a region narrower than two of them has none.
        let fades = fit.overflow && fit.region >= 2.0 * Dim::Box5.px();
        let scroller = rect()
            .width(Size::px(content))
            .height(Size::px(geometry.strip))
            .offset_x(-offset)
            .child(row);
        let region = rect()
            .tw(&tokens, &[clip()])
            .height(Size::px(geometry.strip))
            .width(Size::px(fit.region))
            .on_sized(move |event: Event<SizedEventData>| {
                region_area.set_if_modified(event.area);
            })
            .child(scroller)
            .maybe_child((fades && offset > 0.5).then(|| fade(true)))
            .maybe_child((fades && offset < range - 0.5).then(|| fade(false)));

        // ---- The buttons at the end of the strip, in one cluster as tall as the
        // strip. Each is a square button, a tab tall, with the corner of a tab
        // (the default of an icon button); the cluster has the inset before and
        // after them, so a button is the same distance from the strip's top and
        // bottom, from the last tab and from the strip's trailing edge.
        let all_tabs: Vec<MenuItem> = live
            .iter()
            .map(|entry| {
                let on_select = self.on_select.clone();
                let id = entry.item.id;
                // The tab list shows what the tab does: its icon, then its name, which
                // is cut in the middle (its end is what tells two tabs apart) with the
                // whole name as the tooltip; a tick marks the selected tab.
                let item = menu_item(entry.item.title.clone(), move |_| on_select.call(id))
                    .name()
                    .checked(id == self.selected);
                match entry.item.icon {
                    Some(icon) => item.icon(icon),
                    None => item,
                }
            })
            .collect();
        let mut buttons: Vec<Element> = Vec::with_capacity(2);
        if fit.overflow {
            buttons.push(
                rect()
                    .on_sized(move |event: Event<SizedEventData>| {
                        button_area.set_if_modified(event.area);
                    })
                    .child(
                        icon_button(Icon::PanDown, move |_| {
                            if let Some(overlays) = overlays {
                                overlays.show_menu_from(
                                    None,
                                    PanelTrigger::Control(PanelRect::from_area(&button_area.read())),
                                    all_tabs.clone(),
                                    false,
                                );
                            }
                        })
                        .size(Dim::TabButton)
                        .tooltip("Tab list"),
                    )
                    .into_element(),
            );
        }
        if let Some(on_new) = self.on_new.clone() {
            let rapid = rapid.clone();
            buttons.push(
                icon_button(Icon::ListAdd, move |_: ()| {
                    rapid.borrow_mut().reset();
                    on_new.call(());
                })
                .size(Dim::TabButton)
                .tooltip("New tab")
                .into_element(),
            );
        }
        let cluster = (!buttons.is_empty()).then(|| {
            let count = buttons.len();
            rect()
                .component("TabEnd")
                .width(Size::px(geometry.cluster(count)))
                .height(Size::px(geometry.strip))
                .tw(&tokens, &[flex_row(), items(Align::Center), px(FIELD_INSET)])
                // The tab list, then the New tab button at the trailing edge; the other
                // way round when the strip reads from the right.
                .children(buttons)
                .into_element()
        });

        // ---- Drag to reorder: the strip tracks the pointer while one is on.
        let release = env.clone();
        let cancel = env.clone();
        let mut ends = vec![Element::from(region)];
        ends.extend(cluster);
        let strip = rect()
            .component("TabStrip")
            .a11y_role(AccessibilityRole::TabList)
            .a11y_alt("Tabs")
            .width(Size::px(fit.rail))
            .height(Size::px(geometry.strip))
            .tw(&tokens, &[flex_row()])
            .on_sized(move |event: Event<SizedEventData>| {
                rail_area.set_if_modified(event.area);
            })
            // The tabs, then what follows the last of them: the other way round on the
            // screen when the strip reads from the right.
            .children(ends);
        let strip = drag.track(strip, move |at| release.release(at), move || cancel.cancel());

        // The pointer in the strip's whole row, which is wider than the tabs: the
        // tabs keep their width until it leaves, and then grow.
        let track = {
            let env = env.clone();
            move |event: Event<PointerEventData>| {
                let at = event.global_location();
                env.pointer_at((at.x as f32, at.y as f32));
            }
        };
        let enter = {
            let rapid = rapid.clone();
            move |_: Event<PointerEventData>| rapid.borrow_mut().pointer_in_strip(true, started.elapsed())
        };
        let leave = {
            let rapid = rapid.clone();
            let env = env.clone();
            let mut growing = growing;
            let mut tick = tick;
            move |_: Event<PointerEventData>| {
                env.pointer_gone();
                let now = started.elapsed();
                if !rapid.borrow().is_held() {
                    return;
                }
                rapid.borrow_mut().pointer_in_strip(false, now);
                if rapid.borrow().is_growing(now, reaction) {
                    growing.start();
                    let rapid = rapid.clone();
                    spawn(async move {
                        timer(reaction + Duration::from_millis(40)).await;
                        if rapid.borrow_mut().settle(started.elapsed(), reaction) {
                            let next = tick.peek().wrapping_add(1);
                            tick.set(next);
                        }
                    });
                } else {
                    rapid.borrow_mut().settle(now, reaction);
                    let next = tick.peek().wrapping_add(1);
                    tick.set(next);
                }
            }
        };
        // The wheel over the strip scrolls the tabs; a finger dragged along them
        // does too, and a press that ends anywhere ends the tab's pressed look.
        let wheel = env.clone();
        let (pan_down, pan_move, pan_up) = (env.clone(), env.clone(), env.clone());
        rect()
            .tw(&tokens, &[w_full()])
            .height(Size::px(geometry.strip))
            .on_sized(move |event: Event<SizedEventData>| {
                available.set_if_modified(Some(event.area.width()));
            })
            .on_pointer_enter(enter)
            .on_pointer_move(track)
            .on_pointer_leave(leave)
            .on_wheel(move |event: Event<WheelEventData>| wheel.wheel(&event))
            .on_global_pointer_down(move |event: Event<PointerEventData>| {
                if matches!(event.data(), PointerEventData::Touch(_)) {
                    let at = event.global_location();
                    pan_down.pan_down((at.x as f32, at.y as f32));
                }
            })
            .on_global_pointer_move(move |event: Event<PointerEventData>| {
                if pan_move.shared.pan.get().is_some() {
                    let at = event.global_location();
                    pan_move.pan_move((at.x as f32, at.y as f32));
                }
            })
            .on_global_pointer_up(move |_| {
                pan_up.pan_end();
                pan_up.press_ended();
            })
            .child(strip)
    }
}

/// The place of tab `id` among the live tabs.
fn place_of(live_ids: &[u64], id: u64) -> Option<usize> {
    live_ids.iter().position(|live| *live == id)
}

/// The tab whose body the pointer at `at` (window x, y) is on: `spans` are the
/// bodies' left and right edges and their ids, and `band` is the vertical
/// range a body occupies. The pointer in the band above the tabs, which drags
/// the window, or in the gap between two bodies, is on none.
fn tab_at(spans: &[(f32, f32, u64)], band: (f32, f32), at: (f32, f32)) -> Option<u64> {
    if at.1 < band.0 || at.1 >= band.1 {
        return None;
    }
    spans.iter().find(|(left, right, _)| at.0 >= *left && at.0 < *right).map(|span| span.2)
}

// ---- A tab -----------------------------------------------------------------

/// What fills a tab: nothing, the tint of a drop target, the selection fill
/// (the one of a sidebar's selected row), the pressed fill, or the hover fill
/// at some strength (0 to 1, easing in and out). The states a tab can be in
/// each look different: selected (the selection fill), hovered (a fill that
/// fades in), pressed (a stronger one, at once) and held by a drag (lifted,
/// over its neighbours; faint once going on would tear it off).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Wash {
    None,
    Drop,
    Selected,
    Pressed,
    Hover(f32),
}

fn wash(selected: bool, drop_target: bool, pressed: bool, hover: f32) -> Wash {
    if drop_target {
        Wash::Drop
    } else if selected {
        Wash::Selected
    } else if pressed {
        Wash::Pressed
    } else if hover > 0.0 {
        Wash::Hover(hover.min(1.0))
    } else {
        Wash::None
    }
}

/// One tab: a rounded rectangle of [`Radius::Control`], the corner of a button
/// and of a sidebar row, [`FIELD_INSET`] from the strip's top and bottom. The
/// slot is placed by the strip, `left` from the start edge of the tabs'
/// content; the body in it is the control that takes the pointer and the focus.
#[derive(PartialEq)]
struct Tab {
    item: TabItem,
    geometry: Geometry,
    /// The body's width at this moment of the motion.
    width: f32,
    /// The slot's start edge in the strip's content, in reading order (the engine
    /// puts an absolute `left` of a box that reads from the right on the right).
    left: f32,
    /// How much of the tab is there, for a tab that is arriving or leaving.
    presence: f32,
    selected: bool,
    /// The tab is in the hand of a drag.
    held: bool,
    /// The held tab is far enough from the strip that going on tears it off:
    /// it is drawn dimmer, so the person knows it will leave.
    armed: bool,
    drop_target: bool,
    /// The pointer is on this tab. The strip decides, from where the pointer is
    /// and where the tabs are, so exactly one tab is hovered, however the tabs
    /// move under a pointer that does not.
    hover: bool,
    /// The press that went down on this tab is still down.
    pressed: bool,
    /// Its right click and long press menu.
    menu: Vec<MenuItem>,
    on_select: EventHandler<u64>,
    /// The tab was closed with the pointer (its button or a middle click).
    on_close: EventHandler<u64>,
    /// A press went down on the tab.
    on_down: EventHandler<u64>,
    /// A press with the primary mouse button went down at this window point;
    /// set when the strip reorders.
    on_arm: Option<EventHandler<(u64, (f32, f32))>>,
    a11y_id: AccessibilityId,
}

impl Component for Tab {
    /// A tab keeps its hover and focus when another one before it closes.
    fn render_key(&self) -> DiffKey {
        DiffKey::U64(self.item.id)
    }

    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let geometry = self.geometry;
        let mut area = use_state(Area::default);
        let a11y_id = self.a11y_id;
        let focus = use_focus(a11y_id);
        let press = use_press();
        let overlays = use_try_consume::<Overlays>();
        let id = self.item.id;
        let (on_select, on_close, on_down) = (self.on_select.clone(), self.on_close.clone(), self.on_down.clone());
        let on_arm = self.on_arm.clone();
        let open_menu: EventHandler<()> = {
            let items = self.menu.clone();
            EventHandler::new(move |_: ()| {
                if let (Some(overlays), false) = (overlays, items.is_empty()) {
                    overlays.show_menu_from(
                        Some(a11y_id),
                        PanelTrigger::Control(PanelRect::from_area(&area.read())),
                        items.clone(),
                        false,
                    );
                }
            })
        };
        let long_press = open_menu.clone();
        let mouse = pointer_kind() == PointerKind::Mouse;
        // How much of the hover fill shows. It eases in and out on the standard
        // motion, so crossing from one tab to the next fades one out as the other
        // fades in, and a pointer that only passes over a tab does not flash it.
        let glow = use_tween(
            if self.hover && mouse && !self.selected { 1.0 } else { 0.0 },
            tokens.motion(MOVE_MS),
        );
        let fill = wash(self.selected, self.drop_target, self.pressed, glow);
        // The close button shows on the selected tab and on the one the pointer
        // is over. A finger cannot hover, and at compact density the button is
        // the touch target of the tab, so there it is always shown.
        let close_alpha = if self.selected || !mouse || geometry.inset == 0.0 { 1.0 } else { glow };
        let lit = self.selected || glow > 0.5 || matches!(fill, Wash::Pressed | Wash::Drop);

        // The selected tab's fill is the selection layer over the bar, as one opaque
        // colour (`surface`): the tab in the hand of a drag slides over its neighbours'
        // titles and must hide them, which a translucent layer would not.
        let pill = match fill {
            Wash::None => None,
            Wash::Drop => Some((bg(Paint::AccentSoft), 1.0)),
            Wash::Selected => Some((surface(BAR, false, false, true), 1.0)),
            Wash::Pressed => Some((bg(Paint::Pressed), 1.0)),
            Wash::Hover(strength) => Some((bg(Paint::Hover), strength)),
        };
        let ink = if lit { Ink::Text } else { Ink::Muted };

        let mut parts: Vec<Element> = Vec::with_capacity(3);
        if let Some(icon) = self.item.icon {
            parts.push(icons::icon(icon, IconSize::Control, ink.color(&tokens)).into_element());
        }
        parts.push(Element::from(
            fade_text(self.item.title.clone()).style(CAPTION.in_ink(ink)).grow(),
        ));
        parts.push(Element::from(CloseTab {
            id,
            alpha: close_alpha,
            title: self.item.title.clone(),
            on_close: self.on_close.clone(),
        }));
        // The engine lays a row that reads from the right out from the right, and
        // `pl` is the padding at the start edge: the icon is S2 from the tab's start
        // edge and the close button a field inset from its end, the inset it nests in.
        let pad = [pl(S2), pr(FIELD_INSET)];

        let face = rect()
            .component("Tab")
            .a11y_id(a11y_id)
            .a11y_focusable(self.selected)
            .a11y_role(AccessibilityRole::Tab)
            .a11y_alt(self.item.title.clone())
            .a11y_builder(|builder| builder.set_selected(self.selected))
            .tw(
                &tokens,
                &[
                    w_full(),
                    h_full(),
                    rounded(Radius::Control),
                    clip(),
                ],
            )
            .on_sized(move |event: Event<SizedEventData>| {
                area.set_if_modified(event.area);
            })
            .on_pointer_down(move |event: Event<PointerEventData>| {
                event.stop_propagation();
                on_down.call(id);
                if let PointerEventData::Mouse(mouse) = event.data() {
                    match mouse.button {
                        Some(MouseButton::Middle) => {
                            on_close.call(id);
                            return;
                        }
                        Some(MouseButton::Right) => {
                            open_menu.call(());
                            return;
                        }
                        _ => {}
                    }
                }
                // A mouse selects as it goes down, so the tab can be dragged
                // at once; a finger selects when it lifts, unless it scrolls
                // the strip or holds for the menu.
                if !matches!(event.data(), PointerEventData::Touch(_)) {
                    on_select.call(id);
                    a11y_id.request_focus();
                    if let Some(on_arm) = &on_arm {
                        let at = event.global_location();
                        on_arm.call((id, (at.x as f32, at.y as f32)));
                    }
                }
                let long_press = long_press.clone();
                press.begin(&event, Some(EventHandler::new(move |_: (f32, f32)| long_press.call(()))));
            })
            .on_global_pointer_move(move |event: Event<PointerEventData>| press.moved(&event))
            .on_global_pointer_up(move |_| press.release())
            .on_press({
                let on_select = self.on_select.clone();
                move |event: Event<PressEventData>| {
                    event.stop_propagation();
                    if !press.tap(&event) {
                        return;
                    }
                    on_select.call(id);
                    a11y_id.request_focus();
                }
            })
            .maybe_child(pill.map(|(paint, strength)| {
                rect()
                    .key("pill")
                    .position(Position::new_absolute().top(0.0).left(0.0))
                    .interactive(false)
                    .width(Size::px(self.width))
                    .height(Size::px(geometry.tab))
                    .tw(&tokens, &[rounded(Radius::Control), paint])
                    .maybe(strength < 1.0, |pill| pill.opacity(strength))
            }))
            .child(
                rect()
                    .key("content")
                    .tw(
                        &tokens,
                        &[
                            w_full(),
                            h_full(),
                            flex_row(),
                            grow_children(),
                            items(Align::Center),
                            gap(S1),
                        ],
                    )
                    .tw(&tokens, &pad)
                    .children(parts),
            );

        let body = face
            .key("body")
            .focusable_inside(focus())
            .position(Position::new_absolute().top(geometry.top).left(0.0))
            .height(Size::px(geometry.tab))
            .width(Size::px(self.width));

        rect()
            .position(Position::new_absolute().top(0.0).left(self.left))
            // Paint order is the layer, which is the depth in the tree, so the
            // contents of a tab beside this one would draw over this tab's fill
            // where they overlap (a dragged tab). A whole tab is lifted over its
            // neighbours by more than a tab is deep.
            .layer(Layer::Relative(if self.held { HELD_LAYER } else { 0 }))
            .height(Size::px(geometry.strip))
            .width(Size::px(self.width))
            .maybe(self.presence < 1.0 || self.armed, |slot| {
                slot.opacity(self.presence * if self.armed { ARMED_OPACITY } else { 1.0 })
            })
            .child(body)
    }
}

/// The close button of a tab: always laid out, so a tab does not change width
/// when it is hovered, and drawn only while it can be used (`alpha` > 0),
/// fading in with the tab's hover fill. It is the standard icon button,
/// [`FIELD_INSET`] from the edges of the fill it sits in: 24 px at mouse density
/// (the fill's 32 less the inset above and below), and the whole 44 px touch
/// target at compact density. Its corner is the fill's, nested by that inset.
#[derive(PartialEq)]
struct CloseTab {
    id: u64,
    alpha: f32,
    title: String,
    on_close: EventHandler<u64>,
}

impl Component for CloseTab {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let id = self.id;
        let on_close = self.on_close.clone();
        let inset = FIELD_INSET.px();
        let size = if inset > 0.0 { Dim::DenseControl } else { Dim::InlineButton };
        let radius = Radius::Control.nested(inset);
        // A press on the button is the button's: it must not select the tab,
        // begin dragging it, or reach the header bar's drag of the window.
        let slot = rect()
            .component("TabClose")
            .tw(&tokens, &[w(size), h(size)])
            .on_pointer_down(|event: Event<PointerEventData>| event.stop_propagation());
        if self.alpha <= 0.0 {
            return slot.into_element();
        }
        // The tab is the keyboard stop, and Delete or Ctrl+W closes it, so the
        // button is for the pointer and for touch.
        slot.maybe(self.alpha < 1.0, |slot| slot.opacity(self.alpha))
            .child(
                icon_button(Icon::WindowClose, move |_| on_close.call(id))
                    .size(size)
                    .radius(radius)
                    .tooltip(&format!("Close {}", self.title))
                    .pointer_only(),
            )
            .into_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLASSES: [SizeClass; 2] = [SizeClass::Regular, SizeClass::Compact];

    fn item(id: u64) -> TabItem {
        TabItem::new(id, format!("Tab {id}"))
    }

    #[test]
    fn the_strip_never_passes_its_container_at_any_width() {
        for class in CLASSES {
            let geometry = Geometry::in_class(class);
            for add in [false, true] {
                for count in 1..=14 {
                    for available in (100..=900).step_by(7) {
                        let available = available as f32;
                        let fit = fit(geometry, available, count, count as f32, add);
                        assert!(
                            fit.rail <= available + 0.01,
                            "{class:?} add {add}: {count} tabs in {available}px make a strip {} wide",
                            fit.rail
                        );
                        let end = fit.region + geometry.cluster(usize::from(add) + usize::from(fit.overflow));
                        assert!(
                            end <= fit.rail + 0.01,
                            "{class:?}: the region and the buttons ({end}) outrun the strip ({})",
                            fit.rail
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn tabs_share_a_width_between_the_tokens_until_they_scroll() {
        for class in CLASSES {
            let geometry = Geometry::in_class(class);
            let wide = fit(geometry, 2000.0, 3, 3.0, true);
            assert_eq!(wide.tab, geometry.max, "{class:?}: roomy tabs are the widest");
            assert!(!wide.overflow);
            let mut last = wide.tab;
            for available in (300..2000).rev().step_by(11) {
                let narrower = fit(geometry, available as f32, 6, 6.0, true);
                assert!(narrower.tab <= last + 0.01, "{class:?}: tabs only get narrower as room goes");
                assert!(narrower.tab >= geometry.min - 0.01, "{class:?}: never below the minimum");
                last = narrower.tab;
            }
        }
    }

    #[test]
    fn overflow_begins_when_the_minimum_widths_no_longer_fit() {
        let geometry = Geometry::in_class(SizeClass::Regular);
        // 4 px gaps, a New tab button and its insets (4 + 32 + 4), 96 px tabs:
        // a strip of 500 holds 460 of tabs; four need 4 * 100 - 4 = 396, five
        // need 496.
        assert!(!fit(geometry, 500.0, 4, 4.0, true).overflow);
        assert!(fit(geometry, 500.0, 5, 5.0, true).overflow);
        let overflowing = fit(geometry, 500.0, 9, 9.0, true);
        assert_eq!(overflowing.tab, geometry.min);
        assert_eq!(overflowing.rail, 500.0);
        assert_eq!(
            overflowing.region,
            500.0 - geometry.cluster(2),
            "the tab list and New tab buttons share one cluster after the tabs"
        );
    }

    #[test]
    fn a_leaving_tab_does_not_change_whether_the_strip_overflows() {
        let geometry = Geometry::in_class(SizeClass::Regular);
        let settled = fit(geometry, 500.0, 4, 4.0, true);
        let leaving = fit(geometry, 500.0, 4, 4.6, true);
        assert_eq!(settled.overflow, leaving.overflow);
    }

    #[test]
    fn a_new_tab_grows_in_and_a_closed_tab_stays_until_it_has_shrunk() {
        let tabs = |ids: &[u64]| ids.iter().map(|id| item(*id)).collect::<Vec<_>>();
        let mut entries = Vec::new();
        assert!(!merge(&mut entries, &tabs(&[1, 2]), false), "the first tabs do not grow in");
        assert!(entries.iter().all(|entry| entry.life == Life::Live));
        assert!(merge(&mut entries, &tabs(&[1, 2, 3]), true));
        assert_eq!(entries[2].life, Life::Entering);
        assert!(merge(&mut entries, &tabs(&[1, 3]), true));
        let lives: Vec<(u64, Life)> = entries.iter().map(|entry| (entry.item.id, entry.life)).collect();
        assert_eq!(lives, [(1, Life::Live), (2, Life::Leaving), (3, Life::Live)]);
        assert_eq!(entries[1].presence(0.25), 0.75);
    }

    #[test]
    fn reordering_is_not_a_motion() {
        let tabs = |ids: &[u64]| ids.iter().map(|id| item(*id)).collect::<Vec<_>>();
        let mut entries = Vec::new();
        merge(&mut entries, &tabs(&[1, 2, 3]), false);
        assert!(!merge(&mut entries, &tabs(&[3, 1, 2]), true));
        let ids: Vec<u64> = entries.iter().map(|entry| entry.item.id).collect();
        assert_eq!(ids, [3, 1, 2]);
    }

    #[test]
    fn dragging_the_first_tab_past_the_middle_of_the_next_swaps_them() {
        let geometry = Geometry::in_class(SizeClass::Regular);
        // Three tabs of 100 with 4 px between: a step is 104.
        let stack = Stack::new(vec![100.0; 3], geometry.gap());
        assert_eq!(stack.landing(0, 40.0), 0, "a short way is still the same place");
        assert_eq!(stack.landing(0, 60.0), 1, "the front edge passes the next tab's middle");
        assert_eq!(stack.landing(0, 500.0), 2);
        assert_eq!(stack.landing(2, -500.0), 0);
    }

    #[test]
    fn the_close_button_nests_in_the_tab_and_every_target_fits_it() {
        // A tab's corner is Control; a button FIELD_INSET inside it is that
        // radius less the inset, and at compact density there is no inset.
        for class in CLASSES {
            let inset = FIELD_INSET.px_in(class);
            assert_eq!(Radius::Control.nested(inset).px(), Radius::Control.px() - inset);
            let geometry = Geometry::in_class(class);
            let close = if inset > 0.0 { Dim::DenseControl } else { Dim::InlineButton }.px_in(class);
            // The close button is centred in the tab, inset by the field inset.
            assert_eq!((geometry.tab - close) / 2.0, inset, "{class:?}: the close button is inset by the field inset");
            assert!(close >= 24.0, "{class:?}: the close button is {close}px, under 24");
            // The end buttons are as tall as a tab.
            assert_eq!(geometry.button(), geometry.tab, "{class:?}: the end buttons are as tall as a tab");
        }
    }

    #[test]
    fn at_compact_density_every_target_is_the_touch_minimum() {
        let geometry = Geometry::in_class(SizeClass::Compact);
        assert!(geometry.tab >= TOUCH_MIN, "a tab is {} tall", geometry.tab);
        assert_eq!(geometry.button(), geometry.tab);
        assert!(geometry.min >= TOUCH_MIN, "a tab is {} wide at least", geometry.min);
        assert_eq!(Dim::InlineButton.px_in(SizeClass::Compact), TOUCH_MIN);
        assert_eq!(Dim::TabButton.px_in(SizeClass::Compact), TOUCH_MIN);
    }

    #[test]
    fn a_held_width_keeps_the_tabs_narrow_and_the_strip_short() {
        let geometry = Geometry::in_class(SizeClass::Regular);
        // Four tabs in 800 are 240 wide at most; the room for three would give
        // them 240 too, but with 600 the room gives more as one tab closes.
        let before = fit(geometry, 600.0, 4, 4.0, true);
        let after = fit(geometry, 600.0, 3, 3.0, true);
        assert!(after.tab > before.tab, "one tab fewer and the room gives wider tabs");
        let held = after.held_at(geometry, before.tab, 3.0, true);
        assert_eq!(held.tab, before.tab, "held: the tabs stay as they were");
        assert!(held.rail < before.rail, "the strip is one tab shorter");
        assert!(held.rail <= after.rail, "and no longer than the room allows");
        assert!(!held.overflow);
        // A scrolling strip is not held: its tabs are at the minimum already.
        let scrolling = fit(geometry, 300.0, 9, 9.0, true);
        assert_eq!(scrolling.held_at(geometry, 150.0, 9.0, true), scrolling);
    }

    #[test]
    fn each_state_of_a_tab_has_its_own_fill() {
        // Nothing, from a tab nothing is happening to.
        assert_eq!(wash(false, false, false, 0.0), Wash::None);
        // Hovered: the hover fill, at the strength the fade has got to.
        assert_eq!(wash(false, false, false, 0.4), Wash::Hover(0.4));
        assert_eq!(wash(false, false, false, 1.7), Wash::Hover(1.0), "never past full");
        // Pressed beats hovered, and at once.
        assert_eq!(wash(false, false, true, 1.0), Wash::Pressed);
        // Selected has the selection fill, whatever the pointer does.
        assert_eq!(wash(true, false, true, 1.0), Wash::Selected);
        assert_eq!(wash(true, false, false, 1.0), Wash::Selected);
        // A drop target is tinted, selected or not, over everything else.
        assert_eq!(wash(false, true, true, 1.0), Wash::Drop);
        assert_eq!(wash(true, true, false, 0.0), Wash::Drop);
        let all = [Wash::None, Wash::Drop, Wash::Selected, Wash::Pressed, Wash::Hover(1.0)];
        for (index, a) in all.iter().enumerate() {
            for b in &all[index + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn the_pointer_is_on_the_tab_whose_body_it_is_over_and_on_none_in_a_gap_or_above_the_tabs() {
        // Three 100 px tabs, 4 px apart, from x = 20, in a band from y = 4 to 36.
        let spans = [(20.0, 120.0, 7), (124.0, 224.0, 8), (228.0, 328.0, 9)];
        let band = (4.0, 36.0);
        assert_eq!(tab_at(&spans, band, (20.0, 10.0)), Some(7));
        assert_eq!(tab_at(&spans, band, (119.9, 35.0)), Some(7));
        assert_eq!(tab_at(&spans, band, (121.0, 20.0)), None, "the gap between two tabs");
        assert_eq!(tab_at(&spans, band, (124.0, 20.0)), Some(8));
        assert_eq!(tab_at(&spans, band, (300.0, 20.0)), Some(9));
        assert_eq!(tab_at(&spans, band, (60.0, 2.0)), None, "the band above the tabs drags the window");
        assert_eq!(tab_at(&spans, band, (60.0, 36.0)), None, "the band below the tabs drags it too");
        assert_eq!(tab_at(&spans, band, (500.0, 20.0)), None, "the empty bar");
        assert_eq!(tab_at(&[], band, (60.0, 20.0)), None);
    }

    #[test]
    fn every_title_is_readable_on_the_fill_it_sits_on_in_both_schemes() {
        use crate::contrast::{AA_TEXT, composite, worst_ratio};
        use crate::theme::{Scheme, Tokens};
        for scheme in [Scheme::PreferDark, Scheme::PreferLight] {
            let tokens = Tokens::new("blue", scheme, false);
            let bar = BAR.color(&tokens);
            // An unselected title is muted ink on the bar, and text ink once the
            // pointer is over it (under the hover and pressed fills). The selected
            // title is text ink on the selection fill.
            let muted = Ink::Muted.color(&tokens);
            assert!(worst_ratio(muted, &[bar]) >= AA_TEXT, "{scheme:?}: muted on the bar is {}", worst_ratio(muted, &[bar]));
            let text = Ink::Text.color(&tokens);
            let lit = [
                composite(Paint::Hover.color(&tokens), bar),
                composite(Paint::Pressed.color(&tokens), bar),
                composite(Paint::Selection.color(&tokens), bar),
                composite(Paint::AccentSoft.color(&tokens), bar),
            ];
            assert!(worst_ratio(text, &lit) >= AA_TEXT, "{scheme:?}: text on a lit tab is {}", worst_ratio(text, &lit));
            // The close button's icon is muted ink on the same fills.
            assert!(worst_ratio(muted, &lit[..3]) >= AA_TEXT, "{scheme:?}: muted on a lit tab");
        }
    }

    #[test]
    fn the_selected_tab_is_the_fill_of_a_selected_sidebar_row_and_is_not_the_hover_fill() {
        use crate::contrast::composite;
        use crate::theme::{Scheme, Tokens};
        for scheme in [Scheme::PreferDark, Scheme::PreferLight] {
            let tokens = Tokens::new("blue", scheme, false);
            let bar = BAR.color(&tokens);
            let selected = composite(Paint::Selection.color(&tokens), bar);
            assert_ne!(selected, bar, "{scheme:?}: a selected tab is visible on the bar");
            assert_ne!(selected, composite(Paint::Hover.color(&tokens), bar), "{scheme:?}: selected is not hovered");
        }
    }
}
