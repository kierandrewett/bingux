//! Pure transition timeline math. No clock, no GPU: the compositor feeds it a
//! progress value `t` in `0.0..=1.0` computed from its monotonic frame time, and
//! the tests assert the exact trajectories (see `docs/bsm/architecture.md`).

use crate::state::TransitionKind;

/// Default duration in milliseconds.
pub fn default_duration_ms(kind: TransitionKind) -> u32 {
    match kind {
        TransitionKind::NewLoginFade => 420,
        TransitionKind::UnlockSlideUp => 520,
        TransitionKind::Lock => 240,
        TransitionKind::SwitchUser => 240,
        TransitionKind::LogoutFade => 320,
        TransitionKind::PowerFade => 360,
        TransitionKind::Suspend => 200,
    }
}

/// One frame of the scene. Layers from bottom to top: cover, then
/// (greeter, session) in the order given by `session_above_greeter`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    /// Opaque safe colour behind everything.
    pub cover_opacity: f32,
    pub greeter_opacity: f32,
    /// Vertical offset of the whole greeter scene in output heights. 0 = in place,
    /// -1 = fully moved up and off the output. The same value applies to every output.
    pub greeter_offset_y: f32,
    pub session_opacity: f32,
    pub session_above_greeter: bool,
}

/// `cubic-bezier(x1, y1, x2, y2)` easing, solved with Newton and bisection fallback.
pub fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let bez = |a: f32, b: f32, s: f32| {
        let u = 1.0 - s;
        3.0 * u * u * s * a + 3.0 * u * s * s * b + s * s * s
    };
    let dbez = |a: f32, b: f32, s: f32| {
        let u = 1.0 - s;
        3.0 * u * u * a + 6.0 * u * s * (b - a) + 3.0 * s * s * (1.0 - b)
    };
    let mut s = t;
    for _ in 0..8 {
        let err = bez(x1, x2, s) - t;
        if err.abs() < 1e-6 {
            return bez(y1, y2, s);
        }
        let d = dbez(x1, x2, s);
        if d.abs() < 1e-6 {
            break;
        }
        s = (s - err / d).clamp(0.0, 1.0);
    }
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..32 {
        s = 0.5 * (lo + hi);
        if bez(x1, x2, s) < t {
            lo = s;
        } else {
            hi = s;
        }
    }
    bez(y1, y2, s)
}

/// Easing used by the unlock slide: fast start, long soft landing (Windows-like).
pub fn ease_unlock(t: f32) -> f32 {
    cubic_bezier(0.22, 1.0, 0.36, 1.0, t)
}

pub fn ease_out_cubic(t: f32) -> f32 {
    let u = 1.0 - t.clamp(0.0, 1.0);
    1.0 - u * u * u
}

pub fn ease_in_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 { 4.0 * t * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(3) / 2.0 }
}

/// The scene at progress `t` of `kind`. `t` is clamped to `0.0..=1.0`.
pub fn trajectory(kind: TransitionKind, t: f32) -> Frame {
    let t = t.clamp(0.0, 1.0);
    match kind {
        TransitionKind::NewLoginFade => Frame {
            cover_opacity: 1.0,
            greeter_opacity: 1.0,
            greeter_offset_y: 0.0,
            session_opacity: ease_out_cubic(t),
            session_above_greeter: true,
        },
        TransitionKind::UnlockSlideUp => Frame {
            cover_opacity: 1.0,
            greeter_opacity: 1.0,
            greeter_offset_y: -ease_unlock(t),
            session_opacity: 1.0,
            session_above_greeter: false,
        },
        TransitionKind::Lock | TransitionKind::SwitchUser | TransitionKind::Suspend => Frame {
            cover_opacity: 1.0,
            greeter_opacity: ease_out_cubic(t),
            greeter_offset_y: 0.0,
            session_opacity: 0.0,
            session_above_greeter: false,
        },
        TransitionKind::LogoutFade => Frame {
            cover_opacity: 1.0,
            greeter_opacity: 1.0,
            greeter_offset_y: 0.0,
            session_opacity: 1.0 - ease_in_out_cubic(t),
            session_above_greeter: true,
        },
        TransitionKind::PowerFade => Frame {
            cover_opacity: ease_in_out_cubic(t),
            greeter_opacity: 1.0 - ease_in_out_cubic(t),
            greeter_offset_y: 0.0,
            session_opacity: 1.0 - ease_in_out_cubic(t),
            session_above_greeter: false,
        },
    }
}

/// Progress for a frame at `elapsed_ms` of a transition lasting `duration_ms`.
/// With `reduce_motion` the transition completes in its first frame.
pub fn progress(elapsed_ms: u64, duration_ms: u32, reduce_motion: bool) -> f32 {
    if reduce_motion || duration_ms == 0 {
        return 1.0;
    }
    (elapsed_ms as f32 / duration_ms as f32).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unlock_slides_monotonically_from_zero_to_minus_one() {
        let mut prev = 0.0;
        assert_eq!(trajectory(TransitionKind::UnlockSlideUp, 0.0).greeter_offset_y, 0.0);
        for i in 1..=100 {
            let y = trajectory(TransitionKind::UnlockSlideUp, i as f32 / 100.0).greeter_offset_y;
            assert!(y <= prev + 1e-6, "offset moved back down at step {i}");
            prev = y;
        }
        assert!((prev + 1.0).abs() < 1e-4);
    }

    #[test]
    fn unlock_easing_front_loads_motion() {
        // Windows-like: more than half the distance is covered in the first third.
        assert!(ease_unlock(1.0 / 3.0) > 0.5);
    }

    #[test]
    fn new_login_keeps_greeter_opaque_and_fades_session_above() {
        for i in 0..=20 {
            let f = trajectory(TransitionKind::NewLoginFade, i as f32 / 20.0);
            assert_eq!(f.greeter_opacity, 1.0);
            assert!(f.session_above_greeter);
        }
        assert_eq!(trajectory(TransitionKind::NewLoginFade, 0.0).session_opacity, 0.0);
        assert_eq!(trajectory(TransitionKind::NewLoginFade, 1.0).session_opacity, 1.0);
    }

    #[test]
    fn lock_never_shows_the_session() {
        for i in 0..=20 {
            assert_eq!(trajectory(TransitionKind::Lock, i as f32 / 20.0).session_opacity, 0.0);
        }
    }

    #[test]
    fn reduced_motion_is_one_frame() {
        assert_eq!(progress(0, 520, true), 1.0);
        assert_eq!(progress(0, 520, false), 0.0);
        assert_eq!(progress(10_000, 520, false), 1.0);
    }
}
