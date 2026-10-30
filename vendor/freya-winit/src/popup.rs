//! Popup windows: windows the compositor places against a parent window.
//!
//! Bingux patch, see `vendor/README.md` and `design/popup-windows.md`. A popup is an
//! `xdg_popup` (winit's `WindowAttributesExtWayland::with_popup`): it can extend past the parent's
//! edges, sits above it, and with a grab is dismissed by the compositor on a click elsewhere.
//! [`WindowConfig::with_popup`](crate::config::WindowConfig::with_popup) makes one; this module
//! is what the renderer and the app know about it.

use std::{
    cell::RefCell,
    sync::atomic::{
        AtomicBool,
        Ordering,
    },
};

use freya_core::prelude::{State, WritableUtils};
use rustc_hash::FxHashMap;

static SUPPORTED: AtomicBool = AtomicBool::new(false);

/// Whether this process can open popup windows: its event loop is a Wayland one. False
/// until the event loop has started, and on X11, macOS and Windows.
pub fn supported() -> bool {
    SUPPORTED.load(Ordering::Relaxed)
}

pub(crate) fn set_supported(supported: bool) {
    SUPPORTED.store(supported, Ordering::Relaxed);
}

/// A root context of every popup window's runner: the components inside can tell they are in one.
#[derive(Clone, Copy)]
pub struct PopupMarker;

thread_local! {
    /// The place the compositor gave each popup: its card's top left, in logical pixels from the
    /// top left of its parent's window geometry. The state is owned by the popup's runner.
    static PLACED: RefCell<FxHashMap<u64, State<Option<(f32, f32)>>>> = RefCell::default();
}

pub(crate) fn track(window: u64, placed: State<Option<(f32, f32)>>) {
    PLACED.with(|map| map.borrow_mut().insert(window, placed));
}

pub(crate) fn forget(window: u64) {
    PLACED.with(|map| map.borrow_mut().remove(&window));
}

pub(crate) fn place(window: u64, at: (f32, f32)) {
    let state = PLACED.with(|map| map.borrow().get(&window).copied());
    if let Some(mut state) = state {
        state.set_if_modified(Some(at));
    }
}

/// Where the compositor put popup `window`, once it has: a state to read from the popup's own
/// components, so they redraw when the compositor moves or flips it.
pub fn placed(window: u64) -> Option<State<Option<(f32, f32)>>> {
    PLACED.with(|map| map.borrow().get(&window).copied())
}

/// What a popup asks of the compositor (anchor, gravity, constraint adjustment, grab), as winit
/// states it.
#[cfg(target_os = "linux")]
pub use winit::platform::wayland::{PopupAdjust, PopupAttributes, PopupEdge};
