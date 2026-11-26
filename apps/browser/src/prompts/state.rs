//! The text the dialogs of a page hold while they are open.

use freya::prelude::*;

/// What the open dialog's fields hold: a name or the text of a `prompt`, and a password.
#[derive(Clone, Copy, PartialEq)]
pub struct DialogFields {
    pub(super) text: State<String>,
    pub(super) password: State<String>,
}

impl DialogFields {
    /// The states, as a hook: call it on every render, in the same place.
    pub fn use_new() -> Self {
        Self {
            text: use_state(String::new),
            password: use_state(String::new),
        }
    }
}
