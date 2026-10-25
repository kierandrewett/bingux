//! Windows: the main window and the popups that open over it, behind one
//! family of traits, the way `GtkWindow` and its dialogs share `GtkWindow`.
//!
//! - [`WindowLike`] is the base every window has: close, resize, title, size,
//!   parent and address.
//! - [`WindowParent`] adds [`open`](WindowParent::open): a popup can open
//!   popups, and closing it closes them too.
//! - [`Minimizable`] and [`Maximizable`] belong to the top-level window only,
//!   so calling them on a popup does not compile.
//!
//! [`MainWindow`] is the top-level window, backed by winit through Freya's
//! platform. [`PopupWindow`] lives inside it: it owns its size, its
//! presentation and its [`PopupFlags`]. [`Window`] is the `Copy` handle that
//! components receive, and it implements the base traits by dispatching to
//! whichever kind it holds, so callers never match on it.
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
//!
//! On a narrow window (a phone) every popup is a sheet from the bottom edge,
//! see [`PopupFlags`]. [`WindowHost`] makes that choice from the window width.

mod popup;

use freya::animation::*;
use freya::prelude::*;

use crate::address::Address;
use crate::shell::with_window;
use crate::style::{Paint, Radius, Tw, bg, clip, rounded_scaled};
use crate::surface::use_tokens;
use crate::theme::NARROW_BELOW;

use popup::{MOTION_MS, PopupEntry, PopupFrame};
pub use popup::{Popup, PopupAction, PopupActions, PopupFlags, popup, use_popup_actions};

/// How far the content shrinks for a popup with [`PopupFlags::INSET_VIEWPORT`]:
/// the share of the window it keeps. The card it becomes has the `Card` radius.
const INSET_SCALE: f32 = 0.92;

/// The open popups and the counter that names them. Copy, so a window handle
/// can carry it into closures.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct WindowManager {
    popups: State<Vec<PopupEntry>>,
    next: State<u64>,
    /// The scope that owns the per-popup state, so it outlives the
    /// component that opened the popup.
    scope: ScopeId,
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

    /// Add a popup under `parent` (`None` for the main window).
    fn open(self, parent: Option<u64>, popup: Popup) -> PopupWindow {
        let mut manager = self;
        let id = *manager.next.peek();
        manager.next.set(id + 1);
        let entry = PopupEntry::new(manager.scope, id, parent, popup);
        manager.popups.write().push(entry);
        PopupWindow { manager, id }
    }
}

/// What every window can do. Components reach it through [`use_window`], and
/// the trait is what they import to call it.
pub trait WindowLike: Copy {
    /// Close this window. A popup animates out and its tasks stop; the main
    /// window closes the application window.
    fn close(self);

    /// Change the size. A popup animates to it and stays between its minimum
    /// and the window less its margin; the main window asks the compositor.
    /// Dragged edges call this too, so both kinds of resize clamp the same way.
    fn resize_to(self, width: f32, height: f32);

    /// Set the title: a popup's header text, or the main window's title.
    fn set_title(self, title: impl Into<String>);

    /// The size. Reading it in a component subscribes the component.
    fn size(self) -> (f32, f32);

    fn is_popup(self) -> bool;

    /// The window this one opened from. `None` for the main window.
    fn parent(self) -> Option<Window>;

    /// The address the popup was opened with, if it has one.
    fn address(self) -> Option<Address>;
}

/// A window that can open popups.
pub trait WindowParent: WindowLike {
    /// Open a popup modal to this window and return its handle. The popup's
    /// parent is this window, so closing this window closes the popup too.
    fn open(self, popup: Popup) -> PopupWindow;
}

/// A window the user can minimise. Only the top-level window is one.
pub trait Minimizable: WindowLike {
    fn minimize(self);
}

/// A window the user can maximise or restore. Only the top-level window is one.
pub trait Maximizable: WindowLike {
    fn toggle_maximize(self);
}

/// The top-level window, backed by winit through Freya's platform.
#[derive(Clone, Copy, PartialEq)]
pub struct MainWindow {
    manager: WindowManager,
}

/// A popup: a modal panel inside the main window, or inside the popup it
/// opened from. Copy, cheap and valid in closures.
#[derive(Clone, Copy, PartialEq)]
pub struct PopupWindow {
    manager: WindowManager,
    /// The id of the popup's entry in the manager.
    id: u64,
}

/// A handle to any window. Copy, cheap and valid in closures. Components get
/// it from [`use_window`] and call the [`WindowLike`] methods on it directly.
#[derive(Clone, Copy, PartialEq)]
pub enum Window {
    Main(MainWindow),
    Popup(PopupWindow),
}

/// The window this component renders in: the popup whose content it is part
/// of, or the main window.
pub fn use_window() -> Window {
    use_consume::<Window>()
}

/// The main window, even from inside a popup.
pub fn use_main_window() -> MainWindow {
    MainWindow {
        manager: use_consume::<WindowManager>(),
    }
}

impl WindowLike for MainWindow {
    fn close(self) {
        Platform::get().close_window(Platform::window_id());
    }

    fn resize_to(self, width: f32, height: f32) {
        with_window(move |window| {
            let _ = window.request_inner_size(freya::winit::dpi::LogicalSize::new(
                f64::from(width),
                f64::from(height),
            ));
        });
    }

    fn set_title(self, title: impl Into<String>) {
        let title = title.into();
        with_window(move |window| window.set_title(&title));
    }

    fn size(self) -> (f32, f32) {
        let size = *Platform::get().current_window().root_size.read();
        (size.width, size.height)
    }

    fn is_popup(self) -> bool {
        false
    }

    fn parent(self) -> Option<Window> {
        None
    }

    fn address(self) -> Option<Address> {
        None
    }
}

impl WindowParent for MainWindow {
    fn open(self, popup: Popup) -> PopupWindow {
        self.manager.open(None, popup)
    }
}

impl Minimizable for MainWindow {
    fn minimize(self) {
        with_window(|window| window.set_minimized(true));
    }
}

impl Maximizable for MainWindow {
    fn toggle_maximize(self) {
        with_window(|window| window.set_maximized(!window.is_maximized()));
    }
}

impl PopupWindow {
    /// The popup entry behind this handle, `None` once the popup has gone.
    /// Does not subscribe the caller.
    pub(crate) fn entry(self) -> Option<PopupEntry> {
        self.manager
            .popups
            .peek()
            .iter()
            .find(|entry| entry.id == self.id)
            .cloned()
    }
}

impl WindowLike for PopupWindow {
    fn close(self) {
        self.manager.begin_close(self.id);
    }

    fn resize_to(self, width: f32, height: f32) {
        if let Some(entry) = self.entry() {
            let mut size = entry.size;
            size.set_if_modified(entry.clamp((width, height)));
        }
    }

    fn set_title(self, title: impl Into<String>) {
        if let Some(entry) = self.entry() {
            let mut state = entry.title;
            state.set_if_modified(title.into());
        }
    }

    fn size(self) -> (f32, f32) {
        self.manager
            .popups
            .read()
            .iter()
            .find(|entry| entry.id == self.id)
            .map_or((0.0, 0.0), |entry| *entry.size.read())
    }

    fn is_popup(self) -> bool {
        true
    }

    fn parent(self) -> Option<Window> {
        let entry = self.entry()?;
        Some(match entry.parent {
            Some(id) => Window::Popup(PopupWindow {
                manager: self.manager,
                id,
            }),
            None => Window::Main(MainWindow {
                manager: self.manager,
            }),
        })
    }

    fn address(self) -> Option<Address> {
        self.entry()?.address
    }
}

impl WindowParent for PopupWindow {
    fn open(self, popup: Popup) -> PopupWindow {
        self.manager.open(Some(self.id), popup)
    }
}

impl From<MainWindow> for Window {
    fn from(window: MainWindow) -> Self {
        Window::Main(window)
    }
}

impl From<PopupWindow> for Window {
    fn from(window: PopupWindow) -> Self {
        Window::Popup(window)
    }
}

impl Window {
    /// The main window, if this handle is it.
    pub fn as_main(self) -> Option<MainWindow> {
        match self {
            Window::Main(window) => Some(window),
            Window::Popup(_) => None,
        }
    }
}

impl WindowLike for Window {
    fn close(self) {
        match self {
            Window::Main(window) => window.close(),
            Window::Popup(window) => window.close(),
        }
    }

    fn resize_to(self, width: f32, height: f32) {
        match self {
            Window::Main(window) => window.resize_to(width, height),
            Window::Popup(window) => window.resize_to(width, height),
        }
    }

    fn set_title(self, title: impl Into<String>) {
        match self {
            Window::Main(window) => window.set_title(title),
            Window::Popup(window) => window.set_title(title),
        }
    }

    fn size(self) -> (f32, f32) {
        match self {
            Window::Main(window) => window.size(),
            Window::Popup(window) => window.size(),
        }
    }

    fn is_popup(self) -> bool {
        match self {
            Window::Main(window) => window.is_popup(),
            Window::Popup(window) => window.is_popup(),
        }
    }

    fn parent(self) -> Option<Window> {
        match self {
            Window::Main(window) => window.parent(),
            Window::Popup(window) => window.parent(),
        }
    }

    fn address(self) -> Option<Address> {
        match self {
            Window::Main(window) => window.address(),
            Window::Popup(window) => window.address(),
        }
    }
}

impl WindowParent for Window {
    fn open(self, popup: Popup) -> PopupWindow {
        match self {
            Window::Main(window) => window.open(popup),
            Window::Popup(window) => window.open(popup),
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
        let tokens = use_tokens();
        let scope = current_scope_id();
        let manager = use_hook(move || WindowManager {
            popups: State::create(Vec::new()),
            next: State::create(0),
            scope,
        });
        use_provide_context(move || manager);
        use_provide_context(move || Window::Main(MainWindow { manager }));

        // The window's width is the size class: below the narrow breakpoint
        // the window is a phone, and popups are sheets.
        let narrow = Platform::get().current_window().root_size.read().width < NARROW_BELOW;

        let popups = manager.popups.read().clone();

        // A popup that asks for the whole viewport shrinks the content into a
        // rounded card on black, and the card springs back when it closes.
        let inset = popups.iter().any(|entry| {
            entry.flags.contains(PopupFlags::INSET_VIEWPORT) && !*entry.closing.read()
        });
        let motion = tokens.motion(MOTION_MS);
        let shown = use_animation_transition(
            if inset { 1.0_f32 } else { 0.0 },
            move |from: f32, to: f32| {
                AnimNum::new(from, to)
                    .time(motion)
                    .function(Function::Quad)
                    .ease(Ease::Out)
            },
        )
        .get()
        .value();

        // The card is the same element whether or not a popup insets it, so
        // the content keeps its state when the popup opens and closes.
        rect()
            .expanded()
            .tw(&tokens, &[bg(Paint::Backdrop)])
            .child(
                rect()
                    .expanded()
                    .scale(1.0 - (1.0 - INSET_SCALE) * shown)
                    .tw(&tokens, &[rounded_scaled(Radius::Card, shown), clip()])
                    .child(self.child.clone()),
            )
            .children(popups.into_iter().map(|entry| PopupFrame {
                manager,
                entry,
                sheet: narrow,
            }))
    }
}
