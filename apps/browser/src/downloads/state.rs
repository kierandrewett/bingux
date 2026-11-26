//! The states of the downloads card that belong to the window, not to a tab.

use freya::prelude::*;

/// Whether the downloads card is open, and where its button is.
#[derive(Clone, Copy, PartialEq)]
pub struct CardState {
    pub(super) open: State<bool>,
    /// Where the downloads button is, in the window.
    pub(super) anchor: State<Area>,
}

impl CardState {
    /// The states, as a hook: call it on every render, in the same place.
    pub fn use_new() -> Self {
        Self {
            open: use_state(|| false),
            anchor: use_state(Area::default),
        }
    }

    /// Whether the card is open.
    pub fn is_open(self) -> bool {
        *self.open.peek()
    }

    /// Show the downloads card.
    pub fn open(self) {
        let mut open = self.open;
        open.set(true);
    }
}
