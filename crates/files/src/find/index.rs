//! The names index: file names under the home folder, in the shared search
//! engine (`bingux-search`), so a search there answers before the walker has
//! read a folder.
//!
//! The index holds one document per file or folder: its path as the id, the
//! name (the one searchable field), the extension, the kind and every
//! ancestor folder (so "below this folder" is a filter). It does not hold
//! sizes or dates: a hit is read from the disk when it is returned, which
//! also drops what has gone since.
//!
//! What it is, and is not:
//!
//! - The engine matches words: the start of a word and near misses. It does
//!   not match the middle of a word (`port` in `report`) or letters in
//!   order; the walker does, so a search always runs the walker too
//!   ([`super::engine`]). The index is the quick first answer and the source
//!   of near-miss matches, never the only source.
//! - It is built by a crawl on a thread of its own at a low priority, the
//!   first time a search starts under the home folder, and again when it is
//!   older than [`STALE_AFTER`]. The crawl skips hidden entries and
//!   [`NOISE_FOLDERS`](super::NOISE_FOLDERS) and stays on one filesystem. It
//!   keeps at most [`MAX_DOCUMENTS`] names. A search under the home folder
//!   uses whatever is indexed so far.
//! - It lives in this process only. Nothing is written to disk and it is
//!   not shared with `bingux-searchd`, which keeps its own file index for the
//!   launcher (see `design/files-search.md`).

use std::collections::HashSet;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant};

use bingux_search::{Cancel as EngineCancel, Document, Filter, Index, Limits, Lit, Schema, Search, Strategy};

use super::matcher::Matcher;
use super::query::{Kind, NameMode};
use super::walk::NOISE_FOLDERS;
use crate::listing::hidden_names;
use crate::mime;

/// The index is crawled again when it is older than this.
pub const STALE_AFTER: Duration = Duration::from_secs(300);

/// The most names the index keeps.
pub const MAX_DOCUMENTS: usize = 500_000;

/// How many hits one search takes from the index. The walker finds the rest.
pub const TAKE: usize = 1_000;

/// How many documents a crawl adds to the index at a time.
const BATCH: usize = 512;

enum State {
    /// Never crawled.
    Empty,
    Crawling,
    Ready {
        at: Instant,
    },
}

/// The names under one folder, in the engine.
pub struct NameIndex {
    root: PathBuf,
    index: RwLock<Arc<Index>>,
    state: Mutex<State>,
}

fn schema() -> Schema {
    Schema::new().searchable("name", 10.0).filterable(["ext", "kind", "under"])
}

fn fresh_index() -> Arc<Index> {
    Arc::new(
        Index::with_limits(
            "files.names",
            schema(),
            Limits {
                max_documents: MAX_DOCUMENTS,
                ..Limits::default()
            },
        )
        .expect("the names schema is valid"),
    )
}

impl NameIndex {
    /// An index of the names under `root`, empty until [`NameIndex::refresh`].
    pub fn new(root: &Path) -> Arc<NameIndex> {
        Arc::new(NameIndex {
            root: root.to_path_buf(),
            index: RwLock::new(fresh_index()),
            state: Mutex::new(State::Empty),
        })
    }

    /// The index of this process, for the home folder `home`. The first call
    /// decides the folder.
    pub fn global(home: &Path) -> Arc<NameIndex> {
        static GLOBAL: OnceLock<Arc<NameIndex>> = OnceLock::new();
        GLOBAL.get_or_init(|| NameIndex::new(home)).clone()
    }

    /// The folder the index covers.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Whether a search below `folder` can use the index.
    pub fn covers(&self, folder: &Path) -> bool {
        folder.starts_with(&self.root)
    }

    /// How many names are indexed.
    pub fn len(&self) -> usize {
        self.engine().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// A crawl is running.
    pub fn crawling(&self) -> bool {
        matches!(*self.state.lock().unwrap_or_else(|e| e.into_inner()), State::Crawling)
    }

    /// Crawled at least once.
    pub fn ready(&self) -> bool {
        matches!(*self.state.lock().unwrap_or_else(|e| e.into_inner()), State::Ready { .. })
    }

    fn engine(&self) -> Arc<Index> {
        self.index.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Start a crawl on a thread of its own when there is none running and
    /// the index is empty or [`STALE_AFTER`] old. Returns at once.
    pub fn refresh(self: &Arc<Self>) {
        {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            match &*state {
                State::Crawling => return,
                State::Ready { at } if at.elapsed() < STALE_AFTER => return,
                _ => {}
            }
            *state = State::Crawling;
        }
        let this = self.clone();
        let spawned = std::thread::Builder::new().name("bingux-names-crawl".into()).spawn(move || {
            lower_priority();
            let next = fresh_index();
            // A crawl that stopped on the document limit is still a usable
            // index; the walker covers the rest.
            this.crawl(&next);
            *this.index.write().unwrap_or_else(|e| e.into_inner()) = next;
            *this.state.lock().unwrap_or_else(|e| e.into_inner()) = State::Ready { at: Instant::now() };
        });
        if spawned.is_err() {
            *self.state.lock().unwrap_or_else(|e| e.into_inner()) = State::Empty;
        }
    }

    /// Walk the root and add every name that is not hidden or in a noise
    /// folder to `into`. False when it stopped at the document limit.
    fn crawl(&self, into: &Index) -> bool {
        let Ok(root_meta) = fs::metadata(&self.root) else {
            return true;
        };
        let device = root_meta.dev();
        let mut seen: HashSet<(u64, u64)> = HashSet::new();
        seen.insert((root_meta.dev(), root_meta.ino()));
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(self.root.clone());
        let mut batch: Vec<Document> = Vec::with_capacity(BATCH);
        let mut total = 0_usize;
        while let Some(folder) = queue.pop_front() {
            let Ok(reader) = fs::read_dir(&folder) else {
                continue;
            };
            let listed = hidden_names(&folder);
            let ancestors = ancestors_of(&folder, &self.root);
            for item in reader.flatten() {
                let file_name = item.file_name();
                let Some(name) = file_name.to_str() else {
                    continue;
                };
                if name.starts_with('.') || listed.contains(name) {
                    continue;
                }
                let Ok(file_type) = item.file_type() else {
                    continue;
                };
                let is_dir = file_type.is_dir();
                if is_dir && NOISE_FOLDERS.contains(&name) {
                    continue;
                }
                let path = folder.join(&file_name);
                let Some(path_text) = path.to_str() else {
                    continue;
                };
                batch.push(document(path_text, name, is_dir, &ancestors));
                total += 1;
                if batch.len() >= BATCH {
                    if into.upsert(std::mem::take(&mut batch)).is_err() {
                        return false;
                    }
                    batch = Vec::with_capacity(BATCH);
                }
                if total >= MAX_DOCUMENTS {
                    let _ = into.upsert(batch);
                    return false;
                }
                if is_dir
                    && let Ok(meta) = fs::symlink_metadata(&path)
                    && meta.dev() == device
                    && seen.insert((meta.dev(), meta.ino()))
                {
                    queue.push_back(path);
                }
            }
        }
        let _ = into.upsert(batch);
        true
    }

    /// The indexed paths below `folder` that the query's words, extensions
    /// and kinds can match, best first, at most [`TAKE`]. Empty when the
    /// index has nothing to say (not covered, or a query only the walker
    /// answers: a regular expression, a start or an exact name, a case
    /// that matters).
    pub fn candidates(&self, matcher: &Matcher, folder: &Path, cancel: &crate::listing::Cancel) -> Vec<PathBuf> {
        let query = matcher.query();
        let filters = &query.filters;
        if !self.covers(folder) || filters.name_mode != NameMode::Contains || filters.case_sensitive {
            return Vec::new();
        }
        if cancel.is_cancelled() {
            return Vec::new();
        }
        let text = engine_text(matcher);
        let mut parts = vec![Filter::Eq("under".to_string(), Lit::Str(folder.to_string_lossy().into_owned()))];
        if !filters.exts.is_empty() {
            parts.push(Filter::In(
                "ext".to_string(),
                filters.exts.iter().map(|ext| Lit::Str(ext.clone())).collect(),
            ));
        }
        if !filters.kinds.is_empty() {
            parts.push(Filter::In(
                "kind".to_string(),
                filters.kinds.iter().map(|kind| Lit::Str(kind.word().to_string())).collect(),
            ));
        }
        let mut search = Search::new(&text).limit(TAKE).strategy(Strategy::All);
        for part in parts {
            search = search.filter_ast(part);
        }
        let Ok(response) = self.engine().search(&search, &EngineCancel::new()) else {
            return Vec::new();
        };
        response.hits.into_iter().map(|hit| PathBuf::from(hit.id)).collect()
    }
}

/// The words as the engine takes them: phrases in quotes.
fn engine_text(matcher: &Matcher) -> String {
    let mut text = String::new();
    for term in matcher.query().terms.iter().filter(|term| !term.is_excluded()) {
        if !text.is_empty() {
            text.push(' ');
        }
        if term.is_phrase() {
            text.push('"');
            text.push_str(term.text());
            text.push('"');
        } else {
            text.push_str(term.text());
        }
    }
    text
}

/// `folder` and every folder above it down to `root`, as the index writes
/// them.
fn ancestors_of(folder: &Path, root: &Path) -> Vec<String> {
    let mut list = Vec::new();
    let mut at = Some(folder);
    while let Some(path) = at {
        if !path.starts_with(root) {
            break;
        }
        list.push(path.to_string_lossy().into_owned());
        at = path.parent();
    }
    list
}

fn document(path: &str, name: &str, is_dir: bool, ancestors: &[String]) -> Document {
    let kind = if is_dir {
        Some(Kind::Folder)
    } else {
        Kind::of(mime::for_name(name).category)
    };
    let ext = if is_dir {
        ""
    } else {
        name.rsplit_once('.').map_or("", |(stem, ext)| if stem.is_empty() { "" } else { ext })
    };
    Document::new(path)
        .field("name", name)
        .field("ext", ext.to_lowercase())
        .field("kind", kind.map_or("other", Kind::word))
        .field(
            "under",
            ancestors.iter().map(|path| bingux_search::Value::String(path.clone())).collect::<Vec<_>>(),
        )
}

/// Run the calling thread at a lower priority, so a crawl never competes
/// with the window.
fn lower_priority() {
    // SAFETY: `setpriority` takes plain integers; `gettid` has no arguments.
    // On Linux a nice value set with `PRIO_PROCESS` and a thread id applies
    // to that thread alone.
    unsafe {
        let tid = libc::syscall(libc::SYS_gettid) as libc::id_t;
        libc::setpriority(libc::PRIO_PROCESS, tid, 10);
    }
}
