//! `bsm` and `bsmctl`: the process around the session manager and the compositor.
//!
//! * [`cli`]: command line of `bsm`.
//! * [`config`]: `/etc/bsm/bsm.toml`.
//! * [`notify`]: `sd_notify` readiness and watchdog.
//! * [`adapters`]: accounts and appearance through `bsm-appearance`.
//! * [`run`]: wires the compositor (main thread) and the manager runtime (own threads).

pub mod adapters;
pub mod cli;
pub mod config;
pub mod ctl;
pub mod notify;
pub mod run;

/// `bsm --version` text.
pub fn version_line() -> String {
    format!("bsm {} (protocol {})", env!("CARGO_PKG_VERSION"), bsm_proto::PROTOCOL_VERSION)
}
