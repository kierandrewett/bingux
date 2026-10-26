//! Commands and events between the session manager and the compositor core,
//! which live in the same `bsm` process (the manager on its own thread, the
//! compositor on the render thread; they exchange these values through bounded
//! channels). The compositor never decides policy; the manager never touches
//! pixels. Every message that concerns a hosted session carries `(session,
//! generation)` and the receiver drops it when the generation is not current.

use serde::{Deserialize, Serialize};

use crate::state::{Generation, InputOwner, OperationId, PowerAction, SessionId, TransitionKind};

/// A physical output as the compositor sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputInfo {
    /// Stable name (`DP-1`, `Virtual-1`, ...).
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    /// Fractional scale times 120 (wp-fractional-scale convention): 120 = 1.0.
    pub scale_120: u32,
    pub refresh_mhz: u32,
    pub transform: u32,
}

/// What a hosted surface has reported so far; evidence for the reveal gate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceEvidence {
    pub output: String,
    /// Last configure serial bsm sent for this output of this generation.
    pub configure_serial: u32,
    /// Configure acknowledged by the session.
    pub acked: bool,
    /// Number of buffers bsm received and presented for this output after the ack.
    pub presented_frames: u64,
}

/// Visual identity of a protected/greeter cover when nothing is authorised to show.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CoverColor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

/// Manager to compositor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SceneCommand {
    /// Create the brokered socket for a session generation and start expecting its host.
    /// The compositor answers with `SceneEvent::SessionSocketReady`.
    RegisterSession {
        session: SessionId,
        generation: Generation,
        uid: u32,
        /// Cgroup scope the connecting process must be in (checked via /proc/<pid>/cgroup).
        scope: String,
        /// Pid of the bridge process that is allowed to connect.
        host_pid: u32,
        /// Absolute path the bridge's `/proc/<pid>/exe` must resolve to.
        host_exe: String,
    },
    /// Drop the session: remove its socket, scene node and buffers. Safe cover until the
    /// next command changes the scene.
    UnregisterSession {
        session: SessionId,
        generation: Generation,
    },
    /// Authorise the greeter peer (pid started by the helper, account uid).
    RegisterGreeter {
        generation: Generation,
        uid: u32,
        pid: u32,
    },
    UnregisterGreeter {
        generation: Generation,
    },
    /// Immediately protect a session: its pixels leave every output and capture and its
    /// input route is revoked. Replied to with `SceneEvent::Protected` only once the
    /// compositor has rendered a frame without that session on all outputs.
    Protect {
        session: SessionId,
        generation: Generation,
    },
    /// Route physical input. Takes effect at the next frame boundary and replies with
    /// `SceneEvent::InputOwnerChanged`.
    SetInputOwner(InputOwner),
    /// Show only the greeter scene (plus cover colour behind it) with no desktop. Instant.
    ShowGreeterOnly,
    /// Run a compositor-owned transition. The compositor validates source and target are
    /// registered and presentable before the first animated frame and again each frame.
    BeginTransition {
        operation: OperationId,
        kind: TransitionKind,
        /// Session fading/sliding away from, if any.
        source: Option<(SessionId, Generation)>,
        /// Session being revealed, if any.
        target: Option<(SessionId, Generation)>,
        duration_ms: u32,
        /// Reduced motion: complete in one secure frame, same gates.
        reduce_motion: bool,
        /// For `PowerFade`/`Suspend`: the action, shown nowhere but recorded.
        power: Option<PowerAction>,
    },
    /// Abort a transition that has not committed. The compositor restores the scene that
    /// was authorised before it began, or the safe cover plus greeter.
    CancelTransition {
        operation: OperationId,
    },
    SetCoverColor(CoverColor),
    /// Disable/enable DPMS. Lock state is unaffected.
    SetDpms {
        on: bool,
    },
}

/// Compositor to manager.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SceneEvent {
    Ready {
        outputs: Vec<OutputInfo>,
    },
    OutputsChanged {
        outputs: Vec<OutputInfo>,
    },
    GreeterConnected {
        generation: Generation,
        pid: u32,
    },
    GreeterGone {
        generation: Generation,
    },
    SessionSocketReady {
        session: SessionId,
        generation: Generation,
        path: String,
    },
    /// The host connected and passed uid, scope and exe checks.
    SessionConnected {
        session: SessionId,
        generation: Generation,
        pid: u32,
    },
    /// A connection to a session socket was rejected. Reason is for the audit log.
    SessionConnectionRejected {
        session: SessionId,
        generation: Generation,
        reason: String,
    },
    /// Per-output evidence changed (configure sent/acked, new presented frame).
    SessionSurfaces {
        session: SessionId,
        generation: Generation,
        surfaces: Vec<SurfaceEvidence>,
    },
    /// Stage reports from the bridge, validated for shape only; the manager decides what they mean.
    SessionReport {
        session: SessionId,
        generation: Generation,
        report: HostReport,
    },
    SessionGone {
        session: SessionId,
        generation: Generation,
    },
    Protected {
        session: SessionId,
        generation: Generation,
    },
    InputOwnerChanged(InputOwner),
    TransitionPhase {
        operation: OperationId,
        phase: TransitionPhaseEvent,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransitionPhaseEvent {
    /// Source/target validated; first animated frame about to render.
    Prepared,
    Animating,
    /// The irreversible point: input route switched / scene swapped.
    Committed,
    Completed,
    /// Aborted; the compositor already restored a safe scene.
    Failed,
    Cancelled,
}

/// Reports a hosted session's bridge sends over `bsm_session_host_v1.report`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HostReport {
    /// Gnoblin runtime handshake complete (supervisor `READY`) and the compositor
    /// serves session-local clients.
    GnoblinReady,
    /// A virtual output was created for the named bsm output and is configured. Sent once per output.
    OutputConfigured { output: String },
    /// Bingux wallpaper and shell processes launched.
    BinguxLaunched,
    /// Required Bingux surfaces are mapped on `output`: counts of mapped over required.
    SurfacesMapped { output: String, mapped: u32, required: u32 },
    /// Bingux declared its initial desktop ready for the generation.
    DesktopReady,
}
