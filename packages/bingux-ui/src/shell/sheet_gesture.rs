//! The gesture of a bottom sheet, as a pure state machine.
//!
//! A sheet is pulled by its grabber, or by a finger dragging its list down
//! while the list is scrolled to the top. This module decides what each pointer
//! event means: who owns the movement (the sheet or the list inside it), how
//! far the sheet is pulled, and what a release does. It knows nothing of the UI
//! toolkit: positions are logical pixels, time is seconds on a clock the
//! caller supplies, and nothing here reads a real clock, so a test drives a
//! whole drag with a fake one.
//!
//! The model:
//!
//! - `pull` is how far the sheet sits below its rest position. It follows the
//!   pointer exactly while the sheet is dragged, so the sheet moves as a whole
//!   and nothing inside it reflows. Above rest the pull is rubber-banded: it
//!   gives a little and never passes [`Metrics::lift_limit`], which is kept
//!   inside the part of the sheet that hangs below the window edge.
//! - A release settles on one of two snap points: rest, or dismissed. It
//!   dismisses when the pull *projected* a short moment ahead along the
//!   release velocity passes [`Metrics::dismiss_at`], so a fast flick dismisses
//!   from a short drag and an upward flick rests from a long one.
//! - A press in the list is undecided until it moves. Past the slop, a move
//!   down with the list at its top hands the gesture to the sheet; any other
//!   move is the list's. A list that scrolled to its top during the same
//!   gesture hands over the same way, and the sheet hands back to the list
//!   when the finger returns to where the pull began.

/// Seconds on a monotonic clock the caller supplies.
pub(crate) type Seconds = f64;

/// A downward speed, in logical pixels per second, that counts as a fling.
pub(crate) const FLING_SPEED: f32 = 700.0;
/// How far ahead, in seconds, a release projects the pull along its velocity.
pub(crate) const PROJECTION: f32 = 0.12;
/// The newest samples that estimate a release velocity span this many seconds.
pub(crate) const VELOCITY_WINDOW: Seconds = 0.045;
/// A pointer that has not moved for this long has no velocity on release.
pub(crate) const STALE: Seconds = 0.08;
/// How far a press in the list moves before it is a drag, in logical pixels.
pub(crate) const DRAG_SLOP: f32 = 8.0;

const SAMPLES: usize = 12;

/// The sheet's geometry, which a release and a rubber band depend on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Metrics {
    /// The distance that carries the sheet fully off the window.
    pub travel: f32,
    /// The projected pull that dismisses on release.
    pub dismiss_at: f32,
    /// The most the sheet lifts above rest, however far the pointer goes.
    pub lift_limit: f32,
}

impl Metrics {
    /// The metrics of a sheet whose visible part is `height` tall, with
    /// `overhang` of it hanging below the window edge. It dismisses at
    /// `distance`, or half its height when that is less.
    pub(crate) fn new(height: f32, overhang: f32, distance: f32) -> Self {
        Self {
            travel: height + overhang,
            dismiss_at: distance.min(height * 0.5).max(1.0),
            lift_limit: overhang * 0.75,
        }
    }
}

/// Who moves for a pointer event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Claim {
    /// The sheet follows the pointer: the list must not scroll.
    Sheet,
    /// The list scrolls, or nothing moves.
    Content,
}

/// What a release or a cancel did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// The gesture never moved the sheet: a tap, or a scroll of the list.
    None,
    /// The sheet springs back to rest from where it is.
    Rest,
    /// The sheet leaves.
    Dismiss,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    Idle,
    /// A press in the list that has not moved far enough to decide.
    Undecided { origin: f32 },
    /// The list scrolls under the pointer.
    Scrolling { last: f32 },
    /// The sheet follows the pointer. `anchor` is the pointer position at
    /// which the pull is zero.
    Dragging { anchor: f32, from_content: bool },
}

/// The state of one sheet's gesture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SheetGesture {
    phase: Phase,
    pull: f32,
    samples: [(Seconds, f32); SAMPLES],
    len: usize,
}

impl Default for SheetGesture {
    fn default() -> Self {
        Self {
            phase: Phase::Idle,
            pull: 0.0,
            samples: [(0.0, 0.0); SAMPLES],
            len: 0,
        }
    }
}

/// The pull for a pointer `raw` pixels from its anchor: one to one downward,
/// and rubber-banded upward so it approaches `limit` and never passes it.
pub(crate) fn rubber_band(raw: f32, limit: f32) -> f32 {
    if raw >= 0.0 || limit <= 0.0 {
        return raw.max(0.0);
    }
    let reach = -raw;
    -(limit * reach / (reach + limit))
}

/// Where a release at `pull` with `velocity` (pixels per second, downward
/// positive) settles.
pub(crate) fn snap(pull: f32, velocity: f32, metrics: &Metrics) -> Outcome {
    if pull + velocity * PROJECTION >= metrics.dismiss_at && pull > 0.0 {
        Outcome::Dismiss
    } else {
        Outcome::Rest
    }
}

impl SheetGesture {
    /// How far the sheet sits below rest. Negative while it is lifted.
    pub(crate) fn pull(&self) -> f32 {
        self.pull
    }

    /// Whether the sheet follows the pointer right now.
    pub(crate) fn is_dragging(&self) -> bool {
        matches!(self.phase, Phase::Dragging { .. })
    }

    /// Whether a pointer is down on the sheet or its list at all.
    pub(crate) fn is_active(&self) -> bool {
        self.phase != Phase::Idle
    }

    /// Forgets the pull, for a sheet that has left and may open again.
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    fn record(&mut self, y: f32, now: Seconds) {
        if self.len == SAMPLES {
            self.samples.copy_within(1.., 0);
            self.len -= 1;
        }
        self.samples[self.len] = (now, y);
        self.len += 1;
    }

    /// The pointer's speed at `now`, in pixels per second, downward positive,
    /// over the newest samples. A pointer that stopped has none.
    pub(crate) fn velocity(&self, now: Seconds) -> f32 {
        let samples = &self.samples[..self.len];
        let Some(&(last_at, last_y)) = samples.last() else {
            return 0.0;
        };
        if samples.len() < 2 || now - last_at > STALE {
            return 0.0;
        }
        // Every sample inside the window; a slow device that delivers fewer
        // than two in it uses the sample before the last instead.
        let mut first = samples.len() - 1;
        while first > 0 && last_at - samples[first - 1].0 <= VELOCITY_WINDOW {
            first -= 1;
        }
        if first == samples.len() - 1 {
            first -= 1;
        }
        let (first_at, first_y) = samples[first];
        let span = last_at - first_at;
        if span <= 0.0 || span > 4.0 * VELOCITY_WINDOW {
            return 0.0;
        }
        ((f64::from(last_y) - f64::from(first_y)) / span) as f32
    }

    /// The pointer went down on the grabber at `y`. The sheet owns the
    /// gesture. Catching a sheet that is still settling keeps its pull, so it
    /// does not jump under the finger.
    pub(crate) fn grab(&mut self, y: f32, now: Seconds) {
        self.len = 0;
        self.record(y, now);
        self.phase = Phase::Dragging {
            anchor: y - self.pull.max(0.0),
            from_content: false,
        };
    }

    /// The pointer went down in the list at `y`.
    pub(crate) fn press_content(&mut self, y: f32, now: Seconds) {
        self.len = 0;
        self.record(y, now);
        self.phase = Phase::Undecided { origin: y };
    }

    /// The pointer moved to `y`. `at_top` is whether the list is scrolled to
    /// its top, as of now. Returns who moves for this event.
    pub(crate) fn move_to(&mut self, y: f32, now: Seconds, at_top: bool, metrics: &Metrics) -> Claim {
        match self.phase {
            Phase::Idle => Claim::Content,
            Phase::Undecided { origin } => {
                self.record(y, now);
                let distance = y - origin;
                if distance.abs() < DRAG_SLOP {
                    return Claim::Content;
                }
                if at_top && distance > 0.0 {
                    self.take_over(y);
                    Claim::Sheet
                } else {
                    self.phase = Phase::Scrolling { last: y };
                    Claim::Content
                }
            }
            Phase::Scrolling { last } => {
                self.record(y, now);
                if at_top && y > last {
                    self.take_over(y);
                    return Claim::Sheet;
                }
                self.phase = Phase::Scrolling { last: y };
                Claim::Content
            }
            Phase::Dragging {
                anchor,
                from_content,
            } => {
                self.record(y, now);
                let raw = y - anchor;
                if from_content && raw <= 0.0 {
                    // The finger is back where the pull began: the list's.
                    self.pull = 0.0;
                    self.phase = Phase::Scrolling { last: y };
                    self.len = 0;
                    self.record(y, now);
                    return Claim::Content;
                }
                self.pull = rubber_band(raw, metrics.lift_limit);
                Claim::Sheet
            }
        }
    }

    fn take_over(&mut self, y: f32) {
        self.pull = 0.0;
        self.phase = Phase::Dragging {
            anchor: y,
            from_content: true,
        };
    }

    /// The pointer went up at `now`. A sheet that was dragged settles by
    /// [`snap`]; a press that never moved it is [`Outcome::None`].
    pub(crate) fn release(&mut self, now: Seconds, metrics: &Metrics) -> Outcome {
        let dragged = self.is_dragging();
        let outcome = if dragged {
            snap(self.pull, self.velocity(now), metrics)
        } else {
            Outcome::None
        };
        self.phase = Phase::Idle;
        self.len = 0;
        outcome
    }

    /// The pointer was cancelled (a finger the system took over): a dragged
    /// sheet springs back, whatever its pull.
    pub(crate) fn cancel(&mut self) -> Outcome {
        let dragged = self.is_dragging();
        self.phase = Phase::Idle;
        self.len = 0;
        if dragged { Outcome::Rest } else { Outcome::None }
    }

    /// The pull a settling sheet has been eased to, so a caller that animates
    /// the settle can keep the gesture's pull in step. A dismissed sheet keeps
    /// its pull until [`SheetGesture::reset`].
    pub(crate) fn settle_to(&mut self, pull: f32) {
        self.pull = pull;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fake clock, so a test drives a drag at exact times.
    struct Clock(Seconds);

    impl Clock {
        fn advance(&mut self, ms: u32) -> Seconds {
            self.0 += f64::from(ms) / 1000.0;
            self.0
        }
    }

    fn metrics() -> Metrics {
        // A 300 px sheet with a 20 px overhang that dismisses at 96 px.
        Metrics::new(300.0, 20.0, 96.0)
    }

    /// Drags the grabber from `from` to `to` in `steps` equal moves, `ms` apart.
    fn drag(
        gesture: &mut SheetGesture,
        clock: &mut Clock,
        from: f32,
        to: f32,
        steps: u32,
        ms: u32,
    ) {
        gesture.grab(from, clock.0);
        for step in 1..=steps {
            let y = from + (to - from) * step as f32 / steps as f32;
            gesture.move_to(y, clock.advance(ms), true, &metrics());
        }
    }

    #[test]
    fn a_grabber_drag_follows_the_pointer_one_to_one() {
        let mut gesture = SheetGesture::default();
        let mut clock = Clock(0.0);
        drag(&mut gesture, &mut clock, 500.0, 560.0, 6, 16);
        assert_eq!(gesture.pull(), 60.0);
        assert!(gesture.is_dragging());
    }

    #[test]
    fn dragging_up_is_rubber_banded_and_never_passes_the_limit() {
        let mut gesture = SheetGesture::default();
        let mut clock = Clock(0.0);
        let limit = metrics().lift_limit;
        drag(&mut gesture, &mut clock, 500.0, 100.0, 40, 16);
        assert!(gesture.pull() < 0.0, "the sheet lifts a little");
        assert!(gesture.pull() > -limit, "and never reaches the limit");
        // The band is monotone: a longer pull never lifts less.
        let mut last = 0.0;
        for raw in 1..400 {
            let lifted = rubber_band(-(raw as f32), limit);
            assert!(lifted <= last + 1e-4);
            last = lifted;
        }
        // The first pixels move nearly one to one.
        assert!((rubber_band(-1.0, limit) + 1.0).abs() < 0.1);
    }

    #[test]
    fn the_lift_stays_inside_the_overhang() {
        let m = metrics();
        assert!(m.lift_limit < 20.0, "the bottom corners never show");
    }

    #[test]
    fn a_slow_release_short_of_the_threshold_rests() {
        let mut gesture = SheetGesture::default();
        let mut clock = Clock(0.0);
        drag(&mut gesture, &mut clock, 500.0, 560.0, 6, 16);
        // The pointer waits a while: no velocity.
        let at = clock.advance(200);
        assert_eq!(gesture.release(at, &metrics()), Outcome::Rest);
        assert!(!gesture.is_active());
    }

    #[test]
    fn a_slow_release_past_the_threshold_dismisses() {
        let mut gesture = SheetGesture::default();
        let mut clock = Clock(0.0);
        drag(&mut gesture, &mut clock, 500.0, 620.0, 12, 16);
        let at = clock.advance(300);
        assert_eq!(gesture.release(at, &metrics()), Outcome::Dismiss);
    }

    #[test]
    fn a_fast_downward_flick_dismisses_from_a_short_drag() {
        let mut gesture = SheetGesture::default();
        let mut clock = Clock(0.0);
        // 40 px in 32 ms is 1250 px/s.
        drag(&mut gesture, &mut clock, 500.0, 540.0, 2, 16);
        let at = clock.0 + 0.004;
        assert!(gesture.velocity(at) > FLING_SPEED);
        assert_eq!(gesture.release(at, &metrics()), Outcome::Dismiss);
    }

    #[test]
    fn an_upward_flick_rests_even_past_the_threshold() {
        let mut gesture = SheetGesture::default();
        let mut clock = Clock(0.0);
        drag(&mut gesture, &mut clock, 500.0, 640.0, 14, 16);
        // Reverse quickly: 40 px up in 32 ms.
        gesture.move_to(620.0, clock.advance(16), true, &metrics());
        gesture.move_to(600.0, clock.advance(16), true, &metrics());
        assert!(gesture.pull() > metrics().dismiss_at);
        let at = clock.0 + 0.004;
        assert!(gesture.velocity(at) < -FLING_SPEED);
        assert_eq!(gesture.release(at, &metrics()), Outcome::Rest);
    }

    #[test]
    fn a_lifted_sheet_rests_on_release() {
        let mut gesture = SheetGesture::default();
        let mut clock = Clock(0.0);
        drag(&mut gesture, &mut clock, 500.0, 400.0, 10, 16);
        assert!(gesture.pull() < 0.0);
        let at = clock.0 + 0.002;
        assert_eq!(gesture.release(at, &metrics()), Outcome::Rest);
    }

    #[test]
    fn a_stopped_pointer_has_no_velocity() {
        let mut gesture = SheetGesture::default();
        let mut clock = Clock(0.0);
        drag(&mut gesture, &mut clock, 500.0, 580.0, 4, 16);
        assert!(gesture.velocity(clock.0 + 0.001) > 0.0);
        assert_eq!(gesture.velocity(clock.0 + 0.5), 0.0);
    }

    #[test]
    fn the_threshold_is_at_most_half_a_short_sheet() {
        let m = Metrics::new(120.0, 20.0, 96.0);
        assert_eq!(m.dismiss_at, 60.0);
        let tall = Metrics::new(600.0, 20.0, 96.0);
        assert_eq!(tall.dismiss_at, 96.0);
    }

    #[test]
    fn catching_a_settling_sheet_keeps_its_pull() {
        let mut gesture = SheetGesture::default();
        let mut clock = Clock(0.0);
        drag(&mut gesture, &mut clock, 500.0, 560.0, 6, 16);
        let at = clock.advance(100);
        assert_eq!(gesture.release(at, &metrics()), Outcome::Rest);
        // The spring is part-way back when the finger lands again.
        gesture.settle_to(25.0);
        gesture.grab(530.0, clock.advance(50));
        gesture.move_to(540.0, clock.advance(16), true, &metrics());
        assert_eq!(gesture.pull(), 35.0);
    }

    // The list inside the sheet.

    #[test]
    fn a_press_in_the_list_is_undecided_inside_the_slop() {
        let mut gesture = SheetGesture::default();
        let mut clock = Clock(0.0);
        gesture.press_content(400.0, clock.0);
        let claim = gesture.move_to(403.0, clock.advance(16), true, &metrics());
        assert_eq!(claim, Claim::Content);
        assert!(!gesture.is_dragging());
        assert_eq!(gesture.release(clock.advance(16), &metrics()), Outcome::None);
    }

    #[test]
    fn a_list_scrolled_to_its_top_hands_a_downward_drag_to_the_sheet() {
        let mut gesture = SheetGesture::default();
        let mut clock = Clock(0.0);
        gesture.press_content(400.0, clock.0);
        let first = gesture.move_to(412.0, clock.advance(16), true, &metrics());
        assert_eq!(first, Claim::Sheet);
        // The pull starts at zero where the sheet took over: no jump.
        assert_eq!(gesture.pull(), 0.0);
        let next = gesture.move_to(452.0, clock.advance(16), true, &metrics());
        assert_eq!(next, Claim::Sheet);
        assert_eq!(gesture.pull(), 40.0);
    }

    #[test]
    fn a_list_not_at_its_top_keeps_a_downward_drag() {
        let mut gesture = SheetGesture::default();
        let mut clock = Clock(0.0);
        gesture.press_content(400.0, clock.0);
        for step in 1..=10 {
            let claim = gesture.move_to(400.0 + 12.0 * step as f32, clock.advance(16), false, &metrics());
            assert_eq!(claim, Claim::Content);
        }
        assert_eq!(gesture.pull(), 0.0);
        assert_eq!(gesture.release(clock.advance(16), &metrics()), Outcome::None);
    }

    #[test]
    fn an_upward_drag_in_the_list_is_the_lists_even_at_the_top() {
        let mut gesture = SheetGesture::default();
        let mut clock = Clock(0.0);
        gesture.press_content(400.0, clock.0);
        assert_eq!(
            gesture.move_to(380.0, clock.advance(16), true, &metrics()),
            Claim::Content
        );
        // Once the list owns the gesture, a later move down while the list
        // is still short of its top stays the list's.
        assert_eq!(
            gesture.move_to(395.0, clock.advance(16), false, &metrics()),
            Claim::Content
        );
    }

    #[test]
    fn a_list_that_scrolls_to_its_top_mid_gesture_hands_over_going_down() {
        let mut gesture = SheetGesture::default();
        let mut clock = Clock(0.0);
        gesture.press_content(400.0, clock.0);
        // The finger drags the list down, still short of the top.
        gesture.move_to(430.0, clock.advance(16), false, &metrics());
        gesture.move_to(460.0, clock.advance(16), false, &metrics());
        // The list reaches its top and the finger keeps going down.
        let claim = gesture.move_to(480.0, clock.advance(16), true, &metrics());
        assert_eq!(claim, Claim::Sheet);
        assert_eq!(gesture.pull(), 0.0);
        gesture.move_to(520.0, clock.advance(16), true, &metrics());
        assert_eq!(gesture.pull(), 40.0);
    }

    #[test]
    fn the_sheet_hands_back_when_the_finger_returns_to_where_the_pull_began() {
        let mut gesture = SheetGesture::default();
        let mut clock = Clock(0.0);
        gesture.press_content(400.0, clock.0);
        gesture.move_to(412.0, clock.advance(16), true, &metrics());
        gesture.move_to(472.0, clock.advance(16), true, &metrics());
        assert_eq!(gesture.pull(), 60.0);
        gesture.move_to(430.0, clock.advance(16), true, &metrics());
        assert_eq!(gesture.pull(), 18.0);
        // Back past the start: the list's again, and the sheet is at rest.
        let claim = gesture.move_to(400.0, clock.advance(16), true, &metrics());
        assert_eq!(claim, Claim::Content);
        assert_eq!(gesture.pull(), 0.0);
        assert!(!gesture.is_dragging());
        // The list is then scrolled up, away from its top.
        assert_eq!(
            gesture.move_to(380.0, clock.advance(16), false, &metrics()),
            Claim::Content
        );
    }

    #[test]
    fn a_content_drag_past_the_threshold_dismisses_on_release() {
        let mut gesture = SheetGesture::default();
        let mut clock = Clock(0.0);
        gesture.press_content(400.0, clock.0);
        gesture.move_to(412.0, clock.advance(16), true, &metrics());
        for step in 1..=12 {
            gesture.move_to(412.0 + 10.0 * step as f32, clock.advance(16), true, &metrics());
        }
        let at = clock.advance(250);
        assert_eq!(gesture.release(at, &metrics()), Outcome::Dismiss);
    }

    #[test]
    fn a_cancelled_drag_springs_back_and_a_cancelled_tap_does_nothing() {
        let mut gesture = SheetGesture::default();
        let mut clock = Clock(0.0);
        drag(&mut gesture, &mut clock, 500.0, 600.0, 10, 16);
        assert_eq!(gesture.cancel(), Outcome::Rest);
        assert!(!gesture.is_active());
        gesture.press_content(400.0, clock.advance(16));
        assert_eq!(gesture.cancel(), Outcome::None);
    }

    #[test]
    fn reset_forgets_a_dismissed_pull() {
        let mut gesture = SheetGesture::default();
        let mut clock = Clock(0.0);
        drag(&mut gesture, &mut clock, 500.0, 640.0, 14, 16);
        let at = clock.advance(250);
        assert_eq!(gesture.release(at, &metrics()), Outcome::Dismiss);
        assert!(gesture.pull() > 0.0, "a dismissed sheet keeps its pull while it leaves");
        gesture.reset();
        assert_eq!(gesture.pull(), 0.0);
    }

    #[test]
    fn events_with_no_press_are_ignored() {
        let mut gesture = SheetGesture::default();
        let claim = gesture.move_to(300.0, 1.0, true, &metrics());
        assert_eq!(claim, Claim::Content);
        assert_eq!(gesture.release(1.1, &metrics()), Outcome::None);
        assert_eq!(gesture.pull(), 0.0);
    }
}
