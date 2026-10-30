//! The hide clock of a toast: a pure state machine that decides when a
//! notice that dismisses itself may go.
//!
//! Three things hold a notice open: the pointer anywhere over it, a finger
//! down on it, and keyboard focus on one of its buttons. While any of them
//! holds, the clock does not run. When the last one lets go the whole delay
//! starts again from zero, not from where it stopped, so a person who read
//! the notice, moved the pointer off and looked away still gets the full
//! time to find it.
//!
//! The machine knows no clock of its own: the caller feeds it the time that
//! passed ([`HideClock::tick`]) and the holds as they change
//! ([`HideClock::set_pointer`], [`HideClock::set_touch`],
//! [`HideClock::set_focus`]), so the tests drive it with a fake clock.

use std::time::Duration;

/// What holds a notice open right now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Holds {
    /// The pointer is over the notice, on any part of it: its padding, its
    /// text, the gaps between its parts and its buttons.
    pub pointer: bool,
    /// A finger is down on it.
    pub touch: bool,
    /// One of its buttons has the keyboard focus.
    pub focus: bool,
}

impl Holds {
    /// Whether anything holds the notice.
    pub(crate) fn any(self) -> bool {
        self.pointer || self.touch || self.focus
    }
}

/// When a notice may close. See the module's description.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HideClock {
    delay: Duration,
    /// Time that has passed with nothing holding the notice, since the last
    /// time something did (or since it appeared).
    waited: Duration,
    holds: Holds,
}

impl HideClock {
    /// A clock that lets the notice go after `delay` of being left alone.
    pub(crate) fn new(delay: Duration) -> Self {
        Self {
            delay,
            waited: Duration::ZERO,
            holds: Holds::default(),
        }
    }

    /// Set what holds the notice. Any change of the holds, taking one or
    /// letting one go, starts the delay again, so a pointer that crosses the
    /// notice between two ticks still counts, and the delay always starts
    /// from zero once the notice is free.
    pub(crate) fn set(&mut self, holds: Holds) {
        if holds != self.holds {
            self.holds = holds;
            self.waited = Duration::ZERO;
        }
    }

    pub(crate) fn set_pointer(&mut self, over: bool) {
        self.set(Holds {
            pointer: over,
            ..self.holds
        });
    }

    pub(crate) fn set_touch(&mut self, down: bool) {
        self.set(Holds {
            touch: down,
            ..self.holds
        });
    }

    pub(crate) fn set_focus(&mut self, focused: bool) {
        self.set(Holds {
            focus: focused,
            ..self.holds
        });
    }

    /// What holds the notice now.
    #[cfg(test)]
    pub(crate) fn holds(&self) -> Holds {
        self.holds
    }

    /// Let `elapsed` pass. Returns true once the notice has been left alone
    /// for the whole delay: it may go. While anything holds it, time does
    /// not count and the answer is false.
    pub(crate) fn tick(&mut self, elapsed: Duration) -> bool {
        if self.holds.any() {
            self.waited = Duration::ZERO;
            return false;
        }
        self.waited = self.waited.saturating_add(elapsed);
        self.waited >= self.delay
    }

    /// How long until the notice goes if nothing takes hold of it. The whole
    /// delay while it is held.
    #[cfg(test)]
    pub(crate) fn remaining(&self) -> Duration {
        if self.holds.any() {
            self.delay
        } else {
            self.delay.saturating_sub(self.waited)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DELAY: Duration = Duration::from_secs(6);
    const TICK: Duration = Duration::from_millis(100);

    /// A clock and a fake time to drive it with: `run(clock, span)` lets
    /// `span` pass in ticks and says whether the notice went in that time.
    fn run(clock: &mut HideClock, span: Duration) -> bool {
        let mut left = span;
        while !left.is_zero() {
            let step = left.min(TICK);
            left -= step;
            if clock.tick(step) {
                return true;
            }
        }
        false
    }

    fn secs(seconds: f32) -> Duration {
        Duration::from_millis((seconds * 1000.0).round() as u64)
    }

    #[test]
    fn an_untouched_notice_goes_after_its_delay() {
        let mut clock = HideClock::new(DELAY);
        assert!(!run(&mut clock, secs(5.9)));
        assert_eq!(clock.remaining(), secs(0.1));
        assert!(run(&mut clock, secs(0.2)));
    }

    #[test]
    fn the_pointer_over_it_holds_it_for_as_long_as_it_stays() {
        let mut clock = HideClock::new(DELAY);
        assert!(!run(&mut clock, secs(2.0)));
        clock.set_pointer(true);
        assert!(!run(&mut clock, Duration::from_secs(600)), "ten minutes over it");
        assert_eq!(clock.remaining(), DELAY, "the clock is not running");
    }

    #[test]
    fn leaving_restarts_the_whole_delay_not_what_was_left() {
        let mut clock = HideClock::new(DELAY);
        // 5.5 s of the 6 s pass, then the pointer comes and goes.
        assert!(!run(&mut clock, secs(5.5)));
        clock.set_pointer(true);
        assert!(!run(&mut clock, secs(30.0)));
        clock.set_pointer(false);
        assert_eq!(clock.remaining(), DELAY, "the full delay again");
        // Half a second after leaving it would be gone if it resumed.
        assert!(!run(&mut clock, secs(5.9)));
        assert!(run(&mut clock, secs(0.2)));
    }

    #[test]
    fn a_pointer_that_crosses_between_two_ticks_still_restarts_the_delay() {
        let mut clock = HideClock::new(DELAY);
        assert!(!run(&mut clock, secs(5.9)));
        // In and out with no tick between: it was held for an instant.
        clock.set_pointer(true);
        clock.set_pointer(false);
        assert!(!run(&mut clock, secs(5.9)));
        assert!(run(&mut clock, secs(0.2)));
    }

    #[test]
    fn entering_again_pauses_again_and_each_leave_restarts() {
        let mut clock = HideClock::new(DELAY);
        for _ in 0..5 {
            assert!(!run(&mut clock, secs(5.0)));
            clock.set_pointer(true);
            assert!(!run(&mut clock, secs(8.0)));
            clock.set_pointer(false);
            assert_eq!(clock.remaining(), DELAY);
        }
        assert!(run(&mut clock, DELAY + TICK));
    }

    #[test]
    fn keyboard_focus_on_a_button_holds_it_and_its_release_restarts_the_delay() {
        let mut clock = HideClock::new(DELAY);
        assert!(!run(&mut clock, secs(5.0)));
        clock.set_focus(true);
        assert!(!run(&mut clock, secs(60.0)));
        clock.set_focus(false);
        assert!(!run(&mut clock, secs(5.9)));
        assert!(run(&mut clock, secs(0.2)));
    }

    #[test]
    fn a_finger_down_holds_it_and_lifting_restarts_the_delay() {
        let mut clock = HideClock::new(DELAY);
        assert!(!run(&mut clock, secs(5.0)));
        clock.set_touch(true);
        assert!(!run(&mut clock, secs(60.0)));
        clock.set_touch(false);
        assert!(!run(&mut clock, secs(5.9)));
        assert!(run(&mut clock, secs(0.2)));
    }

    #[test]
    fn it_stays_held_until_every_hold_has_let_go() {
        let mut clock = HideClock::new(DELAY);
        clock.set_pointer(true);
        clock.set_focus(true);
        assert!(!run(&mut clock, secs(30.0)));
        // The pointer leaves, but the focus still holds it.
        clock.set_pointer(false);
        assert!(clock.holds().any());
        assert!(!run(&mut clock, secs(30.0)));
        clock.set_focus(false);
        assert!(!clock.holds().any());
        assert!(!run(&mut clock, secs(5.9)));
        assert!(run(&mut clock, secs(0.2)));
    }

    #[test]
    fn setting_a_hold_that_is_already_set_changes_nothing() {
        let mut clock = HideClock::new(DELAY);
        assert!(!run(&mut clock, secs(4.0)));
        // No hold is taken, so the delay is not restarted by a repeat.
        clock.set_pointer(false);
        clock.set_touch(false);
        assert_eq!(clock.remaining(), secs(2.0));
        clock.set_pointer(true);
        clock.set_pointer(true);
        assert!(!run(&mut clock, secs(10.0)));
    }

    #[test]
    fn a_long_gap_between_ticks_counts_in_full() {
        // A stalled frame must not stretch the notice's time.
        let mut clock = HideClock::new(DELAY);
        assert!(clock.tick(Duration::from_secs(7)));
    }
}
