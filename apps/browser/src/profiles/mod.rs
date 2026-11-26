//! Profiles: the registry of the person's browser identities, the folders of
//! each one, the lock that lets one process at a time use a profile, and the
//! store of its settings and history. The design, with every failure and the
//! state the window shows for it, is `docs/browser/profiles-and-store.md`.
//!
//! Everything here touches files, so none of it runs on the UI thread: `main`
//! opens the profile before the window exists, and the store has a thread of
//! its own.

mod gate;
mod lock;
mod open;
mod places;
mod registry;

pub use gate::{Gate, GateWindow};
pub use lock::{Held, LockError, ProfileLock};
pub use open::{Opened, ProfileState, Waiting};
pub use places::{Folders, Places};
pub use registry::{ChangeError, Entry, Id, Profiles, RegistryProblem, Startup};

#[cfg(test)]
mod tests;
