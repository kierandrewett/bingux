//! What the address field, the new tab page and the settings ask the history's searchable.

use bingux_ui::search::{Hit, Schema, Search};

use super::INDEX;
use crate::address::{Suggestion, Visited};

/// The history's searchable: its schema and the questions put to it.
pub struct History;

impl History {
    /// The searchable's schema.
    pub fn schema() -> Schema {
        Schema::new()
            .searchable("host", 10.0)
            .searchable("title", 8.0)
            .searchable("path", 2.0)
            .sortable(["visit_count", "last_visit"])
            .filterable(["visit_count", "last_visit"])
            .displayed(["title", "uri", "visit_count", "last_visit"])
            .external(false)
    }

    /// What the address field asks the searchable for `typed`: relevance first
    /// (a host that begins with it before a title or path that has it), then the
    /// latest visit, then the most visits.
    pub fn suggestions(typed: &str) -> Search {
        Search::new(typed)
            .index(INDEX)
            .limit(Suggestion::LIMIT)
            .sort("_score:desc")
            .sort("last_visit:desc")
            .sort("visit_count:desc")
    }

    /// The `limit` pages visited last, the latest first.
    pub fn recent(limit: usize) -> Search {
        Search::match_all()
            .index(INDEX)
            .limit(limit)
            .sort("last_visit:desc")
    }

    /// The row a hit is.
    pub fn visited(hit: &Hit) -> Visited {
        Visited {
            url: hit.uri().unwrap_or(hit.id.as_str()).to_string(),
            title: hit.title().unwrap_or_default().to_string(),
        }
    }

    /// Every page: the hit count is how many pages the history holds.
    pub fn everything() -> Search {
        Search::match_all().index(INDEX).limit(1)
    }
}
