//! A background executor for blocking work.
//!
//! Freya runs every component and task on one thread, so anything that waits
//! (a command, a file read, a D-Bus call) must run somewhere else. This module
//! owns that somewhere else: a bounded pool of worker threads and a future that
//! a Freya `spawn` task can `.await`.
//!
//! Waiting is done with a [`Waker`], not a polling loop. A worker finishes a
//! job, stores the value and wakes the awaiting task, and Freya polls that
//! task on the UI thread. When the awaiting task is cancelled (its component
//! unmounted) the future is dropped, and a job that has not started is skipped.
//!
//! Never run blocking work in `render`, in a `use_hook` or `use_memo` body, or
//! directly in an event handler. Use [`spawn_blocking`], [`use_resource`] or the
//! helpers in [`crate::runtime`] instead.

use freya::prelude::*;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::pending;
use std::num::NonZeroUsize;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::Pin;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, LazyLock, Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll, Waker};
use std::thread;

type Job = Box<dyn FnOnce() + Send + 'static>;

/// Enough workers that one slow command does not hold up the rest, few enough
/// that a burst of page loads does not start a storm of processes.
const MIN_WORKERS: usize = 4;
const MAX_WORKERS: usize = 8;

/// The shared queue. Idle workers sleep on `READY`; nothing here polls.
static QUEUE: Mutex<VecDeque<Job>> = Mutex::new(VecDeque::new());
static READY: Condvar = Condvar::new();

/// Starts the workers the first time a job is submitted.
static WORKERS: LazyLock<()> = LazyLock::new(start_workers);

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // A job panic is caught before it can poison anything, so a poisoned lock
    // only means another thread died mid-push; the queue is still usable.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn start_workers() {
    let count = thread::available_parallelism()
        .map(NonZeroUsize::get)
        .unwrap_or(MIN_WORKERS)
        .clamp(MIN_WORKERS, MAX_WORKERS);
    for index in 0..count {
        let spawned = thread::Builder::new()
            .name(format!("bingux-io-{index}"))
            .spawn(worker);
        if let Err(error) = spawned {
            eprintln!("bingux-ui: could not start background worker {index}: {error}");
        }
    }
}

fn worker() {
    loop {
        let job = {
            let mut queue = lock(&QUEUE);
            loop {
                if let Some(job) = queue.pop_front() {
                    break job;
                }
                queue = READY.wait(queue).unwrap_or_else(PoisonError::into_inner);
            }
        };
        // Jobs catch their own panics. This guard keeps the worker alive if one
        // ever does not, so the pool never shrinks silently.
        let _ = catch_unwind(AssertUnwindSafe(job));
    }
}

fn submit(job: Job) {
    LazyLock::force(&WORKERS);
    lock(&QUEUE).push_back(job);
    READY.notify_one();
}

/// A job that panicked on a worker. The worker itself survives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JobPanicked;

enum Slot<T> {
    /// Not finished; holds the waker of the task that awaits it, if polled.
    Pending(Option<Waker>),
    Done(Result<T, JobPanicked>),
    /// The value has been handed to the awaiting task.
    Taken,
}

/// The future of one job. Dropping it before the job starts cancels the job.
struct JobFuture<T> {
    slot: Arc<Mutex<Slot<T>>>,
    cancelled: Arc<AtomicBool>,
}

impl<T> Future for JobFuture<T> {
    type Output = Result<T, JobPanicked>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut slot = lock(&self.slot);
        match std::mem::replace(&mut *slot, Slot::Taken) {
            Slot::Done(outcome) => Poll::Ready(outcome),
            Slot::Pending(_) | Slot::Taken => {
                *slot = Slot::Pending(Some(cx.waker().clone()));
                Poll::Pending
            }
        }
    }
}

impl<T> Drop for JobFuture<T> {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

/// Queue `work` on the pool and return the future of its result.
///
/// The job starts as soon as a worker is free, whether or not the future has
/// been polled. Dropping the future skips the job if it has not started, and
/// discards the value if it has. A panic inside `work` resolves to
/// `Err(JobPanicked)`.
pub fn try_spawn_blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> impl Future<Output = Result<T, JobPanicked>> {
    let slot = Arc::new(Mutex::new(Slot::Pending(None)));
    let cancelled = Arc::new(AtomicBool::new(false));
    let job_slot = Arc::clone(&slot);
    let job_cancelled = Arc::clone(&cancelled);
    submit(Box::new(move || {
        if job_cancelled.load(Ordering::Acquire) {
            return;
        }
        let outcome = catch_unwind(AssertUnwindSafe(work)).map_err(|_| JobPanicked);
        let previous = std::mem::replace(&mut *lock(&job_slot), Slot::Done(outcome));
        if let Slot::Pending(Some(waker)) = previous {
            waker.wake();
        }
    }));
    JobFuture { slot, cancelled }
}

/// Run `work` on the pool and resolve to its value, for a Freya `spawn` task:
///
/// ```ignore
/// spawn(async move {
///     let entries = spawn_blocking(|| std::fs::read_dir("/etc")).await;
///     state.set(Some(entries));
/// });
/// ```
///
/// A panic inside `work` leaves the future pending until its task is
/// cancelled. The default panic hook has already printed the message, and the
/// UI thread does not crash. Use [`try_spawn_blocking`] when the caller must
/// react to a panic.
pub fn spawn_blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> impl Future<Output = T> {
    let job = try_spawn_blocking(work);
    async move {
        match job.await {
            Ok(value) => value,
            Err(JobPanicked) => pending().await,
        }
    }
}

/// What [`use_resource`] shows right now.
#[derive(Clone, Debug, PartialEq)]
pub enum Resource<T> {
    /// Nothing has been loaded yet.
    Loading,
    /// The latest load finished.
    Ready(T),
    /// The latest load failed. The message is fit for an inline error row.
    Error(String),
    /// A value is on screen and a newer one is being loaded.
    Refreshing(T),
}

impl<T> Resource<T> {
    /// The value to show: present when ready, and while refreshing.
    pub fn value(&self) -> Option<&T> {
        match self {
            Resource::Ready(value) | Resource::Refreshing(value) => Some(value),
            Resource::Loading | Resource::Error(_) => None,
        }
    }

    /// The failure message, when the latest load failed.
    pub fn error(&self) -> Option<&str> {
        match self {
            Resource::Error(message) => Some(message),
            Resource::Loading | Resource::Ready(_) | Resource::Refreshing(_) => None,
        }
    }

    /// True before the first load has finished.
    pub fn is_loading(&self) -> bool {
        matches!(self, Resource::Loading)
    }

    /// True while a value is shown and a newer one is on its way.
    pub fn is_refreshing(&self) -> bool {
        matches!(self, Resource::Refreshing(_))
    }
}

/// The handle [`use_resource`] returns. Copy, cheap, valid in closures.
pub struct UseResource<T: 'static> {
    state: State<Resource<T>>,
    generation: State<u64>,
}

impl<T: 'static> Clone for UseResource<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: 'static> Copy for UseResource<T> {}

impl<T: 'static> UseResource<T> {
    /// Read the current state. Calling this in `render` subscribes the
    /// component, so it renders again when the load finishes.
    pub fn with<R>(&self, read: impl FnOnce(&Resource<T>) -> R) -> R {
        read(&self.state.read())
    }

    /// Load again in the background. The value on screen stays until the new
    /// one arrives.
    pub fn refresh(&self) {
        let mut generation = self.generation;
        *generation.write() += 1;
    }
}

/// The load a component started last, kept between renders.
struct Running<K> {
    key: K,
    generation: u64,
    task: TaskHandle,
}

/// Load a value in the background. A new load starts when `key` changes or
/// when [`UseResource::refresh`] is called.
///
/// `load` is a plain function of the key, so everything it needs must be in
/// the key. It runs on the pool, never on the UI thread. The first load shows
/// [`Resource::Loading`]. Later loads keep the old value as
/// [`Resource::Refreshing`]. A load that is still running when its key changes
/// is cancelled, and its result is dropped. Unmounting the component cancels
/// the load.
pub fn use_resource<K, T>(key: K, load: fn(K) -> Result<T, String>) -> UseResource<T>
where
    K: Clone + PartialEq + Send + 'static,
    T: Send + 'static,
{
    let state = use_state(|| Resource::<T>::Loading);
    let generation = use_state(|| 0_u64);
    let running = use_hook(|| Rc::new(RefCell::new(None::<Running<K>>)));
    let current = *generation.read();
    let mut running = running.borrow_mut();
    let stale = running
        .as_ref()
        .is_none_or(|run| run.key != key || run.generation != current);
    if stale {
        if let Some(previous) = running.take() {
            previous.task.try_cancel();
        }
        let task = spawn_load(state, key.clone(), load);
        *running = Some(Running {
            key,
            generation: current,
            task,
        });
    }
    UseResource { state, generation }
}

/// Start one load as a component task. The task marks the state as refreshing
/// before it waits, so the mark is written from an event-loop task and never
/// from `render`.
fn spawn_load<K, T>(
    mut state: State<Resource<T>>,
    key: K,
    load: fn(K) -> Result<T, String>,
) -> TaskHandle
where
    K: Send + 'static,
    T: Send + 'static,
{
    spawn(async move {
        let previous = std::mem::replace(&mut *state.write(), Resource::Loading);
        state.set(match previous {
            Resource::Ready(value) | Resource::Refreshing(value) => Resource::Refreshing(value),
            Resource::Loading | Resource::Error(_) => Resource::Loading,
        });
        let outcome = spawn_blocking(move || load(key)).await;
        state.set(match outcome {
            Ok(value) => Resource::Ready(value),
            Err(message) => Resource::Error(message),
        });
    })
}
