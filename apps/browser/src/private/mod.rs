//! The private window: another copy of the browser, started with `--private`,
//! whose tabs keep nothing. It is a process of its own so that its cookies,
//! storage and engine are not shared with this window's in any way. It starts
//! with the settings of `profile` (this window's), which it reads and never writes.

#[cfg(feature = "snapshot")]
mod snapshot;
#[cfg(test)]
mod tests;
#[cfg(feature = "snapshot")]
pub(crate) use snapshot::scenarios;

use std::ffi::OsString;

use crate::app::Launch;

/// The private window, started from this one.
pub struct PrivateWindow;

impl PrivateWindow {
    /// Start a private window. Nothing waits for it.
    pub fn open(profile: Option<String>) {
        let mut arguments: Vec<OsString> = vec!["--private".into()];
        if let Some(profile) = profile {
            arguments.push("--profile".into());
            arguments.push(profile.into());
        }
        Launch::start_copy(arguments);
    }
}
