//! Search in the file views: what is asked, and how it runs.
//!
//! The question is asked in the location field
//! ([`omnibox`](super::omnibox)), which shows the answer as it comes in; the
//! backend is [`bingux_files::find`]. `design/files-search.md` says how the
//! pieces fit and what is not done; this module is the state they share.
//!
//! # What is asked
//!
//! [`Find`] holds what the scope and filter chips edit, per tab: where to
//! look ([`Scope`]) and the filters. The words are the text of the field:
//! `report "tax return" -draft`. A `key:value` typed in the field
//! (`type:image`, `size:>1MB`, the full language is in
//! [`bingux_files::find::query`]) turns into a filter, shown as a chip, the
//! moment a space ends it. [`Find::query`] is the typed [`Query`] of the two
//! together; the text and the chips never disagree, because there is one
//! place the filters live.
//!
//! # Where it runs
//!
//! Every scope is a search of folders: [`use_search_run`] runs
//! [`bingux_files::find::run`] on a thread of its own (the names index first,
//! then the live walk) and publishes what it finds every 60 ms. A changed
//! query, a changed scope, another folder or the field going away cancels it.
//!
//! # What the card does while it runs
//!
//! Results are only appended: nothing already on show moves while the search
//! runs. The previous results stay on show until the new search has its
//! first results, so typing one more letter does not blank the card.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use bingux_files::find::{
    self, Event, Filters, Invalid, NameIndex, Plan, Query, Scope, SortBy, Summary, split_pending,
};
use bingux_files::listing::Cancel;
use bingux_files::mounts::MountKind;
use bingux_files::{Entry, Filter, Listing};
use freya::prelude::*;

use super::address_target::StopOnDrop;

/// How often the UI picks up what the search found.
const POLL: Duration = Duration::from_millis(60);

/// The most results one search shows.
pub const MAX_RESULTS: usize = 10_000;

// ---------------------------------------------------------------------------
// What the person is asking
// ---------------------------------------------------------------------------

/// The search a tab is making, apart from the words. Held by whoever owns the
/// field (the chooser), so it outlives the field and every tab has its own.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Find {
    /// Where to look. It stays between searches, so a person who chooses
    /// "Everywhere" does not choose it again for the next one.
    pub scope: Scope,
    /// Filters, from the chips and from `key:value` typed in the field.
    pub filters: Filters,
}

/// `typed` laid over `base`: what the field says wins where both say
/// something; extensions, types and patterns add up.
pub fn overlay(base: &Filters, typed: Filters) -> Filters {
    let mut out = base.clone();
    for kind in typed.kinds {
        if !out.kinds.contains(&kind) {
            out.kinds.push(kind);
        }
    }
    out.kinds.sort();
    for ext in typed.exts {
        if !out.exts.contains(&ext) {
            out.exts.push(ext);
        }
    }
    out.globs.extend(typed.globs);
    out.size = typed.size.or(out.size);
    out.modified = typed.modified.or(out.modified);
    out.within = typed.within.or(out.within);
    if typed.hidden != find::Hidden::Follow {
        out.hidden = typed.hidden;
    }
    out.case_sensitive |= typed.case_sensitive;
    if typed.name_mode != find::NameMode::Contains {
        out.name_mode = typed.name_mode;
    }
    if typed.sort != SortBy::Relevance {
        out.sort = typed.sort;
    }
    out
}

impl Find {
    /// The query: the words of `text` and the filters. A `key:value` still
    /// being typed (no space after it yet) is not part of it.
    pub fn query(&self, text: &str) -> Query {
        let (committed, _) = split_pending(text);
        let typed = Query::parse(committed);
        Query {
            terms: typed.terms,
            filters: overlay(&self.filters, typed.filters),
        }
    }

    /// Turn the `key:value` tokens of `text` into filters. Done when a space
    /// ends one, or with `finished` (Enter). Returns the text the field
    /// should hold now, when it is not the text it has.
    pub fn lift(&mut self, text: &str, finished: bool) -> Option<String> {
        if !finished && !text.ends_with(char::is_whitespace) {
            return None;
        }
        let typed = Query::parse(text);
        if typed.filters == Filters::default() {
            return None;
        }
        self.filters = overlay(&self.filters, typed.filters);
        let mut words = Query {
            terms: typed.terms,
            filters: Filters::default(),
        }
        .to_string();
        if !finished && !words.is_empty() {
            words.push(' ');
        }
        (words != text).then_some(words)
    }
}

// ---------------------------------------------------------------------------
// The search, running
// ---------------------------------------------------------------------------

/// What a search of folders has found so far.
#[derive(Clone, Debug, PartialEq)]
pub struct DeepResults {
    /// The search these are the results of, as text (query and where).
    pub key: String,
    /// The matches as a listing of the folder searched, growing while
    /// `listing.complete` is false.
    pub listing: Arc<Listing>,
    pub folders: usize,
    pub running: bool,
    pub stopped: bool,
    pub truncated: bool,
    pub unreadable: usize,
    /// How many of the results the names index answered at once.
    pub indexed: usize,
    /// The query could not be run: what is wrong with it.
    pub invalid: Option<Invalid>,
    /// `in:` names a folder that is not there.
    pub missing: Option<String>,
}

/// The handle to the search of folders of a view.
#[derive(Clone)]
pub struct Deep {
    results: State<Option<Arc<DeepResults>>>,
    /// A search is in flight (the one that publishes `results` or its
    /// successor, which has not published yet).
    active: State<bool>,
    stop: Rc<RefCell<Cancel>>,
}

impl Deep {
    /// The newest results. They may be those of an earlier query while the
    /// search for the current one has not found anything yet.
    pub fn latest(&self) -> Option<Arc<DeepResults>> {
        self.results.read().clone()
    }

    /// The results of `key`, when that is the search they are of.
    pub fn results_for(&self, key: &str) -> Option<Arc<DeepResults>> {
        self.results
            .read()
            .as_ref()
            .filter(|results| results.key == key)
            .cloned()
    }

    /// Stop reading folders. What has been found stays.
    pub fn stop(&self) {
        self.stop.borrow().cancel();
    }

    /// Whether a search is reading folders. Subscribes.
    pub fn running(&self) -> bool {
        *self.active.read()
    }

    /// [`Deep::running`] without subscribing, for a key handler.
    pub fn running_now(&self) -> bool {
        *self.active.peek()
    }
}

/// Everything that makes one search different from another. The sort is not
/// among it: sorting the results does not search again.
#[derive(Clone, PartialEq)]
pub struct RunKey {
    pub roots: Vec<PathBuf>,
    pub home: PathBuf,
    pub query: Query,
    pub view_hidden: bool,
    pub include_noise: bool,
    pub filter: Option<Filter>,
    /// The folder results are shown relative to.
    pub base: PathBuf,
}

impl RunKey {
    /// The key as the text results are labelled with.
    fn label(&self) -> String {
        format!("{}|{}|{:?}", self.query, self.base.display(), self.roots)
    }
}

struct Running {
    key: RunKey,
    task: TaskHandle,
}

/// What the search's thread tells the task.
enum Message {
    Event(Event),
    Done(Summary),
}

/// Run the search for `key`, and stop it the moment `key` changes or the
/// component goes away. `None` means no search runs. Call it once per
/// component, from `render`.
pub fn use_search_run(key: Option<RunKey>, mount: MountKind) -> Deep {
    let mut results = use_state(|| None::<Arc<DeepResults>>);
    let active = use_state(|| false);
    let stop = use_hook(|| Rc::new(RefCell::new(Cancel::new())));
    let running = use_hook(|| Rc::new(RefCell::new(None::<Running>)));
    let handle = Deep {
        results,
        active,
        stop: stop.clone(),
    };
    let key = key.map(|mut key| {
        key.query.filters.sort = SortBy::Relevance;
        key
    });
    let mut running = running.borrow_mut();
    if running.as_ref().map(|run| &run.key) == key.as_ref() {
        return handle;
    }
    if let Some(previous) = running.take() {
        previous.task.try_cancel();
        stop.borrow().cancel();
    }
    let Some(key) = key else {
        spawn(async move {
            results.set_if_modified(None);
            let mut active = active;
            active.set_if_modified(false);
        });
        return handle;
    };
    let cancel = Cancel::new();
    *stop.borrow_mut() = cancel.clone();
    let task = spawn(follow_run(key.clone(), mount, cancel, results, active));
    *running = Some(Running { key, task });
    handle
}

async fn follow_run(
    key: RunKey,
    mount: MountKind,
    cancel: Cancel,
    mut results: State<Option<Arc<DeepResults>>>,
    mut active: State<bool>,
) {
    let _stop = StopOnDrop(cancel.clone());
    active.set(true);
    let label = key.label();
    let mut state = DeepResults {
        key: label,
        listing: Arc::new(Listing::new(key.base.clone())),
        folders: 0,
        running: true,
        stopped: false,
        truncated: false,
        unreadable: 0,
        indexed: 0,
        invalid: None,
        missing: None,
    };
    let mut entries: Vec<Entry> = Vec::new();
    let publish = |state: &DeepResults, entries: &[Entry], done: bool| {
        let mut listing = Listing::new(state.listing.path.clone());
        listing.entries = entries.to_vec();
        listing.complete = done;
        listing.mount = mount;
        let mut next = state.clone();
        next.listing = Arc::new(listing);
        next
    };

    let (send, receive) = mpsc::channel::<Message>();
    let plan = Plan {
        query: key.query.clone(),
        roots: key.roots.clone(),
        home: key.home.clone(),
        view_hidden: key.view_hidden,
        include_noise: key.include_noise,
        chooser_filter: key.filter.clone(),
        max_hits: MAX_RESULTS,
        index: key
            .roots
            .iter()
            .any(|root| root.starts_with(&key.home))
            .then(|| NameIndex::global(&key.home)),
    };
    let spawned = std::thread::Builder::new()
        .name("bingux-file-search".into())
        .spawn({
            let cancel = cancel.clone();
            move || {
                #[cfg(test)]
                let hold = holds_the_walk(&plan.roots);
                let summary = find::run(&plan, &cancel, &mut |event| {
                    #[cfg(test)]
                    let found = matches!(event, Event::Found(_));
                    let open = send.send(Message::Event(event)).is_ok();
                    #[cfg(test)]
                    if hold && found {
                        hold_until_stopped(&cancel);
                    }
                    open
                });
                let _ = send.send(Message::Done(summary));
            }
        });
    if spawned.is_err() {
        state.running = false;
        results.set(Some(Arc::new(state)));
        active.set(false);
        return;
    }
    // The first publication replaces the results of the search before this
    // one: until then they stay on show.
    let mut published = false;
    loop {
        timer(POLL).await;
        let mut changed = false;
        let mut finished = false;
        while let Ok(message) = receive.try_recv() {
            match message {
                Message::Event(Event::Found(found)) => {
                    entries.extend(found);
                    changed = true;
                }
                Message::Event(Event::Progress { folders }) => {
                    state.folders = folders;
                    changed = true;
                }
                Message::Done(summary) => {
                    state.folders = summary.folders.max(state.folders);
                    state.truncated = summary.truncated;
                    state.unreadable = summary.unreadable;
                    state.stopped = summary.cancelled;
                    state.indexed = summary.indexed;
                    state.invalid = summary.invalid;
                    state.missing = summary.missing;
                    state.running = false;
                    finished = true;
                    changed = true;
                }
            }
        }
        if changed && (published || !entries.is_empty() || finished) {
            published = true;
            results.set(Some(Arc::new(publish(&state, &entries, finished))));
        }
        if finished {
            active.set(false);
            return;
        }
    }
}

/// How long a held walk waits for Stop before it goes on by itself, so a test that never presses it
/// does not leave a thread behind for the rest of the run.
#[cfg(test)]
const HOLD_CAP: Duration = Duration::from_secs(60);

/// Test seam: a search of a folder whose name ends in `-slow-walk` stops after the first batch of
/// results it sends and waits there until it is stopped, so a test can press Stop while the walk is
/// certainly still running. A pause of some milliseconds per batch was not enough: under load a
/// frame takes longer than the whole walk, and the search was over before Stop was pressed.
#[cfg(test)]
fn holds_the_walk(roots: &[PathBuf]) -> bool {
    roots
        .iter()
        .any(|root| root.to_string_lossy().ends_with("-slow-walk"))
}

#[cfg(test)]
fn hold_until_stopped(cancel: &Cancel) {
    let until = std::time::Instant::now() + HOLD_CAP;
    while !cancel.is_cancelled() && std::time::Instant::now() < until {
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bingux_files::find::{Hidden, Kind};

    #[test]
    fn a_key_typed_in_the_field_becomes_a_filter_when_a_space_ends_it() {
        let mut find = Find::default();
        assert_eq!(
            find.lift("report type:image", false),
            None,
            "not closed yet"
        );
        assert_eq!(find.filters, Filters::default());
        assert_eq!(
            find.lift("report type:image ", false).as_deref(),
            Some("report ")
        );
        assert_eq!(find.filters.kinds, [Kind::Image]);
        // Words alone change nothing.
        assert_eq!(find.lift("report ", false), None);
        // Enter closes the token that is still open.
        assert_eq!(find.lift("tax size:>1MB", true).as_deref(), Some("tax"));
        assert!(find.filters.size.is_some());
        // Only filters typed: the field ends up empty.
        assert_eq!(find.lift("hidden:yes ", false).as_deref(), Some(""));
        assert_eq!(find.filters.hidden, Hidden::Include);
    }

    #[test]
    fn the_query_is_the_field_and_the_filters_together() {
        let mut find = Find::default();
        find.filters.kinds = vec![Kind::Image];
        let query = find.query("beach ext:png ");
        assert_eq!(query.plain_text(), "beach");
        assert_eq!(query.filters.kinds, [Kind::Image]);
        assert_eq!(query.filters.exts, ["png"]);
        // An open key is left out until a space closes it.
        assert_eq!(
            find.query("beach ext:pn").filters.exts,
            Vec::<String>::new()
        );
        assert_eq!(find.query("beach ext:pn").plain_text(), "beach");
        // What is typed wins over what the chips set.
        find.filters.size = Some(find::SizeCond::Gt(5));
        assert_eq!(
            find.query("size:<1k ").filters.size,
            Some(find::SizeCond::Lt(1000))
        );
        // Nothing typed and nothing chosen is a blank query: nothing is searched for.
        assert!(Find::default().query("").is_blank());
        assert!(Find::default().query("  ").is_blank());
        assert!(!Find::default().query("beach").is_blank());
        assert!(!find.query("").is_blank(), "a filter alone is a search");
    }
}
