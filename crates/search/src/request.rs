//! Search requests: the builder API, the Meilisearch-like shorthand and the
//! OpenSearch body subset all end up as a [`Search`].
//!
//! The accepted OpenSearch subset is listed in `design/search.md` 6.1. A key
//! outside it is an error naming the key, never silently ignored.

use std::time::Duration;

use serde_json::Value;

use crate::error::{Error, Result};
use crate::filter::{Filter, Lit};

/// How many of a query's words a document must match.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Strategy {
    /// Every word.
    All,
    /// All words first; words are dropped from the end until there are enough
    /// hits. Meilisearch's default.
    #[default]
    Last,
    /// Any word; more words rank higher.
    Any,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PrefixMode {
    /// The last word is a prefix unless the text ends with a space.
    #[default]
    Auto,
    On,
    Off,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextQuery {
    pub text: String,
    /// Fields to search with boosts; empty searches every searchable field.
    pub fields: Vec<(String, f32)>,
    /// `None` takes the request's strategy.
    pub strategy: Option<Strategy>,
    /// All words must be adjacent, in order, in one field.
    pub phrase: bool,
    pub prefix: PrefixMode,
    /// Typo tolerance (still subject to the schema).
    pub fuzzy: bool,
}

impl TextQuery {
    pub fn new(text: &str) -> Self {
        TextQuery {
            text: text.to_string(),
            fields: Vec::new(),
            strategy: None,
            phrase: false,
            prefix: PrefixMode::Auto,
            fuzzy: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct BoolQuery {
    pub must: Vec<Query>,
    pub should: Vec<Query>,
    pub filter: Vec<Query>,
    pub must_not: Vec<Query>,
    pub minimum_should_match: Option<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Query {
    MatchAll,
    Text(TextQuery),
    Bool(Box<BoolQuery>),
    Filter(Filter),
    /// `term`: a filter when the field is filterable, else an exact word.
    Term(String, Lit),
    Terms(String, Vec<Lit>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum SortKey {
    Score,
    Id,
    Field(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct SortSpec {
    pub key: SortKey,
    pub desc: bool,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub enum SourceSpec {
    #[default]
    All,
    Nothing,
    Only {
        includes: Vec<String>,
        excludes: Vec<String>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct HighlightSpec {
    /// Empty highlights every searchable field.
    pub fields: Vec<String>,
    pub pre_tag: String,
    pub post_tag: String,
    /// Characters of a fragment; 0 returns the whole value.
    pub fragment_size: usize,
}

impl Default for HighlightSpec {
    fn default() -> Self {
        HighlightSpec {
            fields: Vec::new(),
            pre_tag: "<em>".into(),
            post_tag: "</em>".into(),
            fragment_size: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AggSpec {
    pub name: String,
    pub field: String,
    pub size: usize,
}

/// A search. Build with [`Search::new`] or parse with [`Search::from_json`].
#[derive(Clone, Debug, PartialEq)]
pub struct Search {
    /// Index names for a service search; empty means every index.
    pub indexes: Vec<String>,
    pub query: Query,
    pub filter: Option<Filter>,
    /// Filter expressions in the Meilisearch grammar, parsed when run.
    pub filter_exprs: Vec<String>,
    pub sort: Vec<SortSpec>,
    pub from: usize,
    pub size: usize,
    pub search_after: Option<Vec<Value>>,
    pub source: SourceSpec,
    pub highlight: Option<HighlightSpec>,
    pub aggs: Vec<AggSpec>,
    /// Count hits up to this many; beyond it `total.relation` is `gte`.
    /// `None` counts all.
    pub track_total_hits: Option<usize>,
    pub strategy: Strategy,
    pub timeout: Option<Duration>,
    /// Only what other processes may see: external indexes and documents.
    /// Set by the provider of a socket; not part of any request body.
    pub external_only: bool,
}

pub const MAX_PAGE: usize = 1000;
pub const MAX_WINDOW: usize = 10_000;

impl Search {
    /// A text search over every searchable field. Empty text matches all.
    pub fn new(text: &str) -> Self {
        Search {
            indexes: Vec::new(),
            query: if text.trim().is_empty() {
                Query::MatchAll
            } else {
                Query::Text(TextQuery::new(text))
            },
            filter: None,
            filter_exprs: Vec::new(),
            sort: Vec::new(),
            from: 0,
            size: 10,
            search_after: None,
            source: SourceSpec::All,
            highlight: None,
            aggs: Vec::new(),
            track_total_hits: Some(10_000),
            strategy: Strategy::Last,
            timeout: None,
            external_only: false,
        }
    }

    pub fn match_all() -> Self {
        Search::new("")
    }

    /// Searches this index (repeat for several).
    pub fn index(mut self, name: &str) -> Self {
        self.indexes.push(name.to_string());
        self
    }

    /// Adds a filter expression, `category = 'Network' AND tags IN [a, b]`.
    pub fn filter(mut self, expression: &str) -> Self {
        self.filter_exprs.push(expression.to_string());
        self
    }

    pub fn filter_ast(mut self, filter: Filter) -> Self {
        self.filter = Some(match self.filter.take() {
            Some(f) => f.and(filter),
            None => filter,
        });
        self
    }

    pub fn limit(mut self, size: usize) -> Self {
        self.size = size;
        self
    }

    pub fn offset(mut self, from: usize) -> Self {
        self.from = from;
        self
    }

    pub fn strategy(mut self, strategy: Strategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// Restricts a text query to these fields (`"title^3"` boosts).
    pub fn fields(mut self, fields: &[&str]) -> Self {
        if let Query::Text(t) = &mut self.query {
            t.fields = fields.iter().map(|f| parse_boost(f)).collect();
        }
        self
    }

    /// Sorts by a sortable field before relevance: `"price:asc"`.
    pub fn sort(mut self, spec: &str) -> Self {
        let (name, dir) = spec.rsplit_once(':').unwrap_or((spec, "asc"));
        let key = match name {
            "_score" => SortKey::Score,
            "_id" => SortKey::Id,
            other => SortKey::Field(other.to_string()),
        };
        self.sort.push(SortSpec {
            key,
            desc: dir.eq_ignore_ascii_case("desc"),
        });
        self
    }

    /// Highlights these fields (empty: every searchable field).
    pub fn highlight(mut self, fields: &[&str]) -> Self {
        self.highlight = Some(HighlightSpec {
            fields: fields.iter().map(|s| s.to_string()).collect(),
            ..HighlightSpec::default()
        });
        self
    }

    /// Counts the values of a filterable field among the matches.
    pub fn facet(mut self, field: &str) -> Self {
        self.aggs.push(AggSpec {
            name: field.to_string(),
            field: field.to_string(),
            size: 100,
        });
        self
    }

    pub fn source_only(mut self, fields: &[&str]) -> Self {
        self.source = SourceSpec::Only {
            includes: fields.iter().map(|s| s.to_string()).collect(),
            excludes: Vec::new(),
        };
        self
    }

    /// Continues after a hit of a previous page (cursor pagination). The sort
    /// must end in `_id`; without a sort it is `_score desc, _id asc`.
    pub fn search_after(mut self, values: Vec<Value>) -> Self {
        self.search_after = Some(values);
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Parses a request body: an OpenSearch body (`query`, `from`, ...) or the
    /// Meilisearch shorthand (`q`, `filter`, ...).
    pub fn from_json(body: &Value) -> Result<Search> {
        let Some(obj) = body.as_object() else {
            return Err(Error::parsing("the request body must be an object"));
        };
        if obj.contains_key("q") || (!obj.contains_key("query") && obj.contains_key("filter")) {
            parse_shorthand(obj)
        } else {
            parse_opensearch(obj)
        }
    }
}

fn parse_boost(field: &str) -> (String, f32) {
    match field.split_once('^') {
        Some((name, boost)) => (name.to_string(), boost.parse().unwrap_or(1.0)),
        None => (field.to_string(), 1.0),
    }
}

fn get_usize(obj: &serde_json::Map<String, Value>, key: &str) -> Result<Option<usize>> {
    match obj.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .map(|n| Some(n as usize))
            .ok_or_else(|| Error::parsing(format!("`{key}` must be a non-negative integer"))),
    }
}

fn string_list(v: &Value, key: &str) -> Result<Vec<String>> {
    match v {
        Value::String(s) => Ok(vec![s.clone()]),
        Value::Array(a) => a
            .iter()
            .map(|x| {
                x.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| Error::parsing(format!("`{key}` must be a list of strings")))
            })
            .collect(),
        _ => Err(Error::parsing(format!("`{key}` must be a list of strings"))),
    }
}

fn parse_shorthand(obj: &serde_json::Map<String, Value>) -> Result<Search> {
    const KEYS: &[&str] = &[
        "q",
        "filter",
        "facets",
        "sort",
        "limit",
        "offset",
        "attributesToHighlight",
        "highlightPreTag",
        "highlightPostTag",
        "attributesToRetrieve",
        "attributesToSearchOn",
        "matchingStrategy",
        "index",
        "indexes",
    ];
    for k in obj.keys() {
        if !KEYS.contains(&k.as_str()) {
            return Err(Error::parsing(format!("unknown search parameter `{k}`")));
        }
    }
    let q = obj.get("q").and_then(Value::as_str).unwrap_or("");
    let mut s = Search::new(q);
    if let Some(v) = obj.get("index").or_else(|| obj.get("indexes")) {
        s.indexes = string_list(v, "index")?;
    }
    if let Some(v) = obj.get("filter") {
        s.filter_exprs = shorthand_filter(v)?;
    }
    if let Some(v) = obj.get("facets") {
        for f in string_list(v, "facets")? {
            s = s.facet(&f);
        }
    }
    if let Some(v) = obj.get("sort") {
        for spec in string_list(v, "sort")? {
            s = s.sort(&spec);
        }
    }
    if let Some(n) = get_usize(obj, "limit")? {
        s.size = n;
    }
    if let Some(n) = get_usize(obj, "offset")? {
        s.from = n;
    }
    if let Some(v) = obj.get("attributesToHighlight") {
        let fields = string_list(v, "attributesToHighlight")?;
        let all = fields.iter().any(|f| f == "*");
        let mut h = HighlightSpec {
            fields: if all { Vec::new() } else { fields },
            ..HighlightSpec::default()
        };
        if let Some(t) = obj.get("highlightPreTag").and_then(Value::as_str) {
            h.pre_tag = t.to_string();
        }
        if let Some(t) = obj.get("highlightPostTag").and_then(Value::as_str) {
            h.post_tag = t.to_string();
        }
        s.highlight = Some(h);
    }
    if let Some(v) = obj.get("attributesToRetrieve") {
        let fields = string_list(v, "attributesToRetrieve")?;
        if !fields.iter().any(|f| f == "*") {
            s.source = SourceSpec::Only {
                includes: fields,
                excludes: Vec::new(),
            };
        }
    }
    if let Some(v) = obj.get("attributesToSearchOn") {
        let fields = string_list(v, "attributesToSearchOn")?;
        if let Query::Text(t) = &mut s.query {
            t.fields = fields.into_iter().map(|f| (f, 1.0)).collect();
        }
    }
    if let Some(v) = obj.get("matchingStrategy").and_then(Value::as_str) {
        s.strategy = match v {
            "last" => Strategy::Last,
            "all" => Strategy::All,
            "frequency" | "any" => Strategy::Any,
            other => {
                return Err(Error::parsing(format!(
                    "matchingStrategy `{other}` is not last, all or any"
                )));
            }
        };
    }
    Ok(s)
}

/// A Meilisearch filter: a string, or an array whose strings are ANDed and
/// whose nested arrays are ORed.
fn shorthand_filter(v: &Value) -> Result<Vec<String>> {
    match v {
        Value::String(s) => Ok(vec![s.clone()]),
        Value::Array(items) => {
            let mut out = Vec::new();
            for item in items {
                match item {
                    Value::String(s) => out.push(s.clone()),
                    Value::Array(group) => {
                        let parts: Result<Vec<String>> = group
                            .iter()
                            .map(|g| {
                                g.as_str()
                                    .map(|s| format!("({s})"))
                                    .ok_or_else(|| Error::parsing("filter groups hold strings"))
                            })
                            .collect();
                        out.push(format!("({})", parts?.join(" OR ")));
                    }
                    _ => return Err(Error::parsing("filter entries are strings or arrays")),
                }
            }
            Ok(out)
        }
        _ => Err(Error::parsing("`filter` must be a string or an array")),
    }
}

fn parse_opensearch(obj: &serde_json::Map<String, Value>) -> Result<Search> {
    let mut s = Search::new("");
    s.indexes = Vec::new();
    for (key, value) in obj {
        match key.as_str() {
            "query" => s.query = parse_query(value)?,
            "from" => {
                s.from = value
                    .as_u64()
                    .ok_or_else(|| Error::parsing("`from` must be a non-negative integer"))?
                    as usize
            }
            "size" => {
                s.size = value
                    .as_u64()
                    .ok_or_else(|| Error::parsing("`size` must be a non-negative integer"))?
                    as usize
            }
            "sort" => s.sort = parse_sort(value)?,
            "search_after" => {
                s.search_after = Some(
                    value
                        .as_array()
                        .ok_or_else(|| Error::parsing("`search_after` must be an array"))?
                        .clone(),
                )
            }
            "_source" => s.source = parse_source(value)?,
            "highlight" => s.highlight = Some(parse_highlight(value)?),
            "track_total_hits" => {
                s.track_total_hits = match value {
                    Value::Bool(true) => None,
                    Value::Bool(false) => Some(0),
                    Value::Number(n) => Some(n.as_u64().unwrap_or(10_000) as usize),
                    _ => return Err(Error::parsing("`track_total_hits` is a bool or a number")),
                }
            }
            "timeout" => s.timeout = Some(parse_timeout(value)?),
            "aggs" | "aggregations" => s.aggs = parse_aggs(value)?,
            "track_scores" | "version" | "seq_no_primary_term" | "explain" | "stored_fields" => {}
            other => {
                return Err(Error::parsing(format!(
                    "unknown or unsupported search key [{other}]"
                )));
            }
        }
    }
    // OpenSearch `match` defaults to OR; text queries carry their own strategy.
    Ok(s)
}

fn parse_timeout(v: &Value) -> Result<Duration> {
    match v {
        Value::Number(n) => Ok(Duration::from_millis(n.as_u64().unwrap_or(0))),
        Value::String(s) => {
            let s = s.trim();
            let (num, mult) = if let Some(n) = s.strip_suffix("ms") {
                (n, 1.0)
            } else if let Some(n) = s.strip_suffix("micros") {
                (n, 0.001)
            } else if let Some(n) = s.strip_suffix('s') {
                (n, 1000.0)
            } else {
                (s, 1.0)
            };
            num.parse::<f64>()
                .map(|n| Duration::from_secs_f64(n * mult / 1000.0))
                .map_err(|_| Error::parsing(format!("bad timeout [{s}]")))
        }
        _ => Err(Error::parsing("`timeout` is a duration string")),
    }
}

fn parse_sort(v: &Value) -> Result<Vec<SortSpec>> {
    let items = match v {
        Value::Array(a) => a.clone(),
        other => vec![other.clone()],
    };
    let mut out = Vec::new();
    for item in items {
        let (name, desc) = match &item {
            Value::String(s) => (s.clone(), s == "_score"),
            Value::Object(m) if m.len() == 1 => {
                let (name, spec) = m.iter().next().unwrap();
                let order = match spec {
                    Value::String(o) => Some(o.as_str()),
                    Value::Object(o) => o.get("order").and_then(Value::as_str),
                    _ => None,
                };
                let desc = match order {
                    Some("desc") => true,
                    Some("asc") => false,
                    None => name == "_score",
                    Some(other) => {
                        return Err(Error::parsing(format!("sort order [{other}] is not asc or desc")));
                    }
                };
                (name.clone(), desc)
            }
            _ => return Err(Error::parsing("a sort entry is a field name or {field: order}")),
        };
        out.push(SortSpec {
            key: match name.as_str() {
                "_score" => SortKey::Score,
                "_id" => SortKey::Id,
                other => SortKey::Field(other.to_string()),
            },
            desc,
        });
    }
    Ok(out)
}

fn parse_source(v: &Value) -> Result<SourceSpec> {
    match v {
        Value::Bool(true) => Ok(SourceSpec::All),
        Value::Bool(false) => Ok(SourceSpec::Nothing),
        Value::String(s) => Ok(SourceSpec::Only {
            includes: vec![s.clone()],
            excludes: Vec::new(),
        }),
        Value::Array(_) => Ok(SourceSpec::Only {
            includes: string_list(v, "_source")?,
            excludes: Vec::new(),
        }),
        Value::Object(m) => {
            for k in m.keys() {
                if !["includes", "excludes", "include", "exclude"].contains(&k.as_str()) {
                    return Err(Error::parsing(format!("unknown _source key [{k}]")));
                }
            }
            let list = |a: &str, b: &str| -> Result<Vec<String>> {
                match m.get(a).or_else(|| m.get(b)) {
                    Some(v) => string_list(v, a),
                    None => Ok(Vec::new()),
                }
            };
            Ok(SourceSpec::Only {
                includes: list("includes", "include")?,
                excludes: list("excludes", "exclude")?,
            })
        }
        _ => Err(Error::parsing("bad `_source`")),
    }
}

fn parse_highlight(v: &Value) -> Result<HighlightSpec> {
    let Some(m) = v.as_object() else {
        return Err(Error::parsing("`highlight` must be an object"));
    };
    let mut h = HighlightSpec::default();
    for (k, val) in m {
        match k.as_str() {
            "fields" => match val {
                Value::Object(f) => {
                    h.fields = f.keys().cloned().collect();
                    for spec in f.values() {
                        if let Some(n) = spec.get("fragment_size").and_then(Value::as_u64) {
                            h.fragment_size = n as usize;
                        }
                    }
                }
                Value::Array(a) => {
                    for item in a {
                        if let Some(o) = item.as_object() {
                            h.fields.extend(o.keys().cloned());
                        }
                    }
                }
                _ => return Err(Error::parsing("highlight.fields must be an object")),
            },
            "pre_tags" => {
                h.pre_tag = string_list(val, k)?.into_iter().next().unwrap_or_default();
            }
            "post_tags" => {
                h.post_tag = string_list(val, k)?.into_iter().next().unwrap_or_default();
            }
            "fragment_size" => h.fragment_size = val.as_u64().unwrap_or(0) as usize,
            "number_of_fragments" | "type" | "encoder" | "order" | "require_field_match" => {}
            other => {
                return Err(Error::parsing(format!("unsupported highlight key [{other}]")));
            }
        }
    }
    Ok(h)
}

fn parse_aggs(v: &Value) -> Result<Vec<AggSpec>> {
    let Some(m) = v.as_object() else {
        return Err(Error::parsing("`aggs` must be an object"));
    };
    let mut out = Vec::new();
    for (name, spec) in m {
        let Some(o) = spec.as_object() else {
            return Err(Error::parsing(format!("aggregation [{name}] must be an object")));
        };
        let Some(terms) = o.get("terms") else {
            let kind = o.keys().next().cloned().unwrap_or_default();
            return Err(Error::parsing(format!(
                "aggregation type [{kind}] is not supported; only `terms`"
            )));
        };
        let Some(field) = terms.get("field").and_then(Value::as_str) else {
            return Err(Error::parsing(format!("aggregation [{name}] needs a field")));
        };
        let size = terms.get("size").and_then(Value::as_u64).unwrap_or(10) as usize;
        out.push(AggSpec {
            name: name.clone(),
            field: field.to_string(),
            size,
        });
    }
    Ok(out)
}

fn lit_of(v: &Value) -> Result<Lit> {
    match v {
        Value::String(s) => Ok(Lit::Str(s.clone())),
        Value::Bool(b) => Ok(Lit::Bool(*b)),
        Value::Number(n) => Ok(Lit::Num(n.as_f64().unwrap_or(0.0))),
        _ => Err(Error::parsing("a term value is a string, number or bool")),
    }
}

fn single_entry<'a>(v: &'a Value, clause: &str) -> Result<(&'a String, &'a Value)> {
    match v.as_object() {
        Some(m) if m.len() == 1 => Ok(m.iter().next().unwrap()),
        _ => Err(Error::parsing(format!(
            "[{clause}] expects exactly one field"
        ))),
    }
}

fn operator(v: &Value) -> Result<Option<Strategy>> {
    match v.get("operator").and_then(Value::as_str) {
        None => Ok(None),
        Some(o) if o.eq_ignore_ascii_case("and") => Ok(Some(Strategy::All)),
        Some(o) if o.eq_ignore_ascii_case("or") => Ok(Some(Strategy::Any)),
        Some(o) => Err(Error::parsing(format!("operator [{o}] is not and/or"))),
    }
}

fn fuzziness(v: &Value) -> bool {
    !matches!(v.get("fuzziness"), Some(Value::Number(n)) if n.as_u64() == Some(0))
        && !matches!(v.get("fuzziness"), Some(Value::String(s)) if s == "0")
}

/// Parses one OpenSearch query clause.
pub fn parse_query(v: &Value) -> Result<Query> {
    let Some(m) = v.as_object() else {
        return Err(Error::parsing("a query is an object"));
    };
    if m.len() != 1 {
        return Err(Error::parsing(
            "a query clause has exactly one key (e.g. match, bool)",
        ));
    }
    let (kind, body) = m.iter().next().unwrap();
    match kind.as_str() {
        "match_all" => Ok(Query::MatchAll),
        "match" | "match_phrase" | "match_phrase_prefix" => {
            let (field, spec) = single_entry(body, kind)?;
            let (text, op, fuzzy) = match spec {
                Value::String(s) => (s.clone(), None, true),
                Value::Object(o) => {
                    let text = o
                        .get("query")
                        .and_then(Value::as_str)
                        .ok_or_else(|| Error::parsing(format!("[{kind}] needs `query`")))?;
                    (text.to_string(), operator(spec)?, fuzziness(spec))
                }
                other => (leaf_string(other)?, None, true),
            };
            let mut t = TextQuery::new(&text);
            if field != "_all" && field != "*" {
                t.fields = vec![(field.clone(), 1.0)];
            }
            t.strategy = Some(op.unwrap_or(Strategy::Any));
            t.fuzzy = fuzzy && kind == "match";
            if kind != "match" {
                t.phrase = true;
                t.strategy = Some(Strategy::All);
            }
            t.prefix = match kind.as_str() {
                "match_phrase_prefix" => PrefixMode::On,
                "match" => PrefixMode::Off,
                _ => PrefixMode::Off,
            };
            Ok(Query::Text(t))
        }
        "multi_match" => {
            let Some(o) = body.as_object() else {
                return Err(Error::parsing("[multi_match] must be an object"));
            };
            for k in o.keys() {
                if !["query", "fields", "operator", "type", "fuzziness"].contains(&k.as_str()) {
                    return Err(Error::parsing(format!(
                        "unsupported [multi_match] key [{k}]"
                    )));
                }
            }
            let text = o
                .get("query")
                .and_then(Value::as_str)
                .ok_or_else(|| Error::parsing("[multi_match] needs `query`"))?;
            let mut t = TextQuery::new(text);
            if let Some(f) = o.get("fields") {
                t.fields = string_list(f, "fields")?
                    .iter()
                    .filter(|f| *f != "*")
                    .map(|f| parse_boost(f))
                    .collect();
            }
            t.strategy = Some(operator(body)?.unwrap_or(Strategy::Any));
            t.fuzzy = fuzziness(body);
            match o.get("type").and_then(Value::as_str) {
                None | Some("best_fields") | Some("most_fields") | Some("cross_fields") => {
                    t.prefix = PrefixMode::Off;
                }
                Some("phrase") => {
                    t.phrase = true;
                    t.strategy = Some(Strategy::All);
                    t.prefix = PrefixMode::Off;
                }
                Some("phrase_prefix") => {
                    t.phrase = true;
                    t.strategy = Some(Strategy::All);
                    t.prefix = PrefixMode::On;
                }
                Some("bool_prefix") => t.prefix = PrefixMode::On,
                Some(other) => {
                    return Err(Error::parsing(format!(
                        "[multi_match] type [{other}] is not supported"
                    )));
                }
            }
            Ok(Query::Text(t))
        }
        "prefix" => {
            let (field, spec) = single_entry(body, kind)?;
            let value = match spec {
                Value::String(s) => s.clone(),
                Value::Object(o) => o
                    .get("value")
                    .and_then(Value::as_str)
                    .ok_or_else(|| Error::parsing("[prefix] needs `value`"))?
                    .to_string(),
                _ => return Err(Error::parsing("[prefix] needs a string")),
            };
            let mut t = TextQuery::new(&value);
            t.fields = vec![(field.clone(), 1.0)];
            t.strategy = Some(Strategy::All);
            t.prefix = PrefixMode::On;
            t.fuzzy = false;
            Ok(Query::Text(t))
        }
        "term" => {
            let (field, spec) = single_entry(body, kind)?;
            let value = match spec {
                Value::Object(o) => o
                    .get("value")
                    .ok_or_else(|| Error::parsing("[term] needs `value`"))?,
                other => other,
            };
            Ok(Query::Term(field.clone(), lit_of(value)?))
        }
        "terms" => {
            let Some(o) = body.as_object() else {
                return Err(Error::parsing("[terms] must be an object"));
            };
            let (field, list) = o
                .iter()
                .find(|(k, _)| *k != "boost")
                .ok_or_else(|| Error::parsing("[terms] needs a field"))?;
            let Some(items) = list.as_array() else {
                return Err(Error::parsing("[terms] needs an array of values"));
            };
            let lits: Result<Vec<Lit>> = items.iter().map(lit_of).collect();
            Ok(Query::Terms(field.clone(), lits?))
        }
        "range" => {
            let (field, spec) = single_entry(body, kind)?;
            let Some(o) = spec.as_object() else {
                return Err(Error::parsing("[range] needs an object of bounds"));
            };
            let mut parts = Vec::new();
            for (k, v) in o {
                let n = v
                    .as_f64()
                    .ok_or_else(|| Error::parsing(format!("[range] bound [{k}] must be a number")))?;
                parts.push(match k.as_str() {
                    "gt" => Filter::Gt(field.clone(), n),
                    "gte" => Filter::Ge(field.clone(), n),
                    "lt" => Filter::Lt(field.clone(), n),
                    "lte" => Filter::Le(field.clone(), n),
                    "boost" => continue,
                    other => {
                        return Err(Error::parsing(format!("unsupported [range] key [{other}]")));
                    }
                });
            }
            Ok(Query::Filter(if parts.len() == 1 {
                parts.pop().unwrap()
            } else {
                Filter::And(parts)
            }))
        }
        "exists" => {
            let field = body
                .get("field")
                .and_then(Value::as_str)
                .ok_or_else(|| Error::parsing("[exists] needs `field`"))?;
            Ok(Query::Filter(Filter::Exists(field.to_string())))
        }
        "bool" => {
            let Some(o) = body.as_object() else {
                return Err(Error::parsing("[bool] must be an object"));
            };
            let mut b = BoolQuery::default();
            for (k, v) in o {
                let list = |v: &Value| -> Result<Vec<Query>> {
                    match v {
                        Value::Array(a) => a.iter().map(parse_query).collect(),
                        other => Ok(vec![parse_query(other)?]),
                    }
                };
                match k.as_str() {
                    "must" => b.must = list(v)?,
                    "should" => b.should = list(v)?,
                    "filter" => b.filter = list(v)?,
                    "must_not" => b.must_not = list(v)?,
                    "minimum_should_match" => {
                        b.minimum_should_match = Some(
                            v.as_u64()
                                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
                                .ok_or_else(|| {
                                    Error::parsing("minimum_should_match must be an integer")
                                })? as usize,
                        )
                    }
                    "boost" => {}
                    other => {
                        return Err(Error::parsing(format!("unsupported [bool] key [{other}]")));
                    }
                }
            }
            Ok(Query::Bool(Box::new(b)))
        }
        other => Err(Error::parsing(format!(
            "unknown or unsupported query [{other}]"
        ))),
    }
}

fn leaf_string(v: &Value) -> Result<String> {
    match v {
        Value::Number(n) => Ok(n.to_string()),
        Value::Bool(b) => Ok(b.to_string()),
        _ => Err(Error::parsing("a match value is a string")),
    }
}
