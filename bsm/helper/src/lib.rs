//! `bsm-helper`: the privileged PAM and session helper of bsm.
//!
//! See `docs/bsm/helper.md` for the design. The binary is a thin wrapper
//! around [`server::Server`]; the library exists so that the pieces can be
//! tested without root.

extern crate self as bsm_helper;

pub mod auth;
pub mod cgroup;
pub mod config;
pub mod env;
pub mod greeter;
pub mod ipc;
pub mod logind;
pub mod pam;
pub mod server;
pub mod session;
pub mod sys;
pub mod ticket;

#[cfg(test)]
mod testkit;
#[cfg(test)]
mod testpam;
