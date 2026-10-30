//! The process-wide registry of indexes.

use std::collections::BTreeMap;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, LazyLock, Mutex, RwLock};
use std::time::Instant;

use serde_json::Value;

use crate::document::Document;
use crate::error::{Error, Result};
use crate::index::Index;
use crate::request::{Search, SortKey};
use crate::response::{Aggregation, Bucket, SearchResponse};
use crate::schema::Schema;
use crate::text::Cancel;

/// What an app registers: a named index with a schema and a way to produce
/// its documents. The documents are produced by the host on a worker thread,
/// so `documents` may read files or run commands.
pub trait Searchable: Send + Sync + 'static {
    /// The index name, unique in the process: `"settings"`.
    fn name(&self) -> &str;

    fn schema(&self) -> Schema;

    /// Every document, now. Called on a worker, never on the UI thread.
    fn documents(&self) -> Box<dyn Iterator<Item = Document> + Send + '_>;
}

/// A change notification: the index name and its new generation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub index: String,
    pub generation: u64,
}

#[derive(Default)]
pub struct SearchService {
    indexes: RwLock<BTreeMap<String, Arc<Index>>>,
    watchers: Mutex<Vec<Sender<Change>>>,
}

impl SearchService {
    pub fn new() -> Arc<SearchService> {
        Arc::new(SearchService::default())
    }

    /// The service of this process, which `bingux_ui::search` uses.
    pub fn global() -> &'static Arc<SearchService> {
        static GLOBAL: LazyLock<Arc<SearchService>> = LazyLock::new(SearchService::new);
        &GLOBAL
    }

    /// Creates the index `name`. An index with the same name and schema is
    /// returned as it is; a different schema replaces it (its documents are
    /// lost, the owner reloads).
    pub fn create_index(self: &Arc<Self>, name: &str, schema: Schema) -> Result<Arc<Index>> {
        {
            let map = self.indexes.read().unwrap_or_else(|e| e.into_inner());
            if let Some(existing) = map.get(name)
                && *existing.schema() == schema
            {
                return Ok(existing.clone());
            }
        }
        let index = Arc::new(Index::new(name, schema)?);
        let weak = Arc::downgrade(self);
        let owned = name.to_string();
        index.on_change(Arc::new(move |generation| {
            if let Some(service) = weak.upgrade() {
                service.notify(Change {
                    index: owned.clone(),
                    generation,
                });
            }
        }));
        self.indexes
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(name.to_string(), index.clone());
        Ok(index)
    }

    /// Registers a [`Searchable`]: creates its index. Load it with
    /// [`SearchService::load`] on a worker.
    pub fn register(self: &Arc<Self>, searchable: &dyn Searchable) -> Result<Arc<Index>> {
        self.create_index(searchable.name(), searchable.schema())
    }

    /// Replaces the content of the registered index with the searchable's
    /// documents. Blocking: call it on a worker.
    pub fn load(self: &Arc<Self>, searchable: &dyn Searchable) -> Result<usize> {
        let index = self.register(searchable)?;
        index.replace_all(searchable.documents())
    }

    pub fn index(&self, name: &str) -> Option<Arc<Index>> {
        self.indexes
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(name)
            .cloned()
    }

    pub fn names(&self) -> Vec<String> {
        self.indexes
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .keys()
            .cloned()
            .collect()
    }

    pub fn remove(&self, name: &str) {
        self.indexes
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(name);
    }

    /// Changes of any index, as they happen. Dropping the receiver
    /// unsubscribes.
    pub fn watch(&self) -> Receiver<Change> {
        let (tx, rx) = channel();
        self.watchers.lock().unwrap_or_else(|e| e.into_inner()).push(tx);
        rx
    }

    /// Wakes every watcher with an empty change, so one that is stopping
    /// notices its flag.
    pub fn poke(&self) {
        self.notify(Change {
            index: String::new(),
            generation: 0,
        });
    }

    fn notify(&self, change: Change) {
        let mut w = self.watchers.lock().unwrap_or_else(|e| e.into_inner());
        w.retain(|tx| tx.send(change.clone()).is_ok());
    }

    /// Runs `search` on the indexes it names (all when none), merging their
    /// hits by score. Sorting by a field is only possible on one index.
    pub fn search(&self, search: &Search, cancel: &Cancel) -> Result<SearchResponse> {
        let started = Instant::now();
        let targets: Vec<Arc<Index>> = if search.indexes.is_empty() {
            let map = self.indexes.read().unwrap_or_else(|e| e.into_inner());
            map.values()
                .filter(|i| !search.external_only || i.schema().external)
                .cloned()
                .collect()
        } else {
            search
                .indexes
                .iter()
                .map(|n| {
                    self.index(n)
                        .filter(|i| !search.external_only || i.schema().external)
                        .ok_or_else(|| Error::UnknownIndex(n.clone()))
                })
                .collect::<Result<_>>()?
        };
        match targets.len() {
            0 => Ok(SearchResponse::empty()),
            1 => targets[0].search(search, cancel),
            _ => {
                if search
                    .sort
                    .iter()
                    .any(|s| matches!(s.key, SortKey::Field(_)))
                    || search.search_after.is_some()
                {
                    return Err(Error::request(
                        "sorting by a field and search_after work on one index at a time",
                    ));
                }
                let mut each = search.clone();
                each.from = 0;
                each.size = search.from + search.size;
                let mut merged = SearchResponse::empty();
                let mut all = Vec::new();
                for index in &targets {
                    let r = index.search(&each, cancel)?;
                    merged.total += r.total;
                    merged.timed_out |= r.timed_out;
                    if r.relation == crate::response::Relation::Gte {
                        merged.relation = crate::response::Relation::Gte;
                    }
                    all.extend(r.hits);
                    for (name, agg) in r.aggregations {
                        merge_agg(merged.aggregations.entry(name).or_insert(Aggregation {
                            buckets: Vec::new(),
                            sum_other_doc_count: 0,
                        }), agg);
                    }
                }
                all.sort_by(|a, b| b.score.total_cmp(&a.score));
                merged.hits = all.into_iter().skip(search.from).take(search.size).collect();
                merged.max_score = merged.hits.first().map(|h| h.score);
                merged.took_us = started.elapsed().as_micros() as u64;
                Ok(merged)
            }
        }
    }

    /// Parses `body` and searches; an error becomes an OpenSearch error body.
    /// `external_only` restricts to what other processes may see.
    pub fn search_json(
        &self,
        index: Option<&str>,
        body: &Value,
        external_only: bool,
        cancel: &Cancel,
    ) -> std::result::Result<Value, Value> {
        let mut search = Search::from_json(body).map_err(|e| e.to_json())?;
        if let Some(name) = index {
            search.indexes = vec![name.to_string()];
        }
        search.external_only = external_only;
        self.search(&search, cancel)
            .map(|r| r.to_json())
            .map_err(|e| e.to_json())
    }
}

fn merge_agg(into: &mut Aggregation, from: Aggregation) {
    for b in from.buckets {
        match into.buckets.iter_mut().find(|x| x.key == b.key) {
            Some(x) => x.doc_count += b.doc_count,
            None => into.buckets.push(Bucket {
                key: b.key,
                doc_count: b.doc_count,
            }),
        }
    }
    into.sum_other_doc_count += from.sum_other_doc_count;
    into.buckets.sort_by(|a, b| b.doc_count.cmp(&a.doc_count));
}
