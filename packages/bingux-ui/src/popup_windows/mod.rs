//! Popup windows: menus, dropdown lists and popovers as windows of their own.
//!
//! An overlay drawn inside its window is clipped by the window: a context menu near the bottom of
//! the Files window covers a strip of the file list and stops at the window's edge, however much
//! room the screen has. GTK draws every menu and popover as a window the compositor places, so it
//! can leave the window; this module does the same.
//!
//! # The route
//!
//! Every popup still goes through [`Panel`](crate::shell::panel::Panel), the one floating
//! surface, and through [`Overlays`](crate::shell::Overlays) for menus. Callers do not change.
//! `Panel` chooses a route when it renders (see [`route`]):
//!
//! | Surface | Route |
//! |---|---|
//! | Menu, dropdown list, popover, submenu | **Native**: an `xdg_popup` window, when the process runs on Wayland and the window is not a phone-sized one |
//! | Choice on a phone-sized window | In-window sheet from the bottom edge |
//! | Dialog (About, confirmations, forms) | In-window modal card over a scrim: a dialog belongs to its window, as in libadwaita |
//! | Text edit bar | In-window, pinned to the caret it edits |
//! | Everything on X11, in a snapshot or a test, or when the compositor refuses a popup | In-window, as before |
//!
//! # The pieces
//!
//! - [`place`]: what a popup is placed against (`xdg_positioner`: anchor rectangle, anchor,
//!   gravity, constraint adjustment) and a reference implementation of how a compositor answers,
//!   for tests and for the geometry the menu state machine assumes.
//! - [`backend`]: [`PopupBackend`](backend::PopupBackend), the seam between the library and the
//!   windowing system. [`WaylandBackend`](backend::WaylandBackend) opens real windows through
//!   Freya; [`FakeBackend`](backend::FakeBackend) records calls for tests.
//! - [`portal`]: [`NativePopup`](portal::NativePopup), the lifecycle of one popup as seen from
//!   the `Panel` that asked for it, and the root component of the popup's own window.
//!
//! # How the content gets there
//!
//! The popup's window has its own Freya runner. `Panel` builds the card as it always did, in its
//! own window's runner, and hands the element to the popup ([`PopupContent`]); the popup's root
//! renders it. The element's components run in the popup's runner and read the same contexts as
//! the window that opened it: the root takes a [`ContextSnapshot`] of every context visible at
//! the `Panel` (tokens, direction, router, overlays) and provides them to the popup's tree. The
//! state is shared too: a `State` is a handle, so a menu row in the popup writes the same
//! highlight the window reads.
//!
//! The window the popup grew from stays in charge of the keyboard and of dismissal. It keeps the
//! clear shield over its content (a press on it dismisses, and swallows the press as an in-window
//! menu does) and the focus node the menu's key handler hangs on; the renderer delivers the keys
//! the compositor gives the popup to that window. The compositor's popup grab dismisses the
//! popup on a press anywhere that is not part of the app.

pub(crate) mod backend;
pub(crate) mod place;
pub(crate) mod portal;

use std::cell::RefCell;
use std::rc::Rc;

#[cfg(all(test, feature = "testing"))]
mod route_tests;
use freya::prelude::*;

pub(crate) use backend::{PopupBackend, WaylandBackend};
#[cfg(test)]
pub(crate) use backend::FakeBackend;

/// Logical pixels of transparent surface around a popup's card, for its shadow. The card's
/// `Raised` elevation reaches 32 px below it.
pub(crate) const MARGIN: f32 = 36.0;

/// What a popup's components add to a pointer position of their window to get the position
/// the window that opened the popup would have given it: the menu state machine thinks in the
/// coordinates its menu was opened in.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct PopupOrigin(pub(crate) (f32, f32));

/// `point` from an event in this tree, in the coordinates of the window that opened the menu.
pub(crate) fn to_opener_space(point: (f32, f32)) -> (f32, f32) {
    match try_consume_context::<PopupOrigin>() {
        Some(PopupOrigin(origin)) => (point.0 + origin.0, point.1 + origin.1),
        None => point,
    }
}

/// Whether a tree is the content of a popup window.
pub(crate) fn in_popup_window() -> bool {
    try_consume_context::<freya_winit::popup::PopupMarker>().is_some()
}

thread_local! {
    static BACKEND: RefCell<Option<Rc<dyn PopupBackend>>> = const { RefCell::new(None) };
    static REFUSED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Installs `backend` for this thread. A test installs a [`FakeBackend`]; the app never calls
/// this, [`backend`] makes the Wayland one on first use.
pub(crate) fn set_backend(backend: Option<Rc<dyn PopupBackend>>) {
    BACKEND.with(|slot| *slot.borrow_mut() = backend);
}

/// The backend popups open through, or `None` when this process has none: X11, a headless
/// snapshot, or a compositor that refused a popup earlier.
pub(crate) fn backend() -> Option<Rc<dyn PopupBackend>> {
    if REFUSED.with(std::cell::Cell::get) {
        return None;
    }
    BACKEND.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_none() && freya_winit::popup::supported() {
            *slot = Some(Rc::new(WaylandBackend::default()));
        }
        slot.clone().filter(|backend| backend.available())
    })
}

/// The backend whether or not popups are being refused, to close what it opened.
pub(crate) fn raw_backend() -> Option<Rc<dyn PopupBackend>> {
    BACKEND.with(|slot| slot.borrow().clone())
}

/// A compositor that refuses a popup refuses the next one: the rest of the session draws them
/// in their windows.
pub(crate) fn note_refused() {
    REFUSED.with(|refused| {
        if !refused.replace(true) {
            eprintln!("[popups] the compositor refused a popup window: popups stay inside their windows");
        }
    });
}

/// Forget a refusal and the backend (tests).
#[cfg(test)]
pub(crate) fn reset() {
    REFUSED.with(|refused| refused.set(false));
    set_backend(None);
}

/// The element a popup's window shows, which the window that opened it keeps up to date.
///
/// A popup's runner cannot call into the opener's, so the content is a cell both see: the opener
/// writes the element and wakes the popup's root, which reads it when it renders.
#[derive(Clone)]
pub(crate) struct PopupContent {
    inner: Rc<RefCell<ContentInner>>,
}

struct ContentInner {
    element: Element,
    /// A state of the popup's runner the root reads, so writing it redraws the popup.
    wake: Option<State<u64>>,
}

impl PopupContent {
    pub(crate) fn new(element: Element) -> Self {
        Self {
            inner: Rc::new(RefCell::new(ContentInner { element, wake: None })),
        }
    }

    /// Replaces what the popup shows and redraws it.
    pub(crate) fn publish(&self, element: Element) {
        let wake = {
            let mut inner = self.inner.borrow_mut();
            inner.element = element;
            inner.wake
        };
        // The popup's state is gone once its window closed; there is then nothing to wake.
        if let Some(mut wake) = wake
            && let Some(mut count) = wake.try_write()
        {
            *count = count.wrapping_add(1);
        }
    }

    pub(crate) fn element(&self) -> Element {
        self.inner.borrow().element.clone()
    }

    fn bind(&self, wake: State<u64>) {
        self.inner.borrow_mut().wake = Some(wake);
    }
}
