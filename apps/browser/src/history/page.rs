//! A page of history, and what changes to it wait to be written.

use bingux_ui::search::Document;

use crate::address::Address;

/// One page of history: what the index and the address field show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page {
    pub url: String,
    pub title: String,
    pub visit_count: u32,
    /// Unix milliseconds.
    pub last_visit: i64,
}

/// What changed, waiting to be written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    Visit { url: String, title: String, at: i64 },
    Title { url: String, title: String },
}

impl Page {
    /// Whether a load of `url` is history: a page on the web or a file, not the
    /// browser's own pages and not a blank tab.
    pub fn is_recorded(url: &str) -> bool {
        !(url.is_empty() || url.starts_with("about:") || url.starts_with("bingux:"))
    }

    /// The index document of the page: its id is the address.
    pub fn document(&self) -> Document {
        let host = Address::new(&self.url).host();
        let host = host.strip_prefix("www.").unwrap_or(&host).to_string();
        let path = self
            .url
            .split_once("://")
            .map_or(self.url.as_str(), |(_, rest)| rest)
            .split_once('/')
            .map_or("", |(_, path)| path)
            .to_string();
        Document::new(self.url.clone())
            .title(&self.title)
            .uri(&self.url)
            .field("host", host)
            .field("path", path)
            .field("visit_count", self.visit_count)
            .field("last_visit", self.last_visit)
    }

    /// The page an index document holds.
    pub fn from_document(document: &Document) -> Self {
        let text = |name: &str| {
            document
                .fields
                .get(name)
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string()
        };
        let number = |name: &str| {
            document
                .fields
                .get(name)
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0)
        };
        Self {
            url: document.id.clone(),
            title: text("title"),
            visit_count: u32::try_from(number("visit_count")).unwrap_or(u32::MAX),
            last_visit: number("last_visit"),
        }
    }
}
