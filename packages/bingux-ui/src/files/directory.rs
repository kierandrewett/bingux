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
//!
//! The views ask for the sizes and dates of the rows in view through
//! [`Wants`], which this module provides to everything below the component
//! that calls [`use_directory`].

use std::cell::RefCell;
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

struct Running {
    place: Where,
    task: TaskHandle,
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

    /// Whether a re-read asked for through [`Reload::run`] is still going.
    pub fn busy(&self) -> bool {
        *self.reading.read()
    }
}

/// Load `place` in the background and keep the result current: the cached
/// listing at once, then the folder as it is read, then every change in it
/// (a file saved, a download finishing) without showing the loading state.
/// Changing `place` stops the load in flight. Unmounting the component stops
/// the load and the watch.
///
/// Call it once per component, from `render`. The components below it can
/// reach the loader through [`Wants`]; the caller re-reads through the
/// returned [`Reload`].
pub fn use_directory(place: &Where) -> (State<Load>, Reload) {
    let load = use_state(|| Load::Loading);
    let reading = use_state(|| false);
    let wants = use_provide_context(Wants::default);
    let reload = Reload {
        wants: wants.clone(),
        reading,
    };
    let running = use_hook(|| Rc::new(RefCell::new(None::<Running>)));
    let mut running = running.borrow_mut();
    if running.as_ref().is_some_and(|run| run.place == *place) {
        return (load, reload);
    }
    if let Some(previous) = running.take() {
        previous.task.try_cancel();
    }
    let task = spawn_loading(place.clone(), load, reading, wants);
    *running = Some(Running {
        place: place.clone(),
        task,
    });
    (load, reload)
}

fn spawn_loading(
    place: Where,
    mut load: State<Load>,
    mut reading: State<bool>,
    wants: Wants,
) -> TaskHandle {
    spawn(async move {
        reading.set(false);
        load.set(Load::Loading);
        match place {
            Where::Recent => {
                let listing = spawn_blocking(move || {
                    recent::list(&recent::default_path(&home_folder()), RECENT_LIMIT)
                })
                .await;
                load.set(Load::Ready(Arc::new(listing)));
            }
            Where::Folder(path) => follow_folder(path, load, reading, wants).await,
        }
    })
}

/// Follow the loader's events for `path` until the task is dropped.
async fn follow_folder(
    path: PathBuf,
    mut load: State<Load>,
    mut reading: State<bool>,
    wants: Wants,
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
    *wants.0.borrow_mut() = Some(opened.clone());
    let _stop = StopLoader {
        load: opened,
        wants,
    };

    let mut listing = Listing::new(path);
    // Entries are in: the list may appear.
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
                        if !shown {
                            reading.set(false);
                            load.set(Load::Failed(error.clone()));
                            return;
                        }
                    }
                    Event::Cached { .. } | Event::Batch(_) | Event::Done { .. } => {
                        shown = true;
                        stats_only = false;
                        done |= matches!(event, Event::Done { .. });
                    }
                    Event::Diff(_) => stats_only = false,
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
            load.set(Load::Ready(Arc::new(listing.clone())));
        }
        if done {
            reading.set(false);
        }
    }
}
