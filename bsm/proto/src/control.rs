//! Greeter control socket protocol.
//!
//! Transport: `paths::GREETER_CONTROL_SOCKET`, `SOCK_STREAM`, one JSON object
//! per line (UTF-8, `\n` terminated, at most 64 KiB per line). Only the
//! restricted greeter account, as launched by bsm, may connect (peer
//! credentials are checked on accept). There is exactly one greeter
//! connection; a second connection replaces the first only after the first
//! closed. Secrets travel only in [`ClientMessage::AuthResponse`] and are never logged.
//!
//! Quickshell has no generic D-Bus client, so the greeter QML (`BsmAuthClient.qml`,
//! `BsmGreeterModel.qml`) speaks this protocol through `Quickshell.Io.Socket`.
//! The server sends a full [`ServerMessage::Snapshot`] right after `hello` and
//! after every change of selected user, sessions, mode or appearance; smaller
//! events (prompts, progress, operations) follow in between. A client treats a
//! snapshot with a lower `revision` than it has applied as stale.

use serde::{Deserialize, Serialize};

use crate::error::ErrorCode;
use crate::progress::ProgressModel;
use crate::state::{
    AuthIntent, Generation, OperationId, OperationPhase, PowerAction, Revision, SessionAvailability, SessionId,
    TransitionKind,
};

/// Colour scheme published for an account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorScheme {
    Dark,
    Light,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClockFormat {
    #[serde(rename = "12h")]
    H12,
    #[serde(rename = "24h")]
    H24,
}

/// Whitelisted, validated appearance. `accent` is `#rrggbb`. Nothing here is executed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Appearance {
    pub color_scheme: ColorScheme,
    pub accent: String,
    pub clock_format: ClockFormat,
    pub reduce_motion: bool,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            color_scheme: ColorScheme::Dark,
            accent: "#3584e4".into(),
            clock_format: ClockFormat::H24,
            reduce_motion: false,
        }
    }
}

/// One eligible account as shown by the greeter. Image paths are absolute files
/// under `paths::GREETER_CACHE_DIR` that bsm wrote atomically; `*_rev` increases
/// whenever the file content changes so QML can reload by revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserProfile {
    pub uid: u32,
    pub user_name: String,
    pub display_name: String,
    pub avatar_path: Option<String>,
    pub avatar_rev: Revision,
    pub wallpaper_path: Option<String>,
    pub wallpaper_blur_path: Option<String>,
    pub wallpaper_rev: Revision,
    /// False when the account opted out of showing personal images before login:
    /// the paths above are then `None` and the greeter shows the neutral fallback.
    pub show_personal_images: bool,
    pub appearance: Appearance,
    pub availability: SessionAvailability,
    /// Running session of this account, if any (the most recently active eligible one).
    pub session: Option<SessionId>,
}

/// What the greeter is doing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum GreeterMode {
    /// Cold boot or after logout: account choice with the last user selected.
    Login,
    /// Locked session: the unlock form of `uid`'s `session`; picker is a slide-up affordance.
    Unlock { session: SessionId, uid: u32 },
}

/// Everything the greeter shows, as one revisioned value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub revision: Revision,
    pub mode: GreeterMode,
    /// Account whose avatar, wallpaper and theme are shown. Not a session activation.
    pub selected_uid: Option<u32>,
    pub users: Vec<UserProfile>,
    /// Startup progress of the launch in flight, if any.
    pub progress: Option<ProgressModel>,
    /// Global reduced-motion policy (the greeter also honours the selected account's).
    pub reduce_motion: bool,
    pub hostname: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptKind {
    /// Secret answer (password, PIN, OTP): never echoed.
    Secret,
    /// Visible answer (username, OTP shown).
    Visible,
    /// Informational PAM text. No answer expected.
    Info,
    /// Error text from PAM (for example password expiry).
    Error,
}

/// Messages from the greeter to bsm.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    Hello {
        version: u32,
    },
    /// Show another account's identity. Never activates or reveals a desktop.
    SelectUser {
        uid: u32,
    },
    /// Start a PAM conversation. `request_id` is echoed in the first prompt/result/error.
    BeginAuth {
        request_id: u64,
        uid: u32,
        intent: AuthIntent,
    },
    AuthResponse {
        challenge_id: u64,
        response: String,
    },
    CancelAuth {
        challenge_id: u64,
    },
    /// Power action from the greeter. bsm applies policy; the answer is an `Operation` event stream.
    Power {
        request_id: u64,
        action: PowerAction,
    },
    CancelOperation {
        operation_id: OperationId,
    },
}

/// Debug never shows a secret: the `response` field of an authentication answer is replaced.
impl std::fmt::Debug for ClientMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut v = serde_json::to_value(self).unwrap_or(serde_json::Value::Null);
        if let Some(o) = v.as_object_mut() {
            if let Some(r) = o.get_mut("response") {
                *r = serde_json::Value::String("<redacted>".into());
            }
        }
        write!(f, "ClientMessage({v})")
    }
}

/// Messages from bsm to the greeter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Snapshot(Snapshot),
    /// PAM asks something. Bound to one account, intent, seat and generation inside bsm.
    AuthPrompt {
        request_id: Option<u64>,
        challenge_id: u64,
        kind: PromptKind,
        text: String,
    },
    /// Conversation finished. `ok` means PAM accepted; the desktop is NOT revealed by this
    /// message. bsm reveals only after its own gated transition (see `Operation` events).
    AuthResult {
        challenge_id: u64,
        ok: bool,
        code: Option<ErrorCode>,
        message: String,
    },
    /// Progress of the launch in flight; also embedded in snapshots.
    Progress(ProgressModel),
    Operation {
        operation_id: OperationId,
        kind: TransitionKind,
        phase: OperationPhase,
        reason: Option<ErrorCode>,
    },
    Error {
        request_id: Option<u64>,
        code: ErrorCode,
        message: String,
    },
}

/// Event stream of `bsmctl monitor --json` and the D-Bus signals, one flattened shape
/// that Bingux's `SessionPower.qml` consumes through `bsmctl`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum MonitorEvent {
    SessionsChanged {
        revision: Revision,
    },
    ForegroundChanged {
        session: Option<SessionId>,
        generation: Option<Generation>,
    },
    LockStateChanged {
        session: SessionId,
        locked: bool,
    },
    Transition {
        operation_id: OperationId,
        kind: TransitionKind,
        phase: OperationPhase,
        reason: Option<ErrorCode>,
    },
    OperationFinished {
        operation_id: OperationId,
        success: bool,
        reason: Option<ErrorCode>,
    },
    /// One per change of a launch's `ProgressModel` (additive; also the D-Bus signal
    /// `SessionLaunchProgress`). Lets operators and tests follow real milestones.
    Progress {
        session: SessionId,
        generation: Generation,
        model: ProgressModel,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_messages_use_snake_case_tags() {
        let m: ClientMessage =
            serde_json::from_str(r#"{"type":"begin_auth","request_id":3,"uid":1000,"intent":"unlock"}"#).unwrap();
        assert_eq!(
            m,
            ClientMessage::BeginAuth { request_id: 3, uid: 1000, intent: AuthIntent::Unlock }
        );
    }

    #[test]
    fn mode_round_trips() {
        let m = GreeterMode::Unlock { session: "s0000000a".into(), uid: 1000 };
        let j = serde_json::to_string(&m).unwrap();
        assert_eq!(j, r#"{"mode":"unlock","session":"s0000000a","uid":1000}"#);
        assert_eq!(serde_json::from_str::<GreeterMode>(&j).unwrap(), m);
    }
}
