//! The state of the keyboard outside a key event.
//!
//! A wheel, pointer or touch event carries no modifier keys, yet a view
//! needs them: Ctrl with the wheel zooms, Shift with the wheel scrolls
//! sideways, Alt with a click adds a caret, Shift with a click extends a
//! selection. [`held_modifiers`] answers from the window's live keyboard
//! state, so no component keeps its own copy from key-down and key-up
//! (which misses a key pressed while another window had the focus).

use freya::prelude::Modifiers;

/// The modifier keys held right now.
pub fn held_modifiers() -> Modifiers {
    freya_winit::modifiers::live()
}

/// Whether Ctrl is held right now.
pub fn ctrl_held() -> bool {
    held_modifiers().contains(Modifiers::CONTROL)
}

/// Whether Shift is held right now.
pub fn shift_held() -> bool {
    held_modifiers().contains(Modifiers::SHIFT)
}

/// Whether Alt is held right now.
pub fn alt_held() -> bool {
    held_modifiers().contains(Modifiers::ALT)
}
