//! A frame log and stack sampler for runs with no inspector window, such as a
//! headless session. Inert unless one of the variables below is set.
//!
//! - `BINGUX_PERF_LOG=FILE` appends one line per application frame:
//!   `<unix time in milliseconds when it ended> <milliseconds the frame took>
//!   <milliseconds of UI-thread CPU since the previous frame ended>
//!   <the same without the presenting step>. The third figure is the UI
//!   thread's busy time per frame, events and layout and render and the GPU
//!   submit; the fourth leaves the submit out, which on a software renderer
//!   (llvmpipe, headless sway) is rasterisation on the UI thread and says
//!   nothing about the app. Neither depends on waiting.
//!   A frame starts at its layout pass (or its render when the layout was
//!   clean) and ends when it is on screen, the same span the Performance tab
//!   times.
//! - `BINGUX_PERF_FOLDED=FILE` samples the UI thread's stack for as long as
//!   the app runs. Create `FILE.dump` and the next frame writes the stacks
//!   sampled since the last dump to `FILE` in folded form (`a;b;c <count>`,
//!   the input of `flamegraph.pl`), deletes `FILE.dump` and samples again.
//! - `BINGUX_PERF_SLOW_FRAMES=MS`, with `BINGUX_PERF_FOLDED`, keeps only the
//!   stacks of the frames whose busy time (the fourth figure above) is over
//!   `MS`: each such frame appends its stacks to `FILE` at once, under a first
//!   frame `frame <busy> ms`, and every other frame's samples are dropped. It
//!   is the profile of what the frame gate fails on, not of a whole session.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use freya_winit::plugins::PluginEvent;

use super::perf_sample;

struct Log {
    frames: Option<File>,
    folded: Option<PathBuf>,
    /// Frames busy for longer than this many milliseconds are profiled alone.
    slow_over: Option<f64>,
    /// The busy time of the frame that just ended, in milliseconds.
    last_app_ms: f64,
    frame_start: Option<u64>,
    /// The UI thread's CPU clock when the previous frame ended.
    cpu_at: u64,
    /// The UI thread's CPU clock when presenting began, while it runs.
    present_from: Option<u64>,
    /// CPU the UI thread spent presenting since the previous frame ended.
    present_cpu: u64,
}

thread_local! {
    static LOG: RefCell<Option<Log>> = const { RefCell::new(None) };
    static READ_ENV: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn open() -> Option<Log> {
    let frames = std::env::var_os("BINGUX_PERF_LOG")
        .and_then(|path| OpenOptions::new().create(true).append(true).open(path).ok());
    let folded = std::env::var_os("BINGUX_PERF_FOLDED").map(PathBuf::from);
    if frames.is_none() && folded.is_none() {
        return None;
    }
    if folded.is_some() {
        perf_sample::start();
    }
    let slow_over = std::env::var("BINGUX_PERF_SLOW_FRAMES")
        .ok()
        .and_then(|value| value.trim().parse::<f64>().ok());
    Some(Log {
        frames,
        folded,
        slow_over,
        last_app_ms: 0.0,
        frame_start: None,
        cpu_at: thread_cpu_ns(),
        present_from: None,
        present_cpu: 0,
    })
}

/// Feed one plugin event for the application window. Costs one thread-local
/// read when neither variable is set.
pub fn on_event(event: &PluginEvent<'_>) {
    LOG.with(|cell| {
        if !READ_ENV.with(|read| read.replace(true)) {
            *cell.borrow_mut() = open();
        }
        let mut guard = cell.borrow_mut();
        let Some(log) = guard.as_mut() else { return };
        match event {
            PluginEvent::BeforeMeasuringLayout { .. } | PluginEvent::BeforeRender { .. } => {
                let now = perf_sample::now_ns();
                log.frame_start.get_or_insert(now);
            }
            PluginEvent::BeforePresenting { .. } => log.present_from = Some(thread_cpu_ns()),
            PluginEvent::AfterPresenting { .. } => {
                if let Some(from) = log.present_from.take() {
                    log.present_cpu += thread_cpu_ns().saturating_sub(from);
                }
            }
            PluginEvent::AfterRedraw { .. } => {
                let now = perf_sample::now_ns();
                if let (Some(start), Some(file)) = (log.frame_start.take(), log.frames.as_mut()) {
                    let unix_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |elapsed| elapsed.as_millis());
                    let cpu = thread_cpu_ns();
                    let busy = cpu.saturating_sub(log.cpu_at) as f64 / 1.0e6;
                    let app = cpu.saturating_sub(log.cpu_at).saturating_sub(log.present_cpu) as f64 / 1.0e6;
                    log.cpu_at = cpu;
                    log.present_cpu = 0;
                    let _ = writeln!(
                        file,
                        "{unix_ms} {:.2} {busy:.2} {app:.2}",
                        now.saturating_sub(start) as f64 / 1.0e6
                    );
                    log.last_app_ms = app;
                }
                if let (Some(path), Some(over)) = (&log.folded, log.slow_over) {
                    dump_slow_frame(path, log.last_app_ms, over);
                } else if let Some(path) = &log.folded {
                    dump_if_asked(path);
                }
                // Writing the profile is not the next frame's work.
                log.cpu_at = thread_cpu_ns();
            }
            _ => {}
        }
    });
}

/// Appends the samples of the frame that just ended to `path` when it was busy
/// for more than `over` milliseconds, and drops them otherwise.
fn dump_slow_frame(path: &PathBuf, busy_ms: f64, over: f64) {
    if busy_ms <= over {
        perf_sample::reset();
        return;
    }
    let profile = perf_sample::stop();
    let mut counts: HashMap<String, u32> = HashMap::new();
    for sample in &profile.samples {
        let stack: Vec<&str> = sample.stack.iter().map(|name| &**name).collect();
        *counts.entry(stack.join(";")).or_insert(0) += 1;
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        for (stack, count) in counts {
            let _ = writeln!(file, "frame {busy_ms:.0} ms;{stack} {count}");
        }
    }
    perf_sample::start();
}

fn dump_if_asked(path: &PathBuf) {
    let mut trigger = path.clone().into_os_string();
    trigger.push(".dump");
    let trigger = PathBuf::from(trigger);
    if !trigger.exists() {
        return;
    }
    let profile = perf_sample::stop();
    let mut counts: HashMap<String, u32> = HashMap::new();
    for sample in &profile.samples {
        let stack: Vec<&str> = sample.stack.iter().map(|name| &**name).collect();
        *counts.entry(stack.join(";")).or_insert(0) += 1;
    }
    if let Ok(mut file) = File::create(path) {
        for (stack, count) in counts {
            let _ = writeln!(file, "{stack} {count}");
        }
    }
    let _ = std::fs::remove_file(trigger);
    perf_sample::start();
}

/// CPU time this thread has used, in nanoseconds.
fn thread_cpu_ns() -> u64 {
    let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: `ts` is a valid out-pointer.
    unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut ts) };
    ts.tv_sec as u64 * 1_000_000_000 + ts.tv_nsec as u64
}
