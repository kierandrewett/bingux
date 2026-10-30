//! A list whose items the person reorders. The items are rows in one card,
//! as in a settings page, and each has a grip at its end: drag the grip, or
//! focus it and press Up or Down. On a touch screen, press and hold anywhere
//! on a row.
//!
//! * **Rows.** An item is a [`Row`](crate::rows::Row), or any element of
//!   its height, drawn exactly as in a [`Group`](crate::group::Group): flush
//!   in the card, a hairline between rows, the content at the card's gutter.
//!   The grip is a [`Dim::RowButton`] square at the row's end, the same
//!   distance ([`ROW_INSET`]) from the row's top, bottom and end as any
//!   trailing button, with a 16 px [`Icon::DragHandle`] in muted ink that
//!   brightens on hover.
//! * **Dragging.** The held row lifts: it grows a little, takes a shadow
//!   ([`lift_shadow`]) and becomes slightly translucent, and follows the
//!   pointer along the list. The rows it passes slide a row height out of
//!   its way, and nothing outside the list moves. Letting go settles the row
//!   into its place with the same spring and fades the shadow; Escape slides
//!   everything back and changes nothing. Held near the top or bottom edge of
//!   the page, the page scrolls.
//! * **Keyboard.** Up, Down, Home and End on the focused grip, and Alt+Up and
//!   Alt+Down on a focused row, move the item one place (or to an end) with
//!   the same slide. The keyboard focus then follows the item.
//! * **Touch.** Pressing the grip picks the row up at once; pressing a row
//!   and holding for half a second does too, and the lift is the cue that it
//!   is held. A touch that moves before then scrolls the page as it always
//!   does.
//! * **Long lists.** [`Sortable::drop_indicator`] keeps the other rows still
//!   and shows an accent line in the gap where the item would land, so a
//!   list of fifty rows does not shuffle under the pointer.
//! * **Reduced motion** makes every move instant.
//! * **Assistive technology.** Each grip is a button named "Reorder
//!   <label>" that knows its place ("3 of 5"), and a polite live region
//!   says where an item went ("Compressor moved to position 2 of 4").
//!
//! The list does not reorder itself: see the contract of `on_reorder` in
//! [`crate::reorder`]. The caller owns the items and gets
//! `on_reorder(from, to)` once, after the slide has settled. Give
//! [`Sortable::labels`] so the grips have names.
//!
//! The drag is [`crate::drag::PointerDrag`] and the maths [`crate::reorder`];
//! at most one state is written for a frame of an animation, and the pointer
//! position goes through a cell, so a drag never blocks events.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use accesskit::Live;
use freya::prelude::*;

use super::press::use_press;
use crate::component::ComponentName;
use crate::drag::{PointerDrag, Frames, is_touch, use_frames, use_pointer_drag};
use crate::group::{SlotFilter, card_rows};
use crate::icons::{self, Icon};
use crate::motion::Focusable;
use crate::reorder::{self, Done, Indicate, Move, Reorder, Stack};
use crate::shell::navigation::use_page_scroll;
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::Ink;

/// How much a held row grows, as a fraction of its size.
const LIFT_SCALE: f32 = 0.02;
/// How much of its opacity a held row gives up.
const LIFT_FADE: f32 = 0.06;
/// The layer a held row paints on: an overlay layer, so it is above every row
/// it passes whatever their depth in the tree (Freya adds a node's depth to its
/// layer, so a deep node of a neighbouring row would otherwise outrank a
/// shallow one of the held row).
const HELD_LAYER: Layer = crate::layers::DRAG_CARRY;
/// The layer of the drop line: above the held row, which it marks the place
/// under.
const LINE_LAYER: Layer = Layer::Relative(i16::MAX / 16 + 64);

/// A reorderable list of `items`. `on_reorder` gets the place an item was
/// dragged from and the place it now belongs at; see [`crate::reorder`] for
/// the contract.
#[derive(Clone, PartialEq)]
pub struct Sortable {
    items: Vec<Element>,
    labels: Vec<String>,
    on_reorder: EventHandler<(usize, usize)>,
    indicate: Indicate,
}

/// A reorderable list of `items`: rows that the person drags or moves with
/// the keyboard. `on_reorder(from, to)` is called once when an item has been
/// moved; the caller removes the item at `from` and inserts it at `to`.
pub fn sortable(
    items: Vec<Element>,
    on_reorder: impl Into<EventHandler<(usize, usize)>>,
) -> Sortable {
    Sortable {
        items,
        labels: Vec::new(),
        on_reorder: on_reorder.into(),
        indicate: Indicate::Slide,
    }
}

impl Sortable {
    /// The name of each item, for the accessible name of its grip
    /// ("Reorder Compressor") and the announcement of its move.
    pub fn labels(mut self, labels: Vec<String>) -> Self {
        self.labels = labels;
        self
    }

    /// For a long list: leave the other rows where they are and show a line
    /// in the gap where the held item would land.
    pub fn drop_indicator(mut self) -> Self {
        self.indicate = Indicate::Line;
        self
    }
}

/// What is drawn, one value a frame. Equal values are not written, so a list
/// at rest renders nothing.
#[derive(Clone, Default, PartialEq)]
struct View {
    /// The held or settling item.
    held: Option<usize>,
    /// How far each item is drawn from its place.
    offsets: Vec<f32>,
    /// 0 at rest, 1 held.
    lift: f32,
    /// Where the drop line goes, from the top of the list.
    line: Option<f32>,
}

/// Everything a drag works on that is not drawn directly. It lives for the
/// life of the list and is read by handlers, so none of it re-renders.
struct Shared {
    reorder: RefCell<Reorder>,
    /// The measured height of each row.
    sizes: RefCell<Vec<f32>>,
    /// How far into the held row the pointer was when it was picked up.
    grab: Cell<f32>,
    /// The page's scroll when the item was picked up, in pixels.
    scroll_start: Cell<i32>,
    /// The part of a pixel the page has not scrolled yet.
    carry: Cell<f32>,
    /// The grip that takes keyboard focus once a keyboard move is over.
    focus_after: Cell<Option<usize>>,
    reduced: Cell<bool>,
    /// Whether the item in hand was picked up by a finger resting on its row.
    /// That row was pressed by the touch that began it, and is built again
    /// once the drag is over so it does not keep a pressed look.
    remount: Cell<bool>,
    on_reorder: RefCell<EventHandler<(usize, usize)>>,
    labels: RefCell<Vec<String>>,
    ids: RefCell<Vec<AccessibilityId>>,
    count: Cell<usize>,
}

/// The handles every handler of one list needs. Clone it into each.
#[derive(Clone)]
struct Env {
    shared: Rc<Shared>,
    drag: PointerDrag,
    frames: Frames,
    view: State<View>,
    announce: State<String>,
    epoch: State<u64>,
    scroll: Option<ScrollController>,
}

impl Env {
    fn busy(&self) -> bool {
        self.shared.reorder.borrow().is_busy()
    }

    /// The page's scroll position in pixels, 0 at the top and below that
    /// while scrolled down.
    fn scroll_y(&self) -> i32 {
        self.scroll
            .map(|scroll| <(i32, i32)>::from(scroll).1)
            .unwrap_or(0)
    }

    /// Pick item `index` up. `within` is how far down the row the pointer is.
    /// Returns whether anything was picked up: a list that has not been
    /// measured yet, or one that is still sliding, takes nothing.
    fn pick(&self, index: usize, within: f32) -> bool {
        let sizes = self.shared.sizes.borrow().clone();
        if self.busy() || sizes.len() != self.shared.count.get() || index >= sizes.len() {
            return false;
        }
        self.shared.grab.set(within);
        self.shared.scroll_start.set(self.scroll_y());
        self.shared.carry.set(0.0);
        self.shared.focus_after.set(None);
        self.shared.reorder.borrow_mut().pick(Stack::new(sizes, 0.0), index);
        self.publish();
        self.start_frames();
        true
    }

    /// A press on the grip of row `index`.
    fn grip_down(&self, index: usize, event: &Event<PointerEventData>) {
        if !event.data().is_primary() || self.busy() || self.shared.count.get() < 2 {
            return;
        }
        let grip = Dim::RowButton.px();
        let row = self.shared.sizes.borrow().get(index).copied().unwrap_or(grip);
        let within = event.element_location().y as f32 + (row - grip) / 2.0;
        if !self.pick(index, within) {
            return;
        }
        self.drag.begin(event);
    }

    /// A finger has rested on row `index` long enough, at the window point
    /// `at`, `within` pixels down the row.
    fn long_press(&self, index: usize, at: (f32, f32), within: f32) {
        if self.pick(index, within) {
            self.shared.remount.set(true);
            self.drag.begin_at(at, true);
        }
    }

    /// Move item `index` by a key.
    fn key_move(&self, index: usize, step: Move) {
        let count = self.shared.count.get();
        let Some(to) = reorder::key_target(count, index, step) else {
            return;
        };
        let sizes = self.shared.sizes.borrow().clone();
        if self.busy() || sizes.len() != count {
            return;
        }
        self.shared.focus_after.set(Some(to));
        self.shared
            .reorder
            .borrow_mut()
            .nudge(Stack::new(sizes, 0.0), index, to);
        self.publish();
        self.start_frames();
    }

    /// The pointer went up: let the item settle.
    fn release(&self) {
        self.follow(0.0);
        self.shared.reorder.borrow_mut().drop();
        self.drag.end();
        self.publish();
        self.start_frames();
    }

    /// Escape: take the item home.
    fn cancel(&self) {
        self.shared.reorder.borrow_mut().cancel();
        self.drag.end();
        self.publish();
        self.start_frames();
    }

    /// Carry the held item to where the pointer is, and scroll the page if
    /// the pointer is at its edge. `dt` is the seconds since the last frame.
    fn follow(&self, dt: f32) {
        if !self.shared.reorder.borrow().is_held() {
            return;
        }
        let (_, pointer) = self.drag.pointer();
        if let (Some(mut scroll), true) = (self.scroll, dt > 0.0) {
            let window = Platform::get().current_window().root_size.read().height as f32;
            let speed = reorder::edge_speed(pointer, Dim::Header.px(), window);
            if speed != 0.0 {
                let want = self.shared.carry.get() + speed * dt;
                let whole = want.trunc();
                self.shared.carry.set(want - whole);
                if whole != 0.0 {
                    let now = self.scroll_y();
                    scroll.scroll_to_y((now - whole as i32).min(0));
                }
            }
        }
        // Distances are in the content: the page scrolling under the pointer
        // moves the held item with it.
        let scrolled = (self.scroll_y() - self.shared.scroll_start.get()) as f32;
        let delta = (pointer - self.drag.start().1) - scrolled;
        let indicate = self.shared.indicate();
        let mut model = self.shared.reorder.borrow_mut();
        match indicate {
            Indicate::Slide => {
                model.drag(delta);
            }
            Indicate::Line => {
                let top = model.top_of_held();
                model.drag_to_gap(delta, top + self.shared.grab.get() + delta);
            }
        }
    }

    /// One frame: follow the pointer, step the springs, draw, and finish what
    /// came to rest. Returns whether there is more to do.
    fn frame(&self, dt: f32) -> bool {
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
        self.busy()
    }

    fn finish(&self, done: Done) {
        if self.shared.remount.take() {
            let mut epoch = self.epoch;
            let next = *epoch.peek() + 1;
            epoch.set(next);
        }
        let focus = self.shared.focus_after.take();
        let Done::Moved { from, to } = done else {
            return;
        };
        let count = self.shared.count.get();
        let name = self
            .shared
            .labels
            .borrow()
            .get(from)
            .filter(|name| !name.is_empty())
            .cloned()
            .unwrap_or_else(|| "Item".to_string());
        let mut announce = self.announce;
        announce.set(format!("{name} moved to position {} of {count}", to + 1));
        let handler = self.shared.on_reorder.borrow().clone();
        handler.call((from, to));
        if let Some(index) = focus
            && let Some(id) = self.shared.ids.borrow().get(index)
        {
            id.request_focus();
        }
    }

    /// Write what is drawn, if it changed: the one state write of a frame.
    fn publish(&self) {
        let model = self.shared.reorder.borrow();
        let count = self.shared.count.get();
        let next = if model.is_busy() {
            View {
                held: model.held(),
                offsets: (0..count).map(|index| model.offset(index)).collect(),
                lift: model.lift(),
                line: model.line(),
            }
        } else {
            View::default()
        };
        drop(model);
        let mut view = self.view;
        view.set_if_modified(next);
    }

    /// Run the frame clock until everything is at rest. A list with reduced
    /// motion finishes in the same call.
    fn start_frames(&self) {
        if self.shared.reduced.get() && !self.shared.reorder.borrow().is_held() {
            while self.shared.reorder.borrow().is_busy() {
                self.frame(0.0);
            }
            return;
        }
        let env = self.clone();
        self.frames.run(move |dt| env.frame(dt));
    }
}

impl Shared {
    fn indicate(&self) -> Indicate {
        self.reorder.borrow().indicate()
    }
}

impl Component for Sortable {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let count = self.items.len();
        let drag = use_pointer_drag();
        let frames = use_frames();
        let view = use_state(View::default);
        let announce = use_state(String::new);
        let epoch = use_state(|| 0_u64);
        let lit = use_state(Vec::<usize>::new);
        let matched = use_state(Vec::<usize>::new);
        let press = use_press();
        let scroll = use_page_scroll();
        let indicate = self.indicate;
        let shared = use_hook(|| {
            Rc::new(Shared {
                reorder: RefCell::new(Reorder::new(indicate)),
                sizes: RefCell::new(Vec::new()),
                grab: Cell::new(0.0),
                scroll_start: Cell::new(0),
                carry: Cell::new(0.0),
                focus_after: Cell::new(None),
                reduced: Cell::new(false),
                remount: Cell::new(false),
                on_reorder: RefCell::new(self.on_reorder.clone()),
                labels: RefCell::new(Vec::new()),
                ids: RefCell::new(Vec::new()),
                count: Cell::new(0),
            })
        });
        // What the handlers read changes with each render: the caller's
        // callback and labels, and the count. A drag that is under way keeps
        // the list it started with.
        shared.reduced.set(tokens.motion(1) == 0);
        shared.count.set(count);
        *shared.on_reorder.borrow_mut() = self.on_reorder.clone();
        *shared.labels.borrow_mut() = self.labels.clone();
        shared.sizes.borrow_mut().resize(count, 0.0);
        if !shared.reorder.borrow().is_busy() {
            shared.reorder.borrow_mut().set_indicate(indicate);
        }
        let ids: Vec<AccessibilityId> = {
            let mut ids = shared.ids.borrow_mut();
            while ids.len() < count {
                ids.push(AccessibilityId::new_unique());
            }
            ids[..count].to_vec()
        };
        let env = Env {
            shared: shared.clone(),
            drag: drag.clone(),
            frames,
            view,
            announce,
            epoch,
            scroll,
        };

        let current = view.read().clone();
        let busy = current.held.is_some();

        let rows: Vec<Element> = self
            .items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let held = current.held == Some(index);
                let offset = current.offsets.get(index).copied().unwrap_or(0.0);
                let lift = if held { current.lift } else { 0.0 };
                let label = self.labels.get(index).cloned().unwrap_or_default();

                let grip = Grip {
                    id: ids[index],
                    label: label.clone(),
                    index,
                    count,
                    held: held && busy,
                    on_down: {
                        let env = env.clone();
                        (move |event: Event<PointerEventData>| env.grip_down(index, &event)).into()
                    },
                    on_step: {
                        let env = env.clone();
                        (move |step: Move| env.key_move(index, step)).into()
                    },
                };

                let lifted_fill = Paint::Popover.color(&tokens).with_af32(lift);
                let lifted = rect()
                    .tw(
                        &tokens,
                        &[w_full(), flex_row(), grow_children(), items(Align::Center), pr(ROW_INSET)],
                    )
                    .maybe(held, |lifted| {
                        lifted
                            .tw(&tokens, &[rounded(Radius::Card), fill(lifted_fill), lift_shadow(lift)])
                            .maybe(lift > 0.5, |lifted| lifted.tw(&tokens, &[border(Line::Hairline)]))
                            .layer(HELD_LAYER)
                            .scale(1.0 + LIFT_SCALE * lift)
                            .opacity(1.0 - LIFT_FADE * lift)
                    })
                    .child(
                        rect()
                            .key(*epoch.read())
                            .tw(&tokens, &[w_flex()])
                            .child(item.clone()),
                    )
                    .child(grip);

                let sizes = shared.clone();
                let long_env = env.clone();
                let key_env = env.clone();
                let down_press = press;
                let move_press = press;
                let up_press = press;
                rect()
                    .tw(&tokens, &[w_full(), flex_col()])
                    .offset_y(offset)
                    .on_sized(move |event: Event<SizedEventData>| {
                        if let Some(size) = sizes.sizes.borrow_mut().get_mut(index) {
                            *size = event.area.height();
                        }
                    })
                    .on_pointer_down(move |event: Event<PointerEventData>| {
                        if !is_touch(&event) || long_env.busy() || long_env.shared.count.get() < 2 {
                            return;
                        }
                        let within = event.element_location().y as f32;
                        let env = long_env.clone();
                        down_press.begin(
                            &event,
                            Some(EventHandler::new(move |at: (f32, f32)| env.long_press(index, at, within))),
                        );
                    })
                    .on_global_pointer_move(move |event: Event<PointerEventData>| move_press.moved(&event))
                    .on_global_pointer_up(move |_| up_press.release())
                    .on_key_down(move |event: Event<KeyboardEventData>| {
                        if !event.modifiers.alt() {
                            return;
                        }
                        let step = match &event.key {
                            Key::Named(NamedKey::ArrowUp) => Move::Up,
                            Key::Named(NamedKey::ArrowDown) => Move::Down,
                            _ => return,
                        };
                        event.stop_propagation();
                        key_env.key_move(index, step);
                    })
                    .child(lifted)
                    .into_element()
            })
            .collect();

        let filter = SlotFilter {
            matched,
            group_hit: false,
            searching: false,
        };
        let release = env.clone();
        let cancel = env.clone();
        let line = current.line.map(|top| {
            rect()
                .position(Position::new_absolute().top(top - 1.0).left(0.0))
                .layer(LINE_LAYER)
                .tw(&tokens, &[w_full(), h(Dim::Ring), px(GUTTER)])
                .child(
                    rect().tw(
                        &tokens,
                        &[w_full(), h_full(), rounded(Radius::Pill), bg(Paint::Accent)],
                    ),
                )
        });
        let message = announce.read().clone();
        let live = (!message.is_empty()).then(|| {
            rect()
                .position(Position::new_absolute().top(0.0).left(0.0))
                .tw(&tokens, &[w(Dim::Hairline), h(Dim::Hairline)])
                .a11y_role(AccessibilityRole::Status)
                .a11y_alt(message)
                .a11y_builder(|builder| builder.set_live(Live::Polite))
        });

        let card = rect()
            .component("Sortable")
            .tw(
                &tokens,
                &[
                    flex_col(),
                    w_full(),
                    rounded(Radius::Card),
                    bg(Paint::Card),
                    border(Line::Hairline),
                ],
            )
            // The card clips its rows to its rounded corners, as a group's
            // does, except while a row is held: that row grows past the card
            // and casts a shadow, which a clip would cut off.
            .maybe(!busy, |card| card.tw(&tokens, &[clip()]))
            .children(card_rows(rows, lit, filter))
            .maybe_child(line)
            .maybe_child(live);
        drag.track(
            card,
            move |_| release.release(),
            move || cancel.cancel(),
        )
    }
}

/// The grip at the end of a row: a button for dragging and, with the
/// keyboard, for moving the item.
#[derive(Clone, PartialEq)]
struct Grip {
    id: AccessibilityId,
    label: String,
    index: usize,
    count: usize,
    held: bool,
    on_down: EventHandler<Event<PointerEventData>>,
    on_step: EventHandler<Move>,
}

impl Component for Grip {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let focus = use_focus(self.id);
        let movable = self.count > 1;
        let on_down = self.on_down.clone();
        let on_step = self.on_step.clone();
        let name = if self.label.is_empty() {
            "Reorder".to_string()
        } else {
            format!("Reorder {}", self.label)
        };
        let (position, count) = (self.index + 1, self.count);
        let down = movable && (self.held || pressing());
        let glow = movable && (hovering() || down);
        rect()
            .component("SortableGrip")
            .a11y_id(self.id)
            .a11y_focusable(movable)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(name)
            .a11y_builder(move |builder| {
                builder.set_description("Press Up or Down to move, Home or End to send to an end");
                builder.set_position_in_set(position);
                builder.set_size_of_set(count);
            })
            .tw(
                &tokens,
                &[
                    center(),
                    w(Dim::RowButton),
                    h(Dim::RowButton),
                    rounded(Radius::Inner),
                    surface(Paint::Transparent, glow && !down, down, false),
                ],
            )
            .cursor(if self.held { CursorIcon::Grabbing } else { CursorIcon::Grab })
            .maybe(!movable, |grip| grip.opacity(0.4))
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| hovering.set(false))
            .on_pointer_down(move |event: Event<PointerEventData>| {
                // A mouse press picks the row up only once it moves: the grip looks pressed
                // from the press, as every button does.
                if movable {
                    pressing.set(true);
                }
                on_down.call(event)
            })
            .on_global_pointer_up(move |_| pressing.set_if_modified(false))
            .on_key_down(move |event: Event<KeyboardEventData>| {
                let step = match &event.key {
                    Key::Named(NamedKey::ArrowUp) => Move::Up,
                    Key::Named(NamedKey::ArrowDown) => Move::Down,
                    Key::Named(NamedKey::Home) => Move::First,
                    Key::Named(NamedKey::End) => Move::Last,
                    _ => return,
                };
                event.stop_propagation();
                on_step.call(step);
            })
            .child(icons::icon(
                Icon::DragHandle,
                IconSize::Control,
                if glow {
                    Paint::Text.color(&tokens)
                } else {
                    Ink::Muted.color(&tokens)
                },
            )).focusable(focus())
    }
}

#[cfg(all(test, feature = "testing"))]
#[path = "sortable_tests.rs"]
mod tests;
