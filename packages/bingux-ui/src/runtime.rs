//! Running system commands without blocking the UI thread.
//!
//! Freya's event loop is single threaded. Commands run on the shared worker
//! pool in [`crate::task`] and hand their result back to a Freya task, which
//! applies it on the UI thread. Every command has a timeout and a readable
//! failure message.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::shell::overlays::Overlays;
use crate::task::spawn_blocking;
use freya::prelude::*;

/// A readable failure from a system command.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CommandError(pub String);

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

pub type CommandResult = Result<String, CommandError>;

/// What a command does for the person using Settings, for error messages:
/// "nmcli is not installed." means nothing to most people, "Network settings
/// are not available." does. Programs not listed here use their own name.
fn feature(program: &str) -> String {
    match program {
        "nmcli" => "Network settings",
        "wpctl" => "Sound settings",
        "bluetoothctl" => "Bluetooth settings",
        "gsettings" => "Desktop settings",
        "timedatectl" => "Date and time settings",
        "upower" => "Battery information",
        "powerprofilesctl" => "Power mode settings",
        "gnoblinctl" => "Display and input settings",
        "busctl" => "Account and display settings",
        "bingux-config" => "Bingux settings",
        "gio" => "Application settings",
        other => other,
    }
    .to_string()
}

/// Run `program args` with a timeout and return trimmed stdout.
///
/// A missing program, a timeout and a non-zero exit all become a
/// `CommandError` whose message is fit for an inline error row.
pub fn run(program: &str, args: &[&str], timeout: Duration) -> CommandResult {
    run_with_stdin(program, args, None, timeout)
}

static UI_THREAD: std::sync::OnceLock<std::thread::ThreadId> = std::sync::OnceLock::new();

/// Record the calling thread as the UI thread. [`crate::app::Application::run`]
/// does it first thing; a test or snapshot that renders on its own thread may
/// call it too.
pub fn mark_ui_thread() {
    let _ = UI_THREAD.set(std::thread::current().id());
}

/// Whether the caller is the UI thread.
pub fn on_ui_thread() -> bool {
    UI_THREAD.get() == Some(&std::thread::current().id())
}

/// Report a blocking call made on the UI thread: `what` names it. Every
/// blocking helper in this module calls this first. The report goes to stderr
/// once per call site, with a backtrace, as
/// `[ui-blocking] <what> on the UI thread` (the frame-time harness fails on
/// that line). It never panics: a freeze is bad, a crash is worse.
pub fn assert_off_ui_thread(what: &str) {
    if !on_ui_thread() {
        return;
    }
    static SEEN: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    let trace = std::backtrace::Backtrace::force_capture().to_string();
    // The first frame outside this module and std is the call site.
    let site = trace
        .lines()
        .find(|line| line.contains("bingux_") && !line.contains("assert_off_ui_thread"))
        .unwrap_or("")
        .trim()
        .to_string();
    let key = format!("{what}@{site}");
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    if seen.contains(&key) {
        return;
    }
    seen.push(key);
    eprintln!("[ui-blocking] {what} on the UI thread\n{trace}");
}

/// `std::fs::read_to_string` for worker code: the same call, with the UI-thread
/// check. Use these four instead of `std::fs` so a read that slips into
/// `render` is reported.
pub fn read_to_string(path: impl AsRef<std::path::Path>) -> std::io::Result<String> {
    assert_off_ui_thread("fs::read_to_string");
    std::fs::read_to_string(path)
}

/// `std::fs::read` with the UI-thread check.
pub fn read_file(path: impl AsRef<std::path::Path>) -> std::io::Result<Vec<u8>> {
    assert_off_ui_thread("fs::read");
    std::fs::read(path)
}

/// `std::fs::read_dir` with the UI-thread check.
pub fn read_dir(path: impl AsRef<std::path::Path>) -> std::io::Result<std::fs::ReadDir> {
    assert_off_ui_thread("fs::read_dir");
    std::fs::read_dir(path)
}

/// `std::fs::metadata` with the UI-thread check.
pub fn metadata(path: impl AsRef<std::path::Path>) -> std::io::Result<std::fs::Metadata> {
    assert_off_ui_thread("fs::metadata");
    std::fs::metadata(path)
}

/// `BINGUX_SLOW_SOURCES_MS=N` makes every command run through [`run`] wait N
/// milliseconds before it starts, as a slow `nmcli` or `wpctl` would. The wait
/// is inside the call, so a call that wrongly runs on the UI thread freezes
/// the window for N ms and shows up in the watchdog and in the frame-time
/// harness (`scripts/frame-time.py --slow-sources`). Unset, it costs nothing.
static SLOW_SOURCES: std::sync::LazyLock<Option<Duration>> = std::sync::LazyLock::new(|| {
    std::env::var("BINGUX_SLOW_SOURCES_MS")
        .ok()?
        .parse::<u64>()
        .ok()
        .filter(|ms| *ms > 0)
        .map(Duration::from_millis)
});

/// Like [`run`] with data written to the command's stdin.
pub fn run_with_stdin(
    program: &str,
    args: &[&str],
    stdin: Option<&str>,
    timeout: Duration,
) -> CommandResult {
    assert_off_ui_thread(&format!("command `{program}`"));
    if let Some(delay) = *SLOW_SOURCES {
        // A debug knob, not behaviour: see `SLOW_SOURCES`.
        std::thread::sleep(delay);
    }
    let mut child = Command::new(program)
        .args(args)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => CommandError(format!(
                "{} are not available because a part they need is not installed.",
                feature(program)
            )),
            _ => CommandError(format!("{} could not be started.", feature(program))),
        })?;
    if let (Some(data), Some(mut pipe)) = (stdin, child.stdin.take()) {
        use std::io::Write;
        let _ = pipe.write_all(data.as_bytes());
    }
    // Drain both pipes while the child runs. A command that prints more than
    // the pipe buffer (about 64 KB) would otherwise block and look timed out.
    let drain = |pipe: Option<Box<dyn std::io::Read + Send>>| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut bytes);
            }
            bytes
        })
    };
    let out_reader = drain(
        child
            .stdout
            .take()
            .map(|pipe| Box::new(pipe) as Box<dyn std::io::Read + Send>),
    );
    let err_reader = drain(
        child
            .stderr
            .take()
            .map(|pipe| Box::new(pipe) as Box<dyn std::io::Read + Send>),
    );
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() < timeout => std::thread::sleep(Duration::from_millis(15)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(CommandError(format!(
                    "{} did not answer. Try again in a moment.",
                    feature(program)
                )));
            }
            Err(_) => return Err(CommandError(format!("{} failed.", feature(program)))),
        }
    };
    let stdout_bytes = out_reader.join().unwrap_or_default();
    let stderr_bytes = err_reader.join().unwrap_or_default();
    let stdout = String::from_utf8_lossy(&stdout_bytes).trim().to_string();
    if status.success() {
        return Ok(stdout);
    }
    let stderr = String::from_utf8_lossy(&stderr_bytes);
    let detail = stderr
        .trim()
        .lines()
        .last()
        .or_else(|| stdout.lines().last())
        .unwrap_or("")
        .trim();
    Err(CommandError(if detail.is_empty() {
        format!("{} failed.", feature(program))
    } else {
        // The tool's own sentence is the useful part; the program name is not.
        let detail = detail
            .trim_start_matches("Error: ")
            .trim_start_matches("error: ");
        let mut letters = detail.chars();
        match letters.next() {
            Some(first) => format!("{}{}", first.to_uppercase(), letters.as_str()),
            None => format!("{} failed.", feature(program)),
        }
    }))
}

/// Run `work` on the task pool and store its result in `state`.
///
/// Call from an event handler or `use_hook`, never from inside `render`.
pub fn background<T: Send + 'static>(
    mut state: State<Option<T>>,
    work: impl FnOnce() -> T + Send + 'static,
) {
    spawn(async move {
        state.set(Some(spawn_blocking(work).await));
    });
}

thread_local! {
    /// One state per page key, alive for the whole window. Pages read their
    /// data from here, so switching pages never starts from nothing.
    static MODELS: std::cell::RefCell<std::collections::HashMap<&'static str, Box<dyn std::any::Any>>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// The long-lived state for `key`. On first use it is created holding
/// `init()`; every later call returns the same state.
fn cached_entry<T: 'static>(
    key: &'static str,
    init: impl FnOnce() -> Option<T>,
) -> State<Option<T>> {
    MODELS.with(|models| {
        let mut models = models.borrow_mut();
        let entry = models
            .entry(key)
            .or_insert_with(|| Box::new(State::<Option<T>>::create_global(init())));
        *entry
            .downcast_ref::<State<Option<T>>>()
            .expect("a model key is always used with one type")
    })
}

fn cached_state<T: 'static>(key: &'static str) -> State<Option<T>> {
    cached_entry(key, || None)
}

/// How long a page's data counts as fresh after a load started. Opening a page
/// inside this window shows what it holds and starts no new load, so flipping
/// between pages does not run every page's commands again each time.
const FRESH_FOR: Duration = Duration::from_secs(15);

thread_local! {
    /// When each key's latest load started. UI thread only.
    static LOADED_AT: std::cell::RefCell<std::collections::HashMap<&'static str, Instant>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Whether `key` should load now: false while a load started less than
/// [`FRESH_FOR`] ago. Answering true records the start.
fn load_due(key: &'static str) -> bool {
    LOADED_AT.with(|at| {
        let mut at = at.borrow_mut();
        let now = Instant::now();
        match at.get(key) {
            Some(start) if now.duration_since(*start) < FRESH_FOR => false,
            _ => {
                at.insert(key, now);
                true
            }
        }
    })
}

/// A page's data: the last value that was loaded, at once, and a quiet
/// refresh in the background when the page opens, unless the last load is
/// recent. `None` only before the very first load has finished. Call from a
/// component's `render`.
pub fn use_cached<T: Send + 'static>(key: &'static str, load: fn() -> T) -> State<Option<T>> {
    let state = cached_state::<T>(key);
    use_hook(move || {
        if load_due(key) {
            background(state, load);
        }
    });
    state
}

/// The cached state for `key` without starting a refresh, for pages that run
/// their own load and polling.
pub fn cached<T: 'static>(key: &'static str) -> State<Option<T>> {
    cached_state(key)
}

/// Start loading a page's data before the page is opened, so it is there
/// when the user gets to it. Call once at start-up, from a hook.
pub fn prefetch<T: Send + 'static>(key: &'static str, load: fn() -> T) {
    load_due(key);
    background(cached_state::<T>(key), load);
}

/// The persistent cache. A page's last good value is kept on disk, so the
/// page has its data at once when Settings starts, before the first load has
/// finished. Each key is a file at `$XDG_CACHE_HOME/<app-id>/<key>.json`, or
/// under `~/.cache` when `XDG_CACHE_HOME` is unset or relative. The app id is
/// the name of the running program.
///
/// The file holds the value in an envelope with a format number. A file that
/// cannot be read, does not parse or has another format number is ignored, as
/// if it were not there.
const CACHE_FORMAT: u32 = 1;

/// Makes each save's temporary file name unique, so two saves never share one.
static SAVE_SEQ: AtomicU64 = AtomicU64::new(0);

#[derive(serde::Serialize)]
struct SavedRef<'a, T> {
    format: u32,
    value: &'a T,
}

#[derive(serde::Deserialize)]
struct Saved<T> {
    format: u32,
    value: T,
}

fn cache_file(key: &str) -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))?;
    // Linux appends " (deleted)" to the path of a program whose file has been
    // rebuilt since it started; the id is the same program either way.
    let app = std::env::current_exe()
        .ok()?
        .file_stem()?
        .to_string_lossy()
        .trim_end_matches(" (deleted)")
        .to_owned();
    // The key names a file, so any character that is not part of a plain name
    // becomes "_".
    let name: String = key
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    Some(base.join(app).join(format!("{name}.json")))
}

/// The saved value for `key`, or `None` when there is no file this build can
/// read.
fn read_saved<T: serde::de::DeserializeOwned>(key: &str) -> Option<T> {
    let text = std::fs::read_to_string(cache_file(key)?).ok()?;
    let saved: Saved<T> = serde_json::from_str(&text).ok()?;
    (saved.format == CACHE_FORMAT).then_some(saved.value)
}

/// Save `value` for `key`. The file is replaced by a rename, and it is not
/// rewritten when it already holds the same text, so a page that refreshes
/// every few seconds does not write to disk each time. A failure goes to
/// stderr only: the cache is an optimisation the page does not depend on.
fn write_saved<T: serde::Serialize>(key: &str, value: &T) {
    let Some(path) = cache_file(key) else {
        return;
    };
    let Ok(text) = serde_json::to_string(&SavedRef {
        format: CACHE_FORMAT,
        value,
    }) else {
        return;
    };
    if std::fs::read_to_string(&path).is_ok_and(|old| old == text) {
        return;
    }
    let seq = SAVE_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp = path.with_extension(format!("{}.{seq}.tmp", std::process::id()));
    let result = path
        .parent()
        .map_or(Ok(()), |dir| std::fs::create_dir_all(dir))
        .and_then(|()| std::fs::write(&tmp, &text))
        .and_then(|()| std::fs::rename(&tmp, &path));
    if let Err(error) = result {
        let _ = std::fs::remove_file(&tmp);
        eprintln!("could not save the cache for {key}: {error}");
    }
}

/// Run `load` and save its value under `key` when it succeeds. Call this from
/// the background, never from `render`. A failed load saves nothing, so the
/// last good value stays on disk.
pub fn load_persistent<T: serde::Serialize, E>(
    key: &'static str,
    load: fn() -> Result<T, E>,
) -> Result<T, E> {
    let value = load()?;
    write_saved(key, &value);
    Ok(value)
}

/// The state for `key`, as [`cached`] gives it. On first use it starts with
/// the saved value, read here from the local file, so the page has its data
/// before its first render. A file that cannot be read starts it empty.
/// Call from a hook or start-up code, never from `render`.
pub fn cached_persistent<T, E>(key: &'static str) -> State<Option<Result<T, E>>>
where
    T: serde::de::DeserializeOwned + 'static,
    E: 'static,
{
    cached_entry(key, || read_saved::<T>(key).map(Ok))
}

/// Start loading `key` before its page is opened. The saved value is shown
/// first, then `load` runs and replaces it, and its result is saved. Call
/// once at start-up, as with [`prefetch`].
pub fn prefetch_persistent<T, E>(key: &'static str, load: fn() -> Result<T, E>)
where
    T: serde::Serialize + serde::de::DeserializeOwned + Send + 'static,
    E: Send + 'static,
{
    load_due(key);
    background(cached_persistent::<T, E>(key), move || {
        load_persistent(key, load)
    });
}

/// As [`use_cached`], with the persistent cache: the saved value is shown at
/// once, then `load` refreshes it and the result is saved. Call from `render`.
pub fn use_cached_persistent<T, E>(
    key: &'static str,
    load: fn() -> Result<T, E>,
) -> State<Option<Result<T, E>>>
where
    T: serde::Serialize + serde::de::DeserializeOwned + Send + 'static,
    E: Send + 'static,
{
    use_hook(move || {
        let state = cached_persistent::<T, E>(key);
        if load_due(key) {
            background(state, move || load_persistent(key, load));
        }
        state
    })
}

/// Run `load` and save its value under `key`, as [`load_persistent`] does for
/// a fallible load. Call from the background, never from `render`.
pub fn load_saved<T: serde::Serialize>(key: &'static str, load: fn() -> T) -> T {
    let value = load();
    write_saved(key, &value);
    value
}

/// As [`use_cached`], with the persistent cache for a load that cannot fail:
/// the saved value is shown at once, then `load` refreshes it and the result
/// is saved. Call from `render`.
pub fn use_cached_saved<T>(key: &'static str, load: fn() -> T) -> State<Option<T>>
where
    T: serde::Serialize + serde::de::DeserializeOwned + Send + 'static,
{
    use_hook(move || {
        let state = cached_entry(key, || read_saved::<T>(key));
        if load_due(key) {
            background(state, move || load_saved(key, load));
        }
        state
    })
}

/// As [`prefetch_persistent`], for a load that cannot fail. Call once at
/// start-up.
pub fn prefetch_saved<T>(key: &'static str, load: fn() -> T)
where
    T: serde::Serialize + serde::de::DeserializeOwned + Send + 'static,
{
    load_due(key);
    background(cached_entry(key, || read_saved::<T>(key)), move || {
        load_saved(key, load)
    });
}

/// Run `work` on the task pool and call `done` with its result on the UI
/// thread, for actions that update several pieces of state.
pub fn background_then<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
    done: impl FnOnce(T) + 'static,
) {
    spawn(async move {
        done(spawn_blocking(work).await);
    });
}

/// Show `value` at once, run `work` in the background, and revert to `previous`
/// with a toast of the error if `work` fails. For a control whose change is
/// applied by a command: the control never waits on the command.
pub fn optimistic<T, E>(
    shown: State<T>,
    overlays: Option<Overlays>,
    previous: T,
    value: T,
    work: impl FnOnce() -> Result<(), E> + Send + 'static,
) where
    T: Clone + PartialEq + 'static,
    E: std::fmt::Display + Send + 'static,
{
    optimistic_then(shown, overlays, previous, value, work, || {});
}

/// As [`optimistic`], and on success run `done` on the UI thread, for example
/// to reload a cached model after the command has changed it.
pub fn optimistic_then<T, E>(
    shown: State<T>,
    overlays: Option<Overlays>,
    previous: T,
    value: T,
    work: impl FnOnce() -> Result<(), E> + Send + 'static,
    done: impl FnOnce() + 'static,
) where
    T: Clone + PartialEq + 'static,
    E: std::fmt::Display + Send + 'static,
{
    let mut shown = shown;
    shown.set(value);
    background_then(work, move |result| match result {
        Ok(()) => done(),
        Err(error) => {
            shown.set(previous.clone());
            if let Some(overlays) = overlays {
                overlays.toast(error.to_string());
            }
        }
    });
}

thread_local! {
    /// The writes in flight, one lane per control that fires on every step of
    /// a drag. UI thread only.
    static LANES: std::cell::RefCell<std::collections::HashMap<String, Lane>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

#[derive(Default)]
struct Lane {
    /// A write for this lane is running on the pool.
    busy: bool,
    /// The newest write that arrived while one was running.
    queued: Option<Box<dyn FnOnce()>>,
}

/// Show `value` at once and write it in the background, for a control that
/// reports every step of a drag, such as a slider. At most one write per
/// `lane` runs at a time and a write that arrives while one runs replaces the
/// one waiting, so a drag costs a handful of commands, not one per pointer
/// event, and the last value always wins. When the lane has gone quiet
/// `resync` runs once, on the UI thread, to read the system back; if a write
/// fails it toasts the error and resyncs, which restores what the system holds.
pub fn optimistic_latest<T, E>(
    lane: &str,
    shown: State<T>,
    overlays: Option<Overlays>,
    value: T,
    work: impl FnOnce() -> Result<(), E> + Send + 'static,
    resync: impl FnOnce() + 'static,
) where
    T: 'static,
    E: std::fmt::Display + Send + 'static,
{
    let mut shown = shown;
    shown.set(value);
    let start: Box<dyn FnOnce()> = {
        let lane = lane.to_owned();
        Box::new(move || write_lane(lane, overlays, work, resync))
    };
    let first = LANES.with(|lanes| {
        let mut lanes = lanes.borrow_mut();
        let slot = lanes.entry(lane.to_owned()).or_default();
        if slot.busy {
            slot.queued = Some(start);
            None
        } else {
            slot.busy = true;
            Some(start)
        }
    });
    if let Some(start) = first {
        start();
    }
}

fn write_lane<E>(
    lane: String,
    overlays: Option<Overlays>,
    work: impl FnOnce() -> Result<(), E> + Send + 'static,
    resync: impl FnOnce() + 'static,
) where
    E: std::fmt::Display + Send + 'static,
{
    background_then(work, move |result| {
        let next = LANES.with(|lanes| {
            lanes
                .borrow_mut()
                .get_mut(&lane)
                .and_then(|slot| slot.queued.take())
        });
        match (result, next) {
            // The lane stays busy: the next write runs now.
            (Ok(()), Some(next)) => next(),
            (result, _) => {
                LANES.with(|lanes| {
                    if let Some(slot) = lanes.borrow_mut().get_mut(&lane) {
                        slot.busy = false;
                    }
                });
                if let (Err(error), Some(overlays)) = (&result, overlays) {
                    overlays.toast(error.to_string());
                }
                resync();
            }
        }
    });
}

/// Whether no write is running or waiting on any lane whose name starts with
/// `prefix`.
pub fn lanes_idle(prefix: &str) -> bool {
    LANES.with(|lanes| {
        lanes
            .borrow()
            .iter()
            .all(|(name, slot)| !name.starts_with(prefix) || (!slot.busy && slot.queued.is_none()))
    })
}

/// Whether the window has keyboard focus, as a state to `peek` in a polling
/// loop: a page that refreshes itself on a timer skips the tick while this is
/// false, so a window behind others, or on another workspace, costs no
/// commands and no redraws. The next tick after focus returns catches up. Call
/// from a hook or `render`, and move the state into the task.
pub fn window_focus() -> State<bool> {
    Platform::get().current_window().is_app_focused
}

/// Read a page's model again and show it, unless a write has started on a lane
/// under `prefix` since: that write's value is newer than this read, and
/// showing the read would snap the control back mid-drag.
pub fn reload_when_idle<T: Send + 'static>(prefix: &str, state: State<Option<T>>, load: fn() -> T) {
    let prefix = prefix.to_owned();
    background_then(load, move |value| {
        if lanes_idle(&prefix) {
            let mut state = state;
            state.set(Some(value));
        }
    });
}

/// Read a gsettings key; the value keeps gsettings' quoting.
pub fn gsettings_get(schema: &str, key: &str) -> CommandResult {
    run("gsettings", &["get", schema, key], Duration::from_secs(3))
}

/// Write a gsettings key with a serialised GVariant value.
pub fn gsettings_set(schema: &str, key: &str, value: &str) -> CommandResult {
    run(
        "gsettings",
        &["set", schema, key, value],
        Duration::from_secs(3),
    )
}

/// Strip GVariant string quotes (`'blue'` -> `blue`).
pub fn unquote(value: &str) -> String {
    value.trim().trim_matches('\'').to_string()
}

/// A value row-sized summary of a command's availability.
pub fn which(program: &str) -> bool {
    assert_off_ui_thread("which");
    std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).any(|dir| dir.join(program).is_file()))
        .unwrap_or(false)
}

/// The `bingux-config` command: `BINGUX_CONFIG_BIN` when set (the test
/// harness convention), else PATH, else the binary built or installed beside
/// the current executable.
pub fn bingux_config() -> String {
    std::env::var("BINGUX_CONFIG_BIN")
        .ok()
        .filter(|path| !path.is_empty())
        .unwrap_or_else(|| locate("bingux-config"))
}

/// The command to run for a companion program: `program` when it is on
/// PATH, else the copy beside the current executable (a build directory or
/// an install prefix), else `program` so that running it still reports
/// "is not installed".
pub fn locate(program: &str) -> String {
    if which(program) {
        return program.to_string();
    }
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(program)))
        .filter(|path| path.is_file())
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| program.to_string())
}

/// Makes `command`'s child die with this process: the kernel sends it
/// SIGTERM when the thread that spawns it ends. Every long-lived helper
/// (a monitor, a recorder, a scan) must use this, so a process that exits
/// without running destructors (a test run, a crash) leaves nothing behind
/// holding a session bus connection. Spawn from a thread that lives as long
/// as the child.
pub fn die_with_parent(command: &mut Command) -> &mut Command {
    use std::os::unix::process::CommandExt;
    // The parent's pid, read before the fork. If the parent has already
    // gone when the child runs prctl, no signal will ever arrive, so the
    // child checks its parent after prctl and exits at once when it changed.
    let parent = std::process::id() as libc::pid_t;
    // SAFETY: the closure only calls prctl, getppid and _exit, which are
    // async-signal-safe.
    unsafe {
        command.pre_exec(move || {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::getppid() != parent {
                libc::_exit(1);
            }
            Ok(())
        });
    }
    command
}
