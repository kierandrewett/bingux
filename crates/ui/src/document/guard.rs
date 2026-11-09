//! The question asked before a window closes: "is there work in it?".
//!
//! Every way a window can close goes through here first: the compositor's
//! close button and Alt+F4 (winit's close request, see `app.rs`), the
//! window's own Close button and Ctrl+W (`MainWindow::close`). A window
//! whose [`Documents`](super::Documents) holds unsaved work registers a
//! guard; when it says "wait", the window stays open and the guard asks the
//! person, then closes the window itself with [`close_now`].
//!
//! The registry is per thread, which is the UI thread: every window of the
//! application runs on it, and so does winit's close request.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// Asks whether the window may close now. Returns `true` when it may not:
/// the guard is asking the person, and closes the window itself when it is
/// done.
pub(crate) type Guard = Rc<dyn Fn() -> bool>;

thread_local! {
    static GUARDS: RefCell<HashMap<u64, Guard>> = RefCell::new(HashMap::new());
}

/// Puts `guard` in front of the close of `window` (a window id as `u64`).
pub(crate) fn register(window: u64, guard: Guard) {
    GUARDS.with(|guards| guards.borrow_mut().insert(window, guard));
}

/// Takes the guard of `window` away: its root went.
pub(crate) fn unregister(window: u64) {
    GUARDS.with(|guards| guards.borrow_mut().remove(&window));
}

/// Whether `window` must stay open for now. A window with no guard closes.
pub(crate) fn must_wait(window: u64) -> bool {
    // Cloned out, so that a guard that registers or unregisters does not
    // find the registry borrowed.
    let guard = GUARDS.with(|guards| guards.borrow().get(&window).cloned());
    guard.is_some_and(|guard| guard())
}

/// Closes `window` without asking again: what a guard calls when the person
/// has answered.
pub(crate) fn close_now(window: freya::winit::window::WindowId) {
    use freya::prelude::*;
    Platform::get().close_window(window);
}

/// Closes `window`, unless a guard asks to wait: what a window's own Close
/// button and Ctrl+W do. A window with no guard goes at once.
pub(crate) fn request_close(window: freya::winit::window::WindowId) {
    if !must_wait(u64::from(window)) {
        close_now(window);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn a_window_with_no_guard_closes_and_a_guard_can_hold_it() {
        assert!(!must_wait(9001));
        let asked = Rc::new(Cell::new(0));
        let counter = asked.clone();
        register(
            9001,
            Rc::new(move || {
                counter.set(counter.get() + 1);
                true
            }),
        );
        assert!(must_wait(9001));
        assert!(must_wait(9001));
        assert_eq!(asked.get(), 2, "every request is put to the guard");
        assert!(!must_wait(9002), "another window is not held");
        unregister(9001);
        assert!(!must_wait(9001));
    }

    #[test]
    fn a_guard_may_unregister_itself_while_it_runs() {
        register(
            9003,
            Rc::new(|| {
                unregister(9003);
                false
            }),
        );
        assert!(!must_wait(9003));
        assert!(!must_wait(9003));
    }
}
