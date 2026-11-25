//! The pages visited in this window's life, for the address bar's
//! suggestions. It is kept in memory: a browsing history that outlives the
//! window is a database with a privacy policy, and the engine's own session
//! history (Back and Forward) is the engine's.

use super::address;

/// One address, how often it was visited and when last.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Visit {
    pub url: String,
    pub title: String,
    pub count: u32,
    /// A number that grows with every visit anywhere: the larger, the later.
    pub last: u64,
}

/// Every address visited.
#[derive(Clone, Debug, Default)]
pub struct History {
    visits: Vec<Visit>,
    clock: u64,
}

/// The address without its scheme and `www.`, lower case: what a person
/// types the start of.
fn bare(url: &str) -> String {
    let text = url.split_once("://").map_or(url, |(_, rest)| rest);
    text.strip_prefix("www.").unwrap_or(text).to_lowercase()
}

impl History {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.visits.len()
    }

    pub fn is_empty(&self) -> bool {
        self.visits.is_empty()
    }

    /// A page was loaded.
    pub fn record(&mut self, url: &str, title: &str) {
        if url.is_empty() || url.starts_with("about:") || url.starts_with("bingux:") {
            return;
        }
        self.clock += 1;
        if let Some(visit) = self.visits.iter_mut().find(|visit| visit.url == url) {
            visit.count += 1;
            visit.last = self.clock;
            if !title.is_empty() {
                visit.title = title.to_string();
            }
            return;
        }
        self.visits.push(Visit {
            url: url.to_string(),
            title: title.to_string(),
            count: 1,
            last: self.clock,
        });
    }

    /// The page at `url` got its title after it was recorded.
    pub fn set_title(&mut self, url: &str, title: &str) {
        if let Some(visit) = self.visits.iter_mut().find(|visit| visit.url == url) {
            visit.title = title.to_string();
        }
    }

    pub fn clear(&mut self) {
        self.visits.clear();
    }

    /// The `limit` pages visited last, the latest first.
    pub fn recent(&self, limit: usize) -> Vec<&Visit> {
        let mut visits: Vec<&Visit> = self.visits.iter().collect();
        visits.sort_by(|a, b| b.last.cmp(&a.last));
        visits.truncate(limit);
        visits
    }

    /// Up to `limit` visits that match every word of `query` in their address
    /// or title, best first: those whose address begins with the query, then
    /// by how often and how recently.
    pub fn matching(&self, query: &str, limit: usize) -> Vec<&Visit> {
        let lowered = query.trim().to_lowercase();
        let words: Vec<&str> = lowered.split_whitespace().collect();
        if words.is_empty() {
            return Vec::new();
        }
        let mut scored: Vec<(u64, &Visit)> = self
            .visits
            .iter()
            .filter_map(|visit| {
                let address = bare(&visit.url);
                let title = visit.title.to_lowercase();
                if !words.iter().all(|word| address.contains(word) || title.contains(word)) {
                    return None;
                }
                let prefix = if address.starts_with(words[0]) { 1_000_000 } else { 0 };
                let host_prefix = if address::host(&visit.url).trim_start_matches("www.").starts_with(words[0]) {
                    500_000
                } else {
                    0
                };
                Some((prefix + host_prefix + u64::from(visit.count) * 1_000 + visit.last, visit))
            })
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0));
        scored.into_iter().take(limit).map(|(_, visit)| visit).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history() -> History {
        let mut history = History::new();
        history.record("https://example.com/", "Example Domain");
        history.record("https://news.bingux.dev/", "Bingux News");
        history.record("https://docs.example.org/guide", "The example guide");
        history.record("https://news.bingux.dev/", "Bingux News");
        history
    }

    #[test]
    fn a_repeat_visit_counts_and_keeps_one_entry() {
        let history = history();
        assert_eq!(history.len(), 3);
        let news = history.matching("news", 5);
        assert_eq!(news[0].count, 2);
    }

    #[test]
    fn every_word_must_match_the_address_or_the_title() {
        let history = history();
        let found: Vec<&str> = history.matching("example guide", 5).iter().map(|v| v.url.as_str()).collect();
        assert_eq!(found, ["https://docs.example.org/guide"]);
        assert!(history.matching("zebra", 5).is_empty());
        assert!(history.matching("   ", 5).is_empty());
    }

    #[test]
    fn an_address_that_begins_with_the_query_comes_first() {
        let history = history();
        let found: Vec<&str> = history.matching("example", 5).iter().map(|v| v.url.as_str()).collect();
        assert_eq!(found[0], "https://example.com/", "begins with it, though visited once");
        assert_eq!(found[1], "https://docs.example.org/guide");
    }

    #[test]
    fn the_latest_visits_come_first() {
        let history = history();
        let urls: Vec<&str> = history.recent(2).iter().map(|visit| visit.url.as_str()).collect();
        assert_eq!(urls, ["https://news.bingux.dev/", "https://docs.example.org/guide"]);
    }

    #[test]
    fn internal_pages_are_not_remembered_and_a_title_can_arrive_late() {
        let mut history = History::new();
        history.record("about:blank", "");
        history.record("bingux:settings", "Settings");
        assert!(history.is_empty());
        history.record("https://a.example/", "");
        history.set_title("https://a.example/", "A");
        assert_eq!(history.matching("a.example", 1)[0].title, "A");
        history.clear();
        assert!(history.is_empty());
    }
}
