//! Legal-transition tables. The manager changes a session, operation or
//! transition state only through setters that consult these tables, so an
//! illegal change is a bug that is refused and logged, never silently applied.

use bsm_proto::state::{OperationPhase, SessionState, TransitionState};

/// Is `from -> to` a legal session state change?
///
/// ```text
/// Creating -> PamOpening -> GnoblinStarting -> BinguxStarting -> OutputReady
///   -> ReadyBackground -> Foreground
/// Foreground -> LockRequested -> LockedGreeterForeground | LockedBackground
/// LockedGreeterForeground -> Foreground | LockedBackground
/// LockedBackground -> Foreground | LockedGreeterForeground
/// ReadyBackground -> Foreground
/// (anything alive) -> Stopping | Failed;  Stopping | Failed -> Terminated
/// (recovery) none -> LockedBackground
/// ```
pub fn session_legal(from: SessionState, to: SessionState) -> bool {
    use SessionState::*;
    if from == to {
        return false;
    }
    match (from, to) {
        (Terminated, _) => false,
        (Failed, Terminated) => true,
        (Failed, _) => false,
        (Stopping, Terminated) => true,
        (Stopping, _) => false,
        (_, Stopping | Failed) => true,
        (Creating, PamOpening) => true,
        (PamOpening, GnoblinStarting) => true,
        (GnoblinStarting, BinguxStarting) => true,
        (BinguxStarting, OutputReady) => true,
        (OutputReady, ReadyBackground) => true,
        (ReadyBackground, Foreground) => true,
        (Foreground, LockRequested) => true,
        (LockRequested, LockedGreeterForeground | LockedBackground) => true,
        (LockedGreeterForeground, Foreground | LockedBackground) => true,
        (LockedBackground, Foreground | LockedGreeterForeground) => true,
        _ => false,
    }
}

/// Is `from -> to` a legal operation phase change?
pub fn phase_legal(from: OperationPhase, to: OperationPhase) -> bool {
    use OperationPhase::*;
    if from == to || from.is_final() {
        return false;
    }
    match (from, to) {
        (Requested, Authorising | Preparing | Animating | Committed | Failed | Cancelled | Completed) => true,
        (Authorising, Preparing | Animating | Committed | Failed | Cancelled) => true,
        (Preparing, Animating | Committed | Failed | Cancelled | Completed) => true,
        (Animating, Committed | Failed | Cancelled) => true,
        // After the point of no return only completion or a failure of the system call remain.
        (Committed, Completed | Failed) => true,
        _ => false,
    }
}

/// Is `from -> to` a legal transition state change?
pub fn transition_legal(from: TransitionState, to: TransitionState) -> bool {
    use TransitionState::*;
    if from == to {
        return false;
    }
    match (from, to) {
        (Cancelled | Failed, _) => false,
        (Idle, WaitingForAuth | WaitingForFrames | Prepared | Cancelled | Failed) => true,
        (WaitingForAuth, WaitingForFrames | Prepared | Cancelled | Failed) => true,
        (WaitingForFrames, Prepared | Cancelled | Failed) => true,
        // Reduced motion and the logout of a hidden session skip the animation.
        (Prepared, Animating | Committed | Cancelled | Failed) => true,
        (Animating, Committed | Cancelled | Failed) => true,
        // A committed transition finishes back at Idle; a failed power call may fail it.
        (Committed, Idle | Failed) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use SessionState::*;

    const ALL: [SessionState; 13] = [
        Creating,
        PamOpening,
        GnoblinStarting,
        BinguxStarting,
        OutputReady,
        ReadyBackground,
        Foreground,
        LockRequested,
        LockedBackground,
        LockedGreeterForeground,
        Stopping,
        Terminated,
        Failed,
    ];

    #[test]
    fn session_table_is_exactly_the_documented_edges() {
        let mut edges: Vec<(SessionState, SessionState)> = vec![
            (Creating, PamOpening),
            (PamOpening, GnoblinStarting),
            (GnoblinStarting, BinguxStarting),
            (BinguxStarting, OutputReady),
            (OutputReady, ReadyBackground),
            (ReadyBackground, Foreground),
            (Foreground, LockRequested),
            (LockRequested, LockedGreeterForeground),
            (LockRequested, LockedBackground),
            (LockedGreeterForeground, Foreground),
            (LockedGreeterForeground, LockedBackground),
            (LockedBackground, Foreground),
            (LockedBackground, LockedGreeterForeground),
            (Stopping, Terminated),
            (Failed, Terminated),
        ];
        for s in ALL {
            if s.is_running() {
                edges.push((s, Stopping));
                edges.push((s, Failed));
            }
        }
        for a in ALL {
            for b in ALL {
                assert_eq!(session_legal(a, b), edges.contains(&(a, b)), "{a:?} -> {b:?}");
            }
        }
    }

    #[test]
    fn terminated_is_terminal_and_failed_only_cleans_up() {
        for s in ALL {
            assert!(!session_legal(Terminated, s));
            if s != Terminated {
                assert!(!session_legal(Failed, s));
            }
        }
    }

    #[test]
    fn a_session_cannot_skip_into_foreground() {
        for from in [Creating, PamOpening, GnoblinStarting, BinguxStarting, OutputReady, Failed, Stopping] {
            assert!(!session_legal(from, Foreground), "{from:?} must not become Foreground");
        }
    }

    #[test]
    fn final_phases_are_terminal() {
        use OperationPhase::*;
        for f in [Failed, Cancelled, Completed] {
            for t in [Requested, Authorising, Preparing, Animating, Committed, Failed, Cancelled, Completed] {
                assert!(!phase_legal(f, t));
            }
        }
        assert!(!phase_legal(Committed, Cancelled), "no cancel after the point of no return");
        assert!(phase_legal(Committed, Failed));
    }

    #[test]
    fn transition_state_cannot_leave_failed_or_cancelled() {
        use TransitionState::*;
        for t in [Idle, WaitingForAuth, WaitingForFrames, Prepared, Animating, Committed, Cancelled, Failed] {
            assert!(!transition_legal(Failed, t));
            assert!(!transition_legal(Cancelled, t));
        }
        assert!(!transition_legal(Committed, Cancelled));
        assert!(!transition_legal(Animating, Idle));
    }
}
