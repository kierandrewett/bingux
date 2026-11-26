//! Profiles: the registry of the person's browser identities, the folders of
//! each one, the lock that lets one process at a time use a profile, and the
//! store of its settings and history. The design, with every failure and the
//! state the window shows for it, is `docs/browser/profiles-and-store.md`.
//!
//! Everything here touches files, so none of it runs on the UI thread: `main`
//! opens the profile before the window exists, and the store has a thread of
//! its own.

pub mod history;
pub mod lock;
pub mod migrate;
pub mod open;
pub mod places;
pub mod registry;
pub mod settings;

#[cfg(test)]
mod history_tests;
#[cfg(test)]
mod private_tests;
#[cfg(test)]
mod registry_tests;
