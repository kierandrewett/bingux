//! A strip of closable tabs, for an application whose window holds several
//! views of the same kind: the folders of a file manager, the documents of an
//! editor. It is the header bar's title, so it is one row tall, and it is a
//! rail like the [`segmented`](super::segmented) control's: the same
//! [`Dim::Control`] height, the same `Control` fill, and a tab that is the
//! rail's radius less its inset.
//!
//! * **Width.** Tabs share one width. It is [`Dim::Field`] at most; as tabs
//!   are added they shrink together, down to [`Dim::TabMin`], and past that the
//!   strip scrolls sideways instead. The rail hugs its tabs and never grows
//!   past its container, so nothing spills out of its rounded shape, whatever
//!   the container's width.
//! * **A tab** is an optional icon, a title that takes what room is left and
//!   is cut with an ellipsis, and a close button. The close button has a slot
//!   of its own that is always laid out, so a long title is cut before it and
//!   a tab does not change width when the button appears. It shows on the
//!   selected tab, and on a tab the pointer is over. A lone tab has no close
//!   button, because there is nothing to close into.
//! * **The ends of the rail** are fixed: the New tab button (drawn when
//!   [`TabStrip::on_new`] is set) is the last item and never scrolls away.
//!   When the tabs overflow, a tab list button beside it opens a menu of
//!   every tab, and the tabs fade at the edge they continue past. The
//!   selected tab is scrolled into view.
//! * **Adding and closing** animate the widths, so the tabs make room and
//!   close ranks without a jump.
//! * **Pointer.** A press selects, a middle click closes, and with
//!   [`TabStrip::on_reorder`] set a tab can be dragged to another place (a
//!   line shows where it would land). A right click, or a touch held on a
//!   tab, opens its menu: Close, and Move left and Move right for a
//!   reorderable strip, which is how touch reorders.
//! * **Keyboard.** Only the selected tab is in the tab order. Left, Right,
//!   Home and End move the selection and the focus; Delete or Ctrl+W closes
//!   the selected tab, and Ctrl+T asks for a new one.
//! * [`TabStrip::drop_target`] marks the tab a drag would drop on with the
//!   accent, so a host can accept files on a tab.
//!
//! At compact density the rail has no inset ([`FIELD_INSET`]): each tab, and
//! the close, tab list and New tab buttons, is the whole 44 px touch target
//! and takes the rail's corner.
//!
//! The strip keeps no state of its own beyond what is on screen: the host
//! owns the list and the selected id.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use freya::animation::*;
use freya::prelude::*;

use super::button::icon_button;
use super::press::use_press;
use crate::component::ComponentName;
use crate::icons::{self, Icon};
use crate::motion::{Focusable, MOVE_MS};
use crate::shell::menu::{MenuItem, menu_item};
use crate::shell::overlays::Overlays;
use crate::shell::panel::{PanelRect, PanelTrigger};
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{BODY, Ink};

/// How far the pointer travels before a press on a tab becomes a drag.
const DRAG_SLOP: f32 = 6.0;

/// The most a pointer may leave the rail, up and down, and still drop a
/// dragged tab: beyond this the drag is cancelled.
const DRAG_REACH: Dim = Dim::Control;

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
    /// dragged (or moved by its menu) and the place it now belongs at in the
    /// list, as if it were removed and inserted there. The strip does not
    /// reorder itself; the host reorders its list and passes it back.
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

// ---- Geometry --------------------------------------------------------------

/// The sizes the strip is laid out with, in logical pixels, for one density.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Metrics {
    /// The rail's padding, and the gap between its items: [`FIELD_INSET`].
    inset: f32,
    /// A tab's height, and the side of the tab list and New tab buttons: the
    /// rail's height less its inset above and below ([`Dim::InlineButton`]).
    height: f32,
    /// The least and the most a tab is wide.
    min: f32,
    max: f32,
}

impl Metrics {
    fn current() -> Self {
        Self {
            inset: FIELD_INSET.px(),
            height: Dim::InlineButton.px(),
            min: TAB_MIN.px(),
            max: TAB_MAX.px(),
        }
    }

    #[cfg(test)]
    fn in_class(class: SizeClass) -> Self {
        Self {
            inset: FIELD_INSET.px_in(class),
            height: Dim::InlineButton.px_in(class),
            min: TAB_MIN.px_in(class),
            max: TAB_MAX.px_in(class),
        }
    }
}

/// The narrowest a tab gets before the strip scrolls.
const TAB_MIN: Dim = Dim::TabMin;
/// The widest a tab is.
const TAB_MAX: Dim = Dim::Field;

/// How the strip fills `available` pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Fit {
    /// The width of one tab that is fully present.
    tab: f32,
    /// The width of the rail.
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
fn fit(metrics: Metrics, available: f32, target: usize, effective: f32, add: bool) -> Fit {
    let Metrics { inset, height, min, max } = metrics;
    let slot = height + inset;
    let inner = (available - 2.0 * inset).max(0.0);
    let room = (inner - if add { slot } else { 0.0 }).max(0.0);
    let needs = |count: f32| count * min + (count - 1.0).max(0.0) * inset;
    if needs(target as f32) > room + 0.01 {
        // Past the narrowest width: the rail is as wide as its container, and
        // the tab list button takes a slot beside the tabs.
        return Fit {
            tab: min,
            rail: available.max(2.0 * inset),
            region: (room - slot).max(0.0),
            overflow: true,
        };
    }
    let share = (room - (effective - 1.0).max(0.0) * inset) / effective.max(1.0);
    let tab = share.min(max).max(0.0);
    let region = effective * tab + (effective - 1.0).max(0.0) * inset;
    Fit {
        tab,
        rail: region + 2.0 * inset + if add { slot } else { 0.0 },
        region,
        overflow: false,
    }
}

/// Where a tab dragged from place `from` ends up when dropped into gap `gap`
/// (a gap is a place between tabs; 0 is before the first): the gaps on either
/// side of the tab itself leave it where it is.
fn destination(from: usize, gap: usize) -> usize {
    if gap > from { gap - 1 } else { gap }
}

/// The index, among `count` tabs of width `tab` set `inset` apart, of the gap
/// a pointer `x` pixels from the first tab's edge would drop a tab into: the
/// number of tabs whose middle is before it. 0 is before the first tab.
fn landing(x: f32, tab: f32, inset: f32, count: usize) -> usize {
    let unit = (tab + inset).max(1.0);
    let passed = ((x - tab / 2.0) / unit).floor() + 1.0;
    (passed.max(0.0) as usize).min(count)
}

/// The scroll offset, from the start of the tabs, that brings the tab at
/// `index` into a region `region` wide without moving more than it must:
/// `current` is where the region starts now.
fn scroll_into_view(index: usize, tab: f32, inset: f32, region: f32, current: f32) -> f32 {
    let left = index as f32 * (tab + inset);
    let right = left + tab;
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

/// A drag of a tab that is under way.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Drag {
    id: u64,
    /// Its place in the list when the drag began.
    from: usize,
    /// Where the pointer was, across the window, when it went down.
    start: f32,
    /// Whether the pointer has moved far enough for this to be a drag.
    active: bool,
    /// The gap it would drop into.
    gap: usize,
}

// ---- The strip -------------------------------------------------------------

impl Component for TabStrip {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let metrics = Metrics::current();
        let mut available = use_state(|| None::<f32>);
        let mut rail_area = use_state(Area::default);
        let mut button_area = use_state(Area::default);
        let mut drag = use_state(|| None::<Drag>);
        let mut seen = use_state(|| 0_u64);
        let mut tick = use_state(|| 0_u64);
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
        let _ = tick();

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
        let clock = use_animation(|_| {
            AnimNum::new(0.0, 1.0)
                .time(MOVE_MS)
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
                    timer(Duration::from_millis(MOVE_MS + 40)).await;
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
            metrics,
            available().unwrap_or(Dim::Page.px()),
            staying,
            effective,
            add,
        );
        let radius = Radius::Control.nested(metrics.inset);
        let unit = fit.tab + metrics.inset;
        let content = (effective * fit.tab + (effective - 1.0).max(0.0) * metrics.inset).max(0.0);
        let range = (content - fit.region).max(0.0);
        let offset = (-(<(i32, i32)>::from(scroll).0 as f32)).clamp(0.0, range);
        scrolled.set(offset);

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
                metrics.inset as i32,
                fit.region.round() as i32,
                fit.overflow,
                tick(),
            );
            use_side_effect_with_deps(&deps, move |deps| {
                let (index, tab, inset, region, overflow, _) = *deps;
                let Some(index) = index else { return };
                if !overflow {
                    return;
                }
                let mut scroll = scroll;
                let to = scroll_into_view(
                    index,
                    tab as f32,
                    inset as f32,
                    region as f32,
                    scrolled.get(),
                );
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
        let closable = live.len() > 1;
        let reorderable = self.on_reorder.is_some();

        // ---- Tabs
        let mut row_children: Vec<Element> = Vec::with_capacity(entries.len() * 2);
        for (position, entry) in entries.iter().enumerate() {
            let presence = entry.presence(progress);
            let id = entry.item.id;
            let place = live_ids.iter().position(|live| *live == id);
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
            row_children.push(Element::from(Tab {
                item: entry.item.clone(),
                width: fit.tab * presence,
                lead: if position > 0 { metrics.inset * presence } else { 0.0 },
                selected: id == self.selected,
                drop_target: self.drop_target == Some(id),
                closable,
                dragging: (*drag.read()).is_some_and(|drag| drag.active && drag.id == id),
                menu,
                on_select: self.on_select.clone(),
                on_close: self.on_close.clone(),
                on_hover: self.on_hover.clone(),
                on_drag: reorderable.then(|| {
                    let live_ids = live_ids.clone();
                    EventHandler::new(move |(id, x): (u64, f32)| {
                        if let Some(from) = live_ids.iter().position(|live| *live == id) {
                            drag.set(Some(Drag {
                                id,
                                from,
                                start: x,
                                active: false,
                                gap: from,
                            }));
                        }
                    })
                }),
                a11y_id: a11y_ids[position],
            }));
        }

        let key_ids: Vec<(u64, AccessibilityId)> = entries
            .iter()
            .zip(&a11y_ids)
            .filter(|(entry, _)| entry.life != Life::Leaving)
            .map(|(entry, a11y)| (entry.item.id, *a11y))
            .collect();
        let (on_select, on_close, on_new) = (
            self.on_select.clone(),
            self.on_close.clone(),
            self.on_new.clone(),
        );
        let selected = self.selected;
        let wants_focus = refocus.clone();
        let row = rect()
            .tw(&tokens, &[h(Dim::Control), flex_row(), items(Align::Center)])
            .on_key_down(move |event: Event<KeyboardEventData>| {
                let Some(index) = key_ids.iter().position(|(id, _)| *id == selected) else {
                    return;
                };
                let last = key_ids.len() - 1;
                let ctrl = event.modifiers.contains(Modifiers::CONTROL);
                let next = match &event.key {
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
        let fade = |at_start: bool| {
            let solid = Paint::Control.color(&tokens);
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
                    .tw(&tokens, &[w(Dim::Box5), h(Dim::Control)]),
                LinearGradient::new()
                    .angle(90.0)
                    .stop(GradientStop::new(from, 0.0))
                    .stop(GradientStop::new(to, 100.0)),
            )
        };
        // The fades need room of their own: a region narrower than two of them has none.
        let fades = fit.overflow && fit.region >= 2.0 * Dim::Box5.px();
        let dropping = drag.read().filter(|drag| drag.active);
        let region = rect()
            .tw(&tokens, &[h(Dim::Control), clip()])
            .width(Size::px(fit.region))
            .child(
                scroll_x_controlled(scroll)
                    .show_scrollbar(false)
                    .scroll_with_arrows(false)
                    .invert_scroll_wheel(true)
                    .child(row),
            )
            .maybe_child((fades && offset > 0.5).then(|| fade(true)))
            .maybe_child((fades && offset < range - 0.5).then(|| fade(false)))
            .maybe_child(dropping.map(|drag| {
                // Where a dropped tab would land: a line in the gap.
                let at = (drag.gap as f32 * unit - metrics.inset / 2.0 - offset)
                    .clamp(0.0, (fit.region - 2.0).max(0.0));
                rect()
                    .position(Position::new_absolute().top(0.0).left(at))
                    .interactive(false)
                    .tw(&tokens, &[w(Dim::Indicator), h(Dim::Control), bg(Paint::Accent)])
            }));

        // ---- The buttons at the end of the rail
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
        });
        let new_tab = self.on_new.clone().map(|on_new| {
            icon_button(Icon::ListAdd, on_new)
                .size(Dim::InlineButton)
                .radius(radius)
                .tooltip("New tab")
        });

        // ---- Drag to reorder
        let on_reorder = self.on_reorder.clone();
        let (tab, inset, count) = (fit.tab, metrics.inset, live_ids.len());
        let reach = DRAG_REACH.px();
        let rail = rect()
            .component("TabStrip")
            .a11y_role(AccessibilityRole::TabList)
            .a11y_alt("Tabs")
            .width(Size::px(fit.rail))
            .tw(
                &tokens,
                &[
                    h(Dim::Control),
                    flex_row(),
                    items(Align::Center),
                    px(FIELD_INSET),
                    gap(FIELD_INSET),
                    rounded(Radius::Control),
                    bg(Paint::Control),
                ],
            )
            .on_sized(move |event: Event<SizedEventData>| {
                rail_area.set_if_modified(event.area);
            })
            .on_capture_global_pointer_move(move |event: Event<PointerEventData>| {
                let Some(mut current) = *drag.peek() else { return };
                let x = event.global_location().x as f32;
                if !current.active {
                    if (x - current.start).abs() < DRAG_SLOP {
                        return;
                    }
                    current.active = true;
                }
                event.prevent_default();
                let origin = rail_area.peek().min_x() + inset - offset;
                current.gap = landing(x - origin, tab, inset, count);
                drag.set_if_modified(Some(current));
            })
            .on_global_pointer_up(move |event: Event<PointerEventData>| {
                let Some(done) = drag.write().take() else { return };
                if !done.active {
                    return;
                }
                let area = *rail_area.peek();
                let y = event.global_location().y as f32;
                if y < area.min_y() - reach || y > area.max_y() + reach {
                    return;
                }
                let to = destination(done.from, done.gap);
                if to != done.from {
                    if let Some(on_reorder) = &on_reorder {
                        on_reorder.call((done.id, to));
                    }
                }
            })
            .child(region)
            .maybe_child(tab_list)
            .maybe_child(new_tab);

        rect()
            .tw(&tokens, &[w_full(), h(Dim::Control)])
            .on_sized(move |event: Event<SizedEventData>| {
                available.set_if_modified(Some(event.area.width()));
            })
            .child(rail)
    }
}

// ---- A tab -----------------------------------------------------------------

#[derive(PartialEq)]
struct Tab {
    item: TabItem,
    /// Its width at this moment of the motion.
    width: f32,
    /// The gap before it, which grows and shrinks with it.
    lead: f32,
    selected: bool,
    drop_target: bool,
    /// A lone tab has nothing to close into.
    closable: bool,
    /// The tab is being dragged to another place.
    dragging: bool,
    /// Its right click and long press menu.
    menu: Vec<MenuItem>,
    on_select: EventHandler<u64>,
    on_close: EventHandler<u64>,
    on_hover: EventHandler<(u64, bool)>,
    /// A press with the primary button began; set when the strip reorders.
    on_drag: Option<EventHandler<(u64, f32)>>,
    a11y_id: AccessibilityId,
}

impl Component for Tab {
    /// A tab keeps its hover and focus when another one before it closes.
    fn render_key(&self) -> DiffKey {
        DiffKey::U64(self.item.id)
    }

    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let metrics = Metrics::current();
        let radius = Radius::Control.nested(metrics.inset);
        let mut hovering = use_state(|| false);
        let mut area = use_state(Area::default);
        let a11y_id = self.a11y_id;
        let focus = use_focus(a11y_id);
        let press = use_press();
        let overlays = use_try_consume::<Overlays>();
        let id = self.item.id;
        let (on_select, on_close) = (self.on_select.clone(), self.on_close.clone());
        let (on_enter, on_leave) = (self.on_hover.clone(), self.on_hover.clone());
        let on_drag = self.on_drag.clone();
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
        let show_close = self.closable
            && (self.selected || (hovering() && pointer_kind() == PointerKind::Mouse));
        let control = rect()
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
                    rounded(radius),
                    clip(),
                    bg(if self.drop_target {
                        Paint::AccentSoft
                    } else if self.selected {
                        Paint::Raised
                    } else {
                        Paint::Transparent
                    }),
                ],
            )
            .maybe(self.selected, |tab| tab.tw(&tokens, &[shadow(Elevation::Knob)]))
            .maybe(self.dragging, |tab| tab.opacity(0.5))
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
                    if let Some(on_drag) = &on_drag {
                        on_drag.call((id, event.global_location().x as f32));
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
            // The control holds no padding, so its focus ring, which is placed
            // from the control's own corner, hugs the tab; the row inside has it.
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
                            if self.selected || hovering() {
                                Ink::Text.color(&tokens)
                            } else {
                                Ink::Muted.color(&tokens)
                            },
                        )
                    }))
                    .child(
                        rect().tw(&tokens, &[w_flex()]).child(
                            BODY.in_ink(if self.selected || hovering() {
                                Ink::Text
                            } else {
                                Ink::Muted
                            })
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
        // The slot is as tall as the rail, so the raised tab's shadow is not
        // cut by the scroll region. It holds the gap before the tab and the
        // tab, and is as wide as the motion allows. No sibling of a tab is
        // unkeyed, so tabs can change places.
        rect()
            .tw(&tokens, &[h(Dim::Control), flex_row(), items(Align::Center)])
            .width(Size::px(self.width + self.lead))
            .child(rect().width(Size::px(self.lead)))
            .child(
                control
                    .focusable_inside(focus(), radius)
                    .tw(&tokens, &[h(Dim::InlineButton)])
                    .width(Size::px(self.width)),
            )
    }
}

/// The close button of a tab: always laid out, so a tab does not change width
/// when it is hovered, and drawn only while it can be used. It is the standard
/// icon button, [`FIELD_INSET`] from the tab's edges like a field's trailing
/// button: 20 px at mouse density, and the whole 44 px touch target at compact
/// density.
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
        let radius = Radius::Control.nested(inset).nested(inset);
        let slot = rect().component("TabClose").tw(&tokens, &[w(size), h(size)]);
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
    fn the_rail_never_passes_its_container_at_any_width() {
        for class in CLASSES {
            let metrics = Metrics::in_class(class);
            for add in [false, true] {
                for count in 1..=14 {
                    for available in (100..=900).step_by(7) {
                        let available = available as f32;
                        let fit = fit(metrics, available, count, count as f32, add);
                        assert!(
                            fit.rail <= available.max(2.0 * metrics.inset) + 0.01,
                            "{class:?} add {add}: {count} tabs in {available}px make a rail {} wide",
                            fit.rail
                        );
                        let slot = metrics.height + metrics.inset;
                        let end = 2.0 * metrics.inset
                            + fit.region
                            + if add { slot } else { 0.0 }
                            + if fit.overflow { slot } else { 0.0 };
                        assert!(
                            end <= fit.rail + 0.01,
                            "{class:?}: the region and the buttons ({end}) outrun the rail ({})",
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
            let metrics = Metrics::in_class(class);
            let wide = fit(metrics, 2000.0, 3, 3.0, true);
            assert_eq!(wide.tab, metrics.max, "{class:?}: few tabs are as wide as the token");
            assert!(!wide.overflow);
            let mut last = f32::MAX;
            for count in 1..=40 {
                let fit = fit(metrics, 500.0, count, count as f32, true);
                assert!(fit.tab <= last + 0.01, "{class:?}: more tabs never widen the tabs");
                last = fit.tab;
                if fit.overflow {
                    assert_eq!(fit.tab, metrics.min, "{class:?}: a scrolling strip holds tabs at the minimum");
                } else {
                    assert!(fit.tab >= metrics.min - 0.01 && fit.tab <= metrics.max + 0.01);
                    let used = count as f32 * fit.tab + (count as f32 - 1.0) * metrics.inset;
                    assert!((used - fit.region).abs() < 0.01, "{class:?}: the tabs fill their region");
                }
            }
            assert!(fit(metrics, 500.0, 40, 40.0, true).overflow);
        }
    }

    #[test]
    fn overflow_begins_when_the_minimum_widths_no_longer_fit() {
        let metrics = Metrics::in_class(SizeClass::Regular);
        // 4 px inset, a 28 px button: a rail of 500 holds 492 - 32 = 460 of tabs.
        // Four at 96 with three 4 px gaps are 396; five are 496.
        assert!(!fit(metrics, 500.0, 4, 4.0, true).overflow);
        assert!(fit(metrics, 500.0, 5, 5.0, true).overflow);
        // Without the New tab button there is 32 px more room: 5 fit in 510.
        assert!(fit(metrics, 500.0, 5, 5.0, false).overflow);
        assert!(!fit(metrics, 510.0, 5, 5.0, false).overflow);
    }

    #[test]
    fn a_leaving_tab_does_not_change_whether_the_strip_overflows() {
        let metrics = Metrics::in_class(SizeClass::Regular);
        let settled = fit(metrics, 500.0, 4, 4.0, true);
        let leaving = fit(metrics, 500.0, 4, 4.6, true);
        assert_eq!(settled.overflow, leaving.overflow);
    }

    #[test]
    fn a_pointer_lands_a_dragged_tab_in_the_gap_it_is_nearest() {
        // Three tabs of 100 with 4 px between: middles at 50, 154 and 258.
        assert_eq!(landing(-20.0, 100.0, 4.0, 3), 0);
        assert_eq!(landing(49.0, 100.0, 4.0, 3), 0);
        assert_eq!(landing(51.0, 100.0, 4.0, 3), 1);
        assert_eq!(landing(160.0, 100.0, 4.0, 3), 2);
        assert_eq!(landing(900.0, 100.0, 4.0, 3), 3);
    }

    #[test]
    fn dropping_beside_itself_leaves_a_tab_where_it_is() {
        assert_eq!(destination(1, 1), 1);
        assert_eq!(destination(1, 2), 1);
        assert_eq!(destination(0, 3), 2);
        assert_eq!(destination(2, 0), 0);
    }

    #[test]
    fn the_selected_tab_is_scrolled_in_by_the_least_distance() {
        // Tabs of 96 with 4 px between, in a region 250 wide.
        let view = |index, current| scroll_into_view(index, 96.0, 4.0, 250.0, current);
        assert_eq!(view(1, 0.0), 0.0, "a tab in view does not scroll");
        assert_eq!(view(2, 0.0), 296.0 - 250.0, "a tab past the end scrolls its end into view");
        assert_eq!(view(0, 150.0), 0.0, "a tab before the start scrolls its start into view");
    }

    #[test]
    fn a_new_tab_grows_in_and_a_closed_tab_stays_until_it_has_shrunk() {
        let tabs = |ids: &[u64]| ids.iter().map(|id| item(*id)).collect::<Vec<_>>();
        let mut entries = Vec::new();
        assert!(!merge(&mut entries, &tabs(&[1, 2]), false), "the first tabs do not grow in");
        assert!(entries.iter().all(|entry| entry.life == Life::Live));

        assert!(merge(&mut entries, &tabs(&[1, 2, 3]), true));
        assert_eq!(entries[2].life, Life::Entering);
        assert!(!merge(&mut entries, &tabs(&[1, 2, 3]), true), "the same list is no new motion");

        assert!(merge(&mut entries, &tabs(&[1, 3]), true));
        let lives: Vec<(u64, Life)> = entries.iter().map(|entry| (entry.item.id, entry.life)).collect();
        assert_eq!(
            lives,
            vec![(1, Life::Live), (2, Life::Leaving), (3, Life::Live)],
            "tab 2 stays where it was while it shrinks; tab 3 had arrived by the new motion"
        );
        assert!(!merge(&mut entries, &tabs(&[1, 3]), true), "a leaving tab does not restart its motion");
    }

    #[test]
    fn reordering_is_not_a_motion() {
        let tabs = |ids: &[u64]| ids.iter().map(|id| item(*id)).collect::<Vec<_>>();
        let mut entries = Vec::new();
        merge(&mut entries, &tabs(&[1, 2, 3]), false);
        assert!(!merge(&mut entries, &tabs(&[3, 1, 2]), true));
        let ids: Vec<u64> = entries.iter().map(|entry| entry.item.id).collect();
        assert_eq!(ids, vec![3, 1, 2]);
    }

    #[test]
    fn the_inset_is_the_nested_radius_rule() {
        // A tab is the rail's radius less the rail's inset; the close button
        // is the tab's less its own.
        for class in CLASSES {
            let inset = FIELD_INSET.px_in(class);
            let tab = Radius::Control.nested(inset);
            let close = tab.nested(inset);
            assert!((Radius::Control.px() - inset - tab.px()).abs() < 0.01);
            match class {
                SizeClass::Regular => assert_eq!((tab, close), (Radius::Inner, Radius::Mark)),
                SizeClass::Compact => assert_eq!((tab, close), (Radius::Control, Radius::Control)),
            }
        }
    }

    #[test]
    fn at_compact_density_every_target_is_the_touch_minimum() {
        let metrics = Metrics::in_class(SizeClass::Compact);
        assert!(metrics.height >= TOUCH_MIN, "a tab is {} tall", metrics.height);
        assert_eq!(Dim::InlineButton.px_in(SizeClass::Compact), metrics.height);
        assert!(TAB_MIN.px_in(SizeClass::Compact) >= TOUCH_MIN);
    }
}

#[cfg(all(test, feature = "testing"))]
mod geometry_tests {
    use std::time::Duration;

    use freya::prelude::*;
    use freya_core::elements::label::Label;
    use torin::prelude::Area;

    use super::*;
    use crate::snapshot::Snapshot;
    use crate::theme::provide_tokens;
    use crate::window::WindowHost;

    const LONG: &str = "A name far too long for the room it is given, which must be cut";

    /// The window root: it provides the tokens the window host reads.
    #[derive(PartialEq, Clone, Copy)]
    struct Root(Fixed);

    impl Component for Root {
        fn render(&self) -> impl IntoElement {
            let _ = provide_tokens();
            WindowHost::new(self.0)
        }
    }

    #[derive(PartialEq, Clone, Copy)]
    struct Fixed {
        count: usize,
        container: f32,
        add: bool,
        long: bool,
    }

    impl Component for Fixed {
        fn render(&self) -> impl IntoElement {
            let tokens = use_tokens();
            let tabs = (0..self.count)
                .map(|index| {
                    let id = index as u64 + 1;
                    if self.long && index == 0 {
                        TabItem::new(id, LONG)
                    } else {
                        TabItem::new(id, format!("Folder {id}"))
                    }
                })
                .collect();
            let mut strip = tab_strip(tabs, 1);
            if self.add {
                strip = strip.on_new(|_| {});
            }
            rect().expanded().tw(&tokens, &[p(S4)]).child(
                rect()
                    .component("Container")
                    .width(Size::px(self.container))
                    .child(strip),
            )
        }
    }

    /// Every box a strip draws, by role.
    #[derive(Debug)]
    struct Geo {
        container: Area,
        rail: Area,
        tabs: Vec<Area>,
        closes: Vec<Area>,
        titles: Vec<Area>,
        buttons: Vec<Area>,
        inset: f32,
        height: f32,
        rail_height: f32,
    }

    fn named(shot: &mut Snapshot, name: &'static str) -> Vec<Area> {
        shot.runner().find_many(|node, element| {
            (element.accessibility().builder.class_name() == Some(name)).then(|| node.layout().area)
        })
    }

    fn read(shot: &mut Snapshot) -> Geo {
        let container = named(shot, "Container").into_iter().next().expect("the container");
        let rail = named(shot, "TabStrip").into_iter().next().expect("the rail");
        let tabs = named(shot, "Tab");
        let closes = named(shot, "TabClose");
        let titles = shot.runner().find_many(|node, element| {
            let label = Label::try_downcast(element)?;
            (label.text.starts_with("Folder") || label.text.starts_with("Tab ") || label.text.starts_with("A name"))
                .then(|| node.layout().area)
        });
        let mut buttons: Vec<Area> = named(shot, "IconButton")
            .into_iter()
            .filter(|button| closes.iter().all(|close| !contains(close, button, 0.5)))
            .collect();
        buttons.sort_by(|a, b| a.min_x().total_cmp(&b.min_x()));
        let (inset, height, rail_height) = shot.runner().run_in(|| {
            (FIELD_INSET.px(), Dim::InlineButton.px(), Dim::Control.px())
        });
        Geo { container, rail, tabs, closes, titles, buttons, inset, height, rail_height }
    }

    fn contains(outer: &Area, inner: &Area, slack: f32) -> bool {
        inner.min_x() >= outer.min_x() - slack
            && inner.max_x() <= outer.max_x() + slack
            && inner.min_y() >= outer.min_y() - slack
            && inner.max_y() <= outer.max_y() + slack
    }

    fn render(window: f32, fixed: Fixed) -> (Snapshot, Geo) {
        let mut shot = Snapshot::new(
            move || Root(fixed).into_element(),
            (window, 200.0),
        );
        shot.settle(Duration::from_millis(400));
        let geo = read(&mut shot);
        (shot, geo)
    }

    /// Check one strip: everything inside its container and its rail, the
    /// tabs one width, the close slot reserved on every tab, the end buttons
    /// pinned and clear of the tabs.
    fn check(case: &str, window: f32, container: f32, count: usize, add: bool, long: bool) {
        let (_shot, geo) = render(window, Fixed { count, container, add, long });
        let Geo { rail, inset, height, rail_height, .. } = geo;
        let near = |a: f32, b: f32| (a - b).abs() < 0.6;
        assert!(contains(&geo.container, &rail, 0.5), "{case}: the rail {rail:?} spills out of its container {:?}", geo.container);
        assert!(near(rail.height(), rail_height), "{case}: the rail is {} tall", rail.height());
        assert_eq!(geo.tabs.len(), count, "{case}: a tab is drawn for each");
        // Tabs: one height and, while they all show, one width.
        for tab in &geo.tabs {
            assert!(near(tab.height(), height), "{case}: a tab is {} tall, not {height}", tab.height());
            assert!(near(tab.min_y() - rail.min_y(), inset), "{case}: a tab sits {inset} px from the rail's top");
            assert!(near(rail.max_y() - tab.max_y(), inset), "{case}: a tab sits {inset} px from the rail's bottom");
        }
        let first = geo.tabs[0];
        assert!(near(first.min_x() - rail.min_x(), inset), "{case}: the first tab keeps the rail's inset");
        let width = first.width();
        for tab in &geo.tabs {
            assert!(near(tab.width(), width), "{case}: tab widths differ: {} and {width}", tab.width());
        }
        // The close slot is reserved on every tab, with an equal inset above, below and after it.
        assert_eq!(geo.closes.len(), if count > 1 { count } else { 0 }, "{case}: a lone tab has no close slot");
        for (tab, close) in geo.tabs.iter().zip(&geo.closes) {
            assert!(contains(tab, close, 0.5), "{case}: the close slot {close:?} is outside its tab {tab:?}");
            assert!(near(tab.max_x() - close.max_x(), inset), "{case}: the close slot is {} from the tab's end", tab.max_x() - close.max_x());
            assert!(
                near(close.min_y() - tab.min_y(), tab.max_y() - close.max_y()),
                "{case}: the close slot is not centred in its tab"
            );
            assert!(near(close.width(), close.height()), "{case}: the close slot is square");
        }
        // A title is cut before the close slot, never under it.
        for (index, title) in geo.titles.iter().enumerate() {
            if let Some(close) = geo.closes.get(index) {
                assert!(title.max_x() <= close.min_x() + 0.5, "{case}: title {index} runs under the close slot");
            }
        }
        // The end buttons: inside the rail, square, centred, pinned to its end, clear of the tabs.
        let expected = usize::from(add) + usize::from(width <= TAB_MIN_PX + 0.6 && count > 1);
        for button in &geo.buttons {
            assert!(contains(&rail, button, 0.5), "{case}: a button {button:?} is outside the rail {rail:?}");
            assert!(near(button.width(), height) && near(button.height(), height), "{case}: a button is {}x{}", button.width(), button.height());
            assert!(near(button.min_y() - rail.min_y(), inset), "{case}: a button is centred in the rail");
        }
        if add {
            let new = geo.buttons.last().expect("the New tab button");
            assert!(near(rail.max_x() - new.max_x(), inset), "{case}: the New tab button is pinned to the rail's end: {:?}, rail {rail:?}, tabs {:?}", geo.buttons, geo.tabs);
            if let Some(list) = geo.buttons.iter().rev().nth(1) {
                assert!(near(new.min_x() - list.max_x(), inset), "{case}: the buttons are an inset apart");
            }
        }
        assert!(geo.buttons.len() <= expected.max(2), "{case}: {} end buttons", geo.buttons.len());
        let overflowing = count as f32 * TAB_MIN_PX + (count as f32 - 1.0) * inset > container - 2.0 * inset - if add { height + inset } else { 0.0 } + 0.01;
        if !overflowing {
            // Every tab is inside the rail, before the end buttons.
            let limit = geo.buttons.first().map_or(rail.max_x() - inset, |button| button.min_x() - inset);
            let last = geo.tabs.last().unwrap();
            assert!(last.max_x() <= limit + 0.6, "{case}: the last tab ({}) overlaps what follows ({limit})", last.max_x());
            for pair in geo.tabs.windows(2) {
                assert!(near(pair[1].min_x() - pair[0].max_x(), inset), "{case}: tabs are an inset apart");
            }
            assert!(width <= TAB_MAX_PX + 0.6, "{case}: tabs are {width} wide, past the token");
        } else {
            assert!(near(width, TAB_MIN_PX), "{case}: scrolling tabs are {width} wide, not the minimum");
            assert!(geo.buttons.len() >= 1 + usize::from(add), "{case}: the tab list button is drawn when tabs overflow");
            assert!(near(rail.width(), geo.container.width()), "{case}: an overflowing rail fills its container");
        }
    }

    const TAB_MIN_PX: f32 = 96.0;
    const TAB_MAX_PX: f32 = 240.0;

    #[test]
    fn at_mouse_density_everything_is_inside_the_rail_at_every_width() {
        for container in [260.0, 340.0, 440.0, 600.0, 720.0] {
            for (count, add, long) in [(1, true, false), (2, true, false), (3, true, false), (3, false, false), (6, true, false), (12, true, false), (2, true, true)] {
                check(&format!("regular {container}px, {count} tabs, add {add}, long {long}"), 800.0, container, count, add, long);
            }
        }
    }

    #[test]
    fn at_compact_density_everything_is_inside_the_rail_at_every_width() {
        for container in [200.0, 280.0, 340.0] {
            for (count, add, long) in [(1, true, false), (2, true, false), (3, true, false), (6, true, false), (2, true, true)] {
                let (window, _) = (390.0, ());
                // The compact density's tokens are not the regular ones.
                check_compact(&format!("compact {container}px, {count} tabs, add {add}, long {long}"), window, container, count, add, long);
            }
        }
    }

    fn check_compact(case: &str, window: f32, container: f32, count: usize, add: bool, long: bool) {
        let (_shot, geo) = render(window, Fixed { count, container, add, long });
        let near = |a: f32, b: f32| (a - b).abs() < 0.6;
        assert_eq!(geo.inset, 0.0, "{case}: no rail inset at compact density");
        assert!(contains(&geo.container, &geo.rail, 0.5), "{case}: the rail spills out of its container");
        assert!(near(geo.rail.height(), 44.0), "{case}: the rail is {} tall", geo.rail.height());
        for tab in &geo.tabs {
            assert!(near(tab.height(), 44.0), "{case}: a tab is {} tall: not a touch target", tab.height());
        }
        for close in &geo.closes {
            assert!(near(close.width(), 44.0) && near(close.height(), 44.0), "{case}: the close target is {}x{}", close.width(), close.height());
        }
        for button in &geo.buttons {
            assert!(near(button.width(), 44.0) && near(button.height(), 44.0), "{case}: a button is {}x{}", button.width(), button.height());
            assert!(contains(&geo.rail, button, 0.5), "{case}: a button is outside the rail");
        }
        let widths: Vec<f32> = geo.tabs.iter().map(Area::width).collect();
        assert!(widths.windows(2).all(|pair| near(pair[0], pair[1])), "{case}: tab widths differ: {widths:?}");
        if add {
            let new = geo.buttons.last().unwrap_or_else(|| panic!("{case}: no New tab button; {geo:?}"));
            assert!(near(geo.rail.max_x() - new.max_x(), 0.0), "{case}: the New tab button is flush with the rail's end");
        }
        for (tab, close) in geo.tabs.iter().zip(&geo.closes) {
            assert!(near(tab.max_x(), close.max_x()), "{case}: the close target is flush with its tab");
        }
    }
}

#[cfg(all(test, feature = "testing"))]
mod interaction_tests {
    use std::time::Duration;

    use freya::prelude::*;
    use torin::prelude::Area;

    use super::*;
    use crate::snapshot::Snapshot;
    use crate::theme::provide_tokens;
    use crate::window::WindowHost;

    /// The window root: it provides the tokens the window host reads.
    #[derive(PartialEq)]
    struct Root(f32);

    impl Component for Root {
        fn render(&self) -> impl IntoElement {
            let _ = provide_tokens();
            WindowHost::new(Strip(self.0))
        }
    }

    /// A strip that keeps its own tabs, like a host does.
    #[derive(PartialEq)]
    struct Strip(f32);

    impl Component for Strip {
        fn render(&self) -> impl IntoElement {
            let tokens = use_tokens();
            let mut tabs = use_state(|| {
                vec![TabItem::new(1, "Home"), TabItem::new(2, "Documents"), TabItem::new(3, "Downloads")]
            });
            let mut selected = use_state(|| 1_u64);
            let mut next = use_state(|| 4_u64);
            let strip = tab_strip(tabs.read().clone(), selected())
                .on_select(move |id| selected.set(id))
                .on_close(move |id| {
                    let mut list = tabs.write();
                    let Some(index) = list.iter().position(|tab| tab.id == id) else {
                        return;
                    };
                    list.remove(index);
                    if selected() == id {
                        selected.set(list[index.min(list.len() - 1)].id);
                    }
                })
                .on_new(move |_| {
                    let id = next();
                    next.set(id + 1);
                    tabs.write().push(TabItem::new(id, format!("Tab {id}")));
                    selected.set(id);
                })
                .on_reorder(move |(id, to): (u64, usize)| {
                    let mut list = tabs.write();
                    if let Some(from) = list.iter().position(|tab| tab.id == id) {
                        let tab = list.remove(from);
                        let to = to.min(list.len());
                        list.insert(to, tab);
                    }
                });
            rect().expanded().tw(&tokens, &[p(S4)]).child(
                rect().component("Container").width(Size::px(self.0)).child(strip),
            )
        }
    }

    fn start(width: f32) -> Snapshot {
        let mut shot = Snapshot::new(move || Root(width).into_element(), (800.0, 200.0));
        shot.settle(Duration::from_millis(400));
        shot
    }

    /// The tabs in order: name and whether it is the selected one.
    fn tabs_of(shot: &mut Snapshot) -> Vec<(String, bool)> {
        shot.runner().find_many(|_, element| {
            let data = element.accessibility();
            (data.builder.class_name() == Some("Tab")).then(|| {
                (
                    data.builder.label().unwrap_or_default().to_string(),
                    data.builder.is_selected() == Some(true),
                )
            })
        })
    }

    fn selected(shot: &mut Snapshot) -> String {
        tabs_of(shot)
            .into_iter()
            .find(|(_, selected)| *selected)
            .map(|(name, _)| name)
            .unwrap_or_default()
    }

    fn areas(shot: &mut Snapshot, name: &'static str) -> Vec<Area> {
        shot.runner().find_many(|node, element| {
            (element.accessibility().builder.class_name() == Some(name)).then(|| node.layout().area)
        })
    }

    fn rest(shot: &mut Snapshot) {
        shot.settle(Duration::from_millis(400));
    }

    #[test]
    fn the_keyboard_selects_closes_and_adds() {
        let mut shot = start(600.0);
        // Tab reaches the selected tab, the only one in the tab order.
        shot.key(Key::Named(NamedKey::Tab));
        rest(&mut shot);
        assert_eq!(selected(&mut shot), "Home");
        shot.key(Key::Named(NamedKey::ArrowRight));
        rest(&mut shot);
        assert_eq!(selected(&mut shot), "Documents");
        shot.key(Key::Named(NamedKey::End));
        rest(&mut shot);
        assert_eq!(selected(&mut shot), "Downloads");
        shot.key(Key::Named(NamedKey::ArrowRight));
        rest(&mut shot);
        assert_eq!(selected(&mut shot), "Downloads", "Right stops at the last tab");
        shot.key(Key::Named(NamedKey::Home));
        rest(&mut shot);
        assert_eq!(selected(&mut shot), "Home");
        // The focus follows the selection, so the keys keep working.
        shot.key(Key::Named(NamedKey::ArrowRight));
        shot.key(Key::Named(NamedKey::ArrowRight));
        rest(&mut shot);
        assert_eq!(selected(&mut shot), "Downloads");

        shot.key(Key::Named(NamedKey::Delete));
        rest(&mut shot);
        let names: Vec<String> = tabs_of(&mut shot).into_iter().map(|(name, _)| name).collect();
        assert_eq!(names, ["Home", "Documents"], "Delete closes the selected tab");
        assert_eq!(selected(&mut shot), "Documents", "the neighbour takes the selection");
        shot.key(Key::Named(NamedKey::ArrowLeft));
        rest(&mut shot);
        assert_eq!(selected(&mut shot), "Home", "the focus followed the close");

        shot.key_with(Key::Character("t".into()), Modifiers::CONTROL);
        rest(&mut shot);
        assert_eq!(tabs_of(&mut shot).len(), 3, "Ctrl+T adds a tab");
        assert_eq!(selected(&mut shot), "Tab 4");
        shot.key_with(Key::Character("w".into()), Modifiers::CONTROL);
        rest(&mut shot);
        assert_eq!(tabs_of(&mut shot).len(), 2, "Ctrl+W closes the selected tab");
    }

    #[test]
    fn a_lone_tab_cannot_be_closed() {
        let mut shot = start(600.0);
        shot.key(Key::Named(NamedKey::Tab));
        shot.key(Key::Named(NamedKey::Delete));
        rest(&mut shot);
        shot.key(Key::Named(NamedKey::Delete));
        rest(&mut shot);
        assert_eq!(tabs_of(&mut shot).len(), 1);
        shot.key(Key::Named(NamedKey::Delete));
        rest(&mut shot);
        assert_eq!(tabs_of(&mut shot).len(), 1, "the last tab stays");
        assert!(areas(&mut shot, "TabClose").is_empty(), "and it has no close button");
    }

    #[test]
    fn the_new_tab_button_adds_and_the_widths_never_leave_the_rail_while_they_move() {
        let mut shot = start(600.0);
        let rail = areas(&mut shot, "TabStrip")[0];
        let buttons: Vec<Area> = areas(&mut shot, "IconButton");
        let new = buttons.last().copied().expect("the New tab button");
        let before = areas(&mut shot, "Tab");
        assert_eq!(before.len(), 3);
        shot.click(new.min_x() + new.width() / 2.0, new.min_y() + new.height() / 2.0);
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..30 {
            shot.settle(Duration::from_millis(16));
            let rail_now = areas(&mut shot, "TabStrip")[0];
            let tabs = areas(&mut shot, "Tab");
            seen.insert(tabs.len());
            for tab in &tabs {
                assert!(
                    tab.max_x() <= rail_now.max_x() + 0.6,
                    "a tab at {tab:?} is outside the rail {rail_now:?} while a tab arrives"
                );
            }
            let first = tabs[0].width();
            let widths: Vec<f32> = tabs.iter().take(3).map(Area::width).collect();
            assert!(
                widths.iter().all(|width| (width - first).abs() < 0.6),
                "the tabs that were already there keep one width while the new one grows: {widths:?}"
            );
        }
        assert!(seen.contains(&4));
        rest(&mut shot);
        let after = areas(&mut shot, "Tab");
        assert_eq!(after.len(), 4);
        assert!(after[0].width() < before[0].width(), "the tabs made room");
        assert!(rail.width() > 0.0);
        assert_eq!(selected(&mut shot), "Tab 4", "the new tab is selected");
    }

    #[test]
    fn closing_a_tab_with_its_button_shrinks_it_away_and_the_others_widen() {
        let mut shot = start(600.0);
        // The selected tab shows its close button.
        let close = areas(&mut shot, "IconButton")
            .into_iter()
            .find(|button| button.width() < 24.0)
            .expect("the selected tab's close button");
        let before = areas(&mut shot, "Tab")[1].width();
        shot.click(close.min_x() + close.width() / 2.0, close.min_y() + close.height() / 2.0);
        let mut widest_gap = 0.0_f32;
        let mut last_end = None;
        for _ in 0..30 {
            shot.settle(Duration::from_millis(16));
            let rail = areas(&mut shot, "TabStrip")[0];
            let tabs = areas(&mut shot, "Tab");
            for tab in &tabs {
                assert!(tab.max_x() <= rail.max_x() + 0.6, "a tab leaves the rail while one closes");
            }
            let end = tabs.last().unwrap().max_x();
            if let Some(last) = last_end {
                widest_gap = widest_gap.max((end - last as f32).abs());
            }
            last_end = Some(end);
        }
        rest(&mut shot);
        let names: Vec<String> = tabs_of(&mut shot).into_iter().map(|(name, _)| name).collect();
        assert_eq!(names, ["Documents", "Downloads"]);
        assert!(areas(&mut shot, "Tab")[0].width() >= before - 0.6, "the remaining tabs are no narrower");
        assert!(widest_gap < 60.0, "the strip closed ranks in steps, not in one jump ({widest_gap})");
    }

    #[test]
    fn dragging_a_tab_past_its_neighbour_reorders_and_a_short_drag_does_not() {
        let mut shot = start(600.0);
        let tabs = areas(&mut shot, "Tab");
        let (a, c) = (tabs[0], tabs[2]);
        let y = a.min_y() + a.height() / 2.0;
        // A drag from the first tab to the end of the last.
        shot.drag((a.min_x() + 30.0, y), (c.max_x() - 10.0, y));
        rest(&mut shot);
        let names: Vec<String> = tabs_of(&mut shot).into_iter().map(|(name, _)| name).collect();
        assert_eq!(names, ["Documents", "Downloads", "Home"]);
        assert_eq!(selected(&mut shot), "Home", "the dragged tab is the selected one");
        // A drag that stays on the tab only selects it.
        let tabs = areas(&mut shot, "Tab");
        let b = tabs[1];
        shot.drag((b.min_x() + 30.0, y), (b.min_x() + 33.0, y));
        rest(&mut shot);
        let names: Vec<String> = tabs_of(&mut shot).into_iter().map(|(name, _)| name).collect();
        assert_eq!(names, ["Documents", "Downloads", "Home"]);
    }

    #[test]
    fn many_tabs_scroll_with_the_new_tab_button_pinned_and_the_selection_in_view() {
        let mut shot = start(300.0);
        let new_x = |shot: &mut Snapshot| areas(shot, "IconButton").last().copied().expect("the New tab button").min_x();
        let pinned = new_x(&mut shot);
        shot.key(Key::Named(NamedKey::Tab));
        for _ in 0..8 {
            shot.key_with(Key::Character("t".into()), Modifiers::CONTROL);
            rest(&mut shot);
        }
        assert_eq!(tabs_of(&mut shot).len(), 11);
        assert_eq!(new_x(&mut shot), pinned, "the New tab button does not move while the tabs scroll");
        // The newest tab, selected, is scrolled into the region, which ends before the buttons.
        let rail = areas(&mut shot, "TabStrip")[0];
        let tab = areas(&mut shot, "Tab").into_iter().last().unwrap();
        let tab_list = areas(&mut shot, "IconButton").into_iter().rev().nth(1).expect("the tab list button");
        assert!(
            tab.max_x() <= tab_list.min_x() + 0.6 && tab.min_x() >= rail.min_x(),
            "the selected tab {tab:?} is in view, before the buttons ({tab_list:?}) in the rail {rail:?}"
        );
        shot.key(Key::Named(NamedKey::Home));
        rest(&mut shot);
        let first = areas(&mut shot, "Tab")[0];
        assert!(first.min_x() >= rail.min_x() && first.max_x() <= tab_list.min_x() + 0.6, "Home scrolls back to the first tab");
    }
}
