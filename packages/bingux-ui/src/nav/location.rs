//! Locations: RFC 3986 URIs of the form `scheme://seg/seg?k=v#fragment`.
//!
//! Every place an application can be is a [`Location`]: a page, a subpage, a
//! folder. The convention, which the whole navigation system relies on:
//!
//! * **path segments** go into the hierarchy: a subpage adds a segment, and
//!   [`Location::up`] drops the last one;
//! * the **query** is one-off state that does not change the hierarchy (a
//!   filter, a selected tab, a nonce): changing only the query never slides;
//! * the **fragment** is an anchor inside the page: the page scrolls to the row
//!   with that anchor and flashes it.
//!
//! Segments and query values are percent-encoded when written and decoded when
//! parsed, so a segment may hold any text (a Wi-Fi name with spaces and
//! slashes) and a location always survives a round trip through text.
//!
//! A location is always canonical: the scheme is lower case, empty and `.`
//! segments are dropped, `..` pops a segment (RFC 3986 section 5.2.4, applied to
//! the raw text, so an encoded `%2E%2E` stays a literal segment), the query is
//! sorted by key and an empty fragment is dropped. Equal places therefore
//! compare equal.
//!
//! Two forms exist. `bingux-settings://network/wifi` has the first segment
//! where a host would be (the authority is not a host here, only the first
//! segment). `file:///home/me` has an empty authority and a rooted path
//! ([`Location::is_rooted`]); `file://localhost/...` is read as the rooted form.

use std::fmt;
use std::str::FromStr;

/// A place in an application, as a canonical URI.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Location {
    scheme: String,
    rooted: bool,
    segments: Vec<String>,
    query: Vec<(String, String)>,
    fragment: Option<String>,
}

impl Location {
    /// The root location of `scheme`: no segments, no query, not rooted.
    pub fn new(scheme: impl Into<String>) -> Self {
        Self {
            scheme: scheme.into().to_ascii_lowercase(),
            rooted: false,
            segments: Vec::new(),
            query: Vec::new(),
            fragment: None,
        }
    }

    /// The root of `scheme` in the rooted form, `scheme:///`.
    pub fn rooted(scheme: impl Into<String>) -> Self {
        Self {
            rooted: true,
            ..Self::new(scheme)
        }
    }

    /// Parse a full URI, `scheme://seg/seg?k=v#fragment`. `None` for a bare
    /// path, a bad scheme or a bad percent escape. Use [`Location::parse_in`]
    /// for input that may be a bare path.
    pub fn parse(text: &str) -> Option<Self> {
        let (scheme, rest) = split_scheme(text.trim())?;
        Self::from_parts(scheme, rest, false)
    }

    /// Parse `text` as a full URI, or as a bare path (`network/proxy`,
    /// `/network/proxy`) that takes `scheme`. A full URI with another scheme is
    /// returned as it is. A bare path starting with `/` is rooted.
    pub fn parse_in(scheme: &str, text: &str) -> Option<Self> {
        let text = text.trim();
        match split_scheme(text) {
            Some((found, rest)) => Self::from_parts(found, rest, false),
            None => Self::from_parts(scheme, text, true),
        }
    }

    fn from_parts(scheme: &str, rest: &str, bare: bool) -> Option<Self> {
        if !valid_scheme(scheme) {
            return None;
        }
        let (rest, fragment) = match rest.split_once('#') {
            Some((before, fragment)) => (before, Some(decode(fragment)?)),
            None => (rest, None),
        };
        let (path, query) = match rest.split_once('?') {
            Some((path, query)) => (path, Some(query)),
            None => (rest, None),
        };
        let mut location = Self::new(scheme);
        // `scheme:///path` and `file://localhost/path` are rooted. A bare
        // `/path` is rooted too.
        let mut path = path;
        if path.starts_with('/') {
            location.rooted = true;
        } else if !bare && location.scheme == "file" {
            if let Some(after) = path
                .strip_prefix("localhost")
                .filter(|after| after.is_empty() || after.starts_with('/'))
            {
                location.rooted = true;
                path = after;
            }
        }
        let mut segments: Vec<String> = Vec::new();
        for raw in path.split('/') {
            match raw {
                "" | "." => {}
                ".." => {
                    segments.pop();
                }
                raw => segments.push(decode(raw)?),
            }
        }
        location.segments = segments;
        for pair in query.into_iter().flat_map(|query| query.split('&')) {
            if pair.is_empty() {
                continue;
            }
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            let key = decode(key)?;
            if !key.is_empty() {
                location.query.push((key, decode(value)?));
            }
        }
        location.fragment = fragment.filter(|fragment| !fragment.is_empty());
        location.canonicalise();
        Some(location)
    }

    fn canonicalise(&mut self) {
        // A stable sort keeps repeated keys in the order they were written.
        self.query.sort_by(|a, b| a.0.cmp(&b.0));
    }

    pub fn scheme(&self) -> &str {
        &self.scheme
    }

    pub fn segments(&self) -> &[String] {
        &self.segments
    }

    /// Whether the path starts at an empty authority, as in `file:///home/me`.
    pub fn is_rooted(&self) -> bool {
        self.rooted
    }

    /// The first value of query key `key`.
    pub fn query(&self, key: &str) -> Option<&str> {
        self.query
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }

    /// Every query pair, sorted by key.
    pub fn query_pairs(&self) -> &[(String, String)] {
        &self.query
    }

    /// The anchor inside the page, if any.
    pub fn fragment(&self) -> Option<&str> {
        self.fragment.as_deref()
    }

    /// The last path segment.
    pub fn last_segment(&self) -> Option<&str> {
        self.segments.last().map(String::as_str)
    }

    /// The number of path segments.
    pub fn depth(&self) -> usize {
        self.segments.len()
    }

    /// True when there are no segments.
    pub fn is_root(&self) -> bool {
        self.segments.is_empty()
    }

    /// The location with `segment` added to its path. The query and fragment
    /// belong to one place, so they are dropped.
    pub fn join(mut self, segment: impl Into<String>) -> Self {
        self.segments.push(segment.into());
        self.query.clear();
        self.fragment = None;
        self
    }

    /// The location with query `key` set to `value`, replacing earlier values.
    pub fn with_query(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        let (key, value) = (key.into(), value.into());
        self.query.retain(|(name, _)| *name != key);
        self.query.push((key, value));
        self.canonicalise();
        self
    }

    /// The location without query key `key`.
    pub fn without_query_key(mut self, key: &str) -> Self {
        self.query.retain(|(name, _)| name != key);
        self
    }

    /// The location without its query.
    pub fn without_query(mut self) -> Self {
        self.query.clear();
        self
    }

    /// The location with fragment `fragment`; an empty one removes it.
    pub fn with_fragment(mut self, fragment: impl Into<String>) -> Self {
        let fragment = fragment.into();
        self.fragment = (!fragment.is_empty()).then_some(fragment);
        self
    }

    /// The location without its fragment.
    pub fn without_fragment(mut self) -> Self {
        self.fragment = None;
        self
    }

    /// The location one segment up, with no query or fragment, or `None` at
    /// the root.
    pub fn up(&self) -> Option<Self> {
        let mut up = self.clone().without_query().without_fragment();
        up.segments.pop()?;
        Some(up)
    }

    /// The same place: scheme, path and query, ignoring the fragment. Two
    /// entries for the same place are one page.
    pub fn same_place(&self, other: &Self) -> bool {
        self.scheme == other.scheme
            && self.rooted == other.rooted
            && self.segments == other.segments
            && self.query == other.query
    }

    /// The same path, ignoring query and fragment.
    pub fn same_path(&self, other: &Self) -> bool {
        self.scheme == other.scheme && self.segments == other.segments
    }

    /// Whether this location is below `ancestor` in the hierarchy: the same
    /// scheme and `ancestor`'s segments are a strict prefix of this path.
    pub fn is_below(&self, ancestor: &Self) -> bool {
        self.scheme == ancestor.scheme
            && self.segments.len() > ancestor.segments.len()
            && self.segments.starts_with(&ancestor.segments)
    }

    /// The path as text, `/network/wifi`, segments unescaped.
    pub fn path_text(&self) -> String {
        let mut text = String::new();
        for segment in &self.segments {
            text.push('/');
            text.push_str(segment);
        }
        if text.is_empty() {
            text.push('/');
        }
        text
    }
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}://", self.scheme)?;
        if self.rooted {
            f.write_str("/")?;
        }
        for (index, segment) in self.segments.iter().enumerate() {
            if index > 0 {
                f.write_str("/")?;
            }
            f.write_str(&encode_segment(segment))?;
        }
        for (index, (key, value)) in self.query.iter().enumerate() {
            f.write_str(if index == 0 { "?" } else { "&" })?;
            write!(f, "{}={}", encode(key), encode(value))?;
        }
        if let Some(fragment) = &self.fragment {
            write!(f, "#{}", encode(fragment))?;
        }
        Ok(())
    }
}

impl FromStr for Location {
    type Err = InvalidLocation;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse(text).ok_or_else(|| InvalidLocation(text.to_string()))
    }
}

/// The text is not a location: no scheme, a bad scheme or a bad escape.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvalidLocation(pub String);

impl fmt::Display for InvalidLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "not a location: {}", self.0)
    }
}

impl std::error::Error for InvalidLocation {}

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
/// `&`, `=`, `#`, `%`, `+`, spaces and non-ASCII text never end a part early.
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

/// A segment of `.` or `..` is written escaped, so it is never a dot segment.
fn encode_segment(text: &str) -> String {
    match text {
        "." => "%2E".to_string(),
        ".." => "%2E%2E".to_string(),
        text => encode(text),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn loc(text: &str) -> Location {
        Location::parse(text).unwrap_or_else(|| panic!("{text} should parse"))
    }

    #[test]
    fn a_location_round_trips_through_text() {
        for text in [
            "bingux-settings://",
            "bingux-settings://network",
            "bingux-settings://network/wifi/saved/Home%20Net",
            "bingux-settings://network?id=enp42s0",
            "bingux-settings://network/proxy#method",
            "bingux-settings://sound/applications?filter=a%26b&tab=x#row",
            "file:///",
            "file:///home/kieran/Documents",
            "file:///home/kieran/My%20Files/a%2Fb",
            "bingux-files://home/me?launch=123",
        ] {
            assert_eq!(loc(text).to_string(), text, "{text}");
        }
    }

    #[test]
    fn awkward_text_survives_a_round_trip() {
        let name = "caf\u{e9} / ?#&=% + \u{1f4f6}";
        let made = Location::new("app")
            .join(name)
            .join("..")
            .join(".")
            .with_query("q", name)
            .with_fragment(name);
        let again = loc(&made.to_string());
        assert_eq!(again, made);
        assert_eq!(again.segments(), [name.to_string(), "..".into(), ".".into()]);
        assert_eq!(again.query("q"), Some(name));
        assert_eq!(again.fragment(), Some(name));
    }

    #[test]
    fn parts_are_read_where_the_convention_puts_them() {
        let place = loc("bingux-settings://network/wifi/network/Home?tab=saved&a=1#password");
        assert_eq!(place.scheme(), "bingux-settings");
        assert_eq!(place.segments(), ["network", "wifi", "network", "Home"]);
        assert_eq!(place.query("tab"), Some("saved"));
        assert_eq!(place.fragment(), Some("password"));
        assert!(!place.is_rooted());
        assert_eq!(place.depth(), 4);
    }

    #[test]
    fn the_canonical_form_makes_equal_places_equal() {
        let canonical = loc("Bingux-Settings://network/wifi?a=1&b=2");
        for same in [
            "bingux-settings://network/wifi/?b=2&a=1",
            "bingux-settings://network//wifi?a=1&b=2",
            "bingux-settings://network/./wifi?a=1&b=2",
            "bingux-settings://network/x/../wifi?b=2&a=1#",
            "BINGUX-SETTINGS://network/wifi?a=1&b=2&",
        ] {
            assert_eq!(loc(same), canonical, "{same}");
            assert_eq!(loc(same).to_string(), canonical.to_string());
        }
        assert_ne!(loc("bingux-settings://network/wifi?a=1"), canonical);
        assert_ne!(loc("bingux-settings://network/wifi#x"), loc("bingux-settings://network/wifi"));
    }

    #[test]
    fn dot_segments_are_removed_but_encoded_dots_are_literal() {
        assert_eq!(loc("a://x/y/../z").segments(), ["x", "z"]);
        assert_eq!(loc("a://x/../../z").segments(), ["z"]);
        assert_eq!(loc("a://x/%2E%2E/z").segments(), ["x", "..", "z"]);
        assert_eq!(loc("a://x/%2E/z").segments(), ["x", ".", "z"]);
    }

    #[test]
    fn a_rooted_location_keeps_its_form() {
        let folder = loc("file:///home/kieran/Documents");
        assert!(folder.is_rooted());
        assert_eq!(folder.segments(), ["home", "kieran", "Documents"]);
        assert_eq!(folder.path_text(), "/home/kieran/Documents");
        assert_eq!(loc("file://localhost/etc").to_string(), "file:///etc");
        assert_eq!(loc("file:///").path_text(), "/");
        assert!(loc("file:///").is_root());
        assert_eq!(Location::rooted("file").to_string(), "file:///");
        // Only the file scheme has a `localhost` authority.
        assert_eq!(loc("a://localhost/x").segments(), ["localhost", "x"]);
    }

    #[test]
    fn up_drops_the_last_segment_and_everything_after_the_path() {
        let place = loc("app://a/b/c?x=1#f");
        assert_eq!(place.up(), Some(loc("app://a/b")));
        assert_eq!(loc("app://a").up(), Some(loc("app://")));
        assert_eq!(loc("app://").up(), None);
        assert_eq!(loc("file:///home").up(), Some(loc("file:///")));
    }

    #[test]
    fn same_place_ignores_only_the_fragment() {
        assert!(loc("app://a?x=1#one").same_place(&loc("app://a?x=1#two")));
        assert!(!loc("app://a?x=1").same_place(&loc("app://a?x=2")));
        assert!(loc("app://a?x=1").same_path(&loc("app://a?x=2")));
        assert!(!loc("app://a").same_path(&loc("app://a/b")));
    }

    #[test]
    fn below_is_a_strict_path_prefix() {
        assert!(loc("app://a/b/c").is_below(&loc("app://a")));
        assert!(loc("app://a").is_below(&loc("app://")));
        assert!(!loc("app://a").is_below(&loc("app://a")));
        assert!(!loc("app://ab/c").is_below(&loc("app://a")));
        assert!(!loc("other://a/b").is_below(&loc("app://a")));
    }

    #[test]
    fn bare_paths_take_the_scheme_and_bad_text_is_rejected() {
        assert_eq!(
            Location::parse_in("app", "network/proxy#m").unwrap(),
            loc("app://network/proxy#m")
        );
        assert_eq!(Location::parse_in("app", "/home/me").unwrap(), loc("app:///home/me"));
        assert_eq!(
            Location::parse_in("app", "other://x/y").unwrap(),
            loc("other://x/y")
        );
        assert!(Location::parse("network/proxy").is_none());
        assert!(Location::parse("9bad://x").is_none());
        assert!(Location::parse("app://x/%zz").is_none());
        assert!(Location::parse("app://x/%4").is_none());
        assert!(Location::parse("app://x/%ff").is_none(), "not UTF-8");
        assert!("garbage".parse::<Location>().is_err());
    }

    #[test]
    fn query_edits_keep_the_order_canonical() {
        let place = loc("app://a").with_query("z", "1").with_query("b", "2").with_query("z", "3");
        assert_eq!(place.to_string(), "app://a?b=2&z=3");
        assert_eq!(place.clone().without_query_key("b").to_string(), "app://a?z=3");
        assert_eq!(place.without_query().to_string(), "app://a");
    }
}
