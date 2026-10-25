//! Keyboard modifiers as held right now, for pointer handlers.
//!
//! Freya's pointer events carry no modifiers, so a handler cannot tell that
//! Alt is held when a click lands. The renderer records every
//! `WindowEvent::ModifiersChanged` here and handlers read the state at press
//! time, which is exact, unlike tracking Alt from key events.

use std::sync::atomic::{
    AtomicU32,
    Ordering,
};

use keyboard_types::Modifiers;

static LIVE: AtomicU32 = AtomicU32::new(0);

pub(crate) fn set_live(modifiers: Modifiers) {
    LIVE.store(modifiers.bits(), Ordering::Relaxed);
}

/// Modifiers held on the keyboard at this moment, across all windows.
pub fn live() -> Modifiers {
    Modifiers::from_bits_truncate(LIVE.load(Ordering::Relaxed))
}
