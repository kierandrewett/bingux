//! Behaviour and security tests of the pure core.
//!
//! `Sim` closes the loop with a fake compositor and a fake helper at the event level:
//! it answers `SceneCommand`s and `HelperRequest`s the way the real components do
//! (contract in `bsm-proto`), and lets a test hold or withhold any answer to examine
//! the state in between. Assertions are on manager state and on the effects that
//! would reach the compositor, never on "a function was called".

mod flows;
mod power;
mod security;
mod sim;

pub(crate) use sim::*;
