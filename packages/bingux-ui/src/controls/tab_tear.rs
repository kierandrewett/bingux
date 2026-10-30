//! Tearing a tab off its strip, and putting it back: the rules, with no
//! widgets in them, so each can be tested without a window.
//!
//! * [`TearGate`]: when a tab that is being dragged leaves the strip for good.
//!   The pointer's distance from the strip, in strip heights, goes through two
//!   thresholds with a gap between them, so a hand that drifts around one
//!   cannot make the tab flicker between held and loose: the tab is *armed*
//!   (drawn lifted) at [`ARM_AT`], disarmed again only below [`DISARM_AT`], and
//!   torn off at [`TEAR_AT`]. A lone tab has no neighbours to be reordered
//!   among, so dragging it means moving its window, which begins as soon as the
//!   pointer has travelled [`LONE_SLOP`] in any direction.
//! * [`StripLayout`]: where a strip's tabs are in its window, published by the
//!   strip through a [`StripProbe`], so another window's strip can be asked
//!   where a dragged tab would land.
//! * [`StripLayout::index_at`] and [`StripLayout::boundary`]: the place in the
//!   list a pointer at some x asks for, and the x of the marker that shows it.

use std::cell::RefCell;
use std::rc::Rc;

/// The pointer's distance from the strip, in strip heights, at which a held
/// tab is drawn lifted (armed) to say that going on will tear it off.
pub const ARM_AT: f32 = 0.6;

/// The distance below which an armed tab is put down again. It is lower than
/// [`ARM_AT`]: that gap is the hysteresis.
pub const DISARM_AT: f32 = 0.4;

/// The distance, in strip heights, at which the tab is torn off. It is
/// measured from the strip's nearest edge, up or down.
pub const TEAR_AT: f32 = 1.0;

/// How far a lone tab travels, in logical pixels, before its window follows
/// the pointer.
pub const LONE_SLOP: f32 = 6.0;

/// What the pointer's distance from the strip has made of a drag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pull {
    /// Within reach of the strip: the tab is being reordered.
    Docked,
    /// Far enough that letting go still reorders, but going on tears it off.
    Armed,
    /// Torn off. The state does not end: the tab has left.
    Torn,
}

/// The state of one drag against the tear thresholds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TearGate {
    strip: f32,
    pull: Pull,
}

impl TearGate {
    /// A gate for a strip `strip` pixels tall.
    pub fn new(strip: f32) -> Self {
        Self { strip: strip.max(1.0), pull: Pull::Docked }
    }

    pub fn pull(&self) -> Pull {
        self.pull
    }

    /// Move the pointer to `away` pixels outside the strip (see [`away`]; 0
    /// is anywhere inside it) and say what that makes of the drag.
    pub fn update(&mut self, away: f32) -> Pull {
        let heights = away.max(0.0) / self.strip;
        self.pull = match self.pull {
            Pull::Torn => Pull::Torn,
            _ if heights >= TEAR_AT => Pull::Torn,
            Pull::Docked if heights >= ARM_AT => Pull::Armed,
            Pull::Armed if heights < DISARM_AT => Pull::Docked,
            pull => pull,
        };
        self.pull
    }

    /// The same for a lone tab: it is torn once the pointer has travelled
    /// [`LONE_SLOP`] from where it went down, whichever way.
    pub fn update_lone(&mut self, travelled: (f32, f32)) -> Pull {
        if travelled.0.hypot(travelled.1) >= LONE_SLOP {
            self.pull = Pull::Torn;
        }
        self.pull
    }
}

/// How far `y` is outside the strip that spans `top..bottom`: 0 inside it.
pub fn away(y: f32, top: f32, bottom: f32) -> f32 {
    if y < top {
        top - y
    } else if y > bottom {
        y - bottom
    } else {
        0.0
    }
}

/// Where a strip's tabs are in its window, in logical pixels.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StripLayout {
    /// The strip's row: left, top, right, bottom.
    pub rail: (f32, f32, f32, f32),
    /// The left and right edge of each tab's body, in the strip's order, scrolled
    /// as they are now. A tab that is leaving is not in the list. In a
    /// right-to-left strip the first tab is the rightmost.
    pub spans: Vec<(f32, f32)>,
    /// The strip reads from the right.
    pub rtl: bool,
}

impl StripLayout {
    /// Whether a drag whose pointer is at `(x, y)` in the window is over the
    /// strip's row, taking `slack` pixels above and below it for the hand that
    /// is not exact. The row runs to `right` (a window's width), not to its
    /// last tab: the empty bar beside the tabs is a place to drop at the end.
    pub fn accepts(&self, x: f32, y: f32, right: f32, slack: f32) -> bool {
        let (left, top, _, bottom) = self.rail;
        // The empty bar is beside the tabs, on the side the strip ends: the right
        // in a left-to-right strip, the left in a right-to-left one, which hugs
        // the window's right edge.
        let (from, to) = if self.rtl { (0.0, self.rail.2) } else { (left, right) };
        self.rail.2 > self.rail.0 && x >= from && x < to && y >= top - slack && y <= bottom + slack
    }

    /// The place in the list a tab dropped with the pointer at `x` belongs
    /// at: the number of tabs whose middle the pointer has passed, going in
    /// the strip's reading direction (to the right, or in a right-to-left strip
    /// to the left).
    pub fn index_at(&self, x: f32) -> usize {
        if self.rtl {
            self.spans.iter().take_while(|(left, right)| (left + right) / 2.0 > x).count()
        } else {
            self.spans.iter().take_while(|(left, right)| (left + right) / 2.0 < x).count()
        }
    }

    /// The x, in the window, of the line between the tab before `index` and
    /// the tab at it (the start of the first, the end of the last), halfway
    /// across the gap where tabs have one. `None` for an empty strip.
    pub fn boundary(&self, index: usize) -> Option<f32> {
        let first = self.spans.first()?;
        let last = self.spans.last()?;
        Some(match (self.rtl, index) {
            (false, 0) => first.0,
            (true, 0) => first.1,
            (false, i) if i >= self.spans.len() => last.1,
            (true, i) if i >= self.spans.len() => last.0,
            (false, i) => (self.spans[i - 1].1 + self.spans[i].0) / 2.0,
            (true, i) => (self.spans[i - 1].0 + self.spans[i].1) / 2.0,
        })
    }
}

/// A strip's layout, shared between the strip, which writes it every time it
/// draws, and a host, which reads it when a dragged tab asks where it is.
#[derive(Clone, Default)]
pub struct StripProbe(Rc<RefCell<StripLayout>>);

impl StripProbe {
    pub fn new() -> Self {
        Self::default()
    }

    /// The layout as the strip last drew it.
    pub fn layout(&self) -> StripLayout {
        self.0.borrow().clone()
    }

    pub(crate) fn set(&self, layout: StripLayout) {
        if *self.0.borrow() != layout {
            *self.0.borrow_mut() = layout;
        }
    }
}

impl PartialEq for StripProbe {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// A tab torn off its strip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Detach {
    /// The tab.
    pub id: u64,
    /// The pointer in the strip's window when it was torn off.
    pub pointer: (f32, f32),
    /// The pointer in the strip's window when its button went down. A
    /// compositor may anchor a window move there (mutter's
    /// `xdg_toplevel.move` does) rather than at [`Detach::pointer`].
    pub press: (f32, f32),
    /// Where the pointer held the tab, from its body's left edge and the
    /// strip's top edge.
    pub grab: (f32, f32),
    /// The button was already up: the tab was let go beyond the strip, so
    /// nothing is being dragged any more and no window can follow the pointer.
    pub released: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    const STRIP: f32 = 36.0;

    fn pull_after(path: &[f32]) -> Vec<Pull> {
        let mut gate = TearGate::new(STRIP);
        path.iter().map(|away| gate.update(*away)).collect()
    }

    #[test]
    fn a_tab_inside_the_strip_is_reordered_never_torn() {
        for away in [0.0, 1.0, 10.0, 0.59 * STRIP] {
            assert_eq!(pull_after(&[away]), [Pull::Docked], "{away}px from the strip");
        }
    }

    #[test]
    fn the_tab_is_armed_at_the_arm_distance_and_torn_at_one_strip_height() {
        assert_eq!(pull_after(&[ARM_AT * STRIP + 0.01]), [Pull::Armed]);
        assert_eq!(pull_after(&[TEAR_AT * STRIP - 0.5]), [Pull::Armed]);
        assert_eq!(pull_after(&[TEAR_AT * STRIP]), [Pull::Torn]);
        assert_eq!(pull_after(&[400.0]), [Pull::Torn], "a quick flick goes straight to torn");
    }

    #[test]
    fn drift_around_the_arm_distance_does_not_flicker() {
        // Out to the arm distance, then wobbling by a few pixels around it.
        let wobble = [0.62, 0.58, 0.61, 0.57, 0.6, 0.45, 0.5, 0.6].map(|h| h * STRIP);
        assert_eq!(
            pull_after(&wobble),
            [Pull::Armed; 8],
            "armed stays armed until the pointer is well back"
        );
        // Well back disarms, and the same drift does not arm it again below the arm distance.
        assert_eq!(pull_after(&[0.7 * STRIP, 0.39 * STRIP, 0.5 * STRIP]), [Pull::Armed, Pull::Docked, Pull::Docked]);
    }

    #[test]
    fn a_torn_tab_stays_torn_however_the_pointer_comes_back() {
        let path = [2.0 * STRIP, 0.0, 0.1 * STRIP, 3.0 * STRIP];
        assert_eq!(pull_after(&path), [Pull::Torn; 4]);
    }

    #[test]
    fn a_pointer_below_or_above_the_strip_is_the_same_distance() {
        assert_eq!(away(100.0, 0.0, 36.0), 64.0);
        assert_eq!(away(-20.0, 0.0, 36.0), 20.0);
        assert_eq!(away(10.0, 0.0, 36.0), 0.0);
        assert_eq!(away(0.0, 0.0, 36.0), 0.0);
        assert_eq!(away(36.0, 0.0, 36.0), 0.0);
    }

    #[test]
    fn a_lone_tab_follows_the_pointer_with_its_window_after_the_slop() {
        let mut gate = TearGate::new(STRIP);
        assert_eq!(gate.update_lone((3.0, 3.0)), Pull::Docked, "a click that wobbles is a click");
        assert_eq!(gate.update_lone((LONE_SLOP, 0.0)), Pull::Torn, "sideways counts");
        let mut gate = TearGate::new(STRIP);
        assert_eq!(gate.update_lone((0.0, -LONE_SLOP)), Pull::Torn, "so does up");
    }

    fn layout() -> StripLayout {
        // Three 100 px bodies, 2 px apart, from x = 20.
        StripLayout {
            rail: (12.0, 0.0, 400.0, 36.0),
            spans: vec![(20.0, 120.0), (122.0, 222.0), (224.0, 324.0)],
            rtl: false,
        }
    }

    #[test]
    fn a_pointer_asks_for_the_place_whose_middle_it_has_passed() {
        let layout = layout();
        let at = |x: f32| layout.index_at(x);
        assert_eq!(at(-50.0), 0);
        assert_eq!(at(20.0), 0);
        assert_eq!(at(69.0), 0, "left of the first tab's middle");
        assert_eq!(at(71.0), 1);
        assert_eq!(at(171.0), 1, "left of the second tab's middle (172)");
        assert_eq!(at(173.0), 2);
        assert_eq!(at(273.0), 2, "left of the third tab's middle (274)");
        assert_eq!(at(275.0), 3);
        assert_eq!(at(900.0), 3, "past the last tab is the end");
        assert_eq!(StripLayout::default().index_at(10.0), 0, "an empty strip takes it first");
    }

    #[test]
    fn the_marker_stands_between_the_neighbours_it_will_have() {
        let layout = layout();
        assert_eq!(layout.boundary(0), Some(20.0));
        assert_eq!(layout.boundary(1), Some(121.0));
        assert_eq!(layout.boundary(2), Some(223.0));
        assert_eq!(layout.boundary(3), Some(324.0));
        assert_eq!(layout.boundary(9), Some(324.0), "beyond the end is the end");
        assert_eq!(StripLayout::default().boundary(0), None);
        // The place and the marker agree for every pointer position.
        for x in (0..400).step_by(3) {
            let index = layout.index_at(x as f32);
            let marker = layout.boundary(index).unwrap();
            let before = layout.spans.get(index.wrapping_sub(1));
            let after = layout.spans.get(index);
            if let Some(before) = before {
                assert!(marker >= before.1 - 0.01, "x {x}: the marker is past the tab before it");
            }
            if let Some(after) = after {
                assert!(marker <= after.0 + 0.01, "x {x}: the marker is before the tab after it");
            }
        }
    }

    #[test]
    fn the_bar_beside_the_tabs_accepts_a_drop_but_the_page_does_not() {
        let layout = layout();
        let right = 1000.0;
        assert!(layout.accepts(60.0, 18.0, right, 0.0));
        assert!(layout.accepts(700.0, 18.0, right, 0.0), "the empty bar is the end of the strip");
        assert!(!layout.accepts(60.0, 120.0, right, 0.0), "the page is not the strip");
        assert!(layout.accepts(60.0, 36.0 + 10.0, right, 12.0), "a little below counts when the hand is loose");
        assert!(!layout.accepts(60.0, 36.0 + 20.0, right, 12.0));
        assert!(!layout.accepts(4.0, 18.0, right, 12.0), "left of the strip is the window's edge");
        assert!(!StripLayout::default().accepts(5.0, 5.0, right, 5.0), "a strip never laid out takes nothing");
    }

    /// The same three bodies in a right-to-left strip: the first is at the
    /// right, and the strip hugs the right edge of a 400 px window.
    fn mirrored() -> StripLayout {
        StripLayout {
            rail: (60.0, 0.0, 400.0, 36.0),
            spans: vec![(288.0, 388.0), (186.0, 286.0), (84.0, 184.0)],
            rtl: true,
        }
    }

    #[test]
    fn right_to_left_a_pointer_asks_for_the_place_whose_middle_it_has_passed_going_left() {
        let layout = mirrored();
        let at = |x: f32| layout.index_at(x);
        assert_eq!(at(900.0), 0, "right of the first tab's middle");
        assert_eq!(at(339.0), 0);
        assert_eq!(at(337.0), 1);
        assert_eq!(at(237.0), 1);
        assert_eq!(at(235.0), 2);
        assert_eq!(at(135.0), 2);
        assert_eq!(at(133.0), 3);
        assert_eq!(at(-50.0), 3, "past the last tab, on the left, is the end");
        assert_eq!(StripLayout { rtl: true, ..StripLayout::default() }.index_at(10.0), 0);
    }

    #[test]
    fn right_to_left_the_marker_stands_between_the_neighbours_it_will_have() {
        let layout = mirrored();
        assert_eq!(layout.boundary(0), Some(388.0), "the start is the first tab's right edge");
        assert_eq!(layout.boundary(1), Some(287.0));
        assert_eq!(layout.boundary(2), Some(185.0));
        assert_eq!(layout.boundary(3), Some(84.0), "the end is the last tab's left edge");
        assert_eq!(layout.boundary(9), Some(84.0));
        for x in (0..420).step_by(3) {
            let index = layout.index_at(x as f32);
            let marker = layout.boundary(index).unwrap();
            if let Some(before) = layout.spans.get(index.wrapping_sub(1)) {
                assert!(marker <= before.0 + 0.01, "x {x}: the marker is on the wrong side of the tab before it");
            }
            if let Some(after) = layout.spans.get(index) {
                assert!(marker >= after.1 - 0.01, "x {x}: the marker is on the wrong side of the tab after it");
            }
        }
    }

    #[test]
    fn right_to_left_the_empty_bar_beside_the_tabs_is_on_the_left() {
        let layout = mirrored();
        assert!(layout.accepts(30.0, 18.0, 400.0, 0.0), "left of the tabs is the end of the strip");
        assert!(layout.accepts(200.0, 18.0, 400.0, 0.0));
        assert!(!layout.accepts(399.0 + 5.0, 18.0, 400.0, 0.0), "past the window's right edge");
        assert!(!layout.accepts(30.0, 120.0, 400.0, 0.0), "the page is not the strip");
    }
}
