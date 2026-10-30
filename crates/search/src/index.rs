//! The index: inverted index, document store, facets and columns, with
//! incremental add, update and delete. See `design/search.md` section 4.2.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::sync::{Mutex, RwLock};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::bitmap::Bitmap;
use crate::document::{Document, Map, Value, for_each_value, leaf_text};
use crate::error::{Error, Result};
use crate::filter::{Filter, Lit};
use crate::schema::Schema;
use crate::tokenizer::{self, HARD_GAP, MAX_POSITION};

/// Upper bounds on what one index holds. An upsert that would pass one fails
/// with [`Error::LimitExceeded`]; the documents before the offending one stay.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_documents: usize,
    pub max_document_bytes: usize,
    pub max_index_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_documents: 2_000_000,
            max_document_bytes: 64 * 1024,
            max_index_bytes: 1 << 30,
        }
    }
}

pub(crate) const OVERFLOW: u32 = 0x8000_0000;

#[inline]
pub(crate) fn pack(field: usize, position: u32) -> u32 {
    ((field as u32) << 16) | position
}

#[inline]
pub(crate) fn unpack(entry: u32) -> (usize, u32) {
    (((entry >> 16) & 0x7f) as usize, entry & 0xffff)
}

#[derive(Default)]
pub(crate) struct PostingList {
    pub docs: Vec<u32>,
    pub occ: Vec<u32>,
    pub extra: Vec<u32>,
}

impl PostingList {
    /// The packed entries of the `i`th posting.
    #[inline]
    pub fn entries<'a>(&'a self, i: usize, single: &'a mut [u32; 1]) -> &'a [u32] {
        let o = self.occ[i];
        if o & OVERFLOW == 0 {
            single[0] = o;
            &single[..]
        } else {
            let off = (o & !OVERFLOW) as usize;
            let n = self.extra[off] as usize;
            &self.extra[off + 1..off + 1 + n]
        }
    }

    /// The first posting at or after index `from` whose doc is `>= doc`.
    #[inline]
    pub fn seek(&self, from: usize, doc: u32) -> usize {
        let docs = &self.docs;
        let mut lo = from;
        if lo >= docs.len() || docs[lo] >= doc {
            return lo;
        }
        // Gallop, then binary search the bracket.
        let mut step = 1;
        let mut hi = lo + 1;
        while hi < docs.len() && docs[hi] < doc {
            lo = hi;
            step <<= 1;
            hi = lo + step;
        }
        let hi = hi.min(docs.len());
        lo + docs[lo..hi].partition_point(|&d| d < doc)
    }

    fn heap_bytes(&self) -> usize {
        (self.docs.capacity() + self.occ.capacity() + self.extra.capacity()) * 4
    }
}

pub(crate) struct DocSlot {
    pub id: Arc<str>,
    pub source: Box<[u8]>,
    pub external: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SortVal {
    Missing,
    Num(f64),
    Str(Box<str>),
}

#[derive(Default)]
pub(crate) struct FacetField {
    pub strings: HashMap<Box<str>, Vec<u32>>,
    /// Original spelling of a folded string value, for aggregation keys.
    pub labels: HashMap<Box<str>, Box<str>>,
    pub numbers: BTreeMap<u64, Vec<u32>>,
    pub exists: Vec<u32>,
}

pub(crate) fn num_key(f: f64) -> u64 {
    let b = f.to_bits();
    if b >> 63 == 1 { !b } else { b | (1 << 63) }
}

pub(crate) struct Inner {
    pub terms: BTreeMap<Box<str>, u32>,
    pub postings: Vec<PostingList>,
    /// Length in chars of each term, by term id: lets a typo scan skip a term
    /// without reading its text.
    pub term_chars: Vec<u8>,
    pub docs: Vec<DocSlot>,
    pub id_map: HashMap<Arc<str>, u32>,
    pub live: Bitmap,
    /// Live documents that may be shown to other processes.
    pub ext: Bitmap,
    pub live_count: usize,
    pub field_len: Vec<u16>,
    pub field_len_sum: Vec<u64>,
    pub facets: Vec<FacetField>,
    pub columns: Vec<Vec<SortVal>>,
    pub dead: usize,
    pub source_bytes: usize,
    /// Running estimate of the heap, for the size limit: the stored source
    /// plus about 24 bytes per posting and 120 per document.
    pub approx_bytes: usize,
    /// Changes on every structural change (add, delete, compact): a cache of
    /// term ids is valid while it matches.
    pub version: u64,
}

impl Inner {
    fn new(schema: &Schema) -> Self {
        Inner {
            terms: BTreeMap::new(),
            postings: Vec::new(),
            term_chars: Vec::new(),
            docs: Vec::new(),
            id_map: HashMap::new(),
            live: Bitmap::new(0),
            ext: Bitmap::new(0),
            live_count: 0,
            field_len: Vec::new(),
            field_len_sum: vec![0; schema.searchable.len()],
            facets: schema.filterable.iter().map(|_| FacetField::default()).collect(),
            columns: schema.sortable.iter().map(|_| Vec::new()).collect(),
            dead: 0,
            source_bytes: 0,
            approx_bytes: 0,
            version: next_version(),
        }
    }

    pub fn n_docs(&self) -> usize {
        self.docs.len()
    }

    fn kill(&mut self, doc: u32, nf: usize) {
        self.version = next_version();
        let d = doc as usize;
        self.live.clear(doc);
        self.ext.clear(doc);
        self.live_count -= 1;
        self.dead += 1;
        for f in 0..nf {
            self.field_len_sum[f] -= u64::from(self.field_len[d * nf + f]);
        }
        self.approx_bytes = self.approx_bytes.saturating_sub(self.docs[d].source.len() + 120);
        self.source_bytes -= self.docs[d].source.len();
        self.docs[d].source = Box::new([]);
    }
}

pub(crate) enum FacetVal {
    Str(String, String),
    Num(f64),
}

struct Prepared {
    id: String,
    source: Vec<u8>,
    external: bool,
    terms: Vec<(String, u32)>,
    field_len: Vec<u16>,
    facets: Vec<(usize, FacetVal)>,
    sorts: Vec<SortVal>,
}

/// A searchable index. Cheap to share: wrap in an [`Arc`].
pub struct Index {
    pub(crate) name: String,
    pub(crate) schema: Schema,
    pub(crate) limits: Limits,
    /// Rank of each searchable field (0 is best), from its weight.
    pub(crate) rank: Vec<u8>,
    pub(crate) stop: HashSet<String>,
    pub(crate) inner: RwLock<Inner>,
    generation: AtomicU64,
    updated: AtomicU64,
    on_change: Mutex<Option<ChangeHook>>,
    pub(crate) exp_cache: Mutex<ExpCache>,
}

/// Prefix expansions of short words, valid for one [`Inner::version`].
#[derive(Default)]
pub(crate) struct ExpCache {
    pub version: u64,
    pub map: HashMap<String, Arc<Vec<(Box<str>, u32, u32)>>>,
}

/// Called, outside the index's lock, after every change.
pub type ChangeHook = Arc<dyn Fn(u64) + Send + Sync>;

const APPLY_BATCH: usize = 256;

/// A process-wide counter, so versions of different indexes never collide.
fn next_version() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

impl Index {
    pub fn new(name: &str, schema: Schema) -> Result<Index> {
        Index::with_limits(name, schema, Limits::default())
    }

    pub fn with_limits(name: &str, schema: Schema, limits: Limits) -> Result<Index> {
        schema.validate()?;
        let mut order: Vec<usize> = (0..schema.searchable.len()).collect();
        order.sort_by(|&a, &b| {
            schema.searchable[b]
                .weight
                .total_cmp(&schema.searchable[a].weight)
        });
        let mut rank = vec![0u8; schema.searchable.len()];
        for (r, &f) in order.iter().enumerate() {
            rank[f] = r.min(255) as u8;
        }
        let stop = schema.stop_words.iter().cloned().collect();
        let inner = Inner::new(&schema);
        Ok(Index {
            name: name.to_string(),
            schema,
            limits,
            rank,
            stop,
            inner: RwLock::new(inner),
            generation: AtomicU64::new(1),
            updated: AtomicU64::new(unix_now()),
            on_change: Mutex::new(None),
            exp_cache: Mutex::new(ExpCache::default()),
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    /// Increases on every change; callers use it to drop caches.
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    /// Sets the function called after every change, replacing the previous
    /// one. It runs on the thread that changed the index and must not block.
    pub fn on_change(&self, hook: ChangeHook) {
        *self.on_change.lock().unwrap_or_else(|e| e.into_inner()) = Some(hook);
    }

    /// When the index last changed, in unix seconds.
    pub fn updated(&self) -> u64 {
        self.updated.load(Ordering::Acquire)
    }

    /// How many documents other processes may see.
    pub fn external_len(&self) -> usize {
        self.read().ext.count()
    }

    /// The document with `id` when other processes may see it.
    pub fn get_external(&self, id: &str) -> Option<Document> {
        let doc = self.get(id)?;
        (self.schema.external && doc.external == Some(true)).then_some(doc)
    }

    fn changed(&self) {
        self.updated.store(unix_now(), Ordering::Release);
        let g = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
        let hook = self.on_change.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if let Some(hook) = hook {
            hook(g);
        }
    }

    pub fn len(&self) -> usize {
        self.read().live_count
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub(crate) fn read(&self) -> std::sync::RwLockReadGuard<'_, Inner> {
        self.inner.read().unwrap_or_else(|e| e.into_inner())
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, Inner> {
        self.inner.write().unwrap_or_else(|e| e.into_inner())
    }

    fn prepare(&self, doc: Document) -> Result<Prepared> {
        let nf = self.schema.searchable.len();
        let external = self.schema.external && doc.external != Some(false);
        let mut terms: Vec<(String, u32)> = Vec::new();
        let mut field_len = vec![0u16; nf];
        let mut toks = Vec::new();
        for (f, field) in self.schema.searchable.iter().enumerate() {
            let mut pos = 0u32;
            let mut count = 0u32;
            for_each_value(&doc.fields, &field.name, &mut |v| {
                let Some(text) = leaf_text(v) else { return };
                toks.clear();
                let next = tokenizer::tokenize_into(&text, pos, true, &mut toks);
                for t in toks.drain(..) {
                    if self.stop.contains(&t.text) {
                        continue;
                    }
                    if !t.joined {
                        count += 1;
                    }
                    terms.push((t.text, pack(f, t.position)));
                }
                pos = next + HARD_GAP;
            });
            field_len[f] = count.min(u32::from(u16::MAX)) as u16;
            let _ = MAX_POSITION;
        }
        terms.sort_unstable();
        terms.dedup();

        let mut facets = Vec::new();
        for (fi, name) in self.schema.filterable.iter().enumerate() {
            for_each_value(&doc.fields, name, &mut |v| match v {
                Value::String(s) => {
                    facets.push((fi, FacetVal::Str(tokenizer::fold(s), s.clone())));
                }
                Value::Bool(b) => {
                    let s = if *b { "true" } else { "false" };
                    facets.push((fi, FacetVal::Str(s.to_string(), s.to_string())));
                }
                Value::Number(n) => {
                    if let Some(x) = n.as_f64() {
                        facets.push((fi, FacetVal::Num(x)));
                    }
                }
                _ => {}
            });
        }
        let mut sorts = Vec::with_capacity(self.schema.sortable.len());
        for name in &self.schema.sortable {
            let mut found = SortVal::Missing;
            for_each_value(&doc.fields, name, &mut |v| {
                if matches!(found, SortVal::Missing) {
                    found = match v {
                        Value::Number(n) => n.as_f64().map_or(SortVal::Missing, SortVal::Num),
                        Value::String(s) => SortVal::Str(tokenizer::fold(s).into_boxed_str()),
                        Value::Bool(b) => SortVal::Num(f64::from(u8::from(*b))),
                        _ => SortVal::Missing,
                    };
                }
            });
            sorts.push(found);
        }
        let source = serde_json::to_vec(&doc.fields)
            .map_err(|e| Error::request(format!("document `{}`: {e}", doc.id)))?;
        if source.len() > self.limits.max_document_bytes {
            return Err(Error::LimitExceeded(format!(
                "document `{}` is {} bytes; the limit is {}",
                doc.id,
                source.len(),
                self.limits.max_document_bytes
            )));
        }
        Ok(Prepared {
            id: doc.id,
            source,
            external,
            terms,
            field_len,
            facets,
            sorts,
        })
    }

    fn apply(&self, inner: &mut Inner, p: Prepared) {
        let nf = self.schema.searchable.len();
        if let Some(&old) = inner.id_map.get(p.id.as_str()) {
            inner.kill(old, nf);
            inner.id_map.remove(p.id.as_str());
        }
        inner.version = next_version();
        let did = inner.docs.len() as u32;
        let external = p.external;
        inner.approx_bytes += p.source.len() + p.terms.len() * 24 + 120;
        let id: Arc<str> = Arc::from(p.id);
        inner.id_map.insert(id.clone(), did);
        inner.source_bytes += p.source.len();
        inner.docs.push(DocSlot {
            id,
            source: p.source.into_boxed_slice(),
            external: p.external,
        });
        inner.live.push(true);
        inner.ext.push(external);
        inner.live_count += 1;
        for (f, &l) in p.field_len.iter().enumerate() {
            inner.field_len.push(l);
            inner.field_len_sum[f] += u64::from(l);
        }
        let mut i = 0;
        while i < p.terms.len() {
            let term = &p.terms[i].0;
            let mut j = i + 1;
            while j < p.terms.len() && p.terms[j].0 == *term {
                j += 1;
            }
            let tid = match inner.terms.get(term.as_str()) {
                Some(&t) => t,
                None => {
                    let t = inner.postings.len() as u32;
                    inner.postings.push(PostingList::default());
                    inner.term_chars.push(term.chars().count().min(255) as u8);
                    inner.terms.insert(term.clone().into_boxed_str(), t);
                    t
                }
            };
            let list = &mut inner.postings[tid as usize];
            list.docs.push(did);
            if j - i == 1 {
                list.occ.push(p.terms[i].1);
            } else {
                let off = list.extra.len() as u32;
                list.extra.push((j - i) as u32);
                list.extra.extend(p.terms[i..j].iter().map(|t| t.1));
                list.occ.push(OVERFLOW | off);
            }
            i = j;
        }
        for (fi, v) in p.facets {
            let ff = &mut inner.facets[fi];
            if ff.exists.last() != Some(&did) {
                ff.exists.push(did);
            }
            match v {
                FacetVal::Str(folded, label) => {
                    let list = ff.strings.entry(folded.clone().into_boxed_str()).or_default();
                    if list.last() != Some(&did) {
                        list.push(did);
                    }
                    ff.labels.entry(folded.into_boxed_str()).or_insert(label.into_boxed_str());
                }
                FacetVal::Num(x) => {
                    let list = ff.numbers.entry(num_key(x)).or_default();
                    if list.last() != Some(&did) {
                        list.push(did);
                    }
                }
            }
        }
        for (ci, v) in p.sorts.into_iter().enumerate() {
            inner.columns[ci].push(v);
        }
    }

    /// Adds documents, replacing those whose id exists. Returns how many were
    /// applied. Stops at the first document that would pass a limit; those
    /// before it stay.
    pub fn upsert<I: IntoIterator<Item = Document>>(&self, docs: I) -> Result<usize> {
        let mut applied = 0;
        let mut batch: Vec<Prepared> = Vec::with_capacity(APPLY_BATCH);
        let mut pending_new: HashSet<String> = HashSet::new();
        let mut failure = None;
        let flush = |batch: &mut Vec<Prepared>, pending: &mut HashSet<String>| -> Result<usize> {
            if batch.is_empty() {
                return Ok(0);
            }
            let mut inner = self.write();
            let mut n = 0;
            for p in batch.drain(..) {
                if !inner.id_map.contains_key(p.id.as_str())
                    && inner.live_count >= self.limits.max_documents
                {
                    pending.clear();
                    return Err(Error::LimitExceeded(format!(
                        "index `{}` holds {} documents, its limit",
                        self.name, self.limits.max_documents
                    )));
                }
                if inner.approx_bytes + p.source.len() + p.terms.len() * 24 > self.limits.max_index_bytes {
                    pending.clear();
                    return Err(Error::LimitExceeded(format!(
                        "index `{}` would pass {} bytes",
                        self.name, self.limits.max_index_bytes
                    )));
                }
                self.apply(&mut inner, p);
                n += 1;
            }
            pending.clear();
            self.maybe_compact(&mut inner);
            Ok(n)
        };
        for doc in docs {
            match self.prepare(doc) {
                Ok(p) => batch.push(p),
                Err(e) => {
                    failure = Some(e);
                    break;
                }
            }
            if batch.len() >= APPLY_BATCH {
                match flush(&mut batch, &mut pending_new) {
                    Ok(n) => applied += n,
                    Err(e) => {
                        failure = Some(e);
                        break;
                    }
                }
            }
        }
        if failure.is_none() {
            match flush(&mut batch, &mut pending_new) {
                Ok(n) => applied += n,
                Err(e) => failure = Some(e),
            }
        } else if !batch.is_empty() {
            // Keep what was prepared before the failing document.
            if let Ok(n) = flush(&mut batch, &mut pending_new) {
                applied += n;
            }
        }
        if applied > 0 {
            self.changed();
        }
        match failure {
            Some(e) => Err(e),
            None => Ok(applied),
        }
    }

    /// Removes documents by id. Returns how many existed.
    pub fn delete<I, S>(&self, ids: I) -> usize
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let nf = self.schema.searchable.len();
        let mut inner = self.write();
        let mut n = 0;
        for id in ids {
            if let Some(doc) = inner.id_map.remove(id.as_ref()) {
                inner.kill(doc, nf);
                n += 1;
            }
        }
        self.maybe_compact(&mut inner);
        drop(inner);
        if n > 0 {
            self.changed();
        }
        n
    }

    /// Removes every document.
    pub fn clear(&self) {
        let mut inner = self.write();
        *inner = Inner::new(&self.schema);
        drop(inner);
        self.changed();
    }

    /// Replaces the whole content: the documents not named are deleted.
    pub fn replace_all<I: IntoIterator<Item = Document>>(&self, docs: I) -> Result<usize> {
        self.clear();
        self.upsert(docs)
    }

    /// The document with `id`, as it was stored.
    pub fn get(&self, id: &str) -> Option<Document> {
        let inner = self.read();
        let &doc = inner.id_map.get(id)?;
        let slot = &inner.docs[doc as usize];
        let fields: Map = serde_json::from_slice(&slot.source).ok()?;
        Some(Document {
            id: id.to_string(),
            fields,
            external: Some(slot.external),
        })
    }

    /// Every live document, optionally only those other processes may see.
    pub fn export(&self, external_only: bool) -> Vec<Document> {
        let inner = self.read();
        let mut out = Vec::with_capacity(inner.live_count);
        for d in inner.live.ones() {
            let slot = &inner.docs[d as usize];
            if external_only && !slot.external {
                continue;
            }
            if let Ok(fields) = serde_json::from_slice::<Map>(&slot.source) {
                out.push(Document {
                    id: slot.id.to_string(),
                    fields,
                    external: Some(slot.external),
                });
            }
        }
        out
    }

    fn estimate_bytes(&self, inner: &Inner) -> usize {
        let nf = self.schema.searchable.len();
        let mut bytes = 0usize;
        bytes += inner.terms.len() * 56;
        bytes += inner.terms.keys().map(|k| k.len()).sum::<usize>();
        bytes += inner.postings.iter().map(|p| 72 + p.heap_bytes()).sum::<usize>();
        bytes += inner.source_bytes;
        bytes += inner.docs.len() * (16 + 8 + 8 + 48);
        bytes += inner.docs.iter().map(|d| d.id.len()).sum::<usize>();
        bytes += inner.field_len.capacity() * 2 + nf * 8;
        bytes += inner.live.heap_bytes();
        for f in &inner.facets {
            bytes += f.exists.capacity() * 4;
            bytes += f.strings.values().map(|v| 56 + v.capacity() * 4).sum::<usize>();
            bytes += f.numbers.values().map(|v| 48 + v.capacity() * 4).sum::<usize>();
        }
        bytes += inner.columns.iter().map(|c| c.capacity() * 24).sum::<usize>();
        bytes
    }

    /// An estimate of the heap the index holds, from the lengths of its
    /// structures.
    pub fn memory_usage(&self) -> usize {
        self.estimate_bytes(&self.read())
    }

    fn maybe_compact(&self, inner: &mut Inner) {
        if inner.dead > 1024 && inner.dead * 4 > inner.docs.len() {
            self.compact(inner);
        }
    }

    /// Rewrites the index without dead documents.
    pub fn compact_now(&self) {
        let mut inner = self.write();
        if inner.dead > 0 {
            self.compact(&mut inner);
        }
    }

    fn compact(&self, inner: &mut Inner) {
        inner.version = next_version();
        let nf = self.schema.searchable.len();
        let old_n = inner.docs.len();
        let mut remap = vec![u32::MAX; old_n];
        let mut next = 0u32;
        for d in inner.live.ones() {
            remap[d as usize] = next;
            next += 1;
        }
        // Documents, ids, field lengths, columns.
        let old_docs = std::mem::take(&mut inner.docs);
        let old_len = std::mem::take(&mut inner.field_len);
        let old_cols = std::mem::take(&mut inner.columns);
        let mut docs = Vec::with_capacity(next as usize);
        let mut field_len = Vec::with_capacity(next as usize * nf);
        let mut cols: Vec<Vec<SortVal>> = old_cols.iter().map(|_| Vec::with_capacity(next as usize)).collect();
        let mut old_cols = old_cols;
        for (d, slot) in old_docs.into_iter().enumerate() {
            if remap[d] == u32::MAX {
                continue;
            }
            inner.id_map.insert(slot.id.clone(), remap[d]);
            docs.push(slot);
            field_len.extend_from_slice(&old_len[d * nf..(d + 1) * nf]);
            for (c, col) in old_cols.iter_mut().enumerate() {
                cols[c].push(std::mem::replace(&mut col[d], SortVal::Missing));
            }
        }
        inner.docs = docs;
        inner.field_len = field_len;
        inner.columns = cols;
        // Postings: remap, drop empty terms.
        let postings = std::mem::take(&mut inner.postings);
        let old_terms = std::mem::take(&mut inner.terms);
        let mut new_postings: Vec<PostingList> = Vec::with_capacity(postings.len());
        let old_chars = std::mem::take(&mut inner.term_chars);
        let mut new_chars: Vec<u8> = Vec::with_capacity(postings.len());
        let mut new_terms = BTreeMap::new();
        let mut postings: Vec<Option<PostingList>> = postings.into_iter().map(Some).collect();
        for (term, tid) in old_terms {
            let list = postings[tid as usize].take().unwrap();
            let mut out = PostingList::default();
            for (i, &d) in list.docs.iter().enumerate() {
                let nd = remap[d as usize];
                if nd == u32::MAX {
                    continue;
                }
                out.docs.push(nd);
                let o = list.occ[i];
                if o & OVERFLOW == 0 {
                    out.occ.push(o);
                } else {
                    let off = (o & !OVERFLOW) as usize;
                    let n = list.extra[off] as usize;
                    let new_off = out.extra.len() as u32;
                    out.extra.extend_from_slice(&list.extra[off..off + 1 + n]);
                    out.occ.push(OVERFLOW | new_off);
                }
            }
            if out.docs.is_empty() {
                continue;
            }
            out.docs.shrink_to_fit();
            out.occ.shrink_to_fit();
            out.extra.shrink_to_fit();
            let nt = new_postings.len() as u32;
            new_chars.push(old_chars[tid as usize]);
            new_postings.push(out);
            new_terms.insert(term, nt);
        }
        inner.postings = new_postings;
        inner.term_chars = new_chars;
        inner.terms = new_terms;
        // Facets.
        for ff in &mut inner.facets {
            let remap_list = |v: &mut Vec<u32>| {
                v.retain_mut(|d| {
                    let nd = remap[*d as usize];
                    *d = nd;
                    nd != u32::MAX
                });
            };
            remap_list(&mut ff.exists);
            ff.strings.retain(|_, v| {
                remap_list(v);
                !v.is_empty()
            });
            ff.numbers.retain(|_, v| {
                remap_list(v);
                !v.is_empty()
            });
            let strings = &ff.strings;
            ff.labels.retain(|k, _| strings.contains_key(k));
        }
        inner.live = Bitmap::full(next as usize);
        let mut ext = Bitmap::new(0);
        for slot in &inner.docs {
            ext.push(slot.external);
        }
        inner.ext = ext;
        inner.dead = 0;
        inner.approx_bytes = inner.source_bytes
            + inner.postings.iter().map(|p| p.docs.len() * 24).sum::<usize>()
            + inner.docs.len() * 120;
    }

    /// Documents matching `filter`, as a bit set over internal ids, live only.
    pub(crate) fn eval_filter(&self, inner: &Inner, filter: &Filter) -> Result<Bitmap> {
        let mut b = self.eval_filter_raw(inner, filter)?;
        b.and_with(&inner.live);
        Ok(b)
    }

    fn facet_index(&self, field: &str) -> Result<usize> {
        self.schema
            .filterable
            .iter()
            .position(|f| f == field)
            .ok_or_else(|| {
                Error::request(format!(
                    "attribute `{field}` is not filterable; filterable attributes are [{}]",
                    self.schema.filterable.join(", ")
                ))
            })
    }

    fn eval_filter_raw(&self, inner: &Inner, filter: &Filter) -> Result<Bitmap> {
        let n = inner.n_docs();
        let from_list = |list: &[u32]| {
            let mut b = Bitmap::new(n);
            for &d in list {
                b.set(d);
            }
            b
        };
        Ok(match filter {
            Filter::Eq(field, lit) => {
                let ff = &inner.facets[self.facet_index(field)?];
                match lit {
                    Lit::Str(s) => ff
                        .strings
                        .get(tokenizer::fold(s).as_str())
                        .map_or_else(|| Bitmap::new(n), |l| from_list(l)),
                    Lit::Bool(v) => ff
                        .strings
                        .get(if *v { "true" } else { "false" })
                        .map_or_else(|| Bitmap::new(n), |l| from_list(l)),
                    Lit::Num(x) => ff
                        .numbers
                        .get(&num_key(*x))
                        .map_or_else(|| Bitmap::new(n), |l| from_list(l)),
                }
            }
            Filter::Ne(field, lit) => {
                let mut b = self.eval_filter_raw(inner, &Filter::Eq(field.clone(), lit.clone()))?;
                b.not();
                b
            }
            Filter::Lt(field, x) => self.numeric(inner, field, None, Some((*x, false)))?,
            Filter::Le(field, x) => self.numeric(inner, field, None, Some((*x, true)))?,
            Filter::Gt(field, x) => self.numeric(inner, field, Some((*x, false)), None)?,
            Filter::Ge(field, x) => self.numeric(inner, field, Some((*x, true)), None)?,
            Filter::Between(field, a, b) => {
                self.numeric(inner, field, Some((*a, true)), Some((*b, true)))?
            }
            Filter::In(field, lits) => {
                let mut out = Bitmap::new(n);
                for lit in lits {
                    out.or_with(&self.eval_filter_raw(inner, &Filter::Eq(field.clone(), lit.clone()))?);
                }
                out
            }
            Filter::Exists(field) => {
                let ff = &inner.facets[self.facet_index(field)?];
                from_list(&ff.exists)
            }
            Filter::Not(f) => {
                let mut b = self.eval_filter_raw(inner, f)?;
                b.not();
                b
            }
            Filter::And(v) => {
                let mut out = Bitmap::full(n);
                for f in v {
                    out.and_with(&self.eval_filter_raw(inner, f)?);
                }
                out
            }
            Filter::Or(v) => {
                let mut out = Bitmap::new(n);
                for f in v {
                    out.or_with(&self.eval_filter_raw(inner, f)?);
                }
                out
            }
        })
    }

    fn numeric(
        &self,
        inner: &Inner,
        field: &str,
        lo: Option<(f64, bool)>,
        hi: Option<(f64, bool)>,
    ) -> Result<Bitmap> {
        use std::ops::Bound::{Excluded, Included, Unbounded};
        let ff = &inner.facets[self.facet_index(field)?];
        let lower = match lo {
            Some((x, true)) => Included(num_key(x)),
            Some((x, false)) => Excluded(num_key(x)),
            None => Unbounded,
        };
        let upper = match hi {
            Some((x, true)) => Included(num_key(x)),
            Some((x, false)) => Excluded(num_key(x)),
            None => Unbounded,
        };
        let mut b = Bitmap::new(inner.n_docs());
        if let (Included(a) | Excluded(a), Included(z) | Excluded(z)) = (&lower, &upper)
            && a > z
        {
            return Ok(b);
        }
        for list in ff.numbers.range((lower, upper)).map(|(_, l)| l) {
            for &d in list {
                b.set(d);
            }
        }
        Ok(b)
    }
}
