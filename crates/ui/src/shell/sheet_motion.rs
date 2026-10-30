//! How a bottom sheet moves: opening, closing, a release and a spring back.
//!
//! A sheet is laid out once, at its final size, and only translated, so
//! nothing in it reflows while it moves. This module decides where it is.
//!
//! It draws its numbers from the one motion token set of the navigation
//! gestures ([`crate::nav::gesture`]), so a sheet and a subpage move alike:
//!
//! - **Opening** is [`slide`] over [`SLIDE_MS`]: a symmetric ease-in-out, a
//!   page's own curve, so a sheet gets going within a few frames and settles
//!   without a long tail.
//! - **Closing** is the opening played backwards over the same time: a clock
//!   that runs from 1 to 0 and a sheet that is where it was on the way up.
//!   [`slide`] is point-symmetric, so that is also the opening mirrored in
//!   space: one motion by either reading, at the same speed in and out.
//!   Because it is one clock, a sheet closed half way up turns round where it
//!   is and a sheet reopened half way down does too: neither starts again.
//! - **A release past the dismiss distance** carries on from where the finger
//!   left the sheet, at the speed it had ([`release_offset`]), over the same
//!   [`SLIDE_MS`], and comes to rest off screen.
//! - **A release short of it** springs back from where it is, with the speed
//!   it had, over [`SPRING_MS`] ([`spring_back`]): the spring of a page that
//!   a swipe cancels.
//!
//! The backdrop follows the sheet's position, so it fades with exactly the
//! sheet's timing.

use std::cell::Cell;
use std::rc::Rc;

use freya::animation::*;
use freya::prelude::*;

use crate::nav::gesture::{
    SLIDE_MS, SPRING_MS, SPRING_OMEGA, SPRING_ZETA, release_curve, release_slope, slide, spring,
};

/// How long a sheet takes to open or close.
pub(crate) const SHEET_MS: u64 = SLIDE_MS;
/// How long a sheet takes to spring back.
pub(crate) const SHEET_SPRING_MS: u64 = SPRING_MS;

/// How open a sheet is (0 shut, 1 open) when its clock reads `t`. The clock
/// runs forward to open and backward to close.
pub(crate) fn openness(t: f32) -> f32 {
    slide(t)
}

/// Where a sheet let go at `pull` with `velocity` is at `u` (0 to 1) of
/// [`SHEET_MS`] on its way off the window `travel` away.
pub(crate) fn release_offset(pull: f32, velocity: f32, travel: f32, u: f32) -> f32 {
    let remaining = (travel - pull).max(0.0);
    pull + remaining * release_curve(u, release_slope(velocity, remaining))
}

/// Where a sheet let go at `from` (pixels below rest) with `velocity` (pixels
/// per second, downward positive) is at `u` (0 to 1) of its spring back. It
/// is the page spring ([`spring`]) for the distance, and the response of the
/// same spring to the speed, added: the spring is linear, so they add. It
/// never rises more than `lift` above rest.
pub(crate) fn spring_back(from: f32, velocity: f32, u: f32, lift: f32) -> f32 {
    let u = u.clamp(0.0, 1.0);
    if u >= 1.0 {
        return 0.0;
    }
    let damped = SPRING_OMEGA * (1.0 - SPRING_ZETA * SPRING_ZETA).sqrt();
    // The speed in pixels per unit of u.
    let speed = velocity * SHEET_SPRING_MS as f32 / 1000.0;
    let impulse = (-SPRING_ZETA * SPRING_OMEGA * u).exp() * (damped * u).sin() / damped;
    (from * (1.0 - spring(u)) + speed * impulse).max(-lift)
}

/// How a sheet that is not being dragged is pulled: what is at the pull, and
/// the release that last set it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Release {
    /// Counts releases, so a new one starts the spring over.
    pub id: u64,
    /// How far the sheet was pulled when let go.
    pub pull: f32,
    /// How fast it was moving, in pixels per second, downward positive.
    pub velocity: f32,
}

impl Release {
    /// The next release, at `pull` and `velocity`.
    pub(crate) fn next(self, pull: f32, velocity: f32) -> Self {
        Self {
            id: self.id.wrapping_add(1).max(1),
            pull,
            velocity,
        }
    }
}

/// The pull a sheet shows: the pointer's while `dragging`; otherwise what is
/// held (a dismissed sheet's) or, when the pull is back at rest, the spring
/// from `release` at `u` of the way.
pub(crate) fn settled_pull(pull: f32, dragging: bool, release: Release, u: f32, lift: f32) -> f32 {
    if dragging || pull != 0.0 || release.id == 0 {
        return pull;
    }
    spring_back(release.pull, release.velocity, u, lift)
}

/// The length of a clock run from `from` to `target` that takes `ms` for the
/// whole way: a clock turned round half way takes the time it has left to go.
pub(crate) fn clock_ms(ms: u64, from: f32, target: f32) -> u64 {
    (ms as f32 * (target - from).abs()).round() as u64
}

/// A sheet's clock, from 0 (shut) to 1 (open) in [`SHEET_MS`], and back.
///
/// `shown` is whether the sheet is up; `alive` whether it is wanted at all.
/// A sheet that is alive and not yet shown (it waits for its content to be
/// measured) holds at 0 and stays; a sheet that is not alive runs back to 0
/// and then calls `on_exit`, on the frame the clock reaches 0 (the exit is on
/// the clock, not on a timer beside it, so the two never drift apart).
/// Turning round while it runs continues from the clock's value, so a position
/// is never lost, and the leg left takes only the time it needs. With `ms` of
/// 0 (reduced motion) the clock jumps.
///
/// The clock is linear: the curve is [`openness`] of it.
pub(crate) fn use_sheet_clock(
    shown: bool,
    alive: bool,
    ms: u64,
    on_exit: impl Fn() + Clone + 'static,
) -> f32 {
    let clock = use_animation(|_| AnimNum::new(0.0, 0.0).time(0).function(Function::Linear));
    use_side_effect_with_deps(&shown, move |&shown: &bool| {
        let mut clock = clock;
        let mut state = *clock;
        let from = state.peek().value();
        let target = if shown { 1.0 } else { 0.0 };
        let run = clock_ms(ms, from, target);
        if run == 0 {
            *state.write() = AnimNum::new(target, target)
                .time(0)
                .function(Function::Linear);
        } else {
            *state.write() = AnimNum::new(from, target)
                .time(run)
                .function(Function::Linear);
            clock.start();
        }
    });
    let value = clock.get().value();
    // A sheet that mounted unwanted (a closed panel) never went up, so it has
    // nothing to exit: its first run does not count.
    let mounted = use_hook(|| Rc::new(Cell::new(false)));
    let gone = !alive && value <= 0.0;
    use_side_effect_with_deps(&gone, move |&gone: &bool| {
        if gone && mounted.replace(true) {
            on_exit();
        }
        mounted.set(true);
    });
    value
}

/// The pull a sheet shows. It follows the pointer exactly while `dragging`;
/// a dismissed sheet's pull is held where it was; and a pull let back to rest
/// springs there from where the finger let go, with its speed ([`spring_back`]).
/// `ms` is the spring's time (0 for reduced motion), `lift` how far above rest
/// the spring may go.
pub(crate) fn use_sheet_spring(
    pull: State<f32>,
    release: State<Release>,
    dragging: bool,
    ms: u64,
    lift: f32,
) -> f32 {
    let target = *pull.read();
    let now = *release.read();
    let clock = use_animation(|_| AnimNum::new(1.0, 1.0).time(0).function(Function::Linear));
    // The release the clock was last started for. A sheet that mounts after a
    // release has nothing to spring from.
    let started = use_hook(|| Rc::new(Cell::new(release.peek().id)));
    let seen = started.clone();
    use_side_effect_with_deps(&now.id, move |&id: &u64| {
        if seen.replace(id) == id {
            return;
        }
        let mut clock = clock;
        let mut state = *clock;
        if ms == 0 {
            *state.write() = AnimNum::new(1.0, 1.0).time(0).function(Function::Linear);
        } else {
            *state.write() = AnimNum::new(0.0, 1.0).time(ms).function(Function::Linear);
            clock.start();
        }
    });
    // Always read, so the sheet redraws as the clock runs. The render that
    // sees a new release is before its clock has started: it shows the start
    // of the spring, not the end of the last one.
    let running = clock.get().value();
    let u = if started.get() == now.id {
        running
    } else {
        0.0
    };
    settled_pull(target, dragging, now, u, lift)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nav::gesture::slide;

    const TRAVEL: f32 = 420.0;
    const LIFT: f32 = 15.0;

    /// A curve sampled every 1/100 of its time, and checked monotonic.
    fn samples(curve: impl Fn(f32) -> f32) -> Vec<f32> {
        (0..=100).map(|i| curve(i as f32 / 100.0)).collect()
    }

    #[test]
    fn opening_is_the_page_slide_and_ends_exactly_open() {
        let open = samples(openness);
        assert_eq!(open[0], 0.0);
        assert_eq!(open[100], 1.0);
        assert!(open.windows(2).all(|pair| pair[1] >= pair[0]), "monotonic");
        // It is the navigation token, not a curve of its own.
        for i in 0..=100 {
            let t = i as f32 / 100.0;
            assert_eq!(openness(t), slide(t));
        }
        // An ease-in-out: gets going inside four frames (64 ms of 320) and
        // has no long tail: 90 % of the way is covered with a quarter of the
        // time to go.
        assert!(openness(64.0 / SHEET_MS as f32) > 0.01);
        assert!(openness(250.0 / SHEET_MS as f32) > 0.9);
    }

    #[test]
    fn closing_is_the_same_motion_as_opening() {
        // The clock runs from 1 to 0, a sheet is where the opening put it the
        // same time before the end (the open backwards) ...
        for i in 0..=100 {
            let elapsed = i as f32 / 100.0;
            let closing = openness(1.0 - elapsed);
            // ... and has come as far down as the opening had come up at the
            // same elapsed time (the open mirrored): neither is faster.
            assert!(
                (closing - (1.0 - openness(elapsed))).abs() < 1e-6,
                "{elapsed}: {closing} vs {}",
                1.0 - openness(elapsed)
            );
        }
        // And it ends exactly shut, after exactly the opening's time.
        assert_eq!(openness(0.0), 0.0);
        assert_eq!(clock_ms(SHEET_MS, 1.0, 0.0), clock_ms(SHEET_MS, 0.0, 1.0));
        assert_eq!(clock_ms(SHEET_MS, 0.0, 1.0), SHEET_MS);
    }

    #[test]
    fn a_clock_turned_round_takes_the_time_it_has_left() {
        // Closed when 0.4 open, it has 0.4 of the time to run, from there.
        assert_eq!(clock_ms(320, 0.4, 0.0), 128);
        assert_eq!(clock_ms(320, 0.4, 1.0), 192);
        assert_eq!(clock_ms(320, 1.0, 1.0), 0);
        // Reduced motion jumps.
        assert_eq!(clock_ms(0, 0.0, 1.0), 0);
    }

    #[test]
    fn turning_round_mid_way_continues_from_the_position() {
        // Open for 100 ms of 320, then close: the clock is at 100/320 and runs
        // down from there. The position is the same on both sides of the turn.
        let t = 100.0 / 320.0;
        let before = openness(t);
        assert_eq!(openness(t), before);
        // Down a little later it is behind, never ahead, and never past shut.
        let later: Vec<f32> = (0..=10)
            .map(|i| openness(t - t * i as f32 / 10.0))
            .collect();
        assert!(later.windows(2).all(|pair| pair[1] <= pair[0]));
        assert_eq!(*later.last().unwrap(), 0.0);
    }

    #[test]
    fn a_release_continues_from_the_pull_and_stops_off_screen() {
        let curve = |u: f32| release_offset(120.0, 900.0, TRAVEL, u);
        assert_eq!(curve(0.0), 120.0, "it starts where the finger left it");
        assert_eq!(curve(1.0), TRAVEL, "it ends exactly off the window");
        let path = samples(curve);
        assert!(path.windows(2).all(|pair| pair[1] >= pair[0]), "never back");
        assert!(path.iter().all(|&y| y <= TRAVEL), "never past the end");
    }

    #[test]
    fn a_release_leaves_at_the_speed_of_the_finger() {
        // Pixels per second at the start of the curve: the finger's.
        let speed = |velocity: f32| {
            let dt = 0.0002;
            let step = release_offset(120.0, velocity, TRAVEL, dt / (SHEET_MS as f32 / 1000.0))
                - release_offset(120.0, velocity, TRAVEL, 0.0);
            step / dt
        };
        for velocity in [300.0, 900.0, 1200.0] {
            let got = speed(velocity);
            assert!(
                (got - velocity).abs() < 0.02 * velocity,
                "finger {velocity} px/s, sheet {got} px/s"
            );
        }
        // A finger at rest starts it from rest: a smooth step, no jump.
        assert!(speed(0.0).abs() < 20.0);
        // And a much faster finger is capped, not overshot.
        let fast = samples(|u| release_offset(120.0, 20_000.0, TRAVEL, u));
        assert!(fast.iter().all(|&y| y <= TRAVEL));
        assert!(fast.windows(2).all(|pair| pair[1] >= pair[0]));
    }

    #[test]
    fn a_release_eases_to_a_stop() {
        // The last tenth of the time covers a sliver of the way.
        let end =
            release_offset(120.0, 900.0, TRAVEL, 1.0) - release_offset(120.0, 900.0, TRAVEL, 0.9);
        assert!(
            end < 0.03 * (TRAVEL - 120.0),
            "the last tenth goes {end} px"
        );
    }

    #[test]
    fn a_spring_back_starts_at_the_pull_and_ends_at_rest() {
        assert_eq!(spring_back(90.0, 0.0, 0.0, LIFT), 90.0);
        assert_eq!(spring_back(90.0, 0.0, 1.0, LIFT), 0.0);
        let path = samples(|u| spring_back(90.0, 0.0, u, LIFT));
        // Down to rest: damped, so it dips under it by a hair at most and
        // settles.
        assert!(path.iter().all(|&y| y > -2.0), "{path:?}");
        assert!(path[10] < 90.0 && path[50] < path[10]);
        assert!(path[95].abs() < 0.5);
        // Eased: it leaves rest slowly (the spring starts with no speed) and
        // is fastest in the middle of the run.
        let first = path[0] - path[5];
        let middle = path[20] - path[25];
        assert!(middle > 2.0 * first, "{first} vs {middle}");
    }

    #[test]
    fn a_spring_back_carries_the_speed_of_the_finger() {
        // Still moving up: it leaves at that speed.
        let dt = 0.0001;
        let u = dt / (SHEET_SPRING_MS as f32 / 1000.0);
        for velocity in [-800.0, 600.0] {
            let step =
                spring_back(90.0, velocity, u, LIFT) - spring_back(90.0, velocity, 0.0, LIFT);
            let got = step / dt;
            assert!(
                (got - velocity).abs() < 0.05 * velocity.abs(),
                "finger {velocity} px/s, sheet {got} px/s"
            );
        }
        // Moving down it goes a little further down before it turns.
        assert!(spring_back(90.0, 600.0, 0.1, LIFT) > spring_back(90.0, 0.0, 0.1, LIFT));
        // A flick up never lifts it past the limit.
        let up = samples(|u| spring_back(5.0, -5000.0, u, LIFT));
        assert!(up.iter().all(|&y| y >= -LIFT), "{up:?}");
    }

    #[test]
    fn release_ids_count_and_never_return_to_none() {
        let first = Release::default().next(10.0, 0.0);
        assert_eq!(first.id, 1);
        assert_eq!(first.next(0.0, 0.0).id, 2);
        assert_eq!(
            Release {
                id: u64::MAX,
                ..first
            }
            .next(0.0, 0.0)
            .id,
            1
        );
    }

    #[test]
    fn the_pull_follows_the_pointer_holds_a_dismiss_and_springs_to_rest() {
        let release = Release::default().next(90.0, 0.0);
        // The pointer's, exactly, while dragging.
        assert_eq!(settled_pull(70.0, true, release, 0.5, LIFT), 70.0);
        // A dismissed sheet's stays where it was.
        assert_eq!(settled_pull(90.0, false, release, 0.5, LIFT), 90.0);
        // A sheet back at rest: the spring, from where the finger let go.
        assert_eq!(settled_pull(0.0, false, release, 0.0, LIFT), 90.0);
        assert_eq!(settled_pull(0.0, false, release, 1.0, LIFT), 0.0);
        // A sheet never released is at rest.
        assert_eq!(settled_pull(0.0, false, Release::default(), 0.0, LIFT), 0.0);
    }
}
