//! Opening a folder: streaming, cancellable, cached, and never blocking.
//!
//! [`open`] starts a [`Load`]. It reports what it finds as [`Event`]s from
//! its own thread; a view keeps a [`Listing`] up to date with
//! [`Listing::apply_event`] and re-selects when that returns true.
//!
//! ```text
//!   open ──► Kind ─► Cached ─► (nothing changed) ──────────────► Done
//!                       └────► Diff ─────────────────────────────► Done
//!            Kind ─► Batch ─► Batch ─► ... ─────────────────────► Done
//!                    Stats (visible rows, then the rest where cheap)
//!                    Diff  (the folder changed later)
//!                    Unresponsive (the drive stopped answering)
//! ```
//!
//! # How it stays fast
//!
//! * **Stale while revalidate.** If the [`cache`](crate::cache) knows the
//!   folder, its entries arrive first as [`Event::Cached`], before the disk is
//!   touched. The folder is then checked with one `statx` on the folder
//!   itself. If its stamp is unchanged, nothing else is read. If it changed,
//!   the folder is read and only the difference is sent as [`Event::Diff`],
//!   so rows are patched, not rebuilt, and the scroll position and selection
//!   stay.
//! * **A first batch in 16 ms.** Without a cache the first [`Event::Batch`]
//!   leaves after 50 entries or 16 ms, whichever is first. Later batches
//!   come at growing intervals up to 256 ms.
//! * **A directory read gives a name and a type; nothing more is asked of a
//!   network mount.** On local disks every entry is `statx`ed as it is read
//!   (a few microseconds each). On a network mount the entries arrive
//!   unsized, and [`Load::want_stats`] reads size and time only for the rows
//!   in view, newest request first, on one worker thread, so scrolling
//!   never queues behind rows nobody sees.
//! * **Nothing blocks the caller, and a hung mount cannot hold a thread the
//!   caller needs.** All blocking calls run on threads the loader owns. If a
//!   mount stops answering the loader reports [`Event::Unresponsive`] and
//!   keeps showing what it has; cancelling abandons the stuck thread, which
//!   ends when the kernel finally returns.
//!
//! The kind of drive is found from the folder's own `st_dev` and the mount
//! table, not from its path, so `~/Downloads` that links to a network share
//! is treated as the network folder it is.

use std::collections::{HashMap, HashSet, VecDeque};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::cache::Cache;
use crate::entry::Entry;
use crate::listing::{Cancel, ListError, Listing, read_folder};
use crate::mounts::{self, MountKind};
use crate::stat::{self, DirStamp, StatInfo};
use crate::watch::Watcher;

/// How hard the loader works on a kind of drive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Policy {
    pub kind: MountKind,
    /// Threads reading sizes and times. 4 for a local disk, 1 for a network
    /// share: a share answers one thing at a time as fast as many.
    pub workers: usize,
    /// `statx` every entry while reading the folder.
    pub stat_all: bool,
    /// Views may decode thumbnails of files here. False where it would read
    /// file content over a network or from a disc.
    pub thumbnails: bool,
    /// Views may open files to find their type.
    pub sniff_content: bool,
    /// How long without any answer before [`Event::Unresponsive`].
    pub unresponsive_after: Duration,
    /// How long, with nothing to show, before [`Event::Failed`] with
    /// [`ListError::Unresponsive`].
    pub give_up_after: Duration,
    /// How often the folder's stamp is checked for changes where inotify
    /// cannot be trusted (network mounts). `None` for no polling.
    pub poll_every: Option<Duration>,
}

impl Policy {
    pub fn for_kind(kind: MountKind) -> Policy {
        match kind {
            MountKind::Local => Policy {
                kind,
                workers: 4,
                stat_all: true,
                thumbnails: true,
                sniff_content: true,
                unresponsive_after: Duration::from_secs(3),
                give_up_after: Duration::from_secs(60),
                poll_every: None,
            },
            MountKind::Removable => Policy {
                kind,
                workers: 2,
                stat_all: true,
                thumbnails: true,
                sniff_content: true,
                unresponsive_after: Duration::from_secs(3),
                give_up_after: Duration::from_secs(60),
                poll_every: None,
            },
            MountKind::Network | MountKind::Optical | MountKind::Other => Policy {
                kind,
                workers: 1,
                stat_all: false,
                thumbnails: false,
                sniff_content: false,
                unresponsive_after: Duration::from_secs(2),
                give_up_after: Duration::from_secs(30),
                poll_every: Some(Duration::from_secs(5)),
            },
        }
    }

    /// The policy for the mount `path` is on, by the path's text. Reads the
    /// mount table, so not for a UI thread. [`open`] does not use it: it
    /// learns the real kind from the folder's own device.
    pub fn for_path(path: &Path) -> Policy {
        Policy::for_kind(mounts::kind_of(path))
    }
}

/// What differs between the folder and the listing a view has.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Diff {
    /// New entries, appended to the listing.
    pub added: Vec<Entry>,
    /// Names no longer in the folder.
    pub removed: Vec<String>,
    /// Entries that are different now, matched by name and replaced in place.
    pub changed: Vec<Entry>,
}

impl Diff {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }
}

/// Sizes and times read for entries of a listing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatBatch {
    /// [`Listing::revision`] the indices belong to.
    pub revision: u64,
    /// `(index into Listing::entries, what was read)`.
    pub updates: Vec<(u32, StatInfo)>,
}

/// What a [`Load`] reports. See the [module documentation](self).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// What kind of drive the folder is on, once known. Honour
    /// [`Policy::for_kind`] for thumbnails from here on.
    Kind(MountKind),
    /// The cache's entries, shown at once. They may be out of date until
    /// [`Event::Done`]. Replaces whatever the listing held.
    Cached { entries: Vec<Entry> },
    /// More entries, appended. Only sent when no cache was shown.
    Batch(Vec<Entry>),
    /// The folder has been read (or confirmed unchanged).
    Done { total: usize },
    /// The folder differs from what was shown: patch, do not rebuild.
    Diff(Diff),
    /// Sizes and times for entries.
    Stats(StatBatch),
    /// The drive has not answered for [`Policy::unresponsive_after`]. What is
    /// shown stays. The next event from the loader means it answered.
    Unresponsive,
    Failed(ListError),
}

/// How to open a folder.
#[derive(Clone, Debug)]
pub struct LoadOptions {
    /// Read the folder even if the cache says it has not changed (F5).
    pub force_refresh: bool,
    /// Show and keep cached listings.
    pub use_cache: bool,
    /// Follow changes: inotify on local disks, a stamp check every
    /// [`Policy::poll_every`] elsewhere. Changes arrive as [`Event::Diff`].
    pub watch: bool,
    /// Override the policy chosen from the mount table.
    pub policy: Option<Policy>,
    /// Use this cache instead of the user's.
    pub cache: Option<Arc<Cache>>,
}

impl Default for LoadOptions {
    fn default() -> Self {
        LoadOptions {
            force_refresh: false,
            use_cache: true,
            watch: true,
            policy: None,
            cache: None,
        }
    }
}

enum Msg {
    Want(u64, Vec<u32>),
    StatAll,
    Refresh,
    Cancel,
    Kind(u64, MountKind),
    Entries(u64, Vec<Entry>),
    ReadDone {
        generation: u64,
        stamp: Option<DirStamp>,
        unchanged: bool,
        result: Result<(), ListError>,
    },
    Stats(u64, Vec<(u32, io::Result<StatInfo>)>),
    Changed,
    Polled(io::Result<DirStamp>),
}

#[derive(Default)]
struct Requested {
    revision: u64,
    bits: Vec<bool>,
}

/// A running load. Dropping it cancels it.
pub struct Load {
    tx: Sender<Msg>,
    cancel: Cancel,
    policy: Arc<Mutex<Policy>>,
    requested: Mutex<Requested>,
}

impl Load {
    /// The policy in force. Until the folder's drive is known (the
    /// [`Event::Kind`] event, about a millisecond away) it is the careful
    /// one for [`MountKind::Other`]: no thumbnails, no reading of content.
    pub fn policy(&self) -> Policy {
        self.policy.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone()
    }

    /// Ask for sizes and times of these entries (indices into `listing`):
    /// the rows in view. Safe to call on every frame: an index is sent once
    /// per [`Listing::revision`]. Newest requests are served first.
    pub fn want_stats(&self, listing: &Listing, indices: &[usize]) {
        let mut requested = self
            .requested
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if requested.revision != listing.revision {
            requested.revision = listing.revision;
            requested.bits.clear();
        }
        let mut fresh = Vec::new();
        for &index in indices {
            if index >= listing.entries.len() {
                continue;
            }
            if index >= requested.bits.len() {
                requested.bits.resize(index + 1, false);
            }
            if !requested.bits[index] {
                requested.bits[index] = true;
                fresh.push(index as u32);
            }
        }
        if !fresh.is_empty() {
            let _ = self.tx.send(Msg::Want(listing.revision, fresh));
        }
    }

    /// Read sizes and times of every entry, for sorting by them on a drive
    /// that is not read that way already.
    pub fn stat_all(&self) {
        let _ = self.tx.send(Msg::StatAll);
    }

    /// Read the folder again now, whatever the cache says.
    pub fn refresh(&self) {
        let _ = self.tx.send(Msg::Refresh);
    }

    /// Stop. Never blocks. Also happens on drop.
    pub fn cancel(&self) {
        self.cancel.cancel();
        let _ = self.tx.send(Msg::Cancel);
    }
}

impl Drop for Load {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Open the folder `path`. `on_event` runs on the loader's thread, in order.
pub fn open(
    path: &Path,
    options: LoadOptions,
    on_event: impl FnMut(Event) + Send + 'static,
) -> Load {
    let policy = Arc::new(Mutex::new(
        options.policy.clone().unwrap_or_else(|| Policy::for_kind(MountKind::Other)),
    ));
    let (tx, rx) = mpsc::channel();
    let cancel = Cancel::new();
    let state = Loader::new(path.to_path_buf(), options, policy.clone(), tx.clone(), cancel.clone(), Box::new(on_event));
    let spawned = thread::Builder::new()
        .name("bingux-files-load".into())
        .spawn(move || state.run(rx));
    if spawned.is_err() {
        cancel.cancel();
    }
    Load {
        tx,
        cancel,
        policy,
        requested: Mutex::new(Requested::default()),
    }
}

// ---- stat workers ------------------------------------------------------------

struct Job {
    revision: u64,
    items: Vec<(u32, PathBuf)>,
}

#[derive(Default)]
struct Queues {
    high: VecDeque<Job>,
    low: VecDeque<Job>,
    closed: bool,
}

#[derive(Default)]
struct StatQueue {
    queues: Mutex<Queues>,
    wake: Condvar,
}

impl StatQueue {
    fn push(&self, job: Job, urgent: bool) {
        let mut queues = self.queues.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if urgent {
            queues.high.push_back(job);
        } else {
            queues.low.push_back(job);
        }
        self.wake.notify_one();
    }

    fn close(&self) {
        let mut queues = self.queues.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        queues.closed = true;
        queues.high.clear();
        queues.low.clear();
        self.wake.notify_all();
    }

    /// The newest urgent job, else the oldest background one. `None` once
    /// closed.
    fn pop(&self) -> Option<Job> {
        let mut queues = self.queues.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        loop {
            if queues.closed {
                return None;
            }
            if let Some(job) = queues.high.pop_back().or_else(|| queues.low.pop_front()) {
                return Some(job);
            }
            queues = self
                .wake
                .wait(queues)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }

    fn is_closed(&self) -> bool {
        self.queues
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .closed
    }
}

fn stat_worker(queue: Arc<StatQueue>, tx: Sender<Msg>) {
    while let Some(job) = queue.pop() {
        let mut results = Vec::with_capacity(16);
        for (index, path) in job.items {
            if queue.is_closed() {
                return;
            }
            results.push((index, stat::stat_entry(&path)));
            if results.len() == 16 {
                let batch = std::mem::replace(&mut results, Vec::with_capacity(16));
                if tx.send(Msg::Stats(job.revision, batch)).is_err() {
                    return;
                }
            }
        }
        if !results.is_empty() && tx.send(Msg::Stats(job.revision, results)).is_err() {
            return;
        }
    }
}

// ---- the loader thread -------------------------------------------------------

#[derive(PartialEq, Eq)]
enum Mode {
    /// Entries are new: forward every batch.
    Append,
    /// The listing shows something: collect the read and send the difference.
    Diff,
}

struct Loader {
    path: PathBuf,
    options: LoadOptions,
    policy: Policy,
    shared_policy: Arc<Mutex<Policy>>,
    tx: Sender<Msg>,
    cancel: Cancel,
    on_event: Box<dyn FnMut(Event) + Send>,
    cache: Option<Arc<Cache>>,

    master: Listing,
    verified: Vec<bool>,
    queued: Vec<bool>,
    stamp: Option<DirStamp>,

    generation: u64,
    reading: bool,
    read_cancel: Cancel,
    mode: Mode,
    fresh: Vec<Entry>,
    reread: bool,
    done_once: bool,
    kind_sent: bool,
    /// The cache file the view was shown, kept to decode the loader's own copy.
    lazy: Option<Vec<u8>>,
    cached_len: usize,

    queue: Arc<StatQueue>,
    workers_started: bool,
    stats_in_flight: usize,

    last_activity: Instant,
    unresponsive_sent: bool,
    gave_up: bool,

    dirty: bool,
    last_stat: Instant,
    watcher: Option<Watcher>,
    polling: bool,
    last_poll: Instant,
}

impl Loader {
    fn new(
        path: PathBuf,
        options: LoadOptions,
        shared_policy: Arc<Mutex<Policy>>,
        tx: Sender<Msg>,
        cancel: Cancel,
        on_event: Box<dyn FnMut(Event) + Send>,
    ) -> Loader {
        let policy = shared_policy.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
        let cache = options.use_cache.then(|| options.cache.clone().unwrap_or_else(Cache::shared));
        let mut master = Listing::new(path.clone());
        master.mount = policy.kind;
        Loader {
            path,
            options,
            policy,
            shared_policy,
            tx,
            cancel,
            on_event,
            cache,
            master,
            verified: Vec::new(),
            queued: Vec::new(),
            stamp: None,
            generation: 0,
            reading: false,
            read_cancel: Cancel::new(),
            mode: Mode::Append,
            fresh: Vec::new(),
            reread: false,
            done_once: false,
            kind_sent: false,
            lazy: None,
            cached_len: 0,
            queue: Arc::new(StatQueue::default()),
            workers_started: false,
            stats_in_flight: 0,
            last_activity: Instant::now(),
            unresponsive_sent: false,
            gave_up: false,
            dirty: false,
            last_stat: Instant::now(),
            watcher: None,
            polling: false,
            last_poll: Instant::now(),
        }
    }

    fn emit(&mut self, event: Event) {
        self.master.apply_event(event.clone());
        (self.on_event)(event);
    }

    /// Whether the listing shows anything yet.
    fn has_content(&self) -> bool {
        self.lazy.is_some() || self.master.from_cache || !self.master.entries.is_empty()
    }

    fn total(&self) -> usize {
        if self.lazy.is_some() { self.cached_len } else { self.master.entries.len() }
    }

    /// Decode the cached entries into the loader's own copy. Done after the
    /// view has been given its copy, and only when something needs it.
    fn ensure_master(&mut self) {
        let Some(bytes) = self.lazy.take() else { return };
        let Some(cache) = &self.cache else { return };
        if let Some(cached) = cache.decode(&bytes, &self.path) {
            self.verified = vec![false; cached.entries.len()];
            self.queued = vec![false; cached.entries.len()];
            self.master.entries = cached.entries;
        }
    }

    fn run(mut self, rx: Receiver<Msg>) {
        // Start the read first: its `statx` of the folder is the slow step
        // on a network mount, and decoding the cache overlaps with it.
        let hint = if self.options.force_refresh {
            None
        } else {
            self.cache.as_ref().and_then(|cache| cache.peek_stamp(&self.path))
        };
        self.start_read(hint);
        if let Some(cache) = self.cache.clone()
            && !self.options.force_refresh
            && let Some(bytes) = cache.read(&self.path)
        {
            match cache.decode(&bytes, &self.path) {
                Some(cached) => {
                    self.stamp = Some(cached.stamp);
                    self.mode = Mode::Diff;
                    self.cached_len = cached.entries.len();
                    // The view takes the entries as they are; the loader
                    // decodes its own copy later, off the way to first paint.
                    self.master.revision += 1;
                    self.master.from_cache = true;
                    (self.on_event)(Event::Cached { entries: cached.entries });
                    self.lazy = Some(bytes);
                }
                None => cache.discard_if_damaged(&self.path, &bytes),
            }
        }
        loop {
            if self.cancel.is_cancelled() {
                break;
            }
            match rx.recv_timeout(self.wait_time()) {
                Ok(Msg::Cancel) | Err(RecvTimeoutError::Disconnected) => break,
                Ok(message) => self.handle(message),
                Err(RecvTimeoutError::Timeout) => {}
            }
            self.tick();
        }
        self.finish();
    }

    fn finish(&mut self) {
        self.read_cancel.cancel();
        self.queue.close();
        self.save_if_dirty();
    }

    fn handle(&mut self, message: Msg) {
        if matches!(message, Msg::Want(..) | Msg::StatAll | Msg::Stats(..)) {
            self.ensure_master();
        }
        match message {
            Msg::Cancel => {}
            Msg::Want(revision, indices) => self.want(revision, indices),
            Msg::StatAll => self.stat_everything(),
            Msg::Refresh => self.restart_read(),
            Msg::Kind(generation, kind) => {
                if generation == self.generation && (!self.kind_sent || kind != self.master.mount) {
                    self.kind_sent = true;
                    self.policy = self.options.policy.clone().unwrap_or_else(|| Policy::for_kind(kind));
                    *self.shared_policy.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = self.policy.clone();
                    self.emit(Event::Kind(kind));
                }
            }
            Msg::Entries(generation, batch) => {
                if generation != self.generation {
                    return;
                }
                self.activity();
                match self.mode {
                    Mode::Append => {
                        self.verified.extend(batch.iter().map(|entry| entry.sized));
                        self.queued.resize(self.verified.len(), false);
                        self.emit(Event::Batch(batch));
                    }
                    Mode::Diff => self.fresh.extend(batch),
                }
            }
            Msg::ReadDone { generation, stamp, unchanged, result } => {
                if generation != self.generation {
                    return;
                }
                self.activity();
                self.reading = false;
                if !unchanged {
                    self.ensure_master();
                }
                self.read_done(stamp, unchanged, result);
            }
            Msg::Stats(revision, results) => self.stats(revision, results),
            Msg::Changed => {
                if self.done_once {
                    self.restart_read();
                }
            }
            Msg::Polled(result) => {
                self.polling = false;
                self.last_poll = Instant::now();
                if let (Ok(now), Some(known)) = (result, self.stamp)
                    && now != known
                    && !self.reading
                {
                    self.restart_read();
                }
            }
        }
    }

    fn activity(&mut self) {
        self.last_activity = Instant::now();
        self.unresponsive_sent = false;
    }

    /// Timers: unresponsive drives, background stores, polling.
    /// How long the loop may sleep before `tick` has something to do. Work in
    /// flight is looked at four times a second, as the timeouts it keeps are
    /// counted in seconds; a finished folder with an inotify watch has nothing
    /// to look at, so it sleeps until a message arrives (cancelling is one,
    /// and the long bound only covers a message lost), and a folder that is
    /// polled sleeps until its next poll. An idle window therefore wakes this
    /// thread about once in five seconds instead of four times a second.
    fn wait_time(&self) -> Duration {
        const BUSY: Duration = Duration::from_millis(250);
        const IDLE: Duration = Duration::from_secs(5);
        let busy = self.reading || self.stats_in_flight > 0 || self.dirty || !self.done_once;
        if busy {
            return BUSY;
        }
        match self.policy.poll_every {
            Some(every) if self.options.watch && self.watcher.is_none() && !self.polling => every
                .saturating_sub(self.last_poll.elapsed())
                .clamp(Duration::from_millis(10), IDLE),
            _ => IDLE,
        }
    }

    fn tick(&mut self) {
        let busy = self.reading || self.stats_in_flight > 0;
        if busy && !self.unresponsive_sent && self.last_activity.elapsed() > self.policy.unresponsive_after {
            self.unresponsive_sent = true;
            self.emit(Event::Unresponsive);
        }
        if self.reading
            && !self.gave_up
            && !self.has_content()
            && self.last_activity.elapsed() > self.policy.give_up_after
        {
            self.gave_up = true;
            self.emit(Event::Failed(ListError::Unresponsive));
        }
        if self.dirty && self.last_stat.elapsed() > Duration::from_millis(750) && self.stats_in_flight == 0 {
            self.save_if_dirty();
        }
        if self.options.watch
            && self.done_once
            && !self.polling
            && !self.reading
            && let Some(every) = self.policy.poll_every
            && self.watcher.is_none()
            && self.last_poll.elapsed() >= every
        {
            self.polling = true;
            let path = self.path.clone();
            let tx = self.tx.clone();
            let _ = thread::Builder::new().name("bingux-files-poll".into()).spawn(move || {
                let _ = tx.send(Msg::Polled(stat::dir_stamp(&path)));
            });
        }
    }

    fn start_read(&mut self, cached_stamp: Option<DirStamp>) {
        self.generation += 1;
        self.reading = true;
        self.activity();
        self.read_cancel.cancel();
        self.read_cancel = Cancel::new();
        self.fresh.clear();
        self.mode = if !self.has_content() {
            Mode::Append
        } else {
            Mode::Diff
        };
        let generation = self.generation;
        let tx = self.tx.clone();
        let path = self.path.clone();
        let cancel = self.read_cancel.clone();
        let guess = self.policy.kind;
        let forced = self.options.policy.clone();
        let spawned = thread::Builder::new().name("bingux-files-read".into()).spawn(move || {
            read_job(&path, generation, cached_stamp, guess, forced, &cancel, &tx);
        });
        if spawned.is_err() {
            self.reading = false;
            self.emit(Event::Failed(ListError::Other("no thread to read the folder".into())));
        }
    }

    /// Read again, showing only the difference.
    fn restart_read(&mut self) {
        if self.reading {
            self.reread = true;
            return;
        }
        self.start_read(None);
    }

    fn read_done(&mut self, stamp: Option<DirStamp>, unchanged: bool, result: Result<(), ListError>) {
        if let Err(error) = result {
            if error != ListError::Cancelled {
                self.emit(Event::Failed(error));
            }
            return;
        }
        if unchanged && !self.has_content() {
            // The cache said "unchanged" but could not be read: read for real.
            self.start_read(None);
            return;
        }
        let mut changed = !unchanged;
        if !unchanged && self.mode == Mode::Diff {
            let fresh = std::mem::take(&mut self.fresh);
            let diff = diff_entries(&self.master.entries, fresh);
            changed = !diff.is_empty();
            if changed {
                self.apply_diff(diff);
            }
        }
        if stamp.is_some() {
            self.stamp = stamp;
        }
        let total = self.total();
        self.emit(Event::Done { total });
        let first = !self.done_once;
        self.done_once = true;
        if let (Some(cache), Some(stamp)) = (&self.cache, &self.stamp)
            && (first && !unchanged || changed)
        {
            cache.store(&self.path, stamp, &self.master.entries);
            self.dirty = false;
        }
        if first {
            self.start_watching();
        }
        if std::mem::take(&mut self.reread) {
            self.start_read(None);
        }
    }

    fn apply_diff(&mut self, diff: Diff) {
        let before: HashMap<String, bool> = if diff.removed.is_empty() {
            HashMap::new()
        } else {
            self.master
                .entries
                .iter()
                .zip(&self.verified)
                .map(|(entry, verified)| (entry.name.clone(), *verified))
                .collect()
        };
        let had_removals = !diff.removed.is_empty();
        let added: Vec<bool> = diff.added.iter().map(|entry| entry.sized).collect();
        let changed_names: HashSet<String> = diff.changed.iter().map(|entry| entry.name.clone()).collect();
        self.emit(Event::Diff(diff));
        if had_removals {
            self.verified = self
                .master
                .entries
                .iter()
                .map(|entry| before.get(&entry.name).copied().unwrap_or(false))
                .collect();
        } else {
            self.verified.extend(added);
        }
        for (entry, verified) in self.master.entries.iter().zip(self.verified.iter_mut()) {
            if changed_names.contains(&entry.name) {
                *verified = entry.sized;
            }
        }
        self.queued = vec![false; self.master.entries.len()];
        self.verified.resize(self.master.entries.len(), false);
    }

    fn start_watching(&mut self) {
        if !self.options.watch || self.policy.poll_every.is_some() {
            return;
        }
        let tx = self.tx.clone();
        self.watcher = Watcher::new(&self.path, move || {
            let _ = tx.send(Msg::Changed);
        })
        .ok();
    }

    fn start_workers(&mut self) {
        if self.workers_started {
            return;
        }
        self.workers_started = true;
        for _ in 0..self.policy.workers.max(1) {
            let queue = self.queue.clone();
            let tx = self.tx.clone();
            let _ = thread::Builder::new()
                .name("bingux-files-stat".into())
                .spawn(move || stat_worker(queue, tx));
        }
    }

    fn want(&mut self, revision: u64, indices: Vec<u32>) {
        if revision != self.master.revision {
            return;
        }
        let mut items: Vec<(u32, PathBuf)> = Vec::with_capacity(indices.len());
        for index in indices {
            let at = index as usize;
            if at < self.master.entries.len() && !self.verified[at] && !self.queued[at] {
                self.queued[at] = true;
                items.push((index, self.master.entries[at].path.clone()));
            }
        }
        if items.is_empty() {
            return;
        }
        self.start_workers();
        self.stats_in_flight += items.len();
        self.last_activity = Instant::now();
        self.queue.push(Job { revision, items }, true);
    }

    fn stat_everything(&mut self) {
        let revision = self.master.revision;
        let mut chunk = Vec::with_capacity(256);
        let mut jobs = Vec::new();
        for (index, entry) in self.master.entries.iter().enumerate() {
            if self.verified[index] || self.queued[index] {
                continue;
            }
            self.queued[index] = true;
            chunk.push((index as u32, entry.path.clone()));
            if chunk.len() == 256 {
                jobs.push(std::mem::replace(&mut chunk, Vec::with_capacity(256)));
            }
        }
        if !chunk.is_empty() {
            jobs.push(chunk);
        }
        if jobs.is_empty() {
            return;
        }
        self.start_workers();
        self.last_activity = Instant::now();
        for items in jobs {
            self.stats_in_flight += items.len();
            self.queue.push(Job { revision, items }, false);
        }
    }

    fn stats(&mut self, revision: u64, results: Vec<(u32, io::Result<StatInfo>)>) {
        self.stats_in_flight = self.stats_in_flight.saturating_sub(results.len());
        self.activity();
        if revision != self.master.revision {
            return;
        }
        let mut updates = Vec::new();
        let mut gone = false;
        for (index, result) in results {
            let at = index as usize;
            if at >= self.master.entries.len() {
                continue;
            }
            self.verified[at] = true;
            self.queued[at] = false;
            match result {
                Ok(info) => {
                    if stat_differs(&self.master.entries[at], &info) {
                        updates.push((index, info));
                    }
                }
                Err(error) => gone |= error.kind() == io::ErrorKind::NotFound,
            }
        }
        if !updates.is_empty() {
            self.dirty = true;
            self.last_stat = Instant::now();
            self.emit(Event::Stats(StatBatch { revision, updates }));
        }
        if gone && self.done_once {
            self.restart_read();
        }
    }

    fn save_if_dirty(&mut self) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        if let (Some(cache), Some(stamp)) = (&self.cache, &self.stamp)
            && self.done_once
        {
            cache.store(&self.path, stamp, &self.master.entries);
        }
    }
}

/// Whether `info` says something the entry does not (access time aside,
/// which changes by being read).
fn stat_differs(entry: &Entry, info: &StatInfo) -> bool {
    !entry.sized
        || entry.kind != info.kind
        || entry.size != info.size
        || entry.modified != info.modified
        || entry.mode != info.mode
        || entry.uid != info.uid
        || entry.gid != info.gid
        || entry.created != info.created
        || entry.pending_link
}

/// Times as the cache keeps them, to the second.
fn whole_seconds(time: Option<std::time::SystemTime>) -> Option<i64> {
    time.map(|time| match time.duration_since(std::time::UNIX_EPOCH) {
        Ok(after) => after.as_secs() as i64,
        Err(before) => -(before.duration().as_secs() as i64),
    })
}

/// What `new` has that `old` does not, and the reverse.
pub(crate) fn diff_entries(old: &[Entry], new: Vec<Entry>) -> Diff {
    let at: HashMap<&str, &Entry> = old.iter().map(|entry| (entry.name.as_str(), entry)).collect();
    let mut seen: HashSet<&str> = HashSet::with_capacity(new.len());
    let mut diff = Diff::default();
    for entry in &new {
        seen.insert(entry.name.as_str());
    }
    for entry in new.iter() {
        match at.get(entry.name.as_str()) {
            None => diff.added.push(entry.clone()),
            Some(known) => {
                // An entry read without `statx` has no size to compare, and
                // must not wipe the one we have.
                let kind_differs = if entry.pending_link && known.symlink {
                    false
                } else {
                    known.kind != entry.kind
                };
                let differs = kind_differs
                    || known.symlink != entry.symlink
                    || known.hidden != entry.hidden
                    || (entry.sized
                        && (known.size != entry.size
                            || whole_seconds(known.modified) != whole_seconds(entry.modified)
                            || (known.mode != 0 && known.mode != entry.mode)
                            || !known.sized));
                if differs {
                    let mut replacement = entry.clone();
                    if !replacement.sized && known.sized && !kind_differs {
                        let keep = (*known).clone();
                        replacement = Entry { hidden: entry.hidden, ..keep };
                    }
                    diff.changed.push(replacement);
                }
            }
        }
    }
    diff.removed = old
        .iter()
        .filter(|entry| !seen.contains(entry.name.as_str()))
        .map(|entry| entry.name.clone())
        .collect();
    diff
}

/// The reader thread: stamp the folder, find out its kind, and read it
/// unless the cache already has it.
fn read_job(
    path: &Path,
    generation: u64,
    cached_stamp: Option<DirStamp>,
    guess: MountKind,
    forced: Option<Policy>,
    cancel: &Cancel,
    tx: &Sender<Msg>,
) {
    let stamp = match stat::dir_stamp(path) {
        Ok(stamp) => stamp,
        Err(error) => {
            let _ = tx.send(Msg::ReadDone {
                generation,
                stamp: None,
                unchanged: false,
                result: Err(ListError::from_io(&error)),
            });
            return;
        }
    };
    // The folder's own device names its mount even through links. Where the
    // table has no such device (btrfs gives each subvolume its own), the
    // path's text is the best left.
    let kind = mounts::kind_for_device(stamp.dev).unwrap_or_else(|| {
        if guess == MountKind::Other { mounts::kind_of(path) } else { guess }
    });
    let _ = tx.send(Msg::Kind(generation, kind));
    if cached_stamp == Some(stamp) {
        let _ = tx.send(Msg::ReadDone {
            generation,
            stamp: Some(stamp),
            unchanged: true,
            result: Ok(()),
        });
        return;
    }
    let policy = forced.unwrap_or_else(|| Policy::for_kind(kind));
    let result = read_folder(path, policy.stat_all, policy.workers, cancel, &mut |batch| {
        !cancel.is_cancelled() && tx.send(Msg::Entries(generation, batch)).is_ok()
    });
    let _ = tx.send(Msg::ReadDone {
        generation,
        stamp: Some(stamp),
        unchanged: false,
        result,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::Kind;
    use std::fs;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bingux-files-loader-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn collect(path: &Path, options: LoadOptions) -> (Load, Receiver<Event>) {
        let (sender, receiver) = mpsc::channel();
        let load = open(path, options, move |event| {
            let _ = sender.send(event);
        });
        (load, receiver)
    }

    /// Events until `Done` (or a failure), applied to a listing.
    fn until_done(receiver: &Receiver<Event>, listing: &mut Listing) -> Vec<Event> {
        let mut seen = Vec::new();
        loop {
            let event = receiver.recv_timeout(Duration::from_secs(10)).expect("no event");
            listing.apply(&event);
            let end = matches!(event, Event::Done { .. } | Event::Failed(_));
            seen.push(event);
            if end {
                return seen;
            }
        }
    }

    /// The loader stores the cache just after `Done`: wait for the file.
    fn wait_for_cache(cache: &Path) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Cache::at(cache, u64::MAX, usize::MAX).disk_usage().1 == 0 {
            assert!(Instant::now() < deadline, "the cache was never written");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn options(cache: &Path) -> LoadOptions {
        LoadOptions {
            cache: Some(Arc::new(Cache::at(cache, crate::cache::DEFAULT_BUDGET, 64))),
            watch: false,
            ..LoadOptions::default()
        }
    }

    fn names(listing: &Listing) -> Vec<String> {
        let mut names: Vec<String> = listing.entries.iter().map(|e| e.name.clone()).collect();
        names.sort();
        names
    }

    #[test]
    fn streams_a_folder_then_serves_it_from_the_cache() {
        let root = scratch("stream");
        let dir = root.join("data");
        fs::create_dir_all(&dir).unwrap();
        for i in 0..300 {
            fs::write(dir.join(format!("file{i:03}.txt")), vec![0u8; i]).unwrap();
        }
        fs::create_dir(dir.join("sub")).unwrap();
        let cache = root.join("cache");

        let (_load, events) = collect(&dir, options(&cache));
        let mut listing = Listing::new(&dir);
        let seen = until_done(&events, &mut listing);
        assert_eq!(seen.first(), Some(&Event::Kind(MountKind::Local)));
        assert_eq!(listing.mount, MountKind::Local);
        assert!(seen.iter().any(|e| matches!(e, Event::Batch(_))));
        assert!(!seen.iter().any(|e| matches!(e, Event::Cached { .. })));
        assert_eq!(listing.entries.len(), 301);
        assert!(listing.complete && !listing.from_cache);
        let file = listing.entries.iter().find(|e| e.name == "file007.txt").unwrap();
        assert_eq!((file.size, file.sized, file.kind), (7, true, Kind::File));

        // Second open: cached entries first, then confirmed unchanged.
        wait_for_cache(&cache);
        let (_load, events) = collect(&dir, options(&cache));
        let mut again = Listing::new(&dir);
        let seen = until_done(&events, &mut again);
        assert!(seen.iter().any(|e| matches!(e, Event::Cached { .. })));
        assert!(!seen.iter().any(|e| matches!(e, Event::Batch(_) | Event::Diff(_))));
        assert_eq!(names(&again), names(&listing));
        assert!(again.complete && !again.from_cache);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_changed_folder_arrives_as_a_diff() {
        let root = scratch("diff");
        let dir = root.join("data");
        fs::create_dir_all(&dir).unwrap();
        for name in ["a.txt", "b.txt", "c.txt"] {
            fs::write(dir.join(name), b"1").unwrap();
        }
        let cache = root.join("cache");
        let (_load, events) = collect(&dir, options(&cache));
        let mut first = Listing::new(&dir);
        until_done(&events, &mut first);
        wait_for_cache(&cache);

        fs::remove_file(dir.join("b.txt")).unwrap();
        fs::write(dir.join("d.txt"), b"1234").unwrap();
        fs::write(dir.join("a.txt"), b"longer now").unwrap();
        std::thread::sleep(Duration::from_millis(20));
        let (_load, events) = collect(&dir, options(&cache));
        let mut second = Listing::new(&dir);
        let seen = until_done(&events, &mut second);
        let diff = seen
            .iter()
            .find_map(|event| match event {
                Event::Diff(diff) => Some(diff.clone()),
                _ => None,
            })
            .expect("a diff");
        assert_eq!(diff.removed, ["b.txt"]);
        assert_eq!(diff.added.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["d.txt"]);
        assert_eq!(diff.changed.iter().map(|e| (e.name.as_str(), e.size)).collect::<Vec<_>>(), [("a.txt", 10)]);
        assert_eq!(names(&second), ["a.txt", "c.txt", "d.txt"]);
        assert_eq!(second.entries.iter().find(|e| e.name == "a.txt").unwrap().size, 10);
        assert_eq!(second.revision, 2);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_network_policy_reads_names_first_and_sizes_for_visible_rows() {
        let root = scratch("lazy");
        let dir = root.join("data");
        fs::create_dir_all(&dir).unwrap();
        for i in 0..40 {
            fs::write(dir.join(format!("f{i:02}")), vec![0u8; 100 + i]).unwrap();
        }
        let mut opts = options(&root.join("cache"));
        opts.use_cache = false;
        opts.policy = Some(Policy {
            poll_every: None,
            ..Policy::for_kind(MountKind::Network)
        });
        let (load, events) = collect(&dir, opts);
        let mut listing = Listing::new(&dir);
        until_done(&events, &mut listing);
        // The mount table says local for a temp dir, but the policy was forced.
        assert_eq!(listing.entries.len(), 40);
        assert!(listing.entries.iter().all(|entry| entry.sized || entry.mode == 0));
        load.want_stats(&listing, &[3, 4, 5]);
        load.want_stats(&listing, &[3, 4, 5]);
        let mut sized = 0;
        while sized < 3 {
            let event = events.recv_timeout(Duration::from_secs(10)).unwrap();
            if let Event::Stats(batch) = &event {
                sized += batch.updates.len();
            }
            listing.apply(&event);
        }
        // Only the asked-for rows were read, each once.
        assert_eq!(sized, 3);
        assert_eq!(listing.entries.iter().filter(|e| e.sized && e.mode != 0).count() >= 3, true);
        load.stat_all();
        let mut all = 0;
        while all < 37 {
            let event = events.recv_timeout(Duration::from_secs(10)).unwrap();
            if let Event::Stats(batch) = &event {
                all += batch.updates.len();
            }
            listing.apply(&event);
        }
        assert!(listing.entries.iter().all(|entry| entry.mode != 0));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn cancelling_stops_the_events() {
        let root = scratch("cancel");
        let dir = root.join("data");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("x"), b"").unwrap();
        let (load, events) = collect(&dir, options(&root.join("cache")));
        load.cancel();
        std::thread::sleep(Duration::from_millis(100));
        drop(load);
        // The channel closes once the loader thread has gone.
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match events.recv_timeout(Duration::from_millis(100)) {
                Err(RecvTimeoutError::Disconnected) => break,
                _ if Instant::now() > deadline => panic!("loader did not stop"),
                _ => {}
            }
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_missing_folder_fails() {
        let root = scratch("missing");
        let (_load, events) = collect(&root.join("nope"), options(&root.join("cache")));
        let mut listing = Listing::new(root.join("nope"));
        let seen = until_done(&events, &mut listing);
        assert!(matches!(seen.last(), Some(Event::Failed(ListError::NotFound))));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn follows_changes_with_the_watcher() {
        let root = scratch("watch");
        let dir = root.join("data");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("one"), b"1").unwrap();
        let mut opts = options(&root.join("cache"));
        opts.watch = true;
        let (_load, events) = collect(&dir, opts);
        let mut listing = Listing::new(&dir);
        until_done(&events, &mut listing);
        std::thread::sleep(Duration::from_millis(100));
        fs::write(dir.join("two"), b"22").unwrap();
        loop {
            let event = events.recv_timeout(Duration::from_secs(10)).expect("a diff");
            listing.apply(&event);
            if matches!(event, Event::Diff(_)) {
                break;
            }
        }
        assert_eq!(names(&listing), ["one", "two"]);
        let _ = fs::remove_dir_all(&root);
    }

    /// A `.hidden` that is a pipe nobody writes to blocks the read like a
    /// dead NFS server would: the loader must say so, keep serving, and let
    /// go of the stuck thread when cancelled.
    #[test]
    fn a_stuck_read_is_reported_and_can_be_abandoned() {
        use std::os::unix::ffi::OsStrExt;
        let root = scratch("stuck");
        let dir = root.join("data");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("file"), b"1").unwrap();
        let pipe = dir.join(".hidden");
        let c_pipe = std::ffi::CString::new(pipe.as_os_str().as_bytes()).unwrap();
        // SAFETY: `c_pipe` is a valid NUL-terminated path.
        assert_eq!(unsafe { libc::mkfifo(c_pipe.as_ptr(), 0o600) }, 0);
        let mut opts = options(&root.join("cache"));
        opts.policy = Some(Policy {
            unresponsive_after: Duration::from_millis(300),
            ..Policy::for_kind(MountKind::Local)
        });
        let (load, events) = collect(&dir, opts);
        let mut listing = Listing::new(&dir);
        let mut saw_unresponsive = false;
        let started = Instant::now();
        while !saw_unresponsive {
            let event = events.recv_timeout(Duration::from_secs(5)).expect("no Unresponsive event");
            saw_unresponsive = event == Event::Unresponsive;
            listing.apply_event(event);
        }
        assert!(started.elapsed() < Duration::from_secs(3));
        assert!(listing.unresponsive && listing.entries.is_empty());
        // Walking away must not wait for the stuck read.
        let before = Instant::now();
        drop(load);
        assert!(before.elapsed() < Duration::from_millis(100));
        // Release the reader so the test leaves no thread behind.
        drop(fs::OpenOptions::new().write(true).open(&pipe).unwrap());
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match events.recv_timeout(Duration::from_millis(100)) {
                Err(RecvTimeoutError::Disconnected) => break,
                _ if Instant::now() > deadline => panic!("loader did not stop"),
                _ => {}
            }
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn diffs_ignore_unsized_rereads() {
        let mut known = Entry::named("/x/a".into(), "a".into(), Kind::File, false, false);
        known.size = 9;
        known.sized = true;
        let unsized_again = Entry::named("/x/a".into(), "a".into(), Kind::File, false, false);
        let added = Entry::named("/x/b".into(), "b".into(), Kind::File, false, false);
        let diff = diff_entries(&[known.clone()], vec![unsized_again, added]);
        assert!(diff.changed.is_empty() && diff.removed.is_empty());
        assert_eq!(diff.added.len(), 1);
    }
}
