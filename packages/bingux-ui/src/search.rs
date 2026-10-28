//! Search for Bingux apps: register what is searchable once, get in-app
//! search and Spotlight for free.
//!
//! The engine is the UI-free crate `bingux-search`, re-exported here, so an
//! app writes no matcher: it describes its [`Document`]s and a [`Schema`],
//! registers a [`Searchable`], and draws the hits however it likes.
//!
//! ```ignore
//! struct Pages;
//! impl Searchable for Pages {
//!     fn name(&self) -> &str { "pages" }
//!     fn schema(&self) -> Schema {
//!         Schema::new().searchable("title", 10.0).searchable("keywords", 5.0)
//!             .displayed(["title", "uri", "icon"])
//!     }
//!     fn documents(&self) -> Box<dyn Iterator<Item = Document> + Send + '_> {
//!         Box::new(PAGES.iter().map(|p| Document::new(p.id).title(p.title).uri(p.uri)))
//!     }
//! }
//! search::register(Pages);                       // once, at start-up
//! let state = search::use_search("pages", &text); // in a component
//! for hit in &state.hits { /* hit.title(), hit.ranges("title") */ }
//! ```
//!
//! Searches and loads run on the worker pool of [`crate::task`]; a component
//! never waits for them. See `design/search.md`.

use std::cell::RefCell;
use std::future::poll_fn;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, LazyLock, Mutex, PoisonError};
use std::task::{Poll, Waker};
use std::time::Duration;

use freya::prelude::*;

pub use bingux_search::*;

use crate::task::{spawn_detached, spawn_named};

/// Every searchable registered in this process, so it can be reloaded.
static REGISTERED: LazyLock<Mutex<Vec<Arc<dyn Searchable>>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

/// Indexes whose documents are being loaded.
static LOADING: LazyLock<Mutex<Vec<String>>> = LazyLock::new(|| Mutex::new(Vec::new()));
/// Signalled whenever a load finishes.
static LOAD_DONE: Condvar = Condvar::new();

/// The process-wide search service.
pub fn service() -> &'static Arc<SearchService> {
    SearchService::global()
}

/// The registered index called `name`.
pub fn index(name: &str) -> Option<Arc<Index>> {
    service().index(name)
}

/// Registers `searchable` and loads its documents on a worker. The index
/// exists at once and fills as the load runs; [`use_search`] reports
/// `loading` until it is done. Registering a name again reloads it.
pub fn register(searchable: impl Searchable) -> Result<Arc<Index>> {
    let searchable: Arc<dyn Searchable> = Arc::new(searchable);
    let index = service().register(&*searchable)?;
    {
        let mut all = REGISTERED.lock().unwrap_or_else(PoisonError::into_inner);
        all.retain(|s| s.name() != searchable.name());
        all.push(searchable.clone());
    }
    load(searchable);
    start_bridge();
    Ok(index)
}

/// Loads the documents of the registered searchable `name` again, on a
/// worker: call it when the data changed (apps installed, a list refreshed).
pub fn reload(name: &str) {
    let found = REGISTERED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .find(|s| s.name() == name)
        .cloned();
    if let Some(searchable) = found {
        load(searchable);
    }
}

fn load(searchable: Arc<dyn Searchable>) {
    let name = searchable.name().to_string();
    LOADING
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(name.clone());
    spawn_detached("search-load", move || {
        if let Err(error) = service().load(&*searchable) {
            eprintln!("bingux-ui: search index `{name}` did not load: {error}");
        }
        let mut loading = LOADING.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(at) = loading.iter().position(|n| *n == name) {
            loading.remove(at);
        }
        drop(loading);
        LOAD_DONE.notify_all();
        wake_all();
    });
}

/// Blocks until every registered index has loaded, or `timeout` passes.
/// Returns whether they all loaded. For a process with no window (a headless
/// provider, a test); a component never waits.
pub fn wait_loaded(timeout: Duration) -> bool {
    let loading = LOADING.lock().unwrap_or_else(PoisonError::into_inner);
    let (loading, _) = LOAD_DONE
        .wait_timeout_while(loading, timeout, |loading| !loading.is_empty())
        .unwrap_or_else(PoisonError::into_inner);
    loading.is_empty()
}

/// Whether the documents of index `name` are still being loaded.
pub fn is_loading(name: &str) -> bool {
    LOADING
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .any(|n| n == name)
}

// ------------------------------------------------------------ change wake-up

/// Counts index changes; [`changed`] resolves when it moves.
static EPOCH: AtomicU64 = AtomicU64::new(0);
static WAKERS: Mutex<Vec<Waker>> = Mutex::new(Vec::new());

fn wake_all() {
    EPOCH.fetch_add(1, Ordering::AcqRel);
    let wakers = std::mem::take(&mut *WAKERS.lock().unwrap_or_else(PoisonError::into_inner));
    for w in wakers {
        w.wake();
    }
}

/// Resolves once any index has changed since `seen`, returning the new epoch.
async fn changed(seen: u64) -> u64 {
    poll_fn(move |cx| {
        let now = EPOCH.load(Ordering::Acquire);
        if now != seen {
            return Poll::Ready(now);
        }
        WAKERS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(cx.waker().clone());
        // A change may have landed between the check and the push.
        let now = EPOCH.load(Ordering::Acquire);
        if now != seen { Poll::Ready(now) } else { Poll::Pending }
    })
    .await
}

/// A thread that turns the service's change notifications into wake-ups of
/// the components that search.
fn start_bridge() {
    static STARTED: LazyLock<()> = LazyLock::new(|| {
        let rx = service().watch();
        let spawned = std::thread::Builder::new()
            .name("bingux-search-bridge".into())
            .spawn(move || {
                while rx.recv().is_ok() {
                    wake_all();
                }
            });
        if let Err(error) = spawned {
            eprintln!("bingux-ui: search change bridge did not start: {error}");
        }
    });
    LazyLock::force(&STARTED);
}

// ------------------------------------------------------------------- the hook

/// What [`use_search`] shows right now.
#[derive(Clone, Debug, PartialEq)]
pub struct SearchState {
    /// The hits of the latest finished search, best first. While a newer
    /// search runs these stay, so a list never blinks empty between keys.
    pub hits: Vec<Hit>,
    /// How many documents matched in all (a lower bound when `total_exact` is
    /// false).
    pub total: u64,
    pub total_exact: bool,
    /// How long the engine took for these hits.
    pub took: Duration,
    /// True while a search is running or the index is still loading.
    pub loading: bool,
    /// The search these hits answer.
    pub query: String,
    /// Why the last search failed, if it did.
    pub error: Option<String>,
}

impl Default for SearchState {
    fn default() -> Self {
        SearchState {
            hits: Vec::new(),
            total: 0,
            total_exact: true,
            took: Duration::ZERO,
            loading: false,
            query: String::new(),
            error: None,
        }
    }
}

impl SearchState {
    pub fn is_empty(&self) -> bool {
        self.hits.is_empty()
    }
}

/// The search a hook last started.
struct Running {
    request: Search,
    epoch_key: u64,
    task: TaskHandle,
}

/// Searches `index` for `query` as the user types. A blank query has no hits.
///
/// Never blocks: the search runs on the worker pool and the result is written
/// to the component's state when it lands. A new query drops the previous one
/// (a queued search is skipped, a running one stops at its next check), so
/// there is no debounce. See [`use_search_with`] for filters, facets and
/// paging.
pub fn use_search(index: &str, query: &str) -> SearchState {
    let request = Search::new(query).index(index).limit(20).highlight(&[]);
    use_search_with(request, !query.trim().is_empty())
}

/// [`use_search`] for a full [`Search`] request. `active` false shows no hits
/// (nothing is searched), for a field that is empty or closed.
pub fn use_search_with(request: Search, active: bool) -> SearchState {
    let mut state = use_state(SearchState::default);
    let running = use_hook(|| Rc::new(RefCell::new(None::<Running>)));
    // Wake when an index changes (a load finished, a document was added).
    let mut epoch = use_state(|| EPOCH.load(Ordering::Acquire));
    use_hook(|| {
        start_bridge();
        spawn(async move {
            let mut seen = EPOCH.load(Ordering::Acquire);
            loop {
                seen = changed(seen).await;
                epoch.set(seen);
            }
        })
    });
    let epoch_key = *epoch.read();

    let mut slot = running.borrow_mut();
    let stale = match (&*slot, active) {
        (None, false) => false,
        (None, true) => true,
        (Some(run), true) => run.request != request || run.epoch_key != epoch_key,
        (Some(_), false) => true,
    };
    if stale {
        if let Some(previous) = slot.take() {
            previous.task.try_cancel();
        }
        if active {
            let task = spawn_search(state, request.clone());
            *slot = Some(Running {
                request,
                epoch_key,
                task,
            });
        } else {
            // Closed: show nothing, without writing during render.
            let task = spawn(async move {
                state.set(SearchState::default());
            });
            drop(task);
        }
    }
    state.read().clone()
}

fn spawn_search(mut state: State<SearchState>, request: Search) -> TaskHandle {
    spawn(async move {
        let names: Vec<String> = request.indexes.clone();
        let query = match &request.query {
            Query::Text(t) => t.text.clone(),
            _ => String::new(),
        };
        // Mark the running search without clearing what is shown.
        {
            let mut s = state.write();
            s.loading = true;
        }
        let cancel = Cancel::new();
        struct Guard(Cancel);
        impl Drop for Guard {
            fn drop(&mut self) {
                self.0.cancel();
            }
        }
        let _guard = Guard(cancel.clone());
        let svc = service().clone();
        let job_request = request.clone();
        let outcome = spawn_named("search", move |_| svc.search(&job_request, &cancel)).await;
        let loading = names.iter().any(|n| is_loading(n));
        match outcome {
            Ok(response) => state.set(SearchState {
                total: response.total,
                total_exact: response.relation == Relation::Eq,
                took: Duration::from_micros(response.took_us),
                hits: response.hits,
                loading,
                query,
                error: None,
            }),
            Err(Error::Cancelled) => {}
            Err(error) => {
                let mut s = state.write();
                s.loading = false;
                s.error = Some(error.to_string());
            }
        }
    })
}

// -------------------------------------------------------------- Spotlight side

/// What an app tells [`serve_external`].
pub struct ExternalConfig {
    /// The app id: `bingux-settings`. Names the socket and the snapshots.
    pub app_id: String,
    pub app_version: String,
}

/// The running provider of this process.
static PROVIDER: Mutex<Option<bingux_search::ipc::Provider>> = Mutex::new(None);

/// Exposes the `external` indexes of this process to Spotlight: opens the app
/// search socket, keeps the snapshots up to date and hands an activation's
/// `uri` to [`crate::app::open_uri`]. Nothing is exposed unless an index's
/// schema is `external`. Returns the socket path. See `design/search.md`
/// section 8.
pub fn serve_external(config: ExternalConfig) -> std::io::Result<std::path::PathBuf> {
    serve_external_with(config, crate::app::open_uri)
}

/// [`serve_external`] with another way of opening an activation's `uri`, for a
/// process that has no window (a headless provider, a test).
pub fn serve_external_with(
    config: ExternalConfig,
    on_activate: impl Fn(&str) -> bool + Send + Sync + 'static,
) -> std::io::Result<std::path::PathBuf> {
    let cfg = bingux_search::ipc::ProviderConfig::new(
        &config.app_id,
        &config.app_version,
        on_activate,
    );
    let provider = bingux_search::ipc::serve(service().clone(), cfg)?;
    let path = provider.socket_path().to_path_buf();
    *PROVIDER.lock().unwrap_or_else(PoisonError::into_inner) = Some(provider);
    Ok(path)
}

#[cfg(all(test, feature = "testing"))]
mod tests {
    use super::*;
    use crate::snapshot::Snapshot;

    struct Docs;

    impl Searchable for Docs {
        fn name(&self) -> &str {
            "hook-test"
        }

        fn schema(&self) -> Schema {
            Schema::new().searchable("title", 10.0)
        }

        fn documents(&self) -> Box<dyn Iterator<Item = Document> + Send + '_> {
            Box::new(
                ["Proxy", "Mouse", "Wi-Fi password"]
                    .into_iter()
                    .map(|t| Document::new(t).title(t)),
            )
        }
    }

    /// Shows the hits for a query that a click on "next" changes.
    fn app() -> Element {
        let mut text = use_state(|| "prxoy".to_string());
        let query = text.read().clone();
        let state = use_search("hook-test", &query);
        let mut column = rect()
            .child(label().text("next").on_press(move |_| text.set("mouse".to_string())))
            .child(label().text(format!(
                "state:{}:{}",
                state.hits.len(),
                if state.loading { "loading" } else { "idle" }
            )));
        for hit in &state.hits {
            column = column.child(label().text(hit.title().unwrap_or_default().to_string()));
        }
        column.into()
    }

    #[test]
    fn a_hook_finds_with_typos_off_the_ui_thread_and_follows_the_query() {
        register(Docs).expect("register");
        let mut snap = Snapshot::new(app, (300.0, 200.0));
        snap.settle(Duration::from_millis(400));
        assert!(snap.click_text("Proxy"), "a typo still finds Proxy");
        assert!(!snap.click_text("Mouse"));
        assert!(snap.click_text("state:1:idle"), "one hit, no longer loading");
        // Typing something else replaces the hits.
        assert!(snap.click_text("next"));
        snap.settle(Duration::from_millis(400));
        assert!(snap.click_text("Mouse"));
        assert!(!snap.click_text("Proxy"), "the earlier hit is gone");
        // The index changing under the hook updates it without a new query.
        index("hook-test")
            .expect("index")
            .upsert([Document::new("m2").title("Mouse pointer")])
            .expect("upsert");
        snap.settle(Duration::from_millis(400));
        assert!(snap.click_text("Mouse pointer"));
    }

    #[test]
    fn a_blank_query_has_no_hits() {
        register(Docs).expect("register");
        fn blank() -> Element {
            let state = use_search("hook-test", "   ");
            label().text(format!("hits:{}", state.hits.len())).into()
        }
        let mut snap = Snapshot::new(blank, (200.0, 100.0));
        snap.settle(Duration::from_millis(300));
        assert!(snap.click_text("hits:0"));
    }
}
