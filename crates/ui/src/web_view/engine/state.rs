//! What the real backend keeps between the engine's delegates and the UI's
//! calls: the sessions, the questions the engine is waiting on, and the engine
//! itself.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use vivarium::contextual_identities::ContextualIdentity;
use vivarium::navigation::NewWindowReply;
use vivarium::request::Responder;
use vivarium::session::{Session, SessionId};

use super::requests::Open;
use super::runtime::Runtime;
use crate::web_view::events::BrowserEvent;
use crate::web_view::ids::{RequestId, TabId};
use crate::web_view::queue::EventQueue;
use crate::web_view::surface::PageSurface;

pub(super) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A session of the engine is a tab of the browser.
pub(super) fn tab_of(session: SessionId) -> TabId {
    TabId(session.get())
}

pub(super) struct Entry {
    pub session: Arc<Session>,
    pub surface: Arc<PageSurface>,
    /// The person is looking at the tab (`set_visible`); a hidden tab may not
    /// take the screen.
    pub visible: bool,
}

/// A question the engine waits on: the tab that asked, and the responder
/// that answers it.
pub(super) struct Asked {
    pub tab: TabId,
    pub open: Open,
}

#[derive(Default)]
pub(super) struct State {
    /// The engine, until it is dropped. After a failure it stays (its handles
    /// answer `EngineFailed`) so the settings page can still say what it was.
    pub runtime: Option<Arc<Runtime>>,
    pub live: HashMap<TabId, Entry>,
    /// `window.open` requests the host has not answered yet, by the
    /// provisional session the engine made for them.
    pub windows: HashMap<TabId, Responder<NewWindowReply>>,
    /// The other open requests, by the id the browser knows them by.
    pub requests: HashMap<RequestId, Asked>,
    pub next_request: u32,
    /// Prints the engine has not finished, by the tag they carry: the tab and
    /// the name the file is to have.
    pub prints: HashMap<u64, (TabId, String)>,
    pub next_print: u64,
    /// Contextual identities made for containers, by (container, private):
    /// kept so a container's tabs share one and it lives as long as the
    /// engine.
    pub identities: HashMap<(u32, bool), ContextualIdentity>,
}

pub(super) struct Shared {
    /// Also held by the engine's wake hook, which wakes the UI through it.
    pub queue: Arc<EventQueue>,
    pub state: Mutex<State>,
}

impl Shared {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            queue: Arc::new(EventQueue::new()),
            state: Mutex::new(State::default()),
        })
    }

    pub fn push(&self, event: BrowserEvent) {
        self.queue.push(event);
    }
}
