//! A frame log and stack sampler for runs with no inspector window, such as a
//! headless session. Inert unless one of the variables below is set.
//!
//! - `BINGUX_PERF_LOG=FILE` appends one line per application frame:
//!   `<unix time in milliseconds when it ended> <milliseconds the frame took>`.
//!   A frame starts at its layout pass (or its render when the layout was
//!   clean) and ends when it is on screen, the same span the Performance tab
//!   times.
//! - `BINGUX_PERF_FOLDED=FILE` samples the UI thread's stack for as long as
//!   the app runs. Create `FILE.dump` and the next frame writes the stacks
//!   sampled since the last dump to `FILE` in folded form (`a;b;c <count>`,
//!   the input of `flamegraph.pl`), deletes `FILE.dump` and samples again.

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
    frame_start: Option<u64>,
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
    Some(Log {
        frames,
        folded,
        frame_start: None,
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
            PluginEvent::AfterRedraw { .. } => {
                let now = perf_sample::now_ns();
                if let (Some(start), Some(file)) = (log.frame_start.take(), log.frames.as_mut()) {
                    let unix_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |elapsed| elapsed.as_millis());
                    let _ = writeln!(file, "{unix_ms} {:.2}", now.saturating_sub(start) as f64 / 1.0e6);
                }
                if let Some(path) = &log.folded {
                    dump_if_asked(path);
                }
            }
            _ => {}
        }
    });
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
