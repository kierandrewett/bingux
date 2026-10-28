//! Running a [`Search`] against an [`Index`]: planning, collecting, sorting,
//! paging, facets and highlights.

use std::cmp::{Ordering, Reverse};
use std::collections::{BTreeMap, BinaryHeap};
use std::sync::Arc;
use std::time::Instant;

use serde_json::{Map, Value};

use crate::bitmap::Bitmap;
use crate::document::{Map as DocMap, for_each_value};
use crate::error::{Error, Result};
use crate::filter::Filter;
use crate::highlight::{Matcher, crop, mark};
use crate::index::{FacetField, Index, Inner, SortVal, num_key};
use crate::request::{
    AggSpec, MAX_PAGE, MAX_WINDOW, Query, Search, SortKey, SortSpec, SourceSpec, Strategy,
    TextQuery,
};
use crate::response::{Aggregation, Bucket, Hit, MatchRange, Relation, SearchResponse};
use crate::text::{Budget, Cancel, Compiled, Layout, Runner};

/// A query reduced to at most one text clause plus filters.
#[derive(Default)]
struct Plan<'q> {
    text: Option<&'q TextQuery>,
    filters: Vec<Filter>,
    exclude: Vec<Filter>,
}

fn text_free(q: &Query) -> bool {
    match q {
        Query::Text(_) => false,
        Query::Bool(b) => [&b.must, &b.should, &b.filter, &b.must_not]
            .iter()
            .all(|l| l.iter().all(text_free)),
        _ => true,
    }
}

/// A text-free query as a filter.
fn to_filter(q: &Query) -> Filter {
    match q {
        Query::MatchAll => Filter::And(Vec::new()),
        Query::Filter(f) => f.clone(),
        Query::Bool(b) => {
            let mut parts: Vec<Filter> = b.must.iter().chain(&b.filter).map(to_filter).collect();
            for n in &b.must_not {
                parts.push(Filter::Not(Box::new(to_filter(n))));
            }
            if !b.should.is_empty() && (b.must.is_empty() && b.filter.is_empty()) {
                let msm = b.minimum_should_match.unwrap_or(1);
                if msm <= 1 {
                    parts.push(Filter::Or(b.should.iter().map(to_filter).collect()));
                }
            }
            Filter::And(parts)
        }
        // Resolved before planning; unreachable otherwise.
        Query::Text(_) | Query::Term(..) | Query::Terms(..) => Filter::And(Vec::new()),
    }
}

fn reduce<'q>(q: &'q Query, plan: &mut Plan<'q>) -> bool {
    match q {
        Query::MatchAll => true,
        Query::Text(t) => {
            if plan.text.is_some() {
                return false;
            }
            plan.text = Some(t);
            true
        }
        Query::Filter(f) => {
            plan.filters.push(f.clone());
            true
        }
        Query::Term(..) | Query::Terms(..) => false,
        Query::Bool(b) => {
            for c in b.must.iter().chain(&b.filter) {
                if text_free(c) {
                    plan.filters.push(to_filter(c));
                } else if !reduce(c, plan) {
                    return false;
                }
            }
            for c in &b.must_not {
                if !text_free(c) {
                    return false;
                }
                plan.exclude.push(to_filter(c));
            }
            if !b.should.is_empty() {
                if b.should.iter().all(text_free) {
                    if b.must.is_empty() && b.filter.is_empty() && b.minimum_should_match.unwrap_or(1) <= 1 {
                        plan.filters
                            .push(Filter::Or(b.should.iter().map(to_filter).collect()));
                    }
                } else if b.must.is_empty()
                    && b.filter.is_empty()
                    && b.should.len() == 1
                    && b.minimum_should_match.unwrap_or(1) <= 1
                {
                    return reduce(&b.should[0], plan);
                } else {
                    return false;
                }
            }
            true
        }
    }
}

/// Bounded best-k by score, then lowest document id.
struct Top {
    k: usize,
    heap: BinaryHeap<Reverse<(u64, Reverse<u32>)>>,
}

impl Top {
    fn push(&mut self, doc: u32, score: f64) {
        if self.k == 0 {
            return;
        }
        let item = Reverse((score.to_bits(), Reverse(doc)));
        if self.heap.len() < self.k {
            self.heap.push(item);
        } else if let Some(mut worst) = self.heap.peek_mut()
            && item < *worst
        {
            *worst = item;
        }
    }

    fn into_sorted(self) -> Vec<(u32, f64)> {
        let mut v: Vec<(u32, f64)> = self
            .heap
            .into_iter()
            .map(|Reverse((bits, Reverse(doc)))| (doc, f64::from_bits(bits)))
            .collect();
        v.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        v
    }
}

enum Sink {
    Top(Top),
    All(Vec<(u32, f64)>),
}

struct Collector {
    sink: Sink,
    count: u64,
    matched: Option<Bitmap>,
}

impl Collector {
    #[inline]
    fn push(&mut self, doc: u32, score: f64) {
        self.count += 1;
        if let Some(m) = &mut self.matched {
            m.set(doc);
        }
        match &mut self.sink {
            Sink::Top(t) => t.push(doc, score),
            Sink::All(v) => v.push((doc, score)),
        }
    }
}

/// Dense scores and the documents of a clause, for the general `bool` path.
struct Eval {
    docs: Bitmap,
    scores: Option<Vec<f64>>,
}

impl Index {
    fn resolve(&self, q: &Query) -> Result<Query> {
        Ok(match q {
            Query::Term(field, lit) => self.resolve_term(field, std::slice::from_ref(lit))?,
            Query::Terms(field, lits) => self.resolve_term(field, lits)?,
            Query::Bool(b) => {
                let map = |l: &Vec<Query>| -> Result<Vec<Query>> {
                    l.iter().map(|q| self.resolve(q)).collect()
                };
                Query::Bool(Box::new(crate::request::BoolQuery {
                    must: map(&b.must)?,
                    should: map(&b.should)?,
                    filter: map(&b.filter)?,
                    must_not: map(&b.must_not)?,
                    minimum_should_match: b.minimum_should_match,
                }))
            }
            other => other.clone(),
        })
    }

    fn resolve_term(&self, field: &str, lits: &[crate::filter::Lit]) -> Result<Query> {
        if self.schema.filterable.iter().any(|f| f == field) {
            return Ok(Query::Filter(Filter::In(field.to_string(), lits.to_vec())));
        }
        if self.schema.searchable.iter().any(|f| f.name == field) {
            // An exact word of a text field.
            let mut parts = Vec::new();
            for lit in lits {
                let text = match lit {
                    crate::filter::Lit::Str(s) => s.clone(),
                    crate::filter::Lit::Num(n) => n.to_string(),
                    crate::filter::Lit::Bool(b) => b.to_string(),
                };
                let mut t = TextQuery::new(&text);
                t.fields = vec![(field.to_string(), 1.0)];
                t.strategy = Some(Strategy::All);
                t.prefix = crate::request::PrefixMode::Off;
                t.fuzzy = false;
                parts.push(Query::Text(t));
            }
            return Ok(if parts.len() == 1 {
                parts.pop().unwrap()
            } else {
                Query::Bool(Box::new(crate::request::BoolQuery {
                    should: parts,
                    ..Default::default()
                }))
            });
        }
        Err(Error::request(format!(
            "attribute `{field}` is neither filterable nor searchable"
        )))
    }

    fn run_text(
        &self,
        inner: &Inner,
        c: &Compiled,
        layout: &Layout,
        filter: Option<&Bitmap>,
        want: usize,
        budget: &mut Budget<'_>,
        emit: &mut impl FnMut(u32, u64),
    ) -> Result<bool> {
        let r = Runner::new(self, inner, c, layout);
        if c.words.len() == 1 && c.words[0].sources.len() == 1 {
            r.run_single(filter, budget, emit)
        } else {
            r.run_multi(filter, want, budget, emit)
        }
    }

    /// Runs the search. Cancelling `cancel` makes it return
    /// [`Error::Cancelled`] within about a thousand candidates.
    pub fn search(&self, req: &Search, cancel: &Cancel) -> Result<SearchResponse> {
        let started = Instant::now();
        if req.size > MAX_PAGE {
            return Err(Error::request(format!(
                "`size` is {}; the largest page is {MAX_PAGE}",
                req.size
            )));
        }
        if req.from + req.size > MAX_WINDOW {
            return Err(Error::request(format!(
                "result window is too large: from + size must be at most {MAX_WINDOW}"
            )));
        }
        let deadline = req.timeout.map(|t| started + t);
        let mut budget = Budget::new(cancel, deadline);
        budget.check_now()?;
        let want = req.from + req.size;
        let layout = Layout::new(&self.schema.ranking_rules);
        let want_terms = req.highlight.is_some();

        let inner = self.read();
        let query = self.resolve(&req.query)?;

        // Filters: the request's and the ones lifted out of the query.
        let mut plan = Plan::default();
        let reducible = reduce(&query, &mut plan);
        let mut filters: Vec<Filter> = Vec::new();
        if let Some(f) = &req.filter {
            filters.push(f.clone());
        }
        for expr in &req.filter_exprs {
            filters.push(Filter::parse(expr)?);
        }
        if reducible {
            filters.append(&mut plan.filters);
            for e in std::mem::take(&mut plan.exclude) {
                filters.push(Filter::Not(Box::new(e)));
            }
        }
        let mut filter_bm: Option<Bitmap> = if filters.is_empty() {
            None
        } else {
            Some(self.eval_filter(&inner, &Filter::And(filters))?)
        };
        if req.external_only {
            if !self.schema.external {
                return Err(Error::UnknownIndex(self.name.clone()));
            }
            match &mut filter_bm {
                Some(b) => b.and_with(&inner.ext),
                None => filter_bm = Some(inner.ext.clone()),
            }
        }

        // Sorting.
        let explicit_sort = !req.sort.is_empty();
        let sort_specs: Vec<SortSpec> = if explicit_sort {
            req.sort.clone()
        } else if req.search_after.is_some() {
            vec![
                SortSpec {
                    key: SortKey::Score,
                    desc: true,
                },
                SortSpec {
                    key: SortKey::Id,
                    desc: false,
                },
            ]
        } else {
            Vec::new()
        };
        let mut columns: Vec<Option<usize>> = Vec::new();
        for s in &sort_specs {
            columns.push(match &s.key {
                SortKey::Field(f) => Some(
                    self.schema
                        .sortable
                        .iter()
                        .position(|x| x == f)
                        .ok_or_else(|| {
                            Error::request(format!(
                                "attribute `{f}` is not sortable; sortable attributes are [{}]",
                                self.schema.sortable.join(", ")
                            ))
                        })?,
                ),
                _ => None,
            });
        }
        if let Some(cursor) = &req.search_after
            && cursor.len() != sort_specs.len()
        {
            return Err(Error::request(format!(
                "search_after has {} values but the sort has {}",
                cursor.len(),
                sort_specs.len()
            )));
        }
        let sorted = !sort_specs.is_empty();

        let mut collector = Collector {
            sink: if sorted {
                Sink::All(Vec::new())
            } else {
                Sink::Top(Top {
                    k: want,
                    heap: BinaryHeap::new(),
                })
            },
            count: 0,
            matched: (!req.aggs.is_empty()).then(|| Bitmap::new(inner.n_docs())),
        };

        let mut matcher = Matcher::default();
        let mut match_all = false;
        let mut truncated = false;
        let inv = 1.0 / (1u64 << layout.total_bits) as f64;

        if reducible {
            let compiled = match plan.text {
                Some(t) => self.compile_text(&inner, t, req.strategy, want_terms, want)?,
                None => None,
            };
            match compiled {
                None => {
                    match_all = true;
                    let mut docs = inner.live.clone();
                    if let Some(f) = &filter_bm {
                        docs.and_with(f);
                    }
                    if !sorted && collector.matched.is_none() {
                        // Insertion order, first `want` documents.
                        collector.count = docs.count() as u64;
                        if let Sink::Top(t) = &mut collector.sink {
                            let mut v: Vec<(u32, f64)> = Vec::new();
                            for d in docs.ones().take(want) {
                                v.push((d, 1.0));
                            }
                            for (d, s) in v {
                                t.push(d, s);
                            }
                        }
                    } else {
                        for d in docs.ones() {
                            if !budget.check()? {
                                break;
                            }
                            collector.push(d, 1.0);
                        }
                    }
                }
                Some(c) => {
                    if want_terms {
                        matcher.add(&c);
                    }
                    let complete = self.run_text(
                        &inner,
                        &c,
                        &layout,
                        filter_bm.as_ref(),
                        want,
                        &mut budget,
                        &mut |doc, key| collector.push(doc, key as f64 * inv),
                    )?;
                    truncated |= !complete && !budget.timed_out;
                }
            }
        } else {
            let ev = self.eval_general(&inner, &query, &layout, want, want_terms, &mut matcher, &mut budget)?;
            let mut docs = ev.docs;
            docs.and_with(&inner.live);
            if let Some(f) = &filter_bm {
                docs.and_with(f);
            }
            match_all = ev.scores.is_none();
            for d in docs.ones() {
                if !budget.check()? {
                    break;
                }
                let s = ev.scores.as_ref().map_or(1.0, |v| v[d as usize]);
                collector.push(d, s);
            }
        }
        let timed_out = budget.timed_out;

        // Order and page.
        let total_count = collector.count;
        let mut ordered: Vec<(u32, f64)> = match collector.sink {
            Sink::Top(t) => t.into_sorted(),
            Sink::All(mut all) => {
                let cmp = |a: &(u32, f64), b: &(u32, f64)| -> Ordering {
                    self.compare(&inner, &sort_specs, &columns, a, b)
                };
                if let Some(cursor) = &req.search_after {
                    all.retain(|c| {
                        self.compare_cursor(&inner, &sort_specs, &columns, c, cursor) == Ordering::Greater
                    });
                }
                let k = want.min(all.len());
                if k > 0 && k < all.len() {
                    all.select_nth_unstable_by(k - 1, cmp);
                    all.truncate(k);
                }
                all.sort_by(cmp);
                all
            }
        };
        let page: Vec<(u32, f64)> = ordered.drain(..).skip(req.from).take(req.size).collect();

        let (total, relation) = match req.track_total_hits {
            Some(cap) if total_count > cap as u64 => (cap as u64, Relation::Gte),
            _ if truncated => (total_count, Relation::Gte),
            _ => (total_count, Relation::Eq),
        };
        let max_score = page.first().map(|p| p.1);

        // Aggregations.
        let mut aggregations = BTreeMap::new();
        if let Some(matched) = &collector.matched {
            for spec in &req.aggs {
                aggregations.insert(spec.name.clone(), self.aggregate(&inner, matched, spec)?);
            }
        }

        // Hits.
        let name: Arc<str> = Arc::from(self.name.as_str());
        let mut hits = Vec::with_capacity(page.len());
        for (doc, score) in page {
            hits.push(self.build_hit(
                &inner,
                &name,
                doc,
                if match_all { 1.0 } else { score },
                req,
                &matcher,
                &sort_specs,
                &columns,
            ));
        }

        Ok(SearchResponse {
            took_us: started.elapsed().as_micros() as u64,
            timed_out,
            total,
            relation,
            max_score,
            hits,
            aggregations,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn eval_general(
        &self,
        inner: &Inner,
        q: &Query,
        layout: &Layout,
        want: usize,
        want_terms: bool,
        matcher: &mut Matcher,
        budget: &mut Budget<'_>,
    ) -> Result<Eval> {
        let n = inner.n_docs();
        match q {
            Query::MatchAll => Ok(Eval {
                docs: inner.live.clone(),
                scores: None,
            }),
            Query::Filter(f) => Ok(Eval {
                docs: self.eval_filter(inner, f)?,
                scores: None,
            }),
            Query::Term(..) | Query::Terms(..) => Err(Error::request("unresolved term clause")),
            Query::Text(t) => {
                let Some(c) = self.compile_text(inner, t, Strategy::Any, want_terms, want)? else {
                    return Ok(Eval {
                        docs: inner.live.clone(),
                        scores: None,
                    });
                };
                if want_terms {
                    matcher.add(&c);
                }
                let mut docs = Bitmap::new(n);
                let mut scores = vec![0.0f64; n];
                let inv = 1.0 / (1u64 << layout.total_bits) as f64;
                let _ = self.run_text(inner, &c, layout, None, want, budget, &mut |d, k| {
                    docs.set(d);
                    scores[d as usize] = k as f64 * inv;
                })?;
                Ok(Eval {
                    docs,
                    scores: Some(scores),
                })
            }
            Query::Bool(b) => {
                let mut docs = inner.live.clone();
                let mut scores: Option<Vec<f64>> = None;
                let add = |scores: &mut Option<Vec<f64>>, ev: &Eval, docs: &Bitmap| {
                    if let Some(s) = &ev.scores {
                        let total = scores.get_or_insert_with(|| vec![0.0; n]);
                        for d in docs.ones() {
                            total[d as usize] += s[d as usize];
                        }
                    }
                };
                for c in b.must.iter().chain(&b.filter) {
                    let ev = self.eval_general(inner, c, layout, want, want_terms, matcher, budget)?;
                    docs.and_with(&ev.docs);
                    // Only `must` scores; `filter` clauses do not.
                    if b.must.iter().any(|m| std::ptr::eq(m, c)) {
                        add(&mut scores, &ev, &ev.docs);
                    }
                }
                if !b.should.is_empty() {
                    let msm = b.minimum_should_match.unwrap_or(if b.must.is_empty() && b.filter.is_empty() {
                        1
                    } else {
                        0
                    });
                    let mut counts = vec![0u8; n];
                    let mut any = Bitmap::new(n);
                    for c in &b.should {
                        let ev = self.eval_general(inner, c, layout, want, want_terms, matcher, budget)?;
                        for d in ev.docs.ones() {
                            counts[d as usize] = counts[d as usize].saturating_add(1);
                            any.set(d);
                        }
                        add(&mut scores, &ev, &ev.docs);
                    }
                    if msm > 0 {
                        let mut ok = Bitmap::new(n);
                        for d in any.ones() {
                            if counts[d as usize] as usize >= msm {
                                ok.set(d);
                            }
                        }
                        docs.and_with(&ok);
                    }
                }
                for c in &b.must_not {
                    let ev = self.eval_general(inner, c, layout, want, false, &mut Matcher::default(), budget)?;
                    docs.andnot_with(&ev.docs);
                }
                Ok(Eval { docs, scores })
            }
        }
    }

    fn sort_val<'a>(&self, inner: &'a Inner, col: usize, doc: u32) -> &'a SortVal {
        &inner.columns[col][doc as usize]
    }

    fn compare(
        &self,
        inner: &Inner,
        specs: &[SortSpec],
        columns: &[Option<usize>],
        a: &(u32, f64),
        b: &(u32, f64),
    ) -> Ordering {
        for (s, col) in specs.iter().zip(columns) {
            let ord = match (&s.key, col) {
                (SortKey::Score, _) => a.1.total_cmp(&b.1),
                (SortKey::Id, _) => inner.docs[a.0 as usize].id.cmp(&inner.docs[b.0 as usize].id),
                (SortKey::Field(_), Some(c)) => {
                    let (x, y) = (self.sort_val(inner, *c, a.0), self.sort_val(inner, *c, b.0));
                    match cmp_vals(x, y) {
                        // Missing values sort last in either direction.
                        Some(o) => {
                            if s.desc { o.reverse() } else { o }
                        }
                        None => cmp_missing(x, y),
                    }
                }
                _ => Ordering::Equal,
            };
            let ord = if matches!(s.key, SortKey::Field(_)) {
                ord
            } else if s.desc {
                ord.reverse()
            } else {
                ord
            };
            if ord != Ordering::Equal {
                return ord;
            }
        }
        b.1.total_cmp(&a.1).then(a.0.cmp(&b.0))
    }

    /// How `c` orders against the cursor of a previous page's last hit.
    fn compare_cursor(
        &self,
        inner: &Inner,
        specs: &[SortSpec],
        columns: &[Option<usize>],
        c: &(u32, f64),
        cursor: &[Value],
    ) -> Ordering {
        for ((s, col), cv) in specs.iter().zip(columns).zip(cursor) {
            let ord = match (&s.key, col) {
                (SortKey::Score, _) => c.1.total_cmp(&cv.as_f64().unwrap_or(0.0)),
                (SortKey::Id, _) => (*inner.docs[c.0 as usize].id).cmp(cv.as_str().unwrap_or("")),
                (SortKey::Field(_), Some(col)) => {
                    let x = self.sort_val(inner, *col, c.0);
                    let y = match cv {
                        Value::Number(n) => SortVal::Num(n.as_f64().unwrap_or(0.0)),
                        Value::String(s) => SortVal::Str(s.as_str().into()),
                        _ => SortVal::Missing,
                    };
                    match cmp_vals(x, &y) {
                        Some(o) => {
                            if s.desc { o.reverse() } else { o }
                        }
                        None => cmp_missing(x, &y),
                    }
                }
                _ => Ordering::Equal,
            };
            let ord = if matches!(s.key, SortKey::Field(_)) {
                ord
            } else if s.desc {
                ord.reverse()
            } else {
                ord
            };
            if ord != Ordering::Equal {
                return ord;
            }
        }
        Ordering::Equal
    }

    fn aggregate(&self, inner: &Inner, matched: &Bitmap, spec: &AggSpec) -> Result<Aggregation> {
        let fi = self
            .schema
            .filterable
            .iter()
            .position(|f| *f == spec.field)
            .ok_or_else(|| {
                Error::request(format!(
                    "attribute `{}` is not filterable; it cannot be aggregated",
                    spec.field
                ))
            })?;
        let ff: &FacetField = &inner.facets[fi];
        let count = |list: &[u32]| list.iter().filter(|&&d| matched.get(d) && inner.live.get(d)).count();
        let mut buckets: Vec<Bucket> = Vec::new();
        for (value, list) in &ff.strings {
            let c = count(list);
            if c > 0 {
                let key = ff.labels.get(value).unwrap_or(value);
                let key = match &**key {
                    "true" if !ff.labels.contains_key("true") => Value::Bool(true),
                    k => Value::String(k.to_string()),
                };
                buckets.push(Bucket {
                    key,
                    doc_count: c,
                });
            }
        }
        for (k, list) in &ff.numbers {
            let c = count(list);
            if c > 0 {
                let bits = if k >> 63 == 1 { k & !(1 << 63) } else { !k };
                let x = f64::from_bits(bits);
                let key = if x.fract() == 0.0 && x.abs() < 1e15 {
                    Value::from(x as i64)
                } else {
                    Value::from(x)
                };
                buckets.push(Bucket {
                    key,
                    doc_count: c,
                });
            }
        }
        let _ = num_key;
        buckets.sort_by(|a, b| {
            b.doc_count
                .cmp(&a.doc_count)
                .then_with(|| a.key.to_string().cmp(&b.key.to_string()))
        });
        let total: usize = buckets.iter().map(|b| b.doc_count).sum();
        buckets.truncate(spec.size);
        let shown: usize = buckets.iter().map(|b| b.doc_count).sum();
        Ok(Aggregation {
            buckets,
            sum_other_doc_count: total - shown,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn build_hit(
        &self,
        inner: &Inner,
        name: &Arc<str>,
        doc: u32,
        score: f64,
        req: &Search,
        matcher: &Matcher,
        sort_specs: &[SortSpec],
        columns: &[Option<usize>],
    ) -> Hit {
        let slot = &inner.docs[doc as usize];
        let full: DocMap = serde_json::from_slice(&slot.source).unwrap_or_default();
        let mut highlight = BTreeMap::new();
        let mut matches = BTreeMap::new();
        if let Some(h) = &req.highlight
            && !matcher.is_empty()
        {
            let fields: Vec<&str> = if h.fields.is_empty() {
                self.schema.searchable.iter().map(|f| f.name.as_str()).collect()
            } else {
                h.fields.iter().map(String::as_str).collect()
            };
            for field in fields {
                let mut values: Vec<&str> = Vec::new();
                for_each_value(&full, field, &mut |v| {
                    if let Value::String(s) = v {
                        values.push(s);
                    }
                });
                let mut frags = Vec::new();
                let mut ranges_out = Vec::new();
                for (i, text) in values.iter().enumerate() {
                    let ranges = matcher.ranges(text);
                    if ranges.is_empty() {
                        continue;
                    }
                    for r in &ranges {
                        ranges_out.push(MatchRange {
                            value: i,
                            start: r.start,
                            end: r.end,
                        });
                    }
                    let frag = if h.fragment_size > 0 {
                        let w = crop(text, &ranges, h.fragment_size);
                        let inside: Vec<_> = ranges
                            .iter()
                            .filter(|r| r.start >= w.start && r.end <= w.end)
                            .map(|r| r.start - w.start..r.end - w.start)
                            .collect();
                        let mut s = mark(&text[w.clone()], &inside, &h.pre_tag, &h.post_tag);
                        if w.start > 0 {
                            s.insert(0, '…');
                        }
                        if w.end < text.len() {
                            s.push('…');
                        }
                        s
                    } else {
                        mark(text, &ranges, &h.pre_tag, &h.post_tag)
                    };
                    frags.push(frag);
                }
                if !frags.is_empty() {
                    highlight.insert(field.to_string(), frags);
                    matches.insert(field.to_string(), ranges_out);
                }
            }
        }
        let source = self.shape_source(full, &req.source);
        let sort = (!sort_specs.is_empty()).then(|| {
            sort_specs
                .iter()
                .zip(columns)
                .map(|(s, col)| match (&s.key, col) {
                    (SortKey::Score, _) => Value::from(score),
                    (SortKey::Id, _) => Value::String(slot.id.to_string()),
                    (SortKey::Field(_), Some(c)) => match self.sort_val(inner, *c, doc) {
                        SortVal::Num(n) => Value::from(*n),
                        SortVal::Str(s) => Value::String(s.to_string()),
                        SortVal::Missing => Value::Null,
                    },
                    _ => Value::Null,
                })
                .collect()
        });
        Hit {
            index: name.clone(),
            id: slot.id.to_string(),
            score,
            source,
            highlight,
            matches,
            sort,
        }
    }

    /// The `_source` of a hit: the displayed fields, then the request's
    /// includes and excludes.
    fn shape_source(&self, full: DocMap, spec: &SourceSpec) -> Value {
        if matches!(spec, SourceSpec::Nothing) {
            return Value::Null;
        }
        let mut map = full;
        if let Some(displayed) = &self.schema.displayed {
            map = pick(&map, displayed);
        }
        if let SourceSpec::Only { includes, excludes } = spec {
            if !includes.is_empty() {
                map = pick(&map, includes);
            }
            for path in excludes {
                remove_path(&mut map, &path.split('.').collect::<Vec<_>>());
            }
        }
        Value::Object(map)
    }
}

/// Orders two sort values of the same kind; `None` when either is missing.
fn cmp_vals(a: &SortVal, b: &SortVal) -> Option<Ordering> {
    match (a, b) {
        (SortVal::Missing, _) | (_, SortVal::Missing) => None,
        (SortVal::Num(x), SortVal::Num(y)) => Some(x.total_cmp(y)),
        (SortVal::Str(x), SortVal::Str(y)) => Some(x.cmp(y)),
        (SortVal::Num(_), SortVal::Str(_)) => Some(Ordering::Less),
        (SortVal::Str(_), SortVal::Num(_)) => Some(Ordering::Greater),
    }
}

/// Missing sorts after everything.
fn cmp_missing(a: &SortVal, b: &SortVal) -> Ordering {
    match (matches!(a, SortVal::Missing), matches!(b, SortVal::Missing)) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        _ => Ordering::Less,
    }
}

/// Keeps only the dotted `paths` of `src`.
fn pick(src: &DocMap, paths: &[String]) -> DocMap {
    let mut out = Map::new();
    for path in paths {
        let segs: Vec<&str> = path.split('.').collect();
        copy_path(src, &mut out, &segs);
    }
    out
}

fn copy_path(src: &Map<String, Value>, out: &mut Map<String, Value>, segs: &[&str]) {
    let Some((head, tail)) = segs.split_first() else { return };
    let Some(v) = src.get(*head) else { return };
    if tail.is_empty() {
        out.insert((*head).to_string(), v.clone());
        return;
    }
    if let Value::Object(inner) = v {
        let entry = out
            .entry((*head).to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if let Value::Object(target) = entry {
            copy_path(inner, target, tail);
        }
    }
}

fn remove_path(map: &mut Map<String, Value>, segs: &[&str]) {
    let Some((head, tail)) = segs.split_first() else { return };
    if tail.is_empty() {
        map.remove(*head);
    } else if let Some(Value::Object(inner)) = map.get_mut(*head) {
        remove_path(inner, tail);
    }
}
