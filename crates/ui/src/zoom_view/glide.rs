//! Inertia: what happens to content that a finger or pointer let go of while
//! it was moving. Pure arithmetic over time, so the feel is tested.

use std::collections::VecDeque;

/// How long the glide takes to lose a factor of `e` of its speed, in seconds.
pub const TIME_CONSTANT: f32 = 0.32;
/// Below this many pixels a second the glide is over.
pub const REST_SPEED: f32 = 30.0;
/// The window of recent movement that sets the speed at release, in seconds.
const WINDOW: f32 = 0.09;
/// A pointer that was still for this long before release does not glide.
const STILL: f32 = 0.06;

/// The last few positions of a drag, with the time of each in seconds.
#[derive(Clone, Debug, Default)]
pub struct Samples(VecDeque<(f32, (f32, f32))>);

impl Samples {
    pub fn push(&mut self, at: f32, position: (f32, f32)) {
        self.0.push_back((at, position));
        while self
            .0
            .front()
            .is_some_and(|(then, _)| at - then > WINDOW * 2.0)
        {
            self.0.pop_front();
        }
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    /// The speed at `now`, in pixels a second, from the movement inside the
    /// window; zero when the pointer had stopped before it was released.
    pub fn velocity(&self, now: f32) -> (f32, f32) {
        let Some(&(last_at, last)) = self.0.back() else {
            return (0.0, 0.0);
        };
        if now - last_at > STILL {
            return (0.0, 0.0);
        }
        let Some(&(first_at, first)) = self.0.iter().find(|(then, _)| last_at - then <= WINDOW)
        else {
            return (0.0, 0.0);
        };
        let span = last_at - first_at;
        if span < 0.01 {
            return (0.0, 0.0);
        }
        ((last.0 - first.0) / span, (last.1 - first.1) / span)
    }
}

/// How far a glide at `velocity` moves in `dt` seconds, and its speed after:
/// the exact integral of an exponential decay, so a slow frame and two fast
/// ones end in the same place.
pub fn step(velocity: (f32, f32), dt: f32) -> ((f32, f32), (f32, f32)) {
    let decay = (-dt / TIME_CONSTANT).exp();
    let reach = TIME_CONSTANT * (1.0 - decay);
    (
        (velocity.0 * reach, velocity.1 * reach),
        (velocity.0 * decay, velocity.1 * decay),
    )
}

/// Whether a glide at `velocity` has come to rest.
pub fn is_resting(velocity: (f32, f32)) -> bool {
    velocity.0.hypot(velocity.1) < REST_SPEED
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drag_released_while_moving_has_the_speed_of_the_last_moments() {
        let mut samples = Samples::default();
        for n in 0..10 {
            let t = n as f32 * 0.01;
            samples.push(t, (n as f32 * 5.0, 0.0));
        }
        let (x, y) = samples.velocity(0.095);
        assert!((x - 500.0).abs() < 1.0 && y == 0.0, "{x} {y}");
    }

    #[test]
    fn a_pointer_that_stopped_before_it_was_released_does_not_glide() {
        let mut samples = Samples::default();
        samples.push(0.00, (0.0, 0.0));
        samples.push(0.05, (50.0, 0.0));
        assert_eq!(samples.velocity(0.5), (0.0, 0.0));
        assert_eq!(Samples::default().velocity(1.0), (0.0, 0.0));
    }

    #[test]
    fn the_glide_covers_speed_times_the_time_constant_however_it_is_stepped() {
        let total = |frames: u32| {
            let dt = 5.0 / frames as f32;
            let mut v = (1000.0, 0.0);
            let mut moved = 0.0;
            for _ in 0..frames {
                let (by, next) = step(v, dt);
                moved += by.0;
                v = next;
            }
            moved
        };
        let expected = 1000.0 * TIME_CONSTANT;
        assert!((total(60) - expected).abs() < 0.5, "{}", total(60));
        assert!((total(300) - expected).abs() < 0.5, "{}", total(300));
    }

    #[test]
    fn a_glide_comes_to_rest() {
        assert!(!is_resting((400.0, 0.0)));
        assert!(is_resting((10.0, 10.0)));
        let (_, v) = step((400.0, 0.0), 2.0);
        assert!(is_resting(v));
    }
}
