//! The window of another profile.

use crate::app::Launch;

/// The window of a profile, started from another profile's window.
pub struct ProfileWindow;

impl ProfileWindow {
    /// Start the window of the profile with registry id `id`, or bring forward
    /// the one already open for it (a second copy of a held profile hands its
    /// launch to the first). Nothing waits for it.
    pub fn open(id: String) {
        Launch::start_copy(vec!["--profile".into(), id.into()]);
    }
}
