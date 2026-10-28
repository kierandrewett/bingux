//! Predictive back: the gesture state machine and the pose it drives.
//!
//! Pure logic with an injected clock (a [`Duration`] since any fixed start), so
//! the progress mapping, the commit decision, the spring back and the
//! interruption rules are tested without a window. The view feeds it pointer
//! events and asks it, once per frame, how far along the drag or settle is.
//!
//! Lifecycle: `Idle -> Tracking` (a press in the edge zone) `-> Dragging`
//! (enough horizontal travel) `-> Settling` (released: commit or cancel)
//! `-> Idle`. A vertical start hands the touch to the scroll view
//! (`Aborted`). Pressing the edge again while a cancel is settling continues
//! from the pose on screen.

use std::time::Duration;

/// The tunable numbers, in logical pixels and seconds. See
/// `design/navigation.md`, section 4.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GestureConfig {
    /// A press starts the gesture only within this many pixels of the left edge.
    pub edge: f32,
    /// Travel before the gesture is decided.
    pub slop: f32,
    /// A release past this share of the width commits.
    pub commit_progress: f32,
    /// A rightward fling at least this fast (px/s) commits.
    pub commit_velocity: f32,
    /// A leftward fling at least this fast (px/s) cancels, even far along.
    pub cancel_velocity: f32,
    /// Moving this far back past where the touch began cancels.
    pub cancel_back: f32,
    pub commit_ms: u64,
    pub cancel_ms: u64,
    /// How far back the velocity is measured.
    pub velocity_window: Duration,
}

impl Default for GestureConfig {
    fn default() -> Self {
        Self {
            edge: 20.0,
            slop: 8.0,
            commit_progress: 0.35,
            commit_velocity: 700.0,
            cancel_velocity: 500.0,
            cancel_back: 4.0,
            commit_ms: 260,
            cancel_ms: 240,
            velocity_window: Duration::from_millis(100),
        }
    }
}

/// Where the gesture is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    Idle,
    /// Pressed in the edge zone; not yet decided.
    Tracking,
    /// Decided: the drag drives the screen.
    Dragging,
    /// Released. `commit` says which way it settles.
    Settling {
        commit: bool,
        from: f32,
        started: Duration,
    },
    /// Handed to a scroll view; ignored until the touch ends.
    Aborted,
}

/// What a move did.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Update {
    /// Nothing visible.
    None,
    /// The drag was just decided: build the previous screen now.
    Started,
    Progress(f32),
    /// The touch went back past where it began: the gesture settles back.
    Cancelled,
    /// Vertical travel won: the scroll view keeps the touch.
    Aborted,
}

/// What a release did.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Release {
    /// Pop, settling out from `from`.
    Commit { from: f32 },
    /// Spring back from `from`.
    Cancel { from: f32 },
    /// The touch never became a drag: it is an ordinary press.
    Tap,
    /// There was no gesture.
    Ignored,
}

/// The gesture state machine.
#[derive(Clone, Debug)]
pub struct BackGesture {
    pub config: GestureConfig,
    phase: Phase,
    width: f32,
    origin: (f32, f32),
    /// The progress the drag started from: non-zero when it grabbed a cancel
    /// that was still settling.
    base: f32,
    progress: f32,
    samples: Vec<(Duration, f32)>,
}

impl Default for BackGesture {
    fn default() -> Self {
        Self::new(GestureConfig::default())
    }
}

impl BackGesture {
    pub fn new(config: GestureConfig) -> Self {
        Self {
            config,
            phase: Phase::Idle,
            width: 1.0,
            origin: (0.0, 0.0),
            base: 0.0,
            progress: 0.0,
            samples: Vec::new(),
        }
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// Whether the gesture owns the screen: dragging or settling.
    pub fn is_active(&self) -> bool {
        matches!(self.phase, Phase::Dragging | Phase::Settling { .. })
    }

    /// Whether a touch is being decided or followed.
    pub fn is_pressed(&self) -> bool {
        matches!(self.phase, Phase::Tracking | Phase::Dragging)
    }

    /// A press at `pos` in a window `width` wide. `allowed` says a previous
    /// entry exists and the window takes edge swipes. Returns whether the
    /// press is now tracked.
    pub fn down(&mut self, now: Duration, pos: (f32, f32), width: f32, allowed: bool) -> bool {
        if !allowed || width <= 0.0 || pos.0 > self.config.edge || pos.0 < 0.0 {
            return false;
        }
        if self.is_pressed() {
            return false;
        }
        // Grabbing a cancel that is still settling continues from its pose.
        let base = match self.phase {
            Phase::Settling { commit: false, .. } => self.value(now),
            _ => 0.0,
        };
        if matches!(self.phase, Phase::Settling { commit: true, .. }) {
            // The pop already happened; the new press is about the new page.
            self.phase = Phase::Idle;
        }
        self.phase = Phase::Tracking;
        self.width = width;
        self.origin = pos;
        self.base = base;
        self.progress = base;
        self.samples.clear();
        self.samples.push((now, pos.0));
        true
    }

    /// The pointer moved to `pos`.
    pub fn moved(&mut self, now: Duration, pos: (f32, f32)) -> Update {
        let dx = pos.0 - self.origin.0;
        let dy = pos.1 - self.origin.1;
        match self.phase {
            Phase::Tracking => {
                if dx.abs() < self.config.slop && dy.abs() < self.config.slop {
                    return Update::None;
                }
                // Vertical travel first, or a leftward start: not a back swipe.
                if dy.abs() > dx.abs() || dx < 0.0 {
                    self.phase = Phase::Aborted;
                    return Update::Aborted;
                }
                self.phase = Phase::Dragging;
                self.sample(now, pos.0);
                self.progress = (self.base + dx / self.width).clamp(0.0, 1.0);
                Update::Started
            }
            Phase::Dragging => {
                self.sample(now, pos.0);
                let raw = self.base + dx / self.width;
                if raw < -self.config.cancel_back / self.width {
                    self.phase = Phase::Settling {
                        commit: false,
                        from: self.progress,
                        started: now,
                    };
                    return Update::Cancelled;
                }
                self.progress = raw.clamp(0.0, 1.0);
                Update::Progress(self.progress)
            }
            _ => Update::None,
        }
    }

    fn sample(&mut self, now: Duration, x: f32) {
        self.samples.push((now, x));
        let window = self.config.velocity_window;
        while self.samples.len() > 2 && now.saturating_sub(self.samples[0].0) > window {
            self.samples.remove(0);
        }
    }

    /// The rightward speed in px/s over the velocity window; negative when the
    /// touch is moving back.
    pub fn velocity(&self) -> f32 {
        let (Some(first), Some(last)) = (self.samples.first(), self.samples.last()) else {
            return 0.0;
        };
        let dt = last.0.saturating_sub(first.0).as_secs_f32();
        if dt < 0.008 {
            return 0.0;
        }
        (last.1 - first.1) / dt
    }

    /// The touch ended.
    pub fn up(&mut self, now: Duration) -> Release {
        match self.phase {
            Phase::Tracking => {
                self.phase = Phase::Idle;
                Release::Tap
            }
            Phase::Aborted => {
                self.phase = Phase::Idle;
                Release::Ignored
            }
            Phase::Dragging => {
                // The speed at the lift: a touch that rested before it has none.
                let x = self.samples.last().map_or(0.0, |sample| sample.1);
                self.sample(now, x);
                let velocity = self.velocity();
                let progress = self.progress;
                let cfg = self.config;
                let commit = velocity > -cfg.cancel_velocity
                    && (progress >= cfg.commit_progress
                        || (velocity >= cfg.commit_velocity && progress > 0.03));
                self.phase = Phase::Settling {
                    commit,
                    from: progress,
                    started: now,
                };
                if commit {
                    Release::Commit { from: progress }
                } else {
                    Release::Cancel { from: progress }
                }
            }
            _ => Release::Ignored,
        }
    }

    /// Drop the gesture at once, as a lost pointer or a closed window does.
    pub fn abort(&mut self) {
        self.phase = Phase::Idle;
        self.progress = 0.0;
    }

    /// The drag or settle progress, 0 to 1, at `now`.
    pub fn value(&self, now: Duration) -> f32 {
        match self.phase {
            Phase::Dragging => self.progress,
            Phase::Settling {
                commit,
                from,
                started,
            } => {
                let t = now.saturating_sub(started).as_secs_f32();
                if commit {
                    let total = self.config.commit_ms as f32 / 1000.0;
                    from + (1.0 - from) * expo_out(t / total)
                } else {
                    let total = self.config.cancel_ms as f32 / 1000.0;
                    (from * (1.0 - spring(t / total))).max(0.0)
                }
            }
            Phase::Tracking => self.base,
            _ => 0.0,
        }
    }

    /// Whether a settle has finished at `now`; if so the gesture is idle
    /// again and `Some(committed)` says which way it went.
    pub fn tick(&mut self, now: Duration) -> Option<bool> {
        let Phase::Settling {
            commit, started, ..
        } = self.phase
        else {
            return None;
        };
        let total = if commit {
            self.config.commit_ms
        } else {
            self.config.cancel_ms
        };
        if now.saturating_sub(started) >= Duration::from_millis(total) {
            self.phase = Phase::Idle;
            self.progress = 0.0;
            return Some(commit);
        }
        None
    }

    /// Finish a settle at once, for reduced motion.
    pub fn finish(&mut self) -> Option<bool> {
        self.tick(Duration::MAX)
    }
}

/// `1 - 2^(-10 t)`, reaching 1 at `t == 1`.
pub fn expo_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t >= 1.0 {
        1.0
    } else {
        // Scaled so the curve meets 1 exactly at t = 1.
        let raw = 1.0 - 2f32.powf(-10.0 * t);
        raw / (1.0 - 2f32.powf(-10.0))
    }
}

/// A slightly underdamped spring step response from 0 to 1 over `t` in 0..1
/// of the settle time. It overshoots a little and ends at 1.
pub fn spring(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t >= 1.0 {
        return 1.0;
    }
    let zeta = 0.8_f32;
    let omega = 8.0_f32; // per unit of t: e^(-0.8 * 8) leaves under 0.2 % at the end
    let damped = omega * (1.0 - zeta * zeta).sqrt();
    let decay = (-zeta * omega * t).exp();
    1.0 - decay * ((damped * t).cos() + zeta / (1.0 - zeta * zeta).sqrt() * (damped * t).sin())
}

fn smoothstep(from: f32, to: f32, x: f32) -> f32 {
    let t = ((x - from) / (to - from)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How the two screens look at a drag progress `d` and commit progress `k`.
/// Android 14's predictive back-to-previous: the current screen scales down,
/// rounds and casts a shadow as it follows the finger; the previous screen
/// grows from behind and brightens.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub current_scale: f32,
    /// Horizontal offset of the current screen, as a share of the width.
    pub current_dx: f32,
    /// How round the current screen's corners are, 0 to 1 of the window radius.
    pub current_round: f32,
    /// The current screen's shadow, 0 to 1.
    pub current_shadow: f32,
    pub current_opacity: f32,
    pub previous_scale: f32,
    /// The black veil over the previous screen, 0 to 1 of its maximum.
    pub previous_dim: f32,
}

/// The smallest the current screen gets while dragging.
pub const MIN_SCALE: f32 = 0.90;
/// The previous screen's scale at rest behind the current one.
pub const PREVIOUS_SCALE: f32 = 0.95;
/// The shift toward the finger at full drag, as a share of the width.
pub const SHIFT: f32 = 0.08;

impl Pose {
    /// The pose at drag progress `d` and commit progress `k`, both 0 to 1.
    pub fn at(d: f32, k: f32) -> Self {
        let d = d.clamp(0.0, 1.0);
        let k = k.clamp(0.0, 1.0);
        let reveal = d.max(k);
        Self {
            current_scale: 1.0 - (1.0 - MIN_SCALE) * d,
            current_dx: SHIFT * d + (1.05 - SHIFT * d) * k,
            current_round: (d / 0.3).min(1.0),
            current_shadow: (d / 0.3).min(1.0) * (1.0 - smoothstep(0.7, 1.0, k)),
            current_opacity: 1.0 - 0.6 * smoothstep(0.6, 1.0, k),
            previous_scale: PREVIOUS_SCALE + (1.0 - PREVIOUS_SCALE) * reveal,
            previous_dim: 1.0 - reveal,
        }
    }

    /// The pose while committing from drag progress `d0` at commit progress
    /// `k`: the drag finishes over the first 40 % of the settle, as a fast
    /// fling would.
    pub fn committing(d0: f32, k: f32) -> Self {
        let d = d0 + (1.0 - d0) * (k / 0.4).clamp(0.0, 1.0);
        Self::at(d, k)
    }

    /// The pose under reduced motion: nothing scales or slides, the current
    /// screen only fades out over the previous one.
    pub fn cross_fade(d: f32, k: f32) -> Self {
        let progress = d.max(k).clamp(0.0, 1.0);
        Self {
            current_scale: 1.0,
            current_dx: 0.0,
            current_round: 0.0,
            current_shadow: 0.0,
            current_opacity: 1.0 - progress,
            previous_scale: 1.0,
            previous_dim: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(ms: u64) -> Duration {
        Duration::from_millis(ms)
    }

    /// A phone: 400 px wide.
    const W: f32 = 400.0;

    fn drag(gesture: &mut BackGesture, from_ms: u64, xs: &[f32]) -> u64 {
        let mut now = from_ms;
        for x in xs {
            now += 16;
            gesture.moved(at(now), (*x, 300.0));
        }
        now
    }

    #[test]
    fn only_the_edge_zone_starts_a_gesture() {
        let mut gesture = BackGesture::default();
        assert!(!gesture.down(at(0), (21.0, 300.0), W, true));
        assert_eq!(gesture.phase(), Phase::Idle);
        assert!(gesture.down(at(0), (2.0, 300.0), W, true));
        assert_eq!(gesture.phase(), Phase::Tracking);
    }

    #[test]
    fn it_never_fires_without_a_previous_entry() {
        let mut gesture = BackGesture::default();
        assert!(!gesture.down(at(0), (2.0, 300.0), W, false));
        assert_eq!(gesture.up(at(10)), Release::Ignored);
        assert!(!gesture.is_active());
    }

    #[test]
    fn a_small_wobble_is_not_a_drag_and_lifts_as_a_tap() {
        let mut gesture = BackGesture::default();
        gesture.down(at(0), (2.0, 300.0), W, true);
        assert_eq!(gesture.moved(at(16), (6.0, 302.0)), Update::None);
        assert_eq!(gesture.up(at(40)), Release::Tap);
        assert_eq!(gesture.phase(), Phase::Idle);
    }

    #[test]
    fn vertical_travel_hands_the_touch_to_the_scroll_view() {
        let mut gesture = BackGesture::default();
        gesture.down(at(0), (2.0, 300.0), W, true);
        assert_eq!(gesture.moved(at(16), (4.0, 330.0)), Update::Aborted);
        assert_eq!(gesture.phase(), Phase::Aborted);
        // Later horizontal travel changes nothing.
        assert_eq!(gesture.moved(at(32), (200.0, 330.0)), Update::None);
        assert_eq!(gesture.up(at(48)), Release::Ignored);
        assert!(!gesture.is_active());
    }

    #[test]
    fn a_leftward_start_is_not_a_back_swipe() {
        let mut gesture = BackGesture::default();
        gesture.down(at(0), (10.0, 300.0), W, true);
        assert_eq!(gesture.moved(at(16), (5.0, 300.0)), Update::None);
        assert_eq!(gesture.moved(at(32), (-3.0, 300.0)), Update::Aborted);
        assert_eq!(gesture.up(at(40)), Release::Ignored);
    }

    #[test]
    fn progress_is_distance_over_width() {
        let mut gesture = BackGesture::default();
        gesture.down(at(0), (2.0, 300.0), W, true);
        assert_eq!(gesture.moved(at(16), (50.0, 300.0)), Update::Started);
        let Update::Progress(p) = gesture.moved(at(32), (202.0, 300.0)) else {
            panic!("a drag reports progress");
        };
        assert!((p - 0.5).abs() < 1e-6, "{p}");
        // It stops at 1 and never goes below 0.
        let Update::Progress(p) = gesture.moved(at(48), (900.0, 300.0)) else {
            panic!()
        };
        assert_eq!(p, 1.0);
        assert!((gesture.value(at(48)) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn a_slow_release_past_the_threshold_commits() {
        let mut gesture = BackGesture::default();
        gesture.down(at(0), (2.0, 300.0), W, true);
        // 150 px of 400 = 37.5 %, then rest for a while: no fling.
        let now = drag(&mut gesture, 0, &[20.0, 60.0, 110.0, 152.0]);
        let release = gesture.up(at(now + 400));
        let Release::Commit { from } = release else {
            panic!("{release:?}");
        };
        assert!((from - 0.375).abs() < 1e-6);
    }

    #[test]
    fn a_slow_release_before_the_threshold_cancels() {
        let mut gesture = BackGesture::default();
        gesture.down(at(0), (2.0, 300.0), W, true);
        let now = drag(&mut gesture, 0, &[20.0, 50.0, 80.0, 100.0]);
        assert!(matches!(gesture.up(at(now + 400)), Release::Cancel { .. }));
    }

    #[test]
    fn a_fast_rightward_fling_commits_even_a_little_way_in() {
        let mut gesture = BackGesture::default();
        gesture.down(at(0), (2.0, 300.0), W, true);
        // 60 px in 48 ms is over 1000 px/s.
        let now = drag(&mut gesture, 0, &[20.0, 40.0, 62.0]);
        assert!(gesture.velocity() > 700.0, "{}", gesture.velocity());
        assert!(matches!(gesture.up(at(now + 1)), Release::Commit { .. }));
    }

    #[test]
    fn a_leftward_fling_cancels_even_far_along() {
        let mut gesture = BackGesture::default();
        gesture.down(at(0), (2.0, 300.0), W, true);
        // Out past 60 %, then back quickly.
        let steps = [
            100.0, 200.0, 300.0, 300.0, 300.0, 300.0, 300.0, 290.0, 270.0, 250.0, 230.0,
        ];
        let now = drag(&mut gesture, 0, &steps);
        assert!(gesture.velocity() < -500.0, "{}", gesture.velocity());
        assert!(gesture.value(at(now)) > 0.5);
        assert!(matches!(gesture.up(at(now + 1)), Release::Cancel { .. }));
    }

    #[test]
    fn a_tiny_fling_with_no_distance_does_not_commit() {
        let mut gesture = BackGesture::default();
        gesture.down(at(0), (2.0, 300.0), W, true);
        gesture.moved(at(8), (11.0, 300.0));
        gesture.moved(at(12), (13.0, 300.0));
        // About 2600 px/s, but only 3 % of the width.
        let release = gesture.up(at(13));
        assert!(matches!(release, Release::Cancel { .. }), "{release:?}");
    }

    #[test]
    fn moving_back_past_the_edge_cancels() {
        let mut gesture = BackGesture::default();
        gesture.down(at(0), (2.0, 300.0), W, true);
        drag(&mut gesture, 0, &[40.0, 90.0]);
        // Back past where the touch began.
        assert_eq!(gesture.moved(at(100), (-10.0, 300.0)), Update::Cancelled);
        assert!(matches!(gesture.phase(), Phase::Settling { commit: false, .. }));
        // The release after that is nothing more to decide.
        assert_eq!(gesture.up(at(110)), Release::Ignored);
    }

    #[test]
    fn a_commit_settles_to_one_and_a_cancel_to_zero() {
        let mut gesture = BackGesture::default();
        gesture.down(at(0), (2.0, 300.0), W, true);
        let now = drag(&mut gesture, 0, &[100.0, 200.0]);
        let Release::Commit { from } = gesture.up(at(now + 300)) else {
            panic!()
        };
        let start = now + 300;
        let mut last = from;
        for step in 1..=16 {
            let value = gesture.value(at(start + step * 16));
            assert!(value >= last - 1e-6, "monotonic: {value} after {last}");
            last = value;
        }
        assert_eq!(gesture.tick(at(start + 100)), None);
        assert_eq!(gesture.tick(at(start + 260)), Some(true));
        assert_eq!(gesture.phase(), Phase::Idle);

        gesture.down(at(1000), (2.0, 300.0), W, true);
        let now = drag(&mut gesture, 1000, &[40.0, 80.0]);
        let Release::Cancel { from } = gesture.up(at(now + 300)) else {
            panic!()
        };
        let start = now + 300;
        assert!(gesture.value(at(start + 60)) < from);
        assert!(gesture.value(at(start + 239)).abs() < 0.02);
        assert_eq!(gesture.tick(at(start + 240)), Some(false));
        assert_eq!(gesture.value(at(start + 300)), 0.0);
    }

    #[test]
    fn a_spring_back_never_goes_below_zero() {
        let mut gesture = BackGesture::default();
        gesture.down(at(0), (2.0, 300.0), W, true);
        let now = drag(&mut gesture, 0, &[40.0, 90.0]);
        gesture.up(at(now + 300));
        for step in 0..=30 {
            assert!(gesture.value(at(now + 300 + step * 8)) >= 0.0);
        }
    }

    #[test]
    fn grabbing_a_cancel_mid_settle_continues_from_the_pose_on_screen() {
        let mut gesture = BackGesture::default();
        gesture.down(at(0), (2.0, 300.0), W, true);
        let now = drag(&mut gesture, 0, &[60.0, 120.0]);
        let release_at = now + 300;
        gesture.up(at(release_at));
        let grab_at = release_at + 60;
        let on_screen = gesture.value(at(grab_at));
        assert!(on_screen > 0.0, "still settling");
        // The finger grabs the edge again: no jump.
        assert!(gesture.down(at(grab_at), (3.0, 300.0), W, true));
        assert!((gesture.value(at(grab_at)) - on_screen).abs() < 1e-6);
        assert_eq!(gesture.moved(at(grab_at + 16), (20.0, 300.0)), Update::Started);
        let moved = gesture.value(at(grab_at + 16));
        assert!((moved - (on_screen + 17.0 / W)).abs() < 1e-5, "{moved}");
    }

    #[test]
    fn a_new_press_after_a_commit_starts_fresh() {
        let mut gesture = BackGesture::default();
        gesture.down(at(0), (2.0, 300.0), W, true);
        let now = drag(&mut gesture, 0, &[100.0, 200.0]);
        gesture.up(at(now + 300));
        assert!(gesture.down(at(now + 320), (2.0, 300.0), W, true));
        assert_eq!(gesture.value(at(now + 320)), 0.0);
    }

    #[test]
    fn a_second_press_while_one_is_down_is_ignored() {
        let mut gesture = BackGesture::default();
        assert!(gesture.down(at(0), (2.0, 300.0), W, true));
        assert!(!gesture.down(at(5), (3.0, 300.0), W, true));
    }

    #[test]
    fn finish_ends_a_settle_at_once_for_reduced_motion() {
        let mut gesture = BackGesture::default();
        gesture.down(at(0), (2.0, 300.0), W, true);
        let now = drag(&mut gesture, 0, &[100.0, 200.0]);
        gesture.up(at(now + 300));
        assert_eq!(gesture.finish(), Some(true));
        assert!(!gesture.is_active());
    }

    #[test]
    fn the_pose_shrinks_rounds_and_shadows_the_current_screen() {
        let rest = Pose::at(0.0, 0.0);
        assert_eq!(rest.current_scale, 1.0);
        assert_eq!(rest.current_dx, 0.0);
        assert_eq!(rest.current_round, 0.0);
        assert_eq!(rest.current_shadow, 0.0);
        assert_eq!(rest.current_opacity, 1.0);
        assert_eq!(rest.previous_scale, PREVIOUS_SCALE);
        assert_eq!(rest.previous_dim, 1.0);

        let full = Pose::at(1.0, 0.0);
        assert!((full.current_scale - 0.90).abs() < 1e-6);
        assert!((full.current_dx - SHIFT).abs() < 1e-6);
        assert_eq!(full.current_round, 1.0);
        assert_eq!(full.current_shadow, 1.0);
        assert_eq!(full.previous_scale, 1.0);
        assert_eq!(full.previous_dim, 0.0);

        // Monotonic in d.
        let mut last = Pose::at(0.0, 0.0);
        for step in 1..=20 {
            let next = Pose::at(step as f32 / 20.0, 0.0);
            assert!(next.current_scale <= last.current_scale);
            assert!(next.previous_scale >= last.previous_scale);
            assert!(next.previous_dim <= last.previous_dim);
            last = next;
        }
    }

    #[test]
    fn committing_slides_the_current_screen_out_and_settles_the_previous() {
        let end = Pose::committing(0.4, 1.0);
        assert!(end.current_dx > 1.0, "off to the right");
        assert!(end.current_opacity < 0.5);
        assert_eq!(end.current_shadow, 0.0);
        assert_eq!(end.previous_scale, 1.0);
        assert_eq!(end.previous_dim, 0.0);
        // Continuous from the drag: the first frame equals the drag's last.
        let first = Pose::committing(0.4, 0.0);
        let dragged = Pose::at(0.4, 0.0);
        assert_eq!(first, dragged);
        // A synthetic pop from rest reaches the full drag pose by 40 %.
        let early = Pose::committing(0.0, 0.4);
        assert!((early.current_scale - MIN_SCALE).abs() < 1e-6);
    }

    #[test]
    fn the_curves_meet_their_ends() {
        assert_eq!(expo_out(0.0), 0.0);
        assert_eq!(expo_out(1.0), 1.0);
        assert!(expo_out(0.5) > 0.9);
        assert_eq!(spring(0.0), 0.0);
        assert_eq!(spring(1.0), 1.0);
        let peak = (0..=100).map(|i| spring(i as f32 / 100.0)).fold(0.0, f32::max);
        assert!(peak > 1.0 && peak < 1.05, "a slight overshoot: {peak}");
    }
}
