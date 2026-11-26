//! The text the dialogs of a page hold while they are open.

use freya::prelude::*;

/// What the open dialog's fields hold: a name or the text of a `prompt`, a
/// password, or the option chosen in a list. Only one dialog is open at a time,
/// so the dialogs share them; each one sets what it uses when it opens.
#[derive(Clone, Copy, PartialEq)]
pub struct DialogFields {
    pub(super) text: State<String>,
    pub(super) password: State<String>,
    /// The option of a `<select>` list the dropdown is on.
    pub(super) choice: State<usize>,
}

impl DialogFields {
    /// The states, as a hook: call it on every render, in the same place.
    pub fn use_new() -> Self {
        Self {
            text: use_state(String::new),
            password: use_state(String::new),
            choice: use_state(|| 0),
        }
    }
}
