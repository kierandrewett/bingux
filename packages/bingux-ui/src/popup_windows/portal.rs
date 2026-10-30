//! One popup, from the `Panel` that wants it and from its own window.
//!
//! [`use_native_popup`] gives the `Panel` a [`NativePopup`]: each render it says whether it wants
//! the popup open, where, how big and what it shows ([`NativePopup::sync`]); the handle opens,
//! resizes and closes the window through the backend to match, and tells the panel when the
//! compositor refused or dismissed it. The popup's own window renders [`popup_root`].

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use freya::prelude::*;

use super::backend::{OpenRequest, Parent, PopupBackend, PopupEvent, Token};
use super::place::PopupRequest;
use super::{MARGIN, PopupContent, PopupOrigin};
use crate::resilience::error_boundary;

/// The popup a panel's nested panels (a submenu) and the content of the popup itself open their
/// own popups from.
#[derive(Clone)]
pub(crate) struct ParentSlot {
    inner: Rc<ParentInner>,
}

struct ParentInner {
    token: Cell<Option<Token>>,
    /// Where the parent's card starts, in the coordinates the child's trigger is given in: the
    /// anchor of the child is the trigger's less this.
    card_origin: Cell<(f32, f32)>,
}

impl ParentSlot {
    pub(crate) fn new() -> Self {
        Self {
            inner: Rc::new(ParentInner { token: Cell::new(None), card_origin: Cell::new((0.0, 0.0)) }),
        }
    }

    pub(crate) fn set(&self, token: Option<Token>, card_origin: (f32, f32)) {
        self.inner.token.set(token);
        self.inner.card_origin.set(card_origin);
    }

    pub(crate) fn token(&self) -> Option<Token> {
        self.inner.token.get()
    }

    pub(crate) fn card_origin(&self) -> (f32, f32) {
        self.inner.card_origin.get()
    }
}

impl PartialEq for ParentSlot {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

/// What a panel wants of its popup this render.
pub(crate) struct Want {
    pub(crate) open: bool,
    /// Where the popup goes, in the coordinates of the parent's window geometry.
    pub(crate) placement: PopupRequest,
    pub(crate) parent: Parent,
    /// What the popup's components add to a pointer position to get the opener's coordinates.
    pub(crate) origin: (f32, f32),
    /// What the panel does when the compositor takes the popup away.
    pub(crate) on_dismiss: EventHandler<()>,
    /// What the popup shows.
    pub(crate) content: Element,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    Closed,
    /// Asked for; the window is being made.
    Opening,
    Open,
    /// The compositor would not make it.
    Refused,
}

struct Inner {
    phase: Cell<Phase>,
    token: Cell<Option<Token>>,
    size: Cell<(f32, f32)>,
    content: PopupContent,
    dismiss: RefCell<EventHandler<()>>,
    /// The compositor took the popup; it stays closed until the panel's owner closes it.
    dismissed: Cell<bool>,
    refused: State<bool>,
}

/// The lifecycle of one popup window as its panel sees it.
#[derive(Clone)]
pub(crate) struct NativePopup {
    inner: Rc<Inner>,
}

impl NativePopup {
    fn new(refused: State<bool>) -> Self {
        Self {
            inner: Rc::new(Inner {
                phase: Cell::new(Phase::Closed),
                token: Cell::new(None),
                size: Cell::new((0.0, 0.0)),
                content: PopupContent::new(rect().into_element()),
                dismiss: RefCell::new(EventHandler::new(|_: ()| {})),
                dismissed: Cell::new(false),
                refused,
            }),
        }
    }

    /// The popup's token while it is open or opening, for the popups that grow from it.
    pub(crate) fn token(&self) -> Option<Token> {
        self.inner.token.get()
    }

    /// Whether the compositor refused to make the popup.
    pub(crate) fn refused(&self) -> bool {
        self.inner.phase.get() == Phase::Refused
    }

    /// Brings the popup to what the panel wants now. Idempotent: a render that changes nothing
    /// changes nothing here.
    pub(crate) fn sync(&self, backend: &Rc<dyn PopupBackend>, want: Want) {
        let inner = &self.inner;
        *inner.dismiss.borrow_mut() = want.on_dismiss.clone();
        if !want.open {
            inner.dismissed.set(false);
            self.close(backend);
            return;
        }
        if inner.dismissed.get() || inner.phase.get() == Phase::Refused {
            return;
        }
        inner.content.publish(want.content);
        match inner.phase.get() {
            Phase::Closed => self.open(backend, want.placement, want.parent, want.origin),
            Phase::Opening | Phase::Open => {
                if let Some(token) = inner.token.get()
                    && inner.size.get() != want.placement.size
                {
                    inner.size.set(want.placement.size);
                    backend.resize(token, want.placement.size);
                }
            }
            Phase::Refused => {}
        }
    }

    fn open(
        &self,
        backend: &Rc<dyn PopupBackend>,
        placement: PopupRequest,
        parent: Parent,
        origin: (f32, f32),
    ) {
        let inner = self.inner.clone();
        inner.size.set(placement.size);
        inner.phase.set(Phase::Opening);
        let events = inner.clone();
        let on_event: Rc<dyn Fn(PopupEvent)> = Rc::new(move |event| match event {
            PopupEvent::Ready => {
                if events.phase.get() == Phase::Opening {
                    events.phase.set(Phase::Open);
                }
            }
            PopupEvent::Refused => {
                events.phase.set(Phase::Refused);
                events.token.set(None);
                super::note_refused();
                let mut refused = events.refused;
                refused.set(true);
            }
            PopupEvent::Dismissed => {
                events.phase.set(Phase::Closed);
                events.token.set(None);
                events.dismissed.set(true);
                let dismiss = events.dismiss.borrow().clone();
                dismiss.call(());
            }
        });
        let token = backend.open(OpenRequest {
            placement,
            parent,
            content: inner.content.clone(),
            contexts: snapshot_contexts(),
            origin,
            on_event,
        });
        // A backend that answered at once (the fake one) has already said what became of it.
        if matches!(inner.phase.get(), Phase::Opening | Phase::Open) {
            inner.token.set(Some(token));
        }
    }

    /// Closes the popup, and the popups opened from it.
    pub(crate) fn close(&self, backend: &Rc<dyn PopupBackend>) {
        let inner = &self.inner;
        if let Some(token) = inner.token.take() {
            backend.close(token);
        }
        if matches!(inner.phase.get(), Phase::Opening | Phase::Open) {
            inner.phase.set(Phase::Closed);
        }
    }

    /// The panel is gone.
    fn shutdown(&self) {
        if let Some(backend) = super::backend() {
            self.close(&backend);
        } else if let Some(token) = self.inner.token.take() {
            // The backend went away under an open popup (a refusal elsewhere): close it anyway.
            if let Some(backend) = super::raw_backend() {
                backend.close(token);
            }
        }
    }
}

/// The popup of the panel being rendered. A hook: call it unconditionally.
pub(crate) fn use_native_popup() -> NativePopup {
    let refused = use_state(|| false);
    let popup = use_hook(move || NativePopup::new(refused));
    let closing = popup.clone();
    use_drop(move || closing.shutdown());
    popup
}

/// The root of a popup window: the contexts of the window it came from, then what the panel
/// published, in a boundary of its own so a failing menu row closes nothing but itself.
pub(crate) fn popup_root(
    content: &PopupContent,
    contexts: &ContextSnapshot,
    origin: PopupOrigin,
    token: Token,
) -> Element {
    use_hook(|| provide_context_snapshot(contexts));
    use_provide_context(move || origin);
    use_provide_context(move || {
        let slot = ParentSlot::new();
        slot.set(Some(token), (MARGIN, MARGIN));
        slot
    });
    let wake = use_state(|| 0_u64);
    use_hook(move || content.bind(wake));
    // Reading it is what subscribes the root: a write redraws the popup.
    let _ = *wake.read();
    rect()
        .expanded()
        .child(error_boundary("Popup", content.element()))
        .into_element()
}
