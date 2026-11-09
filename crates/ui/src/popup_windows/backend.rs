//! The seam between popups and the windowing system.
//!
//! [`PopupBackend`] is what the library needs: open a window against a parent, resize it, close
//! it, and hear back when it exists, when it was refused and when the compositor dismissed it.
//! [`WaylandBackend`] does it with Freya windows made as `xdg_popup`s (see
//! `vendor/README.md`, winit's `with_popup`); [`FakeBackend`] records the calls and
//! lets a test play the compositor.

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::rc::Rc;
use std::task::{Poll, Waker};

use freya::prelude::*;
use freya::winit::window::WindowId;
use freya_winit::config::{CloseDecision, WindowConfig};
use freya_winit::{WinitPlatformExt, popup};
use std::collections::HashMap;

use super::place::{Adjust, Edge, PopupRequest};
use super::{MARGIN, PopupContent, PopupOrigin};

/// Names one popup to the backend that opened it.
pub(crate) type Token = u64;

/// What happens to a popup that the window it came from has to hear about.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum PopupEvent {
    /// The window exists and is showing.
    Ready,
    /// The compositor would not make it. The content is drawn in the window instead.
    Refused,
    /// The compositor took it away: a press elsewhere, the parent unmapped, another grab.
    Dismissed,
}

/// What a popup is a popup of.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Parent {
    /// A window of the app: its anchor is in that window's coordinates.
    Window(WindowId),
    /// Another popup: its anchor is in that popup's card coordinates.
    Popup(Token),
}

/// Everything the backend needs to make one popup.
pub(crate) struct OpenRequest {
    pub(crate) placement: PopupRequest,
    pub(crate) parent: Parent,
    pub(crate) content: PopupContent,
    /// What the popup's tree reads: the contexts visible where the popup was asked for.
    pub(crate) contexts: ContextSnapshot,
    /// Added to a pointer position of the popup to get one in the opener's coordinates.
    pub(crate) origin: (f32, f32),
    pub(crate) on_event: Rc<dyn Fn(PopupEvent)>,
}

pub(crate) trait PopupBackend {
    /// Whether popups can be opened now.
    fn available(&self) -> bool;
    /// Starts opening a popup; `request.on_event` hears `Ready` or `Refused` once. Call from a
    /// component's render or handler: the backend spawns a task.
    fn open(&self, request: OpenRequest) -> Token;
    /// Gives the popup's card a new size.
    fn resize(&self, token: Token, size: (f32, f32));
    /// Closes the popup and every popup opened from it, the newest first.
    fn close(&self, token: Token);
    /// The popup's window, once it exists.
    fn window(&self, token: Token) -> Option<WindowId>;
}

/// A value that arrives later, awaited by the popups waiting on it.
struct Ready<T: Copy> {
    value: Cell<Option<T>>,
    wakers: RefCell<Vec<Waker>>,
}

impl<T: Copy> Ready<T> {
    fn new() -> Self {
        Self {
            value: Cell::new(None),
            wakers: RefCell::new(Vec::new()),
        }
    }

    fn set(&self, value: T) {
        self.value.set(Some(value));
        for waker in self.wakers.borrow_mut().drain(..) {
            waker.wake();
        }
    }

    fn get(&self) -> Option<T> {
        self.value.get()
    }

    /// Wakes the waiters without a value: the thing they wait for will not come.
    fn give_up(&self) {
        for waker in self.wakers.borrow_mut().drain(..) {
            waker.wake();
        }
    }

    fn wait(&self) -> impl Future<Output = Option<T>> + '_ {
        std::future::poll_fn(move |cx| match self.value.get() {
            Some(value) => Poll::Ready(Some(value)),
            None => {
                self.wakers.borrow_mut().push(cx.waker().clone());
                Poll::Pending
            }
        })
    }
}

struct Entry {
    window: Rc<Ready<WindowId>>,
    parent: Option<Token>,
    /// The window a root popup opened from; `None` for a popup of a popup.
    opener: Option<WindowId>,
    /// Told `Dismissed` when a newer root popup of the same window takes this one away.
    on_event: Rc<dyn Fn(PopupEvent)>,
    /// Closed (or refused) before or after it opened: nothing is to be done with its window.
    closed: Cell<bool>,
    /// Taken away by a newer root popup: it hears `Dismissed` from that, never `Refused`.
    displaced: Cell<bool>,
}

/// Opens popups as Freya windows made as `xdg_popup`s.
#[derive(Default)]
pub(crate) struct WaylandBackend {
    entries: Rc<RefCell<HashMap<Token, Rc<Entry>>>>,
    next: Cell<Token>,
}

impl WaylandBackend {
    /// `token` and every popup opened from it, the newest first: a popup is destroyed before the
    /// popup it grew from.
    fn subtree(&self, token: Token) -> Vec<Token> {
        let entries = self.entries.borrow();
        let mut order = vec![token];
        let mut index = 0;
        while index < order.len() {
            let parent = order[index];
            let mut children: Vec<Token> = entries
                .iter()
                .filter(|(_, entry)| entry.parent == Some(parent))
                .map(|(child, _)| *child)
                .collect();
            children.sort_unstable();
            order.extend(children);
            index += 1;
        }
        order.reverse();
        order
    }

    /// Closes the root popups still open on `opener`, and the popups opened from them. Returns
    /// the `on_event` of each popup closed, so each hears `Dismissed` once the new popup is asked
    /// for. Their windows are closed here, before the new one is launched.
    fn displace(&self, opener: Option<WindowId>) -> Vec<Rc<dyn Fn(PopupEvent)>> {
        let Some(window) = opener else {
            return Vec::new();
        };
        let roots: Vec<Token> = self
            .entries
            .borrow()
            .iter()
            .filter(|(_, entry)| entry.opener == Some(window) && !entry.closed.get())
            .map(|(token, _)| *token)
            .collect();
        let mut listeners = Vec::new();
        for root in roots {
            for token in self.subtree(root) {
                let entries = self.entries.borrow();
                let Some(entry) = entries.get(&token).filter(|entry| !entry.closed.get()) else {
                    continue;
                };
                entry.displaced.set(true);
                listeners.push(entry.on_event.clone());
            }
            self.close(root);
        }
        listeners
    }
}

impl PopupBackend for WaylandBackend {
    fn available(&self) -> bool {
        popup::supported()
    }

    fn open(&self, request: OpenRequest) -> Token {
        let token = self.next.get() + 1;
        self.next.set(token);
        let window = Rc::new(Ready::new());
        let parent_token = match request.parent {
            Parent::Popup(parent) => Some(parent),
            Parent::Window(_) => None,
        };
        let opener = match request.parent {
            Parent::Window(id) => Some(id),
            Parent::Popup(_) => None,
        };
        // The compositor keeps one grabbing popup chain per toplevel and refuses a second while
        // the first is up: a root popup still open on this window is taken away first.
        let displaced = self.displace(opener);
        let entry = Rc::new(Entry {
            window: window.clone(),
            parent: parent_token,
            opener,
            on_event: request.on_event.clone(),
            closed: Cell::new(false),
            displaced: Cell::new(false),
        });
        self.entries.borrow_mut().insert(token, entry.clone());
        register(token, request.parent);
        let entries = self.entries.clone();
        if !displaced.is_empty() {
            spawn_global(async move {
                for listener in displaced {
                    listener(PopupEvent::Dismissed);
                }
            });
        }
        let OpenRequest {
            placement,
            parent,
            content,
            contexts,
            origin,
            on_event,
        } = request;

        // A popup is made by the event loop, a moment after it is asked for. The task is not
        // tied to the component that asked: the window is made either way, and the task is what
        // closes it if the component is gone by then.
        spawn_global(async move {
            let parent_window = match parent {
                Parent::Window(id) => Some(id),
                Parent::Popup(parent) => {
                    let waiting = entries
                        .borrow()
                        .get(&parent)
                        .map(|entry| entry.window.clone());
                    match waiting {
                        Some(ready) => ready.wait().await,
                        None => None,
                    }
                }
            };
            let Some(parent_window) = parent_window.filter(|_| !entry.closed.get()) else {
                entry.closed.set(true);
                window.give_up();
                if !entry.displaced.get() {
                    on_event(PopupEvent::Refused);
                }
                return;
            };

            let size = (
                placement.size.0.round().max(1.0) as u32,
                placement.size.1.round().max(1.0) as u32,
            );
            let attributes = popup::PopupAttributes {
                parent: parent_window,
                anchor_rect: (
                    placement.anchor.x.round() as i32,
                    placement.anchor.y.round() as i32,
                    placement.anchor.width.round().max(1.0) as i32,
                    placement.anchor.height.round().max(1.0) as i32,
                ),
                anchor: winit_edge(placement.edge),
                gravity: winit_edge(placement.gravity),
                offset: (0, 0),
                adjust: winit_adjust(placement.adjust),
                size,
                margin: MARGIN as i32,
                grab: true,
                reactive: false,
            };
            let dismissed = on_event.clone();
            let entry_for_close = entry.clone();
            let config = WindowConfig::new(move || portal_root(&content, &contexts, origin, token))
                .with_popup(attributes)
                // ui-lint: allow(the window itself must be fully transparent: the popup's card is drawn inside it)
                .with_background(Color::TRANSPARENT)
                .with_title("")
                .with_on_close(move |_, _| {
                    // The compositor is done with it; the window goes.
                    entry_for_close.closed.set(true);
                    dismissed(PopupEvent::Dismissed);
                    CloseDecision::Close
                });

            match Platform::get().try_launch_window(config).await {
                Some(id) if !entry.closed.get() => {
                    window.set(id);
                    on_event(PopupEvent::Ready);
                }
                Some(id) => {
                    // Closed while it was being made.
                    Platform::get().close_window(id);
                    window.give_up();
                }
                None => {
                    entry.closed.set(true);
                    window.give_up();
                    on_event(PopupEvent::Refused);
                }
            }
        });
        token
    }

    fn resize(&self, token: Token, size: (f32, f32)) {
        let Some(id) = self.window(token) else {
            return;
        };
        let margin = MARGIN as f64;
        Platform::get().with_window(id, move |window| {
            let _ = window.request_inner_size(freya::winit::dpi::LogicalSize::new(
                f64::from(size.0.round()) + 2.0 * margin,
                f64::from(size.1.round()) + 2.0 * margin,
            ));
        });
    }

    fn close(&self, token: Token) {
        let doomed = self.subtree(token);
        for token in doomed {
            let entry = self.entries.borrow().get(&token).cloned();
            let Some(entry) = entry else { continue };
            let was_open = !entry.closed.replace(true);
            if was_open && let Some(id) = entry.window.get() {
                Platform::get().close_window(id);
            }
            entry.window.give_up();
            self.entries.borrow_mut().remove(&token);
            crate::layers::with_stack(|stack| stack.close(stack_id(token)));
        }
    }

    fn window(&self, token: Token) -> Option<WindowId> {
        self.entries
            .borrow()
            .get(&token)
            .and_then(|entry| entry.window.get())
    }
}

/// Records a popup in the window stack, above its parent.
fn register(token: Token, parent: Parent) {
    crate::layers::with_stack(|stack| {
        let parent_id = match parent {
            Parent::Window(window) => {
                let id = u64::from(window);
                if !stack.contains(id) {
                    stack.open(
                        id,
                        crate::layers::WindowKind::Toplevel,
                        None,
                        false,
                        "window",
                    );
                }
                id
            }
            Parent::Popup(parent) => stack_id(parent),
        };
        stack.open(
            stack_id(token),
            crate::layers::WindowKind::Popup,
            Some(parent_id),
            false,
            "popup",
        );
    });
}

fn unregister(token: Token) {
    crate::layers::with_stack(|stack| stack.close(stack_id(token)));
}

/// A popup's id in the window stack: popups are not winit windows until they open, so they are
/// numbered apart from them.
pub(super) fn stack_id(token: Token) -> u64 {
    (1 << 62) | token
}

fn portal_root(
    content: &PopupContent,
    contexts: &ContextSnapshot,
    origin: (f32, f32),
    token: Token,
) -> Element {
    super::portal::popup_root(content, contexts, PopupOrigin(origin), token)
}

fn winit_edge(edge: Edge) -> popup::PopupEdge {
    match edge {
        Edge::Centre => popup::PopupEdge::None,
        Edge::Top => popup::PopupEdge::Top,
        Edge::Bottom => popup::PopupEdge::Bottom,
        Edge::Left => popup::PopupEdge::Left,
        Edge::Right => popup::PopupEdge::Right,
        Edge::TopLeft => popup::PopupEdge::TopLeft,
        Edge::TopRight => popup::PopupEdge::TopRight,
        Edge::BottomLeft => popup::PopupEdge::BottomLeft,
        Edge::BottomRight => popup::PopupEdge::BottomRight,
    }
}

fn winit_adjust(adjust: Adjust) -> popup::PopupAdjust {
    popup::PopupAdjust {
        slide_x: adjust.slide_x,
        slide_y: adjust.slide_y,
        flip_x: adjust.flip_x,
        flip_y: adjust.flip_y,
        resize_x: false,
        resize_y: false,
    }
}

/// What a [`FakeBackend`] was asked, in order.
#[derive(Clone, PartialEq, Debug)]
pub(crate) enum Call {
    Open {
        token: Token,
        placement: PopupRequest,
        parent: Parent,
    },
    Resize {
        token: Token,
        size: (f32, f32),
    },
    Close {
        token: Token,
    },
}

/// A backend with no windows: it records what it is asked and answers `Ready` at once, or
/// `Refused` when told to. A test plays the compositor with [`dismiss`](Self::dismiss).
#[derive(Default)]
pub(crate) struct FakeBackend {
    calls: RefCell<Vec<Call>>,
    handlers: RefCell<HashMap<Token, Rc<dyn Fn(PopupEvent)>>>,
    parents: RefCell<HashMap<Token, Parent>>,
    next: Cell<Token>,
    refuse: Cell<bool>,
    unavailable: Cell<bool>,
}

impl FakeBackend {
    pub(crate) fn calls(&self) -> Vec<Call> {
        self.calls.borrow().clone()
    }

    pub(crate) fn clear_calls(&self) {
        self.calls.borrow_mut().clear();
    }

    /// The next popups are refused.
    pub(crate) fn refuse_next(&self, refuse: bool) {
        self.refuse.set(refuse);
    }

    pub(crate) fn set_unavailable(&self, unavailable: bool) {
        self.unavailable.set(unavailable);
    }

    /// The compositor dismisses `token`, as it does on a press elsewhere.
    pub(crate) fn dismiss(&self, token: Token) {
        let handler = self.handlers.borrow().get(&token).cloned();
        if let Some(handler) = handler {
            handler(PopupEvent::Dismissed);
        }
    }

    /// The tokens that are open (opened and not closed), oldest first.
    pub(crate) fn open_tokens(&self) -> Vec<Token> {
        let mut tokens: Vec<Token> = self.handlers.borrow().keys().copied().collect();
        tokens.sort_unstable();
        tokens
    }
}

impl PopupBackend for FakeBackend {
    fn available(&self) -> bool {
        !self.unavailable.get()
    }

    fn open(&self, request: OpenRequest) -> Token {
        let token = self.next.get() + 1;
        self.next.set(token);
        self.calls.borrow_mut().push(Call::Open {
            token,
            placement: request.placement,
            parent: request.parent,
        });
        self.parents.borrow_mut().insert(token, request.parent);
        if !self.refuse.get() {
            register(token, request.parent);
        }
        if self.refuse.get() {
            (request.on_event)(PopupEvent::Refused);
        } else {
            self.handlers
                .borrow_mut()
                .insert(token, request.on_event.clone());
            (request.on_event)(PopupEvent::Ready);
        }
        token
    }

    fn resize(&self, token: Token, size: (f32, f32)) {
        self.calls.borrow_mut().push(Call::Resize { token, size });
    }

    fn close(&self, token: Token) {
        // Newest first, the children of `token` before it.
        let mut doomed = vec![token];
        let mut index = 0;
        while index < doomed.len() {
            let parent = doomed[index];
            let mut children: Vec<Token> = self
                .parents
                .borrow()
                .iter()
                .filter(|(child, kind)| {
                    **kind == Parent::Popup(parent) && self.handlers.borrow().contains_key(*child)
                })
                .map(|(child, _)| *child)
                .collect();
            children.sort_unstable();
            doomed.extend(children);
            index += 1;
        }
        doomed.reverse();
        for token in doomed {
            if self.handlers.borrow_mut().remove(&token).is_some() {
                self.calls.borrow_mut().push(Call::Close { token });
                unregister(token);
            }
        }
    }

    fn window(&self, _: Token) -> Option<WindowId> {
        None
    }
}

#[cfg(test)]
mod displace_tests {
    use super::*;

    fn entry(opener: Option<WindowId>, parent: Option<Token>) -> Rc<Entry> {
        Rc::new(Entry {
            window: Rc::new(Ready::new()),
            parent,
            opener,
            on_event: Rc::new(|_| {}),
            closed: Cell::new(false),
            displaced: Cell::new(false),
        })
    }

    #[test]
    fn a_new_root_popup_takes_the_open_popups_of_its_window_and_not_of_another() {
        let backend = WaylandBackend::default();
        let window = WindowId::from(7_u64);
        {
            let mut entries = backend.entries.borrow_mut();
            entries.insert(1, entry(Some(window), None));
            entries.insert(2, entry(None, Some(1)));
            entries.insert(3, entry(Some(WindowId::from(8_u64)), None));
        }
        let listeners = backend.displace(Some(window));
        assert_eq!(listeners.len(), 2);
        assert!(backend.entries.borrow().get(&1).is_none());
        assert!(backend.entries.borrow().get(&2).is_none());
        assert!(
            backend
                .entries
                .borrow()
                .get(&3)
                .is_some_and(|e| !e.closed.get() && !e.displaced.get())
        );
        assert!(backend.displace(None).is_empty());
    }

    fn recording(
        opener: Option<WindowId>,
        parent: Option<Token>,
        token: Token,
        log: &Rc<RefCell<Vec<Token>>>,
    ) -> Rc<Entry> {
        let log = log.clone();
        Rc::new(Entry {
            window: Rc::new(Ready::new()),
            parent,
            opener,
            on_event: Rc::new(move |event| {
                if event == PopupEvent::Dismissed {
                    log.borrow_mut().push(token);
                }
            }),
            closed: Cell::new(false),
            displaced: Cell::new(false),
        })
    }

    #[test]
    fn a_chain_is_destroyed_newest_first_and_each_popup_parents_to_the_one_above() {
        let backend = WaylandBackend::default();
        let window = WindowId::from(7_u64);
        let log = Rc::new(RefCell::new(Vec::new()));
        {
            let mut entries = backend.entries.borrow_mut();
            entries.insert(1, recording(Some(window), None, 1, &log));
            entries.insert(2, recording(None, Some(1), 2, &log));
            entries.insert(3, recording(None, Some(2), 3, &log));
        }
        // Each popup parents to the popup directly above it; the root parents to the window.
        {
            let entries = backend.entries.borrow();
            assert_eq!(entries[&1].parent, None);
            assert_eq!(entries[&2].parent, Some(1));
            assert_eq!(entries[&3].parent, Some(2));
        }
        // Destroying the root destroys the chain: the newest popup first, the root last.
        assert_eq!(backend.subtree(1), vec![3, 2, 1]);
        let listeners = backend.displace(Some(window));
        for listener in &listeners {
            listener(PopupEvent::Dismissed);
        }
        assert_eq!(*log.borrow(), vec![3, 2, 1]);
        assert!(backend.entries.borrow().is_empty());
    }
}
