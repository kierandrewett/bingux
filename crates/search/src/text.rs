//! The text engine: compiles a text query against an index (expanding each
//! word into terms with prefixes and typos), finds candidate documents and
//! scores them into a `u64` key. See `design/search.md` 4.3 and 4.4.

use std::ops::Bound;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use crate::bitmap::Bitmap;
use crate::error::{Error, Result};
use crate::index::{Index, Inner, PostingList, unpack};
use crate::request::{PrefixMode, Strategy, TextQuery};
use crate::schema::RankingRule;
use crate::tokenizer;
use crate::typo;

/// Most words of a query that take part in ranking.
pub const MAX_WORDS: usize = 16;
/// Most terms one word expands to.
const MAX_EXP: usize = 256;
/// Most postings one word scans. A query that would scan more (a one-letter
/// prefix in a million documents) reports a lower bound for its total.
pub const MAX_SCAN: usize = 100_000;
/// Terms with more postings than this skip BM25's length normalisation.
const BM25_LIMIT: usize = 10_000;
const K1: f32 = 1.2;
const B: f32 = 0.75;

/// Cancels a running search from another thread.
#[derive(Clone, Default, Debug)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub fn new() -> Self {
        Cancel::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Checks cancellation and the deadline every so many calls.
pub(crate) struct Budget<'a> {
    cancel: &'a Cancel,
    deadline: Option<Instant>,
    tick: u32,
    pub timed_out: bool,
}

impl<'a> Budget<'a> {
    pub fn new(cancel: &'a Cancel, deadline: Option<Instant>) -> Self {
        Budget {
            cancel,
            deadline,
            tick: 0,
            timed_out: false,
        }
    }

    /// `Ok(true)` to go on, `Ok(false)` when the deadline passed (stop and
    /// return what there is), `Err` when cancelled.
    #[inline]
    pub fn check(&mut self) -> Result<bool> {
        self.tick = self.tick.wrapping_add(1);
        if self.tick & 1023 != 0 {
            return Ok(true);
        }
        self.check_now()
    }

    pub fn check_now(&mut self) -> Result<bool> {
        if self.cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        if let Some(d) = self.deadline
            && Instant::now() >= d
        {
            self.timed_out = true;
            return Ok(false);
        }
        Ok(true)
    }
}

// ---------------------------------------------------------------- key layout

const BITS_WORDS: u32 = 5;
const BITS_TYPO: u32 = 6;
const BITS_PROX: u32 = 8;
const BITS_ATTR: u32 = 13;
const BITS_EXACT: u32 = 5;
const BITS_BM25: u32 = 14;

/// The inputs of a key. `words` and `exact` are scaled to 0..=31 (31 is every
/// word), so a perfect match scores near 1.
pub(crate) struct Parts {
    pub words: u32,
    pub typo: u32,
    pub prox: u32,
    pub attr_rank: u32,
    pub attr_pos: u32,
    pub exact: u32,
    pub bm25: f32,
}

#[derive(Clone)]
pub(crate) struct Layout {
    rules: Vec<RankingRule>,
    pub total_bits: u32,
}

impl Layout {
    pub fn new(rules: &[RankingRule]) -> Self {
        let mut seen = Vec::new();
        for r in rules {
            if !seen.contains(r) {
                seen.push(*r);
            }
        }
        let mut total = BITS_BM25;
        for r in &seen {
            total += match r {
                RankingRule::Words => BITS_WORDS,
                RankingRule::Typo => BITS_TYPO,
                RankingRule::Proximity => BITS_PROX,
                RankingRule::Attribute => BITS_ATTR,
                RankingRule::Exactness => BITS_EXACT,
            };
        }
        Layout {
            rules: seen,
            total_bits: total,
        }
    }

    pub fn key(&self, p: &Parts) -> u64 {
        let mut key = 0u64;
        for r in &self.rules {
            let (bits, v) = match r {
                RankingRule::Words => (BITS_WORDS, p.words.min(31)),
                RankingRule::Typo => (BITS_TYPO, 63 - p.typo.min(63)),
                RankingRule::Proximity => (BITS_PROX, 255 - p.prox.min(255)),
                RankingRule::Attribute => {
                    let cost = p.attr_rank.min(31) * 256 + p.attr_pos.min(255);
                    (BITS_ATTR, 8191 - cost.min(8191))
                }
                RankingRule::Exactness => (BITS_EXACT, p.exact.min(31)),
            };
            key = (key << bits) | u64::from(v);
        }
        let q = (p.bm25 / 16.0 * 16383.0).clamp(0.0, 16383.0) as u64;
        (key << BITS_BM25) | q
    }

}

// ------------------------------------------------------------- compiled form

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Kind {
    Exact,
    Prefix,
    Typo,
    Synonym,
}

pub(crate) struct Exp {
    pub tid: u32,
    pub typo: u8,
    pub kind: Kind,
    /// The term text, kept only when highlights are wanted.
    pub term: Option<Box<str>>,
}

pub(crate) enum Source {
    Terms(Vec<Exp>),
    /// A multi-word synonym: the terms must be adjacent.
    Phrase { tids: Vec<u32>, terms: Vec<Box<str>> },
}

pub(crate) struct CWord {
    pub chars: usize,
    pub sources: Vec<Source>,
}

pub(crate) struct Compiled {
    pub words: Vec<CWord>,
    /// Ranges of words that must be adjacent.
    pub groups: Vec<(usize, usize)>,
    pub strategy: Strategy,
    /// Bit per searchable field that may match.
    pub mask: u64,
    pub all_fields: bool,
    pub weight: Vec<f32>,
}

fn df(list: &PostingList) -> usize {
    list.docs.len()
}

impl Index {
    /// Compiles `t`. `None` means the text has no words (it matches all).
    pub(crate) fn compile_text(
        &self,
        inner: &Inner,
        t: &TextQuery,
        default_strategy: Strategy,
        want_terms: bool,
        want: usize,
    ) -> Result<Option<Compiled>> {
        let nf = self.schema.searchable.len();
        let mut weight: Vec<f32> = self.schema.searchable.iter().map(|f| f.weight).collect();
        let (mask, all_fields) = if t.fields.is_empty() {
            (if nf == 64 { u64::MAX } else { (1u64 << nf) - 1 }, true)
        } else {
            let mut mask = 0u64;
            for (name, boost) in &t.fields {
                let Some(f) = self.schema.searchable.iter().position(|s| &s.name == name) else {
                    return Err(Error::request(format!(
                        "attribute `{name}` is not searchable; searchable attributes are [{}]",
                        self.schema
                            .searchable
                            .iter()
                            .map(|s| s.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )));
                };
                mask |= 1 << f;
                weight[f] = self.schema.searchable[f].weight * boost;
            }
            (mask, false)
        };

        // Words, with quotes marking phrases.
        struct W {
            text: String,
            quoted: bool,
        }
        let mut words: Vec<W> = Vec::new();
        let mut groups: Vec<(usize, usize)> = Vec::new();
        for (i, segment) in t.text.split('"').enumerate() {
            let quoted = t.phrase || i % 2 == 1;
            let start = words.len();
            for tok in tokenizer::tokenize_query(segment) {
                if self.stop.contains(&tok.text) {
                    continue;
                }
                if words.len() >= MAX_WORDS {
                    break;
                }
                words.push(W {
                    text: tok.text,
                    quoted,
                });
            }
            if quoted && words.len() - start >= 2 {
                groups.push((start, words.len()));
            }
        }
        if t.phrase && groups.len() > 1 {
            // One phrase over the whole text, whatever quotes it contained.
            groups = vec![(0, words.len())];
        }
        if words.is_empty() {
            return Ok(None);
        }
        let trailing_space = t.text.ends_with(char::is_whitespace);
        let last = words.len() - 1;
        let n_words = words.len();
        let strategy = t.strategy.unwrap_or(default_strategy);
        let mut out = Vec::with_capacity(n_words);
        for (i, w) in words.iter().enumerate() {
            let prefix = i == last
                && match t.prefix {
                    PrefixMode::Off => false,
                    PrefixMode::On => true,
                    PrefixMode::Auto => !trailing_space && !w.quoted,
                };
            let chars = w.text.chars().count();
            let typo_budget = if t.fuzzy
                && self.schema.typo.enabled
                && !w.quoted
                && !self.schema.typo.disabled_words.contains(&w.text)
            {
                typo::budget(
                    chars,
                    self.schema.typo.min_word_len_one_typo,
                    self.schema.typo.min_word_len_two_typos,
                )
            } else {
                0
            };
            // A lone word with plenty of exact or prefix matches needs no
            // typo variants: they would rank below all of them.
            let skip_typo_at = if n_words == 1 { Some(want.max(1)) } else { None };
            let exps = self.expand(inner, &w.text, prefix, typo_budget, want_terms, skip_typo_at);
            let mut sources = vec![Source::Terms(exps)];
            if let Some(alts) = self.schema.synonyms.get(&w.text) {
                for alt in alts {
                    let toks: Vec<String> = tokenizer::tokenize_query(alt)
                        .into_iter()
                        .map(|t| t.text)
                        .collect();
                    if toks.len() == 1 {
                        if let Some(&tid) = inner.terms.get(toks[0].as_str())
                            && let Source::Terms(v) = &mut sources[0]
                            && !v.iter().any(|e| e.tid == tid)
                        {
                            v.push(Exp {
                                tid,
                                typo: 0,
                                kind: Kind::Synonym,
                                term: want_terms.then(|| toks[0].clone().into_boxed_str()),
                            });
                        }
                    } else if toks.len() > 1 {
                        let tids: Option<Vec<u32>> = toks
                            .iter()
                            .map(|t| inner.terms.get(t.as_str()).copied())
                            .collect();
                        if let Some(tids) = tids {
                            sources.push(Source::Phrase {
                                tids,
                                terms: toks.into_iter().map(String::into_boxed_str).collect(),
                            });
                        }
                    }
                }
            }
            out.push(CWord {
                chars,
                sources,
            });
        }
        Ok(Some(Compiled {
            words: out,
            groups,
            strategy,
            mask,
            all_fields,
            weight,
        }))
    }

    /// The terms that start with `word` (not `word` itself), the commonest
    /// [`MAX_EXP`] of them, most frequent first. Short prefixes are the ones
    /// that match thousands of terms and are typed first, so their answers are
    /// kept until the index next changes.
    fn prefix_terms(&self, inner: &Inner, word: &str) -> Arc<Vec<(Box<str>, u32, u32)>> {
        let cacheable = word.chars().count() <= 3;
        if cacheable {
            let cache = self.exp_cache.lock().unwrap_or_else(|e| e.into_inner());
            if cache.version == inner.version
                && let Some(hit) = cache.map.get(word)
            {
                return hit.clone();
            }
        }
        let mut found: Vec<(&str, u32, usize)> = Vec::new();
        for (term, &tid) in inner
            .terms
            .range::<str, _>((Bound::Included(word), Bound::Unbounded))
        {
            if !term.starts_with(word) {
                break;
            }
            if term.len() == word.len() {
                continue;
            }
            found.push((term, tid, df(&inner.postings[tid as usize])));
        }
        if found.len() > MAX_EXP {
            found.select_nth_unstable_by(MAX_EXP - 1, |a, b| b.2.cmp(&a.2).then(a.0.cmp(b.0)));
            found.truncate(MAX_EXP);
        }
        found.sort_unstable_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(b.0)));
        let list: Arc<Vec<(Box<str>, u32, u32)>> = Arc::new(
            found
                .into_iter()
                .map(|(t, tid, d)| (t.into(), tid, d as u32))
                .collect(),
        );
        if cacheable {
            let mut cache = self.exp_cache.lock().unwrap_or_else(|e| e.into_inner());
            if cache.version != inner.version {
                cache.version = inner.version;
                cache.map.clear();
            }
            if cache.map.len() < 4096 {
                cache.map.insert(word.to_string(), list.clone());
            }
        }
        list
    }

    fn expand(
        &self,
        inner: &Inner,
        word: &str,
        prefix: bool,
        budget: u8,
        want_terms: bool,
        skip_typo_at: Option<usize>,
    ) -> Vec<Exp> {
        let mut out: Vec<Exp> = Vec::new();
        let name = |t: &str| want_terms.then(|| t.to_string().into_boxed_str());
        if let Some(&tid) = inner.terms.get(word) {
            out.push(Exp {
                tid,
                typo: 0,
                kind: Kind::Exact,
                term: name(word),
            });
        }
        let mut covered = out.iter().map(|e| df(&inner.postings[e.tid as usize])).sum::<usize>();
        if prefix {
            let found = self.prefix_terms(inner, word);
            for (term, tid, d) in found.iter() {
                covered += *d as usize;
                out.push(Exp {
                    tid: *tid,
                    typo: 0,
                    kind: Kind::Prefix,
                    term: name(term),
                });
            }
        }
        if budget > 0 && skip_typo_at.is_none_or(|want| covered < want) {
            let wchars: Vec<char> = word.chars().collect();
            let Some(first) = wchars.first().copied() else {
                return out;
            };
            let lo = first.to_string();
            let hi = char::from_u32(first as u32 + 1).map(|c| c.to_string());
            let range: (Bound<&str>, Bound<&str>) = (
                Bound::Included(lo.as_str()),
                hi.as_deref().map_or(Bound::Unbounded, Bound::Excluded),
            );
            let mut found: Vec<(u8, usize, &str, u32)> = Vec::new();
            let mut buf: Vec<char> = Vec::new();
            let mut scratch = typo::Scratch::default();
            for (term, &tid) in inner.terms.range::<str, _>(range) {
                let tchars = inner.term_chars[tid as usize] as usize;
                if prefix {
                    if tchars + (budget as usize) < wchars.len() {
                        continue;
                    }
                } else if tchars.abs_diff(wchars.len()) > budget as usize {
                    continue;
                }
                if &**term == word || (prefix && term.starts_with(word)) {
                    continue;
                }
                buf.clear();
                buf.extend(term.chars());
                // For a whole-word match, reject on the shared ends first.
                if !prefix {
                    let near = if budget == 1 {
                        typo::could_match(&wchars, &buf, budget)
                    } else {
                        typo::bag_ok(&wchars, &buf, budget)
                    };
                    if !near {
                        continue;
                    }
                }
                let d = if prefix {
                    typo::prefix_distance(&wchars, &buf, budget, &mut scratch)
                } else {
                    typo::distance(&wchars, &buf, budget, &mut scratch)
                };
                if let Some(d) = d
                    && d > 0
                {
                    found.push((d, df(&inner.postings[tid as usize]), term, tid));
                }
            }
            found.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)).then(a.2.cmp(b.2)));
            found.truncate(MAX_EXP);
            for (d, _, term, tid) in found {
                out.push(Exp {
                    tid,
                    typo: d,
                    kind: Kind::Typo,
                    term: name(term),
                });
            }
        }
        out
    }
}

// -------------------------------------------------------------------- runner

/// A document's matches of one word.
#[derive(Default)]
struct DocWord {
    matched: bool,
    typo: u8,
    exact: bool,
    df: usize,
    entries: Vec<(u8, u16)>,
}

struct ExpCur<'a> {
    list: &'a PostingList,
    pos: usize,
    typo: u8,
    exact: bool,
}

enum SrcCur<'a> {
    Terms(Vec<ExpCur<'a>>),
    Phrase {
        lists: Vec<&'a PostingList>,
        pos: Vec<usize>,
    },
}

pub(crate) struct Runner<'a> {
    pub idx: &'a Index,
    pub inner: &'a Inner,
    pub c: &'a Compiled,
    pub layout: &'a Layout,
    avg: Vec<f32>,
    n_live: f32,
}

fn idf(n: f32, df: usize) -> f32 {
    let df = (df as f32).min(n);
    (1.0 + (n - df + 0.5) / (df + 0.5)).ln()
}

impl<'a> Runner<'a> {
    pub fn new(idx: &'a Index, inner: &'a Inner, c: &'a Compiled, layout: &'a Layout) -> Self {
        let n = inner.live_count.max(1) as f32;
        let avg = inner
            .field_len_sum
            .iter()
            .map(|&s| (s as f32 / n).max(0.5))
            .collect();
        Runner {
            idx,
            inner,
            c,
            layout,
            avg,
            n_live: n,
        }
    }

    /// Streams every document of a one-word query with its key. Returns
    /// whether the scan was complete: it stops after [`MAX_SCAN`] postings.
    pub fn run_single(
        &self,
        filter: Option<&Bitmap>,
        budget: &mut Budget<'_>,
        emit: &mut impl FnMut(u32, u64),
    ) -> Result<bool> {
        let Source::Terms(exps) = &self.c.words[0].sources[0] else {
            return Ok(true);
        };
        let n = self.inner.n_docs();
        // A document that several terms of one word match is scored by the
        // first of them (exact, then prefix terms, then typos), once.
        let multi = exps.len() > 1;
        let mut seen = if multi { Bitmap::new(n) } else { Bitmap::new(0) };
        let mut single = [0u32; 1];
        let mut scanned = 0usize;
        let mut complete = true;
        'lists: for exp in exps {
            let list = &self.inner.postings[exp.tid as usize];
            let idf = idf(self.n_live, df(list));
            let exact = if exp.kind == Kind::Exact { 31 } else { 0 };
            // BM25 only breaks ties: skip its per-posting length lookups for a
            // term in more than 10 000 documents.
            let weigh = list.docs.len() <= BM25_LIMIT;
            for i in 0..list.docs.len() {
                if !budget.check()? {
                    complete = false;
                    break 'lists;
                }
                scanned += 1;
                if scanned > MAX_SCAN {
                    complete = false;
                    break 'lists;
                }
                let doc = list.docs[i];
                if !self.inner.live.get(doc) || filter.is_some_and(|f| !f.get(doc)) {
                    continue;
                }
                if multi && seen.get(doc) {
                    continue;
                }
                let entries = list.entries(i, &mut single);
                let mut best_rank = u32::MAX;
                let mut best_pos = 0u32;
                let mut tf = 0.0f32;
                let nf = self.idx.schema.searchable.len();
                for &e in entries {
                    let (f, p) = unpack(e);
                    if self.c.mask >> f & 1 == 0 {
                        continue;
                    }
                    let rank = u32::from(self.idx.rank[f]);
                    if best_rank == u32::MAX || (rank, p) < (best_rank, best_pos) {
                        best_rank = rank;
                        best_pos = p;
                    }
                    if weigh {
                        let len = f32::from(self.inner.field_len[doc as usize * nf + f]);
                        tf += self.c.weight[f] / (1.0 - B + B * len / self.avg[f]);
                    } else {
                        tf += self.c.weight[f];
                    }
                }
                if best_rank == u32::MAX {
                    continue;
                }
                let key = self.layout.key(&Parts {
                    words: 31,
                    typo: u32::from(exp.typo),
                    prox: 0,
                    attr_rank: best_rank,
                    attr_pos: best_pos,
                    exact,
                    bm25: idf * tf / (K1 + tf),
                });
                if multi {
                    seen.set(doc);
                }
                emit(doc, key);
            }
        }
        Ok(complete)
    }

    /// How many terms word `i` expands to.
    fn word_terms(&self, i: usize) -> usize {
        self.c.words[i]
            .sources
            .iter()
            .map(|s| match s {
                Source::Terms(e) => e.len(),
                Source::Phrase { .. } => 1,
            })
            .sum()
    }

    /// The documents word `i` may match, as a bit set (a superset where
    /// phrases are involved).
    fn word_bitmap(&self, i: usize, budget: &mut Budget<'_>) -> Result<Bitmap> {
        let n = self.inner.n_docs();
        let mut bm = Bitmap::new(n);
        let mut single = [0u32; 1];
        for src in &self.c.words[i].sources {
            match src {
                Source::Terms(exps) => {
                    for exp in exps {
                        let list = &self.inner.postings[exp.tid as usize];
                        for k in 0..list.docs.len() {
                            if !budget.check()? {
                                return Ok(bm);
                            }
                            if !self.c.all_fields {
                                let entries = list.entries(k, &mut single);
                                if !entries.iter().any(|&e| self.c.mask >> unpack(e).0 & 1 == 1) {
                                    continue;
                                }
                            }
                            bm.set(list.docs[k]);
                        }
                    }
                }
                Source::Phrase { tids, .. } => {
                    // Over-approximate: documents holding every token.
                    let mut acc: Option<Bitmap> = None;
                    for &tid in tids {
                        let mut b = Bitmap::new(n);
                        for &d in &self.inner.postings[tid as usize].docs {
                            b.set(d);
                        }
                        match &mut acc {
                            None => acc = Some(b),
                            Some(a) => a.and_with(&b),
                        }
                    }
                    if let Some(a) = acc {
                        bm.or_with(&a);
                    }
                }
            }
        }
        Ok(bm)
    }

    /// The postings a word would scan: its cost as a driver or a probe.
    fn word_cost(&self, i: usize) -> usize {
        let mut cost = 0usize;
        for src in &self.c.words[i].sources {
            match src {
                Source::Terms(exps) => {
                    for e in exps {
                        cost += df(&self.inner.postings[e.tid as usize]);
                    }
                }
                Source::Phrase { tids, .. } => {
                    cost += tids
                        .iter()
                        .map(|&t| df(&self.inner.postings[t as usize]))
                        .min()
                        .unwrap_or(0);
                }
            }
        }
        cost
    }

    /// The documents word `i` may match, sorted, within the scan budget. A
    /// superset where phrases are involved. `false` when the budget cut it.
    fn word_docs(
        &self,
        i: usize,
        live_filter: &Bitmap,
        scanned: &mut usize,
        budget: &mut Budget<'_>,
    ) -> Result<(Vec<u32>, bool)> {
        let mut docs: Vec<u32> = Vec::new();
        let mut single = [0u32; 1];
        let mut complete = true;
        for src in &self.c.words[i].sources {
            match src {
                Source::Terms(exps) => {
                    for exp in exps {
                        let list = &self.inner.postings[exp.tid as usize];
                        for k in 0..list.docs.len() {
                            if !budget.check()? {
                                return Ok((docs, false));
                            }
                            *scanned += 1;
                            if *scanned > MAX_SCAN {
                                complete = false;
                                break;
                            }
                            let d = list.docs[k];
                            if !live_filter.get(d) {
                                continue;
                            }
                            if !self.c.all_fields {
                                let entries = list.entries(k, &mut single);
                                if !entries
                                    .iter()
                                    .any(|&e| self.c.mask >> unpack(e).0 & 1 == 1)
                                {
                                    continue;
                                }
                            }
                            docs.push(d);
                        }
                    }
                }
                Source::Phrase { tids, .. } => {
                    // Intersect the token lists, smallest first.
                    let mut lists: Vec<&PostingList> = tids
                        .iter()
                        .map(|&t| &self.inner.postings[t as usize])
                        .collect();
                    lists.sort_by_key(|l| l.docs.len());
                    let mut cursors = vec![0usize; lists.len()];
                    for &d in &lists[0].docs {
                        if !budget.check()? {
                            return Ok((docs, false));
                        }
                        *scanned += 1;
                        if !live_filter.get(d) {
                            continue;
                        }
                        let mut all = true;
                        for (j, l) in lists.iter().enumerate().skip(1) {
                            cursors[j] = l.seek(cursors[j], d);
                            if cursors[j] >= l.docs.len() || l.docs[cursors[j]] != d {
                                all = false;
                                break;
                            }
                        }
                        if all {
                            docs.push(d);
                        }
                    }
                }
            }
        }
        // One term's list is already sorted and has each document once.
        let lists = self.c.words[i].sources.len()
            + match &self.c.words[i].sources[0] {
                Source::Terms(e) => e.len(),
                Source::Phrase { .. } => 1,
            };
        if lists > 2 {
            docs.sort_unstable();
            docs.dedup();
        }
        Ok((docs, complete))
    }

    /// Streams the documents of a multi-word query with their keys. Returns
    /// whether the scan was complete.
    pub fn run_multi(
        &self,
        filter: Option<&Bitmap>,
        want: usize,
        budget: &mut Budget<'_>,
        emit: &mut impl FnMut(u32, u64),
    ) -> Result<bool> {
        let n_words = self.c.words.len();
        let mut live_filter = self.inner.live.clone();
        if let Some(f) = filter {
            live_filter.and_with(f);
        }
        let costs: Vec<usize> = (0..n_words).map(|i| self.word_cost(i)).collect();
        let mut cursors = self.cursors();
        let mut probes: Vec<Option<Bitmap>> = (0..n_words).map(|_| None).collect();
        let mut buf: Vec<DocWord> = (0..n_words).map(|_| DocWord::default()).collect();
        let mut complete = true;
        let mut scanned = 0usize;

        match self.c.strategy {
            Strategy::Any => {
                // Union of every word's documents.
                let mut all: Vec<u32> = Vec::new();
                for i in 0..n_words {
                    let (docs, ok) = self.word_docs(i, &live_filter, &mut scanned, budget)?;
                    complete &= ok;
                    all.extend(docs);
                }
                all.sort_unstable();
                all.dedup();
                let order: Vec<usize> = (0..n_words).collect();
                let required = vec![false; n_words];
                for doc in all {
                    if !budget.check()? {
                        complete = false;
                        break;
                    }
                    if let Some(key) =
                        self.eval_doc(&mut cursors, doc, &mut buf, &order, &required, &probes)
                    {
                        emit(doc, key);
                    }
                }
            }
            Strategy::All | Strategy::Last => {
                let first_tier = n_words;
                let last_tier = if self.c.strategy == Strategy::All { n_words } else { 1 };
                for k in (last_tier..=first_tier).rev() {
                    // The first `k` words are required; probe the cheapest first.
                    cursors = self.cursors();
                    let mut order: Vec<usize> = (0..k).collect();
                    order.sort_by_key(|&i| costs[i]);
                    let mut required = vec![false; n_words];
                    for r in required.iter_mut().take(k) {
                        *r = true;
                    }
                    order.extend(k..n_words);
                    let driver = order[0];
                    let mut sc = 0usize;
                    let (docs, ok) = self.word_docs(driver, &live_filter, &mut sc, budget)?;
                    // A probed word with many terms is cheaper to test from a bit
                    // set than by seeking in each of its lists for every candidate.
                    for &i in order.iter().skip(1).take(k.saturating_sub(1)) {
                        let terms = self.word_terms(i);
                        if probes[i].is_none() && terms > 8 && costs[i] <= docs.len() * terms * 4 {
                            probes[i] = Some(self.word_bitmap(i, budget)?);
                        }
                    }
                    let mut found: Vec<(u32, u64)> = Vec::new();
                    for doc in docs {
                        if !budget.check()? {
                            complete = false;
                            break;
                        }
                        if let Some(key) =
                            self.eval_doc(&mut cursors, doc, &mut buf, &order, &required, &probes)
                        {
                            found.push((doc, key));
                        }
                    }
                    if found.len() >= want.max(1) || k == last_tier {
                        complete &= ok;
                        for (doc, key) in found {
                            emit(doc, key);
                        }
                        break;
                    }
                }
            }
        }
        Ok(complete)
    }

    fn cursors(&self) -> Vec<Vec<SrcCur<'a>>> {
        self.c
            .words
            .iter()
            .map(|w| {
                w.sources
                    .iter()
                    .map(|s| match s {
                        Source::Terms(exps) => SrcCur::Terms(
                            exps.iter()
                                .map(|e| ExpCur {
                                    list: &self.inner.postings[e.tid as usize],
                                    pos: 0,
                                    typo: e.typo,
                                    exact: e.kind == Kind::Exact,
                                })
                                .collect(),
                        ),
                        Source::Phrase { tids, .. } => SrcCur::Phrase {
                            lists: tids
                                .iter()
                                .map(|&t| &self.inner.postings[t as usize])
                                .collect(),
                            pos: vec![0; tids.len()],
                        },
                    })
                    .collect()
            })
            .collect()
    }

    /// Scores `doc`, or `None` when it does not match. Words are probed in
    /// `order`; a required word that misses ends it at once. Cursors only
    /// move forward, so documents must come in increasing order.
    fn eval_doc(
        &self,
        cursors: &mut [Vec<SrcCur<'_>>],
        doc: u32,
        buf: &mut [DocWord],
        order: &[usize],
        required: &[bool],
        probes: &[Option<Bitmap>],
    ) -> Option<u64> {
        let n_words = buf.len();
        let mut single = [0u32; 1];
        for &i in order {
            let srcs = &mut cursors[i];
            let w = &mut buf[i];
            w.matched = false;
            w.typo = u8::MAX;
            w.exact = false;
            w.df = usize::MAX;
            w.entries.clear();
            if let Some(bits) = &probes[i]
                && !bits.get(doc)
            {
                if required[i] {
                    return None;
                }
                continue;
            }
            for src in srcs.iter_mut() {
                match src {
                    SrcCur::Terms(curs) => {
                        for c in curs.iter_mut() {
                            c.pos = c.list.seek(c.pos, doc);
                            if c.pos >= c.list.docs.len() || c.list.docs[c.pos] != doc {
                                continue;
                            }
                            let before = w.entries.len();
                            for &e in c.list.entries(c.pos, &mut single) {
                                let (f, p) = unpack(e);
                                if self.c.mask >> f & 1 == 1 {
                                    w.entries.push((f as u8, p as u16));
                                }
                            }
                            if w.entries.len() > before {
                                w.matched = true;
                                w.typo = w.typo.min(c.typo);
                                w.exact |= c.exact;
                                w.df = w.df.min(c.list.docs.len());
                            }
                        }
                    }
                    SrcCur::Phrase { lists, pos } => {
                        let mut all = true;
                        for (j, l) in lists.iter().enumerate() {
                            pos[j] = l.seek(pos[j], doc);
                            if pos[j] >= l.docs.len() || l.docs[pos[j]] != doc {
                                all = false;
                            }
                        }
                        if !all {
                            continue;
                        }
                        let mut s = [0u32; 1];
                        let heads: Vec<Vec<u32>> = lists
                            .iter()
                            .enumerate()
                            .map(|(j, l)| l.entries(pos[j], &mut s).to_vec())
                            .collect();
                        let mut any = false;
                        for &e in &heads[0] {
                            let (f, p) = unpack(e);
                            if self.c.mask >> f & 1 == 0 {
                                continue;
                            }
                            let ok = (1..heads.len()).all(|j| {
                                heads[j].iter().any(|&x| {
                                    let (f2, p2) = unpack(x);
                                    f2 == f && p2 == p + j as u32
                                })
                            });
                            if ok {
                                w.entries.push((f as u8, p as u16));
                                any = true;
                            }
                        }
                        if any {
                            w.matched = true;
                            w.typo = w.typo.min(0);
                            w.df = w.df.min(lists[0].docs.len());
                        }
                    }
                }
            }
            if required[i] && !w.matched {
                return None;
            }
        }
        let matched = buf.iter().filter(|w| w.matched).count();
        if matched == 0 {
            return None;
        }
        // Quoted groups: adjacent, in order, in one field.
        for &(a, b) in &self.c.groups {
            if !chain(&buf[a..b]) {
                return None;
            }
        }
        let mut typo_sum = 0u32;
        let mut exact = 0u32;
        let mut rank_sum = 0u32;
        let mut pos_sum = 0u32;
        let mut bm25 = 0.0f32;
        let nf = self.idx.schema.searchable.len();
        for w in buf.iter_mut() {
            if !w.matched {
                continue;
            }
            w.entries.sort_unstable();
            w.entries.dedup();
            typo_sum += u32::from(w.typo);
            exact += u32::from(w.exact);
            let mut best = (u32::MAX, 0u32);
            let mut tf = 0.0f32;
            for &(f, p) in &w.entries {
                let rank = u32::from(self.idx.rank[f as usize]);
                if (rank, u32::from(p)) < best {
                    best = (rank, u32::from(p));
                }
                let len = f32::from(self.inner.field_len[doc as usize * nf + f as usize]);
                tf += self.c.weight[f as usize] / (1.0 - B + B * len / self.avg[f as usize]);
            }
            rank_sum += best.0;
            pos_sum += best.1.min(31);
            bm25 += idf(self.n_live, w.df) * tf / (K1 + tf);
        }
        let mut layers: [&[(u8, u16)]; MAX_WORDS] = [&[]; MAX_WORDS];
        let mut n_layers = 0;
        for w in buf.iter().filter(|w| w.matched) {
            layers[n_layers] = w.entries.as_slice();
            n_layers += 1;
        }
        let prox = proximity(&layers[..n_layers]);
        Some(self.layout.key(&Parts {
            words: (matched * 31 / n_words) as u32,
            typo: typo_sum,
            prox,
            attr_rank: rank_sum,
            attr_pos: pos_sum,
            exact: exact * 31 / n_words as u32,
            bm25: bm25 / n_words as f32,
        }))
    }
}

/// Whether the words of a quoted group sit next to each other, in order, in
/// one field.
fn chain(words: &[DocWord]) -> bool {
    if words.iter().any(|w| !w.matched) {
        return false;
    }
    words[0].entries.iter().any(|&(f, p)| {
        (1..words.len()).all(|j| {
            words[j]
                .entries
                .iter()
                .any(|&(f2, p2)| f2 == f && u32::from(p2) == u32::from(p) + j as u32)
        })
    })
}

/// The smallest summed gap between consecutive words, choosing one position
/// per word. Different fields cost the maximum; a word after its predecessor
/// costs `gap - 1` (adjacent is 0); out of order costs `gap + 1`.
pub(crate) fn proximity(layers: &[&[(u8, u16)]]) -> u32 {
    const MAX_GAP: u32 = 8;
    const CAP: usize = 24;
    if layers.len() < 2 {
        return 0;
    }
    let cost = |a: (u8, u16), b: (u8, u16)| -> u32 {
        if a.0 != b.0 {
            return MAX_GAP;
        }
        let d = i32::from(b.1) - i32::from(a.1);
        let c = match d {
            0 => 0,
            d if d > 0 => d - 1,
            d => -d + 1,
        };
        (c as u32).min(MAX_GAP)
    };
    let mut prev = [0u32; CAP];
    let mut next = [0u32; CAP];
    for w in 1..layers.len() {
        let before = &layers[w - 1][..layers[w - 1].len().min(CAP)];
        let here = &layers[w][..layers[w].len().min(CAP)];
        for (j, &b) in here.iter().enumerate() {
            let mut best = u32::MAX;
            for (k, &a) in before.iter().enumerate() {
                best = best.min(prev[k] + cost(a, b));
            }
            next[j] = best;
        }
        prev = next;
    }
    let last = layers[layers.len() - 1].len().min(CAP);
    prev[..last].iter().copied().min().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proximity_prefers_adjacent_in_order() {
        let a = [(0u8, 3u16)];
        let b = [(0u8, 4u16)];
        let far = [(0u8, 9u16)];
        let before = [(0u8, 1u16)];
        assert_eq!(proximity(&[&a, &b]), 0);
        assert_eq!(proximity(&[&a, &far]), 5);
        assert_eq!(proximity(&[&a, &before]), 3);
        assert_eq!(proximity(&[&a, &[(1u8, 4u16)]]), 8);
    }

    #[test]
    fn layout_orders_rules_as_configured() {
        let l = Layout::new(&[RankingRule::Words, RankingRule::Typo]);
        let a = l.key(&Parts {
            words: 2,
            typo: 5,
            prox: 0,
            attr_rank: 0,
            attr_pos: 0,
            exact: 0,
            bm25: 0.0,
        });
        let b = l.key(&Parts {
            words: 1,
            typo: 0,
            prox: 0,
            attr_rank: 0,
            attr_pos: 0,
            exact: 0,
            bm25: 0.0,
        });
        assert!(a > b);
        assert!(a < 1u64 << l.total_bits);
    }
}
