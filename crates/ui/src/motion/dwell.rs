//! A signal that shows late and goes late, so a state that is true for a
//! moment does not flicker.
//!
//! A Stop button that stands in for Reload while a folder loads is the case
//! this is for: a folder that opens in 40 ms must not flash a Stop square for
//! two frames, and a load that ends 20 ms after the square came up must not
//! take it away again at once. [`Dwell`] says how long:
//!
//! - **Show after.** The signal is `active`, but it is not shown until it has
//!   been active for [`Dwell::show_after`] without a break. A break (the signal
//!   goes false) starts the wait over.
//! - **Hold.** Once shown, it stays shown for at least [`Dwell::hold`], from
//!   the moment it was shown, even if `active` goes false at once. It goes
//!   when `active` is false and the hold is over. While `active` stays true
//!   it stays shown, however long that is. If `active` comes back during the
//!   hold the hold is simply over later: the signal never goes off and on
//!   again.
//!
//! [`DwellGate`] is the state machine, on a clock the caller passes in, so it
//! is tested with a fake one. [`use_dwell`] is the hook: it reads the real
//! clock and wakes the component when the answer changes by time alone.
//!
//! [`Dwell::LOADING`] is the Files toolbar's choice: 150 ms before the Stop
//! state shows, then at least 300 ms on screen. A folder that opens within
//! 150 ms never shows the state at all. The hold is a little longer than the
//! 160 ms morph (`MOVE_MS`) it starts, so the icon reaches the Stop square and
//! is on screen as such before it turns back; when the read ends at once after
//! the state came up, the change back starts 300 ms after it came up.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use freya::prelude::*;

/// How long a signal waits before it shows and stays once shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dwell {
    /// How long the signal must have been active before it is shown.
    pub show_after: Duration,
    /// The least time it stays shown, counted from when it was shown.
    pub hold: Duration,
}

impl Dwell {
    /// The Stop state of a loading view: shown after 150 ms of loading, then
    /// on screen for at least 300 ms.
    pub const LOADING: Dwell = Dwell {
        show_after: Duration::from_millis(150),
        hold: Duration::from_millis(300),
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    /// Not active, not shown.
    Idle,
    /// Active since `since`, not shown yet.
    Waiting { since: Duration },
    /// Shown since `since`; `active` is whether the signal is still on.
    Shown { since: Duration, active: bool },
}

/// The state machine of a [`Dwell`], on a clock the caller supplies: the time
/// since any fixed start, which never goes back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DwellGate {
    dwell: Dwell,
    phase: Phase,
}

impl DwellGate {
    pub fn new(dwell: Dwell) -> Self {
        DwellGate {
            dwell,
            phase: Phase::Idle,
        }
    }

    /// Tell the gate whether the signal is `active` at `now`, and get whether
    /// it is shown at `now`. Safe to call as often as wanted with the same
    /// arguments.
    pub fn update(&mut self, active: bool, now: Duration) -> bool {
        self.phase = match (self.phase, active) {
            (Phase::Idle, false) => Phase::Idle,
            (Phase::Idle, true) => self.begin(now),
            (Phase::Waiting { .. }, false) => Phase::Idle,
            (Phase::Waiting { since }, true) => {
                if now.saturating_sub(since) >= self.dwell.show_after {
                    Phase::Shown { since: now, active: true }
                } else {
                    Phase::Waiting { since }
                }
            }
            (Phase::Shown { since, .. }, true) => Phase::Shown { since, active: true },
            (Phase::Shown { since, .. }, false) => {
                if now.saturating_sub(since) >= self.dwell.hold {
                    Phase::Idle
                } else {
                    Phase::Shown { since, active: false }
                }
            }
        };
        self.shown()
    }

    fn begin(&self, now: Duration) -> Phase {
        if self.dwell.show_after.is_zero() {
            Phase::Shown { since: now, active: true }
        } else {
            Phase::Waiting { since: now }
        }
    }

    /// Whether the signal is shown, as of the last [`DwellGate::update`].
    pub fn shown(&self) -> bool {
        matches!(self.phase, Phase::Shown { .. })
    }

    /// The time at which the answer changes with no new input (the wait ends
    /// and the signal shows, or the hold ends and it goes), if one is
    /// pending: when to call [`DwellGate::update`] again.
    pub fn wake_at(&self) -> Option<Duration> {
        match self.phase {
            Phase::Waiting { since } => Some(since + self.dwell.show_after),
            Phase::Shown { since, active: false } => Some(since + self.dwell.hold),
            Phase::Idle | Phase::Shown { active: true, .. } => None,
        }
    }
}

/// [`DwellGate`] on the real clock, for a component: whether `active` is
/// shown under `dwell`. The component draws again when the answer changes by
/// time alone, so it is right without any other input.
pub fn use_dwell(active: bool, dwell: Dwell) -> bool {
    let mut wake = use_state(|| 0_u32);
    // Reading it subscribes: the timer below bumps it.
    let _ = wake.read();
    let started = use_hook(Instant::now);
    let gate = use_hook(|| Rc::new(RefCell::new(DwellGate::new(dwell))));
    let pending = use_hook(|| Rc::new(RefCell::new(None::<(Duration, TaskHandle)>)));
    let now = started.elapsed();
    let (shown, wake_at) = {
        let mut gate = gate.borrow_mut();
        (gate.update(active, now), gate.wake_at())
    };
    let mut pending_slot = pending.borrow_mut();
    if pending_slot.as_ref().map(|(at, _)| *at) != wake_at {
        if let Some((_, task)) = pending_slot.take() {
            task.try_cancel();
        }
        if let Some(at) = wake_at {
            let wait = at.saturating_sub(now);
            let done = pending.clone();
            let task = spawn(async move {
                timer(wait).await;
                // This timer has fired: a later render may ask for another.
                done.borrow_mut().take();
                *wake.write() += 1;
            });
            *pending_slot = Some((at, task));
        }
    }
    shown
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn gate() -> DwellGate {
        DwellGate::new(Dwell::LOADING)
    }

    #[test]
    fn a_fast_load_never_shows() {
        let mut gate = gate();
        assert!(!gate.update(true, ms(0)));
        assert!(!gate.update(true, ms(100)));
        assert!(!gate.update(false, ms(120)));
        // Nothing is left pending: no flicker later either.
        assert_eq!(gate.wake_at(), None);
        assert!(!gate.update(false, ms(1000)));
    }

    #[test]
    fn it_shows_after_the_wait_and_asks_to_be_woken_then() {
        let mut gate = gate();
        assert!(!gate.update(true, ms(1000)));
        assert_eq!(gate.wake_at(), Some(ms(1150)));
        assert!(!gate.update(true, ms(1149)));
        assert!(gate.update(true, ms(1150)));
        // Still going: it stays and nothing is pending.
        assert_eq!(gate.wake_at(), None);
        assert!(gate.update(true, ms(60_000)));
    }

    #[test]
    fn once_shown_it_stays_for_the_hold_whatever_active_says() {
        let mut gate = gate();
        gate.update(true, ms(0));
        assert!(gate.update(true, ms(150)));
        // The load ends 20 ms after the square came up.
        assert!(gate.update(false, ms(170)));
        assert_eq!(gate.wake_at(), Some(ms(450)));
        assert!(gate.update(false, ms(449)));
        assert!(!gate.update(false, ms(450)));
        assert_eq!(gate.wake_at(), None);
    }

    #[test]
    fn a_load_that_outlasts_the_hold_ends_the_moment_it_is_over() {
        let mut gate = gate();
        gate.update(true, ms(0));
        assert!(gate.update(true, ms(150)));
        assert!(gate.update(true, ms(2000)));
        assert!(!gate.update(false, ms(2001)));
    }

    #[test]
    fn a_break_in_the_wait_starts_it_over() {
        let mut gate = gate();
        gate.update(true, ms(0));
        gate.update(true, ms(140));
        gate.update(false, ms(145));
        // Active again at 200: the wait counts from here, not from 0.
        assert!(!gate.update(true, ms(200)));
        assert_eq!(gate.wake_at(), Some(ms(350)));
        assert!(!gate.update(true, ms(349)));
        assert!(gate.update(true, ms(350)));
    }

    #[test]
    fn coming_back_during_the_hold_keeps_it_shown_without_a_second_wait() {
        let mut gate = gate();
        gate.update(true, ms(0));
        assert!(gate.update(true, ms(150)));
        assert!(gate.update(false, ms(200)));
        assert!(gate.update(true, ms(250)));
        assert_eq!(gate.wake_at(), None);
        // The hold counted from 150; with the signal on again it no longer
        // matters, and the end is the signal's own.
        assert!(gate.update(true, ms(600)));
        assert!(!gate.update(false, ms(601)));
    }

    #[test]
    fn a_late_update_still_holds_from_when_it_was_shown() {
        // The component is only drawn again at 400 ms: the wait was over at
        // 150, but the person first sees the square at 400.
        let mut gate = gate();
        gate.update(true, ms(0));
        assert!(gate.update(true, ms(400)));
        assert!(gate.update(false, ms(500)));
        assert_eq!(gate.wake_at(), Some(ms(700)));
        assert!(!gate.update(false, ms(700)));
    }

    #[test]
    fn no_wait_shows_at_once() {
        let mut gate = DwellGate::new(Dwell { show_after: Duration::ZERO, hold: ms(300) });
        assert!(gate.update(true, ms(5)));
        assert!(gate.update(false, ms(6)));
        assert!(!gate.update(false, ms(305)));
    }
}
