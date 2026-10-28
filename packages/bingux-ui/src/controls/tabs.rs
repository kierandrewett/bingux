//! A strip of closable tabs that are attached to the content they open, for an
//! application whose window holds several views of the same kind: the folders
//! of a file manager, the documents of an editor. It is Chromium's tab strip:
//! the selected tab is the same surface as the content under it, with rounded
//! top corners and a concave flare at each foot that joins the strip's bottom
//! edge, and the other tabs are flat and recessed on a darker strip. See
//! `design/tabs.md` for the model and the numbers.
//!
//! The strip is the title of a header bar and sits in a [`tab_frame`], which
//! owns the content below it and the seam between them: the selected tab is
//! painted in the frame's surface, so tab and content are one shape and there
//! is no line between them. A strip outside a frame draws the same tabs on
//! whatever is behind it, and the tab is still painted in the page colour.
//!
//! * **Shape.** A tab is [`Dim::Control`] tall and stands on the strip's
//!   bottom edge, [`Dim::Header`] below its top, so a band above the tabs
//!   still drags the window. Its top corners are [`Radius::Control`]; its
//!   feet flare out by that radius less [`FIELD_INSET`], and the feet of two
//!   neighbours are the gap between them. Only the selected tab has an
//!   outline; an unselected tab is flat, lifts with a fill when the pointer
//!   is over it, and has a short separator after it that hides next to the
//!   selected tab and the tab the pointer is on.
//! * **Width.** Tabs share one width. It is [`Dim::Field`] at most; as tabs
//!   are added they shrink together, down to [`Dim::TabMin`], and past that the
//!   strip scrolls sideways instead. The strip hugs its tabs and never grows
//!   past its container.
//! * **A tab** is an optional icon, a title that takes what room is left and
//!   is cut with an ellipsis, and a close button. The close button has a slot
//!   of its own that is always laid out, so a long title is cut before it and
//!   a tab does not change width when the button appears. It shows on the
//!   selected tab, and on a tab the pointer is over. A lone tab has no close
//!   button, because there is nothing to close into.
//! * **The end of the strip** is fixed: the New tab button (drawn when
//!   [`TabStrip::on_new`] is set) follows the last tab and never scrolls away.
//!   When the tabs overflow, a tab list button beside it opens a menu of
//!   every tab, and the tabs fade at the edge they continue past. The
//!   selected tab is scrolled into view.
//! * **Adding and closing** animate the widths, so the tabs make room and
//!   close ranks without a jump.
//! * **Pointer.** A press selects, a middle click closes, and with
//!   [`TabStrip::on_reorder`] set a tab can be dragged: it follows the
//!   pointer and the others slide aside ([`crate::reorder`]), and it settles
//!   into its place when let go. Escape, or letting go far above or below the
//!   strip, puts it back. A right click, or a touch held on a tab, opens its
//!   menu: Close, and Move left and Move right for a reorderable strip, which
//!   is how touch reorders.
//! * **Keyboard.** Only the selected tab is in the tab order. Left, Right,
//!   Home and End move the selection and the focus; Ctrl+Shift+Left and Right
//!   move the selected tab; Delete or Ctrl+W closes the selected tab, and
//!   Ctrl+T asks for a new one.
//! * [`TabStrip::drop_target`] marks the tab a drag would drop on with the
//!   accent, so a host can accept files on a tab.
//!
//! At compact density the strip has no [`FIELD_INSET`]: each tab, and the
//! close, tab list and New tab buttons, is the whole 44 px touch target.
//!
//! The strip keeps no state of its own beyond what is on screen: the host
//! owns the list and the selected id.
//!
//! [`tab_frame`]: super::tab_frame

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use freya::animation::*;
use freya::prelude::*;

use super::button::icon_button;
use super::press::use_press;
use super::tab_frame::{STRIP, SURFACE, TabSurface};
use super::tab_shape::{Geometry, Shape, shape_box};
use crate::component::ComponentName;
use crate::drag::{Frames, PointerDrag, use_frames, use_pointer_drag};
use crate::icons::{self, Icon};
use crate::motion::{Focusable, MOVE_MS};
use crate::reorder::{self, Done, Indicate, Reorder, Stack};
use crate::shell::menu::{MenuItem, menu_item};
use crate::shell::overlays::Overlays;
use crate::shell::panel::{PanelRect, PanelTrigger};
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{BODY, Ink};

/// How far the pointer travels before a press on a tab becomes a drag.
const DRAG_SLOP: f32 = 6.0;

/// The most a pointer may leave the strip, up and down, and still drop a
/// dragged tab: beyond this the drag is cancelled.
const DRAG_REACH: Dim = Dim::Header;

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
        on_new: None,
        on_hover: EventHandler::new(|_: (u64, bool)| {}),
        on_reorder: None,
        drop_target: None,
    }
}

#[derive(Clone, PartialEq)]
pub struct TabStrip {
    tabs: Vec<TabItem>,
    selected: u64,
    on_select: EventHandler<u64>,
    on_close: EventHandler<u64>,
    on_new: Option<EventHandler<()>>,
    on_hover: EventHandler<(u64, bool)>,
    on_reorder: Option<EventHandler<(u64, usize)>>,
    drop_target: Option<u64>,
}

impl TabStrip {
    pub fn on_select(mut self, handler: impl Into<EventHandler<u64>>) -> Self {
        self.on_select = handler.into();
        self
    }

    /// A close button, a middle click, Delete or Ctrl+W asked to close this tab.
    pub fn on_close(mut self, handler: impl Into<EventHandler<u64>>) -> Self {
        self.on_close = handler.into();
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

/// The widths for `target` tabs that are staying and `effective` tabs' worth of
/// width right now (a tab that is arriving or leaving counts for part of one),
/// in `available` pixels, with or without the New tab button.
fn fit(geometry: Geometry, available: f32, target: usize, effective: f32, add: bool) -> Fit {
    let slot = geometry.button;
    let room = (available - if add { slot } else { 0.0 }).max(0.0);
    if geometry.content(target as f32, geometry.min) > room + 0.01 {
        // Past the narrowest width: the strip is as wide as its container, and
        // the tab list button takes a slot beside the tabs.
        return Fit {
            tab: geometry.min,
            rail: available.max(0.0),
            region: (room - slot).max(0.0),
            overflow: true,
        };
    }
    let count = effective.max(1.0);
    let tab = ((room - geometry.foot) / count - geometry.gap()).clamp(0.0, geometry.max);
    let region = geometry.content(effective, tab);
    Fit {
        tab,
        rail: region + if add { slot } else { 0.0 },
        region,
        overflow: false,
    }
}

/// The scroll offset, from the start of the tabs, that brings the tab at
/// `index` and its feet into a region `region` wide without moving more than
/// it must: `current` is where the region starts now.
fn scroll_into_view(index: usize, geometry: Geometry, tab: f32, region: f32, current: f32) -> f32 {
    let left = index as f32 * (tab + geometry.gap());
    let right = left + tab + 2.0 * geometry.foot;
    if left < current {
        left
    } else if right > current + region {
        right - region
    } else {
        current
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

// ---- Drag to reorder -------------------------------------------------------

/// What a drag draws: which tab is held, how far each live tab is from its
/// place, and how far the held one is lifted. Idle is the default.
#[derive(Clone, Default, PartialEq)]
struct View {
    held: Option<usize>,
    offsets: Vec<f32>,
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
    /// How far the tabs are scrolled now, and when the tab was picked up.
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
}

/// The handles every handler of one strip needs. Clone it into each.
#[derive(Clone)]
struct Env {
    shared: Rc<Shared>,
    drag: PointerDrag,
    frames: Frames,
    view: State<View>,
    scroll: ScrollController,
}

impl Env {
    fn busy(&self) -> bool {
        self.shared.reorder.borrow().is_busy()
    }

    /// A press went down on live tab `index`: the drag begins when the
    /// pointer has gone far enough.
    fn arm(&self, index: usize, at: (f32, f32)) {
        if self.busy() || self.shared.ids.borrow().len() < 2 {
            return;
        }
        self.shared.pending.set(Some(index));
        self.drag.begin_at(at, false);
        self.start_frames();
    }

    /// Pick the pressed tab up once the pointer has gone [`DRAG_SLOP`] from
    /// where it went down.
    fn pick_if_moved(&self) {
        let Some(index) = self.shared.pending.get() else {
            return;
        };
        if self.drag.is_active_now() && !self.busy() && self.drag.travelled().0.abs() >= DRAG_SLOP {
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
    /// pointer is far from the strip.
    fn release(&self, at: (f32, f32)) {
        // A drag that was quicker than a frame is still a drag.
        self.pick_if_moved();
        self.shared.pending.set(None);
        self.follow(0.0);
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
        self.drag.end();
        self.publish();
        self.start_frames();
    }

    /// Carry the held tab to where the pointer is, and scroll the strip when
    /// the pointer is at the edge of the region. `dt` is the seconds since the
    /// last frame.
    fn follow(&self, dt: f32) {
        if !self.shared.reorder.borrow().is_held() {
            return;
        }
        let (pointer, _) = self.drag.pointer();
        let (left, right) = self.shared.region.get();
        let speed = reorder::edge_speed(pointer, left, right);
        if speed != 0.0 && dt > 0.0 {
            let want = self.shared.carry.get() + speed * dt;
            let whole = want.trunc();
            self.shared.carry.set(want - whole);
            if whole != 0.0 {
                let next = (self.shared.offset.get() + whole).clamp(0.0, self.shared.range.get());
                self.shared.offset.set(next);
                let mut scroll = self.scroll;
                scroll.scroll_to_x(-(next.round() as i32));
            }
        }
        // Distances are in the content: the strip scrolling under the pointer
        // moves the held tab with it.
        let scrolled = self.shared.offset.get() - self.shared.offset_start.get();
        let delta = (pointer - self.drag.start().0) + scrolled;
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
        self.pick_if_moved();
        if !self.drag.is_active_now() {
            self.shared.pending.set(None);
        }
        self.follow(dt);
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

impl Component for TabStrip {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let geometry = Geometry::current();
        let surface = use_try_consume::<TabSurface>().map_or(SURFACE, |surface| surface.0);
        let mut available = use_state(|| None::<f32>);
        let mut rail_area = use_state(Area::default);
        let mut region_area = use_state(Area::default);
        let mut button_area = use_state(Area::default);
        let mut hovered = use_state(|| None::<u64>);
        let seen = use_state(|| 0_u64);
        let tick = use_state(|| 0_u64);
        let scroll = use_scroll_controller(ScrollConfig::default);
        let overlays = use_try_consume::<Overlays>();
        let memory = use_hook(|| Rc::new(RefCell::new(Memory::default())));
        // Where the tabs are scrolled to, for the effect that scrolls the
        // selected tab into view: it must not subscribe to the scroll itself.
        let scrolled = use_hook(|| Rc::new(Cell::new(0.0_f32)));
        // Whether the person closed a tab from the keyboard, so the focus can
        // follow the selection to the tab that takes its place.
        let refocus = use_hook(|| Rc::new(Cell::new(false)));
        // One accessibility id per tab, kept for the life of the strip, so
        // the arrow keys can move focus to a neighbouring tab.
        let ids = use_hook(|| Rc::new(RefCell::new(HashMap::<u64, AccessibilityId>::new())));
        let drag = use_pointer_drag();
        let frames = use_frames();
        let view = use_state(View::default);
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
            })
        });
        let _ = tick();
        shared.reduced.set(tokens.reduced_motion);
        *shared.on_reorder.borrow_mut() = self.on_reorder.clone();

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
        let fit = fit(
            geometry,
            available().unwrap_or(Dim::Page.px()),
            staying,
            effective,
            add,
        );
        let content = geometry.content(effective, fit.tab).max(0.0);
        let range = (content - fit.region).max(0.0);
        let offset = (-(<(i32, i32)>::from(scroll).0 as f32)).clamp(0.0, range);
        scrolled.set(offset);
        shared.tab.set(fit.tab);
        shared.gap.set(geometry.gap());
        shared.range.set(range);
        if !drag.is_active() {
            shared.offset.set(offset);
        }
        let region_bounds = *region_area.peek();
        shared.region.set((region_bounds.min_x(), region_bounds.max_x()));
        let rail_bounds = *rail_area.peek();
        shared.bounds.set((rail_bounds.min_y(), rail_bounds.max_y()));

        // Scroll the selected tab into view when the selection moves, and again
        // when the tabs have settled, because the content only has its final
        // width then.
        let selected_index = entries
            .iter()
            .position(|entry| entry.item.id == self.selected && entry.life != Life::Leaving);
        {
            let scrolled = scrolled.clone();
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
                let mut scroll = scroll;
                let to = scroll_into_view(index, geometry, tab as f32, region as f32, scrolled.get());
                scroll.scroll_to_x(-(to.round() as i32));
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
        let closable = live.len() > 1;
        let reorderable = self.on_reorder.is_some();
        let env = Env {
            shared: shared.clone(),
            drag: drag.clone(),
            frames,
            view,
            scroll,
        };
        let current_view = view.read().clone();

        // ---- Tabs
        let mut row_children: Vec<Element> = Vec::with_capacity(entries.len());
        let mut held_child: Option<Element> = None;
        let mut x = geometry.foot;
        for (position, entry) in entries.iter().enumerate() {
            let presence = entry.presence(progress);
            let id = entry.item.id;
            let place = live_ids.iter().position(|live| *live == id);
            let body = fit.tab * presence;
            let slide = place
                .and_then(|place| current_view.offsets.get(place).copied())
                .unwrap_or(0.0);
            let held = place.is_some() && place == current_view.held;
            let mut menu = Vec::new();
            if closable && entry.life != Life::Leaving {
                let on_close = self.on_close.clone();
                menu.push(menu_item("Close tab", move |_| on_close.call(id)));
            }
            if let (Some(place), Some(on_reorder)) = (place, &self.on_reorder) {
                if place > 0 {
                    let on_reorder = on_reorder.clone();
                    menu.push(menu_item("Move left", move |_| on_reorder.call((id, place - 1))));
                }
                if place + 1 < live_ids.len() {
                    let on_reorder = on_reorder.clone();
                    menu.push(menu_item("Move right", move |_| on_reorder.call((id, place + 1))));
                }
            }
            // A separator follows a tab unless the tab or the next one is the
            // selected one, the one the pointer is on, or the one in hand.
            let next = live.get(place.map_or(usize::MAX, |place| place + 1)).map(|entry| entry.item.id);
            let quiet = |tab: u64| {
                tab == self.selected
                    || hovered() == Some(tab)
                    || place_of(&live_ids, tab).is_some_and(|place| Some(place) == current_view.held)
            };
            let separator = entry.life == Life::Live
                && next.is_some_and(|next| !quiet(id) && !quiet(next));
            let tab = Element::from(Tab {
                item: entry.item.clone(),
                geometry,
                surface,
                width: body,
                left: x - geometry.foot + slide,
                presence,
                selected: id == self.selected,
                drop_target: self.drop_target == Some(id),
                closable,
                separator,
                menu,
                on_select: self.on_select.clone(),
                on_close: self.on_close.clone(),
                on_hover: {
                    let on_hover = self.on_hover.clone();
                    EventHandler::new(move |(tab, entered): (u64, bool)| {
                        let next = if entered { Some(tab) } else { None };
                        if entered || *hovered.peek() == Some(tab) {
                            hovered.set_if_modified(next);
                        }
                        on_hover.call((tab, entered));
                    })
                },
                on_arm: (reorderable && place.is_some()).then(|| {
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
            x += (fit.tab + geometry.gap()) * presence;
        }
        // The tab in hand is drawn last, over the ones that slide aside.
        row_children.extend(held_child);

        let key_ids: Vec<(u64, AccessibilityId)> = entries
            .iter()
            .zip(&a11y_ids)
            .filter(|(entry, _)| entry.life != Life::Leaving)
            .map(|(entry, a11y)| (entry.item.id, *a11y))
            .collect();
        let (on_select, on_close, on_new, on_reorder) = (
            self.on_select.clone(),
            self.on_close.clone(),
            self.on_new.clone(),
            self.on_reorder.clone(),
        );
        let selected = self.selected;
        let wants_focus = refocus.clone();
        let row = rect()
            .tw(&tokens, &[h(Dim::Header)])
            .width(Size::px(content))
            .on_key_down(move |event: Event<KeyboardEventData>| {
                let Some(index) = key_ids.iter().position(|(id, _)| *id == selected) else {
                    return;
                };
                let last = key_ids.len() - 1;
                let ctrl = event.modifiers.contains(Modifiers::CONTROL);
                let shift = event.modifiers.contains(Modifiers::SHIFT);
                let next = match &event.key {
                    Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowRight) if ctrl && shift => {
                        event.stop_propagation();
                        let to = if event.key == Key::Named(NamedKey::ArrowLeft) {
                            index.checked_sub(1)
                        } else {
                            (index < last).then_some(index + 1)
                        };
                        if let (Some(to), Some(on_reorder)) = (to, &on_reorder) {
                            on_reorder.call((selected, to));
                        }
                        return;
                    }
                    Key::Named(NamedKey::ArrowLeft) => index.saturating_sub(1),
                    Key::Named(NamedKey::ArrowRight) => (index + 1).min(last),
                    Key::Named(NamedKey::Home) => 0,
                    Key::Named(NamedKey::End) => last,
                    Key::Named(NamedKey::Delete) => {
                        event.stop_propagation();
                        if key_ids.len() > 1 {
                            wants_focus.set(true);
                            on_close.call(selected);
                        }
                        return;
                    }
                    Key::Character(text) if ctrl && text.eq_ignore_ascii_case("w") => {
                        event.stop_propagation();
                        if key_ids.len() > 1 {
                            wants_focus.set(true);
                            on_close.call(selected);
                        }
                        return;
                    }
                    Key::Character(text) if ctrl && text.eq_ignore_ascii_case("t") => {
                        event.stop_propagation();
                        if let Some(on_new) = &on_new {
                            wants_focus.set(true);
                            on_new.call(());
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
            .children(row_children);

        // ---- The region the tabs scroll in, with fades where they continue
        // A fade stops above the bar's last row, so it does not hide the
        // hairline the strip stands on.
        let fade_width = Dim::Box5.px();
        let fade_height = geometry.strip - geometry.hairline;
        let fade = |at_start: bool| {
            let solid = STRIP.color(&tokens);
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
                    .angle(90.0)
                    .stop(GradientStop::new(from, 0.0))
                    .stop(GradientStop::new(to, 100.0)),
            )
        };
        // The fades need room of their own: a region narrower than two of them has none.
        let fades = fit.overflow && fit.region >= 2.0 * Dim::Box5.px();
        let region = rect()
            .tw(&tokens, &[h(Dim::Header), clip()])
            .width(Size::px(fit.region))
            .on_sized(move |event: Event<SizedEventData>| {
                region_area.set_if_modified(event.area);
            })
            .child(
                scroll_x_controlled(scroll)
                    .show_scrollbar(false)
                    .scroll_with_arrows(false)
                    .invert_scroll_wheel(true)
                    .child(row),
            )
            .maybe_child((fades && offset > 0.5).then(|| fade(true)))
            .maybe_child((fades && offset < range - 0.5).then(|| fade(false)));

        // ---- The buttons at the end of the strip. Each sits in a cell as
        // tall as the strip, level with the tabs: the nested corner of a tab.
        let radius = Radius::Control.nested(geometry.inset);
        let cell = |button: Element| {
            rect()
                .tw(&tokens, &[w(Dim::InlineButton), h(Dim::Header), flex_col(), justify(Align::End)])
                .child(
                    rect()
                        .component("TabEnd")
                        .tw(&tokens, &[w_full(), h(Dim::Control), flex_row(), items(Align::Center)])
                        .child(button),
                )
        };
        let all_tabs: Vec<MenuItem> = live
            .iter()
            .map(|entry| {
                let on_select = self.on_select.clone();
                let id = entry.item.id;
                menu_item(entry.item.title.clone(), move |_| on_select.call(id))
                    .checked(id == self.selected)
            })
            .collect();
        let tab_list = fit.overflow.then(|| {
            cell(
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
                        .size(Dim::InlineButton)
                        .radius(radius)
                        .tooltip("Tab list"),
                    )
                    .into_element(),
            )
        });
        let new_tab = self.on_new.clone().map(|on_new| {
            cell(
                icon_button(Icon::ListAdd, on_new)
                    .size(Dim::InlineButton)
                    .radius(radius)
                    .tooltip("New tab")
                    .into_element(),
            )
        });

        // ---- Drag to reorder: the strip tracks the pointer while one is on.
        let release = env.clone();
        let cancel = env.clone();
        let strip = rect()
            .component("TabStrip")
            .a11y_role(AccessibilityRole::TabList)
            .a11y_alt("Tabs")
            .width(Size::px(fit.rail))
            .tw(&tokens, &[h(Dim::Header), flex_row()])
            .on_sized(move |event: Event<SizedEventData>| {
                rail_area.set_if_modified(event.area);
            })
            .child(region)
            .maybe_child(tab_list)
            .maybe_child(new_tab);
        let strip = drag.track(strip, move |at| release.release(at), move || cancel.cancel());

        rect()
            .tw(&tokens, &[w_full(), h(Dim::Header)])
            .on_sized(move |event: Event<SizedEventData>| {
                available.set_if_modified(Some(event.area.width()));
            })
            .child(strip)
    }
}

/// The place of tab `id` among the live tabs.
fn place_of(live_ids: &[u64], id: u64) -> Option<usize> {
    live_ids.iter().position(|live| *live == id)
}

// ---- A tab -----------------------------------------------------------------

/// One tab: a slot a foot wider than the body on each side, so the selected
/// tab's flares fit. The slot is placed by the strip, `left` from the start of
/// the tabs; the body is the control that takes the pointer and the focus.
#[derive(PartialEq)]
struct Tab {
    item: TabItem,
    geometry: Geometry,
    /// The paint of the content, which the selected tab is painted in.
    surface: Paint,
    /// The body's width at this moment of the motion.
    width: f32,
    /// The slot's left edge in the strip's content.
    left: f32,
    /// How much of the tab is there, for a tab that is arriving or leaving.
    presence: f32,
    selected: bool,
    drop_target: bool,
    /// A lone tab has nothing to close into.
    closable: bool,
    /// Whether the short separator after the tab is drawn.
    separator: bool,
    /// Its right click and long press menu.
    menu: Vec<MenuItem>,
    on_select: EventHandler<u64>,
    on_close: EventHandler<u64>,
    on_hover: EventHandler<(u64, bool)>,
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
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let mut area = use_state(Area::default);
        let a11y_id = self.a11y_id;
        let focus = use_focus(a11y_id);
        let press = use_press();
        let overlays = use_try_consume::<Overlays>();
        let id = self.item.id;
        let (on_select, on_close) = (self.on_select.clone(), self.on_close.clone());
        let (on_enter, on_leave) = (self.on_hover.clone(), self.on_hover.clone());
        let on_arm = self.on_arm.clone();
        let closable = self.closable;
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
        let mouse_over = hovering() && pointer_kind() == PointerKind::Mouse;
        let show_close = self.closable && (self.selected || mouse_over);
        let lit = self.selected || mouse_over;

        // What fills an unselected tab: a pill that is lifted from the strip
        // by the pointer, pressed by a finger, or marked as a drop target.
        let pill = if self.drop_target {
            Paint::AccentSoft
        } else if pressing() && !self.selected {
            Paint::Pressed
        } else if mouse_over && !self.selected {
            Paint::Hover
        } else {
            Paint::Transparent
        };
        let lift = geometry.tab - geometry.inset.max(Dim::Hairline.px());

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
                    rounded_on(Radius::Control, Corners::Top, 0.0),
                    clip(),
                ],
            )
            .on_sized(move |event: Event<SizedEventData>| {
                area.set_if_modified(event.area);
            })
            .on_pointer_enter(move |_| {
                hovering.set(true);
                on_enter.call((id, true));
            })
            .on_pointer_leave(move |_| {
                hovering.set(false);
                on_leave.call((id, false));
            })
            .on_pointer_down(move |event: Event<PointerEventData>| {
                event.stop_propagation();
                pressing.set(true);
                if let PointerEventData::Mouse(mouse) = event.data() {
                    match mouse.button {
                        Some(MouseButton::Middle) => {
                            if closable {
                                on_close.call(id);
                            }
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
            .on_global_pointer_up(move |_| {
                press.release();
                pressing.set_if_modified(false);
            })
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
            .maybe_child((pill != Paint::Transparent).then(|| {
                rect()
                    .position(Position::new_absolute().top(0.0).left(0.0))
                    .interactive(false)
                    .width(Size::px(self.width))
                    .height(Size::px(lift))
                    .tw(&tokens, &[rounded(Radius::Control), bg(pill)])
            }))
            .child(
                rect()
                    .tw(
                        &tokens,
                        &[
                            w_full(),
                            h_full(),
                            flex_row(),
                            grow_children(),
                            items(Align::Center),
                            pl(S2),
                            pr(FIELD_INSET),
                            gap(S1),
                        ],
                    )
                    .maybe_child(self.item.icon.map(|icon| {
                        icons::icon(
                            icon,
                            IconSize::Control,
                            if lit {
                                Ink::Text.color(&tokens)
                            } else {
                                Ink::Muted.color(&tokens)
                            },
                        )
                    }))
                    .child(
                        rect().tw(&tokens, &[w_flex()]).child(
                            BODY.in_ink(if lit { Ink::Text } else { Ink::Muted })
                                .label(&tokens, self.item.title.clone())
                                .max_lines(1)
                                .text_overflow(TextOverflow::Ellipsis),
                        ),
                    )
                    .maybe_child(self.closable.then(|| {
                        CloseTab {
                            id,
                            visible: show_close,
                            title: self.item.title.clone(),
                            on_close: self.on_close.clone(),
                        }
                    })),
            );

        let body = face
            .focusable_inside(focus())
            .position(Position::new_absolute().top(geometry.top).left(geometry.foot))
            .tw(&tokens, &[h(Dim::Control)])
            .width(Size::px(self.width));

        let slot = geometry.foot * 2.0 + self.width;
        rect()
            .position(Position::new_absolute().top(0.0).left(self.left))
            .tw(&tokens, &[h(Dim::Header)])
            .width(Size::px(slot))
            .maybe(self.presence < 1.0, |slot| slot.opacity(self.presence))
            // The selected tab's outline, under the body that holds its text.
            .maybe_child(self.selected.then(|| {
                shape_box(Shape::new(&geometry, self.width), self.surface, Paint::Separator, &tokens)
                    .position(Position::new_absolute().top(geometry.top).left(0.0))
            }))
            .maybe_child(self.separator.then(|| {
                let at = geometry.foot + self.width + geometry.gap() / 2.0 - Dim::Indicator.px() / 2.0;
                let rise = geometry.top + (geometry.tab - Dim::Box5.px()) / 2.0;
                rect()
                    .position(Position::new_absolute().top(rise).left(at))
                    .interactive(false)
                    .tw(
                        &tokens,
                        &[w(Dim::Indicator), h(Dim::Box5), rounded(Radius::Mark), bg(Paint::MutedFaint)],
                    )
            }))
            .child(body)
    }
}

/// The close button of a tab: always laid out, so a tab does not change width
/// when it is hovered, and drawn only while it can be used. It is the standard
/// icon button, [`FIELD_INSET`] from the tab's edges like a field's trailing
/// button: 20 px at mouse density, and the whole 44 px touch target at compact
/// density. Its corner is the tab's, nested by that inset.
#[derive(PartialEq)]
struct CloseTab {
    id: u64,
    visible: bool,
    title: String,
    on_close: EventHandler<u64>,
}

impl Component for CloseTab {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let id = self.id;
        let on_close = self.on_close.clone();
        let inset = FIELD_INSET.px();
        let size = if inset > 0.0 { Dim::Box5 } else { Dim::InlineButton };
        let radius = Radius::Control.nested(inset);
        // A press on the button is the button's: it must not select the tab
        // or begin dragging it.
        let slot = rect()
            .component("TabClose")
            .tw(&tokens, &[w(size), h(size)])
            .on_pointer_down(|event: Event<PointerEventData>| event.stop_propagation());
        if !self.visible {
            return slot.into_element();
        }
        // The tab is the keyboard stop, and Delete or Ctrl+W closes it, so the
        // button is for the pointer and for touch.
        slot.child(
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
                        let end = fit.region
                            + if add { geometry.button } else { 0.0 }
                            + if fit.overflow { geometry.button } else { 0.0 };
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
        // 8 px feet, a 28 px button, 96 px bodies: a strip of 500 holds 472 of
        // tabs; four need 8 + 4 * 104 = 424, five need 528.
        assert!(!fit(geometry, 500.0, 4, 4.0, true).overflow);
        assert!(fit(geometry, 500.0, 5, 5.0, true).overflow);
        let overflowing = fit(geometry, 500.0, 9, 9.0, true);
        assert_eq!(overflowing.tab, geometry.min);
        assert_eq!(overflowing.rail, 500.0);
        assert_eq!(overflowing.region, 500.0 - 2.0 * geometry.button);
    }

    #[test]
    fn a_leaving_tab_does_not_change_whether_the_strip_overflows() {
        let geometry = Geometry::in_class(SizeClass::Regular);
        let settled = fit(geometry, 500.0, 4, 4.0, true);
        let leaving = fit(geometry, 500.0, 4, 4.6, true);
        assert_eq!(settled.overflow, leaving.overflow);
    }

    #[test]
    fn the_selected_tab_is_scrolled_in_by_the_least_distance() {
        let geometry = Geometry::in_class(SizeClass::Regular);
        // Bodies of 96 with 8 px feet between, in a region 250 wide: a tab's
        // slot is 112 wide and a step is 104.
        let view = |index, current| scroll_into_view(index, geometry, 96.0, 250.0, current);
        assert_eq!(view(1, 0.0), 0.0, "a tab in view does not scroll");
        assert_eq!(view(3, 0.0), 3.0 * 104.0 + 112.0 - 250.0, "the tab's far foot comes to the edge");
        assert_eq!(view(0, 200.0), 0.0, "a tab before the region scrolls back to it");
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
        // Three bodies of 100 with the feet between: a step is 108.
        let stack = Stack::new(vec![100.0; 3], geometry.gap());
        assert_eq!(stack.landing(0, 40.0), 0, "a short way is still the same place");
        assert_eq!(stack.landing(0, 60.0), 1, "the front edge passes the next tab's middle");
        assert_eq!(stack.landing(0, 500.0), 2);
        assert_eq!(stack.landing(2, -500.0), 0);
    }

    #[test]
    fn the_close_button_and_the_new_tab_button_nest_in_the_tab_corner() {
        // A tab's corner is Control; a button FIELD_INSET inside it is that
        // radius less the inset, and at compact density there is no inset.
        for class in CLASSES {
            let inset = FIELD_INSET.px_in(class);
            assert_eq!(Radius::Control.nested(inset).px(), Radius::Control.px() - inset);
            let geometry = Geometry::in_class(class);
            assert_eq!((geometry.tab - geometry.button) / 2.0, inset, "{class:?}: the button is inset by the field inset");
        }
    }

    #[test]
    fn at_compact_density_every_target_is_the_touch_minimum() {
        let geometry = Geometry::in_class(SizeClass::Compact);
        assert!(geometry.tab >= TOUCH_MIN, "a tab is {} tall", geometry.tab);
        assert_eq!(geometry.button, geometry.tab);
        assert!(geometry.min >= TOUCH_MIN, "a tab is {} wide at least", geometry.min);
        assert_eq!(Dim::InlineButton.px_in(SizeClass::Compact), TOUCH_MIN);
    }

    #[test]
    fn unselected_titles_are_readable_on_the_strip_in_both_schemes() {
        use crate::contrast::{AA_TEXT, composite, worst_ratio};
        use crate::theme::{Scheme, Tokens};
        for scheme in [Scheme::PreferDark, Scheme::PreferLight] {
            let tokens = Tokens::new("blue", scheme, false);
            let strip = STRIP.color(&tokens);
            let surface = SURFACE.color(&tokens);
            // An unselected title is muted ink on the strip, under the hover
            // fill and the pressed fill too; the selected title is text ink
            // on the surface.
            let grounds = [
                strip,
                composite(Paint::Hover.color(&tokens), strip),
                composite(Paint::Pressed.color(&tokens), strip),
            ];
            let muted = Ink::Muted.color(&tokens);
            assert!(worst_ratio(muted, &grounds) >= AA_TEXT, "{scheme:?}: muted on the strip is {}", worst_ratio(muted, &grounds));
            let text = Ink::Text.color(&tokens);
            assert!(worst_ratio(text, &[surface]) >= AA_TEXT, "{scheme:?}: text on the surface");
        }
    }

    #[test]
    fn the_strip_is_one_step_off_the_surface_in_both_schemes() {
        use crate::theme::{Scheme, Tokens};
        for scheme in [Scheme::PreferDark, Scheme::PreferLight] {
            let tokens = Tokens::new("blue", scheme, false);
            let (strip, surface) = (STRIP.color(&tokens), SURFACE.color(&tokens));
            assert_ne!(strip, surface);
            let luma = |c: Color| 0.2126 * c.r() as f32 + 0.7152 * c.g() as f32 + 0.0722 * c.b() as f32;
            assert!(luma(strip) < luma(surface), "{scheme:?}: the strip is darker than the surface");
        }
    }
}
