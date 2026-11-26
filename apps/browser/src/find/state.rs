//! The state of the find bar that belongs to the window, not to a tab.

use freya::prelude::*;

/// Counts the requests to focus the find field.
#[derive(Clone, Copy, PartialEq)]
pub struct BarState {
    focus: State<u32>,
}

impl BarState {
    /// The state, as a hook: call it on every render, in the same place.
    pub fn use_new() -> Self {
        Self {
            focus: use_state(|| 0_u32),
        }
    }

    /// Ask the find field to take the focus: a new count is a new field.
    pub fn focus(self) {
        let mut focus = self.focus;
        let next = *focus.peek() + 1;
        focus.set(next);
    }

    /// How many times the field was asked for.
    pub fn requests(self) -> u32 {
        self.focus.read().to_owned()
    }
}
