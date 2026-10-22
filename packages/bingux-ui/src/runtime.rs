//! Running system commands without blocking the UI thread.
//!
//! Freya's event loop is single threaded, so every command runs on a std
//! thread and hands its result back through a channel that a Freya task
//! polls. Every command has a timeout and a readable failure message.

use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use freya::prelude::*;

/// A readable failure from a system command.
#[derive(Clone, Debug, PartialEq)]
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
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
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
    }
    let output = child
        .wait_with_output()
        .map_err(|_| CommandError(format!("{} failed.", feature(program))))?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if output.status.success() {
        return Ok(stdout);
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
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

/// Run `work` on a background thread and deliver its result to `state`.
///
/// Call from an event handler or `use_hook`; never from inside `render`
/// without a hook, or every render would start a new thread.
pub fn background<T: Send + 'static>(
    mut state: State<Option<T>>,
    work: impl FnOnce() -> T + Send + 'static,
) {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(work());
    });
    spawn(async move {
        loop {
            match receiver.try_recv() {
                Ok(value) => {
                    state.set(Some(value));
                    return;
                }
                Err(mpsc::TryRecvError::Empty) => timer(Duration::from_millis(30)).await,
                Err(mpsc::TryRecvError::Disconnected) => return,
            }
        }
    });
}

thread_local! {
    /// One state per page key, alive for the whole window. Pages read their
    /// data from here, so switching pages never starts from nothing.
    static MODELS: std::cell::RefCell<std::collections::HashMap<&'static str, Box<dyn std::any::Any>>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// The long-lived state for `key`, created empty on first use.
fn cached_state<T: 'static>(key: &'static str) -> State<Option<T>> {
    MODELS.with(|models| {
        let mut models = models.borrow_mut();
        let entry = models
            .entry(key)
            .or_insert_with(|| Box::new(State::<Option<T>>::create_global(None)));
        *entry
            .downcast_ref::<State<Option<T>>>()
            .expect("a model key is always used with one type")
    })
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

/// Run `work` in the background and call `done` with its result on the UI
/// thread, for actions that update several pieces of state.
pub fn background_then<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
    done: impl FnOnce(T) + 'static,
) {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(work());
    });
    spawn(async move {
        loop {
            match receiver.try_recv() {
                Ok(value) => {
                    done(value);
                    return;
                }
                Err(mpsc::TryRecvError::Empty) => timer(Duration::from_millis(30)).await,
                Err(mpsc::TryRecvError::Disconnected) => return,
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

/// What [`pick_path`] asks for.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    /// One image file.
    Image,
    /// One folder.
    Folder,
}

const IMAGE_PATTERNS: &str = "*.png *.jpg *.jpeg *.webp *.jxl *.avif";

/// Show the system file chooser and return the picked path, or `None` when
/// the person cancels. Uses zenity when present, otherwise kdialog. Blocks
/// until the chooser closes, so call it from a background thread.
pub fn pick_path(title: &str, pick: Pick) -> PickResult {
    let mut command = if which("zenity") {
        let mut command = Command::new("zenity");
        command
            .arg("--file-selection")
            .arg(format!("--title={title}"));
        match pick {
            Pick::Image => command.arg(format!("--file-filter=Images | {IMAGE_PATTERNS}")),
            Pick::Folder => command.arg("--directory"),
        };
        command
    } else if which("kdialog") {
        let mut command = Command::new("kdialog");
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        command.args(["--title", title]);
        match pick {
            Pick::Image => command
                .arg("--getopenfilename")
                .arg(home)
                .arg(format!("Images ({IMAGE_PATTERNS})")),
            Pick::Folder => command.arg("--getexistingdirectory").arg(home),
        };
        command
    } else {
        return Err(CommandError(
            "Choosing a file needs a file chooser, and none is installed.".into(),
        ));
    };
    let output = command
        .stdin(Stdio::null())
        .output()
        .map_err(|_| CommandError("The file chooser could not be started.".into()))?;
    if !output.status.success() {
        // Both dialogs exit 1 with nothing on stderr when the user cancels.
        let stderr = String::from_utf8_lossy(&output.stderr);
        return match stderr.trim().lines().last() {
            None => Ok(None),
            Some(detail) => Err(CommandError(format!(
                "The file chooser failed: {}",
                detail.trim()
            ))),
        };
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok((!path.is_empty()).then_some(path))
}

type PickResult = Result<Option<String>, CommandError>;
