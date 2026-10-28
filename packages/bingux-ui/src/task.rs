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
            // A page is sliding: hold the result back until it has settled, so
            // data does not re-render the page mid-slide.
            Slot::Done(outcome) if sliding() => {
                *slot = Slot::Done(outcome);
                defer_until_settled(cx.waker().clone());
                Poll::Pending
            }
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

thread_local! {
    /// When the running slide began, on the UI thread. `None` at rest.
    static SLIDE_SINCE: std::cell::Cell<Option<std::time::Instant>> = const { std::cell::Cell::new(None) };
    /// The tasks waiting for the slide to settle.
    static SLIDE_WAITERS: RefCell<Vec<Waker>> = const { RefCell::new(Vec::new()) };
    /// Jobs submitted from the UI thread during the slide: they start when it
    /// settles, so a command's process spawn and its parsing do not compete
    /// with the slide for the CPU.
    static SLIDE_JOBS: RefCell<Vec<Job>> = const { RefCell::new(Vec::new()) };
}

/// The longest a result is held back for a slide. A slide that never reports
/// its end must not starve the page of its data.
const SLIDE_HOLD: std::time::Duration = std::time::Duration::from_millis(1500);

fn sliding() -> bool {
    SLIDE_SINCE.with(|since| since.get().is_some_and(|at| at.elapsed() < SLIDE_HOLD))
}

fn defer_until_settled(waker: Waker) {
    SLIDE_WAITERS.with(|waiters| waiters.borrow_mut().push(waker));
}

/// Start `job` now, or when the running slide has settled if one is running on
/// this (the UI) thread. Threads of the pool and others never see a slide and
/// start their jobs at once.
fn submit_after_slide(job: Job) {
    if sliding() {
        SLIDE_JOBS.with(|jobs| jobs.borrow_mut().push(job));
    } else {
        submit(job);
    }
}

/// Tell the task pool whether a page slide is running. Call from the render
/// that owns the slide, with `true` while it runs and `false` once it has
/// settled (every render may call it: only a change does anything). While it
/// is true, jobs submitted from the UI thread are queued but not started, and
/// the results of jobs already running wait instead of being delivered. When
/// it turns false the queued jobs start and the held results are delivered
/// together. A slide that never reports its end is over after `SLIDE_HOLD`.
pub fn set_sliding(active: bool) {
    let was = SLIDE_SINCE.with(|since| since.get().is_some());
    // A slide that has outlasted SLIDE_HOLD without reporting its end is over.
    let expired = was && !sliding();
    if active == was && !expired {
        return;
    }
    let active = active && !expired;
    SLIDE_SINCE.with(|since| since.set(active.then(std::time::Instant::now)));
    if !active {
        let jobs = SLIDE_JOBS.with(|jobs| std::mem::take(&mut *jobs.borrow_mut()));
        for job in jobs {
            submit(job);
        }
        let waiters = SLIDE_WAITERS.with(|waiters| std::mem::take(&mut *waiters.borrow_mut()));
        for waker in waiters {
            waker.wake();
        }
    }
}

/// A cancellation token for a named job. The job's closure gets one and
/// checks [`CancelToken::is_cancelled`] between steps (a loop over many
/// files, a series of commands), so work whose page has gone stops early.
#[derive(Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
}

thread_local! {
    /// The name of the job this worker is running, for panic reports and
    /// the watchdog's view of the pool.
    static CURRENT_JOB: std::cell::Cell<&'static str> = const { std::cell::Cell::new("") };
}

/// The name of the job running on this thread, empty off the pool.
pub fn current_job() -> &'static str {
    CURRENT_JOB.with(std::cell::Cell::get)
}

/// Like [`spawn_blocking`], for a job with a name and a cancellation token.
/// The name appears in the panic message and in watchdog reports; dropping the
/// returned future cancels the token, and a job that has not started is
/// skipped. A panic in `work` leaves the future pending, as with
/// [`spawn_blocking`].
pub fn spawn_named<T: Send + 'static>(
    name: &'static str,
    work: impl FnOnce(&CancelToken) -> T + Send + 'static,
) -> impl Future<Output = T> {
    let token = CancelToken::default();
    let guard = token.clone();
    let job = try_spawn_blocking(move || {
        CURRENT_JOB.with(|current| current.set(name));
        let result = catch_unwind(AssertUnwindSafe(|| work(&token)));
        CURRENT_JOB.with(|current| current.set(""));
        match result {
            Ok(value) => value,
            Err(panic) => {
                eprintln!("bingux-ui: background job `{name}` panicked");
                std::panic::resume_unwind(panic)
            }
        }
    });
    async move {
        struct Cancel(CancelToken);
        impl Drop for Cancel {
            fn drop(&mut self) {
                self.0.cancel();
            }
        }
        let _cancel = Cancel(guard);
        match job.await {
            Ok(value) => value,
            Err(JobPanicked) => pending().await,
        }
    }
}

/// Testing builds only: `BINGUX_UI_RESULT_DELAY_MS=N` holds back the result of
/// every background job for N milliseconds after the work has run, so a page
/// stays in its loading state long enough to capture the frames before the
/// data lands. The job's own work is not delayed, only its delivery.
#[cfg(feature = "testing")]
fn result_delay() -> Option<std::time::Duration> {
    static DELAY: LazyLock<Option<std::time::Duration>> = LazyLock::new(|| {
        std::env::var("BINGUX_UI_RESULT_DELAY_MS")
            .ok()?
            .parse::<u64>()
            .ok()
            .filter(|ms| *ms > 0)
            .map(std::time::Duration::from_millis)
    });
    *DELAY
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
    submit_after_slide(Box::new(move || {
        if job_cancelled.load(Ordering::Acquire) {
            return;
        }
        let outcome = catch_unwind(AssertUnwindSafe(work)).map_err(|_| JobPanicked);
        let deliver = move || {
            let previous = std::mem::replace(&mut *lock(&job_slot), Slot::Done(outcome));
            if let Slot::Pending(Some(waker)) = previous {
                waker.wake();
            }
        };
        #[cfg(feature = "testing")]
        if let Some(delay) = result_delay() {
            thread::spawn(move || {
                thread::sleep(delay);
                deliver();
            });
            return;
        }
        deliver();
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
    resource_with(key, load, None)
}

/// [`use_resource`] that also reads again every `every` while the window has
/// focus, and writes only when the new value is not `same` as the one on
/// screen. A read that finds nothing new makes no state change, so it redraws
/// nothing; the state never passes through [`Resource::Refreshing`] for it.
/// `same` is a comparison of what the page draws, so a field that changes on
/// every read without showing (a timestamp) can be left out of it.
/// [`UseResource::refresh`] still reloads at once, with the mark.
pub fn use_polled_resource<K, T>(
    key: K,
    load: fn(K) -> Result<T, String>,
    every: std::time::Duration,
    same: fn(&T, &T) -> bool,
) -> UseResource<T>
where
    K: Clone + PartialEq + Send + 'static,
    T: Send + 'static,
{
    resource_with(key, load, Some(Repeat { every, same }))
}

/// How a polled resource reads again.
struct Repeat<T> {
    every: std::time::Duration,
    /// Whether two values are the same, so the state is not rewritten.
    same: fn(&T, &T) -> bool,
}

impl<T> Clone for Repeat<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Repeat<T> {}

fn resource_with<K, T>(
    key: K,
    load: fn(K) -> Result<T, String>,
    repeat: Option<Repeat<T>>,
) -> UseResource<T>
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
        let task = spawn_load(state, key.clone(), load, repeat);
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
/// from `render`. With `repeat` it then reads again on a timer while the
/// window has focus, in the same task, so a new key or an unmount ends the
/// polling with the load.
fn spawn_load<K, T>(
    mut state: State<Resource<T>>,
    key: K,
    load: fn(K) -> Result<T, String>,
    repeat: Option<Repeat<T>>,
) -> TaskHandle
where
    K: Clone + Send + 'static,
    T: Send + 'static,
{
    spawn(async move {
        let previous = std::mem::replace(&mut *state.write(), Resource::Loading);
        state.set(match previous {
            Resource::Ready(value) | Resource::Refreshing(value) => Resource::Refreshing(value),
            Resource::Loading | Resource::Error(_) => Resource::Loading,
        });
        let first = key.clone();
        let outcome = spawn_blocking(move || load(first)).await;
        state.set(match outcome {
            Ok(value) => Resource::Ready(value),
            Err(message) => Resource::Error(message),
        });
        let Some(repeat) = repeat else { return };
        let focus = Platform::get().current_window().is_app_focused;
        loop {
            timer(repeat.every).await;
            if !*focus.peek() {
                continue;
            }
            let again = key.clone();
            let outcome = spawn_blocking(move || load(again)).await;
            let unchanged = match (&outcome, &*state.peek()) {
                (Ok(new), Resource::Ready(old) | Resource::Refreshing(old)) => (repeat.same)(old, new),
                (Err(new), Resource::Error(old)) => new == old,
                _ => false,
            };
            if !unchanged {
                state.set(match outcome {
                    Ok(value) => Resource::Ready(value),
                    Err(message) => Resource::Error(message),
                });
            }
        }
    })
}

/// Run `work` on the pool and do not wait for it: for a job whose effect is
/// elsewhere (a search service loading its index, a snapshot being written).
/// Unlike [`spawn_blocking`] there is no future to drop, so the job always
/// runs. A panic is caught and reported; the worker survives.
pub fn spawn_detached(name: &'static str, work: impl FnOnce() + Send + 'static) {
    submit(Box::new(move || {
        CURRENT_JOB.with(|current| current.set(name));
        if catch_unwind(AssertUnwindSafe(work)).is_err() {
            eprintln!("bingux-ui: background job `{name}` panicked");
        }
        CURRENT_JOB.with(|current| current.set(""));
    }));
}

/// Values sent from a thread to a Freya task, which sleeps until one comes.
///
/// A thread that reads a stream (`gsettings monitor`, an inotify descriptor)
/// [`push`](Feed::push)es what it reads; the task `.await`s
/// [`next`](Feed::next). The thread wakes the task, so no timer polls the
/// channel and an idle window stays idle. [`close`](Feed::close) ends the
/// stream: `next` returns `None` once the queue is empty.
pub struct Feed<T> {
    state: Mutex<FeedState<T>>,
}

struct FeedState<T> {
    queue: VecDeque<T>,
    waker: Option<Waker>,
    closed: bool,
}

impl<T> Default for Feed<T> {
    fn default() -> Self {
        Self {
            state: Mutex::new(FeedState {
                queue: VecDeque::new(),
                waker: None,
                closed: false,
            }),
        }
    }
}

impl<T> Feed<T> {
    pub fn push(&self, value: T) {
        let mut state = lock(&self.state);
        state.queue.push_back(value);
        if let Some(waker) = state.waker.take() {
            waker.wake();
        }
    }

    pub fn close(&self) {
        let mut state = lock(&self.state);
        state.closed = true;
        if let Some(waker) = state.waker.take() {
            waker.wake();
        }
    }

    /// The next value, waiting for one; `None` when the feed is closed and
    /// drained.
    pub async fn next(&self) -> Option<T> {
        std::future::poll_fn(|cx| {
            let mut state = lock(&self.state);
            match state.queue.pop_front() {
                Some(value) => Poll::Ready(Some(value)),
                None if state.closed => Poll::Ready(None),
                None => {
                    state.waker = Some(cx.waker().clone());
                    Poll::Pending
                }
            }
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::time::Duration;

    /// A job submitted during a slide does not start until the slide settles,
    /// and then starts at once. Jobs submitted off a slide start at once.
    #[test]
    fn jobs_wait_for_the_slide() {
        let started = Arc::new(AtomicUsize::new(0));
        let count = Arc::clone(&started);
        set_sliding(true);
        let held = try_spawn_blocking(move || count.fetch_add(1, Ordering::SeqCst));
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(started.load(Ordering::SeqCst), 0, "the job started mid-slide");
        set_sliding(false);
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while started.load(Ordering::SeqCst) == 0 {
            assert!(std::time::Instant::now() < deadline, "the job never started after the slide");
            std::thread::sleep(Duration::from_millis(5));
        }
        let count = Arc::clone(&started);
        let free = try_spawn_blocking(move || count.fetch_add(1, Ordering::SeqCst));
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while started.load(Ordering::SeqCst) < 2 {
            assert!(std::time::Instant::now() < deadline, "the job did not start at rest");
            std::thread::sleep(Duration::from_millis(5));
        }
        drop((held, free));
    }
}
