//! Touch-aware presses for controls.
//!
//! Freya fires `on_press` for a touch when the finger lifts, with no check
//! that it stayed still. A finger that scrolls a list and lifts over a
//! button would therefore press that button. A `Press` remembers where a
//! touch began and cancels the tap once the finger strays past `TAP_SLOP`.
//! A touch that rests for `LONG_PRESS` runs the long-press action instead,
//! which is how a touch opens a row's menu in place of a right click.
//! Mouse, keyboard and assistive presses are never changed.
//!
//! A control calls `begin` on its pointer down, `moved` from
//! `on_global_pointer_move`, `release` from `on_global_pointer_up`, and
//! checks `tap` before acting in `on_press`.

use std::time::Duration;

use freya::prelude::*;

/// How long a touch must rest before it counts as a long press.
pub(crate) const LONG_PRESS: Duration = Duration::from_millis(500);
/// How far a touch may wander and still be a tap, in logical pixels.
const TAP_SLOP: f32 = 8.0;

/// The touch state of one control. Copy, so each handler takes its own.
#[derive(Clone, Copy)]
pub(crate) struct Press {
    /// Where the pending touch began. `None` once it is no longer a tap.
    origin: State<Option<(f32, f32)>>,
    timer: State<Option<TaskHandle>>,
}

/// The touch state for the calling component. Call it during render.
pub(crate) fn use_press() -> Press {
    Press {
        origin: use_state(|| None),
        timer: use_state(|| None),
    }
}

/// The logical point of a pointer event, in window coordinates.
pub(crate) fn point_of(event: &Event<PointerEventData>) -> (f32, f32) {
    let at = event.global_location();
    (at.x as f32, at.y as f32)
}

/// Whether a pointer event came from a touch rather than a mouse.
pub(crate) fn is_touch(event: &Event<PointerEventData>) -> bool {
    matches!(event.data(), PointerEventData::Touch(_))
}

impl Press {
    /// The pointer went down on the element. A touch starts a tap and, when
    /// `long` is set, the long-press timer, which calls `long` with the point
    /// where the touch began. A long press ends the tap, so the release does
    /// not also activate the control. A mouse press does nothing here.
    pub(crate) fn begin(
        self,
        event: &Event<PointerEventData>,
        long: Option<EventHandler<(f32, f32)>>,
    ) {
        if !is_touch(event) {
            return;
        }
        let point = point_of(event);
        self.cancel_timer();
        let mut origin = self.origin;
        origin.set(Some(point));
        let Some(long) = long else {
            return;
        };
        let task = spawn(async move {
            timer(LONG_PRESS).await;
            if origin.write().take().is_some() {
                long.call(point);
            }
        });
        let mut timer_state = self.timer;
        timer_state.set(Some(task));
    }

    /// The pointer moved anywhere. A touch that strays past the slop is a
    /// drag, so it is no longer a tap and no longer a long press.
    pub(crate) fn moved(self, event: &Event<PointerEventData>) {
        if !is_touch(event) {
            return;
        }
        let pending = *self.origin.read();
        let Some(origin) = pending else {
            return;
        };
        let (x, y) = point_of(event);
        if (x - origin.0).hypot(y - origin.1) > TAP_SLOP {
            let mut state = self.origin;
            state.set(None);
            self.cancel_timer();
        }
    }

    /// The pointer went up somewhere. This ends a pending long press. It does
    /// not clear the origin, because the press handler may still run after
    /// this and must see the tap.
    pub(crate) fn release(self) {
        self.cancel_timer();
    }

    /// Whether a touch that began on this element is still a tap: it has not
    /// strayed past the slop and has not lifted. A control that shows a
    /// pressed state clears it when this turns false.
    pub(crate) fn pending(self) -> bool {
        self.origin.read().is_some()
    }

    /// Whether a press on the element should act. Mouse, keyboard and
    /// assistive presses always do. A touch does only if it began on this
    /// element and stayed a tap.
    pub(crate) fn tap(self, event: &Event<PressEventData>) -> bool {
        match event.data() {
            PressEventData::Touch(_) => {
                let mut origin = self.origin;
                let tapped = origin.write().take().is_some();
                self.cancel_timer();
                tapped
            }
            _ => true,
        }
    }

    /// The pointer went up anywhere, for an element that heard the touch from
    /// its global down. Ends the timer and returns whether the touch stayed a
    /// tap on this element.
    pub(crate) fn lift(self) -> bool {
        let mut origin = self.origin;
        let tapped = origin.write().take().is_some();
        self.cancel_timer();
        tapped
    }

    fn cancel_timer(self) {
        let mut timer_state = self.timer;
        if let Some(handle) = timer_state.write().take() {
            handle.cancel();
        }
    }
}

/// How long a button is held, with the primary button or a finger, before its
/// context menu opens.
pub(crate) const HOLD: Duration = Duration::from_millis(450);

/// Whether a pointer event is the secondary mouse button.
pub(crate) fn is_right_click(event: &Event<PointerEventData>) -> bool {
    matches!(event.data(), PointerEventData::Mouse(mouse) if mouse.button == Some(MouseButton::Right))
}

/// A button held down: after [`HOLD`] it runs an action once, and the press
/// that ends the hold must not also act ([`Hold::fired`]). The hold is
/// cancelled by lifting, or by moving past the tap slop.
#[derive(Clone, Copy)]
pub(crate) struct Hold {
    origin: State<Option<(f32, f32)>>,
    timer: State<Option<TaskHandle>>,
    fired: State<bool>,
}

/// The hold state for the calling component. Call it during render.
pub(crate) fn use_hold() -> Hold {
    Hold {
        origin: use_state(|| None),
        timer: use_state(|| None),
        fired: use_state(|| false),
    }
}

impl Hold {
    /// The pointer went down on the element: start the timer. Only the
    /// primary button or a finger holds.
    pub(crate) fn begin(self, event: &Event<PointerEventData>, run: EventHandler<()>) {
        let mut fired = self.fired;
        fired.set(false);
        if !event.data().is_primary() {
            return;
        }
        self.cancel();
        let mut origin = self.origin;
        origin.set(Some(point_of(event)));
        let mut fired = self.fired;
        let task = spawn(async move {
            timer(HOLD).await;
            if origin.write().take().is_some() {
                fired.set(true);
                run.call(());
            }
        });
        let mut timer_state = self.timer;
        timer_state.set(Some(task));
    }

    /// The pointer moved anywhere: past the slop it is a drag, not a hold.
    pub(crate) fn moved(self, event: &Event<PointerEventData>) {
        let pending = *self.origin.read();
        let Some(origin) = pending else { return };
        let (x, y) = point_of(event);
        if (x - origin.0).hypot(y - origin.1) > TAP_SLOP {
            self.cancel();
        }
    }

    /// The pointer went up: a hold that has not fired will not.
    pub(crate) fn release(self) {
        self.cancel();
    }

    /// Whether the hold that is ending fired its action. The press that ends
    /// a hold asks, and must not act when this is true.
    pub(crate) fn fired(self) -> bool {
        *self.fired.peek()
    }

    fn cancel(self) {
        let mut origin = self.origin;
        if origin.peek().is_some() {
            origin.set(None);
        }
        let mut timer_state = self.timer;
        if let Some(handle) = timer_state.write().take() {
            handle.cancel();
        }
    }
}
