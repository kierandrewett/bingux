//! Messages between the long-lived helper and the processes it forks. One
//! JSON document per `SOCK_SEQPACKET` datagram, like the public protocol.
//!
//! These never leave the machine and are not a stable interface.

use std::os::fd::RawFd;

use bsm_proto::error::ErrorCode;
use bsm_proto::helper::{MAX_DATAGRAM, PamPromptKind};
use bsm_proto::state::{Generation, SessionId};
use serde::{Deserialize, Serialize};
use serde::de::DeserializeOwned;
use zeroize::Zeroize;

use crate::sys::{self, Recv};

/// Authentication child to helper.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum AuthChildEvent {
    Prompt { kind: PamPromptKind, text: String },
    Done { ok: bool, code: Option<ErrorCode>, message: String },
}

/// Helper to authentication child.
#[derive(Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum AuthParentCmd {
    Response { text: String },
    Abort,
}

impl Drop for AuthParentCmd {
    fn drop(&mut self) {
        if let AuthParentCmd::Response { text } = self {
            text.zeroize();
        }
    }
}

/// Session supervisor to helper.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum SupEvent {
    /// The desktop processes run. Also sent first to a helper that adopts the
    /// supervisor after a helper restart.
    Opened {
        session: SessionId,
        generation: Generation,
        supervisor_pid: u32,
        leader_pid: u32,
        host_pid: u32,
        scope: String,
        logind_session: Option<String>,
    },
    Failed { code: ErrorCode, message: String },
    /// The session is over and the PAM session is closed.
    Exited { status: i32 },
}

/// Helper to session supervisor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum SupCmd {
    Close { grace_ms: u32 },
}

/// Serialise and send one internal message. The serialised buffer is wiped.
pub fn send_msg<T: Serialize>(fd: RawFd, msg: &T) -> std::io::Result<bool> {
    let mut bytes = serde_json::to_vec(msg).map_err(std::io::Error::other)?;
    if bytes.len() > MAX_DATAGRAM {
        bytes.zeroize();
        return Err(std::io::Error::other("internal message too large"));
    }
    let r = sys::send(fd, &bytes);
    bytes.zeroize();
    r
}

pub enum Incoming<T> {
    Msg(T),
    Eof,
    WouldBlock,
    /// Undecodable or oversized datagram; the peer is misbehaving.
    Bad,
}

/// Receive and decode one internal message. The datagram buffer is wiped.
pub fn recv_msg<T: DeserializeOwned>(fd: RawFd) -> std::io::Result<Incoming<T>> {
    let mut buf = vec![0u8; MAX_DATAGRAM + 1];
    let r = sys::recv(fd, &mut buf);
    let out = match r {
        Ok(Recv::Data(n)) => match serde_json::from_slice::<T>(&buf[..n]) {
            Ok(m) => Ok(Incoming::Msg(m)),
            Err(_) => Ok(Incoming::Bad),
        },
        Ok(Recv::Eof) => Ok(Incoming::Eof),
        Ok(Recv::WouldBlock) => Ok(Incoming::WouldBlock),
        Ok(Recv::TooBig) => Ok(Incoming::Bad),
        Err(e) => Err(e),
    };
    buf.zeroize();
    out
}
