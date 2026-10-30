//! The ring buffer of recent failures, readable from anywhere.
//!
//! Every failure the runtime catches (a component's render, an event handler,
//! an effect, a task, a worker thread, the framework) lands here, from any
//! thread. The inspector's Performance tab lists it, Spotlight and bug reports
//! read it with [`recent_errors`] and [`report_text`], and worker-thread
//! failures also reach the window as a toast.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use freya_core::guard::{Failure, FailureKind};

use crate::task::Feed;

/// How many failures are kept. Older ones fall off the back.
const CAPACITY: usize = 64;

/// One caught failure.
#[derive(Clone, Debug, PartialEq)]
pub struct ErrorRecord {
    /// Counts up from 1 in the order failures were recorded.
    pub id: u64,
    /// When the failure was first recorded, in seconds since the Unix epoch.
    pub at: u64,
    pub kind: FailureKind,
    /// The failing component's short name, or the thread's name.
    pub component: String,
    /// Short component names from the window root down to the failing one.
    pub path: Vec<String>,
    pub message: String,
    /// `file:line:column` of the `panic!`.
    pub location: Option<String>,
    /// Present when `RUST_BACKTRACE` asked for one.
    pub backtrace: Option<String>,
    pub thread: String,
    /// How many times the same failure came in a row; the record is updated,
    /// not repeated.
    pub repeats: u32,
}

impl ErrorRecord {
    /// The record as plain text, for a clipboard or a bug report.
    pub fn details(&self) -> String {
        let mut text = format!(
            "{} failed: {}\ncomponent: {}\npath: {}\n",
            self.kind.label(),
            self.message,
            if self.component.is_empty() { "-" } else { &self.component },
            if self.path.is_empty() { "-".to_string() } else { self.path.join(" > ") },
        );
        if let Some(location) = &self.location {
            text.push_str(&format!("at: {location}\n"));
        }
        text.push_str(&format!("thread: {}\n", self.thread));
        if self.repeats > 1 {
            text.push_str(&format!("repeated: {} times\n", self.repeats));
        }
        match &self.backtrace {
            Some(backtrace) => text.push_str(&format!("backtrace:\n{backtrace}\n")),
            None => text.push_str("backtrace: not captured (run with RUST_BACKTRACE=1)\n"),
        }
        text
    }
}

static RING: Mutex<VecDeque<ErrorRecord>> = Mutex::new(VecDeque::new());
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Failures from threads other than the UI thread, waiting for the window to
/// show them. The window's task sleeps on it; nothing polls.
static THREAD_FAILURES: LazyLock<Feed<Arc<ErrorRecord>>> = LazyLock::new(Feed::default);

/// Set once a window's task listens to [`THREAD_FAILURES`]; before that (a
/// headless test, a tool) nothing is queued for a window that is not there.
static LISTENING: AtomicBool = AtomicBool::new(false);

fn ring() -> MutexGuard<'static, VecDeque<ErrorRecord>> {
    // Nothing panics while the ring is held, so a poisoned lock is still whole.
    RING.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The names a reader of a component path never needs: the plumbing every
/// window has.
const PLUMBING: &[&str] = &[
    "ErrorBoundary",
    "BoundaryBody",
    "AppComponent",
    "OverlayHost",
    "WindowHost",
    "root",
];

/// The part of a type name a person recognises: `bingux_ui::shell::PageFrame`
/// is `PageFrame`, and `Group<T>` is `Group`.
pub fn short_name(type_name: &str) -> String {
    let base = type_name.split('<').next().unwrap_or(type_name);
    let mut segments: Vec<&str> = base.split("::").collect();
    let last = segments.pop().unwrap_or(base);
    if last.starts_with("{{")
        && let Some(owner) = segments.pop()
    {
        return format!("{owner}::{last}");
    }
    last.to_string()
}

/// A component path as people read it: `Window › Page › Switches`.
pub fn display_path(path: &[&str]) -> String {
    path.iter()
        .map(|name| short_name(name))
        .filter(|name| !PLUMBING.contains(&name.as_str()))
        .collect::<Vec<_>>()
        .join(" › ")
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

/// Record a failure. Called by the runtime's observer, from any thread.
pub(crate) fn record(failure: &Failure) {
    let component = if failure.kind == FailureKind::Thread {
        // The hook runs on the panicking thread, so the pool's current job is
        // the one that failed.
        match crate::task::current_job() {
            "" => failure.thread.clone(),
            job => format!("{job} ({})", failure.thread),
        }
    } else {
        short_name(failure.component)
    };
    let path: Vec<String> = failure.path.iter().map(|name| short_name(name)).collect();
    let mut ring = ring();
    if let Some(last) = ring.back_mut()
        && last.kind == failure.kind
        && last.message == failure.message
        && last.location == failure.location
        && last.component == component
    {
        last.repeats += 1;
        return;
    }
    let record = ErrorRecord {
        id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
        at: now_secs(),
        kind: failure.kind,
        component,
        path,
        message: failure.message.clone(),
        location: failure.location.clone(),
        backtrace: failure.backtrace.clone(),
        thread: failure.thread.clone(),
        repeats: 1,
    };
    if ring.len() == CAPACITY {
        ring.pop_front();
    }
    if failure.kind == FailureKind::Thread && LISTENING.load(Ordering::Acquire) {
        THREAD_FAILURES.push(Arc::new(record.clone()));
    }
    ring.push_back(record);
}

/// The recent failures, newest first, up to 64 of them. Safe to call from any
/// thread: a search provider, a bug-report button, the inspector.
pub fn recent_errors() -> Vec<ErrorRecord> {
    ring().iter().rev().cloned().collect()
}

/// How many failures have been recorded since the process started, including
/// the ones that fell off the ring.
pub fn error_count() -> u64 {
    NEXT_ID.load(Ordering::Relaxed) - 1
}

/// Forget the recent failures. The count is kept.
pub fn clear_errors() {
    ring().clear();
}

/// The recent failures as one text for a bug report, newest first.
pub fn report_text() -> String {
    let records = recent_errors();
    if records.is_empty() {
        return "No errors recorded.\n".to_string();
    }
    let mut text = format!("{} recent error(s), newest first\n", records.len());
    for record in records {
        text.push_str(&format!("\n--- #{} at {}\n{}", record.id, record.at, record.details()));
    }
    text
}

/// The next failure from another thread, waiting for one. For the window's
/// own task, which shows it as a toast.
pub(crate) async fn next_thread_failure() -> Option<Arc<ErrorRecord>> {
    LISTENING.store(true, Ordering::Release);
    THREAD_FAILURES.next().await
}
