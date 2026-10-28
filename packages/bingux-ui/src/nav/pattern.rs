//! Path patterns: how a route says which locations it owns.
//!
//! A pattern is written like the path it matches, with parameters:
//!
//! | Segment | Matches |
//! | ------- | ------- |
//! | `wifi` | exactly that text |
//! | `:ssid` | any one segment, bound to `ssid` |
//! | `:index:int` | a segment of ASCII digits, bound to `index` |
//! | `*path` | all remaining segments (last position only), bound to `path` joined by `/` |
//!
//! When several patterns match one location the most specific wins: at the
//! first segment where they differ a literal beats an integer, which beats a
//! string, which beats a rest. `applications/default` therefore wins over
//! `applications/:id` for that location.

/// One segment of a [`Pattern`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Part {
    Literal(String),
    Str(String),
    Int(String),
    Rest(String),
}

impl Part {
    /// How specific the part is, for choosing between matching patterns.
    fn weight(&self) -> u8 {
        match self {
            Part::Literal(_) => 3,
            Part::Int(_) => 2,
            Part::Str(_) => 1,
            Part::Rest(_) => 0,
        }
    }
}

/// A parsed path pattern.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pattern {
    text: String,
    parts: Vec<Part>,
}

/// The values a pattern bound, in pattern order.
pub type Bindings = Vec<(String, String)>;

impl Pattern {
    /// Parse `text`. Panics on a malformed pattern: patterns are written in
    /// source (a route table), so a bad one is a programming error found by
    /// the first test, never an input error.
    pub fn new(text: &str) -> Self {
        let mut parts = Vec::new();
        let pieces: Vec<&str> = text.split('/').filter(|piece| !piece.is_empty()).collect();
        for (index, piece) in pieces.iter().enumerate() {
            let part = if let Some(name) = piece.strip_prefix('*') {
                assert!(
                    index + 1 == pieces.len() && !name.is_empty(),
                    "pattern `{text}`: `*name` must be the last segment and named"
                );
                Part::Rest(name.to_string())
            } else if let Some(param) = piece.strip_prefix(':') {
                let (name, kind) = param.split_once(':').unwrap_or((param, "str"));
                assert!(!name.is_empty(), "pattern `{text}`: an unnamed parameter");
                match kind {
                    "str" => Part::Str(name.to_string()),
                    "int" => Part::Int(name.to_string()),
                    other => panic!("pattern `{text}`: unknown parameter type `{other}`"),
                }
            } else {
                Part::Literal((*piece).to_string())
            };
            parts.push(part);
        }
        Self {
            text: text.trim_matches('/').to_string(),
            parts,
        }
    }

    /// The pattern as written, without surrounding slashes.
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn parts(&self) -> &[Part] {
        &self.parts
    }

    /// The number of segments, counting a rest as one.
    pub fn len(&self) -> usize {
        self.parts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    /// The bindings when `segments` match, else `None`.
    pub fn matches(&self, segments: &[String]) -> Option<Bindings> {
        let rest = matches!(self.parts.last(), Some(Part::Rest(_)));
        if rest {
            if segments.len() < self.parts.len() - 1 {
                return None;
            }
        } else if segments.len() != self.parts.len() {
            return None;
        }
        let mut bound = Bindings::new();
        for (index, part) in self.parts.iter().enumerate() {
            match part {
                Part::Literal(text) => {
                    if segments.get(index)? != text {
                        return None;
                    }
                }
                Part::Str(name) => bound.push((name.clone(), segments.get(index)?.clone())),
                Part::Int(name) => {
                    let value = segments.get(index)?;
                    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
                        return None;
                    }
                    bound.push((name.clone(), value.clone()));
                }
                Part::Rest(name) => {
                    bound.push((name.clone(), segments[index..].join("/")));
                }
            }
        }
        Some(bound)
    }

    /// The segments for `bindings`, or `None` when one is missing. The inverse
    /// of [`Pattern::matches`]: a rest binding is split on `/`.
    pub fn build(&self, bindings: &[(&str, String)]) -> Option<Vec<String>> {
        let mut segments = Vec::new();
        for part in &self.parts {
            let lookup = |name: &str| {
                bindings
                    .iter()
                    .find(|(key, _)| *key == name)
                    .map(|(_, value)| value.clone())
            };
            match part {
                Part::Literal(text) => segments.push(text.clone()),
                Part::Str(name) | Part::Int(name) => segments.push(lookup(name)?),
                Part::Rest(name) => segments.extend(
                    lookup(name)?
                        .split('/')
                        .filter(|piece| !piece.is_empty())
                        .map(str::to_string),
                ),
            }
        }
        Some(segments)
    }

    /// Orders matching patterns: a greater score is more specific.
    pub fn score(&self) -> Vec<u8> {
        self.parts.iter().map(Part::weight).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segments(text: &str) -> Vec<String> {
        text.split('/')
            .filter(|piece| !piece.is_empty())
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn literals_match_exactly() {
        let pattern = Pattern::new("network/wifi");
        assert_eq!(pattern.matches(&segments("network/wifi")), Some(vec![]));
        assert_eq!(pattern.matches(&segments("network")), None);
        assert_eq!(pattern.matches(&segments("network/wifi/x")), None);
        assert_eq!(pattern.matches(&segments("network/wired")), None);
    }

    #[test]
    fn the_empty_pattern_is_the_root() {
        let pattern = Pattern::new("");
        assert!(pattern.is_empty());
        assert_eq!(pattern.matches(&[]), Some(vec![]));
        assert_eq!(pattern.matches(&segments("a")), None);
    }

    #[test]
    fn parameters_bind_and_typed_parameters_check() {
        let pattern = Pattern::new("network/:device/profile/:uuid");
        let bound = pattern.matches(&segments("network/enp42s0/profile/abc-1")).unwrap();
        assert_eq!(
            bound,
            vec![("device".into(), "enp42s0".into()), ("uuid".into(), "abc-1".into())]
        );
        let number = Pattern::new("display/:index:int");
        assert_eq!(
            number.matches(&segments("display/3")),
            Some(vec![("index".into(), "3".into())])
        );
        assert_eq!(number.matches(&segments("display/x")), None);
        assert_eq!(number.matches(&segments("display/-1")), None);
    }

    #[test]
    fn a_rest_takes_the_remaining_segments() {
        let pattern = Pattern::new("*path");
        assert_eq!(
            pattern.matches(&segments("home/me/Documents")),
            Some(vec![("path".into(), "home/me/Documents".into())])
        );
        assert_eq!(pattern.matches(&[]), Some(vec![("path".into(), String::new())]));
        let below = Pattern::new("files/*path");
        assert_eq!(below.matches(&segments("files")), Some(vec![("path".into(), String::new())]));
        assert_eq!(below.matches(&segments("other/x")), None);
    }

    #[test]
    fn the_most_specific_pattern_wins_by_score() {
        let literal = Pattern::new("applications/default").score();
        let string = Pattern::new("applications/:id").score();
        let number = Pattern::new("applications/:id:int").score();
        let rest = Pattern::new("applications/*all").score();
        assert!(literal > number && number > string && string > rest);
    }

    #[test]
    fn build_is_the_inverse_of_matches() {
        let pattern = Pattern::new("network/:device/profile/:uuid");
        let built = pattern
            .build(&[("device", "enp42s0".into()), ("uuid", "abc".into())])
            .unwrap();
        assert_eq!(built, segments("network/enp42s0/profile/abc"));
        assert!(pattern.build(&[("device", "x".into())]).is_none());
        let rest = Pattern::new("*path");
        assert_eq!(
            rest.build(&[("path", "a/b".into())]).unwrap(),
            segments("a/b")
        );
    }

    #[test]
    #[should_panic(expected = "last segment")]
    fn a_rest_must_be_last() {
        Pattern::new("*path/more");
    }

    #[test]
    #[should_panic(expected = "unknown parameter type")]
    fn unknown_parameter_types_are_rejected() {
        Pattern::new(":x:float");
    }
}
