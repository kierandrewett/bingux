//! The writer of an open profile: one thread that owns its files, so nothing
//! the window does waits for a disk. The window sends what changed; the thread
//! writes it and answers with how keeping went ([`Event`]), waking the window
//! the way the engine's events do.
//!
//! Settings are written at once. History is batched: what has queued is
//! written at most once a second, on flush, and when the store is dropped, so
//! a crash loses at most the last second. The `history` search index is kept
//! in step with every change as it arrives, so the visits of this run are
//! found even when they cannot be written.
//!
//! A file this build could not read (damaged, or from a newer browser) is never
//! written over: its part stays [`NotSaved`] until the person chooses Start
//! over, which moves the file aside first.

mod files;
mod keeping;

#[cfg(test)]
mod tests;

pub use files::{Files, Problem};
pub use keeping::{ClearScope, Keeping, NotSaved, Part};

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::task::Waker;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use bingux_ui::search::{Index, SearchService};

use crate::history::{CAP, Change, Database, History, INDEX, Page};
use crate::profiles::Folders;
use crate::settings::Settings;

/// How long history waits in memory before it is written.
const BATCH: Duration = Duration::from_secs(1);

/// How keeping a part went.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Settings(Keeping),
    History(Keeping),
}

enum Command {
    Settings(Vec<u8>),
    StartOverSettings(Vec<u8>),
    History(Change),
    ClearHistory,
    StartOverHistory,
    Flush(Sender<()>),
}

/// The events the thread has sent and the waker that tells the window.
#[derive(Default)]
struct Outbox {
    events: Vec<Event>,
    waker: Option<Waker>,
}

impl Outbox {
    fn send(outbox: &Mutex<Self>, event: Event) {
        let mut outbox = outbox.lock().unwrap_or_else(PoisonError::into_inner);
        outbox.events.push(event);
        if let Some(waker) = &outbox.waker {
            waker.wake_by_ref();
        }
    }
}

/// The writer of one open profile. Dropping it writes what is queued and
/// stops the thread.
pub struct Store {
    commands: Option<Sender<Command>>,
    outbox: Arc<Mutex<Outbox>>,
    index: Arc<Index>,
    thread: Option<JoinHandle<()>>,
}

/// What the thread owns.
struct Writer {
    settings_path: PathBuf,
    settings: Keeping,
    history_path: PathBuf,
    history: Keeping,
    database: Option<Database>,
    /// Changes not written yet, oldest first.
    pending: Vec<Change>,
    /// When `pending` is written.
    due: Option<Instant>,
    index: Arc<Index>,
    cap: usize,
    /// The file was opened after the index was filled: the next good write
    /// fills the index from it again, older pages included.
    stale_index: bool,
    outbox: Arc<Mutex<Outbox>>,
}

impl Writer {
    fn report_settings(&mut self, keeping: Keeping) {
        if keeping != self.settings {
            self.settings = keeping.clone();
            Outbox::send(&self.outbox, Event::Settings(keeping));
        }
    }

    fn report_history(&mut self, keeping: Keeping) {
        if keeping != self.history {
            self.history = keeping.clone();
            Outbox::send(&self.outbox, Event::History(keeping));
        }
    }

    fn write_settings(&mut self, bytes: &[u8]) {
        // What could not be read is never written over.
        if !self.settings.is_writable() {
            return;
        }
        self.replace_settings(bytes);
    }

    fn replace_settings(&mut self, bytes: &[u8]) {
        let keeping = match Files::replace(&self.settings_path, bytes) {
            Ok(()) => Keeping::Saved,
            Err(error) => Keeping::of_problem(&Problem::of(&error)),
        };
        self.report_settings(keeping);
    }

    fn start_over_settings(&mut self, bytes: &[u8]) {
        let Keeping::NotSaved(NotSaved::Damaged { detail }) = &self.settings else {
            return;
        };
        let detail = detail.clone();
        match NotSaved::move_aside(&[self.settings_path.clone()]) {
            Ok(()) => self.replace_settings(bytes),
            // Still damaged, so still never written over; the page says why
            // Start over did not work.
            Err(error) => self.report_settings(Keeping::NotSaved(NotSaved::Damaged {
                detail: format!("{detail}; it could not be moved aside: {error}"),
            })),
        }
    }

    /// Opens the history file and fills the index from it.
    fn load_history(&mut self) {
        let loaded = Database::open(&self.history_path).and_then(|database| {
            database
                .trim(self.cap)
                .and_then(|_| database.pages())
                .map(|pages| (database, pages))
                .map_err(|error| Database::not_saved(&error))
        });
        match loaded {
            Ok((database, pages)) => {
                self.database = Some(database);
                if let Err(error) = self.index.replace_all(pages.iter().map(Page::document)) {
                    self.report_history(Keeping::NotSaved(NotSaved::Failed(format!(
                        "the history index: {error}"
                    ))));
                    return;
                }
                self.report_history(Keeping::Saved);
            }
            Err(why) => self.report_history(Keeping::NotSaved(why)),
        }
    }

    /// Whether there is a connection to write with, opening the file again
    /// after a failed open; false when the file must not or cannot be written.
    fn can_write(&mut self) -> bool {
        if !self.history.is_writable() {
            return false;
        }
        if self.database.is_none() {
            match Database::open(&self.history_path) {
                Ok(database) => {
                    self.database = Some(database);
                    self.stale_index = true;
                }
                Err(why) => {
                    self.report_history(Keeping::NotSaved(why));
                    return false;
                }
            }
        }
        true
    }

    /// The index follows `change` at once.
    fn index_change(&self, change: &Change) {
        let page = match change {
            Change::Visit { url, title, at } => {
                let mut page = self
                    .index
                    .get(url)
                    .map(|document| Page::from_document(&document))
                    .unwrap_or(Page {
                        url: url.clone(),
                        title: String::new(),
                        visit_count: 0,
                        last_visit: 0,
                    });
                page.visit_count = page.visit_count.saturating_add(1);
                page.last_visit = *at;
                if !title.is_empty() {
                    page.title.clone_from(title);
                }
                page
            }
            Change::Title { url, title } => {
                let Some(document) = self.index.get(url) else {
                    return;
                };
                let mut page = Page::from_document(&document);
                page.title.clone_from(title);
                page
            }
        };
        let _ = self.index.upsert([page.document()]);
    }

    fn queue(&mut self, change: Change) {
        self.index_change(&change);
        self.pending.push(change);
        if self.due.is_none() {
            self.due = Some(Instant::now() + BATCH);
        }
    }

    /// Fills the index from the file again when it was opened late.
    fn reload_index(&mut self) {
        if !std::mem::take(&mut self.stale_index) {
            return;
        }
        if let Some(database) = &self.database
            && let Ok(pages) = database.pages()
        {
            let _ = self.index.replace_all(pages.iter().map(Page::document));
        }
    }

    /// Writes what has queued. On failure it stays queued for the next try.
    fn commit(&mut self) {
        self.due = None;
        if self.pending.is_empty() {
            return;
        }
        if !self.can_write() {
            return;
        }
        let Some(database) = self.database.as_mut() else {
            return;
        };
        let written = database.write(&self.pending);
        match written {
            Ok(()) => {
                self.pending.clear();
                self.reload_index();
                self.report_history(Keeping::Saved);
            }
            Err(error) => self.report_history(Keeping::NotSaved(Database::not_saved(&error))),
        }
    }

    fn clear_history(&mut self) {
        self.pending.clear();
        self.due = None;
        self.index.clear();
        if !self.can_write() {
            return;
        }
        let Some(database) = self.database.as_mut() else {
            return;
        };
        let cleared = database.clear();
        match cleared {
            Ok(()) => self.report_history(Keeping::Saved),
            Err(error) => self.report_history(Keeping::NotSaved(Database::not_saved(&error))),
        }
    }

    fn start_over_history(&mut self) {
        let Keeping::NotSaved(NotSaved::Damaged { detail }) = &self.history else {
            return;
        };
        let detail = detail.clone();
        self.database = None;
        let path = &self.history_path;
        let files = [
            path.clone(),
            Files::beside(path, "-wal"),
            Files::beside(path, "-shm"),
        ];
        if let Err(error) = NotSaved::move_aside(&files) {
            self.report_history(Keeping::NotSaved(NotSaved::Damaged {
                detail: format!("{detail}; it could not be moved aside: {error}"),
            }));
            return;
        }
        // A new file: what this run visited is in the index and queued.
        self.history = Keeping::Loading;
        match Database::open(&self.history_path) {
            Ok(database) => {
                self.database = Some(database);
                self.report_history(Keeping::Saved);
                self.commit();
            }
            Err(why) => self.report_history(Keeping::NotSaved(why)),
        }
    }

    fn next(&self, commands: &Receiver<Command>) -> Result<Option<Command>, ()> {
        match self.due {
            None => commands.recv().map(Some).map_err(|_| ()),
            Some(due) => {
                match commands.recv_timeout(due.saturating_duration_since(Instant::now())) {
                    Ok(command) => Ok(Some(command)),
                    Err(RecvTimeoutError::Timeout) => Ok(None),
                    Err(RecvTimeoutError::Disconnected) => Err(()),
                }
            }
        }
    }

    fn run(mut self, commands: Receiver<Command>) {
        self.load_history();
        while let Ok(command) = self.next(&commands) {
            match command {
                None => self.commit(),
                Some(Command::Settings(bytes)) => self.write_settings(&bytes),
                Some(Command::StartOverSettings(bytes)) => self.start_over_settings(&bytes),
                Some(Command::History(change)) => self.queue(change),
                Some(Command::ClearHistory) => self.clear_history(),
                Some(Command::StartOverHistory) => self.start_over_history(),
                Some(Command::Flush(done)) => {
                    self.commit();
                    let _ = done.send(());
                }
            }
        }
        self.commit();
    }
}

impl Store {
    /// Starts the writer of the profile in `folders`. `settings` is how the
    /// settings read at open went. The history index is created in `search`
    /// at once (empty) and filled by the thread from the file.
    pub fn open(
        folders: &Folders,
        settings: Keeping,
        search: &Arc<SearchService>,
    ) -> std::io::Result<Self> {
        Self::open_with_cap(folders, settings, search, CAP)
    }

    /// [`Store::open`], keeping at most `cap` pages.
    pub fn open_with_cap(
        folders: &Folders,
        settings: Keeping,
        search: &Arc<SearchService>,
        cap: usize,
    ) -> std::io::Result<Self> {
        let index = search
            .create_index(INDEX, History::schema())
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        let outbox = Arc::new(Mutex::new(Outbox::default()));
        let writer = Writer {
            settings_path: folders.settings(),
            settings,
            history_path: folders.store(),
            history: Keeping::Loading,
            database: None,
            pending: Vec::new(),
            due: None,
            index: Arc::clone(&index),
            cap,
            stale_index: false,
            outbox: Arc::clone(&outbox),
        };
        let (commands, receiver) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("browser-store".into())
            .spawn(move || writer.run(receiver))?;
        Ok(Self {
            commands: Some(commands),
            outbox,
            index,
            thread: Some(thread),
        })
    }

    fn send(&self, command: Command) {
        if let Some(commands) = &self.commands {
            let _ = commands.send(command);
        }
    }

    /// Keep `settings`.
    pub fn save_settings(&self, settings: &Settings) {
        self.send(Command::Settings(settings.encode()));
    }

    /// Move the damaged settings file aside and keep `settings` from now on.
    pub fn start_over_settings(&self, settings: &Settings) {
        self.send(Command::StartOverSettings(settings.encode()));
    }

    /// Unix milliseconds now.
    fn now_ms() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| {
                i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
            })
    }

    /// A page was loaded. The browser's own pages are not history.
    pub fn record_visit(&self, url: &str, title: &str) {
        if !Page::is_recorded(url) {
            return;
        }
        self.send(Command::History(Change::Visit {
            url: url.to_string(),
            title: title.to_string(),
            at: Self::now_ms(),
        }));
    }

    /// The page at `url` got its title after it was recorded.
    pub fn set_title(&self, url: &str, title: &str) {
        if !Page::is_recorded(url) || title.is_empty() {
            return;
        }
        self.send(Command::History(Change::Title {
            url: url.to_string(),
            title: title.to_string(),
        }));
    }

    /// Forget every page: the file and the index.
    pub fn clear_history(&self) {
        self.send(Command::ClearHistory);
    }

    /// Move the damaged history file aside and keep history in a new one.
    pub fn start_over_history(&self) {
        self.send(Command::StartOverHistory);
    }

    /// How many pages the history holds now.
    pub fn history_len(&self) -> usize {
        self.index.len()
    }

    /// Wake `waker` when an event is waiting.
    pub fn set_waker(&self, waker: Waker) {
        let mut outbox = self.outbox.lock().unwrap_or_else(PoisonError::into_inner);
        if !outbox.events.is_empty() {
            waker.wake_by_ref();
        }
        outbox.waker = Some(waker);
    }

    /// The events not taken yet, oldest first.
    pub fn take_events(&self) -> Vec<Event> {
        std::mem::take(
            &mut self
                .outbox
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .events,
        )
    }

    /// Waits until everything sent before has been written. For tests and
    /// for the window's last moment, never for a frame.
    pub fn flush(&self) {
        let (done, wait) = mpsc::channel();
        self.send(Command::Flush(done));
        let _ = wait.recv();
    }
}

impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Store").finish_non_exhaustive()
    }
}

impl Drop for Store {
    fn drop(&mut self) {
        // Closing the channel ends the thread once the queue is written.
        self.commands = None;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
