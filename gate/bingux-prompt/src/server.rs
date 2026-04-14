use std::collections::HashMap;

use crate::backend::{BackendError, PromptBackend};
use crate::history::PromptHistory;
use crate::types::{PromptRequest, PromptResponse};

/// Errors from the prompt server.
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("backend error: {0}")]
    Backend(#[from] BackendError),
}

pub type Result<T> = std::result::Result<T, ServerError>;

/// The prompt server manages pending prompts and dispatches them to a backend.
pub struct PromptServer {
    backend: Box<dyn PromptBackend>,
    history: PromptHistory,
    pending: HashMap<u64, PromptRequest>,
    next_id: u64,
}

impl PromptServer {
    pub fn new(backend: Box<dyn PromptBackend>) -> Self {
        Self {
            backend,
            history: PromptHistory::new(1000),
            pending: HashMap::new(),
            next_id: 1,
        }
    }

    /// Assign an ID, register as pending, dispatch to the backend, and record
    /// the result in history.
    pub fn submit(&mut self, mut request: PromptRequest) -> Result<PromptResponse> {
        let id = self.next_id;
        self.next_id += 1;
        request.id = id;

        self.pending.insert(id, request.clone());

        let response = self.backend.show_prompt(&request)?;

        self.pending.remove(&id);
        self.history.record(request, response);

        Ok(response)
    }

    /// Dismiss a pending prompt (e.g. the requesting process exited).
    pub fn dismiss(&mut self, prompt_id: u64) {
        if self.pending.remove(&prompt_id).is_some() {
            let _ = self.backend.dismiss(prompt_id);
            tracing::info!(prompt_id, "dismissed pending prompt");
        }
    }

    /// Access the prompt history.
    pub fn history(&self) -> &PromptHistory {
        &self.history
    }

    /// Check whether a prompt is currently pending.
    pub fn is_pending(&self, prompt_id: u64) -> bool {
        self.pending.contains_key(&prompt_id)
    }
}
