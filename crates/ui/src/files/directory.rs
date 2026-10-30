//! Loading a folder, and keeping it current.
//!
//! [`use_directory`] opens a folder through the model's loader
//! (`bingux_files::loader`). The loader shows the cached listing at once when
//! there is one, reads the folder in the background in batches, and then
//! follows changes (inotify on local drives, a cheap check on network
//! drives). This module turns its events into one [`Load`] state:
//!
//! - nothing is shown until the first entries are in (no flash of an empty
//!   folder), then the list appears and grows while `listing.complete` is
//!   false;
//! - a change in the folder patches the listing; the view is not rebuilt and
//!   the person's place in it stays;
//! - publishing is throttled, so a folder of twenty thousand files makes a
//!   handful of updates, not one per batch.
//! - a read can be stopped ([`Reload::stop`]): the loader gives up the read in
//!   flight and ignores its late answers, and the listing stays as it is. What a
//!   first load had read stays on show, marked `Listing::stopped`; a re-read
//!   leaves the listing untouched. [`Reload::loading`] says a read is going
//!   (the first load, the check of a cached listing, a re-read asked for).
//!
//! The views ask for the sizes and dates of the rows in view through
//! [`Wants`], which this module provides to everything below the component
//! that calls [`use_directory`].

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

use bingux_files::listing::{ListError, Listing};
use bingux_files::loader::{self, Event, LoadOptions};
use bingux_files::places::Places;
use bingux_files::recent;
use freya::prelude::*;

use crate::task::{Resource, spawn_blocking, use_resource};

/// How many recent files the Recent place lists.
const RECENT_LIMIT: usize = 200;
/// The shortest time between two publications of a growing listing.
const PUBLISH_EVERY: Duration = Duration::from_millis(80);
/// The same for sizes and dates arriving, which change nothing the person
/// is looking at.
const STATS_EVERY: Duration = Duration::from_millis(300);

/// Where a view is looking.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Where {
    Folder(PathBuf),
    /// The recently used files.
    Recent,
}

/// What loading a [`Where`] has given so far.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Load {
    Loading,
    /// The entries known so far. While `listing.complete` is false more are
    /// coming, and a `listing.from_cache` listing is waiting to be checked
    /// against the disk.
    Ready(Arc<Listing>),
    Failed(ListError),
}

/// What a loader has produced, and for which place.
#[derive(Clone, Debug, PartialEq)]
struct Loaded {
    place: Where,
    load: Load,
}

/// What a view reads of the loader that follows the place on show. Cheap to
/// copy.
///
/// The loader works in the background and publishes when it has something, so
/// right after the place changes (another folder, another tab) its state is
/// still the previous place's. [`Directory::read`] says what belongs to the
/// place asked for and never anything else: the load of that place, else the
/// last complete listing of it this process had (the view of a tab that comes
/// back, or of a folder shown a moment ago, is whole in its first frame, and
/// the loader checks it against the disk behind it), else
/// [`Load::Loading`].
#[derive(Clone, Copy, PartialEq)]
pub struct Directory {
    state: State<Loaded>,
}

impl Directory {
    /// The load of `place`, subscribing the caller to it.
    pub fn read(&self, place: &Where) -> Load {
        resolve(&self.state.read(), place)
    }

    /// The same without subscribing.
    pub fn peek(&self, place: &Where) -> Load {
        resolve(&self.state.peek(), place)
    }
}

fn resolve(loaded: &Loaded, place: &Where) -> Load {
    if loaded.place == *place && loaded.load != Load::Loading {
        return loaded.load.clone();
    }
    remembered(place).map_or(Load::Loading, Load::Ready)
}

/// How many folders are kept, and how many entries all of them hold. A folder
/// of more entries than that is not kept.
const KEPT_FOLDERS: usize = 16;
const KEPT_ENTRIES: usize = 60_000;

thread_local! {
    /// The last complete listing of the places shown most recently, newest
    /// first. In memory, so a view can start from it in its first frame; the
    /// loader's cache on disk is for the first time a folder is opened.
    static KEPT: RefCell<Vec<(Where, Arc<Listing>)>> = const { RefCell::new(Vec::new()) };
}

/// Keep `listing`, when it is a whole one, as what `place` last looked like.
fn keep(place: &Where, listing: &Arc<Listing>) {
    if !listing.complete || listing.entries.len() > KEPT_ENTRIES {
        return;
    }
    KEPT.with_borrow_mut(|kept| {
        kept.retain(|(kept_place, _)| kept_place != place);
        kept.insert(0, (place.clone(), listing.clone()));
        let mut entries = 0;
        let mut fit = 0;
        for (_, listing) in kept.iter().take(KEPT_FOLDERS) {
            entries += listing.entries.len();
            if entries > KEPT_ENTRIES {
                break;
            }
            fit += 1;
        }
        kept.truncate(fit.max(1));
    });
}

/// What `place` last looked like, if a complete listing of it is kept.
pub(super) fn remembered(place: &Where) -> Option<Arc<Listing>> {
    KEPT.with_borrow(|kept| {
        kept.iter()
            .find(|(kept_place, _)| kept_place == place)
            .map(|(_, listing)| listing.clone())
    })
}

/// The home folder: `$HOME`, or `/` when it is not set.
pub fn home_folder() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn load_places(_: ()) -> Result<Places, String> {
    Ok(Places::load())
}

/// The places of this system, loaded in the background once per window.
pub fn use_places() -> Resource<Places> {
    use_resource((), load_places).with(Clone::clone)
}

/// The loader of the folder on show, for the views below
/// [`use_directory`]: they ask it for the sizes and dates of the rows in view
/// and for a re-read. Cheap to clone; every method does nothing when no
/// folder is loading.
#[derive(Clone, Default)]
pub(super) struct Wants(Rc<RefCell<Option<Arc<loader::Load>>>>);

impl PartialEq for Wants {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Wants {
    /// Ask for the size and date of these entries (indices into `listing`).
    /// Safe to call as often as rows draw: the loader asks for each once.
    pub fn want(&self, listing: &Listing, indices: &[usize]) {
        if let Some(load) = self.0.borrow().as_ref() {
            load.want_stats(listing, indices);
        }
    }

    /// Read the folder again now, as F5 does.
    pub fn refresh(&self) {
        if let Some(load) = self.0.borrow().as_ref() {
            load.refresh();
        }
    }

    /// Stop the read in flight and keep the listing as it is.
    pub fn stop_read(&self) {
        if let Some(load) = self.0.borrow().as_ref() {
            load.stop_read();
        }
    }

    /// Read every entry's size and date, for sorting by them on a network
    /// drive.
    pub fn stat_all(&self) {
        if let Some(load) = self.0.borrow().as_ref() {
            load.stat_all();
        }
    }
}

/// Events from the loader's thread, waiting for the UI task.
#[derive(Default)]
struct Queue {
    events: Mutex<Vec<Event>>,
    signal: Signal,
}

impl Queue {
    fn push(&self, event: Event) {
        self.events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(event);
        self.signal.notify();
    }

    fn take(&self) -> Vec<Event> {
        std::mem::take(&mut *self.events.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

/// A wake-up from a thread to the task that waits on it. A notification that
/// arrives while nobody waits is kept.
#[derive(Default)]
struct Signal {
    state: Mutex<(bool, Option<Waker>)>,
}

impl Signal {
    fn notify(&self) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.0 = true;
        if let Some(waker) = state.1.take() {
            waker.wake();
        }
    }

    fn wait(&self) -> Wait<'_> {
        Wait(self)
    }
}

struct Wait<'a>(&'a Signal);

impl Future for Wait<'_> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let mut state = self.0.state.lock().unwrap_or_else(PoisonError::into_inner);
        if state.0 {
            state.0 = false;
            Poll::Ready(())
        } else {
            state.1 = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

/// Stops the loader when the task that follows it is dropped, because the
/// place changed or the view went away.
struct StopLoader {
    load: Arc<loader::Load>,
    wants: Wants,
}

impl Drop for StopLoader {
    fn drop(&mut self) {
        self.load.cancel();
        let mut slot = self.wants.0.borrow_mut();
        if slot
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, &self.load))
        {
            *slot = None;
        }
    }
}

/// How many folders left half-read are read to their end in the background at
/// once. The oldest is given up past that.
const BACKGROUND_LOADS: usize = 2;

/// How a load in flight is tied to the view that started it. A load is of a
/// place, not of the view: when the view moves to another place while the folder
/// is still being read, the read goes on unseen so its listing can be kept.
#[derive(Clone)]
struct Attach {
    /// Whether the view is on this place. Only then does the load publish and
    /// answer the views below; unseen, it only keeps its listing once whole.
    foreground: Rc<Cell<bool>>,
    /// The loader, once it is open.
    opened: Rc<RefCell<Option<Arc<loader::Load>>>>,
    /// The folder has been read to its end, or could not be.
    finished: Rc<Cell<bool>>,
}

impl Attach {
    /// A load the view is looking at.
    fn shown() -> Self {
        Attach {
            foreground: Rc::new(Cell::new(true)),
            opened: Rc::new(RefCell::new(None)),
            finished: Rc::new(Cell::new(false)),
        }
    }

    /// The view is on this place again: the views below talk to its loader.
    fn show(&self, wants: &Wants) {
        self.foreground.set(true);
        *wants.0.borrow_mut() = self.opened.borrow().clone();
    }

    /// The view has moved on: the load carries on unseen, and the views below
    /// stop asking it for anything.
    fn hide(&self, wants: &Wants) {
        self.foreground.set(false);
        let mut slot = wants.0.borrow_mut();
        let ours = self.opened.borrow();
        if let (Some(current), Some(ours)) = (slot.as_ref(), ours.as_ref())
            && Arc::ptr_eq(current, ours)
        {
            *slot = None;
        }
    }
}

struct Running {
    place: Where,
    task: TaskHandle,
    attach: Attach,
}

/// Re-reads the folder on show, for a Refresh button or a shortcut, and says
/// while it does. Returned by [`use_directory`].
#[derive(Clone, PartialEq)]
pub struct Reload {
    wants: Wants,
    reading: State<bool>,
}

impl Reload {
    /// Read the folder again now, even when the watcher saw no change (a
    /// network or removable drive sends none). The listing is patched, so the
    /// selection and scroll position stay. Does nothing where no folder is
    /// loading, such as Recent.
    pub fn run(&self) {
        self.begin();
        self.wants.refresh();
    }

    /// Show that a re-read has started, for one the file view asked the
    /// loader for itself (F5 or the menu in the list).
    pub fn begin(&self) {
        let mut reading = self.reading;
        if self.wants.0.borrow().is_some() {
            reading.set(true);
        }
    }

    /// Stop reading the folder now (the Stop button, Escape): the read in
    /// flight, a first load or a re-read, ends where it is. The listing on
    /// show stays; what a first load had read stays and says it stopped
    /// (`Listing::stopped`); a re-read leaves the listing untouched. The
    /// folder is still followed, and F5 reads it again. Does nothing when no
    /// read is going.
    pub fn stop(&self) {
        let mut reading = self.reading;
        self.wants.stop_read();
        if *reading.peek() {
            reading.set(false);
        }
    }

    /// Whether the folder on show is being read: its first load, the check of
    /// a listing shown from the cache, or a re-read from F5 or the button. A
    /// change the watcher saw is read without saying so. Subscribes.
    pub fn loading(&self) -> bool {
        *self.reading.read()
    }

    /// [`Reload::loading`] without subscribing, for a key handler.
    pub fn loading_now(&self) -> bool {
        *self.reading.peek()
    }
}

/// Load `place` in the background and keep the result current: the cached
/// listing at once, then the folder as it is read, then every change in it
/// (a file saved, a download finishing) without showing the loading state.
/// Unmounting the component stops the load and the watch.
///
/// Changing `place` lets go of the folder it leaves, unless that folder is not
/// read to its end yet: that read carries on unseen (at most
/// [`BACKGROUND_LOADS`] at once) and keeps the whole listing when it is done,
/// so a tab that comes back to a folder of twenty thousand files it left
/// half-read is whole in its first frame instead of starting over. Coming back
/// while that read is still going takes it up where it is.
///
/// The load it returns is a [`Directory`], which answers for the place asked
/// and starts from the listing this process last had of it, if any. A view
/// that shows a place again (a tab that comes back, a folder opened a moment
/// ago) is therefore whole in its first frame. The loader still runs and
/// checks the folder against the disk; until it has the folder read it
/// publishes nothing that would take rows away from a listing that is shown (a
/// batch of entries is only part of the folder), and it never publishes
/// [`Load::Loading`] over one.
///
/// Call it once per component, from `render`. The components below it can
/// reach the loader through [`Wants`]; the caller re-reads through the
/// returned [`Reload`].
pub fn use_directory(place: &Where) -> (Directory, Reload) {
    let state = use_state(|| Loaded {
        place: place.clone(),
        load: Load::Loading,
    });
    let reading = use_state(|| false);
    let wants = use_provide_context(Wants::default);
    let directory = Directory { state };
    let reload = Reload {
        wants: wants.clone(),
        reading,
    };
    let running = use_hook(|| Rc::new(RefCell::new(None::<Running>)));
    let background = use_hook(|| Rc::new(RefCell::new(Vec::<Running>::new())));
    let mut running = running.borrow_mut();
    if running.as_ref().is_some_and(|run| run.place == *place) {
        return (directory, reload);
    }
    let mut background = background.borrow_mut();
    // A read in the background that reached its end has stopped by itself.
    background.retain(|run| !run.attach.finished.get());
    if let Some(previous) = running.take() {
        previous.attach.hide(&wants);
        if previous.attach.finished.get() {
            previous.task.try_cancel();
        } else {
            background.push(previous);
            while background.len() > BACKGROUND_LOADS {
                background.remove(0).task.try_cancel();
            }
        }
    }
    if let Some(index) = background.iter().position(|run| run.place == *place) {
        let taken_up = background.remove(index);
        let mut reading = reading;
        let finished = taken_up.attach.finished.get();
        spawn(async move { reading.set(!finished) });
        taken_up.attach.show(&wants);
        *running = Some(taken_up);
        return (directory, reload);
    }
    let seeded = matches!(resolve(&state.peek(), place), Load::Ready(_));
    let attach = Attach::shown();
    let task = spawn_folder(place.clone(), state, reading, wants, seeded, attach.clone());
    *running = Some(Running {
        place: place.clone(),
        task,
        attach,
    });
    (directory, reload)
}

fn publish(mut state: State<Loaded>, place: &Where, load: Load) {
    if let Load::Ready(listing) = &load {
        keep(place, listing);
    }
    state.set(Loaded {
        place: place.clone(),
        load,
    });
}

fn spawn_folder(
    place: Where,
    state: State<Loaded>,
    mut reading: State<bool>,
    wants: Wants,
    seeded: bool,
    attach: Attach,
) -> TaskHandle {
    spawn(async move {
        reading.set(matches!(place, Where::Folder(_)));
        match &place {
            Where::Recent => {
                // Nothing to carry on in the background: it is read at once.
                attach.finished.set(true);
                let listing = spawn_blocking(move || {
                    recent::list(&recent::default_path(&home_folder()), RECENT_LIMIT)
                })
                .await;
                publish(state, &place, Load::Ready(Arc::new(listing)));
            }
            Where::Folder(path) => {
                follow(place.clone(), path.clone(), state, reading, wants, seeded, attach).await;
            }
        }
    })
}

/// Follow the loader's events for `path` until the task is dropped, or, for a
/// view that has moved on, until the folder is read to its end.
async fn follow(
    place: Where,
    path: PathBuf,
    state: State<Loaded>,
    mut reading: State<bool>,
    wants: Wants,
    seeded: bool,
    attach: Attach,
) {
    let queue = Arc::new(Queue::default());
    let opened = {
        let queue = queue.clone();
        let path = path.clone();
        spawn_blocking(move || {
            loader::open(&path, LoadOptions::default(), move |event| queue.push(event))
        })
        .await
    };
    let opened = Arc::new(opened);
    *attach.opened.borrow_mut() = Some(opened.clone());
    if attach.foreground.get() {
        *wants.0.borrow_mut() = Some(opened.clone());
    }
    let _stop = StopLoader {
        load: opened,
        wants,
    };

    let mut listing = Listing::new(path);
    // Entries are in: the list may appear. With a listing already on show,
    // only the cache's entries or the whole folder replace it.
    let mut shown = false;
    let mut last_publish: Option<Instant> = None;
    loop {
        queue.signal.wait().await;
        let mut stats_only = true;
        let mut done = false;
        loop {
            // Whatever has come in while waiting is applied together.
            let events = queue.take();
            if events.is_empty() {
                break;
            }
            for event in events {
                match &event {
                    Event::Failed(error) => {
                        // Also over a listing on show: a cached listing of a
                        // folder that has since gone must not stay up as if
                        // the folder were there (an empty one reads "Folder
                        // is empty"), and neither must one that goes away
                        // while it is shown.
                        attach.finished.set(true);
                        if attach.foreground.get() {
                            reading.set(false);
                            publish(state, &place, Load::Failed(error.clone()));
                        }
                        return;
                    }
                    Event::Cached { .. } | Event::Done { .. } => {
                        shown = true;
                        stats_only = false;
                        done |= matches!(event, Event::Done { .. });
                    }
                    Event::Batch(_) => {
                        shown |= !seeded;
                        stats_only = false;
                    }
                    Event::Diff(_) => stats_only = false,
                    Event::Stopped => {
                        // What was read stays on show, and says it was
                        // stopped. Nothing follows to finish the read, so a
                        // view that moves on lets go of the load.
                        shown |= !seeded;
                        stats_only = false;
                        attach.finished.set(true);
                        if attach.foreground.get() {
                            reading.set(false);
                        }
                    }
                    Event::Kind(_) | Event::Unresponsive | Event::Stats(_) => {}
                }
                if matches!(event, Event::Unresponsive) {
                    listing.unresponsive = true;
                }
                listing.apply_event(event);
            }
            // Give a burst of batches a moment to collect.
            let wait = last_publish.map(|at| {
                (if stats_only { STATS_EVERY } else { PUBLISH_EVERY }).saturating_sub(at.elapsed())
            });
            match wait {
                Some(wait) if !wait.is_zero() && !done => timer(wait).await,
                _ => break,
            }
        }
        if shown {
            last_publish = Some(Instant::now());
            if attach.foreground.get() {
                publish(state, &place, Load::Ready(Arc::new(listing.clone())));
            } else if done {
                // Nobody is looking: the whole folder is all there is to keep.
                keep(&place, &Arc::new(listing.clone()));
            }
        }
        if done {
            attach.finished.set(true);
            if attach.foreground.get() {
                reading.set(false);
            } else {
                // The read is over and the view is elsewhere: stop watching.
                return;
            }
        }
    }
}

/// Forget every kept listing (tests start from nothing). Below the last public
/// item: the coverage check reads everything after a `#[cfg(test)]` as tests.
#[cfg(test)]
pub(super) fn forget_kept() {
    KEPT.with_borrow_mut(Vec::clear);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listing(path: &str, entries: usize, complete: bool) -> Arc<Listing> {
        let mut listing = Listing::new(path);
        for index in 0..entries {
            listing.entries.push(bingux_files::Entry::named(
                PathBuf::from(path).join(format!("f{index}")),
                format!("f{index}"),
                bingux_files::Kind::File,
                false,
                false,
            ));
        }
        listing.complete = complete;
        Arc::new(listing)
    }

    fn folder(path: &str) -> Where {
        Where::Folder(PathBuf::from(path))
    }

    #[test]
    fn the_load_of_another_place_is_never_shown_for_this_one() {
        forget_kept();
        let loaded = Loaded {
            place: folder("/a"),
            load: Load::Ready(listing("/a", 3, true)),
        };
        assert_eq!(resolve(&loaded, &folder("/a")), loaded.load);
        assert_eq!(resolve(&loaded, &folder("/b")), Load::Loading, "nothing is known of /b yet");
        let failed = Loaded {
            place: folder("/a"),
            load: Load::Failed(ListError::NotFound),
        };
        assert_eq!(resolve(&failed, &folder("/b")), Load::Loading, "/a's failure is not /b's");
    }

    #[test]
    fn a_place_shown_before_starts_from_its_last_whole_listing() {
        forget_kept();
        let whole = listing("/b", 5, true);
        keep(&folder("/b"), &whole);
        let loaded = Loaded {
            place: folder("/a"),
            load: Load::Ready(listing("/a", 3, true)),
        };
        assert_eq!(resolve(&loaded, &folder("/b")), Load::Ready(whole.clone()));
        // A loader that has published nothing for the place yet does not hide it.
        let starting = Loaded {
            place: folder("/b"),
            load: Load::Loading,
        };
        assert_eq!(resolve(&starting, &folder("/b")), Load::Ready(whole));
    }

    #[test]
    fn a_listing_that_is_still_coming_is_not_kept() {
        forget_kept();
        keep(&folder("/part"), &listing("/part", 5, false));
        assert!(remembered(&folder("/part")).is_none(), "a batch of entries is not the folder");
    }

    #[test]
    fn the_newest_listing_of_a_place_replaces_the_older_one() {
        forget_kept();
        keep(&folder("/a"), &listing("/a", 2, true));
        let newer = listing("/a", 4, true);
        keep(&folder("/a"), &newer);
        assert_eq!(remembered(&folder("/a")), Some(newer));
    }

    #[test]
    fn only_so_many_folders_and_entries_are_kept_and_the_oldest_go_first() {
        forget_kept();
        for index in 0..=KEPT_FOLDERS {
            keep(&folder(&format!("/f{index}")), &listing(&format!("/f{index}"), 1, true));
        }
        assert!(remembered(&folder("/f0")).is_none(), "past the number of folders, the oldest goes");
        assert!(remembered(&folder(&format!("/f{KEPT_FOLDERS}"))).is_some());

        forget_kept();
        let half = KEPT_ENTRIES / 2 + 1;
        keep(&folder("/old"), &listing("/old", half, true));
        keep(&folder("/new"), &listing("/new", half, true));
        assert!(remembered(&folder("/old")).is_none(), "past the number of entries, the oldest goes");
        assert!(remembered(&folder("/new")).is_some());

        forget_kept();
        keep(&folder("/huge"), &listing("/huge", KEPT_ENTRIES + 1, true));
        assert!(remembered(&folder("/huge")).is_none(), "a folder too big for the memory is not kept");
    }

    #[test]
    fn the_recent_files_are_kept_like_a_folder() {
        forget_kept();
        let mut recent = Listing::new(PathBuf::new());
        recent.recent = true;
        recent.complete = true;
        let recent = Arc::new(recent);
        keep(&Where::Recent, &recent);
        assert_eq!(remembered(&Where::Recent), Some(recent));
        assert!(remembered(&folder("/a")).is_none());
    }
}
