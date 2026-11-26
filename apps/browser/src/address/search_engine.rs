//! The search engines the address bar can send words to.

use super::Typed;

/// A search engine: a stable id (what the settings keep), a name and the
/// address a query is put into at `%s`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchEngine {
    pub id: &'static str,
    pub name: &'static str,
    pub template: &'static str,
}

impl SearchEngine {
    /// The address that searches for `query`.
    pub fn url_for(&self, query: &str) -> String {
        self.template.replace("%s", &Self::encode(query))
    }

    /// The engines the settings page offers. The first is the default. An id never
    /// changes once released: a profile's settings name the engine by it.
    pub const ALL: [SearchEngine; 4] = [
        SearchEngine {
            id: "duckduckgo",
            name: "DuckDuckGo",
            template: "https://duckduckgo.com/?q=%s",
        },
        SearchEngine {
            id: "startpage",
            name: "Startpage",
            template: "https://www.startpage.com/do/search?q=%s",
        },
        SearchEngine {
            id: "wikipedia",
            name: "Wikipedia",
            template: "https://en.wikipedia.org/w/index.php?search=%s",
        },
        SearchEngine {
            id: "google",
            name: "Google",
            template: "https://www.google.com/search?q=%s",
        },
    ];

    /// The address to load for what was typed, searching with `engine` when it
    /// was words. `None` for nothing.
    pub fn resolve(&self, input: &str) -> Option<String> {
        match Typed::of(input) {
            Typed::Nothing => None,
            Typed::Address(url) => Some(url),
            Typed::Search(words) => Some(self.url_for(&words)),
        }
    }

    /// `query` as a URL query value: spaces are `+`, everything outside the
    /// unreserved set is percent-encoded over its UTF-8 bytes.
    fn encode(query: &str) -> String {
        let mut out = String::with_capacity(query.len());
        for byte in query.trim().bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    out.push(byte as char)
                }
                b' ' => out.push('+'),
                other => out.push_str(&format!("%{other:02X}")),
            }
        }
        out
    }
}
