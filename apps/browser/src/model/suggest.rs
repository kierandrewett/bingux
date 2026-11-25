//! The list under the address bar while it is being typed in.

use super::address::{self, SearchEngine, Typed};
use super::history::History;

/// Where a suggestion came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// What was typed, as an address or a search. Always first.
    Typed,
    /// A tab that is already open: go to it.
    Tab(u64),
    /// A page visited before.
    History,
}

/// One row of the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suggestion {
    pub source: Source,
    /// The first line: a title, or what the row does.
    pub title: String,
    /// The second line: the address.
    pub detail: String,
    /// The address that opens when the row is taken.
    pub url: String,
}

/// A tab that is open, for the "switch to" rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenTab {
    pub key: u64,
    pub title: String,
    pub url: String,
}

/// The most rows the list shows.
pub const LIMIT: usize = 7;

/// The rows for what is typed: the typed text first (go there, or search for
/// it), then open tabs that match, then history.
pub fn suggest(typed: &str, engine: &SearchEngine, tabs: &[OpenTab], history: &History) -> Vec<Suggestion> {
    let first = match address::classify(typed) {
        Typed::Nothing => return Vec::new(),
        Typed::Address(url) => Suggestion {
            source: Source::Typed,
            title: format!("Go to {}", address::display(&url)),
            detail: url.clone(),
            url,
        },
        Typed::Search(words) => Suggestion {
            source: Source::Typed,
            title: format!("Search {} for \u{201c}{words}\u{201d}", engine.name),
            detail: String::new(),
            url: engine.url_for(&words),
        },
    };
    let mut rows = vec![first];
    let query = typed.trim().to_lowercase();
    for tab in tabs {
        let matches = query
            .split_whitespace()
            .all(|word| tab.url.to_lowercase().contains(word) || tab.title.to_lowercase().contains(word));
        if matches && !tab.url.is_empty() && rows.len() < LIMIT {
            rows.push(Suggestion {
                source: Source::Tab(tab.key),
                title: if tab.title.is_empty() { address::display(&tab.url) } else { tab.title.clone() },
                detail: address::display(&tab.url),
                url: tab.url.clone(),
            });
        }
    }
    for visit in history.matching(typed, LIMIT) {
        if rows.len() >= LIMIT {
            break;
        }
        // A page that is open is already offered as its tab.
        if rows.iter().any(|row| row.url == visit.url) {
            continue;
        }
        rows.push(Suggestion {
            source: Source::History,
            title: if visit.title.is_empty() { address::display(&visit.url) } else { visit.title.clone() },
            detail: address::display(&visit.url),
            url: visit.url.clone(),
        });
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::address::ENGINES;

    fn history() -> History {
        let mut history = History::new();
        history.record("https://example.com/", "Example Domain");
        history.record("https://docs.example.org/guide", "The example guide");
        history
    }

    #[test]
    fn the_first_row_is_what_was_typed() {
        let rows = suggest("example.com", &ENGINES[0], &[], &History::new());
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].source, Source::Typed);
        assert_eq!(rows[0].title, "Go to example.com");
        assert_eq!(rows[0].url, "https://example.com");
        let rows = suggest("rust book", &ENGINES[0], &[], &History::new());
        assert_eq!(rows[0].title, "Search DuckDuckGo for \u{201c}rust book\u{201d}");
        assert_eq!(rows[0].url, "https://duckduckgo.com/?q=rust+book");
    }

    #[test]
    fn nothing_typed_offers_nothing() {
        assert!(suggest("  ", &ENGINES[0], &[], &history()).is_empty());
    }

    #[test]
    fn open_tabs_come_before_history_and_are_not_repeated() {
        let tabs = [OpenTab {
            key: 7,
            title: "Example Domain".to_string(),
            url: "https://example.com/".to_string(),
        }];
        let rows = suggest("example", &ENGINES[0], &tabs, &history());
        let sources: Vec<Source> = rows.iter().map(|row| row.source.clone()).collect();
        assert_eq!(sources, [Source::Typed, Source::Tab(7), Source::History]);
        assert_eq!(rows[2].url, "https://docs.example.org/guide", "the open page is not offered twice");
    }

    #[test]
    fn the_list_is_never_longer_than_the_limit() {
        let mut history = History::new();
        for index in 0..30 {
            history.record(&format!("https://site{index}.example/"), "Site");
        }
        assert_eq!(suggest("site", &ENGINES[0], &[], &history).len(), LIMIT);
    }
}
