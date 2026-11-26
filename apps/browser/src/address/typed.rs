//! What was typed in the address bar: an address to go to, or words to search
//! for. Every rule has a test.

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

impl Typed {
    /// Reads what was typed.
    pub fn of(input: &str) -> Self {
        let text = input.trim();
        if text.is_empty() {
            return Self::Nothing;
        }
        if text.chars().any(char::is_whitespace) {
            return Self::Search(text.to_string());
        }
        if let Some(scheme) = Self::scheme_of(text) {
            let lower = scheme.to_ascii_lowercase();
            // The address bar never runs script, and a host with a port reads as
            // `host:port`, not as a scheme.
            if matches!(lower.as_str(), "javascript" | "vbscript") {
                return Self::Search(text.to_string());
            }
            let after = &text[scheme.len() + 1..];
            let port_like = after
                .split(['/', '?', '#'])
                .next()
                .is_some_and(|port| !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()));
            if !port_like {
                return Self::Address(text.to_string());
            }
        }
        if text.starts_with('/') {
            return Self::Address(format!("file://{text}"));
        }
        if Self::looks_like_host(text) {
            return Self::Address(format!("{}://{text}", Self::default_scheme(text)));
        }
        Self::Search(text.to_string())
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
            && labels.last().is_some_and(|tld| {
                tld.chars().any(char::is_alphabetic) && tld.chars().count() >= 2
            }))
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
        let numeric =
            host.split('.').all(|label| label.parse::<u8>().is_ok()) && host.contains('.');
        if host.eq_ignore_ascii_case("localhost") || host.starts_with('[') || numeric {
            "http"
        } else {
            "https"
        }
    }
}

/// The pages the browser draws itself, reached by name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Internal {
    Settings,
    NewTab,
}

impl Typed {
    /// The browser's own page `typed` names, if it names one.
    pub fn internal(typed: &str) -> Option<Internal> {
        match typed.trim() {
            "bingux:settings" | "about:preferences" | "about:config" => Some(Internal::Settings),
            "bingux:newtab" | "about:newtab" | "about:home" => Some(Internal::NewTab),
            _ => None,
        }
    }
}
