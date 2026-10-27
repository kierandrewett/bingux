//! Search in the file views: the address bar as the search box.
//!
//! The address bar ([`AddressBar`](super::AddressBar)) is one text field for
//! a path and for a search. What is typed decides which, by
//! [`bingux_files::search::classify`], with no disk access:
//!
//! | Typed | Is |
//! | ----- | -- |
//! | starts with `/`, `~`, `./`, `../`, or a scheme such as `smb://` | a path |
//! | names an entry of this folder (`Documents`, `Documents/Sub`) | a path |
//! | anything else (`report`, `2024 invoice`, `.png`) | a search |
//!
//! # Stage 1: filter this folder
//!
//! A search filters the loaded listing as each letter is typed. Nothing is
//! read from the disk: the listing is already in memory, so it costs nothing.
//! Names match without regard to case. Matches come in the order of
//! [`Tier`](bingux_files::search::Tier): the exact name, a name that starts
//! with the text, a word that starts with it, the text anywhere, every word
//! of it, and last the letters in order (`rpt` finds `my_report.txt`).
//! Inside one tier the list keeps the sort the person chose. The matched
//! letters of every name are drawn on an accent tint. The status line reads
//! "3 of 120 items match".
//!
//! | Key | Does |
//! | --- | ---- |
//! | Ctrl+F on the list, or a letter typed on it | focus the address bar as a search of this folder |
//! | the chip `in <Folder>` in the field | the scope. Its close button, or Escape, ends the search |
//! | Up, Down | move the selected match, without leaving the field |
//! | Enter | open the selected match (the first, when none is selected): a folder is entered, a file opened or chosen. With no match it starts the subfolder search |
//! | Ctrl+Enter, Shift+Enter | search the subfolders for the text now |
//! | Escape | clear the text, show the whole folder again and give back the selection the person had before searching |
//!
//! Going to another folder ends the search. Leaving the field with the
//! pointer does not: the filter and the field stay until Escape or the chip's
//! close button, so a result can be clicked.
//!
//! The file type filter of a chooser is a different thing (see
//! [`bingux_files::filter`]). It stays on during a search, so a search in a
//! chooser for pictures only finds pictures. In the folder its "hidden by
//! filter" count is reported separately; the subfolder walker applies the
//! filter itself ([`WalkOptions::filter`](bingux_files::search::WalkOptions)),
//! so rejected files neither show nor count toward its limit.
//!
//! # Stage 2: search the subfolders
//!
//! Under the field, the last row is always there while there is text:
//! **Search subfolders for "text"**. Pressing it, or Ctrl+Enter, starts
//! [`walk`](bingux_files::search::walk) on a thread of its own. Results
//! stream into the list as they are found, shallow folders first. The row
//! becomes "Searching subfolders… 12 found" with **Stop** at its end (the
//! whole row stops the search), and the status line counts the same. Changing the text, going to another folder or
//! closing the dialog stops the walk; Stop keeps what has been found.
//!
//! What the walk does and does not do:
//!
//! - It starts in the folder on show and reads one folder at a time, on one
//!   thread. Everything it finds is a name match, so folders are results too.
//! - It skips hidden folders and hidden entries unless Show hidden files is
//!   on, and never enters a symbolic link, a folder of another filesystem (a
//!   mounted drive or network share, `/proc`), or the same folder twice.
//! - By default it looks in everything the person could browse to, including
//!   `node_modules`, `target` and `.git` when hidden files are shown. The
//!   view menu's "Search build and dependency folders" switch turns that off
//!   and skips [`NOISE_FOLDERS`](bingux_files::search::NOISE_FOLDERS).
//! - It stops at 10,000 results and says so.
//! - Each result shows its name and, in a list, a Location column with its
//!   folder below the folder searched; a grid shows it on the detail card.
//!
//! The Bingux search service (`bingux-searchd`) is not used. It is a launcher
//! search over a fixed set of roots, answers over a private socket with
//! results capped at the launcher's size, cannot be limited to a folder and
//! does not say which folders it covers, so it cannot give a complete answer
//! for "this folder and below". The walker is the only source.
//!
//! File *contents* are not searched, only names.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use bingux_files::listing::Cancel;
use bingux_files::mounts::MountKind;
use bingux_files::search::{self, Query, Summary, Typed, WalkOptions, Walked};
use bingux_files::{Entry, Filter, Listing, Selection};
use freya::prelude::*;

use super::address_target::StopOnDrop;
use crate::component::ComponentName;
use crate::controls::{SpinnerSize, spinner};
use crate::icons::{self, Icon};
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{BODY, Ink};

/// How often the UI picks up what the walker found.
const POLL: Duration = Duration::from_millis(60);

// ---------------------------------------------------------------------------
// What the person is searching for
// ---------------------------------------------------------------------------

/// The search the address bar is making. Held by whoever owns the bar (the
/// chooser), so the bar, the list and the status line read one state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Find {
    /// Search mode: the chip shows and an empty field is a search, not a path.
    pub scoped: bool,
    /// The search text, trimmed. Empty means no filtering yet.
    pub query: String,
    /// The text a subfolder search was started for. It is the search shown
    /// only while it equals `query`.
    pub deep: Option<String>,
    /// Counts requests to put the keyboard in the field, so a search started
    /// from the list reaches a field that is already open.
    pub focus: u32,
    /// The selection from before the search began, given back on Escape.
    restore: Option<Selection>,
}

impl Find {
    /// The query in effect, if there is text.
    pub fn query(&self) -> Option<Query> {
        if self.scoped {
            Query::new(&self.query)
        } else {
            None
        }
    }

    /// The query of the subfolder search being shown, if there is one.
    pub fn deep_query(&self) -> Option<Query> {
        let query = self.query()?;
        (self.deep.as_deref() == Some(query.text())).then_some(query)
    }

    /// Start a search of the folder with nothing typed yet. `selection` is
    /// what Escape gives back.
    pub fn begin(&mut self, selection: &Selection) {
        if !self.scoped {
            self.scoped = true;
            self.restore = Some(selection.clone());
        }
    }

    /// The text of the field changed. `is_child` says whether a name is an
    /// entry of the folder on show. Returns the selection to give back when
    /// the text turned into a path and the search ended.
    pub fn typed(
        &mut self,
        text: &str,
        is_child: impl Fn(&str) -> bool,
        selection: &Selection,
    ) -> Option<Selection> {
        match search::classify(text, is_child) {
            Typed::Query(text) => {
                self.begin(selection);
                if self.query != text {
                    self.query = text.to_string();
                    if self.deep.as_deref() != Some(text) {
                        self.deep = None;
                    }
                }
                None
            }
            Typed::Path if text.trim().is_empty() => {
                self.query.clear();
                self.deep = None;
                None
            }
            Typed::Path => self.clear(),
        }
    }

    /// End the search. Returns the selection to give back.
    pub fn clear(&mut self) -> Option<Selection> {
        std::mem::take(self).restore
    }
}

// ---------------------------------------------------------------------------
// What the list and the status line are told
// ---------------------------------------------------------------------------

/// How a subfolder search is going, as the bar, the list and the status line
/// tell it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DeepStatus {
    /// The folder the search started in.
    pub root: PathBuf,
    pub found: usize,
    pub folders: usize,
    pub running: bool,
    /// The person pressed Stop.
    pub stopped: bool,
    /// The limit was reached.
    pub truncated: bool,
}

/// What a file view shows of a search: the query to mark in names, how many
/// entries the folder has, and the subfolder search when one is shown.
#[derive(Clone, Debug, PartialEq)]
pub struct SearchView {
    pub query: Query,
    /// Entries of the folder on show that the search chose among (hidden
    /// ones and the ones the file type filter removes not counted).
    pub of: usize,
    /// Set while the list shows a subfolder search.
    pub deep: Option<DeepStatus>,
}

// ---------------------------------------------------------------------------
// The bar's side of a search
// ---------------------------------------------------------------------------

/// Everything the address bar needs to act as a search box; see
/// [`AddressBar::search`](super::AddressBar::search).
#[derive(Clone, PartialEq)]
pub struct SearchLink {
    pub(super) find: State<Find>,
    pub(super) selection: State<Selection>,
    /// The folder on show, to tell paths from searches without the disk.
    pub(super) listing: Option<Arc<Listing>>,
    /// The folder's name, for the chip and the field's name.
    pub(super) folder: String,
    pub(super) deep: Option<DeepStatus>,
    pub(super) on_step: EventHandler<i32>,
    pub(super) on_open: EventHandler<()>,
    pub(super) on_deep: EventHandler<()>,
    pub(super) on_stop: EventHandler<()>,
    pub(super) on_clear: EventHandler<()>,
}

impl SearchLink {
    /// A link for `find`. `selection` is the list's, kept so that a search
    /// can give it back. `folder` is the name of the folder on show.
    pub fn new(find: State<Find>, selection: State<Selection>, folder: impl Into<String>) -> Self {
        SearchLink {
            find,
            selection,
            listing: None,
            folder: folder.into(),
            deep: None,
            on_step: EventHandler::new(|_: i32| {}),
            on_open: EventHandler::new(|_: ()| {}),
            on_deep: EventHandler::new(|_: ()| {}),
            on_stop: EventHandler::new(|_: ()| {}),
            on_clear: EventHandler::new(|_: ()| {}),
        }
    }

    /// The folder's entries, to tell a path from a search.
    pub fn listing(mut self, listing: Option<Arc<Listing>>) -> Self {
        self.listing = listing;
        self
    }

    /// The state of the subfolder search, for the last row.
    pub fn deep(mut self, deep: Option<DeepStatus>) -> Self {
        self.deep = deep;
        self
    }

    /// Up (-1) or Down (1) in the field: move the selected match.
    pub fn on_step(mut self, handler: impl Into<EventHandler<i32>>) -> Self {
        self.on_step = handler.into();
        self
    }

    /// Enter: open the selected match.
    pub fn on_open(mut self, handler: impl Into<EventHandler<()>>) -> Self {
        self.on_open = handler.into();
        self
    }

    /// The last row or Ctrl+Enter: search the subfolders.
    pub fn on_deep(mut self, handler: impl Into<EventHandler<()>>) -> Self {
        self.on_deep = handler.into();
        self
    }

    /// The Stop button of a running subfolder search.
    pub fn on_stop(mut self, handler: impl Into<EventHandler<()>>) -> Self {
        self.on_stop = handler.into();
        self
    }

    /// The search ended (Escape, the chip's close button): the owner shows
    /// the whole folder and gives the selection back.
    pub fn on_clear(mut self, handler: impl Into<EventHandler<()>>) -> Self {
        self.on_clear = handler.into();
        self
    }

    /// Whether `name` is an entry of the folder on show.
    pub(super) fn is_child(&self, name: &str) -> bool {
        self.listing
            .as_ref()
            .is_some_and(|listing| listing.entries.iter().any(|entry| entry.name == name))
    }

    /// The text of the chip: `in Downloads`.
    pub(super) fn chip(&self) -> String {
        format!("in {}", self.folder)
    }
}

/// Move the cursor of `selection` one row within `order`, for Up and Down in
/// the field.
pub fn step_selection(selection: &mut Selection, order: &[&std::path::Path], by: i32) {
    let key = if by < 0 {
        bingux_files::selection::Move::Up
    } else {
        bingux_files::selection::Move::Down
    };
    selection.step(key, order, false);
}

/// The row under the field: the one place to start the subfolder search, and
/// where its progress and Stop button are.
#[derive(PartialEq)]
pub(super) struct SubfolderRow {
    pub query: String,
    pub deep: Option<DeepStatus>,
    pub on_deep: EventHandler<()>,
    pub on_stop: EventHandler<()>,
    pub on_hover: EventHandler<bool>,
}

impl Component for SubfolderRow {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let on_deep = self.on_deep.clone();
        let on_enter = self.on_hover.clone();
        let on_leave = self.on_hover.clone();
        let (text, busy, offer) = match &self.deep {
            None => (format!("Search subfolders for “{}”", self.query), false, true),
            Some(deep) if deep.running => (
                format!("Searching subfolders… {} found", count(deep.found)),
                true,
                false,
            ),
            Some(deep) => {
                let mut text = format!("{} found in subfolders", count(deep.found));
                if deep.truncated {
                    text.push_str(" (limit reached)");
                } else if deep.stopped {
                    text.push_str(" (stopped)");
                }
                (text, false, false)
            }
        };
        let running = self.deep.as_ref().is_some_and(|deep| deep.running);
        let on_stop = self.on_stop.clone();
        let row = rect()
            .component("AddressSubfolderSearch")
            .a11y_role(AccessibilityRole::MenuItem)
            .a11y_alt(if running {
                format!("{text}. Press to stop.")
            } else {
                text.clone()
            })
            .tw(&tokens, MENU_ITEM)
            .tw(
                &tokens,
                &[surface(
                    Paint::Transparent,
                    (offer || running) && hovering(),
                    false,
                    false,
                )],
            )
            .on_pointer_enter(move |_| {
                hovering.set(true);
                on_enter.call(true);
            })
            .on_pointer_leave(move |_| {
                hovering.set(false);
                on_leave.call(false);
            })
            // A press never reaches the field or the shield under the row.
            .on_pointer_down(|event: Event<PointerEventData>| event.stop_propagation())
            .on_press(move |event: Event<PressEventData>| {
                event.stop_propagation();
                // The whole row is the button: it starts the search, and
                // while the search runs it stops it.
                if offer {
                    on_deep.call(());
                } else if running {
                    on_stop.call(());
                }
            });
        let lead: Element = if busy {
            spinner(SpinnerSize::Small).into()
        } else {
            icons::icon(Icon::Search, IconSize::Control, Paint::Text.color(&tokens)).into()
        };
        row.child(lead)
            .child(
                BODY.in_ink(if offer { Ink::Text } else { Ink::Muted })
                    .label(&tokens, text)
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis)
                    .width(Size::flex(1.0)),
            )
            .maybe_child(running.then(|| {
                Element::from(
                    BODY.in_ink(Ink::Accent)
                        .label(&tokens, "Stop".to_string())
                        .max_lines(1),
                )
            }))
    }
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

// ---------------------------------------------------------------------------
// The subfolder search, running
// ---------------------------------------------------------------------------

/// What a subfolder search has found so far.
#[derive(Clone, Debug, PartialEq)]
pub struct DeepResults {
    /// The text searched for.
    pub query: String,
    /// The matches as a listing of the folder searched, growing while
    /// `listing.complete` is false.
    pub listing: Arc<Listing>,
    pub folders: usize,
    pub running: bool,
    pub stopped: bool,
    pub truncated: bool,
    pub unreadable: usize,
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
        }
    }
}

/// The handle to the subfolder search of a view.
#[derive(Clone)]
pub struct Deep {
    results: State<Option<Arc<DeepResults>>>,
    stop: Rc<RefCell<Cancel>>,
}

impl Deep {
    /// The results for `query`, when that is what the search has.
    pub fn results_for(&self, query: &Query) -> Option<Arc<DeepResults>> {
        self.results
            .read()
            .as_ref()
            .filter(|results| results.query == query.text())
            .cloned()
    }

    /// Stop reading folders. What has been found stays.
    pub fn stop(&self) {
        self.stop.borrow().cancel();
    }
}

#[derive(Clone, PartialEq)]
struct Key {
    root: PathBuf,
    query: String,
    show_hidden: bool,
    include_noise: bool,
    filter: Option<Filter>,
}

struct Running {
    key: Key,
    task: TaskHandle,
}

/// What the walker's thread tells the task.
enum Message {
    Walked(Walked),
    Done(Summary),
}

/// Run the subfolder search for `query` in `root`, and stop it the moment
/// `query` changes, `root` changes or the component goes away. `None` for
/// either means no search runs. `filter` is a chooser's file type filter:
/// files it rejects are neither found nor counted. Call it once per
/// component, from `render`.
pub fn use_subfolder_search(
    root: Option<PathBuf>,
    query: Option<Query>,
    show_hidden: bool,
    include_noise: bool,
    filter: Option<Filter>,
    mount: MountKind,
) -> Deep {
    let mut results = use_state(|| None::<Arc<DeepResults>>);
    let stop = use_hook(|| Rc::new(RefCell::new(Cancel::new())));
    let running = use_hook(|| Rc::new(RefCell::new(None::<Running>)));
    let handle = Deep {
        results,
        stop: stop.clone(),
    };
    let key = root.zip(query).map(|(root, query)| Key {
        root,
        query: query.text().to_string(),
        show_hidden,
        include_noise,
        filter,
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
        spawn(async move { results.set_if_modified(None) });
        return handle;
    };
    let cancel = Cancel::new();
    *stop.borrow_mut() = cancel.clone();
    let task = spawn(follow_walk(key.clone(), mount, cancel, results));
    *running = Some(Running { key, task });
    handle
}

async fn follow_walk(
    key: Key,
    mount: MountKind,
    cancel: Cancel,
    mut results: State<Option<Arc<DeepResults>>>,
) {
    let _stop = StopOnDrop(cancel.clone());
    let Some(query) = Query::new(&key.query) else {
        return;
    };
    let mut state = DeepResults {
        query: key.query.clone(),
        listing: Arc::new(Listing::new(key.root.clone())),
        folders: 0,
        running: true,
        stopped: false,
        truncated: false,
        unreadable: 0,
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
    state = publish(&state, &entries, false);
    results.set(Some(Arc::new(state.clone())));

    let (send, receive) = mpsc::channel::<Message>();
    let options = WalkOptions {
        show_hidden: key.show_hidden,
        include_noise: key.include_noise,
        filter: key.filter.clone(),
        ..WalkOptions::default()
    };
    let spawned = std::thread::Builder::new()
        .name("bingux-subfolder-search".into())
        .spawn({
            let root = key.root.clone();
            let cancel = cancel.clone();
            move || {
                let summary = search::walk(&root, &query, &options, &cancel, &mut |walked| {
                    send.send(Message::Walked(walked)).is_ok()
                });
                let _ = send.send(Message::Done(summary));
            }
        });
    if spawned.is_err() {
        state.running = false;
        results.set(Some(Arc::new(state)));
        return;
    }
    loop {
        timer(POLL).await;
        let mut changed = false;
        let mut finished = None;
        while let Ok(message) = receive.try_recv() {
            match message {
                Message::Walked(Walked::Hits(found)) => {
                    entries.extend(found);
                    changed = true;
                }
                Message::Walked(Walked::Progress { folders }) => {
                    state.folders = folders;
                    changed = true;
                }
                Message::Done(summary) => {
                    state.folders = summary.folders;
                    state.truncated = summary.truncated;
                    state.unreadable = summary.unreadable;
                    state.stopped = summary.cancelled;
                    state.running = false;
                    finished = Some(summary);
                    changed = true;
                }
            }
        }
        if changed {
            let next = publish(&state, &entries, finished.is_some());
            results.set(Some(Arc::new(next)));
        }
        if finished.is_some() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(known: &'static [&'static str]) -> impl Fn(&str) -> bool {
        move |name| known.contains(&name)
    }

    #[test]
    fn typed_text_decides_between_a_path_and_a_search() {
        let selection = Selection::new();
        let mut find = Find::default();
        let known = names(&["Documents"]);
        assert_eq!(find.typed("rep", &known, &selection), None);
        assert!(find.scoped);
        assert_eq!(find.query, "rep");

        // The text turns into a path: the search ends.
        let mut chosen = Selection::new();
        chosen.select_only(std::path::Path::new("/x/a"));
        let mut find = Find::default();
        find.begin(&chosen);
        find.typed("rep", &known, &Selection::new());
        assert_eq!(find.typed("/home", &known, &Selection::new()), Some(chosen));
        assert_eq!(find, Find::default());

        // An entry of the folder is a path, not a search.
        let mut find = Find::default();
        assert_eq!(find.typed("Documents", &known, &selection), None);
        assert!(!find.scoped);
    }

    #[test]
    fn empty_text_keeps_the_search_open_and_drops_its_results() {
        let selection = Selection::new();
        let known = names(&[]);
        let mut find = Find::default();
        find.typed("rep", &known, &selection);
        find.deep = Some("rep".into());
        assert!(find.deep_query().is_some());
        find.typed("", &known, &selection);
        assert!(find.scoped);
        assert!(find.query().is_none());
        assert_eq!(find.deep, None);
    }

    #[test]
    fn changing_the_text_ends_the_subfolder_search() {
        let selection = Selection::new();
        let known = names(&[]);
        let mut find = Find::default();
        find.typed("rep", &known, &selection);
        find.deep = Some("rep".into());
        find.typed("repo", &known, &selection);
        assert_eq!(find.deep, None);
        assert!(find.deep_query().is_none());
        // A trailing space does not change the query, so the walk goes on.
        find.typed("repo", &known, &selection);
        find.deep = Some("repo".into());
        find.typed("repo ", &known, &selection);
        assert!(find.deep_query().is_some());
    }

    #[test]
    fn clear_gives_the_selection_back_once() {
        let mut before = Selection::new();
        before.select_only(std::path::Path::new("/x/keep"));
        let mut find = Find::default();
        find.begin(&before);
        find.begin(&Selection::new());
        find.typed("a", &names(&[]), &Selection::new());
        assert_eq!(find.clear(), Some(before));
        assert_eq!(find.clear(), None);
    }

    #[test]
    fn counts_have_thousands_separators() {
        assert_eq!(count(0), "0");
        assert_eq!(count(999), "999");
        assert_eq!(count(10_000), "10,000");
    }
}
