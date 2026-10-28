//! Pointer capture: how a control turns a drag into a value.
//!
//! The slider and the colour picker's square and strips all do the same thing:
//! a press on a small element starts a drag, the drag follows the pointer
//! wherever it goes until the button comes up, and each new position becomes a
//! value. This module does that once, so every such control has the same
//! behaviour and the same fixes. (`crate::drag` is for carrying an item with
//! the pointer; this is for reading a position off it.)
//!
//! What a [`Capture`] guarantees:
//!
//! - **The drag is owned by the press.** From the press until the release the
//!   moves go to this control whatever is under the pointer: outside the small
//!   element, outside a popup's edge, over a scrim, over another control. Those
//!   moves are consumed (`prevent_default` on the capture move), so nothing
//!   under the pointer shows hover, runs a tooltip or starts another gesture
//!   in the meantime, and the release does not click what it ends over.
//! - **It ends exactly once.** A release of the button that started it ends
//!   it. So does a lost pointer: a touch is only followed by that finger, a
//!   cancelled touch ends it, and a mouse move while a finger drag is on means
//!   the finger is gone. Nothing leaves a control holding the pointer after
//!   the button is up (the next hover must never drag).
//! - **At most one report per frame.** A pointer can send hundreds of moves a
//!   second. The press, the release and the first move of a burst are reported
//!   at once; the moves after it wait for the next frame and only the newest
//!   goes through. A control sets its state once per frame, not once per move,
//!   and does no work in the event handler beyond that.
//! - **Positions are the element's own.** A report carries the pointer in the
//!   element's coordinates, in logical pixels, as the layout has the element
//!   *now*: a scroll view that scrolls, a sheet that slides or a window that
//!   resizes mid-drag moves the origin with it. Positions are not clamped; a
//!   control clamps with [`fraction`], so a pointer past an end is the end.
//!
//! A control calls [`use_capture`] in its component, puts [`Capture::bind`] on
//! the element that starts the drag, and reads the [`CaptureEvent`]s in its
//! handler.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use freya::prelude::*;

/// A point in logical pixels.
pub(crate) type Point = (f32, f32);

/// Where a drag's pointer comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Source {
    Mouse,
    /// One finger, by its id. Another finger is another pointer.
    Touch(u64),
}

/// What one pointer event means to the drag in progress.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Step {
    /// Nothing: no drag is on, or the event is from another pointer.
    Ignore,
    /// The drag is at this window point.
    Follow(Point),
    /// The button came up here: the drag is over.
    End(Point),
    /// The pointer was lost: the drag is over and has no final point.
    Cancel,
}

/// The drag state machine: whether a drag is held and by which pointer. It
/// knows nothing of the toolkit, so a test drives a drag by calling it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Gesture {
    held: Option<Source>,
}

impl Gesture {
    /// Whether a drag is on.
    pub(crate) fn is_held(&self) -> bool {
        self.held.is_some()
    }

    /// A pointer went down on the element. A press while a drag is on starts
    /// the drag over, with the new pointer.
    pub(crate) fn press(&mut self, source: Source, at: Point) -> Step {
        self.held = Some(source);
        Step::Follow(at)
    }

    /// A pointer moved anywhere.
    pub(crate) fn moved(&mut self, source: Source, at: Point) -> Step {
        match self.held {
            Some(held) if held == source => Step::Follow(at),
            // A finger drag, and now the mouse moves: the finger's release or
            // cancel never reached us. The drag is over.
            Some(Source::Touch(_)) if source == Source::Mouse => {
                self.held = None;
                Step::Cancel
            }
            _ => Step::Ignore,
        }
    }

    /// A pointer's button or finger came up anywhere.
    pub(crate) fn released(&mut self, source: Source, at: Point) -> Step {
        match self.held {
            Some(held) if held == source => {
                self.held = None;
                Step::End(at)
            }
            _ => Step::Ignore,
        }
    }

    /// A pointer was cancelled by the system. Only a drag of that pointer
    /// ends.
    pub(crate) fn cancelled(&mut self, source: Source) -> Step {
        match self.held {
            Some(held) if held == source => {
                self.held = None;
                Step::Cancel
            }
            _ => Step::Ignore,
        }
    }
}

/// How far, from 0 to 1, `pos` is along a track whose first stop is at
/// `start` and whose last is `travel` further on. A position past either end
/// is that end; a track with no length is 0; NaN is 0.
pub(crate) fn fraction(pos: f32, start: f32, travel: f32) -> f32 {
    if travel.is_nan() || travel <= 0.0 || pos.is_nan() {
        return 0.0;
    }
    ((pos - start) / travel).clamp(0.0, 1.0)
}

/// How a report arrived.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    /// The press. The position is where the pointer went down.
    Start,
    /// The pointer moved, at most once per frame.
    Move,
    /// The button came up. The position is where; apply it like a move.
    End,
    /// The pointer was lost. The position is the last one reported.
    Cancel,
}

/// One report of a drag.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CaptureEvent {
    pub phase: Phase,
    /// The pointer in the element's own coordinates: logical pixels from its
    /// top-left corner. Negative or past the size when the pointer is outside.
    pub local: Point,
    /// The pointer in the window, in logical pixels.
    pub at: Point,
}

/// The state a drag keeps between events. Plain cells, not signals: nothing
/// here may re-render a component by being written.
struct Core {
    gesture: Cell<Gesture>,
    /// The element's top-left corner in the window, in logical pixels.
    origin: Cell<Point>,
    /// The newest move that has not been reported yet.
    pending: Cell<Option<Point>>,
    /// Whether a report went out this frame, so the next move waits.
    waiting: Cell<bool>,
    /// The last point reported, for a `Cancel`.
    last: Cell<Point>,
    /// The newest handler: the one from the component's latest render.
    handler: RefCell<Option<EventHandler<CaptureEvent>>>,
}

impl Core {
    fn report(&self, phase: Phase, at: Point) {
        self.last.set(at);
        let handler = self.handler.borrow().clone();
        if let Some(handler) = handler {
            let (x, y) = self.origin.get();
            handler.call(CaptureEvent {
                phase,
                local: (at.0 - x, at.1 - y),
                at,
            });
        }
    }

    /// A move: reported now when none went out this frame, else held for the
    /// next frame, replacing any move already held.
    fn follow(self: &Rc<Self>, at: Point) {
        if self.waiting.get() {
            self.pending.set(Some(at));
            return;
        }
        self.report(Phase::Move, at);
        self.waiting.set(true);
        let core = self.clone();
        let ticker = RenderingTicker::get();
        let window = Platform::get().current_window();
        spawn(async move {
            loop {
                // A frame must come even if the report changed nothing.
                window.request_redraw();
                ticker.tick().await;
                match core.pending.take() {
                    Some(at) if core.gesture.get().is_held() => core.report(Phase::Move, at),
                    _ => break,
                }
            }
            core.waiting.set(false);
        });
    }
}

/// The capture of one control. Cheap to clone; every clone is the same one.
#[derive(Clone)]
pub(crate) struct Capture {
    core: Rc<Core>,
}

/// The capture of the calling component. `on_capture` hears every report.
/// Call it during render, in the same place every render; the newest
/// `on_capture` is the one that runs, so it may capture this render's values.
pub(crate) fn use_capture(on_capture: impl Into<EventHandler<CaptureEvent>>) -> Capture {
    let core = use_hook(|| {
        Rc::new(Core {
            gesture: Cell::new(Gesture::default()),
            origin: Cell::new((0.0, 0.0)),
            pending: Cell::new(None),
            waiting: Cell::new(false),
            last: Cell::new((0.0, 0.0)),
            handler: RefCell::new(None),
        })
    });
    *core.handler.borrow_mut() = Some(on_capture.into());
    Capture { core }
}

/// The source of a pointer event.
fn source_of(event: &Event<PointerEventData>) -> Source {
    match event.data() {
        PointerEventData::Mouse(_) => Source::Mouse,
        PointerEventData::Touch(touch) => Source::Touch(touch.finger_id),
    }
}

/// The window point of a pointer event.
fn point_of(event: &Event<PointerEventData>) -> Point {
    let at = event.global_location();
    (at.x as f32, at.y as f32)
}

impl Capture {
    /// Puts the capture on `element`: the press that starts the drag, and the
    /// moves and the release that follow it wherever they happen. The element
    /// also tracks where it is, so a layout that moves it mid-drag is followed.
    pub(crate) fn bind(&self, element: Rect) -> Rect {
        let tracker = self.clone();
        self.handlers(element)
            .on_sized(move |event: Event<SizedEventData>| tracker.sized(&event))
    }

    /// [`bind`](Self::bind) without tracking the element's place, for an
    /// element that already has an `on_sized` handler. That handler must pass
    /// its event to [`sized`](Self::sized).
    pub(crate) fn handlers(&self, element: Rect) -> Rect {
        let down = self.clone();
        let moving = self.clone();
        let up = self.clone();
        let cancel = self.clone();
        element
            .on_pointer_down(move |event: Event<PointerEventData>| down.pressed(&event))
            .on_capture_global_pointer_move(move |event: Event<PointerEventData>| {
                moving.moved(&event)
            })
            .on_capture_global_pointer_up(move |event: Event<PointerEventData>| {
                up.released(&event)
            })
            .on_touch_cancel(move |event: Event<TouchEventData>| {
                cancel.cancelled(Source::Touch(event.finger_id))
            })
    }

    /// The element's layout changed: its place in the window is the new one.
    pub(crate) fn sized(&self, event: &Event<SizedEventData>) {
        let area = event.visible_area;
        self.core.origin.set((area.min_x(), area.min_y()));
    }

    fn pressed(&self, event: &Event<PointerEventData>) {
        if !event.data().is_primary() {
            return;
        }
        event.stop_propagation();
        let at = point_of(event);
        let local = event.element_location();
        // Exact at this moment, whatever moved the element since it was laid out.
        self.core
            .origin
            .set((at.0 - local.x as f32, at.1 - local.y as f32));
        let mut gesture = self.core.gesture.get();
        gesture.press(source_of(event), at);
        self.core.gesture.set(gesture);
        self.core.pending.set(None);
        self.core.report(Phase::Start, at);
    }

    fn moved(&self, event: &Event<PointerEventData>) {
        let mut gesture = self.core.gesture.get();
        let step = gesture.moved(source_of(event), point_of(event));
        self.core.gesture.set(gesture);
        match step {
            Step::Follow(at) => {
                // The drag has the pointer: nothing under it hovers.
                event.prevent_default();
                self.core.follow(at);
            }
            Step::Cancel => self.lost(),
            Step::Ignore | Step::End(_) => {}
        }
    }

    fn released(&self, event: &Event<PointerEventData>) {
        let cancelled = matches!(
            event.data(),
            PointerEventData::Touch(touch) if touch.phase == TouchPhase::Cancelled
        );
        let source = source_of(event);
        let mut gesture = self.core.gesture.get();
        let step = if cancelled {
            gesture.cancelled(source)
        } else if event.data().is_primary() {
            gesture.released(source, point_of(event))
        } else {
            Step::Ignore
        };
        self.core.gesture.set(gesture);
        match step {
            Step::End(at) => {
                // The button came up on this drag: no one else's release.
                event.prevent_default();
                self.core.pending.set(None);
                self.core.report(Phase::End, at);
            }
            Step::Cancel => self.lost(),
            Step::Follow(_) | Step::Ignore => {}
        }
    }

    fn cancelled(&self, source: Source) {
        let mut gesture = self.core.gesture.get();
        let step = gesture.cancelled(source);
        self.core.gesture.set(gesture);
        if step == Step::Cancel {
            self.lost();
        }
    }

    /// The drag ended without a release.
    fn lost(&self) {
        self.core.pending.set(None);
        self.core.report(Phase::Cancel, self.core.last.get());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AT: Point = (10.0, 20.0);

    #[test]
    fn a_press_starts_a_drag_and_the_release_ends_it_once() {
        let mut drag = Gesture::default();
        assert!(!drag.is_held());
        assert_eq!(drag.press(Source::Mouse, AT), Step::Follow(AT));
        assert!(drag.is_held());
        assert_eq!(
            drag.moved(Source::Mouse, (30.0, 5.0)),
            Step::Follow((30.0, 5.0))
        );
        assert_eq!(
            drag.released(Source::Mouse, (31.0, 6.0)),
            Step::End((31.0, 6.0))
        );
        assert!(!drag.is_held());
        // Nothing follows the release: hovering must not drag.
        assert_eq!(drag.moved(Source::Mouse, (40.0, 5.0)), Step::Ignore);
        assert_eq!(drag.released(Source::Mouse, (40.0, 5.0)), Step::Ignore);
    }

    #[test]
    fn moves_with_no_press_are_ignored() {
        let mut drag = Gesture::default();
        assert_eq!(drag.moved(Source::Mouse, AT), Step::Ignore);
        assert_eq!(drag.moved(Source::Touch(1), AT), Step::Ignore);
        assert_eq!(drag.released(Source::Mouse, AT), Step::Ignore);
        assert_eq!(drag.cancelled(Source::Touch(1)), Step::Ignore);
    }

    #[test]
    fn a_drag_continues_outside_the_element_until_the_release() {
        // Positions are never refused for being far from the element.
        let mut drag = Gesture::default();
        drag.press(Source::Mouse, AT);
        for at in [(-500.0, -500.0), (5000.0, 20.0), (10.0, 9000.0)] {
            assert_eq!(drag.moved(Source::Mouse, at), Step::Follow(at));
        }
        assert_eq!(
            drag.released(Source::Mouse, (9999.0, 9999.0)),
            Step::End((9999.0, 9999.0))
        );
    }

    #[test]
    fn a_touch_is_followed_by_its_own_finger_only() {
        let mut drag = Gesture::default();
        drag.press(Source::Touch(7), AT);
        assert_eq!(drag.moved(Source::Touch(8), (1.0, 1.0)), Step::Ignore);
        assert_eq!(drag.released(Source::Touch(8), (1.0, 1.0)), Step::Ignore);
        assert!(drag.is_held());
        assert_eq!(
            drag.moved(Source::Touch(7), (2.0, 2.0)),
            Step::Follow((2.0, 2.0))
        );
        assert_eq!(
            drag.released(Source::Touch(7), (3.0, 3.0)),
            Step::End((3.0, 3.0))
        );
    }

    #[test]
    fn a_mouse_move_during_a_touch_drag_means_the_finger_is_gone() {
        let mut drag = Gesture::default();
        drag.press(Source::Touch(7), AT);
        assert_eq!(drag.moved(Source::Mouse, (1.0, 1.0)), Step::Cancel);
        assert!(!drag.is_held());
        // The stale touch can no longer be followed or released.
        assert_eq!(drag.moved(Source::Touch(7), (2.0, 2.0)), Step::Ignore);
    }

    #[test]
    fn a_touch_during_a_mouse_drag_is_ignored() {
        let mut drag = Gesture::default();
        drag.press(Source::Mouse, AT);
        assert_eq!(drag.moved(Source::Touch(1), (1.0, 1.0)), Step::Ignore);
        assert_eq!(drag.released(Source::Touch(1), (1.0, 1.0)), Step::Ignore);
        assert!(drag.is_held());
    }

    #[test]
    fn a_cancelled_touch_ends_its_drag_without_a_release() {
        let mut drag = Gesture::default();
        drag.press(Source::Touch(3), AT);
        assert_eq!(drag.cancelled(Source::Touch(4)), Step::Ignore);
        assert!(drag.is_held());
        assert_eq!(drag.cancelled(Source::Touch(3)), Step::Cancel);
        assert!(!drag.is_held());
        assert_eq!(drag.released(Source::Touch(3), AT), Step::Ignore);
    }

    #[test]
    fn a_new_press_starts_the_drag_over() {
        let mut drag = Gesture::default();
        drag.press(Source::Touch(1), AT);
        drag.press(Source::Mouse, (5.0, 5.0));
        assert_eq!(drag.moved(Source::Touch(1), AT), Step::Ignore);
        assert_eq!(
            drag.moved(Source::Mouse, (6.0, 6.0)),
            Step::Follow((6.0, 6.0))
        );
    }

    #[test]
    fn fraction_clamps_to_the_ends() {
        // A track from x = 6 for 188 px, like a 200 px strip with a 12 px thumb.
        assert_eq!(fraction(6.0, 6.0, 188.0), 0.0);
        assert_eq!(fraction(100.0, 6.0, 188.0), 94.0 / 188.0);
        assert_eq!(fraction(194.0, 6.0, 188.0), 1.0);
        assert_eq!(fraction(-1000.0, 6.0, 188.0), 0.0);
        assert_eq!(fraction(1000.0, 6.0, 188.0), 1.0);
        assert_eq!(fraction(f32::INFINITY, 6.0, 188.0), 1.0);
        assert_eq!(fraction(f32::NEG_INFINITY, 6.0, 188.0), 0.0);
    }

    #[test]
    fn fraction_of_a_track_with_no_length_is_zero() {
        assert_eq!(fraction(50.0, 6.0, 0.0), 0.0);
        assert_eq!(fraction(50.0, 6.0, -3.0), 0.0);
        assert_eq!(fraction(f32::NAN, 6.0, 188.0), 0.0);
        assert_eq!(fraction(50.0, 6.0, f32::NAN), 0.0);
    }
}
