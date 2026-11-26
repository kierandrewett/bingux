//! What was typed in the address bar: an address to go to, or words to search
//! for. Pure functions, so every rule has a test.

/// A search engine: a stable id (what the settings keep), a name and the
/// address a query is put into at `%s`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchEngine {
    pub id: &'static str,
    pub name: &'static str,
    pub template: &'static str,
}

/// The engines the settings page offers. The first is the default. An id never
/// changes once released: a profile's settings name the engine by it.
pub const ENGINES: [SearchEngine; 4] = [
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

impl SearchEngine {
    /// The address that searches for `query`.
    pub fn url_for(&self, query: &str) -> String {
        self.template.replace("%s", &encode(query))
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

/// What the text in the address bar means.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Typed {
    /// Nothing, or only spaces.
    Nothing,
    /// An address to load, as it should be sent to the engine.
    Address(String),
    /// Words to search for.
    Search(String),
}

/// The scheme of `text` when it begins with one (`https`, `about`).
fn scheme_of(text: &str) -> Option<&str> {
    let (scheme, _) = text.split_once(':')?;
    let mut chars = scheme.chars();
    let first = chars.next()?;
    (first.is_ascii_alphabetic()
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')))
    .then_some(scheme)
}

/// `host`, `host:port` or `[v6]` followed by nothing or a path, query or
/// fragment: does the start of `text` read as a host that is not a search word?
fn looks_like_host(text: &str) -> bool {
    let authority = text.split(['/', '?', '#']).next().unwrap_or("");
    let authority = authority.rsplit('@').next().unwrap_or(authority);
    if authority.starts_with('[') {
        return authority.contains(']');
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => {
            (host, true)
        }
        Some(_) => return false,
        None => (authority, false),
    };
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    if host.is_empty() || host.starts_with('.') || host.ends_with('.') || host.contains("..") {
        return false;
    }
    let labels: Vec<&str> = host.split('.').collect();
    let label_ok = |label: &&str| {
        !label.is_empty()
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label.chars().all(|c| c.is_alphanumeric() || c == '-')
    };
    if !labels.iter().all(label_ok) {
        return false;
    }
    let is_ipv4 = labels.len() == 4 && labels.iter().all(|label| label.parse::<u8>().is_ok());
    if is_ipv4 {
        return true;
    }
    // A name needs a dot and a last label that is not a number: "3.14" is a
    // number, "notes.txt" is a file name that happens to be a host, and so is
    // a word with a port, which only a host has.
    port || (labels.len() >= 2
        && labels
            .last()
            .is_some_and(|tld| tld.chars().any(char::is_alphabetic) && tld.chars().count() >= 2))
}

/// The scheme a bare host gets: plain HTTP for a machine on this network
/// (`localhost`, an address), HTTPS for a name.
fn default_scheme(text: &str) -> &'static str {
    let authority = text.split(['/', '?', '#']).next().unwrap_or("");
    let host = authority.rsplit('@').next().unwrap_or(authority);
    let host = host
        .rsplit_once(':')
        .filter(|(_, port)| port.bytes().all(|b| b.is_ascii_digit()))
        .map_or(host, |(host, _)| host);
    let numeric = host.split('.').all(|label| label.parse::<u8>().is_ok()) && host.contains('.');
    if host.eq_ignore_ascii_case("localhost") || host.starts_with('[') || numeric {
        "http"
    } else {
        "https"
    }
}

/// Reads what was typed.
pub fn classify(input: &str) -> Typed {
    let text = input.trim();
    if text.is_empty() {
        return Typed::Nothing;
    }
    if text.chars().any(char::is_whitespace) {
        return Typed::Search(text.to_string());
    }
    if let Some(scheme) = scheme_of(text) {
        let lower = scheme.to_ascii_lowercase();
        // The address bar never runs script, and a host with a port reads as
        // `host:port`, not as a scheme.
        if matches!(lower.as_str(), "javascript" | "vbscript") {
            return Typed::Search(text.to_string());
        }
        let after = &text[scheme.len() + 1..];
        let port_like = after
            .split(['/', '?', '#'])
            .next()
            .is_some_and(|port| !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()));
        if !port_like {
            return Typed::Address(text.to_string());
        }
    }
    if text.starts_with('/') {
        return Typed::Address(format!("file://{text}"));
    }
    if looks_like_host(text) {
        return Typed::Address(format!("{}://{text}", default_scheme(text)));
    }
    Typed::Search(text.to_string())
}

/// The address to load for what was typed, searching with `engine` when it
/// was words. `None` for nothing.
pub fn resolve(input: &str, engine: &SearchEngine) -> Option<String> {
    match classify(input) {
        Typed::Nothing => None,
        Typed::Address(url) => Some(url),
        Typed::Search(words) => Some(engine.url_for(&words)),
    }
}

/// The text the address bar shows for `url` while it is not being edited: no
/// `https://`, no trailing slash on a bare host; `http://` stays, because it
/// is what makes the address plain. The browser's own blank pages show
/// nothing, so the placeholder invites a search.
pub fn display(url: &str) -> String {
    if url.is_empty() || url == "about:blank" || url.starts_with("bingux:") {
        return String::new();
    }
    let (scheme, rest) = url
        .split_once("://")
        .map_or(("", url), |(scheme, rest)| (scheme, rest));
    let rest = match rest.strip_suffix('/') {
        Some(host) if !host.contains('/') => host,
        _ => rest,
    };
    match scheme {
        "" => rest.to_string(),
        "https" => rest.to_string(),
        other => format!("{other}://{rest}"),
    }
}

/// The host of an address, lower case, without credentials and port.
pub fn host(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let authority = authority.rsplit('@').next().unwrap_or(authority);
    if let Some(inner) = authority.strip_prefix('[') {
        return inner.split(']').next().unwrap_or("").to_ascii_lowercase();
    }
    authority
        .split(':')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// `scheme://host[:port]` of an address: what a permission belongs to.
pub fn origin(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_string();
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let authority = authority.rsplit('@').next().unwrap_or(authority);
    format!(
        "{}://{}",
        scheme.to_ascii_lowercase(),
        authority.to_ascii_lowercase()
    )
}

/// Whether `url` is delivered over a plain connection: what the page info
/// calls "not secure".
pub fn is_plain_http(url: &str) -> bool {
    url.len() >= 7 && url[..7].eq_ignore_ascii_case("http://")
}

/// The first letter a page's placeholder icon shows: the host's first letter
/// in capitals, or `None` for a page with no host.
pub fn initial(url: &str) -> Option<char> {
    if !url.contains("://") {
        return None;
    }
    let host = host(url);
    let name = host.strip_prefix("www.").unwrap_or(&host);
    name.chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn address(text: &str) -> String {
        match classify(text) {
            Typed::Address(url) => url,
            other => panic!("{text:?} should be an address, was {other:?}"),
        }
    }

    fn search(text: &str) -> String {
        match classify(text) {
            Typed::Search(words) => words,
            other => panic!("{text:?} should be a search, was {other:?}"),
        }
    }

    #[test]
    fn a_name_with_a_dot_is_an_address_over_https() {
        assert_eq!(address("example.com"), "https://example.com");
        assert_eq!(
            address("docs.example.org/a/b?c=d#e"),
            "https://docs.example.org/a/b?c=d#e"
        );
        assert_eq!(address("  example.com  "), "https://example.com");
        assert_eq!(address("bücher.example"), "https://bücher.example");
    }

    #[test]
    fn this_machine_and_numbers_are_plain_http() {
        assert_eq!(address("localhost"), "http://localhost");
        assert_eq!(address("localhost:8080/x"), "http://localhost:8080/x");
        assert_eq!(address("192.168.1.1"), "http://192.168.1.1");
        assert_eq!(address("192.168.1.1:3000"), "http://192.168.1.1:3000");
        assert_eq!(address("[::1]:8080"), "http://[::1]:8080");
    }

    #[test]
    fn a_word_with_a_port_is_a_host() {
        assert_eq!(address("router:8080"), "https://router:8080");
    }

    #[test]
    fn a_scheme_is_kept_as_typed() {
        assert_eq!(address("http://example.com"), "http://example.com");
        assert_eq!(address("file:///etc/hostname"), "file:///etc/hostname");
        assert_eq!(address("about:blank"), "about:blank");
        assert_eq!(
            address("view-source:https://example.com"),
            "view-source:https://example.com"
        );
    }

    #[test]
    fn a_path_is_a_file() {
        assert_eq!(address("/home/me/notes.html"), "file:///home/me/notes.html");
    }

    #[test]
    fn words_and_numbers_are_searches() {
        assert_eq!(search("rust lifetimes"), "rust lifetimes");
        assert_eq!(search("weather"), "weather");
        assert_eq!(search("3.14"), "3.14");
        assert_eq!(search("what is example.com"), "what is example.com");
        assert_eq!(search("notes."), "notes.");
        assert_eq!(search("a..b"), "a..b");
    }

    #[test]
    fn the_address_bar_never_runs_script() {
        assert_eq!(search("javascript:alert(1)"), "javascript:alert(1)");
        assert_eq!(search("JavaScript:void(0)"), "JavaScript:void(0)");
    }

    #[test]
    fn nothing_is_nothing() {
        assert_eq!(classify(""), Typed::Nothing);
        assert_eq!(classify("   "), Typed::Nothing);
        assert_eq!(resolve("  ", &ENGINES[0]), None);
    }

    #[test]
    fn a_search_is_encoded_for_the_engine() {
        assert_eq!(
            resolve("rust & c++ \u{e9}", &ENGINES[0]),
            Some("https://duckduckgo.com/?q=rust+%26+c%2B%2B+%C3%A9".to_string())
        );
        assert_eq!(
            resolve("rust", &ENGINES[2]),
            Some("https://en.wikipedia.org/w/index.php?search=rust".to_string())
        );
    }

    #[test]
    fn the_bar_shows_the_address_without_the_noise() {
        assert_eq!(display("https://example.com/"), "example.com");
        assert_eq!(display("https://example.com/a/"), "example.com/a/");
        assert_eq!(
            display("http://example.com/"),
            "http://example.com",
            "plain http stays visible"
        );
        assert_eq!(display("about:blank"), "");
        assert_eq!(display(""), "");
        assert_eq!(display("bingux:newtab"), "");
    }

    #[test]
    fn host_and_origin_are_read_from_an_address() {
        assert_eq!(
            host("https://User:pw@Docs.Example.org:8443/x?y#z"),
            "docs.example.org"
        );
        assert_eq!(host("http://[::1]:80/"), "::1");
        assert_eq!(
            origin("HTTPS://Example.com:8443/a/b?c"),
            "https://example.com:8443"
        );
        assert!(is_plain_http("HTTP://x"));
        assert!(!is_plain_http("https://x"));
        assert_eq!(initial("https://www.example.com"), Some('E'));
        assert_eq!(initial("about:blank"), None);
    }
}
