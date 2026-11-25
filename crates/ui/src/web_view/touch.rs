//! A finger on an engine that has no touch input yet.
//!
//! ABI 1.1 declares `vv_session_touch` and answers `VV_ERR_UNSUPPORTED`. Until
//! it works, a touch screen still has to browse: a tap is a click, a drag is a
//! scroll and a held finger is a right click. This is that translation, as a
//! state machine of its own so it can be tested without a window. When the
//! backend reports `Capabilities::touch` the browser forwards the fingers
//! as they are and this is not used.

use std::time::{Duration, Instant};

use crate::frame_view::WheelPhase;

use crate::web_view::input::{
    Mods, PageInput, PointerEvent, PointerKind, TouchEvent, TouchKind, WheelEvent, WheelUnit,
};

/// How far a finger may move and still be a tap, in logical pixels.
pub const TAP_SLOP: f64 = 10.0;

/// How long a finger rests, still, before it is a right click.
pub const LONG_PRESS: Duration = Duration::from_millis(500);

#[derive(Clone, Copy, Debug)]
struct Finger {
    id: u64,
    start: (f64, f64),
    last: (f64, f64),
    since: Instant,
    scrolling: bool,
}

/// Turns touches into what a mouse would do.
#[derive(Debug, Default)]
pub struct TouchScroller {
    finger: Option<Finger>,
    /// A second finger came down: nothing the first one does counts.
    spoiled: bool,
}

fn pointer(kind: PointerKind, at: (f64, f64), button: u32, buttons: u32) -> PageInput {
    PageInput::Pointer(PointerEvent {
        kind,
        x: at.0,
        y: at.1,
        button,
        buttons,
        modifiers: Mods::default(),
        click_count: u32::from(button == 0 && kind != PointerKind::Move),
    })
}

impl TouchScroller {
    pub fn new() -> Self {
        Self::default()
    }

    /// The point a held finger would open a menu at, once [`LONG_PRESS`] has
    /// passed: the host calls [`TouchScroller::held`] from a timer it starts
    /// on `Down`.
    pub fn held(&mut self, now: Instant) -> Option<PageInput> {
        let finger = self.finger.as_mut()?;
        if self.spoiled || finger.scrolling || now.duration_since(finger.since) < LONG_PRESS {
            return None;
        }
        // The press is spent: lifting the finger now is not a tap.
        finger.scrolling = true;
        Some(PageInput::ContextMenu {
            at: Some(finger.start),
            modifiers: Mods::default(),
        })
    }

    /// One touch. Returns what the page should receive for it.
    pub fn touch(&mut self, event: TouchEvent, now: Instant) -> Vec<PageInput> {
        let at = (event.x, event.y);
        match event.kind {
            TouchKind::Down => {
                if self.finger.is_some() {
                    self.spoiled = true;
                    return Vec::new();
                }
                self.spoiled = false;
                self.finger = Some(Finger {
                    id: event.id,
                    start: at,
                    last: at,
                    since: now,
                    scrolling: false,
                });
                // The page sees the pointer arrive, so hover styles and
                // scrollable regions know where the finger is.
                vec![pointer(PointerKind::Move, at, 0, 0)]
            }
            TouchKind::Move => {
                let spoiled = self.spoiled;
                let Some(finger) = self.finger.as_mut().filter(|finger| finger.id == event.id) else {
                    return Vec::new();
                };
                let travelled = ((at.0 - finger.start.0).powi(2) + (at.1 - finger.start.1).powi(2)).sqrt();
                if spoiled || (!finger.scrolling && travelled < TAP_SLOP) {
                    return Vec::new();
                }
                finger.scrolling = true;
                // The content follows the finger: a finger moving up scrolls
                // the page down.
                let (dx, dy) = (finger.last.0 - at.0, finger.last.1 - at.1);
                finger.last = at;
                if dx == 0.0 && dy == 0.0 {
                    return Vec::new();
                }
                vec![PageInput::Wheel(WheelEvent {
                    x: at.0,
                    y: at.1,
                    dx,
                    dy,
                    unit: WheelUnit::Pixel,
                    precise: true,
                    modifiers: Mods::default(),
                    phase: WheelPhase::None,
                })]
            }
            TouchKind::Up => {
                let Some(finger) = self.finger.filter(|finger| finger.id == event.id) else {
                    return Vec::new();
                };
                self.finger = None;
                let spoiled = std::mem::take(&mut self.spoiled);
                if spoiled || finger.scrolling {
                    return Vec::new();
                }
                vec![
                    pointer(PointerKind::Down, finger.start, 0, 1),
                    pointer(PointerKind::Up, finger.start, 0, 0),
                ]
            }
            TouchKind::Cancel => {
                if self.finger.is_some_and(|finger| finger.id == event.id) {
                    self.finger = None;
                    self.spoiled = false;
                }
                Vec::new()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(kind: TouchKind, id: u64, x: f64, y: f64) -> TouchEvent {
        TouchEvent { kind, id, x, y }
    }

    #[test]
    fn a_short_touch_is_a_click_where_the_finger_went_down() {
        let now = Instant::now();
        let mut scroller = TouchScroller::new();
        scroller.touch(touch(TouchKind::Down, 1, 50.0, 60.0), now);
        assert!(scroller.touch(touch(TouchKind::Move, 1, 53.0, 62.0), now).is_empty(), "within the slop");
        let out = scroller.touch(touch(TouchKind::Up, 1, 53.0, 62.0), now);
        let kinds: Vec<_> = out
            .iter()
            .map(|input| match input {
                PageInput::Pointer(event) => (event.kind, event.x, event.y, event.click_count),
                other => panic!("unexpected {other:?}"),
            })
            .collect();
        assert_eq!(
            kinds,
            [(PointerKind::Down, 50.0, 60.0, 1), (PointerKind::Up, 50.0, 60.0, 1)]
        );
    }

    #[test]
    fn a_drag_scrolls_the_way_the_finger_goes_and_does_not_click() {
        let now = Instant::now();
        let mut scroller = TouchScroller::new();
        scroller.touch(touch(TouchKind::Down, 1, 100.0, 300.0), now);
        let out = scroller.touch(touch(TouchKind::Move, 1, 100.0, 270.0), now);
        let [PageInput::Wheel(wheel)] = out.as_slice() else {
            panic!("a wheel event, got {out:?}");
        };
        assert_eq!((wheel.dx, wheel.dy), (0.0, 30.0), "a finger moving up scrolls the page down");
        let again = scroller.touch(touch(TouchKind::Move, 1, 100.0, 260.0), now);
        let [PageInput::Wheel(wheel)] = again.as_slice() else {
            panic!("a wheel event");
        };
        assert_eq!(wheel.dy, 10.0, "each move is the distance since the last");
        assert!(scroller.touch(touch(TouchKind::Up, 1, 100.0, 260.0), now).is_empty());
    }

    #[test]
    fn a_held_finger_opens_the_page_menu_once_and_does_not_click() {
        let start = Instant::now();
        let mut scroller = TouchScroller::new();
        scroller.touch(touch(TouchKind::Down, 1, 20.0, 30.0), start);
        assert_eq!(scroller.held(start + Duration::from_millis(100)), None);
        let held = scroller.held(start + LONG_PRESS);
        assert_eq!(
            held,
            Some(PageInput::ContextMenu {
                at: Some((20.0, 30.0)),
                modifiers: Mods::default()
            })
        );
        assert_eq!(scroller.held(start + LONG_PRESS * 2), None, "once");
        assert!(scroller.touch(touch(TouchKind::Up, 1, 20.0, 30.0), start + LONG_PRESS).is_empty());
    }

    #[test]
    fn a_second_finger_cancels_the_first_ones_tap() {
        let now = Instant::now();
        let mut scroller = TouchScroller::new();
        scroller.touch(touch(TouchKind::Down, 1, 10.0, 10.0), now);
        scroller.touch(touch(TouchKind::Down, 2, 90.0, 10.0), now);
        assert!(scroller.touch(touch(TouchKind::Up, 1, 10.0, 10.0), now).is_empty());
        // The next touch starts clean.
        scroller.touch(touch(TouchKind::Down, 3, 5.0, 5.0), now);
        assert_eq!(scroller.touch(touch(TouchKind::Up, 3, 5.0, 5.0), now).len(), 2);
    }

    #[test]
    fn a_cancelled_touch_does_nothing() {
        let now = Instant::now();
        let mut scroller = TouchScroller::new();
        scroller.touch(touch(TouchKind::Down, 1, 10.0, 10.0), now);
        assert!(scroller.touch(touch(TouchKind::Cancel, 1, 10.0, 10.0), now).is_empty());
        assert!(scroller.touch(touch(TouchKind::Up, 1, 10.0, 10.0), now).is_empty());
    }
}
