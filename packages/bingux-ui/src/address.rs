//! Internal addresses: `scheme://segment/segment?key=value`.
//!
//! Every page, subpage and popup of an application has one, so a link such as
//! `bingux-settings://wifi/saved` opens it from anywhere: the command line,
//! `xdg-open`, another process, or a row that navigates.
//!
//! The first segment is written like a URL host but is only a path segment
//! here, so `wifi` and `saved` in `bingux-settings://wifi/saved` are the same
//! kind of thing. A segment may hold any text, such as a Wi-Fi name with
//! spaces and slashes. [`Display`](std::fmt::Display) escapes what would
//! break the form and [`Address::parse`] undoes it, so an address always
//! survives a round trip through text.

use std::fmt;

/// An address: scheme, path segments and query.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Address {
    scheme: String,
    segments: Vec<String>,
    query: Vec<(String, String)>,
}

impl Address {
    /// The root address of `scheme`: no segments, no query.
    pub fn new(scheme: impl Into<String>) -> Self {
        Self {
            scheme: scheme.into().to_ascii_lowercase(),
            segments: Vec::new(),
            query: Vec::new(),
        }
    }

    /// Parse a full address, `scheme://seg/seg?k=v`. `None` for a bare path,
    /// a bad scheme or a bad percent escape. Use [`Address::parse_in`] for
    /// input that may be a bare path.
    pub fn parse(text: &str) -> Option<Self> {
        let (scheme, rest) = split_scheme(text.trim())?;
        Self::from_parts(scheme, rest)
    }

    /// Parse `text` as a full address, or as a bare path (`wifi/saved`,
    /// `/wifi/saved`) that takes `scheme`. A full address with another
    /// scheme is returned as it is.
    pub fn parse_in(scheme: &str, text: &str) -> Option<Self> {
        let text = text.trim();
        match split_scheme(text) {
            Some((found, rest)) => Self::from_parts(found, rest),
            None => Self::from_parts(scheme, text),
        }
    }

    fn from_parts(scheme: &str, rest: &str) -> Option<Self> {
        if !valid_scheme(scheme) {
            return None;
        }
        // A fragment is never part of an address.
        let rest = rest.split_once('#').map_or(rest, |(before, _)| before);
        let (path, query) = match rest.split_once('?') {
            Some((path, query)) => (path, Some(query)),
            None => (rest, None),
        };
        let mut address = Self::new(scheme);
        for segment in path.split('/').filter(|segment| !segment.is_empty()) {
            address.segments.push(decode(segment)?);
        }
        for pair in query.into_iter().flat_map(|query| query.split('&')) {
            if pair.is_empty() {
                continue;
            }
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            address.query.push((decode(key)?, decode(value)?));
        }
        Some(address)
    }

    pub fn scheme(&self) -> &str {
        &self.scheme
    }

    pub fn segments(&self) -> &[String] {
        &self.segments
    }

    /// The first value of query key `key`.
    pub fn query(&self, key: &str) -> Option<&str> {
        self.query
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }

    /// The address with `segment` added to its path.
    pub fn join(mut self, segment: impl Into<String>) -> Self {
        self.segments.push(segment.into());
        self
    }

    /// The address with query `key` set to `value`, replacing an earlier value.
    pub fn with_query(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        let (key, value) = (key.into(), value.into());
        self.query.retain(|(name, _)| *name != key);
        self.query.push((key, value));
        self
    }

    /// The address without its query.
    pub fn without_query(mut self) -> Self {
        self.query.clear();
        self
    }

    /// The address one segment up, or `None` at the root.
    pub fn parent(&self) -> Option<Self> {
        let mut up = self.clone().without_query();
        up.segments.pop()?;
        Some(up)
    }

    /// True when there are no segments.
    pub fn is_root(&self) -> bool {
        self.segments.is_empty()
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}://", self.scheme)?;
        for (index, segment) in self.segments.iter().enumerate() {
            if index > 0 {
                f.write_str("/")?;
            }
            f.write_str(&encode(segment))?;
        }
        for (index, (key, value)) in self.query.iter().enumerate() {
            f.write_str(if index == 0 { "?" } else { "&" })?;
            write!(f, "{}={}", encode(key), encode(value))?;
        }
        Ok(())
    }
}

/// `(scheme, rest)` when `text` starts with `scheme://`.
fn split_scheme(text: &str) -> Option<(&str, &str)> {
    let (scheme, rest) = text.split_once("://")?;
    valid_scheme(scheme).then_some((scheme, rest))
}

fn valid_scheme(scheme: &str) -> bool {
    let mut chars = scheme.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// Bytes written as they are. Everything else becomes `%XX`, so `/`, `?`,
/// `&`, `=`, `#`, `%`, spaces and non-ASCII text never end a segment early.
fn plain(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b':' | b'@' | b',')
}

fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for &byte in text.as_bytes() {
        if plain(byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let pair = bytes.get(index + 1..index + 3)?;
            let value = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
            out.push(value);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}
