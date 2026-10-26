//! Shared types and wire formats for bsm, the Bingux Session Manager.
//!
//! This crate is the single contract between the processes and components of
//! the bsm design (see `docs/bsm/architecture.md`):
//!
//! * [`state`]: the session, system and transition state enums.
//! * [`progress`]: startup milestones and the documented progress model.
//! * [`control`]: the newline-delimited JSON protocol on the greeter control socket.
//! * [`helper`]: the framed protocol between `bsm` and the privileged `bsm-helper`.
//! * [`scene`]: the command and event enums between the session manager and the compositor core.
//! * [`error`]: stable machine-readable error codes shared by every interface.
//! * [`paths`]: the one place that names runtime paths and bus names.
//!
//! The crate has no I/O and no heavy dependencies so that every binary, the
//! test harnesses and the QML clients' documentation can share it.

pub mod control;
pub mod error;
pub mod helper;
pub mod paths;
pub mod progress;
pub mod scene;
pub mod state;

/// Wire version of the control socket, the helper protocol and the D-Bus interface.
pub const PROTOCOL_VERSION: u32 = 1;
