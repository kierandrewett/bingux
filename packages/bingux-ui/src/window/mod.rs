//! Windows: the main window and the popups that open over it, behind one
//! [`Window`] handle, like `window.open` in a browser or `GtkWindow`.
//!
//! [`WindowHost`] sits at the root (the [`OverlayHost`](crate::shell::OverlayHost)
//! wraps one) and owns the list of open popups. Every popup renders in a
//! [`PopupFrame`](popup) of its own, and the frame provides that popup's
//! `Window` as context. So [`use_window`] returns the popup from anywhere inside
//! its content, and the main window everywhere else. A popup has its own
//! scope: its tasks stop when it closes, it handles its own Escape and
//! Enter, and it sets its own size.
//!
//! Popups draw inside the parent surface. winit 0.30 cannot set an
//! xdg-toplevel parent on Wayland, so a native child window would be an
//! unparented toplevel. The `Window` API hides this, so the backend can change
//! without touching callers.

mod popup;

use freya::prelude::*;

use crate::address::Address;
use crate::shell::with_window;

pub use popup::{Popup, PopupAction, PopupActions, popup, use_popup_actions};
use popup::{PopupEntry, PopupFrame};

/// The open popups and the counter that names them. Copy, so a [`Window`]
/// can carry it into closures.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct WindowManager {
    popups: State<Vec<PopupEntry>>,
    next: State<u64>,
    /// The scope that owns the per-popup state, so it outlives the
    /// component that opened the popup.
    scope: ScopeId,
}

/// A handle to a window: the main window or a popup. Copy, cheap and valid
/// in closures.
#[derive(Clone, Copy, PartialEq)]
pub struct Window {
    manager: WindowManager,
    /// `None` is the main window.
    id: Option<u64>,
}

/// The window this component renders in: the popup whose content it is part
/// of, or the main window.
pub fn use_window() -> Window {
    use_consume::<Window>()
}

/// The main window, even from inside a popup.
pub fn use_main_window() -> Window {
    Window {
        manager: use_consume::<WindowManager>(),
        id: None,
    }
}

impl WindowManager {
    /// Start the exit of popup `id` and of every popup opened from it. Each
    /// frame removes itself when its exit has played.
    fn begin_close(self, id: u64) {
        let mut doomed = vec![id];
        {
            let popups = self.popups.peek();
            let mut index = 0;
            while index < doomed.len() {
                let parent = doomed[index];
                doomed.extend(
                    popups
                        .iter()
                        .filter(|entry| entry.parent == Some(parent))
                        .map(|entry| entry.id),
                );
                index += 1;
            }
        }
        let closings: Vec<State<bool>> = self
            .popups
            .peek()
            .iter()
            .filter(|entry| doomed.contains(&entry.id))
            .map(|entry| entry.closing)
            .collect();
        for mut closing in closings {
            closing.set_if_modified(true);
        }
    }

    /// Drop popup `id` once its exit has finished.
    pub(crate) fn remove(self, id: u64) {
        let mut popups = self.popups;
        popups.write().retain(|entry| entry.id != id);
    }
}

impl Window {
    /// The popup entry behind this handle, `None` for the main window or a
    /// popup that has gone. Does not subscribe the caller.
    pub(crate) fn entry(self) -> Option<PopupEntry> {
        let id = self.id?;
        self.manager
            .popups
            .peek()
            .iter()
            .find(|entry| entry.id == id)
            .cloned()
    }

    /// Open a popup modal to this window and return its handle. A popup can
    /// open a popup: `self` is then the parent, and closing it closes the
    /// child too.
    pub fn open(self, popup: Popup) -> Window {
        let mut manager = self.manager;
        let id = *manager.next.peek();
        manager.next.set(id + 1);
        let entry = PopupEntry::new(manager.scope, id, self.id, popup);
        manager.popups.write().push(entry);
        Window {
            manager,
            id: Some(id),
        }
    }

    /// Close this window. A popup animates out and its tasks stop; the main
    /// window closes the application window.
    pub fn close(self) {
        match self.id {
            Some(id) => self.manager.begin_close(id),
            None => Platform::get().close_window(Platform::window_id()),
        }
    }

    /// Change the size. A popup animates to it and stays inside its parent
    /// with a margin; the main window asks the compositor.
    pub fn resize_to(self, width: f32, height: f32) {
        match self.entry() {
            Some(entry) => {
                let mut size = entry.size;
                size.set_if_modified((width, height));
            }
            None if self.id.is_none() => with_window(move |window| {
                let _ = window.request_inner_size(freya::winit::dpi::LogicalSize::new(
                    f64::from(width),
                    f64::from(height),
                ));
            }),
            None => {}
        }
    }

    /// Set the title: a popup's header text, or the main window's title.
    pub fn set_title(self, title: impl Into<String>) {
        let title = title.into();
        match self.entry() {
            Some(entry) => {
                let mut state = entry.title;
                state.set_if_modified(title);
            }
            None if self.id.is_none() => with_window(move |window| window.set_title(&title)),
            None => {}
        }
    }

    /// The size: a popup's requested size (before it is clamped to the
    /// parent), or the main window's size. Reading it in a component
    /// subscribes the component.
    pub fn size(self) -> (f32, f32) {
        match self.id {
            Some(_) => self
                .manager
                .popups
                .read()
                .iter()
                .find(|entry| Some(entry.id) == self.id)
                .map_or((0.0, 0.0), |entry| *entry.size.read()),
            None => {
                let size = *Platform::get().current_window().root_size.read();
                (size.width, size.height)
            }
        }
    }

    pub fn is_popup(self) -> bool {
        self.id.is_some()
    }

    /// The window this popup opened from. `None` for the main window.
    pub fn parent(self) -> Option<Window> {
        let entry = self.entry()?;
        Some(Window {
            manager: self.manager,
            id: entry.parent,
        })
    }

    /// The address the popup was opened with, if it has one.
    pub fn address(self) -> Option<Address> {
        self.entry()?.address
    }

    /// Minimise the main window. A popup ignores it.
    pub fn minimize(self) {
        if self.id.is_none() {
            with_window(|window| window.set_minimized(true));
        }
    }

    /// Maximise the main window, or restore it. A popup ignores it.
    pub fn toggle_maximize(self) {
        if self.id.is_none() {
            with_window(|window| window.set_maximized(!window.is_maximized()));
        }
    }
}

/// Provides the main [`Window`] and draws every open popup over `child`.
#[derive(PartialEq)]
pub struct WindowHost {
    child: Element,
}

impl WindowHost {
    pub fn new(child: impl Into<Element>) -> Self {
        Self {
            child: child.into(),
        }
    }
}

impl Component for WindowHost {
    fn render(&self) -> impl IntoElement {
        let scope = current_scope_id();
        let manager = use_hook(move || WindowManager {
            popups: State::create(Vec::new()),
            next: State::create(0),
            scope,
        });
        use_provide_context(move || manager);
        use_provide_context(move || Window { manager, id: None });
        let popups = manager.popups.read().clone();

        rect().expanded().child(self.child.clone()).children(
            popups
                .into_iter()
                .map(|entry| PopupFrame { manager, entry }),
        )
    }
}
