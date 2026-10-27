//! `bsm-host`: the bridge between bsm and one hosted Gnoblin session.
//!
//! See `docs/bsm/bridge.md` for the launch contract and `docs/bsm/architecture.md`
//! (decision D2) for why the bridge exists.

pub mod bridge;
pub mod config;
pub mod frames;
pub mod gnoblin;
pub mod input;
pub mod layers;
pub mod msg;
pub mod proto;
pub mod readiness;
pub mod report_queue;
pub mod ready_sock;
pub mod stream;
