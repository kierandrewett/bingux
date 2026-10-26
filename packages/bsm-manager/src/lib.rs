//! Session manager of bsm, the Bingux Session Manager.
//!
//! * [`core`]: the pure state machine ([`core::Manager`]). No I/O, no clock, injected randomness.
//! * `runtime`: the threads and sockets around it (added next to the core).
//!
//! See `docs/bsm/manager.md`.

pub mod core;
