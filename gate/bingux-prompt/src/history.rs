use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::types::{PromptRequest, PromptResponse};

/// A record of a single prompt decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub request: PromptRequest,
    pub response: PromptResponse,
    pub responded_at: u64,
}

/// Bounded history of recent prompt decisions.
pub struct PromptHistory {
    entries: Vec<HistoryEntry>,
    max_entries: usize,
}

impl PromptHistory {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: Vec::new(),
            max_entries,
        }
    }

    /// Record a prompt decision, evicting the oldest entry if at capacity.
    pub fn record(&mut self, request: PromptRequest, response: PromptResponse) {
        let responded_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if self.entries.len() >= self.max_entries {
            self.entries.remove(0);
        }

        self.entries.push(HistoryEntry {
            request,
            response,
            responded_at,
        });
    }

    /// Return the most recent `count` entries (or fewer if history is shorter).
    pub fn recent(&self, count: usize) -> &[HistoryEntry] {
        let start = self.entries.len().saturating_sub(count);
        &self.entries[start..]
    }

    /// Clear all history.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Return all entries for a given package name.
    pub fn for_package(&self, package: &str) -> Vec<&HistoryEntry> {
        self.entries
            .iter()
            .filter(|e| e.request.package_name == package)
            .collect()
    }
}
