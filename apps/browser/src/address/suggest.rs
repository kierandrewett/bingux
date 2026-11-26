//! The list under the address bar while it is being typed in.

use bingux_ui::prelude::{Icon, tr};

use super::{Address, SearchEngine, Typed};

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

/// A page from the `history` searchable: its hits, best first, are the
/// history rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Visited {
    pub url: String,
    pub title: String,
}

impl Suggestion {
    /// The most rows the list shows.
    pub const LIMIT: usize = 7;

    /// The icon a row has.
    pub fn icon(&self) -> Icon {
        match self.source {
            Source::Typed if self.detail.is_empty() => Icon::Search,
            Source::Typed => Icon::WebBrowser,
            Source::Tab(_) => Icon::WindowNew,
            Source::History => Icon::Clock,
        }
    }

    /// The rows for what is typed: the typed text first (go there, or search for
    /// it), then open tabs that match, then the hits of the `history` searchable
    /// for it, in the order the search ranked them.
    pub fn list(
        typed: &str,
        engine: &SearchEngine,
        tabs: &[OpenTab],
        history: &[Visited],
    ) -> Vec<Suggestion> {
        let first = match Typed::of(typed) {
            Typed::Nothing => return Vec::new(),
            Typed::Address(url) => Suggestion {
                source: Source::Typed,
                title: tr!(
                    "browser-address-go-to",
                    address = Address::new(&url).display()
                ),
                detail: url.clone(),
                url,
            },
            Typed::Search(words) => Suggestion {
                source: Source::Typed,
                title: tr!(
                    "browser-address-search-for",
                    engine = engine.name,
                    words = words.as_str()
                ),
                detail: String::new(),
                url: engine.url_for(&words),
            },
        };
        let mut rows = vec![first];
        let query = typed.trim().to_lowercase();
        for tab in tabs {
            let matches = query.split_whitespace().all(|word| {
                tab.url.to_lowercase().contains(word) || tab.title.to_lowercase().contains(word)
            });
            if matches && !tab.url.is_empty() && rows.len() < Self::LIMIT {
                rows.push(Suggestion {
                    source: Source::Tab(tab.key),
                    title: if tab.title.is_empty() {
                        Address::new(&tab.url).display()
                    } else {
                        tab.title.clone()
                    },
                    detail: Address::new(&tab.url).display(),
                    url: tab.url.clone(),
                });
            }
        }
        for visit in history {
            if rows.len() >= Self::LIMIT {
                break;
            }
            // A page that is open is already offered as its tab.
            if rows.iter().any(|row| row.url == visit.url) {
                continue;
            }
            rows.push(Suggestion {
                source: Source::History,
                title: if visit.title.is_empty() {
                    Address::new(&visit.url).display()
                } else {
                    visit.title.clone()
                },
                detail: Address::new(&visit.url).display(),
                url: visit.url.clone(),
            });
        }
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn visited(url: &str, title: &str) -> Visited {
        Visited {
            url: url.to_string(),
            title: title.to_string(),
        }
    }

    /// Hits of the `history` searchable, as it ranked them.
    fn history() -> Vec<Visited> {
        vec![
            visited("https://example.com/", "Example Domain"),
            visited("https://docs.example.org/guide", "The example guide"),
        ]
    }

    #[test]
    fn the_first_row_is_what_was_typed() {
        crate::locale::register();
        let rows = Suggestion::list("example.com", &SearchEngine::ALL[0], &[], &[]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].source, Source::Typed);
        assert_eq!(rows[0].title, "Go to example.com");
        assert_eq!(rows[0].url, "https://example.com");
        let rows = Suggestion::list("rust book", &SearchEngine::ALL[0], &[], &[]);
        assert_eq!(
            rows[0].title,
            "Search DuckDuckGo for \u{201c}rust book\u{201d}"
        );
        assert_eq!(rows[0].url, "https://duckduckgo.com/?q=rust+book");
    }

    #[test]
    fn nothing_typed_offers_nothing() {
        crate::locale::register();
        assert!(Suggestion::list("  ", &SearchEngine::ALL[0], &[], &history()).is_empty());
    }

    #[test]
    fn open_tabs_come_before_history_and_are_not_repeated() {
        crate::locale::register();
        let tabs = [OpenTab {
            key: 7,
            title: "Example Domain".to_string(),
            url: "https://example.com/".to_string(),
        }];
        let rows = Suggestion::list("example", &SearchEngine::ALL[0], &tabs, &history());
        let sources: Vec<Source> = rows.iter().map(|row| row.source.clone()).collect();
        assert_eq!(sources, [Source::Typed, Source::Tab(7), Source::History]);
        assert_eq!(
            rows[2].url, "https://docs.example.org/guide",
            "the open page is not offered twice"
        );
    }

    #[test]
    fn the_list_is_never_longer_than_the_limit() {
        crate::locale::register();
        let history: Vec<Visited> = (0..30)
            .map(|index| visited(&format!("https://site{index}.example/"), "Site"))
            .collect();
        assert_eq!(
            Suggestion::list("site", &SearchEngine::ALL[0], &[], &history).len(),
            Suggestion::LIMIT
        );
    }
}
