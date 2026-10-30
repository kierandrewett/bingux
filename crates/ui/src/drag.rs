//! One pointer drag, for anything the pointer carries: a list item, a tab, a
//! handle on a canvas.
//!
//! A drag starts on a press ([`PointerDrag::begin`], or [`begin_at`] for a
//! long press that no event started), and from then on the pointer is
//! captured by the element that called [`PointerDrag::track`]: every move and
//! the release arrive there wherever the pointer is, and nothing under it
//! sees them, so it does not hover or press things on the way. The release
//! may be anywhere, including outside the window.
//!
//! Pointer events can arrive many times a frame, so the position goes into a
//! plain cell rather than a state: nothing re-renders on a move. A component
//! that animates what it drags runs [`Frames`] and reads
//! [`PointerDrag::pointer`] once a frame, and writes whatever state it draws
//! from once, so a drag costs at most one render a frame and never blocks the
//! event loop.
//!
//! [`begin_at`]: PointerDrag::begin_at

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use freya::prelude::*;

/// A point in window coordinates, in logical pixels.
pub type Point = (f32, f32);

/// The longest a frame is taken to last. A frame that took longer (a stalled
/// window, a debugger) advances an animation by this much and no more.
const LONGEST_FRAME: f32 = 0.05;
/// Frames are asked for this often. Freya has no per-frame callback for a
/// component, so this is a timer a little under a 60 Hz frame.
pub const FRAME: Duration = Duration::from_millis(16);

/// A drag in progress, or not. Clone it into each handler.
#[derive(Clone)]
pub struct PointerDrag {
    active: State<bool>,
    start: Rc<Cell<Point>>,
    latest: Rc<Cell<Point>>,
    touch: Rc<Cell<bool>>,
}

/// The drag state of the calling component. Call it during render.
pub fn use_pointer_drag() -> PointerDrag {
    let active = use_state(|| false);
    let (start, latest, touch) = use_hook(|| {
        (
            Rc::new(Cell::new((0.0, 0.0))),
            Rc::new(Cell::new((0.0, 0.0))),
            Rc::new(Cell::new(false)),
        )
    });
    PointerDrag {
        active,
        start,
        latest,
        touch,
    }
}

/// Whether the pointer is a finger.
pub fn is_touch(event: &Event<PointerEventData>) -> bool {
    matches!(event.data(), PointerEventData::Touch(_))
}

/// The window point of a pointer event.
pub fn point_of(event: &Event<PointerEventData>) -> Point {
    let at = event.global_location();
    (at.x as f32, at.y as f32)
}

impl PointerDrag {
    /// Whether a drag is on. Reactive: the caller renders again when it
    /// starts and ends, not on every move.
    pub fn is_active(&self) -> bool {
        *self.active.read()
    }

    /// Whether a drag is on, for a handler.
    pub fn is_active_now(&self) -> bool {
        *self.active.peek()
    }

    /// Whether the drag began with a finger.
    pub fn is_touch(&self) -> bool {
        self.touch.get()
    }

    /// Start a drag at the press `event`. Only the primary button or a finger
    /// starts one. The press goes no further, so what is under the handle does
    /// not also act on it. Returns whether a drag began.
    pub fn begin(&self, event: &Event<PointerEventData>) -> bool {
        if !event.data().is_primary() {
            return false;
        }
        event.stop_propagation();
        self.begin_at(point_of(event), is_touch(event));
        true
    }

    /// Start a drag with the pointer at `at`, for a drag no press event
    /// started: a long press on the item that fired its timer.
    pub fn begin_at(&self, at: Point, touch: bool) {
        self.start.set(at);
        self.latest.set(at);
        self.touch.set(touch);
        let mut active = self.active;
        active.set(true);
    }

    /// End the drag. The release or Escape handlers call this.
    pub fn end(&self) {
        let mut active = self.active;
        active.set_if_modified(false);
    }

    /// Where the pointer was when the drag began.
    pub fn start(&self) -> Point {
        self.start.get()
    }

    /// Where the pointer is now.
    pub fn pointer(&self) -> Point {
        self.latest.get()
    }

    /// How far the pointer has moved since the drag began.
    pub fn travelled(&self) -> Point {
        let (x, y) = self.latest.get();
        let (sx, sy) = self.start.get();
        (x - sx, y - sy)
    }

    /// Capture the pointer for the drag on `element`: while a drag is on,
    /// every move and the release arrive here first and are taken (nothing
    /// under the pointer hovers, and nothing is pressed by the release), the
    /// pointer's place is recorded, and Escape calls `on_cancel`. `on_release`
    /// gets the point where the pointer went up. Neither handler ends the drag
    /// unless it calls [`PointerDrag::end`].
    pub fn track<E: EventHandlersExt>(
        &self,
        element: E,
        on_release: impl Fn(Point) + 'static,
        on_cancel: impl Fn() + 'static,
    ) -> E {
        let moving = self.clone();
        let releasing = self.clone();
        let keying = self.clone();
        element
            .on_capture_global_pointer_move(move |event: Event<PointerEventData>| {
                if !moving.is_active_now() {
                    return;
                }
                event.prevent_default();
                moving.latest.set(point_of(&event));
            })
            // The capture phase, and the release is taken: what the pointer
            // was pressed on is not pressed by it. A finger that picked an
            // item up with a long press began on a row, and Freya would
            // otherwise report that row as pressed when the finger lifts.
            .on_capture_global_pointer_up(move |event: Event<PointerEventData>| {
                if !releasing.is_active_now() {
                    return;
                }
                event.prevent_default();
                let at = point_of(&event);
                releasing.latest.set(at);
                on_release(at);
            })
            .on_global_key_down(move |event: Event<KeyboardEventData>| {
                if keying.is_active_now() && event.key == Key::Named(NamedKey::Escape) {
                    event.stop_propagation();
                    on_cancel();
                }
            })
    }
}

/// A frame clock for what a drag animates. Clone it into each handler.
#[derive(Clone)]
pub struct Frames {
    running: Rc<Cell<bool>>,
}

/// The frame clock of the calling component. Call it during render.
pub fn use_frames() -> Frames {
    let running = use_hook(|| Rc::new(Cell::new(false)));
    Frames { running }
}

impl Frames {
    /// Call `tick` with the seconds the last frame took, about every
    /// [`FRAME`], until it returns false. Does nothing when the clock is
    /// already running: the running `tick` stays, so call it with the same
    /// work each time. The loop stops with the component.
    pub fn run(&self, mut tick: impl FnMut(f32) -> bool + 'static) {
        if self.running.replace(true) {
            return;
        }
        let running = self.running.clone();
        spawn(async move {
            let mut last = Instant::now();
            loop {
                timer(FRAME).await;
                let now = Instant::now();
                let seconds = now.duration_since(last).as_secs_f32().min(LONGEST_FRAME);
                last = now;
                if !tick(seconds) {
                    break;
                }
            }
            running.set(false);
        });
    }

    /// Whether the clock is running.
    pub fn is_running(&self) -> bool {
        self.running.get()
    }
}
