//! The engine, the one thing behind the browser's pages: the handle every part
//! of the browser calls it through, what it can do, and where it is.

mod status;

use std::sync::Arc;

use bingux_ui::web_view::{BrowserBackend, Capabilities, Done};

use crate::browser::Outbox;

pub use status::Status;

/// The engine. Comparable by identity (the backend) so it can be a prop.
#[derive(Clone)]
pub struct Engine {
    backend: Arc<dyn BrowserBackend>,
    capabilities: Capabilities,
    /// Where the engine is.
    pub status: Status,
}

impl PartialEq for Engine {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.backend, &other.backend)
    }
}

impl Engine {
    /// A handle on `backend`, which is starting.
    pub fn new(backend: Arc<dyn BrowserBackend>) -> Self {
        let capabilities = backend.capabilities();
        Self {
            backend,
            capabilities,
            status: Status::Starting,
        }
    }

    /// The backend itself, for the parts of the UI that draw its frames and
    /// send its input directly (the frame view).
    pub fn backend(&self) -> &Arc<dyn BrowserBackend> {
        &self.backend
    }

    /// What the engine says it can do.
    pub fn capabilities(&self) -> Capabilities {
        self.capabilities
    }

    /// Read again what the engine can do: it may say more once it is up, and
    /// a restart is another engine.
    pub fn refresh(&mut self) {
        self.capabilities = self.backend.capabilities();
    }

    /// Run an engine call; what goes wrong becomes a notice in `out`.
    pub fn call(&self, out: &mut Outbox, call: impl FnOnce(&dyn BrowserBackend) -> Done) {
        if let Err(error) = call(self.backend.as_ref()) {
            // A tab that is already closed is not the person's business.
            if !matches!(error, bingux_ui::web_view::BackendError::NoSuchTab(_)) {
                out.fail(&error);
            }
        }
    }
}
