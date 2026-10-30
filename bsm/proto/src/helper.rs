//! Protocol between `bsm` (account `bsm`, unprivileged) and `bsm-helper` (root).
//!
//! Transport: `paths::HELPER_SOCKET`, `SOCK_SEQPACKET`; one JSON message per
//! datagram (at most 64 KiB). `bsm-helper` accepts only a peer whose uid is the
//! `bsm` account and which is the process that systemd started for `bsm.service`.
//! The helper never trusts a uid, session id or role from the message body for
//! authorisation decisions beyond what it checked itself:
//!
//! * A PAM conversation (`AuthStart`) runs entirely inside the helper (a forked
//!   child with the PAM handle, never inside bsm's render process).
//! * `OpenSession` is accepted only with the `ticket` that the helper itself
//!   returned in `AuthDone { ok: true }`. A ticket is single-use, bound to the uid,
//!   the intent and the generation, and expires after `TICKET_TTL_SECS`.
//! * The helper starts the session as that uid, keeps the PAM session handle for
//!   the desktop's lifetime (`pam_close_session` runs only on `CloseSession` or
//!   when the session leader exits) and reports `SessionExited`.

use serde::{Deserialize, Serialize};

use crate::error::ErrorCode;
use crate::state::{AuthIntent, Generation, SessionId};

pub const TICKET_TTL_SECS: u64 = 30;
pub const MAX_DATAGRAM: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PamPromptKind {
    EchoOff,
    EchoOn,
    Info,
    Error,
}

/// bsm to helper.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HelperRequest {
    Ping { nonce: u64 },
    /// Start an authentication conversation for `user`. `service` is the PAM service
    /// (`bsm-login` or `bsm-unlock`, see `docs/bsm/architecture.md`).
    AuthStart {
        req_id: u64,
        user: String,
        uid: u32,
        intent: AuthIntent,
        generation: Generation,
        service: String,
    },
    AuthResponse {
        req_id: u64,
        /// Zeroised by the helper after use. Never logged.
        response: String,
    },
    AuthAbort {
        req_id: u64,
    },
    /// Open the PAM session and start the hosted desktop. `socket_path` is the brokered
    /// per-session Wayland socket that bsm created for this generation.
    OpenSession {
        req_id: u64,
        ticket: u64,
        session: SessionId,
        generation: Generation,
        socket_path: String,
        /// Resolution hints for the first output set; the real outputs come from bsm's protocol.
        output_hints: Vec<(u32, u32)>,
    },
    /// End the PAM session and terminate the session scope. Only this session.
    CloseSession {
        session: SessionId,
        generation: Generation,
        /// Grace period before SIGKILL.
        grace_ms: u32,
    },
    /// Launch the restricted greeter (Quickshell) as the greeter account.
    SpawnGreeter {
        req_id: u64,
        generation: Generation,
        wayland_socket: String,
        control_socket: String,
    },
    KillGreeter {
        generation: Generation,
    },
    /// Set logind's `LockedHint` on this session's logind session (additive, bsm-helper
    /// calls `org.freedesktop.login1.Session.SetLockedHint`). Answered by
    /// `HelperEvent::LockedHintResult`.
    SetLockedHint {
        req_id: u64,
        session: SessionId,
        generation: Generation,
        locked: bool,
    },
}

/// Debug never shows a secret: the `response` field of an authentication answer is replaced.
impl std::fmt::Debug for HelperRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut v = serde_json::to_value(self).unwrap_or(serde_json::Value::Null);
        if let Some(o) = v.as_object_mut() {
            if let Some(r) = o.get_mut("response") {
                *r = serde_json::Value::String("<redacted>".into());
            }
        }
        write!(f, "HelperRequest({v})")
    }
}

/// Helper to bsm.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HelperEvent {
    Pong {
        nonce: u64,
    },
    AuthPrompt {
        req_id: u64,
        kind: PamPromptKind,
        text: String,
    },
    AuthDone {
        req_id: u64,
        ok: bool,
        code: Option<ErrorCode>,
        message: String,
        /// Present when `ok`. Opaque, single-use.
        ticket: Option<u64>,
    },
    SessionOpened {
        req_id: u64,
        session: SessionId,
        generation: Generation,
        /// Pid of the process that will connect to the brokered socket (the bridge).
        host_pid: u32,
        /// Pid of the session leader (the Gnoblin runtime).
        leader_pid: u32,
        /// Absolute cgroup path of the session scope (`/user.slice/user-1000.slice/session-5.scope`,
        /// read from `/proc/<pid>/cgroup` after `pam_systemd`); the unit name is its last
        /// component. The helper has verified that the leader and the bridge (`host_pid`) are in it.
        /// bsm compares `/proc/<peer>/cgroup` with the whole path, never with a bare unit name.
        scope: String,
        logind_session: Option<String>,
    },
    SessionFailed {
        req_id: u64,
        session: SessionId,
        generation: Generation,
        code: ErrorCode,
        message: String,
    },
    SessionExited {
        session: SessionId,
        generation: Generation,
        /// Exit status, or negative signal number.
        status: i32,
    },
    GreeterSpawned {
        req_id: u64,
        generation: Generation,
        pid: u32,
    },
    GreeterExited {
        generation: Generation,
        status: i32,
    },
    /// Answer to `HelperRequest::SetLockedHint`.
    LockedHintResult {
        req_id: u64,
        ok: bool,
        code: Option<ErrorCode>,
        message: String,
    },
}

#[cfg(test)]
mod secret_tests {
    use super::*;

    #[test]
    fn debug_of_an_authentication_answer_never_shows_it() {
        let r = HelperRequest::AuthResponse { req_id: 1, response: "hunter2-very-secret".into() };
        let s = format!("{r:?} {:#?}", r);
        assert!(!s.contains("hunter2"), "{s}");
        assert!(s.contains("redacted"));
        let m = crate::control::ClientMessage::AuthResponse { challenge_id: 1, response: "hunter2-very-secret".into() };
        assert!(!format!("{m:?}").contains("hunter2"));
        // Other variants stay readable.
        assert!(format!("{:?}", HelperRequest::Ping { nonce: 4 }).contains("ping"));
    }
}
