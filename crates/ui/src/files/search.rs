//! Search in the file views: what is asked, how it runs, what the list is told.
//!
//! The search has its own bar under the toolbar ([`search_bar`](super::search_bar):
//! the field, the scope and the filters) and its own backend
//! ([`bingux_files::find`]). `design/files-search.md` says how the pieces fit
//! and what is not done; this module is the state they share.
//!
//! # What is asked
//!
//! [`Find`] holds what the bar edits, per tab: whether the bar is open, where
//! to look ([`Scope`]), and the filters. The words are the text of the field,
//! which the owner keeps (`draft`): `report "tax return" -draft`. A
//! `key:value` typed in the field (`type:image`, `size:>1MB`, the full
//! language is in [`bingux_files::find::query`]) turns into a filter, shown as
//! a token under the field, the moment a space ends it. [`Find::query`] is
//! the typed [`Query`] of the two together; the text and the Filters panel
//! never disagree, because there is one place the filters live.
//!
//! # Where it runs
//!
//! [`Scope::Folder`] filters the listing in memory, as the person types;
//! nothing is read. Every other scope is a search of folders:
//! [`use_search_run`] runs [`bingux_files::find::run`] on a thread of its own
//! (the names index first, then the live walk) and publishes what it finds
//! every 60 ms. A changed query, a changed scope, another folder, Stop or the
//! window going away cancels it.
//!
//! # What the list does while it runs
//!
//! Rows are only appended: nothing already on show moves while the search
//! runs, whatever the sort. When it ends the list is put in the chosen order
//! once. The previous results stay on show until the new search has its first
//! results, so typing one more letter does not blank the list.
//!
//! [`DeepResults::listing`] is a [`Listing`] of the folder the search started
//! in, so a file view shows results like any folder: with a Location column
//! for the folder each result is in.

use std::cell::RefCell;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use bingux_files::find::{
    self, Event, Filters, Invalid, Matcher, NameIndex, Plan, Query, Scope, SortBy, Summary, split_pending,
};
use bingux_files::listing::Cancel;
use bingux_files::mounts::MountKind;
use bingux_files::{Entry, Filter, Listing, Selection};
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
/// bar (the chooser), so the bar, the list and the status line read one state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Find {
    /// The search bar shows, and the list is narrowed by what it asks.
    pub open: bool,
    /// Where to look. It stays when the search ends, so a person who
    /// searches "Everywhere" does not choose it again for the next search.
    pub scope: Scope,
    /// Filters, from the Filters panel and from `key:value` typed in the field.
    pub filters: Filters,
    /// The Filters panel is open.
    pub filters_open: bool,
    /// Counts requests to put the keyboard in the field, so a search started
    /// from the list reaches a bar that is already open.
    pub focus: u32,
    /// The selection from before the search began, given back on Escape.
    restore: Option<Selection>,
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

    /// The query when there is something to narrow the list by, else `None`
    /// (the bar is closed, or nothing is typed and no filter is set).
    pub fn active(&self, text: &str) -> Option<Query> {
        if !self.open {
            return None;
        }
        let query = self.query(text);
        (!query.is_blank()).then_some(query)
    }

    /// Open the bar. `selection` is what Escape gives back.
    pub fn begin(&mut self, selection: &Selection) {
        if !self.open {
            self.open = true;
            self.restore = Some(selection.clone());
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

    /// End the search: the bar closes and the filters are dropped; the scope
    /// stays. Returns the selection to give back.
    pub fn clear(&mut self) -> Option<Selection> {
        let restore = self.restore.take();
        *self = Find {
            scope: std::mem::take(&mut self.scope),
            ..Find::default()
        };
        restore
    }
}

// ---------------------------------------------------------------------------
// What the list and the status line are told
// ---------------------------------------------------------------------------

/// How a search of folders is going, as the bar, the list and the status
/// line tell it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DeepStatus {
    /// The folder the search started in: the Location column is relative to it.
    pub root: PathBuf,
    pub found: usize,
    pub folders: usize,
    pub running: bool,
    /// The person pressed Stop.
    pub stopped: bool,
    /// The limit was reached.
    pub truncated: bool,
    /// Folders that could not be read.
    pub unreadable: usize,
}

/// The matcher a search marks names with.
#[derive(Clone, Debug)]
pub struct Marks {
    /// `None` when the query cannot run (a bad regular expression).
    matcher: Option<Arc<Matcher>>,
    text: String,
}

impl Marks {
    pub fn new(matcher: Option<Arc<Matcher>>, text: String) -> Marks {
        Marks { matcher, text }
    }

    /// The bytes of `name` the search matched, to draw marked.
    pub fn highlight(&self, name: &str) -> Vec<Range<usize>> {
        self.matcher.as_ref().map(|matcher| matcher.highlight(name)).unwrap_or_default()
    }

    /// The query as text: what a view's scroll position is keyed by.
    pub fn text(&self) -> &str {
        &self.text
    }
}

impl PartialEq for Marks {
    fn eq(&self, other: &Self) -> bool {
        match (&self.matcher, &other.matcher) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => self.text == other.text,
            _ => false,
        }
    }
}

/// What a file view shows of a search: the matcher to mark names with, how
/// many entries the folder has, and the results of a search of folders when
/// they are shown.
#[derive(Clone, Debug, PartialEq)]
pub struct SearchView {
    pub query: Marks,
    /// Entries of the folder on show that the search chose among (hidden
    /// ones and the ones the file type filter removes not counted).
    pub of: usize,
    /// Set while the list shows the results of a search of folders.
    pub deep: Option<DeepStatus>,
    /// Where it looks, in words: `Pictures`, `this folder`.
    pub place: String,
    /// The query cannot run: what is wrong with it.
    pub invalid: Option<String>,
    /// The order the results are in.
    pub sort: SortBy,
    /// The words searched for, without the filters.
    pub words: String,
}

/// `1204` as `1,204`.
pub(super) fn count(number: usize) -> String {
    let digits = number.to_string();
    let mut text = String::with_capacity(digits.len() + digits.len() / 3);
    for (at, digit) in digits.chars().enumerate() {
        if at > 0 && (digits.len() - at) % 3 == 0 {
            text.push(',');
        }
        text.push(digit);
    }
    text
}

/// Move the cursor of `selection` one row within `order`, for Up and Down in
/// the field.
pub fn step_selection(selection: &mut Selection, order: &[&Path], by: i32) {
    let key = if by < 0 {
        bingux_files::selection::Move::Up
    } else {
        bingux_files::selection::Move::Down
    };
    selection.step(key, order, false);
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

impl DeepResults {
    /// How the bar and the list tell it.
    pub fn status(&self) -> DeepStatus {
        DeepStatus {
            root: self.listing.path.clone(),
            found: self.listing.entries.len(),
            folders: self.folders,
            running: self.running,
            stopped: self.stopped,
            truncated: self.truncated,
            unreadable: self.unreadable,
        }
    }
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
        self.results.read().as_ref().filter(|results| results.key == key).cloned()
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
        index: key.roots.iter().any(|root| root.starts_with(&key.home)).then(|| NameIndex::global(&key.home)),
    };
    let spawned = std::thread::Builder::new().name("bingux-file-search".into()).spawn({
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
    roots.iter().any(|root| root.to_string_lossy().ends_with("-slow-walk"))
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

    fn selection_of(path: &str) -> Selection {
        let mut selection = Selection::new();
        selection.select_only(Path::new(path));
        selection
    }

    #[test]
    fn a_key_typed_in_the_field_becomes_a_filter_when_a_space_ends_it() {
        let mut find = Find::default();
        assert_eq!(find.lift("report type:image", false), None, "not closed yet");
        assert_eq!(find.filters, Filters::default());
        assert_eq!(find.lift("report type:image ", false).as_deref(), Some("report "));
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
        assert_eq!(find.query("beach ext:pn").filters.exts, Vec::<String>::new());
        assert_eq!(find.query("beach ext:pn").plain_text(), "beach");
        // What is typed wins over what the panel set.
        find.filters.size = Some(find::SizeCond::Gt(5));
        assert_eq!(find.query("size:<1k ").filters.size, Some(find::SizeCond::Lt(1000)));
        // Nothing asked: nothing narrows the list, open or not.
        assert_eq!(find.active("beach"), None, "closed");
        find.open = true;
        assert!(find.active("beach").is_some());
        assert!(find.active("").is_some(), "a filter alone is a search");
        find.filters = Filters::default();
        assert_eq!(find.active(""), None);
        assert_eq!(find.active("  "), None);
    }

    #[test]
    fn closing_the_search_gives_the_selection_back_once_and_keeps_the_scope() {
        let before = selection_of("/x/keep");
        let mut find = Find::default();
        find.begin(&before);
        find.begin(&Selection::new());
        find.scope = Scope::Home;
        find.filters.kinds = vec![Kind::Audio];
        find.filters_open = true;
        assert_eq!(find.clear(), Some(before));
        assert_eq!(find.clear(), None);
        assert_eq!(find.scope, Scope::Home);
        assert!(!find.open && !find.filters_open);
        assert_eq!(find.filters, Filters::default());
    }

    #[test]
    fn counts_have_thousands_separators() {
        assert_eq!(count(0), "0");
        assert_eq!(count(999), "999");
        assert_eq!(count(10_000), "10,000");
    }
}
