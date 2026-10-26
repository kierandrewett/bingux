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

/// Like [`run`] with data written to the command's stdin.
pub fn run_with_stdin(
    program: &str,
    args: &[&str],
    stdin: Option<&str>,
    timeout: Duration,
) -> CommandResult {
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

/// A page's data: the last value that was loaded, at once, and a quiet
/// refresh in the background each time the page opens. `None` only before the
/// very first load has finished. Call from a component's `render`.
pub fn use_cached<T: Send + 'static>(key: &'static str, load: fn() -> T) -> State<Option<T>> {
    let state = cached_state::<T>(key);
    use_hook(move || background(state, load));
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
        background(state, move || load_persistent(key, load));
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
        background(state, move || load_saved(key, load));
        state
    })
}

/// As [`prefetch_persistent`], for a load that cannot fail. Call once at
/// start-up.
pub fn prefetch_saved<T>(key: &'static str, load: fn() -> T)
where
    T: serde::Serialize + serde::de::DeserializeOwned + Send + 'static,
{
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
