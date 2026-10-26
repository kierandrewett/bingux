//! Loading a folder, and keeping it current.

use std::cell::RefCell;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Context, Poll, Waker};

use bingux_files::listing::{Cancel, ListError, Listing, list_dir};
use bingux_files::places::Places;
use bingux_files::recent;
use bingux_files::watch::Watcher;
use freya::prelude::*;

use crate::task::{Resource, spawn_blocking, use_resource};

/// How many recent files the Recent place lists.
const RECENT_LIMIT: usize = 200;

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

/// A wake-up from the thread that watches a folder to the task that reloads
/// it. A notification that arrives while nobody waits is kept.
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

    fn wait(self: &Arc<Self>) -> impl Future<Output = ()> + use<> {
        Wait(self.clone())
    }
}

struct Wait(Arc<Signal>);

impl Future for Wait {
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

/// Sets the flag when dropped: a listing whose task was cancelled stops
/// reading its folder.
struct CancelOnDrop(Cancel);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

struct Running {
    place: Where,
    reloads: u64,
    listing: TaskHandle,
    watch: Option<TaskHandle>,
}

/// Load `place` in the background and keep the result current: a change in
/// the folder (a file saved, a download finishing) lists it again, without
/// showing the loading state. Changing `place` cancels the listing in flight.
/// Unmounting the component stops the load and the watch.
///
/// Call it once per component, from `render`.
pub fn use_directory(place: &Where) -> State<Load> {
    let load = use_state(|| Load::Loading);
    let reloads = use_state(|| 0_u64);
    let running = use_hook(|| Rc::new(RefCell::new(None::<Running>)));
    let current = *reloads.read();
    let mut running = running.borrow_mut();
    let same_place = running.as_ref().is_some_and(|run| run.place == *place);
    let stale = running
        .as_ref()
        .is_none_or(|run| run.place != *place || run.reloads != current);
    if !stale {
        return load;
    }
    let mut previous = running.take();
    if let Some(previous) = &mut previous {
        previous.listing.try_cancel();
    }
    // The watch follows the place, not each reload.
    let watch = if same_place {
        previous.and_then(|run| run.watch)
    } else {
        if let Some(watch) = previous.and_then(|run| run.watch) {
            watch.try_cancel();
        }
        match place {
            Where::Folder(path) => Some(spawn_watch(path.clone(), reloads)),
            Where::Recent => None,
        }
    };
    let listing = spawn_listing(place.clone(), load, same_place);
    *running = Some(Running {
        place: place.clone(),
        reloads: current,
        listing,
        watch,
    });
    load
}

fn spawn_listing(place: Where, mut load: State<Load>, keep_shown: bool) -> TaskHandle {
    spawn(async move {
        if !keep_shown {
            load.set(Load::Loading);
        }
        let cancel = Cancel::new();
        let _stop = CancelOnDrop(cancel.clone());
        let outcome = spawn_blocking(move || match place {
            Where::Folder(path) => list_dir(&path, &cancel),
            Where::Recent => Ok(recent::list(
                &recent::default_path(&home_folder()),
                RECENT_LIMIT,
            )),
        })
        .await;
        match outcome {
            Ok(listing) => load.set(Load::Ready(Arc::new(listing))),
            Err(ListError::Cancelled) => {}
            Err(error) => load.set(Load::Failed(error)),
        }
    })
}

/// Watch `path` and count a reload each time it changes. The folder may be
/// one inotify cannot watch (a permission, a limit); then the view simply
/// does not follow changes and the person can reopen it.
fn spawn_watch(path: PathBuf, mut reloads: State<u64>) -> TaskHandle {
    spawn(async move {
        let signal = Arc::new(Signal::default());
        let notifier = signal.clone();
        let Ok(_watcher) = Watcher::new(Path::new(&path), move || notifier.notify()) else {
            return;
        };
        loop {
            signal.wait().await;
            *reloads.write() += 1;
        }
    })
}
