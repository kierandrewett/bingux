//! The last resort: when the process cannot go on, start it again.
//!
//! A panic is never fatal here, and a failure the UI can recover from (a
//! component, a handler, a task) never gets this far. Two kinds of failure
//! cannot be recovered in place: running out of memory and a graphics driver
//! that cannot be rebuilt. For those the app replaces itself with a fresh copy
//! of itself (`exec`, the same process id) that opens the location the window
//! showed, and tells the user what happened. It does this once: a copy that was
//! itself started this way does not relaunch again for a minute, so a failure
//! that comes back at once ends the process instead of looping.
//!
//! Rust reports an allocation failure by aborting, not by panicking, so
//! [`install`] also handles `SIGABRT` with a handler that only does what a
//! signal handler may: it unblocks signals and calls `execve` with arguments
//! prepared in advance.

use std::cell::UnsafeCell;
use std::ffi::{CString, c_char};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::CommandExt;
use std::process::Command;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use freya_core::guard::Failure;

/// Set in the environment of the new copy, with the reason.
const RESTARTED: &str = "BINGUX_RESTARTED";
/// Set in the environment of the new copy, so it knows it is one.
const RELAUNCHED: &str = "BINGUX_RELAUNCHED";
/// How long a relaunched copy runs before it may relaunch again.
const HOLD_OFF: Duration = Duration::from_secs(60);
/// The longest location kept for the relaunch.
const URI_CAPACITY: usize = 2048;

/// What this process knows about being a relaunch.
struct Start {
    at: Instant,
    /// Monotonic seconds at start, for the signal handler, which cannot use
    /// `Instant`.
    at_secs: i64,
    relaunched: bool,
    reason: Option<String>,
}

static START: OnceLock<Start> = OnceLock::new();

/// A location buffer a signal handler may read. Only the UI thread writes it,
/// and `URI_LEN` says how much of it is valid.
struct UriBuffer(UnsafeCell<[u8; URI_CAPACITY + 1]>);

// SAFETY: one writer (the UI thread, under `URI_BUSY`); the signal handler only
// reads, and a torn read at worst opens a wrong location.
unsafe impl Sync for UriBuffer {}

static URI_BUFFER: UriBuffer = UriBuffer(UnsafeCell::new([0; URI_CAPACITY + 1]));
static URI_LEN: AtomicUsize = AtomicUsize::new(0);
static URI_BUSY: AtomicBool = AtomicBool::new(false);

/// What `execve` needs, built before it is needed.
struct Plan {
    exe: CString,
    argv0: CString,
    /// The environment of this process plus the two variables for the new copy.
    _env: Vec<CString>,
    envp: Vec<*const c_char>,
}

// SAFETY: the pointers in `envp` point into `_env`, which lives as long as the
// plan, and nothing mutates either.
unsafe impl Send for Plan {}
unsafe impl Sync for Plan {}

static PLAN: OnceLock<Plan> = OnceLock::new();

fn monotonic_secs() -> i64 {
    let mut now = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: `now` is a valid timespec; `clock_gettime` is async-signal-safe.
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut now) };
    now.tv_sec
}

fn start() -> &'static Start {
    START.get_or_init(|| Start {
        at: Instant::now(),
        at_secs: monotonic_secs(),
        relaunched: false,
        reason: None,
    })
}

/// Whether this process may relaunch now.
fn allowed() -> bool {
    let start = start();
    !start.relaunched || start.at.elapsed() >= HOLD_OFF
}

/// Prepare to relaunch. Call once, early, before other threads start: it reads
/// and clears the variables a previous copy left, and sets up the `SIGABRT`
/// handler.
pub(crate) fn install() {
    let relaunched = std::env::var_os(RELAUNCHED).is_some();
    let reason = std::env::var(RESTARTED).ok().filter(|reason| !reason.is_empty());
    // SAFETY: called from `Application::run` before any other thread starts, so
    // nothing reads the environment concurrently.
    unsafe {
        std::env::remove_var(RELAUNCHED);
        std::env::remove_var(RESTARTED);
    }
    if START
        .set(Start {
            at: Instant::now(),
            at_secs: monotonic_secs(),
            relaunched,
            reason,
        })
        .is_err()
    {
        return;
    }
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let Ok(exe) = CString::new(exe.as_os_str().as_bytes()) else {
        return;
    };
    let argv0 = std::env::args_os()
        .next()
        .and_then(|arg| CString::new(arg.as_bytes()).ok())
        .unwrap_or_else(|| exe.clone());
    let mut env: Vec<CString> = std::env::vars_os()
        .filter_map(|(key, value)| {
            let mut entry = key.as_bytes().to_vec();
            entry.push(b'=');
            entry.extend_from_slice(value.as_bytes());
            CString::new(entry).ok()
        })
        .collect();
    env.push(CString::from(c"BINGUX_RELAUNCHED=1"));
    env.push(CString::from(
        c"BINGUX_RESTARTED=the app ran out of memory or hit a fatal runtime error",
    ));
    let mut envp: Vec<*const c_char> = env.iter().map(|entry| entry.as_ptr()).collect();
    envp.push(std::ptr::null());
    let _ = PLAN.set(Plan {
        exe,
        argv0,
        _env: env,
        envp,
    });

    // SAFETY: `sigaction` with a handler that only calls async-signal-safe
    // functions (see `on_abort`).
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = on_abort as *const () as usize;
        libc::sigemptyset(&mut action.sa_mask);
        action.sa_flags = libc::SA_NODEFER;
        libc::sigaction(libc::SIGABRT, &action, std::ptr::null_mut());
    }
}

/// Remember the location the window shows, for a relaunch. Cheap; call on
/// every navigation.
pub(crate) fn note_location(uri: &str) {
    let bytes = uri.as_bytes();
    if bytes.len() > URI_CAPACITY || bytes.contains(&0) {
        return;
    }
    if URI_BUSY.swap(true, Ordering::Acquire) {
        return;
    }
    // SAFETY: `URI_BUSY` makes this the only writer; the length is published
    // after the bytes.
    unsafe {
        let buffer = &mut *URI_BUFFER.0.get();
        URI_LEN.store(0, Ordering::Release);
        buffer[..bytes.len()].copy_from_slice(bytes);
        buffer[bytes.len()] = 0;
    }
    URI_LEN.store(bytes.len(), Ordering::Release);
    URI_BUSY.store(false, Ordering::Release);
}

fn current_location() -> Option<String> {
    let length = URI_LEN.load(Ordering::Acquire);
    if length == 0 {
        return None;
    }
    // SAFETY: only the UI thread writes, and this runs on it.
    let bytes = unsafe { &(&*URI_BUFFER.0.get())[..length] };
    String::from_utf8(bytes.to_vec()).ok()
}

/// Why this copy was started, once: the text the new window shows as a toast.
pub(crate) fn take_restart_reason() -> Option<String> {
    static TAKEN: AtomicBool = AtomicBool::new(false);
    if TAKEN.swap(true, Ordering::AcqRel) {
        return None;
    }
    START.get().and_then(|start| start.reason.clone())
}

/// Replace this process with a fresh copy of the app, opening the location the
/// window showed. Only returns if it could not: a relaunch is already running
/// (so a failure that repeats ends the process instead of looping), or `exec`
/// failed.
pub(crate) fn relaunch(failure: &Failure) -> bool {
    if !allowed() {
        eprintln!("[bingux] not relaunching: this copy started less than a minute ago");
        return false;
    }
    let reason = format!("the app restarted after: {}", failure.message.lines().next().unwrap_or(""));
    eprintln!("[bingux] unrecoverable ({}); relaunching", failure.message);
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let mut command = Command::new(exe);
    match current_location() {
        Some(uri) => {
            command.arg(uri);
        }
        None => {
            command.args(std::env::args_os().skip(1));
        }
    }
    command.env(RELAUNCHED, "1").env(RESTARTED, reason);
    // `exec` only returns on failure.
    let error = command.exec();
    eprintln!("[bingux] relaunch failed: {error}");
    false
}

/// The `SIGABRT` handler: an allocation failure or a fatal runtime error is
/// about to end the process, so start it again. Async-signal-safe: no
/// allocation, no locks, only `sigprocmask` and `execve` on memory prepared by
/// [`install`].
extern "C" fn on_abort(_signal: libc::c_int) {
    let Some(plan) = PLAN.get() else {
        return;
    };
    if let Some(start) = START.get()
        && start.relaunched
        && monotonic_secs() - start.at_secs < HOLD_OFF.as_secs() as i64
    {
        return;
    }
    // SAFETY: only async-signal-safe calls, on memory that outlives the
    // process's signal handling. `execve` replaces the process or returns -1.
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        libc::sigprocmask(libc::SIG_SETMASK, &set, std::ptr::null_mut());
        let length = URI_LEN.load(Ordering::Acquire);
        let mut argv: [*const c_char; 3] = [plan.argv0.as_ptr(), std::ptr::null(), std::ptr::null()];
        if length > 0 {
            argv[1] = URI_BUFFER.0.get() as *const c_char;
        }
        libc::execve(plan.exe.as_ptr(), argv.as_ptr(), plan.envp.as_ptr());
    }
}
