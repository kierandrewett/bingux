//! Hover tooltips for controls: a short label on the popup surface, shown
//! after the pointer rests on the control for 500 ms and kept inside the
//! window with an 8 px margin.
//!
//! `hover_tip` wraps any element. The bubble sits below, above, right or left
//! of the element, whichever first fits, and never over it. Where it sits is
//! one pure decision, [`decide`]: a bubble stays on the side it opened on for as
//! long as that side has room, so a window edge cannot make it flip back and
//! forth, and it does not move while the pointer rests.
//!
//! - **A new label never makes the bubble jump.** The bubble draws the text it
//!   was last measured with, at that size. The new text is laid out in a hidden
//!   copy first, and when its size is known the bubble swaps to the new text and
//!   the new place in one frame. No frame draws text at the size of other text.
//!   It stays centred on the control, on the edge that touches it.
//! - **A bubble is as wide as its text**, on one line, up to `Dim::TooltipMax`
//!   or the window less its margins; longer text wraps inside that onto at most
//!   four lines, and a bubble whose text wrapped is that wide (not as wide as
//!   its longest line). It is laid out in a box of that width of its own
//!   (`slot`), so the control it hangs from, or a clipped cell around it,
//!   cannot narrow it.
//! - **Tips share one clock per window.** While one tip is up, or for 300 ms
//!   after it went, the next control's tip opens at once, in full: no 500 ms
//!   wait. The tip it replaces stays as it was until the new bubble has a size
//!   and is drawn, and goes in that same frame, so moving along a row of
//!   controls is one bubble that changes place, never a gap and never two.
//! - **A menu, a choice list, a popover or a dialog takes tips down.** None opens while one
//!   is up, so a bubble never covers the menu a control opened, and never shows from under
//!   a popover that is a window of its own.
//! - **Scrolling takes tips down.** Content that scrolls moves under a resting
//!   pointer, so no tip is up while the window scrolls (`ScrollActivity`, which
//!   every scroll view pulses). A tip that is up goes in the frame the scroll
//!   begins, with no fade; one that is waiting, or whose pointer arrives
//!   meanwhile, opens nothing; when the content has been still for 200 ms a tip
//!   whose pointer is on its control rests the usual 500 ms and opens, with no
//!   grace time after a scroll. A tip reads the signal only while it is up or
//!   waiting.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use freya::prelude::*;

use super::press::{is_touch, use_press};
use crate::icons::{self, Icon};
use crate::motion::{PRESENCE_SCALE, use_presence};
use crate::shell::overlays::Overlays;
use crate::shell::panel::{PANEL_EDGE, PanelRect, Side};
use crate::shell::tooltip_place::decide;
use crate::style::*;
use crate::surface::use_tokens;
use crate::theme::Tokens;
use crate::typography::{CAPTION, Ink};

/// How long the pointer rests on the element before the tooltip shows.
const TOOLTIP_DELAY: Duration = Duration::from_millis(500);
/// How long after a tooltip hides the next one still opens at once.
const TOOLTIP_GRACE: Duration = Duration::from_millis(300);
/// How long the tooltip takes to fade and scale in or out.
const TOOLTIP_MOTION_MS: u64 = 120;
/// The most lines a tooltip wraps onto before its text is cut.
const TOOLTIP_MAX_LINES: usize = 4;

/// A state glyph with a hover tooltip. `text` is both the tooltip and the
/// glyph's accessible label, so the state is announced without a tooltip.
pub fn tooltip_icon(icon: Icon, size: IconSize, paint: Paint, text: impl Into<String>) -> Element {
    Element::from(TooltipIcon {
        icon,
        size,
        paint,
        text: text.into(),
    })
}

#[derive(Clone, PartialEq)]
struct TooltipIcon {
    icon: Icon,
    size: IconSize,
    paint: Paint,
    text: String,
}

impl Component for TooltipIcon {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let press = use_press();
        // Tooltips are hover help. A touch never shows one, so the press only
        // tracks the pointer for the release.
        let glyph = rect()
            .a11y_role(AccessibilityRole::Image)
            .a11y_alt(self.text.clone())
            .tw(&tokens, &[w(Dim::IconBox), h(Dim::IconBox), center()])
            .on_pointer_down(move |event: Event<PointerEventData>| press.begin(&event, None))
            .on_global_pointer_move(move |event: Event<PointerEventData>| press.moved(&event))
            .on_global_pointer_up(move |_| press.release())
            .child(icons::icon(self.icon, self.size, self.paint.color(&tokens)));
        hover_tip(glyph, self.text.clone())
    }
}

/// Wraps `child` so that resting the pointer on it shows `text` in a bubble.
pub fn hover_tip(child: impl IntoElement, text: impl Into<String>) -> Element {
    Element::from(HoverTip {
        child: child.into_element(),
        text: text.into(),
    })
}

/// What a window's tooltips share: which one is up and when the last one went,
/// so a second one can open at once beside it.
#[derive(Clone, Copy)]
struct TipHub {
    /// The tip that is up, if any.
    up: State<Option<u64>>,
    /// Counts the tips that opened. A tip that has hidden and sees the count
    /// move on knows another tip took its place.
    stage: State<u64>,
    /// When a tip last hid with no other tip taking its place.
    hid_at: State<Option<Instant>>,
    /// The stage of the latest tip that has drawn its bubble. Tips come up
    /// before their size is known; the one they replace stays until this
    /// reaches the stage they opened at.
    drawn: State<u64>,
    /// The window's scroll signal, when its host provides one: no tip is up
    /// while it is set.
    scroll: Option<ScrollActivity>,
}

impl TipHub {
    fn new() -> Self {
        Self {
            up: State::create(None),
            stage: State::create(0),
            hid_at: State::create(None),
            drawn: State::create(0),
            scroll: scroll_activity(),
        }
    }

    /// Whether a tip opens at once: one is up, or the last one went a moment ago.
    fn warm(self) -> bool {
        self.up.peek().is_some()
            || self
                .hid_at
                .peek()
                .is_some_and(|at| at.elapsed() < TOOLTIP_GRACE)
    }

    /// Whether the window is scrolling. Reading it subscribes the reader.
    fn scrolling(self) -> bool {
        self.scroll.is_some_and(|scroll| scroll.scrolling())
    }

    /// Whether the window is scrolling, without subscribing.
    fn scrolling_now(self) -> bool {
        self.scroll.is_some_and(|scroll| scroll.is_scrolling())
    }

    /// Makes tip `id` the one that is up. Returns the new stage.
    fn take_over(mut self, id: u64) -> u64 {
        let stage = *self.stage.peek() + 1;
        self.stage.set(stage);
        self.up.set(Some(id));
        stage
    }

    /// Notes that the tip opened at `stage` is drawn.
    fn drawn_by(mut self, stage: u64) {
        if stage > *self.drawn.peek() {
            self.drawn.set(stage);
        }
    }

    /// Lets go of tip `id` if it is the one that is up, and starts the grace
    /// time when `grace`. Safe in a drop: it does nothing for a state that is
    /// gone.
    fn release(self, id: u64, grace: bool) {
        let mut up = self.up;
        let mine = up.try_peek().is_some_and(|up| *up == Some(id));
        if !mine {
            return;
        }
        if let Some(mut up) = up.try_write() {
            *up = None;
        }
        let mut hid_at = self.hid_at;
        if let Some(mut at) = hid_at.try_write() {
            *at = grace.then(Instant::now);
        }
    }
}

/// Gives the window the state its tooltips share. The window host calls it
/// once; a tooltip with no host keeps a hub of its own.
pub(crate) fn provide_tooltips() {
    use_provide_context(TipHub::new);
}

fn use_hub() -> TipHub {
    use_hook(|| try_consume_context::<TipHub>().unwrap_or_else(TipHub::new))
}

fn next_tip_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// A bubble's text with the size it has when it is laid out.
#[derive(Clone, PartialEq)]
struct Measured {
    text: String,
    size: (f32, f32),
}

/// The state one tip needs from its event handlers and tasks.
#[derive(Clone, Copy)]
struct Tip {
    id: u64,
    hub: TipHub,
    overlays: Option<Overlays>,
    visible: State<bool>,
    /// This show opened beside another: it appears in full, with no fade.
    handoff: State<bool>,
    /// The hub's stage when this tip opened. Another stage means another tip
    /// has opened since.
    opened: State<u64>,
    measured: State<Option<Measured>>,
    pending: State<Option<TaskHandle>>,
    /// Content scrolled under the pointer: the tip is down, and comes back
    /// when the content is still, if the pointer is still on the element.
    suspended: State<bool>,
}

impl Tip {
    /// Whether a menu, a choice list or popover, or a dialog is up, which no tooltip opens
    /// over. A choice or a popover is a window of its own where the system allows it, and a
    /// bubble drawn in the control's window would show from under it.
    fn blocked(self) -> bool {
        self.overlays
            .is_some_and(|overlays| overlays.menu_open() || overlays.dialog_open())
            || crate::nav::try_router().is_some_and(|router| router.modal_open())
    }

    fn cancel_pending(mut self) {
        if let Some(handle) = self.pending.take() {
            handle.cancel();
        }
    }

    /// The pointer is over the element: wait for it to rest, or open at once
    /// when another tip is up or has just gone.
    fn rest(mut self) {
        if *self.visible.peek() {
            return;
        }
        self.cancel_pending();
        if self.blocked() {
            return;
        }
        if self.hub.scrolling_now() {
            // Content is moving under the pointer: wait for it to be still.
            self.suspended.set_if_modified(true);
            return;
        }
        if self.hub.warm() {
            self.open(true);
            return;
        }
        let task = spawn(async move {
            timer(TOOLTIP_DELAY).await;
            self.open(false);
        });
        self.pending.set(Some(task));
    }

    fn open(mut self, handoff: bool) {
        if self.blocked() {
            return;
        }
        if self.hub.scrolling_now() {
            self.suspended.set_if_modified(true);
            return;
        }
        self.handoff.set_if_modified(handoff);
        // The text it was last measured with is from an earlier show.
        self.measured.set_if_modified(None);
        let stage = self.hub.take_over(self.id);
        self.opened.set(stage);
        self.visible.set_if_modified(true);
    }

    /// The pointer left, or something opened over the element.
    fn close(mut self) {
        self.cancel_pending();
        self.suspended.set_if_modified(false);
        if *self.visible.peek() {
            self.visible.set(false);
        }
        self.hub.release(self.id, true);
    }

    /// Content started to scroll: the tip goes now, and comes back by
    /// [`Tip::resume`] when the content is still, if the pointer is still here.
    fn suspend(mut self) {
        self.cancel_pending();
        self.suspended.set_if_modified(true);
        self.visible.set_if_modified(false);
        // The next tip after a scroll waits its full rest: no grace.
        self.hub.release(self.id, false);
    }

    /// The content is still: wait for the pointer to rest again.
    fn resume(mut self) {
        self.suspended.set_if_modified(false);
        self.rest();
    }
}

#[derive(Clone, PartialEq)]
struct HoverTip {
    child: Element,
    text: String,
}

/// One bubble: the tooltip surface around `text`. Its size is the size of the
/// text and nothing else, so the same call lays out the same in the hidden copy
/// that measures it and in the visible one. `full` fills its slot: a bubble
/// whose text wrapped is as wide as the room it wrapped in, and not as wide as
/// its longest line, which leaves a gap of up to a word beside a shorter bubble.
fn bubble(tokens: &Tokens, text: &str, full: bool) -> Rect {
    rect()
        .tw(tokens, TOOLTIP)
        .tw(tokens, &[border(Line::Stroke(Dim::Hairline, Paint::FloatingOutline))])
        .maybe(full, |bubble| bubble.width(Size::fill()))
        .interactive(false)
        .child(
            CAPTION
                .in_ink(Ink::OnFloating)
                .label(tokens, text)
                .max_lines(TOOLTIP_MAX_LINES)
                .text_overflow(TextOverflow::Ellipsis),
        )
}

/// The width a bubble is laid out in: `Dim::TooltipMax`, or the window's width
/// less the edge margins when that is less.
fn room(window_width: f32) -> f32 {
    Dim::TooltipMax.px().min((window_width - 2.0 * PANEL_EDGE).max(0.0))
}

/// The box a bubble is laid out in. A text element wraps at the room its
/// ancestors give it, and an absolutely placed element is given the room of
/// the nearest ancestor with a size of its own: the 36 px button it describes,
/// or the clipped cell of a tab strip. This box has a size of its own, `room`
/// wide, so the bubble is as wide as its text on one line up to `room` and
/// never as wide as what it hangs from. It is placed (and raised) as the
/// bubble is; the bubble inside it is what is drawn.
fn slot(room: f32, bubble: Rect) -> Rect {
    rect().width(Size::px(room)).interactive(false).child(bubble)
}

impl Component for HoverTip {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let hub = use_hub();
        let id = use_hook(next_tip_id);
        let overlays = try_consume_context::<Overlays>();
        let visible = use_state(|| false);
        let handoff = use_state(|| false);
        let opened = use_state(|| 0_u64);
        let mut anchor = use_state::<Option<Area>>(|| None);
        let mut measured = use_state::<Option<Measured>>(|| None);
        let pending = use_state::<Option<TaskHandle>>(|| None);
        let suspended = use_state(|| false);
        // The side the bubble is on, kept between renders so the decision can
        // keep it. Read and written during render, but only ever to the value
        // the same inputs give again.
        let side = use_hook(|| Rc::new(Cell::new(None::<Side>)));
        let tip = Tip {
            id,
            hub,
            overlays,
            visible,
            handoff,
            opened,
            measured,
            pending,
            suspended,
        };
        // An unmounted control takes its tip down with it. This touches the
        // hub only: this component's own state may already be gone.
        use_drop(move || hub.release(id, true));

        // A menu or dialog that opens while the tip is up takes it down. It is
        // read only while the tip is up, so an idle tip does not render for it.
        let blocked = visible() && tip.blocked();
        use_side_effect_with_deps(&blocked, move |&blocked| {
            if blocked {
                tip.close();
            }
        });

        // Content that scrolls moves under a resting pointer, so no tip is up
        // while anything does. The scroll takes a tip that is up down at once,
        // and one that was waiting for the pointer to rest waits for the
        // content to be still. Read only while the tip is up or waiting, so an
        // idle tip does not render for it.
        let waiting = suspended();
        let scrolling = (visible() || waiting) && hub.scrolling();
        use_side_effect_with_deps(&(scrolling, waiting), move |&(scrolling, waiting)| {
            if scrolling {
                tip.suspend();
            } else if waiting {
                tip.resume();
            }
        });

        let wrapper = rect()
            .on_sized(move |e: Event<SizedEventData>| {
                anchor.set_if_modified(Some(e.area));
            })
            .on_pointer_over(move |event: Event<PointerEventData>| {
                // A touch has no hover, so it never shows the tooltip.
                if !is_touch(&event) {
                    tip.rest();
                }
            })
            .on_pointer_out(move |event: Event<PointerEventData>| {
                if !is_touch(&event) {
                    tip.close();
                }
            })
            .child(self.child.clone());

        // A tip that opens beside another (or just after one) shows in full at
        // once, so the pair read as one bubble that moves, not two that fade.
        let swapping = visible() && handoff();
        let motion = tokens.motion(if swapping { 0 } else { TOOLTIP_MOTION_MS });
        let mut progress = use_presence(visible(), motion, || {});
        if swapping {
            progress = 1.0;
        }
        // Scrolling took it down: it goes at once, with no fade, so no frame
        // draws a bubble beside content that moved.
        if !visible() && suspended() {
            side.set(None);
            return rect().child(wrapper).into_element();
        }
        if !visible() {
            if progress <= 0.0 {
                side.set(None);
                return rect().child(wrapper).into_element();
            }
            // Fading out. Another tip may have opened since this one hid.
            let stage = *hub.stage.read();
            let since = stage.saturating_sub(opened());
            if since > 0 {
                let replaced = *hub.drawn.read() >= stage;
                let waiting = since == 1 && !replaced && hub.up.read().is_some();
                if waiting {
                    // The next tip is up but has no size yet, so nothing of it
                    // is drawn: this one stays as it was until it is.
                    progress = 1.0;
                } else if replaced || since > 1 {
                    // Two bubbles are never drawn together.
                    side.set(None);
                    return rect().child(wrapper).into_element();
                }
            }
        }

        // The hidden copy lays out the text the tooltip is to show. The
        // bubble keeps the last text it measured until this one has a size.
        let window = crate::surface::window_size();
        let room = room(window.width);
        let text = self.text.clone();
        // A one line bubble is the box of one line of caption.
        let one_line = CAPTION.size.px() * CAPTION.leading + 2.0 * S1.px();
        let probe = slot(
            room,
            bubble(&tokens, &self.text, false)
                .opacity(0.0)
                // The copy is only for measuring: the bubble is what is announced.
                .a11y_builder(|node| node.set_hidden())
                .on_sized(move |e: Event<SizedEventData>| {
                    // A wrapped text fills the room it wrapped in.
                    let wrapped = e.area.height() > one_line * 1.5;
                    measured.set_if_modified(Some(Measured {
                        text: text.clone(),
                        size: (if wrapped { room } else { e.area.width() }, e.area.height()),
                    }));
                    // The bubble is drawn from the next frame, so a tip it replaced
                    // can go in that same frame.
                    if *visible.peek() {
                        hub.drawn_by(*opened.peek());
                    }
                }),
        )
        .position(Position::new_absolute().left(0.0).top(0.0));

        let shown = anchor().zip(measured.read().clone());
        let Some((area, shown)) = shown else {
            return rect().child(wrapper).child(probe).into_element();
        };
        let placed = decide(
            PanelRect::from_area(&area),
            shown.size,
            (window.width, window.height),
            side.get(),
        );
        side.set(Some(placed.side));

        let label = slot(
            room,
            bubble(&tokens, &shown.text, shown.size.0 >= room)
                .opacity(progress)
                .scale(PRESENCE_SCALE + (1.0 - PRESENCE_SCALE) * progress),
        )
        .layer(crate::layers::TOOLTIP)
        .position(
            Position::new_absolute()
                .left(placed.x - area.min_x())
                .top(placed.y - area.min_y()),
        );

        rect().child(wrapper).child(probe).child(label).into_element()
    }
}

#[cfg(all(test, feature = "testing"))]
#[path = "tooltip_tests.rs"]
mod tests;

#[cfg(all(test, feature = "testing"))]
#[path = "tooltip_layout_tests.rs"]
mod layout_tests;

#[cfg(feature = "testing")]
#[path = "tooltip_check.rs"]
pub mod check;
