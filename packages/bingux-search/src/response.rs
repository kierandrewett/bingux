//! Search responses: typed, and as OpenSearch `_search` JSON.

use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::Arc;

use serde_json::{Map, Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Relation {
    Eq,
    Gte,
}

impl Relation {
    pub fn as_str(self) -> &'static str {
        match self {
            Relation::Eq => "eq",
            Relation::Gte => "gte",
        }
    }
}

/// A matched range of one string value of a field, in bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchRange {
    /// Which value of the field (0 for a scalar; the element of an array).
    pub value: usize,
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub index: Arc<str>,
    pub id: String,
    /// In `[0, 1]`; 1 for a match-all. Comparable between indexes only as a
    /// relevance fraction.
    pub score: f64,
    pub source: Value,
    /// Marked fragments by field (`<em>` tags), when highlighting was asked.
    pub highlight: BTreeMap<String, Vec<String>>,
    /// Matched byte ranges by field, when highlighting was asked.
    pub matches: BTreeMap<String, Vec<MatchRange>>,
    /// The sort values, as the cursor for `search_after`.
    pub sort: Option<Vec<Value>>,
}

impl Hit {
    /// A string field of the source.
    pub fn str_field(&self, name: &str) -> Option<&str> {
        self.source.get(name)?.as_str()
    }

    pub fn title(&self) -> Option<&str> {
        self.str_field("title")
    }

    pub fn uri(&self) -> Option<&str> {
        self.str_field("uri")
    }

    pub fn icon(&self) -> Option<&str> {
        self.str_field("icon")
    }

    pub fn category(&self) -> Option<&str> {
        self.str_field("category")
    }

    /// The `breadcrumb` array of the source.
    pub fn breadcrumb(&self) -> Vec<&str> {
        self.source
            .get("breadcrumb")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default()
    }

    /// The matched byte ranges of a scalar field (or the first value of an
    /// array), sorted and not overlapping.
    pub fn ranges(&self, field: &str) -> Vec<Range<usize>> {
        self.ranges_at(field, 0)
    }

    pub fn ranges_at(&self, field: &str, value: usize) -> Vec<Range<usize>> {
        self.matches
            .get(field)
            .map(|v| {
                v.iter()
                    .filter(|m| m.value == value)
                    .map(|m| m.start..m.end)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The values of a string or string-array field joined by `sep`, with the
    /// matched ranges moved into the joined text: `["Network", "Wi-Fi"]` as
    /// `Network › Wi-Fi` with the marks of both parts.
    pub fn joined(&self, field: &str, sep: &str) -> Option<(String, Vec<Range<usize>>)> {
        let values: Vec<&str> = match self.source.get(field)? {
            Value::String(s) => vec![s.as_str()],
            Value::Array(a) => a.iter().filter_map(Value::as_str).collect(),
            _ => return None,
        };
        let mut text = String::new();
        let mut ranges = Vec::new();
        for (i, v) in values.iter().enumerate() {
            if i > 0 {
                text.push_str(sep);
            }
            let base = text.len();
            ranges.extend(self.ranges_at(field, i).into_iter().map(|r| base + r.start..base + r.end));
            text.push_str(v);
        }
        Some((text, ranges))
    }

    fn to_json(&self) -> Value {
        let mut o = Map::new();
        o.insert("_index".into(), Value::String(self.index.to_string()));
        o.insert("_id".into(), Value::String(self.id.clone()));
        o.insert("_score".into(), json!(self.score));
        if !self.source.is_null() {
            o.insert("_source".into(), self.source.clone());
        }
        if !self.highlight.is_empty() {
            let h: Map<String, Value> = self
                .highlight
                .iter()
                .map(|(k, v)| (k.clone(), json!(v)))
                .collect();
            o.insert("highlight".into(), Value::Object(h));
        }
        if let Some(sort) = &self.sort {
            o.insert("sort".into(), Value::Array(sort.clone()));
        }
        Value::Object(o)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Bucket {
    pub key: Value,
    pub doc_count: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Aggregation {
    pub buckets: Vec<Bucket>,
    pub sum_other_doc_count: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SearchResponse {
    /// Wall time in microseconds; `took` in the JSON is whole milliseconds.
    pub took_us: u64,
    pub timed_out: bool,
    pub total: u64,
    pub relation: Relation,
    pub max_score: Option<f64>,
    pub hits: Vec<Hit>,
    pub aggregations: BTreeMap<String, Aggregation>,
}

impl SearchResponse {
    pub fn empty() -> Self {
        SearchResponse {
            took_us: 0,
            timed_out: false,
            total: 0,
            relation: Relation::Eq,
            max_score: None,
            hits: Vec::new(),
            aggregations: BTreeMap::new(),
        }
    }

    /// The OpenSearch `_search` response body.
    pub fn to_json(&self) -> Value {
        let mut root = Map::new();
        root.insert("took".into(), json!(self.took_us / 1000));
        root.insert("timed_out".into(), json!(self.timed_out));
        root.insert(
            "_shards".into(),
            json!({ "total": 1, "successful": 1, "skipped": 0, "failed": 0 }),
        );
        root.insert(
            "hits".into(),
            json!({
                "total": { "value": self.total, "relation": self.relation.as_str() },
                "max_score": self.max_score,
                "hits": self.hits.iter().map(Hit::to_json).collect::<Vec<_>>(),
            }),
        );
        if !self.aggregations.is_empty() {
            let mut aggs = Map::new();
            for (name, agg) in &self.aggregations {
                aggs.insert(
                    name.clone(),
                    json!({
                        "doc_count_error_upper_bound": 0,
                        "sum_other_doc_count": agg.sum_other_doc_count,
                        "buckets": agg.buckets.iter().map(|b| json!({
                            "key": b.key, "doc_count": b.doc_count,
                        })).collect::<Vec<_>>(),
                    }),
                );
            }
            root.insert("aggregations".into(), Value::Object(aggs));
        }
        Value::Object(root)
    }

    /// Parses an OpenSearch `_search` body back into a typed response (what a
    /// client of another process receives). `matches` are not carried by the
    /// JSON and come back empty.
    pub fn from_json(v: &Value) -> Option<SearchResponse> {
        let hits = v.get("hits")?;
        let total = hits.get("total")?;
        let mut out = SearchResponse::empty();
        out.took_us = v.get("took").and_then(Value::as_u64).unwrap_or(0) * 1000;
        out.timed_out = v.get("timed_out").and_then(Value::as_bool).unwrap_or(false);
        out.total = total.get("value").and_then(Value::as_u64)?;
        out.relation = if total.get("relation").and_then(Value::as_str) == Some("gte") {
            Relation::Gte
        } else {
            Relation::Eq
        };
        out.max_score = hits.get("max_score").and_then(Value::as_f64);
        for h in hits.get("hits")?.as_array()? {
            out.hits.push(Hit {
                index: Arc::from(h.get("_index").and_then(Value::as_str).unwrap_or("")),
                id: h.get("_id")?.as_str()?.to_string(),
                score: h.get("_score").and_then(Value::as_f64).unwrap_or(0.0),
                source: h.get("_source").cloned().unwrap_or(Value::Null),
                highlight: h
                    .get("highlight")
                    .and_then(Value::as_object)
                    .map(|m| {
                        m.iter()
                            .map(|(k, v)| {
                                (
                                    k.clone(),
                                    v.as_array()
                                        .map(|a| {
                                            a.iter()
                                                .filter_map(|s| s.as_str().map(str::to_string))
                                                .collect()
                                        })
                                        .unwrap_or_default(),
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                matches: BTreeMap::new(),
                sort: h.get("sort").and_then(Value::as_array).cloned(),
            });
        }
        if let Some(aggs) = v.get("aggregations").and_then(Value::as_object) {
            for (name, a) in aggs {
                let buckets = a
                    .get("buckets")
                    .and_then(Value::as_array)
                    .map(|b| {
                        b.iter()
                            .map(|b| Bucket {
                                key: b.get("key").cloned().unwrap_or(Value::Null),
                                doc_count: b.get("doc_count").and_then(Value::as_u64).unwrap_or(0)
                                    as usize,
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                out.aggregations.insert(
                    name.clone(),
                    Aggregation {
                        buckets,
                        sum_other_doc_count: a
                            .get("sum_other_doc_count")
                            .and_then(Value::as_u64)
                            .unwrap_or(0) as usize,
                    },
                );
            }
        }
        Some(out)
    }
}
