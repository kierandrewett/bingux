//! An address, read for its parts.

/// An address in text form: what the address bar shows, who it belongs to.
#[derive(Clone, Copy, Debug)]
pub struct Address<'a>(&'a str);

impl<'a> Address<'a> {
    /// The address `url`.
    pub fn new(url: &'a str) -> Self {
        Self(url)
    }

    /// The text the address bar shows for `url` while it is not being edited: no
    /// `https://`, no trailing slash on a bare host; `http://` stays, because it
    /// is what makes the address plain. The browser's own blank pages show
    /// nothing, so the placeholder invites a search.
    pub fn display(self) -> String {
        let url = self.0;
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
    pub fn host(self) -> String {
        let url = self.0;
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
    pub fn origin(self) -> String {
        let url = self.0;
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
    pub fn is_plain_http(self) -> bool {
        let url = self.0;
        url.len() >= 7 && url[..7].eq_ignore_ascii_case("http://")
    }

    /// The first letter a page's placeholder icon shows: the host's first letter
    /// in capitals, or `None` for a page with no host.
    pub fn initial(self) -> Option<char> {
        let url = self.0;
        if !url.contains("://") {
            return None;
        }
        let host = self.host();
        let name = host.strip_prefix("www.").unwrap_or(&host);
        name.chars()
            .find(|c| c.is_alphanumeric())
            .map(|c| c.to_ascii_uppercase())
    }
}
