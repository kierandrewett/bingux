//! The Spotlight side: finds the apps that expose search, asks each (their
//! socket when they run, their snapshot when they do not) under a deadline,
//! and merges the hits. See `design/search.md` 8.6-8.8.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use serde_json::{Value, json};

use crate::error::{Error, Result};
use crate::index::Index;
use crate::ipc;
use crate::request::Search;
use crate::response::{Hit, Relation, SearchResponse};
use crate::snapshot;
use crate::text::Cancel;

/// The `kind` of an app manifest.
pub const MANIFEST_KIND: &str = "bingux.search-app";

/// An app that exposes search. Installed as
/// `<data dir>/bingux/search-apps/<app-id>.json`:
///
/// ```json
/// { "kind": "bingux.search-app", "id": "bingux-settings", "displayName": "Settings",
///   "command": ["/usr/bin/bingux-settings"], "appVersion": "0.1.0" }
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct AppManifest {
    pub id: String,
    pub display_name: String,
    /// The program and arguments; the hit's `uri` is appended to open it.
    pub command: Vec<String>,
    /// The installed version. A snapshot of another version is ignored. A
    /// snapshot older than the program in `command` is ignored too.
    pub app_version: Option<String>,
}

impl AppManifest {
    pub fn parse(bytes: &[u8]) -> Result<AppManifest> {
        let v: Value = serde_json::from_slice(bytes)
            .map_err(|e| Error::Corrupt(format!("manifest is not JSON: {e}")))?;
        if v.get("kind").and_then(Value::as_str) != Some(MANIFEST_KIND) {
            return Err(Error::Corrupt(format!("manifest kind is not {MANIFEST_KIND}")));
        }
        let id = v
            .get("id")
            .and_then(Value::as_str)
            .filter(|s| {
                !s.is_empty()
                    && s.len() <= 64
                    && s.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
            })
            .ok_or_else(|| Error::Corrupt("manifest id is missing or not a valid app id".into()))?;
        let command: Vec<String> = v
            .get("command")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect())
            .unwrap_or_default();
        if command.first().is_none_or(|p| !Path::new(p).is_absolute())
            || command.iter().any(|a| a.is_empty() || a.contains('\0'))
        {
            return Err(Error::Corrupt(
                "manifest command must be an argument vector starting with an absolute path".into(),
            ));
        }
        Ok(AppManifest {
            id: id.to_string(),
            display_name: v
                .get("displayName")
                .and_then(Value::as_str)
                .unwrap_or(id)
                .to_string(),
            command,
            app_version: v.get("appVersion").and_then(Value::as_str).map(str::to_string),
        })
    }
}

/// Where manifests are looked for: the user's data dir, then
/// `$XDG_DATA_DIRS` (`/usr/local/share:/usr/share` by default).
pub fn manifest_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let home = match std::env::var_os("XDG_DATA_HOME") {
        Some(p) if !p.is_empty() => Some(PathBuf::from(p)),
        _ => std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")),
    };
    dirs.extend(home);
    let system = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".to_string());
    dirs.extend(system.split(':').filter(|s| !s.is_empty()).map(PathBuf::from));
    dirs.into_iter().map(|d| d.join("bingux/search-apps")).collect()
}

/// Reads every `*.json` manifest in `dirs`; the first directory to name an id
/// wins. Unreadable or invalid manifests are skipped.
pub fn load_manifests(dirs: &[PathBuf]) -> Vec<AppManifest> {
    let mut out: Vec<AppManifest> = Vec::new();
    for dir in dirs {
        let Ok(entries) = fs::read_dir(dir) else { continue };
        let mut files: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
            .collect();
        files.sort();
        for path in files {
            let Ok(meta) = fs::metadata(&path) else { continue };
            if meta.len() > 64 * 1024 {
                continue;
            }
            let Ok(bytes) = fs::read(&path) else { continue };
            if let Ok(m) = AppManifest::parse(&bytes)
                && !out.iter().any(|o| o.id == m.id)
            {
                out.push(m);
            }
        }
    }
    out
}

/// Where an answer came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// The running app, over its socket.
    Live,
    /// The app's snapshot on disk.
    Snapshot,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AppHit {
    pub app: String,
    pub source: Source,
    pub hit: Hit,
    /// `hit.score` times the app's weight.
    pub weighted: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Aggregated {
    pub hits: Vec<AppHit>,
    /// Apps that did not answer in time or failed, with the reason.
    pub failed: Vec<(String, String)>,
    pub total: u64,
}

impl Aggregated {
    /// The merged hits as an OpenSearch `_search` body; `_index` is
    /// `<app>/<index>`.
    pub fn to_json(&self) -> Value {
        let mut resp = SearchResponse::empty();
        resp.total = self.total;
        resp.relation = Relation::Eq;
        resp.timed_out = !self.failed.is_empty();
        for h in &self.hits {
            let mut hit = h.hit.clone();
            hit.index = format!("{}/{}", h.app, hit.index).into();
            hit.score = h.weighted;
            resp.hits.push(hit);
        }
        resp.max_score = resp.hits.first().map(|h| h.score);
        resp.to_json()
    }
}

struct Cached {
    modified: SystemTime,
    index: Arc<Index>,
}

pub struct Aggregator {
    manifests: Vec<AppManifest>,
    socket_dir: Option<PathBuf>,
    state_root: Option<PathBuf>,
    weights: HashMap<String, f64>,
    disabled: HashSet<String>,
    deadline: Duration,
    cache: Mutex<HashMap<PathBuf, Cached>>,
}

impl Aggregator {
    pub fn new(manifests: Vec<AppManifest>) -> Aggregator {
        Aggregator {
            manifests,
            socket_dir: ipc::socket_dir(),
            state_root: snapshot::state_root(),
            weights: HashMap::new(),
            disabled: HashSet::new(),
            deadline: Duration::from_millis(40),
            cache: Mutex::new(HashMap::new()),
        }
    }

    pub fn socket_dir(mut self, dir: PathBuf) -> Self {
        self.socket_dir = Some(dir);
        self
    }

    pub fn state_root(mut self, root: PathBuf) -> Self {
        self.state_root = Some(root);
        self
    }

    /// Per-app multiplier of the score (default 1.0).
    pub fn weight(mut self, app: &str, weight: f64) -> Self {
        self.weights.insert(app.to_string(), weight);
        self
    }

    pub fn disable(mut self, app: &str) -> Self {
        self.disabled.insert(app.to_string());
        self
    }

    pub fn deadline(mut self, deadline: Duration) -> Self {
        self.deadline = deadline;
        self
    }

    pub fn manifests(&self) -> &[AppManifest] {
        &self.manifests
    }

    /// The command that opens `uri` in `app`.
    pub fn open_command(&self, app: &str, uri: &str) -> Option<Vec<String>> {
        let m = self.manifests.iter().find(|m| m.id == app)?;
        let mut cmd = m.command.clone();
        cmd.push(uri.to_string());
        Some(cmd)
    }

    /// Loads every snapshot now, so the first query does not pay for it.
    pub fn warm(&self) {
        for m in &self.manifests {
            let _ = self.snapshots(m);
        }
    }

    /// Searches `text` in every app: at most `per_app` hits from each, `total`
    /// overall.
    pub fn search_text(&self, text: &str, per_app: usize, total: usize) -> Aggregated {
        let body = json!({ "q": text, "limit": per_app });
        self.search_body(&body, per_app, total)
    }

    /// Runs `body` (OpenSearch or shorthand) in every external index of every
    /// app and merges by weighted score.
    pub fn search_body(&self, body: &Value, per_app: usize, total: usize) -> Aggregated {
        let started = Instant::now();
        let live: HashMap<String, PathBuf> = self
            .socket_dir
            .as_deref()
            .map(ipc::running)
            .unwrap_or_default()
            .into_iter()
            .collect();
        let results: Vec<(String, std::result::Result<(Source, Vec<Hit>, u64), String>)> =
            std::thread::scope(|scope| {
                let handles: Vec<_> = self
                    .manifests
                    .iter()
                    .filter(|m| !self.disabled.contains(&m.id))
                    .map(|m| {
                        let live = live.get(&m.id);
                        scope.spawn(move || {
                            let left = self
                                .deadline
                                .saturating_sub(started.elapsed())
                                .max(Duration::from_millis(1));
                            let r = self.ask(m, live.map(PathBuf::as_path), body, per_app, left);
                            (m.id.clone(), r)
                        })
                    })
                    .collect();
                handles.into_iter().filter_map(|h| h.join().ok()).collect()
            });
        let mut out = Aggregated::default();
        for (app, r) in results {
            match r {
                Ok((source, hits, n)) => {
                    out.total += n;
                    let w = self.weights.get(&app).copied().unwrap_or(1.0);
                    for hit in hits {
                        let weighted = hit.score * w;
                        out.hits.push(AppHit {
                            app: app.clone(),
                            source,
                            hit,
                            weighted,
                        });
                    }
                }
                Err(reason) => out.failed.push((app, reason)),
            }
        }
        out.hits
            .sort_by(|a, b| b.weighted.total_cmp(&a.weighted).then(a.app.cmp(&b.app)));
        out.hits.truncate(total);
        out
    }

    fn ask(
        &self,
        m: &AppManifest,
        socket: Option<&Path>,
        body: &Value,
        per_app: usize,
        timeout: Duration,
    ) -> std::result::Result<(Source, Vec<Hit>, u64), String> {
        if let Some(path) = socket {
            match self.ask_live(path, body, timeout) {
                Ok((hits, n)) => return Ok((Source::Live, hits, n)),
                Err(e) => {
                    // Fall through to the snapshot: the app may be starting
                    // up or shutting down.
                    if self.state_root.is_none() {
                        return Err(e);
                    }
                }
            }
        }
        let indexes = self.snapshots(m);
        let mut hits = Vec::new();
        let mut total = 0;
        let cancel = Cancel::new();
        for index in indexes {
            let mut search = Search::from_json(body).map_err(|e| e.to_string())?;
            search.external_only = true;
            search.size = search.size.min(per_app.max(1));
            let r = index.search(&search, &cancel).map_err(|e| e.to_string())?;
            total += r.total;
            hits.extend(r.hits);
        }
        Ok((Source::Snapshot, hits, total))
    }

    fn ask_live(
        &self,
        path: &Path,
        body: &Value,
        timeout: Duration,
    ) -> std::result::Result<(Vec<Hit>, u64), String> {
        let started = Instant::now();
        let hello = ipc::request(
            path,
            &json!({ "v": ipc::PROTOCOL_VERSION, "type": "hello" }),
            timeout,
        )
        .map_err(|e| e.to_string())?;
        let names: Vec<String> = hello
            .get("indexes")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|i| i.get("name").and_then(Value::as_str).map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let mut hits = Vec::new();
        let mut total = 0;
        for name in names {
            let left = timeout
                .saturating_sub(started.elapsed())
                .max(Duration::from_millis(1));
            let reply = ipc::request(
                path,
                &json!({ "v": ipc::PROTOCOL_VERSION, "type": "search", "id": "q",
                         "index": name, "body": body }),
                left,
            )
            .map_err(|e| e.to_string())?;
            match reply.get("type").and_then(Value::as_str) {
                Some("result") => {
                    let r = reply
                        .get("response")
                        .and_then(SearchResponse::from_json)
                        .ok_or("malformed response")?;
                    total += r.total;
                    hits.extend(r.hits);
                }
                _ => {
                    return Err(reply
                        .pointer("/error/reason")
                        .and_then(Value::as_str)
                        .unwrap_or("the app returned an error")
                        .to_string());
                }
            }
        }
        Ok((hits, total))
    }

    /// The snapshot indexes of `m`, loading those that are new or changed.
    fn snapshots(&self, m: &AppManifest) -> Vec<Arc<Index>> {
        let Some(root) = &self.state_root else {
            return Vec::new();
        };
        let dir = root.join(&m.id);
        let Ok(entries) = fs::read_dir(&dir) else {
            return Vec::new();
        };
        let mut files: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "idx"))
            .collect();
        files.sort();
        let mut out = Vec::new();
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        for path in files {
            let Ok(modified) = fs::metadata(&path).and_then(|m| m.modified()) else {
                continue;
            };
            // An app updated after its snapshot was written has a new binary
            // that may tokenize or describe documents differently: the old
            // snapshot waits for the app to run and write a new one.
            if let Some(bin) = m.command.first().and_then(|p| fs::metadata(p).and_then(|m| m.modified()).ok())
                && bin > modified
            {
                cache.remove(&path);
                continue;
            }
            if let Some(c) = cache.get(&path)
                && c.modified == modified
            {
                out.push(c.index.clone());
                continue;
            }
            match snapshot::load(&path, m.app_version.as_deref()) {
                Ok((_, index)) => {
                    let index = Arc::new(index);
                    cache.insert(
                        path,
                        Cached {
                            modified,
                            index: index.clone(),
                        },
                    );
                    out.push(index);
                }
                Err(_) => {
                    cache.remove(&path);
                }
            }
        }
        out
    }
}

/// Builds the `BTreeMap` of weights from `(app, weight)` pairs, for config.
pub fn weights_from(pairs: impl IntoIterator<Item = (String, f64)>) -> BTreeMap<String, f64> {
    pairs.into_iter().collect()
}
