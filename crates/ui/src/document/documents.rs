//! The documents of a window: [`Documents`] is the manager and [`Document`]
//! the handle of one.
//!
//! What an app does and what this does:
//!
//! | The app                                          | [`Documents`] and [`Document`]                                  |
//! | ------------------------------------------------ | --------------------------------------------------------------- |
//! | Holds the content (a rope, a picture, a table)   | Holds the file: its path, name, state and what is on disk       |
//! | Turns content into bytes ([`DocumentContent::snapshot`]) | Writes them to disk atomically, off the UI thread               |
//! | Shows bytes it is handed ([`DocumentContent::load`])     | Reads the file, off the UI thread, and watches it for changes   |
//! | Calls [`Document::mark_edited`] when it changes  | Marks the title and tab, writes drafts, guards closing          |
//! | Shows the tab and the window                     | Says what the title is ([`Document::display_title`])            |
//!
//! A person's view of it:
//!
//! - **Open** goes through the Bingux file chooser ([`Documents::open_dialog`]),
//!   a recent document ([`Documents::recent_menu`]) or a path
//!   ([`Documents::open_path`]). A file that is open already is brought forward,
//!   not opened twice.
//! - **Save** writes the file; a document with no file, or one that cannot be
//!   written, is saved with **Save As**, which asks where. A save never leaves
//!   half a file (see [`write_atomic`](super::write_atomic)), and one that
//!   fails says why and offers what can work ([`dialogs::save_failed`](super::dialogs::save_failed)).
//! - **Unsaved changes** are marked by a dot on the tab and a bullet in the
//!   window title ([`UNSAVED_MARK`]). Closing a document or the window asks first.
//! - **Drafts**: a few seconds after the last edit, the unsaved content is
//!   written to `$XDG_STATE_HOME/<app-id>/drafts/`. After a crash the next
//!   start offers it back ([`dialogs::recover_work`](super::dialogs::recover_work)).
//! - **Another program changes the file** and the document has no unsaved
//!   changes: it is read again, with a note. With unsaved changes: the person
//!   chooses to keep theirs or to reload ([`dialogs::file_changed`](super::dialogs::file_changed)).
//!   A file that is deleted or moved keeps its document open and unsaved.
//! - **Recents**: every open and save is added to `recently-used.xbel`, the
//!   list Files, the file chooser and other programs share.
//! - **The scene**: which documents were open, which was showing, where each
//!   was scrolled, and a value the app keeps per document, come back on the
//!   next start ([`use_scene_state`](super::use_scene_state)).
//!
//! Threads: every function here may be called on the UI thread. Anything that
//! touches the disk runs on a worker; the results come back to the UI thread
//! in order.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use freya::prelude::*;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use super::dialogs::{
    self, Changed, Conflict, Gone, Recoverable, Recovery, Retry, Revert, Unsaved,
};
use super::drafts::{Draft, DraftMeta, Drafts};
use super::guard;
use super::io::{self, Loaded, OpenError, SaveFailure};
use super::recents::{self, Registration};
use super::scene::{SceneDocument, SceneState, use_scene_state};
use super::stamp::Stamp;
use super::watch::{FileChange, FileWatch};
use crate::files::{ChooserRequest, Filter, choose};
use crate::shell::{Dialog, MenuItem, Overlays, menu_item, use_overlays};
use crate::task::{Feed, spawn_blocking, spawn_detached};
use crate::window::{Window, WindowLike, use_window};

/// What the window title starts with while a document has unsaved changes.
/// The tab shows its own dot instead.
pub const UNSAVED_MARK: &str = "\u{2022} ";

/// Drafts older than this, whose process is gone, are deleted at start.
const DRAFT_LIFE: Duration = Duration::from_secs(30 * 24 * 60 * 60);
/// How many recent documents the menu offers.
const RECENT_MENU: usize = 10;

/// Serialises writing and removing drafts, so that a draft being written
/// cannot come back after the document it belongs to was saved or closed.
static DRAFT_LANE: Mutex<()> = Mutex::new(());

// ---- What the app provides ------------------------------------------------

/// Why [`DocumentContent::load`] is being called.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadReason {
    /// The document was opened from a file.
    Open,
    /// The file was read again: it changed on disk, or the person reverted.
    Reload,
    /// The document is a draft that a crash left behind.
    Recover,
}

/// A document turned into bytes, to be written to a file or to a draft.
///
/// It is made on the UI thread ([`DocumentContent::snapshot`]) and run on a worker,
/// so a rope that is cheap to clone is cloned there and encoded (to UTF-16,
/// to a picture format) where it blocks nothing. A document that cannot be
/// turned into bytes returns the text to tell the person: what is wrong and
/// what to change.
pub struct DocumentSnapshot(Box<dyn FnOnce() -> Result<Vec<u8>, String> + Send>);

impl DocumentSnapshot {
    /// A document that already is its bytes.
    pub fn bytes(bytes: Vec<u8>) -> DocumentSnapshot {
        DocumentSnapshot(Box::new(move || Ok(bytes)))
    }

    /// A document that is turned into bytes on a worker.
    pub fn encode(
        work: impl FnOnce() -> Result<Vec<u8>, String> + Send + 'static,
    ) -> DocumentSnapshot {
        DocumentSnapshot(Box::new(work))
    }

    fn run(self) -> Result<Vec<u8>, String> {
        (self.0)()
    }
}

/// The app's side of one document: what it shows and how it becomes bytes.
pub trait DocumentContent {
    /// The document as it is now. Called on the UI thread, for a save or for
    /// a draft; it should only copy, and leave the work to the closure it
    /// returns (see [`DocumentSnapshot`]).
    fn snapshot(&self) -> DocumentSnapshot;

    /// Show `bytes`, the file's. Called on the UI thread, when a file was
    /// opened, read again, or recovered from a draft. It replaces what was
    /// shown (a reload keeps the caret where it can), and it is **not** an
    /// edit: a [`Document::mark_edited`] made inside it is ignored.
    fn load(&self, bytes: Vec<u8>, reason: LoadReason);

    /// The document this content belongs to, once it exists. Keep it to call
    /// [`Document::mark_edited`] when the content changes.
    fn attached(&self, _document: Document) {}
}

/// Where a document came from, for the app that makes its content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// A new, empty document.
    New,
    /// A file the person opened.
    Opened,
    /// A file that was open when the app was last closed.
    Restored,
    /// Work a crash left behind.
    Recovered,
}

/// What the app is told when a document is made, to make its content.
#[derive(Clone, Debug)]
pub struct DocumentInit {
    pub id: DocumentId,
    /// The name it shows.
    pub title: String,
    /// The file, if it has one.
    pub path: Option<PathBuf>,
    /// What the app stored with the document: [`Document::set_app_value`],
    /// back from a scene or a draft. `Null` for a document that is new.
    pub app_value: Value,
    pub origin: Origin,
}

/// What a [`Documents`] tells the app about its list of documents, so that
/// the app can keep its tabs in step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DocumentEvent {
    /// A document was added: open a tab for it.
    Opened(Document),
    /// A document should be shown: it was asked for again, or a change to it
    /// needs the person.
    Reveal(Document),
    /// A document was closed: close its tab.
    Closed(DocumentId),
}

/// How a [`Documents`] is set up.
#[derive(Clone)]
pub struct DocumentsConfig {
    app_id: String,
    app_name: String,
    exec: String,
    mime: Option<String>,
    filters: Vec<Filter>,
    untitled: String,
    autosave: Duration,
    max_size: u64,
    restore: bool,
    blank: bool,
    sandbox: Option<PathBuf>,
}

impl DocumentsConfig {
    /// For the app `app_id` (its drafts and scene are kept under it), which
    /// people know as `app_name` ("Text Editor": in recents and in the
    /// questions the dialogs ask).
    pub fn new(app_id: impl Into<String>, app_name: impl Into<String>) -> Self {
        let app_id = app_id.into();
        Self {
            exec: app_id.clone(),
            app_id,
            app_name: app_name.into(),
            mime: None,
            filters: Vec::new(),
            untitled: "Untitled".to_string(),
            autosave: Duration::from_secs(3),
            max_size: 512 * 1024 * 1024,
            restore: true,
            blank: true,
            sandbox: None,
        }
    }

    /// The command that opens a file, for `recently-used.xbel`. The app id by
    /// default.
    pub fn exec(mut self, exec: impl Into<String>) -> Self {
        self.exec = exec.into();
        self
    }

    /// The MIME type of what the app writes, for `recently-used.xbel`.
    pub fn mime(mut self, mime: impl Into<String>) -> Self {
        self.mime = Some(mime.into());
        self
    }

    /// A kind of file the Open dialog offers. Add the most common first; "All
    /// files" is added after the last.
    pub fn filter(mut self, filter: Filter) -> Self {
        self.filters.push(filter);
        self
    }

    /// The name a new document has ("Untitled": the second is "Untitled 2").
    pub fn untitled(mut self, name: impl Into<String>) -> Self {
        self.untitled = name.into();
        self
    }

    /// How long after the last edit the unsaved content is written to a
    /// draft. Three seconds by default.
    pub fn autosave_after(mut self, after: Duration) -> Self {
        self.autosave = after;
        self
    }

    /// The largest file the app opens, in bytes. Half a gigabyte by default.
    pub fn max_size(mut self, bytes: u64) -> Self {
        self.max_size = bytes;
        self
    }

    /// Whether the documents that were open come back at start, and whether
    /// work a crash left is offered. On by default.
    pub fn restore(mut self, restore: bool) -> Self {
        self.restore = restore;
        self
    }

    /// Whether a new, empty document is made when nothing else is open after
    /// the start. On by default; an app that shows an empty state does not.
    pub fn blank_when_empty(mut self, blank: bool) -> Self {
        self.blank = blank;
        self
    }

    /// Keeps the drafts and the recent documents under `dir` instead of in the
    /// person's own state and data folders: for a test, and for a
    /// documentation capture, which must not read or change them.
    pub fn sandboxed(mut self, dir: impl Into<PathBuf>) -> Self {
        self.sandbox = Some(dir.into());
        self
    }
}

// ---- Ids and state --------------------------------------------------------

/// A document's id, unique in the process. Never reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DocumentId(u64);

impl DocumentId {
    /// The number, for an app that keeps ids in its own tab model.
    pub fn get(self) -> u64 {
        self.0
    }
}

/// Whether a document has its content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// The file is being read. The document shows nothing yet and cannot be
    /// edited.
    Loading,
    Ready,
}

struct Entry {
    id: DocumentId,
    path: Option<PathBuf>,
    /// The number in "Untitled 2"; 0 once the document has a file.
    untitled: u32,
    phase: Phase,
    edited: bool,
    saving: bool,
    read_only: bool,
    external: Option<FileChange>,
    content: Rc<dyn DocumentContent>,
    // What follows changes without anyone redrawing, so it is in cells.
    /// What the file was when the document read or wrote it last.
    stamp: Cell<Option<Stamp>>,
    watch: RefCell<Option<FileWatch>>,
    /// Counts every edit.
    revision: Cell<u64>,
    /// The revision whose draft is on disk.
    drafted: Cell<Option<u64>>,
    draft: String,
    /// Changes whenever the draft is deleted, so that a draft being written
    /// at that moment is not left behind.
    epoch: Arc<AtomicU64>,
    app_value: RefCell<Value>,
    scroll: Cell<(f32, f32)>,
}

type Build = Box<dyn FnOnce(Documents) -> Option<Dialog>>;

struct Queued {
    doc: Option<DocumentId>,
    build: Build,
}

struct Hooks {
    make: Box<dyn Fn(&DocumentInit) -> Rc<dyn DocumentContent>>,
    on_event: Box<dyn Fn(DocumentEvent)>,
}

struct Store {
    settings: DocumentsConfig,
    hooks: Rc<Hooks>,
    drafts: Drafts,
    scene: SceneState,
    feed: Arc<Feed<(DocumentId, FileChange)>>,
    /// The window's close button and Alt+F4, through the close guard.
    close_requests: Arc<Feed<()>>,
    window_id: freya::winit::window::WindowId,
    entries: Vec<Entry>,
    recents: Vec<PathBuf>,
    recents_file: Option<PathBuf>,
    current: Option<DocumentId>,
    next_id: Cell<u64>,
    queue: RefCell<VecDeque<Queued>>,
    pumping: Cell<bool>,
    draft_armed: Cell<bool>,
    scene_armed: Cell<bool>,
    /// A close of the window is being decided.
    closing: Cell<bool>,
    /// Inside `DocumentContent::load`: edits made there are not edits.
    loading: Cell<u32>,
}

impl Store {
    fn entry(&self, id: DocumentId) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    fn entry_mut(&mut self, id: DocumentId) -> Option<&mut Entry> {
        self.entries.iter_mut().find(|entry| entry.id == id)
    }

    fn next_id(&self) -> DocumentId {
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        DocumentId(id)
    }

    /// The smallest "Untitled n" number no open document has.
    fn free_untitled(&self) -> u32 {
        (1..)
            .find(|n| !self.entries.iter().any(|entry| entry.untitled == *n))
            .unwrap_or(1)
    }

    fn title_of(&self, entry: &Entry) -> String {
        match &entry.path {
            Some(path) => display_name(path),
            None if entry.untitled <= 1 => self.settings.untitled.clone(),
            None => format!("{} {}", self.settings.untitled, entry.untitled),
        }
    }
}

/// The name of a file as the person reads it.
fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// What to do when a flow is over: after the save that a close waits for,
/// after the last of several saves.
#[derive(Clone)]
struct After {
    done: Rc<dyn Fn()>,
    stopped: Rc<dyn Fn()>,
}

impl After {
    fn nothing() -> After {
        After {
            done: Rc::new(|| {}),
            stopped: Rc::new(|| {}),
        }
    }

    fn then(done: impl Fn() + 'static) -> After {
        After {
            done: Rc::new(done),
            stopped: Rc::new(|| {}),
        }
    }
}

// ---- The manager ----------------------------------------------------------

/// The documents of a window. Copy it into closures.
///
/// Make it once, in the window's root, with [`use_documents`].
#[derive(Clone, Copy, PartialEq)]
pub struct Documents {
    store: State<Store>,
    overlays: Overlays,
    window: Window,
}

/// One document. Copy it into closures. A handle to a document that was
/// closed reads as an empty document and does nothing.
#[derive(Clone, Copy, PartialEq)]
pub struct Document {
    docs: Documents,
    id: DocumentId,
}

impl std::fmt::Debug for Document {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Document({})", self.id.0)
    }
}

/// Makes the [`Documents`] of this window. Call it in the root, inside the
/// [`OverlayHost`](crate::shell::OverlayHost), once.
///
/// `make` is called for each document, to make its [`DocumentContent`]; `on_event`
/// is told when the list changes, to keep tabs in step.
///
/// It also: registers the close guard of the window (see [`Documents::close_window`]),
/// offers work a crash left and brings back the documents of the last run
/// (see [`DocumentsConfig::restore`]), checks the files again when the window
/// is focused, and keeps the scene ([`use_scene_state`]).
pub fn use_documents(
    config: DocumentsConfig,
    make: impl Fn(&DocumentInit) -> Rc<dyn DocumentContent> + 'static,
    on_event: impl Fn(DocumentEvent) + 'static,
) -> Documents {
    let overlays = use_overlays();
    let window = use_window();
    let scene = use_scene_state();
    let window_id = Platform::window_id();
    let store = use_state(move || Store {
        drafts: match &config.sandbox {
            Some(dir) => Drafts::in_dir(dir.join("drafts")),
            None => Drafts::open(&config.app_id),
        },
        recents_file: config
            .sandbox
            .as_ref()
            .map(|dir| dir.join("recently-used.xbel")),
        settings: config,
        hooks: Rc::new(Hooks {
            make: Box::new(make),
            on_event: Box::new(on_event),
        }),
        scene,
        window_id,
        feed: Arc::new(Feed::default()),
        close_requests: Arc::new(Feed::default()),
        entries: Vec::new(),
        recents: Vec::new(),
        current: None,
        next_id: Cell::new(1),
        queue: RefCell::new(VecDeque::new()),
        pumping: Cell::new(false),
        draft_armed: Cell::new(false),
        scene_armed: Cell::new(false),
        closing: Cell::new(false),
        loading: Cell::new(0),
    });
    let documents = Documents {
        store,
        overlays,
        window,
    };
    use_hook(move || {
        let (changes, requests) =
            documents.with(|store| (store.feed.clone(), store.close_requests.clone()));
        let asked = requests.clone();
        guard::register(
            u64::from(window_id),
            Rc::new(move || {
                asked.push(());
                true
            }),
        );
        // The tasks end with the window.
        spawn_in_window(async move {
            while let Some((id, change)) = changes.next().await {
                documents.external_change(id, change, None);
            }
        });
        spawn_in_window(async move {
            while requests.next().await.is_some() {
                documents.close_window();
            }
        });
        spawn_in_window(documents.start());
    });
    use_drop(move || guard::unregister(u64::from(window_id)));
    // A file that changed while the window was not looking is noticed when it
    // is looked at again: inotify does not reach network shares.
    let focus = crate::runtime::window_focus();
    use_side_effect(move || {
        if *focus.read() {
            documents.check_changes();
        }
    });
    documents
}

/// Keeps the window's title in step with the document that is showing:
/// `name – App`, with [`UNSAVED_MARK`] before the name while it has unsaved
/// changes. A window with no document is titled `app_name`.
pub fn use_document_title(documents: Documents, app_name: &str) {
    let title = documents
        .current()
        .map(|document| document.window_title(app_name))
        .unwrap_or_else(|| app_name.to_string());
    let window = documents.window;
    use_side_effect_with_deps(&title, move |title| window.set_title(title.clone()));
}

impl Documents {
    fn with<R>(self, read: impl FnOnce(&Store) -> R) -> R {
        let store = self.store.peek();
        read(&store)
    }

    /// Like `with`, and the caller is redrawn when the store changes.
    fn watching<R>(self, read: impl FnOnce(&Store) -> R) -> R {
        let store = self.store.read();
        read(&store)
    }

    fn edit<R>(self, change: impl FnOnce(&mut Store) -> R) -> R {
        let mut state = self.store;
        let mut store = state.write();
        change(&mut store)
    }

    fn hooks(self) -> Rc<Hooks> {
        self.with(|store| store.hooks.clone())
    }

    fn emit(self, event: DocumentEvent) {
        (self.hooks().on_event)(event);
    }

    fn handle(self, id: DocumentId) -> Document {
        Document { docs: self, id }
    }

    // ---- The list ---------------------------------------------------------

    /// The open documents, in the order they were opened.
    pub fn list(self) -> Vec<Document> {
        self.watching(|store| {
            store
                .entries
                .iter()
                .map(|entry| self.handle(entry.id))
                .collect()
        })
    }

    /// The document with `id`, if it is still open.
    pub fn get(self, id: DocumentId) -> Option<Document> {
        self.with(|store| store.entry(id).map(|_| self.handle(id)))
    }

    /// The document that is showing, as far as the app has said
    /// ([`set_current`](Self::set_current)).
    pub fn current(self) -> Option<Document> {
        self.watching(|store| {
            store
                .current
                .filter(|id| store.entry(*id).is_some())
                .map(|id| self.handle(id))
        })
    }

    /// Says which document is showing: the title of the window follows it, and
    /// the scene brings it back. Call it when the app's tab changes.
    pub fn set_current(self, document: Option<Document>) {
        let id = document.map(|document| document.id);
        if self.with(|store| store.current) == id {
            return;
        }
        self.edit(|store| store.current = id);
        self.publish_scene();
    }

    /// The documents with unsaved changes.
    pub fn unsaved(self) -> Vec<Document> {
        self.watching(|store| {
            store
                .entries
                .iter()
                .filter(|entry| entry.edited)
                .map(|entry| self.handle(entry.id))
                .collect()
        })
    }

    /// Whether any document has unsaved changes.
    pub fn has_unsaved(self) -> bool {
        self.watching(|store| store.entries.iter().any(|entry| entry.edited))
    }

    // ---- Making documents -------------------------------------------------

    /// A new, empty document ("Untitled", then "Untitled 2" ...).
    pub fn new_document(self) -> Document {
        let init = self.with(|store| DocumentInit {
            id: store.next_id(),
            title: String::new(),
            path: None,
            app_value: Value::Null,
            origin: Origin::New,
        });
        let number = self.with(Store::free_untitled);
        self.add(init, Drafts::new_id(), move |entry| {
            entry.untitled = number;
            entry.phase = Phase::Ready;
        })
    }

    /// Makes the entry, then its content, tells the app, and returns the handle.
    fn add(
        self,
        mut init: DocumentInit,
        draft: String,
        setup: impl FnOnce(&mut Entry),
    ) -> Document {
        let hooks = self.hooks();
        let id = init.id;
        // The entry first: the app's `make` may ask the manager about it.
        let mut entry = Entry {
            id,
            path: init.path.clone(),
            untitled: 0,
            phase: Phase::Loading,
            edited: false,
            saving: false,
            read_only: false,
            external: None,
            content: Rc::new(Placeholder),
            stamp: Cell::new(None),
            watch: RefCell::new(None),
            revision: Cell::new(0),
            drafted: Cell::new(None),
            draft,
            epoch: Arc::new(AtomicU64::new(0)),
            app_value: RefCell::new(init.app_value.clone()),
            scroll: Cell::new((0.0, 0.0)),
        };
        setup(&mut entry);
        self.edit(|store| store.entries.push(entry));
        init.title = self.handle(id).title();
        let content = (hooks.make)(&init);
        self.edit(|store| {
            if let Some(entry) = store.entry_mut(id) {
                entry.content = content.clone();
            }
        });
        let document = self.handle(id);
        content.attached(document);
        (hooks.on_event)(DocumentEvent::Opened(document));
        self.publish_scene();
        document
    }

    /// Opens the file at `path`. A file that is open already is revealed and
    /// returned; no second document is made. A file that cannot be opened is
    /// reported to the person, and the returned document is closed at once.
    pub fn open_path(self, path: impl Into<PathBuf>) -> Document {
        self.open_with(Plan::new(path.into(), Origin::Opened))
    }

    /// Like [`open_path`](Self::open_path), and a file that is not there yet
    /// is a new document that will be saved there: what a command line that
    /// names a file that does not exist yet means.
    pub fn open_or_create(self, path: impl Into<PathBuf>) -> Document {
        let mut plan = Plan::new(path.into(), Origin::Opened);
        plan.create = true;
        self.open_with(plan)
    }

    fn open_with(self, plan: Plan) -> Document {
        let path = std::path::absolute(&plan.path).unwrap_or_else(|_| plan.path.clone());
        let existing = self.with(|store| {
            store
                .entries
                .iter()
                .find(|entry| entry.path.as_deref() == Some(path.as_path()))
                .map(|entry| entry.id)
        });
        if let Some(id) = existing {
            let document = self.handle(id);
            self.emit(DocumentEvent::Reveal(document));
            return document;
        }
        let init = self.with(|store| DocumentInit {
            id: store.next_id(),
            title: String::new(),
            path: Some(path.clone()),
            app_value: plan.app_value.clone(),
            origin: plan.origin,
        });
        let id = init.id;
        let scroll = plan.scroll;
        let document = self.add(init, Drafts::new_id(), move |entry| {
            entry.scroll.set(scroll);
        });
        let limit = self.with(|store| store.settings.max_size);
        let docs = self;
        spawn_in_window(async move {
            let reading = path.clone();
            let result = spawn_blocking(move || io::read(&reading, limit)).await;
            docs.finish_open(id, path, result, plan);
        });
        document
    }

    fn finish_open(
        self,
        id: DocumentId,
        path: PathBuf,
        result: Result<Loaded, OpenError>,
        plan: Plan,
    ) {
        match result {
            Ok(loaded) => self.apply_loaded(id, loaded, LoadReason::Open),
            Err(OpenError::Missing) if plan.create => {
                let writable = io::is_writable(&path);
                self.edit(|store| {
                    if let Some(entry) = store.entry_mut(id) {
                        entry.phase = Phase::Ready;
                        entry.read_only = !writable;
                    }
                });
                self.start_watch(id);
            }
            Err(error) => {
                let name = display_name(&path);
                self.remove_entry(id);
                if error == OpenError::Missing {
                    self.recents_job(move |list_path, _| {
                        let _ = recents::forget_at(&list_path, &path);
                    });
                }
                // A file that was open at the last run and is gone is not
                // news worth a dialog: the person deleted it.
                if plan.origin != Origin::Restored {
                    self.ask(None, false, move |_| {
                        Some(dialogs::open_failed(&name, &error))
                    });
                }
            }
        }
    }

    /// Shows the file chooser, and opens what is chosen.
    pub fn open_dialog(self) {
        let (filters, folder) = self.with(|store| {
            let folder = store
                .current
                .and_then(|id| store.entry(id))
                .and_then(|entry| entry.path.as_deref())
                .and_then(Path::parent)
                .map(Path::to_path_buf);
            (store.settings.filters.clone(), folder)
        });
        let mut request = ChooserRequest::open().title("Open").multiple(true);
        let any = !filters.is_empty();
        for filter in filters {
            request = request.filter(filter);
        }
        if any {
            request = request.filter(Filter::all_files());
        }
        if let Some(folder) = folder {
            request = request.current_folder(folder);
        }
        let docs = self;
        choose(self.window, request, move |chosen| {
            for path in chosen.into_iter().flat_map(|chosen| chosen.paths) {
                docs.open_path(path);
            }
        });
    }

    /// The items of an "Open Recent" menu: the documents this app opened or
    /// saved, newest first, as the shared `recently-used.xbel` has them.
    pub fn recent_menu(self) -> Vec<MenuItem> {
        let recent = self.watching(|store| store.recents.clone());
        if recent.is_empty() {
            return vec![menu_item("No recent documents", |_| {}).disabled()];
        }
        recent
            .into_iter()
            .map(|path| {
                let label = display_name(&path);
                let docs = self;
                menu_item(label, move |_| {
                    docs.open_path(path.clone());
                })
                .name()
            })
            .collect()
    }

    // ---- What the disk says -----------------------------------------------

    fn apply_loaded(self, id: DocumentId, loaded: Loaded, reason: LoadReason) {
        let content = self.edit(|store| {
            let entry = store.entry_mut(id)?;
            entry.stamp.set(Some(loaded.stamp));
            entry.read_only = !loaded.writable;
            entry.phase = Phase::Ready;
            entry.edited = false;
            entry.external = None;
            entry.revision.set(entry.revision.get() + 1);
            Some(entry.content.clone())
        });
        let Some(content) = content else { return };
        // A reload has replaced whatever the draft held.
        self.discard_draft(id);
        self.with(|store| store.loading.set(store.loading.get() + 1));
        content.load(loaded.bytes, reason);
        self.with(|store| store.loading.set(store.loading.get() - 1));
        match reason {
            LoadReason::Open => {
                self.start_watch(id);
                self.note_recent(id);
            }
            _ => {
                let known = loaded.stamp;
                self.with(|store| {
                    if let Some(entry) = store.entry(id)
                        && let Some(watch) = entry.watch.borrow().as_ref()
                    {
                        watch.acknowledge(Some(known));
                    }
                });
            }
        }
        self.publish_scene();
    }

    /// Starts watching the file of document `id` for changes made by others.
    fn start_watch(self, id: DocumentId) {
        let started = self.with(|store| {
            let entry = store.entry(id)?;
            Some((entry.path.clone()?, entry.stamp.get(), store.feed.clone()))
        });
        let Some((path, known, feed)) = started else {
            return;
        };
        let docs = self;
        spawn_in_window(async move {
            let made = spawn_blocking(move || {
                FileWatch::new(&path, known, move |change| feed.push((id, change)))
            })
            .await;
            // No watch (the limit of inotify watches was reached): the file is
            // still compared when the window is focused.
            if let Ok(watch) = made {
                docs.with(|store| {
                    if let Some(entry) = store.entry(id) {
                        *entry.watch.borrow_mut() = Some(watch);
                    }
                });
            }
        });
    }

    fn note_recent(self, id: DocumentId) {
        let (path, settings) = self.with(|store| {
            (
                store.entry(id).and_then(|entry| entry.path.clone()),
                store.settings.clone(),
            )
        });
        let Some(path) = path else { return };
        self.recents_job(move |list_path, _| {
            let _ = recents::register_at(
                &list_path,
                &Registration {
                    path: &path,
                    mime: settings.mime.as_deref(),
                    app_name: &settings.app_name,
                    exec: &settings.exec,
                },
                crate::format::Moment::now(),
            );
        });
    }

    fn refresh_recents(self) {
        self.recents_job(|_, _| {});
    }

    /// Runs `change` on the recents list on a worker, then reads the list
    /// this app has in it for the menu.
    fn recents_job(self, change: impl FnOnce(PathBuf, &str) + Send + 'static) {
        let (file, name) = self.with(|store| {
            (
                store
                    .recents_file
                    .clone()
                    .unwrap_or_else(recents::default_path),
                store.settings.app_name.clone(),
            )
        });
        let docs = self;
        spawn_in_window(async move {
            let list = spawn_blocking(move || {
                change(file.clone(), &name);
                recents::list_of_app_at(&file, &name, RECENT_MENU)
            })
            .await;
            if docs.with(|store| store.recents != list) {
                docs.edit(|store| store.recents = list);
            }
        });
    }

    /// Compares every open file with what the document knows, and acts on a
    /// difference as if the watch had reported it. Called when the window is
    /// focused again; an app may call it too.
    pub fn check_changes(self) {
        let jobs: Vec<(DocumentId, PathBuf, Option<Stamp>)> = self.with(|store| {
            store
                .entries
                .iter()
                .filter(|entry| {
                    entry.phase == Phase::Ready && !entry.saving && entry.external.is_none()
                })
                .filter_map(|entry| Some((entry.id, entry.path.clone()?, entry.stamp.get())))
                .collect()
        });
        if jobs.is_empty() {
            return;
        }
        let docs = self;
        spawn_in_window(async move {
            let found = spawn_blocking(move || {
                jobs.into_iter()
                    .map(|(id, path, known)| (id, known, Stamp::now(&path)))
                    .collect::<Vec<_>>()
            })
            .await;
            for (id, known, now) in found {
                if known != now {
                    let change = if now.is_some() {
                        FileChange::Changed
                    } else {
                        FileChange::Removed
                    };
                    docs.external_change(id, change, Some(now));
                }
            }
        });
    }

    /// A file was changed by someone else. `now` is what it is, when the
    /// caller looked; `None` asks the document's watch.
    fn external_change(self, id: DocumentId, change: FileChange, now: Option<Option<Stamp>>) {
        let known = self.with(|store| {
            let entry = store.entry(id)?;
            if entry.phase != Phase::Ready || entry.saving || entry.external.is_some() {
                return None;
            }
            let seen = match now {
                Some(now) => now,
                None => entry.watch.borrow().as_ref().and_then(FileWatch::known),
            };
            Some((entry.edited, seen))
        });
        let Some((edited, seen)) = known else { return };
        let document = self.handle(id);
        match change {
            FileChange::Changed if !edited => self.reload(id, true),
            FileChange::Changed => {
                self.edit(|store| {
                    if let Some(entry) = store.entry_mut(id) {
                        entry.external = Some(FileChange::Changed);
                    }
                });
                self.emit(DocumentEvent::Reveal(document));
                self.ask(Some(id), false, move |docs| {
                    let name = docs.handle(id).title();
                    Some(dialogs::file_changed(&name, move |answer| match answer {
                        Changed::Keep => docs.keep_version(id, seen),
                        Changed::Reload => docs.reload(id, false),
                    }))
                });
            }
            FileChange::Removed => {
                self.edit(|store| {
                    if let Some(entry) = store.entry_mut(id) {
                        entry.external = Some(FileChange::Removed);
                        // Its only copy is the one open: closing it without
                        // saving loses it, so it is an unsaved document now.
                        entry.edited = true;
                        entry.stamp.set(None);
                        entry.revision.set(entry.revision.get() + 1);
                    }
                });
                self.schedule_draft();
                self.emit(DocumentEvent::Reveal(document));
                self.ask(Some(id), false, move |docs| {
                    let name = docs.handle(id).title();
                    Some(dialogs::file_removed(&name, move |answer| {
                        docs.edit(|store| {
                            if let Some(entry) = store.entry_mut(id) {
                                entry.external = None;
                            }
                        });
                        if answer == Gone::SaveAs {
                            docs.save_as(id, After::nothing());
                        }
                    }))
                });
            }
        }
    }

    /// The person kept their version of a file that changed: what is on disk
    /// now is the file the document's next save replaces, without asking again.
    fn keep_version(self, id: DocumentId, seen: Option<Stamp>) {
        self.edit(|store| {
            if let Some(entry) = store.entry_mut(id) {
                entry.external = None;
                entry.stamp.set(seen);
            }
        });
    }

    /// Reads the file of document `id` again. `quiet`: nobody asked, so the
    /// person is told after the fact.
    fn reload(self, id: DocumentId, quiet: bool) {
        let Some((path, limit)) =
            self.with(|store| Some((store.entry(id)?.path.clone()?, store.settings.max_size)))
        else {
            return;
        };
        let docs = self;
        spawn_in_window(async move {
            let reading = path.clone();
            let result = spawn_blocking(move || io::read(&reading, limit)).await;
            match result {
                Ok(loaded) => {
                    docs.apply_loaded(id, loaded, LoadReason::Reload);
                    if quiet {
                        let name = display_name(&path);
                        docs.overlays.toast(format!(
                            "“{name}” was reloaded after another program changed it."
                        ));
                    }
                }
                Err(OpenError::Missing) => {
                    docs.external_change(id, FileChange::Removed, Some(None))
                }
                // A file in the middle of being written reads again at its
                // next event; a dialog now would be about nothing.
                Err(_) if quiet => {}
                Err(error) => {
                    let name = display_name(&path);
                    docs.ask(Some(id), false, move |_| {
                        Some(dialogs::open_failed(&name, &error))
                    });
                }
            }
        });
    }

    // ---- Saving -----------------------------------------------------------

    /// Saves every document that has unsaved changes, one after the other. A
    /// document with no file asks where to save it.
    pub fn save_all(self) {
        let ids: Vec<DocumentId> = self
            .unsaved()
            .into_iter()
            .map(|document| document.id)
            .collect();
        self.save_each(ids, After::nothing());
    }

    fn save_each(self, mut ids: Vec<DocumentId>, after: After) {
        if ids.is_empty() {
            (after.done)();
            return;
        }
        let first = ids.remove(0);
        let docs = self;
        let rest = After {
            done: {
                let after = after.clone();
                Rc::new(move || docs.save_each(ids.clone(), after.clone()))
            },
            stopped: after.stopped.clone(),
        };
        self.begin_save(first, None, rest, false);
    }

    /// Saves document `id` to `target` (Save As) or to its own file.
    /// `replace`: write over a file that changed since it was read.
    fn begin_save(self, id: DocumentId, target: Option<PathBuf>, after: After, replace: bool) {
        enum Prep {
            Gone,
            Loading,
            Saving,
            Ready(
                Option<(PathBuf, bool)>,
                Rc<dyn DocumentContent>,
                Option<Stamp>,
                u64,
            ),
        }
        let prep = self.with(|store| {
            let Some(entry) = store.entry(id) else {
                return Prep::Gone;
            };
            if entry.phase == Phase::Loading {
                return Prep::Loading;
            }
            if entry.saving {
                return Prep::Saving;
            }
            let path = match (&target, &entry.path, entry.read_only) {
                (Some(target), ..) => Some((target.clone(), true)),
                (None, Some(path), false) => Some((path.clone(), false)),
                _ => None,
            };
            Prep::Ready(
                path,
                entry.content.clone(),
                entry.stamp.get(),
                entry.revision.get(),
            )
        });
        let (path, content, stamp, revision) = match prep {
            Prep::Gone | Prep::Loading => {
                (after.stopped)();
                return;
            }
            // A save is running: this one follows it, so that it writes what
            // the document is by then.
            Prep::Saving => {
                let docs = self;
                spawn_in_window(async move {
                    while docs.handle(id).is_saving() {
                        timer(Duration::from_millis(100)).await;
                    }
                    docs.begin_save(id, target, after, replace);
                });
                return;
            }
            Prep::Ready(path, content, stamp, revision) => (path, content, stamp, revision),
        };
        let Some((path, is_save_as)) = path else {
            self.save_as(id, after);
            return;
        };
        if is_save_as && self.is_open_elsewhere(id, &path) {
            let name = display_name(&path);
            self.ask(Some(id), true, move |_| Some(dialogs::already_open(&name)));
            (after.stopped)();
            return;
        }
        let expected = if is_save_as || replace { None } else { stamp };
        let snapshot = content.snapshot();
        self.edit(|store| {
            if let Some(entry) = store.entry_mut(id) {
                entry.saving = true;
            }
        });
        let docs = self;
        spawn_in_window(async move {
            let writing = path.clone();
            let result = spawn_blocking(move || {
                let bytes = snapshot.run().map_err(SaveFailure::Encode)?;
                io::save(&writing, &bytes, expected)
            })
            .await;
            docs.finish_save(id, path, is_save_as, revision, result, after);
        });
    }

    fn is_open_elsewhere(self, id: DocumentId, path: &Path) -> bool {
        self.with(|store| {
            store
                .entries
                .iter()
                .any(|entry| entry.id != id && entry.path.as_deref() == Some(path))
        })
    }

    fn finish_save(
        self,
        id: DocumentId,
        path: PathBuf,
        is_save_as: bool,
        revision: u64,
        result: Result<Stamp, SaveFailure>,
        after: After,
    ) {
        let alive = self.edit(|store| {
            let entry = store.entry_mut(id)?;
            entry.saving = false;
            Some(())
        });
        if alive.is_none() {
            return;
        }
        let name = display_name(&path);
        match result {
            Ok(stamp) => {
                self.edit(|store| {
                    if let Some(entry) = store.entry_mut(id) {
                        if is_save_as {
                            entry.path = Some(path.clone());
                            entry.untitled = 0;
                            entry.read_only = false;
                        }
                        entry.stamp.set(Some(stamp));
                        entry.external = None;
                        // Edits made while the bytes were being written are
                        // not in the file: the document is still unsaved.
                        if entry.revision.get() == revision {
                            entry.edited = false;
                        }
                    }
                });
                let still_edited =
                    self.with(|store| store.entry(id).is_some_and(|entry| entry.edited));
                if still_edited {
                    self.schedule_draft();
                } else {
                    self.discard_draft(id);
                }
                if is_save_as {
                    self.with(|store| {
                        if let Some(entry) = store.entry(id) {
                            entry.watch.take();
                        }
                    });
                    self.start_watch(id);
                } else {
                    self.with(|store| {
                        if let Some(entry) = store.entry(id)
                            && let Some(watch) = entry.watch.borrow().as_ref()
                        {
                            watch.acknowledge(Some(stamp));
                        }
                    });
                }
                self.note_recent(id);
                self.publish_scene();
                (after.done)();
            }
            Err(SaveFailure::Changed(_)) => {
                let docs = self;
                self.ask(Some(id), true, move |_| {
                    let (again, other, away) = (after.clone(), after.clone(), after);
                    Some(dialogs::save_conflict(&name, move |answer| match answer {
                        Conflict::Replace => docs.begin_save(id, None, again.clone(), true),
                        Conflict::SaveAs => docs.save_as(id, other.clone()),
                        Conflict::Cancel => (away.stopped)(),
                    }))
                });
            }
            Err(SaveFailure::Disk(error)) => {
                let docs = self;
                self.ask(Some(id), true, move |_| {
                    let (again, away) = (after.clone(), after.clone());
                    let target = is_save_as.then(|| path.clone());
                    Some(dialogs::save_failed(
                        &name,
                        &error,
                        move |answer| match answer {
                            Retry::TryAgain => {
                                docs.begin_save(id, target.clone(), again.clone(), false)
                            }
                            Retry::SaveAs => docs.save_as(id, again.clone()),
                            Retry::Cancel => (away.stopped)(),
                        },
                    ))
                });
            }
            Err(SaveFailure::Encode(advice)) => {
                let name = self.handle(id).title();
                self.ask(Some(id), true, move |_| {
                    Some(dialogs::save_not_possible(&name, &advice))
                });
                (after.stopped)();
            }
        }
    }

    /// Asks where to save document `id`, then saves it there.
    fn save_as(self, id: DocumentId, after: After) {
        let Some((name, folder, filters)) = self.with(|store| {
            let entry = store.entry(id)?;
            let folder = entry
                .path
                .as_deref()
                .and_then(Path::parent)
                .filter(|folder| !folder.as_os_str().is_empty())
                .map(Path::to_path_buf);
            Some((
                store.title_of(entry),
                folder,
                store.settings.filters.clone(),
            ))
        }) else {
            return;
        };
        let name = self
            .with(|store| {
                store
                    .entry(id)
                    .and_then(|entry| entry.path.as_deref().map(display_name))
            })
            .unwrap_or(name);
        let mut request = ChooserRequest::save(name).title("Save as");
        for filter in filters {
            request = request.filter(filter);
        }
        if let Some(folder) = folder {
            request = request.current_folder(folder);
        }
        let docs = self;
        choose(self.window, request, move |chosen| {
            match chosen.and_then(|chosen| chosen.paths.into_iter().next()) {
                // The chooser asked about replacing a file that is there.
                Some(path) => docs.begin_save(id, Some(path), after, true),
                None => (after.stopped)(),
            }
        });
    }

    // ---- Closing ----------------------------------------------------------

    fn close_doc(self, id: DocumentId, after: After) {
        let Some(edited) = self.with(|store| store.entry(id).map(|entry| entry.edited)) else {
            (after.done)();
            return;
        };
        if !edited {
            self.remove_entry(id);
            (after.done)();
            return;
        }
        self.emit(DocumentEvent::Reveal(self.handle(id)));
        self.ask(Some(id), true, move |docs| {
            let name = docs.handle(id).title();
            Some(dialogs::unsaved_changes(
                &[name],
                move |answer| match answer {
                    Unsaved::Save => {
                        let (after, closed) = (after.clone(), docs);
                        let saved = After {
                            done: Rc::new(move || {
                                closed.remove_entry(id);
                                (after.done)();
                            }),
                            stopped: after.stopped.clone(),
                        };
                        docs.begin_save(id, None, saved, false);
                    }
                    Unsaved::Discard => {
                        docs.remove_entry(id);
                        (after.done)();
                    }
                    Unsaved::Cancel => (after.stopped)(),
                },
            ))
        });
    }

    fn remove_entry(self, id: DocumentId) {
        let removed = self.edit(|store| {
            let at = store.entries.iter().position(|entry| entry.id == id)?;
            if store.current == Some(id) {
                store.current = None;
            }
            Some(store.entries.remove(at))
        });
        let Some(entry) = removed else { return };
        self.discard_draft_of(&entry);
        // The content and the watch go after the store is free.
        drop(entry);
        self.emit(DocumentEvent::Closed(id));
        self.publish_scene();
    }

    /// Closes the window, after asking about work that would be lost, as the
    /// window's close button does. An app's File > Quit and Close Window call
    /// this; `Window::close` and the compositor's close request reach the
    /// same place through the close guard.
    pub fn request_close(self) {
        let requests = self.with(|store| store.close_requests.clone());
        requests.push(());
    }

    fn close_window(self) {
        if self.with(|store| store.closing.replace(true)) {
            return;
        }
        let unsaved: Vec<DocumentId> = self
            .unsaved()
            .into_iter()
            .map(|document| document.id)
            .collect();
        if unsaved.is_empty() {
            self.finish_close();
            return;
        }
        let docs = self;
        self.emit(DocumentEvent::Reveal(self.handle(unsaved[0])));
        self.ask(None, true, move |docs_now| {
            let names: Vec<String> = unsaved
                .iter()
                .map(|id| docs_now.handle(*id).title())
                .collect();
            let ids = unsaved.clone();
            Some(dialogs::unsaved_changes(
                &names,
                move |answer| match answer {
                    Unsaved::Save => {
                        let closing = docs;
                        let after = After {
                            done: Rc::new(move || closing.finish_close()),
                            stopped: Rc::new(move || {
                                closing.with(|store| store.closing.set(false))
                            }),
                        };
                        docs.save_each(ids.clone(), after);
                    }
                    Unsaved::Discard => {
                        for id in &ids {
                            docs.remove_entry(*id);
                        }
                        docs.finish_close();
                    }
                    Unsaved::Cancel => docs.with(|store| store.closing.set(false)),
                },
            ))
        });
    }

    /// Writes the scene one last time, then closes the window.
    fn finish_close(self) {
        self.publish_scene();
        let (scene, window) = self.with(|store| (store.scene, store.window_id));
        spawn_in_window(async move {
            scene.flush_now().await;
            guard::close_now(window);
        });
    }

    // ---- Drafts -----------------------------------------------------------

    /// Writes drafts shortly, once for any number of edits.
    fn schedule_draft(self) {
        let delay = self
            .with(|store| (!store.draft_armed.replace(true)).then_some(store.settings.autosave));
        let Some(delay) = delay else { return };
        let docs = self;
        spawn_in_window(async move {
            timer(delay).await;
            docs.with(|store| store.draft_armed.set(false));
            docs.write_drafts();
        });
    }

    fn write_drafts(self) {
        struct Job {
            id: DocumentId,
            draft: String,
            meta: DraftMeta,
            revision: u64,
            content: Rc<dyn DocumentContent>,
            epoch: Arc<AtomicU64>,
        }
        let jobs: Vec<Job> = self.with(|store| {
            store
                .entries
                .iter()
                .filter(|entry| {
                    entry.edited
                        && entry.phase == Phase::Ready
                        && entry.drafted.get() != Some(entry.revision.get())
                })
                .map(|entry| Job {
                    id: entry.id,
                    draft: entry.draft.clone(),
                    meta: DraftMeta {
                        title: store.title_of(entry),
                        path: entry.path.clone(),
                        base: entry.stamp.get(),
                        extra: json!({
                            "app": entry.app_value.borrow().clone(),
                            "untitled": entry.untitled,
                            "scroll": entry.scroll.get(),
                        }),
                    },
                    revision: entry.revision.get(),
                    content: entry.content.clone(),
                    epoch: entry.epoch.clone(),
                })
                .collect()
        });
        if jobs.is_empty() {
            return;
        }
        // The app's `snapshot` runs with the store free.
        let work: Vec<_> = jobs
            .into_iter()
            .map(|job| {
                let seen = job.epoch.load(Ordering::SeqCst);
                (
                    job.id,
                    job.draft,
                    job.meta,
                    job.revision,
                    job.epoch,
                    seen,
                    job.content.snapshot(),
                )
            })
            .collect();
        let drafts = self.with(|store| store.drafts.clone());
        let docs = self;
        spawn_in_window(async move {
            let done = spawn_blocking(move || {
                work.into_iter()
                    .map(|(id, draft, meta, revision, epoch, seen, snapshot)| {
                        let wrote = snapshot.run().ok().is_some_and(|bytes| {
                            let _lane = DRAFT_LANE.lock().unwrap_or_else(PoisonError::into_inner);
                            // Deleted since: the draft is not wanted.
                            epoch.load(Ordering::SeqCst) == seen
                                && drafts.put(&draft, &meta, &bytes).is_ok()
                        });
                        (id, revision, wrote)
                    })
                    .collect::<Vec<_>>()
            })
            .await;
            docs.with(|store| {
                for (id, revision, wrote) in done {
                    if wrote && let Some(entry) = store.entry(id) {
                        entry.drafted.set(Some(revision));
                    }
                }
            });
        });
    }

    fn discard_draft(self, id: DocumentId) {
        let drafts = self.with(|store| store.drafts.clone());
        self.with(|store| {
            if let Some(entry) = store.entry(id) {
                discard(&drafts, entry);
            }
        });
    }

    fn discard_draft_of(self, entry: &Entry) {
        let drafts = self.with(|store| store.drafts.clone());
        discard(&drafts, entry);
    }

    // ---- The scene --------------------------------------------------------

    /// Tells the scene which documents are open.
    fn publish_scene(self) {
        let (documents, scene) = self.with(|store| {
            let documents = store
                .entries
                .iter()
                // A new document nobody has typed in has nothing to bring back.
                .filter(|entry| entry.path.is_some() || entry.edited)
                .map(|entry| SceneDocument {
                    path: entry.path.clone(),
                    draft: entry.edited.then(|| entry.draft.clone()),
                    title: store.title_of(entry),
                    scroll: entry.scroll.get(),
                    app: entry.app_value.borrow().clone(),
                    current: store.current == Some(entry.id),
                })
                .collect::<Vec<_>>();
            (documents, store.scene)
        });
        scene.set_documents(documents);
    }

    /// Like `publish_scene`, once for any number of scrolls in a quarter of a
    /// second.
    fn publish_scene_soon(self) {
        if self.with(|store| store.scene_armed.replace(true)) {
            return;
        }
        let docs = self;
        spawn_in_window(async move {
            timer(Duration::from_millis(250)).await;
            docs.with(|store| store.scene_armed.set(false));
            docs.publish_scene();
        });
    }

    // ---- At start ---------------------------------------------------------

    /// Offers the work a crash left, then brings back the documents of the
    /// last run, then makes a blank one when nothing is open.
    async fn start(self) {
        let (settings, drafts, scene) =
            self.with(|store| (store.settings.clone(), store.drafts.clone(), store.scene));
        self.refresh_recents();
        if scene.is_primary() && settings.restore {
            let found = spawn_blocking(move || {
                drafts.prune(DRAFT_LIFE);
                drafts.recoverable()
            })
            .await;
            let scened = scene.documents();
            self.restore(found, scened);
        } else {
            self.blank_if_empty();
        }
    }

    fn blank_if_empty(self) {
        if self.with(|store| store.settings.blank && store.entries.is_empty()) {
            self.new_document();
        }
    }

    /// The plan: the scene's documents in order, each with its draft when it
    /// has one, then the drafts the scene did not mention.
    fn restore(self, found: Vec<Draft>, scened: Vec<SceneDocument>) {
        enum Step {
            Path(SceneDocument),
            Draft(Draft, Option<SceneDocument>),
        }
        let mut found = found;
        let mut steps: Vec<Step> = Vec::new();
        for item in scened {
            let draft = item
                .draft
                .as_ref()
                .and_then(|id| found.iter().position(|draft| &draft.id == id))
                .map(|at| found.remove(at));
            match (draft, item.path.is_some()) {
                (Some(draft), _) => steps.push(Step::Draft(draft, Some(item))),
                (None, true) => steps.push(Step::Path(item)),
                (None, false) => {}
            }
        }
        steps.extend(found.into_iter().map(|draft| Step::Draft(draft, None)));
        let offered: Vec<Draft> = steps
            .iter()
            .filter_map(|step| match step {
                Step::Draft(draft, _) => Some(draft.clone()),
                Step::Path(_) => None,
            })
            .collect();
        let docs = self;
        let run = move |recovery: Recovery| {
            let mut showing = None;
            for step in &steps {
                let opened = match (step, recovery) {
                    (Step::Draft(draft, scene), Recovery::Recover) => {
                        docs.recover(draft.clone(), scene.as_ref())
                    }
                    (Step::Draft(_, Some(scene)), _) if scene.path.is_some() => {
                        docs.restore_path(scene)
                    }
                    (Step::Draft(..), _) => None,
                    (Step::Path(scene), _) => docs.restore_path(scene),
                };
                let current = match step {
                    Step::Path(scene) => scene.current,
                    Step::Draft(_, scene) => scene.as_ref().is_some_and(|scene| scene.current),
                };
                if current {
                    showing = opened;
                }
            }
            if let Some(document) = showing {
                docs.emit(DocumentEvent::Reveal(document));
            }
            docs.blank_if_empty();
        };
        if offered.is_empty() {
            run(Recovery::Later);
            return;
        }
        let app_name = self.with(|store| store.settings.app_name.clone());
        let items: Vec<Recoverable> = offered
            .iter()
            .map(|draft| Recoverable {
                title: draft.meta.title.clone(),
                saved: draft.saved_at,
            })
            .collect();
        let drafts = self.with(|store| store.drafts.clone());
        let run = Rc::new(run);
        self.ask(None, false, move |_| {
            Some(dialogs::recover_work(&app_name, &items, move |answer| {
                if answer == Recovery::Discard {
                    let (drafts, offered) = (drafts.clone(), offered.clone());
                    spawn_detached("document-draft", move || {
                        let _lane = DRAFT_LANE.lock().unwrap_or_else(PoisonError::into_inner);
                        for draft in &offered {
                            drafts.remove(&draft.id);
                        }
                    });
                }
                (run.as_ref())(answer);
            }))
        });
    }

    fn restore_path(self, scene: &SceneDocument) -> Option<Document> {
        let mut plan = Plan::new(scene.path.clone()?, Origin::Restored);
        plan.app_value = scene.app.clone();
        plan.scroll = scene.scroll;
        Some(self.open_with(plan))
    }

    /// Makes a document of `draft`: the unsaved content comes back as it was,
    /// still unsaved.
    fn recover(self, draft: Draft, scene: Option<&SceneDocument>) -> Option<Document> {
        let drafts = self.with(|store| store.drafts.clone());
        let id = self.with(Store::next_id);
        let app_value = draft
            .meta
            .extra
            .get("app")
            .cloned()
            .or_else(|| scene.map(|scene| scene.app.clone()))
            .unwrap_or(Value::Null);
        let init = DocumentInit {
            id,
            title: draft.meta.title.clone(),
            path: draft.meta.path.clone(),
            app_value,
            origin: Origin::Recovered,
        };
        let wanted = draft
            .meta
            .extra
            .get("untitled")
            .and_then(Value::as_u64)
            .map(|n| n as u32)
            .unwrap_or(0);
        let number = self.with(|store| {
            if draft.meta.path.is_some() {
                0
            } else if wanted > 0 && !store.entries.iter().any(|entry| entry.untitled == wanted) {
                wanted
            } else {
                store.free_untitled()
            }
        });
        let scroll = draft
            .meta
            .extra
            .get("scroll")
            .and_then(|value| serde_json::from_value::<(f32, f32)>(value.clone()).ok())
            .or_else(|| scene.map(|scene| scene.scroll))
            .unwrap_or((0.0, 0.0));
        let base = draft.meta.base;
        let has_path = draft.meta.path.is_some();
        let draft_id = draft.id.clone();
        let document = self.add(init, draft_id.clone(), move |entry| {
            entry.untitled = number;
            entry.edited = true;
            entry.stamp.set(base);
            entry.scroll.set(scroll);
        });
        let docs = self;
        spawn_in_window(async move {
            let reading = draft_id.clone();
            let bytes = spawn_blocking(move || {
                // This process owns the draft from now on: another copy of
                // the app does not offer it again.
                drafts.adopt(&reading)?;
                drafts.bytes(&reading)
            })
            .await;
            match bytes {
                Ok(bytes) => {
                    let content = docs.edit(|store| {
                        let entry = store.entry_mut(id)?;
                        entry.phase = Phase::Ready;
                        entry.revision.set(entry.revision.get() + 1);
                        entry.drafted.set(Some(entry.revision.get()));
                        Some(entry.content.clone())
                    });
                    if let Some(content) = content {
                        docs.with(|store| store.loading.set(store.loading.get() + 1));
                        content.load(bytes, LoadReason::Recover);
                        docs.with(|store| store.loading.set(store.loading.get() - 1));
                        if has_path {
                            docs.start_watch(id);
                            // The file may have changed while the app was closed.
                            docs.check_changes();
                        }
                        docs.publish_scene();
                    }
                }
                Err(_) => docs.remove_entry(id),
            }
        });
        Some(document)
    }

    // ---- Questions, one at a time -----------------------------------------

    /// Puts a question to the person when no other is on screen. `doc` is
    /// the document it is about: if that is closed by then, it is not asked.
    /// `urgent` goes to the front: the person just did something that waits
    /// for the answer.
    fn ask(
        self,
        doc: Option<DocumentId>,
        urgent: bool,
        build: impl FnOnce(Documents) -> Option<Dialog> + 'static,
    ) {
        let queued = Queued {
            doc,
            build: Box::new(build),
        };
        self.with(|store| {
            let mut queue = store.queue.borrow_mut();
            if urgent {
                queue.push_front(queued);
            } else {
                queue.push_back(queued);
            }
        });
        if self.with(|store| store.pumping.replace(true)) {
            return;
        }
        let docs = self;
        spawn_in_window(async move {
            loop {
                while docs.overlays.dialog_open() {
                    timer(Duration::from_millis(120)).await;
                }
                let next = docs.with(|store| store.queue.borrow_mut().pop_front());
                let Some(next) = next else { break };
                if next.doc.is_some_and(|id| docs.get(id).is_none()) {
                    continue;
                }
                if let Some(dialog) = (next.build)(docs) {
                    docs.overlays.show_dialog(dialog);
                    // Let it open before looking whether it has closed.
                    timer(Duration::from_millis(60)).await;
                }
            }
            docs.with(|store| store.pumping.set(false));
        });
    }
}

/// Deletes the draft of `entry`, and makes a draft that is being written now
/// give up.
fn discard(drafts: &Drafts, entry: &Entry) {
    entry.epoch.fetch_add(1, Ordering::SeqCst);
    entry.drafted.set(None);
    let (drafts, id) = (drafts.clone(), entry.draft.clone());
    spawn_detached("document-draft", move || {
        let _lane = DRAFT_LANE.lock().unwrap_or_else(PoisonError::into_inner);
        drafts.remove(&id);
    });
}

/// The content of a document that is being made, until the app's is.
struct Placeholder;

impl DocumentContent for Placeholder {
    fn snapshot(&self) -> DocumentSnapshot {
        DocumentSnapshot::bytes(Vec::new())
    }

    fn load(&self, _bytes: Vec<u8>, _reason: LoadReason) {}
}

/// How a file is opened.
#[derive(Clone)]
struct Plan {
    path: PathBuf,
    origin: Origin,
    create: bool,
    app_value: Value,
    scroll: (f32, f32),
}

impl Plan {
    fn new(path: PathBuf, origin: Origin) -> Plan {
        Plan {
            path,
            origin,
            create: false,
            app_value: Value::Null,
            scroll: (0.0, 0.0),
        }
    }
}

// ---- One document ---------------------------------------------------------

impl Document {
    /// The id, to look the document up again or to keep in the app's own tab model.
    pub fn id(self) -> DocumentId {
        self.id
    }

    /// Whether the document is still open.
    pub fn is_open(self) -> bool {
        self.docs.with(|store| store.entry(self.id).is_some())
    }

    fn read<R: Default>(self, read: impl FnOnce(&Store, &Entry) -> R) -> R {
        self.docs
            .watching(|store| store.entry(self.id).map(|entry| read(store, entry)))
            .unwrap_or_default()
    }

    /// The file, or `None` for a document that was never saved.
    pub fn path(self) -> Option<PathBuf> {
        self.read(|_, entry| entry.path.clone())
    }

    /// The name: the file's, or "Untitled" and "Untitled 2".
    pub fn title(self) -> String {
        self.read(|store, entry| store.title_of(entry))
    }

    /// The name of the document for a tab or a list. Unsaved changes are shown
    /// by the tab's dot, not here.
    pub fn display_title(self) -> String {
        self.read(|store, entry| store.title_of(entry))
    }

    /// The title for the window: `name – app_name`, with [`UNSAVED_MARK`]
    /// before the name while there are unsaved changes.
    pub fn window_title(self, app_name: &str) -> String {
        let name = self.display_title();
        let mark = if self.is_unsaved() { UNSAVED_MARK } else { "" };
        format!("{mark}{name} \u{2013} {app_name}")
    }

    /// Whether the document has changes that are not in its file.
    pub fn is_unsaved(self) -> bool {
        self.read(|_, entry| entry.edited)
    }

    /// Whether the document has no file yet.
    pub fn is_untitled(self) -> bool {
        self.read(|_, entry| entry.path.is_none())
    }

    pub fn phase(self) -> Phase {
        self.docs
            .watching(|store| store.entry(self.id).map(|entry| entry.phase))
            .unwrap_or(Phase::Ready)
    }

    /// Whether a save is running.
    pub fn is_saving(self) -> bool {
        self.read(|_, entry| entry.saving)
    }

    /// Whether the person may not write the file. A save asks where to save
    /// a copy.
    pub fn is_read_only(self) -> bool {
        self.read(|_, entry| entry.read_only)
    }

    /// A change made to the file by someone else that the person has not
    /// answered yet.
    pub fn change_on_disk(self) -> Option<FileChange> {
        self.read(|_, entry| entry.external)
    }

    /// Whether the document came back from a crash and has not been saved.
    pub fn is_recovered(self) -> bool {
        self.read(|_, entry| entry.edited && entry.stamp.get().is_none() && entry.path.is_none())
    }

    /// The content has changed. Call it for every edit; it is cheap. The
    /// first edit marks the document unsaved, and every edit schedules a
    /// draft.
    pub fn mark_edited(self) {
        let first = self.docs.with(|store| {
            if store.loading.get() > 0 {
                return None;
            }
            let entry = store.entry(self.id)?;
            if entry.phase == Phase::Loading {
                return None;
            }
            entry.revision.set(entry.revision.get() + 1);
            Some(!entry.edited)
        });
        match first {
            None => return,
            Some(true) => {
                let id = self.id;
                self.docs.edit(|store| {
                    if let Some(entry) = store.entry_mut(id) {
                        entry.edited = true;
                    }
                });
                self.docs.publish_scene();
            }
            Some(false) => {}
        }
        self.docs.schedule_draft();
    }

    /// The content is, or is no longer, what the file holds: an undo that went
    /// back to the point where it was saved calls `set_edited(false)`. With
    /// `true` it is [`mark_edited`](Self::mark_edited).
    pub fn set_edited(self, edited: bool) {
        if edited {
            self.mark_edited();
            return;
        }
        let was = self.docs.edit(|store| {
            let entry = store.entry_mut(self.id)?;
            let was = entry.edited;
            entry.edited = false;
            entry.revision.set(entry.revision.get() + 1);
            Some(was)
        });
        if was == Some(true) {
            self.docs.discard_draft(self.id);
            self.docs.publish_scene();
        }
    }

    /// Saves the document: to its file, or, when it has none or may not write
    /// it, after asking where.
    pub fn save(self) {
        self.docs.begin_save(self.id, None, After::nothing(), false);
    }

    /// Like [`save`](Self::save), and `then` runs once it has been saved.
    pub fn save_then(self, then: impl Fn() + 'static) {
        self.docs
            .begin_save(self.id, None, After::then(then), false);
    }

    /// Asks where to save the document, then saves it there. It then belongs
    /// to the new file.
    pub fn save_as(self) {
        self.docs.save_as(self.id, After::nothing());
    }

    /// Reads the file again. A document with unsaved changes asks first,
    /// because they are lost.
    pub fn reload(self) {
        let Some((edited, has_path)) = self.docs.with(|store| {
            store
                .entry(self.id)
                .map(|entry| (entry.edited, entry.path.is_some()))
        }) else {
            return;
        };
        if !has_path {
            return;
        }
        if !edited {
            self.docs.reload(self.id, false);
            return;
        }
        let (docs, id) = (self.docs, self.id);
        docs.ask(Some(id), true, move |docs| {
            let name = docs.handle(id).title();
            Some(dialogs::revert_changes(&name, move |answer| {
                if answer == Revert::Revert {
                    docs.reload(id, false);
                }
            }))
        });
    }

    /// Closes the document, after asking about its unsaved changes.
    pub fn close(self) {
        self.docs.close_doc(self.id, After::nothing());
    }

    /// Closes the document without asking: its unsaved changes are lost.
    pub fn close_discarding(self) {
        self.docs.remove_entry(self.id);
    }

    /// Asks the app's tabs to show this document.
    pub fn reveal(self) {
        if self.is_open() {
            self.docs.emit(DocumentEvent::Reveal(self));
        }
    }

    /// The value the app stored with this document, `Null` if none.
    pub fn app_value(self) -> Value {
        self.docs
            .with(|store| {
                store
                    .entry(self.id)
                    .map(|entry| entry.app_value.borrow().clone())
            })
            .unwrap_or(Value::Null)
    }

    /// The app's value as a `T`, if there is one and it is one.
    pub fn app_value_as<T: DeserializeOwned>(self) -> Option<T> {
        serde_json::from_value(self.app_value()).ok()
    }

    /// Stores a value of the app's with the document: it is kept in drafts and
    /// in the scene, and handed back in [`DocumentInit::app_value`] when the
    /// document is made again. A caret, a language, a zoom: what the app wants
    /// to find when the person comes back.
    pub fn set_app_value<T: Serialize>(self, value: &T) {
        let Ok(value) = serde_json::to_value(value) else {
            return;
        };
        let changed = self.docs.with(|store| {
            let entry = store.entry(self.id)?;
            let mut stored = entry.app_value.borrow_mut();
            if *stored == value {
                return None;
            }
            *stored = value;
            Some(())
        });
        if changed.is_some() {
            self.docs.publish_scene_soon();
        }
    }

    /// Where the document is scrolled to, as the app last said.
    pub fn scroll(self) -> (f32, f32) {
        self.docs
            .with(|store| store.entry(self.id).map(|entry| entry.scroll.get()))
            .unwrap_or((0.0, 0.0))
    }

    /// Says where the document is scrolled to, in the app's own units. It is
    /// kept in the scene. Cheap: call it as the content scrolls.
    pub fn set_scroll(self, x: f32, y: f32) {
        let changed = self.docs.with(|store| {
            let entry = store.entry(self.id)?;
            (entry.scroll.replace((x, y)) != (x, y)).then_some(())
        });
        if changed.is_some() {
            self.docs.publish_scene_soon();
        }
    }
}

#[cfg(all(test, feature = "testing"))]
#[path = "documents_tests.rs"]
mod tests;
