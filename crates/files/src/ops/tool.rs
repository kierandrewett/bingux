//! Running the programs that make archives and shrink media.
//!
//! Nothing here builds a shell string. A [`Spec`] is a program, its
//! arguments as raw bytes, an environment, a folder and what goes into its
//! standard input; [`run`] and [`run_pipe`] start it so that:
//!
//! - it dies with the process that started it (`PR_SET_PDEATHSIG`), so a
//!   crash leaves no encoder running;
//! - it leads its own process group, so cancelling kills a pipeline and the
//!   helpers a program started, together;
//! - it can run at low priority (`setpriority`), for work that must not slow
//!   the desktop down;
//! - secrets go in through its standard input, never its arguments (which
//!   `ps` shows to everyone) and never its environment.
//!
//! Output is read line by line on helper threads, split at `\n` and `\r` so
//! that a progress meter that rewrites one line reads as many lines. The
//! caller sees each line as it arrives, and the last few lines of what the
//! program said on stderr are kept for the error message.

use std::ffi::{OsStr, OsString};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crate::Cancel;

use super::OpError;

/// How often a running program is checked for being done or cancelled.
const TICK: Duration = Duration::from_millis(40);
/// How long a cancelled program gets to stop before it is killed.
const GRACE: Duration = Duration::from_millis(1500);
/// How many lines of stderr are kept for the error message.
const KEPT_LINES: usize = 12;
/// The longest line read; the rest of a longer one is dropped.
const LINE_LIMIT: usize = 64 * 1024;

/// The first executable called `name` in a folder of `PATH`.
pub fn find(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| {
            fs::metadata(candidate)
                .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        })
}

/// Is `name` installed?
pub fn on_path(name: &str) -> bool {
    find(name).is_some()
}

/// The 7-Zip program: `7z`, `7zz` or `7za`, whichever is installed first.
/// (`7zr` cannot make zip archives and is not looked for.)
pub fn seven_zip() -> Option<PathBuf> {
    ["7z", "7zz", "7za"].into_iter().find_map(find)
}

/// What goes into a program's standard input.
#[derive(Clone)]
pub enum Input {
    Null,
    /// These bytes, then end of file. A password goes here.
    Bytes(Vec<u8>),
    /// The contents of this file: a program that would read a file name as
    /// something else (a format prefix, a frame selector) reads it from here.
    File(PathBuf),
}

/// A program to run.
#[derive(Clone)]
pub struct Spec {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub env: Vec<(OsString, OsString)>,
    pub cwd: Option<PathBuf>,
    pub input: Input,
    /// Run at the lowest priority a person notices least.
    pub low_priority: bool,
}

impl std::fmt::Debug for Spec {
    /// The program and its arguments; the input is never shown.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Spec")
            .field("program", &self.program)
            .field("args", &self.args)
            .finish_non_exhaustive()
    }
}

impl Spec {
    pub fn new(program: impl Into<PathBuf>) -> Spec {
        Spec {
            program: program.into(),
            args: Vec::new(),
            env: Vec::new(),
            cwd: None,
            input: Input::Null,
            low_priority: false,
        }
    }

    pub fn arg(mut self, arg: impl AsRef<OsStr>) -> Spec {
        self.args.push(arg.as_ref().to_os_string());
        self
    }

    pub fn args<I, S>(mut self, args: I) -> Spec
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.args
            .extend(args.into_iter().map(|arg| arg.as_ref().to_os_string()));
        self
    }

    pub fn in_dir(mut self, dir: impl Into<PathBuf>) -> Spec {
        self.cwd = Some(dir.into());
        self
    }

    pub fn with_input(mut self, input: Input) -> Spec {
        self.input = input;
        self
    }

    pub fn nice(mut self, low: bool) -> Spec {
        self.low_priority = low;
        self
    }

    /// The program's name for a sentence: `zip`, `ffmpeg`.
    pub fn name(&self) -> String {
        self.program
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
    }
}

/// Which stream a line came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stream {
    Out,
    Err,
}

/// How a run ended, with the last things the program said on stderr.
#[derive(Debug)]
pub struct Finished {
    pub status: ExitStatus,
    pub complaint: String,
}

impl Finished {
    pub fn success(&self) -> bool {
        self.status.success()
    }

    /// The first non-empty line the program complained with, as a sentence.
    pub fn sentence(&self, what: &str) -> OpError {
        let line = self
            .complaint
            .lines()
            .map(str::trim)
            .rfind(|line| !line.is_empty() && !line.starts_with("Error: "))
            .or_else(|| {
                self.complaint
                    .lines()
                    .map(str::trim)
                    .rfind(|line| !line.is_empty())
            })
            .unwrap_or("The program gave no reason")
            .trim_end_matches('.');
        let line = line.strip_prefix("Error: ").unwrap_or(line);
        OpError::Other(format!("{what}: {line}."))
    }
}

/// Make `command`'s child die with the thread that starts it, lead its own
/// process group and, if asked, run at low priority.
fn prepare(command: &mut Command, low_priority: bool) {
    let parent = std::process::id() as libc::pid_t;
    command.process_group(0);
    // SAFETY: the closure calls only prctl, getppid, setpriority and _exit,
    // which are async-signal-safe.
    unsafe {
        command.pre_exec(move || {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) != 0 {
                return Err(io::Error::last_os_error());
            }
            if libc::getppid() != parent {
                libc::_exit(1);
            }
            if low_priority {
                // Failing to lower the priority is not worth failing the job.
                libc::setpriority(libc::PRIO_PROCESS, 0, 10);
            }
            Ok(())
        });
    }
}

fn command_of(spec: &Spec) -> Command {
    let mut command = Command::new(&spec.program);
    command.args(&spec.args);
    for (key, value) in &spec.env {
        command.env(key, value);
    }
    if let Some(cwd) = &spec.cwd {
        command.current_dir(cwd);
    }
    prepare(&mut command, spec.low_priority);
    command
}

fn spawn_error(spec: &Spec, error: io::Error) -> OpError {
    match error.kind() {
        io::ErrorKind::NotFound => {
            OpError::Unsupported(format!("{} is not installed.", spec.name()))
        }
        _ => error.into(),
    }
}

/// One line, or the end of a stream.
enum Event {
    Line(Stream, Vec<u8>),
    Closed,
}

/// Read `source` to its end, sending each line (split at `\n` or `\r`).
fn read_lines(mut source: impl Read + Send + 'static, stream: Stream, sender: mpsc::Sender<Event>) {
    thread::spawn(move || {
        let mut line = Vec::new();
        let mut buffer = [0u8; 8192];
        loop {
            let count = match source.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(count) => count,
            };
            for &byte in &buffer[..count] {
                if byte == b'\n' || byte == b'\r' {
                    if !line.is_empty()
                        && sender
                            .send(Event::Line(stream, std::mem::take(&mut line)))
                            .is_err()
                    {
                        return;
                    }
                } else if line.len() < LINE_LIMIT {
                    line.push(byte);
                }
            }
        }
        if !line.is_empty() {
            let _ = sender.send(Event::Line(stream, line));
        }
        let _ = sender.send(Event::Closed);
    });
}

/// Feed `bytes` to a child's standard input from a helper thread, so a
/// program that fills its output pipe first cannot deadlock with us.
fn feed(child: &mut Child, input: &Input) {
    match (input, child.stdin.take()) {
        (Input::Bytes(bytes), Some(mut stdin)) => {
            let bytes = bytes.clone();
            thread::spawn(move || {
                let _ = stdin.write_all(&bytes);
            });
        }
        // Dropping the handle closes the pipe.
        (_, stdin) => drop(stdin),
    }
}

fn stdin_of(spec: &Spec) -> Stdio {
    match &spec.input {
        Input::Null => Stdio::null(),
        Input::Bytes(_) => Stdio::piped(),
        // Opened here, so the program never sees a name it could misread.
        Input::File(path) => match File::open(path) {
            Ok(file) => Stdio::from(file),
            Err(_) => Stdio::null(),
        },
    }
}

/// Stop a process group: ask, wait a moment, then insist.
fn stop(child: &mut Child) {
    let group = child.id() as libc::pid_t;
    // SAFETY: killpg with a group this process created.
    unsafe {
        libc::killpg(group, libc::SIGTERM);
    }
    let until = Instant::now() + GRACE;
    while Instant::now() < until {
        if matches!(child.try_wait(), Ok(Some(_))) {
            return;
        }
        thread::sleep(Duration::from_millis(20));
    }
    // SAFETY: as above.
    unsafe {
        libc::killpg(group, libc::SIGKILL);
    }
    let _ = child.wait();
}

/// Run `spec` to its end. Each line of its output goes to `on_line`. With
/// `cancel` set the program is stopped and the answer is
/// [`OpError::Cancelled`].
pub fn run(
    spec: &Spec,
    cancel: &Cancel,
    on_line: &mut dyn FnMut(Stream, &[u8]),
) -> Result<Finished, OpError> {
    let mut command = command_of(spec);
    command
        .stdin(stdin_of(spec))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|error| spawn_error(spec, error))?;
    feed(&mut child, &spec.input);
    let (sender, lines) = mpsc::channel();
    let Some(out) = child.stdout.take() else {
        stop(&mut child);
        return Err(OpError::Other("The program gave no output stream.".into()));
    };
    let Some(err) = child.stderr.take() else {
        stop(&mut child);
        return Err(OpError::Other("The program gave no error stream.".into()));
    };
    read_lines(out, Stream::Out, sender.clone());
    read_lines(err, Stream::Err, sender);
    supervise(vec![child], lines, 2, cancel, on_line)
}

/// Run `first | second > output`: the first program's output feeds the
/// second, which writes into `output`. Both stop together.
pub fn run_pipe(
    first: &Spec,
    second: &Spec,
    output: File,
    cancel: &Cancel,
    on_line: &mut dyn FnMut(Stream, &[u8]),
) -> Result<Finished, OpError> {
    let mut one = command_of(first);
    one.stdin(stdin_of(first))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut one = one.spawn().map_err(|error| spawn_error(first, error))?;
    feed(&mut one, &first.input);
    let Some(pipe) = one.stdout.take() else {
        stop(&mut one);
        return Err(OpError::Other("The program gave no output stream.".into()));
    };
    let mut two = command_of(second);
    two.stdin(Stdio::from(pipe))
        .stdout(Stdio::from(output))
        .stderr(Stdio::piped());
    let mut two = match two.spawn() {
        Ok(child) => child,
        Err(error) => {
            stop(&mut one);
            return Err(spawn_error(second, error));
        }
    };
    let (sender, lines) = mpsc::channel();
    let mut streams = 0;
    if let Some(err) = one.stderr.take() {
        read_lines(err, Stream::Err, sender.clone());
        streams += 1;
    }
    if let Some(err) = two.stderr.take() {
        read_lines(err, Stream::Err, sender);
        streams += 1;
    }
    supervise(vec![one, two], lines, streams, cancel, on_line)
}

/// What the readers have said so far.
struct Heard {
    open: usize,
    kept: Vec<String>,
}

impl Heard {
    fn take(&mut self, event: Event, on_line: &mut dyn FnMut(Stream, &[u8])) {
        match event {
            Event::Line(stream, line) => {
                if stream == Stream::Err {
                    if self.kept.len() == KEPT_LINES {
                        self.kept.remove(0);
                    }
                    self.kept.push(String::from_utf8_lossy(&line).into_owned());
                }
                on_line(stream, &line);
            }
            Event::Closed => self.open = self.open.saturating_sub(1),
        }
    }
}

/// Wait for every child, handing lines to `on_line`, and stop them all on
/// cancel. The result carries the first failing status, else the last.
fn supervise(
    mut children: Vec<Child>,
    lines: mpsc::Receiver<Event>,
    streams: usize,
    cancel: &Cancel,
    on_line: &mut dyn FnMut(Stream, &[u8]),
) -> Result<Finished, OpError> {
    let mut statuses: Vec<Option<ExitStatus>> = vec![None; children.len()];
    let mut heard = Heard {
        open: streams,
        kept: Vec::new(),
    };
    let mut cancelled = false;
    loop {
        if let Ok(event) = lines.recv_timeout(TICK) {
            heard.take(event, on_line);
        }
        if cancel.is_cancelled() && !cancelled {
            cancelled = true;
            for child in &mut children {
                stop(child);
            }
        }
        for (child, status) in children.iter_mut().zip(statuses.iter_mut()) {
            if status.is_none() {
                *status = child.try_wait().map_err(OpError::from)?;
            }
        }
        if statuses.iter().all(Option::is_some) {
            break;
        }
    }
    // Every program is done. What it said last may still be in the pipes; a
    // helper it left behind may hold them open, so do not wait for ever.
    let until = Instant::now() + TICK * 5;
    while heard.open > 0 {
        let left = until.saturating_duration_since(Instant::now());
        match lines.recv_timeout(left) {
            Ok(event) => heard.take(event, on_line),
            Err(_) => break,
        }
    }
    if cancelled || cancel.is_cancelled() {
        return Err(OpError::Cancelled);
    }
    let statuses: Vec<ExitStatus> = statuses.into_iter().flatten().collect();
    let status = statuses
        .iter()
        .copied()
        .find(|status| !status.success())
        .or_else(|| statuses.last().copied())
        .unwrap_or_else(|| ExitStatus::from_raw(0));
    Ok(Finished {
        status,
        complaint: heard.kept.join("\n"),
    })
}

/// Run `spec` and collect what it printed on stdout, up to `limit` bytes.
pub fn capture(spec: &Spec, cancel: &Cancel, limit: usize) -> Result<(Finished, Vec<u8>), OpError> {
    let mut text = Vec::new();
    let finished = run(spec, cancel, &mut |stream, line| {
        if stream == Stream::Out && text.len() < limit {
            text.extend_from_slice(line);
            text.push(b'\n');
        }
    })?;
    Ok((finished, text))
}

/// A path as the bytes the kernel sees it as.
pub fn bytes_of(path: &Path) -> &[u8] {
    path.as_os_str().as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quiet() -> impl FnMut(Stream, &[u8]) {
        |_, _| {}
    }

    #[test]
    fn find_looks_on_the_path() {
        assert!(on_path("sh"));
        assert!(!on_path("definitely-not-a-program-bingux"));
    }

    #[test]
    fn lines_split_at_returns_and_newlines() {
        let spec = Spec::new("printf").arg("a\\rb\\nc");
        let mut seen = Vec::new();
        let finished = run(&spec, &Cancel::new(), &mut |stream, line| {
            assert_eq!(stream, Stream::Out);
            seen.push(String::from_utf8_lossy(line).into_owned());
        })
        .unwrap();
        assert!(finished.success());
        assert_eq!(seen, ["a", "b", "c"]);
    }

    #[test]
    fn input_goes_to_stdin_not_to_the_arguments() {
        let spec = Spec::new("cat").with_input(Input::Bytes(b"secret\n".to_vec()));
        let mut seen = Vec::new();
        run(&spec, &Cancel::new(), &mut |_, line| {
            seen.push(line.to_vec())
        })
        .unwrap();
        assert_eq!(seen, [b"secret".to_vec()]);
        assert!(!format!("{spec:?}").contains("secret"));
    }

    #[test]
    fn the_complaint_is_the_last_line_of_stderr() {
        let spec = Spec::new("sh").args(["-c", "echo first >&2; echo 'second thing.' >&2; exit 3"]);
        let finished = run(&spec, &Cancel::new(), &mut quiet()).unwrap();
        assert_eq!(finished.status.code(), Some(3));
        assert_eq!(
            finished.sentence("It failed"),
            OpError::Other("It failed: second thing.".into())
        );
    }

    #[test]
    fn a_missing_program_says_so() {
        let spec = Spec::new("definitely-not-a-program-bingux");
        assert_eq!(
            run(&spec, &Cancel::new(), &mut quiet()).unwrap_err(),
            OpError::Unsupported("definitely-not-a-program-bingux is not installed.".into())
        );
    }

    #[test]
    fn cancel_stops_a_program_and_its_children() {
        let cancel = Cancel::new();
        let stopper = cancel.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(200));
            stopper.cancel();
        });
        let spec = Spec::new("sh").args(["-c", "sleep 30 & sleep 30"]);
        let started = Instant::now();
        assert_eq!(
            run(&spec, &cancel, &mut quiet()).unwrap_err(),
            OpError::Cancelled
        );
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn a_pipe_feeds_the_second_program_and_writes_the_file() {
        let dir = std::env::temp_dir().join(format!("bingux-tool-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let out = dir.join("out.txt");
        let file = File::create(&out).unwrap();
        let first = Spec::new("printf").arg("hello");
        let second = Spec::new("tr").args(["a-z", "A-Z"]);
        let finished = run_pipe(&first, &second, file, &Cancel::new(), &mut quiet()).unwrap();
        assert!(finished.success());
        assert_eq!(fs::read_to_string(&out).unwrap(), "HELLO");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_low_priority_program_runs() {
        let spec = Spec::new("sh").args(["-c", "exit 0"]).nice(true);
        assert!(run(&spec, &Cancel::new(), &mut quiet()).unwrap().success());
    }
}
