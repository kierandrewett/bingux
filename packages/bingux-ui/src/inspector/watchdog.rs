//! The UI-thread watchdog: finds code that keeps the UI thread from its event
//! loop for longer than a frame, and says where.
//!
//! Off unless `BINGUX_UI_WATCHDOG` is set (`1`, or a number of milliseconds
//! for the threshold; 12 by default). A monitor thread looks at what the UI
//! thread is doing every 2 ms through `/proc/self/task/<tid>/syscall`: parked
//! in `epoll_wait`, `ppoll` and the like it is idle; anything else (running,
//! in `nanosleep`, `wait4`, `read`, a lock) it is busy. When it has been busy
//! for the threshold the monitor signals it, the signal handler takes a
//! backtrace, and the monitor logs
//!
//! ```text
//! [watchdog 1761500000000] UI thread blocked 48 ms in std::thread::sleep (from bingux_settings::...)
//! ```
//!
//! with the whole stack, once at the threshold and again for every further
//! threshold the same stretch lasts, then the total when the thread is idle
//! again. Lines go to stderr, and also to the file named by
//! `BINGUX_UI_WATCHDOG_LOG`, which `scripts/frame-time.py` reads. A stretch
//! that is only drawing (GL submit in the stack) is logged as `slow frame`,
//! not `blocked`.
//!
//! The time the thread spends runnable but waiting for a CPU (the kernel's
//! `schedstat`) is left out of every figure, so a machine with every core busy
//! does not make a 5 ms job read as a 100 ms block; a call that sleeps or waits
//! counts in full.
//!
//! A stretch ends at every plugin event too (an update, a layout, a render and
//! a presenting each send one), so what is reported is one step of a frame that
//! took long, not a stream of short frames that kept the thread busy.

use std::fs::OpenOptions;
use std::io::Write;
use std::os::raw::{c_int, c_void};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{LazyLock, Mutex, Once};
use std::time::{Duration, Instant};

use freya_winit::plugins::PluginEvent;

use super::perf_sample::{Symbolizer, is_handler_frame};

const DEFAULT_THRESHOLD_MS: u64 = 12;
const SAMPLE: Duration = Duration::from_millis(2);
const MAX_DEPTH: usize = 48;

static STACK: [AtomicUsize; MAX_DEPTH] = [const { AtomicUsize::new(0) }; MAX_DEPTH];
static DEPTH: AtomicUsize = AtomicUsize::new(0);
static READY: AtomicBool = AtomicBool::new(false);
/// Counts the plugin events the UI thread has handled. A change means the
/// thread got from one step of a frame (an update, a layout, a render, a
/// presenting) to the next, so a stretch of work never runs across one: a
/// drag that keeps the thread busy for seconds in many short frames is not a
/// block.
static BEATS: AtomicU64 = AtomicU64::new(0);

struct Config {
    threshold: Duration,
    log: Option<std::path::PathBuf>,
}

static CONFIG: LazyLock<Option<Config>> = LazyLock::new(|| {
    let value = std::env::var("BINGUX_UI_WATCHDOG").ok()?;
    if value.is_empty() || value == "0" {
        return None;
    }
    let ms = value.parse::<u64>().ok().filter(|ms| *ms > 1).unwrap_or(DEFAULT_THRESHOLD_MS);
    Some(Config {
        threshold: Duration::from_millis(ms),
        log: std::env::var_os("BINGUX_UI_WATCHDOG_LOG").map(Into::into),
    })
});

/// A stretch the monitor has logged, for the Performance tab.
#[derive(Clone)]
pub struct Stretch {
    pub ms: u64,
    pub kind: &'static str,
    pub place: String,
}

static WORST: Mutex<Vec<Stretch>> = Mutex::new(Vec::new());

/// The longest stretches seen so far, longest first. Empty when the watchdog
/// is off.
pub fn worst() -> Vec<Stretch> {
    WORST.lock().map(|list| list.clone()).unwrap_or_default()
}

/// Start the monitor on the first plugin event, which arrives on the UI thread.
pub fn on_event(_event: &PluginEvent<'_>) {
    static START: Once = Once::new();
    let Some(config) = CONFIG.as_ref() else { return };
    BEATS.fetch_add(1, Ordering::Relaxed);
    START.call_once(|| {
        // SAFETY: SYS_gettid takes no arguments.
        let tid = unsafe { libc::syscall(libc::SYS_gettid) } as c_int;
        if !install_handler() {
            eprintln!("[watchdog] could not install the signal handler; watchdog off");
            return;
        }
        // glibc loads libgcc's unwinder lazily; do it here, not in the handler.
        let mut warm = [std::ptr::null_mut::<c_void>(); 4];
        // SAFETY: `warm` has 4 writable slots.
        unsafe { libc::backtrace(warm.as_mut_ptr(), 4) };
        let spawned = std::thread::Builder::new()
            .name("bingux-watchdog".into())
            .spawn(move || monitor(tid, config));
        if let Err(error) = spawned {
            eprintln!("[watchdog] could not start the monitor: {error}");
        }
    });
}

fn install_handler() -> bool {
    // SAFETY: a zeroed sigaction is valid; the fields are set below.
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = on_signal as *const () as libc::sighandler_t;
        action.sa_flags = libc::SA_SIGINFO | libc::SA_RESTART;
        libc::sigemptyset(&mut action.sa_mask);
        libc::sigaction(libc::SIGUSR2, &action, std::ptr::null_mut()) == 0
    }
}

extern "C" fn on_signal(_sig: c_int, _info: *mut libc::siginfo_t, _ctx: *mut c_void) {
    let mut frames = [std::ptr::null_mut::<c_void>(); MAX_DEPTH];
    // SAFETY: `frames` has MAX_DEPTH writable slots; backtrace was warmed up.
    let depth = unsafe { libc::backtrace(frames.as_mut_ptr(), MAX_DEPTH as c_int) }.max(0) as usize;
    for (slot, frame) in STACK.iter().zip(&frames[..depth]) {
        slot.store(*frame as usize, Ordering::Relaxed);
    }
    DEPTH.store(depth, Ordering::Relaxed);
    READY.store(true, Ordering::Release);
}

/// Whether the thread is parked in the event loop's wait. `None` when the
/// thread is gone. A thread that is running in user space has no syscall: the
/// file says `running`, which is busy, not gone.
fn idle(tid: c_int) -> Option<bool> {
    let text = std::fs::read_to_string(format!("/proc/self/task/{tid}/syscall")).ok()?;
    let first = text.split_whitespace().next()?;
    let Ok(number) = first.parse::<i64>() else {
        return Some(false);
    };
    Some(is_wait(number))
}

fn is_wait(number: i64) -> bool {
    let n = number as libc::c_long;
    #[allow(unused_mut)]
    let mut waits = vec![libc::SYS_epoll_pwait, libc::SYS_ppoll, libc::SYS_pselect6];
    #[cfg(target_arch = "x86_64")]
    waits.extend([libc::SYS_epoll_wait, libc::SYS_poll, libc::SYS_select]);
    waits.contains(&n)
}

fn capture(tid: c_int) -> Vec<usize> {
    READY.store(false, Ordering::Release);
    // SAFETY: tgkill to a thread of this process.
    unsafe { libc::syscall(libc::SYS_tgkill, libc::getpid(), tid, libc::SIGUSR2) };
    let until = Instant::now() + Duration::from_millis(20);
    while !READY.load(Ordering::Acquire) {
        if Instant::now() > until {
            return Vec::new();
        }
        std::thread::sleep(Duration::from_micros(200));
    }
    let depth = DEPTH.load(Ordering::Relaxed).min(MAX_DEPTH);
    STACK[..depth].iter().map(|slot| slot.load(Ordering::Relaxed)).collect()
}

fn emit(config: &Config, text: &str) {
    let unix_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis());
    let text = &text.replacen("[watchdog]", &format!("[watchdog {unix_ms}]"), 1);
    eprintln!("{text}");
    if let Some(path) = &config.log {
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(file, "{text}");
        }
    }
}

/// The frame to blame: the first one that is not the runtime, the signal
/// machinery or libc.
fn place_of(names: &[Box<str>]) -> String {
    let own = names
        .iter()
        .find(|name| name.contains("bingux_") && !is_handler_frame(name));
    let leaf = names.iter().find(|name| {
        !is_handler_frame(name) && !name.starts_with("libc.so") && !name.starts_with("?? ")
    });
    match (leaf, own) {
        (Some(leaf), Some(own)) if leaf != own => format!("{leaf} (from {own})"),
        (Some(leaf), _) => leaf.to_string(),
        (None, _) => "unknown".to_string(),
    }
}

/// Nanoseconds the thread has spent runnable but waiting for a CPU, from the
/// scheduler's accounting. `None` when the kernel does not keep it.
fn queue_wait_ns(tid: c_int) -> Option<u64> {
    let text = std::fs::read_to_string(format!("/proc/self/task/{tid}/schedstat")).ok()?;
    text.split_whitespace().nth(1)?.parse().ok()
}

/// How long the stretch that began at `since` has lasted, without the time the
/// thread waited in the run queue for a CPU: on a machine with every core busy
/// that wait is the machine's, not the app's, and a stretch of work that takes
/// 5 ms of CPU must not read as 100 ms. Time in a blocking call counts in full.
fn active_ms(since: Instant, wait_at_start: u64, tid: c_int) -> u64 {
    let waited = queue_wait_ns(tid).unwrap_or(wait_at_start).saturating_sub(wait_at_start);
    let total = since.elapsed().as_nanos() as u64;
    total.saturating_sub(waited) / 1_000_000
}

fn monitor(tid: c_int, config: &'static Config) {
    let mut symbols = Symbolizer::default();
    let mut wait_at_start = 0u64;
    let mut busy_since: Option<Instant> = None;
    let mut reported = 0u32;
    let mut kind = "blocked";
    let mut place = String::new();
    let mut beats = BEATS.load(Ordering::Relaxed);
    loop {
        std::thread::sleep(SAMPLE);
        let Some(waiting) = idle(tid) else {
            // The thread is gone: the window closed.
            return;
        };
        let stepped = BEATS.load(Ordering::Relaxed) != beats;
        if waiting || stepped {
            // Parked, or on to the next step of a frame: the stretch is over.
            if let Some(since) = busy_since.take() {
                let ms = active_ms(since, wait_at_start, tid);
                if reported > 0 {
                    emit(config, &format!("[watchdog] UI thread {kind} {ms} ms in {place} (total)"));
                    if let Ok(mut list) = WORST.lock() {
                        list.push(Stretch { ms, kind, place: place.clone() });
                        list.sort_by(|a, b| b.ms.cmp(&a.ms));
                        list.truncate(16);
                    }
                }
            }
            reported = 0;
            beats = BEATS.load(Ordering::Relaxed);
            if waiting {
                continue;
            }
        }
        let since = *busy_since.get_or_insert_with(|| {
            wait_at_start = queue_wait_ns(tid).unwrap_or(0);
            Instant::now()
        });
        let over = active_ms(since, wait_at_start, tid) / config.threshold.as_millis() as u64;
        if over > u64::from(reported) && reported < 4 {
            let pcs = capture(tid);
            if pcs.is_empty() {
                continue;
            }
            let names: Vec<Box<str>> = pcs.iter().map(|pc| symbols.name(*pc)).collect();
            let drawing = names.iter().any(|name| {
                name.contains("flush_submit") || name.contains("GrDrawingManager") || name.contains("::present")
            });
            kind = if drawing { "slow frame" } else { "blocked" };
            place = place_of(&names);
            reported += 1;
            let ms = active_ms(since, wait_at_start, tid);
            let mut text = format!("[watchdog] UI thread {kind} {ms} ms in {place}");
            for name in names.iter().filter(|name| !is_handler_frame(name)).take(24) {
                text.push_str("\n    ");
                text.push_str(name);
            }
            emit(config, &text);
        }
    }
}
