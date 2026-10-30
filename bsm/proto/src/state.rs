//! State enums owned by the bsm session manager. The manager is the only
//! writer; every other component reads them through events or snapshots.

use serde::{Deserialize, Serialize};

/// Session identifier: `s` followed by 8 lowercase hex digits. Stable for the
/// life of one hosted session registration.
pub type SessionId = String;

/// Random, non-zero incarnation counter. A new launch of the same account gets a
/// new generation; every readiness, progress and authentication callback carries
/// it and is dropped when it does not match.
pub type Generation = u64;

/// Monotonic revision of a published snapshot, appearance or session list.
pub type Revision = u64;

/// Operation identifier returned by every action request.
pub type OperationId = u64;

/// Lifecycle of one hosted user session.
///
/// ```text
/// Creating -> PamOpening -> GnoblinStarting -> BinguxStarting -> OutputReady
///   -> ReadyBackground -> Foreground
/// Foreground -> LockRequested -> LockedBackground | LockedGreeterForeground
/// LockedGreeterForeground -> Foreground          (authorised unlock)
/// LockedBackground -> Foreground                 (authorised switch back)
/// any -> Stopping -> Terminated
/// any -> Failed -> Terminated                    (cleanup always runs)
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Creating,
    PamOpening,
    GnoblinStarting,
    BinguxStarting,
    OutputReady,
    /// Ready to reveal but not on screen (first login waiting for the gate).
    ReadyBackground,
    Foreground,
    LockRequested,
    /// Protected, not foreground, input revoked, buffers retained.
    LockedBackground,
    /// Protected and the greeter is showing this session's unlock form.
    LockedGreeterForeground,
    Stopping,
    Terminated,
    Failed,
}

impl SessionState {
    /// True while the session's processes are expected to be alive.
    pub fn is_running(self) -> bool {
        !matches!(self, Self::Stopping | Self::Terminated | Self::Failed)
    }
    /// True when the session's pixels must never reach any output or capture.
    pub fn is_protected(self) -> bool {
        matches!(
            self,
            Self::LockRequested | Self::LockedBackground | Self::LockedGreeterForeground
        )
    }
    /// True once a login can be revealed (all required outputs presented).
    pub fn is_revealable(self) -> bool {
        matches!(self, Self::ReadyBackground | Self::LockedBackground | Self::LockedGreeterForeground)
    }
}

/// System UI / seat state. One value per physical seat; bsm has one seat today.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeatState {
    Booting,
    Greeter,
    Authenticating,
    StartingSession,
    RevealingSession,
    UserForeground,
    SecuringLock,
    LockedGreeter,
    SwitchingUser,
    ShuttingDown,
    /// Safe opaque cover plus greeter recovery after a failure.
    Recovery,
}

/// Transition state machine, one per in-flight operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransitionState {
    Idle,
    WaitingForAuth,
    WaitingForFrames,
    Prepared,
    Animating,
    Committed,
    Cancelled,
    Failed,
}

/// Phase reported to callers in `TransitionChanged` / `OperationEvent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationPhase {
    Requested,
    Authorising,
    Preparing,
    Animating,
    Committed,
    Failed,
    Cancelled,
    Completed,
}

impl OperationPhase {
    pub fn is_final(self) -> bool {
        matches!(self, Self::Failed | Self::Cancelled | Self::Completed)
    }
}

/// Semantic transition types. Each has its own animation, see `docs/bsm/architecture.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransitionKind {
    /// Greeter stays; the ready hosted desktop fades in over it (opacity 0 -> 1).
    NewLoginFade,
    /// The greeter/lock scene slides up and off every output, revealing the existing session.
    UnlockSlideUp,
    /// Protect first, then cover the desktop with the greeter scene.
    Lock,
    /// Prior user protected, greeter visible (no desktop-to-desktop crossfade ever).
    SwitchUser,
    /// The leaving session fades out into the already-ready greeter scene.
    LogoutFade,
    /// Fullscreen fade on every output for reboot/power off, kept until the system stops.
    PowerFade,
    /// Protect everything, then suspend; resume lands on the greeter.
    Suspend,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PowerAction {
    Suspend,
    Reboot,
    PowerOff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthIntent {
    /// Cold login or first login of an account that has no session.
    Login,
    /// Unlock the foreground (locked) session of the same account.
    Unlock,
    /// Authenticate another account from the picker (login or resume, decided by the manager).
    Switch,
}

/// What the picker shows for an account. Never inferred from a cached profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionAvailability {
    NotSignedIn,
    Starting,
    RunningLocked,
    RunningForeground,
    Unavailable,
}

/// Which principal owns the physical keyboard and pointer. At most one at any time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "session", rename_all = "snake_case")]
pub enum InputOwner {
    None,
    Greeter,
    Session(SessionId),
}

/// Public summary of a session. Contains no window titles or UI state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: SessionId,
    pub generation: Generation,
    pub uid: u32,
    pub state: SessionState,
    pub availability: SessionAvailability,
}
