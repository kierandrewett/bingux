//! Panics are never fatal.
//!
//! Upstream Freya lets a panic in a component, a handler, an effect or a task
//! unwind into winit, which aborts the process: one hook out of order and the
//! whole window is gone. In Bingux apps every place that runs app code catches
//! the panic instead (the patched `freya-core`, see `packages/vendor/README.md`)
//! and one of these happens:
//!
//! | What panicked | What happens |
//! |---|---|
//! | A component's `render` (any descendant of a boundary) | The nearest [`ErrorBoundary`] replaces its content with an error card (message, component path, Retry, Copy details). Retry mounts the content again in a new scope with fresh hook state. The rest of the window keeps running. |
//! | An event handler, an effect, a spawned task, a `use_drop` callback | The work is abandoned, the window shows a toast with Copy details, the effect keeps listening. |
//! | A worker thread (`spawn_blocking`, `spawn_detached`) | The pool survives, the failure is recorded and the window shows a toast. |
//! | The renderer or the runner itself | The window is laid out and rebuilt from the components' state on the next event. |
//! | Out of memory, a graphics driver that cannot be rebuilt, the framework failing five times in three seconds | The app starts a fresh copy of itself at the same location, once, and says why. |
//!
//! Boundaries exist by construction: around the window's root
//! ([`Application::run`](crate::app::Application::run)), every page, every
//! popup and sheet body and every dialog body. An app adds its own
//! with [`error_boundary`] around a part that may fail alone.
//!
//! Every failure is written to stderr with the component path and the panic
//! location (and a backtrace when `RUST_BACKTRACE` asks for one) and kept in a
//! ring buffer: [`recent_errors`], [`report_text`] and the inspector's
//! Performance tab read it.

mod boundary;
#[cfg(all(test, feature = "testing"))]
mod tests;
mod log;
mod relaunch;

use std::sync::Once;

pub use boundary::{ErrorBoundary, Look, error_boundary, error_boundary_with};
pub use log::{ErrorRecord, clear_errors, display_path, error_count, recent_errors, report_text, short_name};

pub(crate) use boundary::window_boundary;
pub(crate) use relaunch::note_location;

use freya::prelude::*;

/// Install the panic hook and the failure observer for this process. Idempotent.
/// [`Application::run`](crate::app::Application::run) and the headless
/// [`snapshot`](crate::snapshot) harness call it; a program that builds its
/// own runner calls it first.
pub fn install() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        freya_core::guard::install_panic_hook();
        freya_core::guard::set_observer(log::record);
        freya_core::guard::set_unrecoverable_handler(|failure| {
            relaunch::relaunch(failure);
        });
    });
}

/// [`install`], plus what only a real process needs: the relaunch plan and its
/// `SIGABRT` handler. Call before any other thread starts.
pub(crate) fn install_for_app() {
    install();
    relaunch::install();
}

/// The window's task for what happens off the UI thread: it shows the toast
/// for a worker thread's failure, and one for why this copy of the app was
/// restarted. Call from the window root; it uses a hook.
pub(crate) fn use_window_notices() {
    use_hook(|| {
        let restarted = relaunch::take_restart_reason();
        spawn(async move {
            if let Some(reason) = restarted {
                // The window must have drawn once to have somewhere to toast.
                timer(std::time::Duration::from_millis(700)).await;
                toast(reason);
            }
            while let Some(record) = log::next_thread_failure().await {
                toast(format!(
                    "Something went wrong in the background ({}): {}",
                    record.component,
                    record.message.lines().next().unwrap_or("")
                ));
            }
        });
    });
}

fn toast(text: String) {
    if let Some(overlays) = crate::nav::try_router().and_then(|router| router.overlays_handle()) {
        overlays.toast(text);
    }
}
