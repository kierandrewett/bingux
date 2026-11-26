//! The browser's state and its rules, with no window in them: what a tab is,
//! which tab is on show, what the address bar means, what is asked of the
//! engine for each command and what each event does to the state.
//!
//! Everything here is plain data and functions, tested without a UI, over the
//! in-memory test double where the engine is needed.

pub mod downloads;
pub mod failure;
pub mod tab;
pub mod tabs;

#[cfg(test)]
mod engine_tests;
#[cfg(test)]
mod tests;
