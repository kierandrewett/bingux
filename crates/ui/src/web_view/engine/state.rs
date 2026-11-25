//! What the real backend keeps between the engine's thread and the UI's: the
//! sessions, the questions the engine is waiting on, and the engine itself.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use vivarium as vv;

use super::runtime::Runtime;
use crate::web_view::events::BrowserEvent;
use crate::web_view::ids::{RequestId, TabId};
use crate::web_view::queue::EventQueue;
use crate::web_view::surface::PageSurface;

pub(super) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A session of the engine is a tab of the browser.
pub(super) fn tab_of(session: vv::SessionId) -> TabId {
    TabId(session.get())
}

pub(super) struct Entry {
    pub session: Arc<vv::Session>,
    pub surface: Arc<PageSurface>,
    /// An editor has the focus: the caret is shown.
    pub editing: bool,
}

#[derive(Default)]
pub(super) struct State {
    /// The engine, until it is dropped. After a failure it stays (its handles
    /// answer `EngineFailed`) so the settings page can still say what it was.
    pub runtime: Option<Arc<Runtime>>,
    pub live: HashMap<TabId, Entry>,
    /// `window.open` requests the host has not answered yet, by the
    /// provisional session the engine made for them.
    pub windows: HashMap<TabId, vv::Request>,
    /// The other open requests, by the id the browser knows them by.
    pub requests: HashMap<RequestId, vv::Request>,
    pub next_request: u32,
    /// What the tabs of an engine that failed had saved, by their old id:
    /// `TabConfig::restore_from` takes a tab up again from it.
    pub lost: HashMap<TabId, Vec<u8>>,
    /// Contexts made for containers, by (container, private): kept so a
    /// container's tabs share one and it lives as long as the engine.
    pub contexts: HashMap<(u32, bool), vv::Context>,
    /// The engine took the WebRTC policy the host asked for.
    pub webrtc_applied: bool,
}

pub(super) struct Shared {
    pub queue: EventQueue,
    pub state: Mutex<State>,
}

impl Shared {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            queue: EventQueue::new(),
            state: Mutex::new(State::default()),
        })
    }

    pub fn push(&self, event: BrowserEvent) {
        self.queue.push(event);
    }
}
