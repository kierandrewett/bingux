//! `bingux-helper process-action`: signal the selected processes without
//! risking a reused PID.
//!
//! The process table in the shell identifies a process by PID and by the start
//! time the kernel recorded for it (`starttime` in `/proc/<pid>/stat`). Both
//! are checked through a pidfd, so a PID that was reused after the table was
//! drawn is refused instead of signalled.
//!
//! ```text
//! bingux-helper process-action PID START_TIME pause|resume|end|kill|copy
//! echo '[{"pid":1,"startTime":2}]' | bingux-helper process-action --batch ACTION
//! ```

use std::{
    fs,
    io::{self, BufRead, Read},
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    process::{Command, Stdio},
    ptr, thread,
    time::{Duration, Instant},
};

use serde_json::Value;

const WL_COPY_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_BATCH: usize = 8192;
const MAX_INPUT_BYTES: u64 = 8 * 1024 * 1024;
/// Index of `starttime` among the fields that follow the command name in
/// `/proc/<pid>/stat` (field 22 overall, state being the first of them).
const START_TIME_FIELD: usize = 19;
const INVALID_SELECTION: &str = "Invalid process selection";
const EXITED: &str = "The selected process has exited";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Pause,
    Resume,
    End,
    Kill,
    Copy,
}

impl Action {
    fn parse(name: &str) -> Option<Self> {
        match name {
            "pause" => Some(Self::Pause),
            "resume" => Some(Self::Resume),
            "end" => Some(Self::End),
            "kill" => Some(Self::Kill),
            "copy" => Some(Self::Copy),
            _ => None,
        }
    }

    fn signal(self) -> Option<libc::c_int> {
        match self {
            Self::Pause => Some(libc::SIGSTOP),
            Self::Resume => Some(libc::SIGCONT),
            Self::End => Some(libc::SIGTERM),
            Self::Kill => Some(libc::SIGKILL),
            Self::Copy => None,
        }
    }

    fn done(self) -> &'static str {
        match self {
            Self::Pause => "Process paused",
            Self::Resume => "Process resumed",
            Self::End => "End requested",
            Self::Kill => "Force quit requested",
            Self::Copy => "PID copied",
        }
    }

    fn batch_label(self) -> &'static str {
        match self {
            Self::Pause => "Paused",
            Self::Resume => "Resumed",
            Self::End => "End requested for",
            Self::Kill => "Force quit requested for",
            Self::Copy => "Copied",
        }
    }
}

/// The text of an I/O failure as the shell shows it.
fn describe(error: &io::Error) -> String {
    match error.raw_os_error() {
        Some(libc::EPERM | libc::EACCES) => "Permission denied for this process".to_owned(),
        Some(libc::ESRCH | libc::ENOENT) => EXITED.to_owned(),
        _ => error.to_string(),
    }
}

fn copy_to_clipboard(text: &str) -> Result<(), String> {
    // wl-copy forks a clipboard server that inherits its pipes, so its output
    // is not captured: capturing would wait for that server to exit.
    let mut child = Command::new("wl-copy")
        .arg(text)
        .stdout(Stdio::null())
        .spawn()
        .map_err(|error| match error.kind() {
            io::ErrorKind::NotFound => "wl-copy is not installed.".to_owned(),
            _ => error.to_string(),
        })?;
    let deadline = Instant::now() + WL_COPY_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => return Err(format!("wl-copy failed: {status}")),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                // Best effort: the child may have exited since the last check.
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "wl-copy did not answer within {} seconds.",
                    WL_COPY_TIMEOUT.as_secs()
                ));
            }
            Err(error) => return Err(error.to_string()),
        }
    }
}

fn pidfd_open(pid: i32) -> io::Result<OwnedFd> {
    // SAFETY: pidfd_open takes a PID and a flags word and returns a new file
    // descriptor or -1; no pointers are involved.
    let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, libc::c_long::from(pid), 0) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the kernel returned a valid descriptor that nothing else owns.
    Ok(unsafe { OwnedFd::from_raw_fd(fd as libc::c_int) })
}

fn pidfd_send_signal(pidfd: &OwnedFd, signal: libc::c_int) -> io::Result<()> {
    // SAFETY: the descriptor is open for the whole call, and a null siginfo
    // asks the kernel to fill in the sender, as kill(2) does.
    let result = unsafe {
        libc::syscall(
            libc::SYS_pidfd_send_signal,
            pidfd.as_raw_fd(),
            signal,
            ptr::null::<libc::siginfo_t>(),
            0,
        )
    };
    if result < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// The kernel's start time for `pid`, read from `/proc/<pid>/stat`.
fn start_time(pid: i32) -> io::Result<i64> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat"))?;
    // The command name is in parentheses and may contain spaces and
    // parentheses itself; the fields that matter follow the last ") ".
    let tail = stat
        .rsplit_once(") ")
        .map(|(_, tail)| tail)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "unreadable process status"))?;
    tail.split_whitespace()
        .nth(START_TIME_FIELD)
        .and_then(|field| field.parse().ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "unreadable process start time"))
}

fn act(pid: i64, expected_start: i64, action: Action) -> Result<String, String> {
    if action == Action::Copy {
        copy_to_clipboard(&pid.to_string())?;
        return Ok(action.done().to_owned());
    }
    let signal = action.signal().ok_or(INVALID_SELECTION)?;
    let pid = i32::try_from(pid).map_err(|_| INVALID_SELECTION.to_owned())?;
    if pid <= 1 || expected_start <= 0 {
        return Err(INVALID_SELECTION.to_owned());
    }
    // Open the pidfd first, then compare the start time: the pidfd keeps
    // pointing at the process it was opened for, even if the PID is reused
    // between the check and the signal.
    let pidfd = pidfd_open(pid).map_err(|error| describe(&error))?;
    if start_time(pid).map_err(|error| describe(&error))? != expected_start {
        return Err(EXITED.to_owned());
    }
    pidfd_send_signal(&pidfd, signal).map_err(|error| describe(&error))?;
    Ok(action.done().to_owned())
}

fn integer(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::Number(number) => number.as_i64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

fn act_batch(selection: &Value, action: Action) -> Result<String, String> {
    let list = selection
        .as_array()
        .filter(|list| (1..=MAX_BATCH).contains(&list.len()))
        .ok_or(INVALID_SELECTION)?;
    // Duplicates collapse to one entry, in the order they first appear.
    let mut processes: Vec<(i64, i64)> = Vec::with_capacity(list.len());
    for entry in list {
        let pid = integer(entry.get("pid")).ok_or(INVALID_SELECTION)?;
        let started = integer(entry.get("startTime")).ok_or(INVALID_SELECTION)?;
        if !processes.contains(&(pid, started)) {
            processes.push((pid, started));
        }
    }
    if action == Action::Copy {
        let pids: Vec<String> = processes.iter().map(|(pid, _)| pid.to_string()).collect();
        copy_to_clipboard(&pids.join("\n"))?;
        return Ok(match pids.len() {
            1 => action.done().to_owned(),
            count => format!("{count} PIDs copied"),
        });
    }
    if let [(pid, started)] = processes[..] {
        return act(pid, started, action);
    }
    let mut succeeded = 0;
    let mut errors = Vec::new();
    for (pid, started) in processes {
        match act(pid, started, action) {
            Ok(_) => succeeded += 1,
            Err(error) => errors.push(format!("PID {pid}: {error}")),
        }
    }
    let mut message = format!("{} {succeeded} processes", action.batch_label());
    if !errors.is_empty() {
        let shown: Vec<&str> = errors.iter().take(3).map(String::as_str).collect();
        message.push_str(&format!("; {} failed. {}", errors.len(), shown.join("; ")));
    }
    Ok(message)
}

fn parse_action(name: &str) -> Result<Action, String> {
    Action::parse(name).ok_or_else(|| format!("Unknown action {name:?}"))
}

pub fn run(arguments: Vec<String>) -> Result<String, String> {
    match arguments.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["--batch", action] => {
            let action = parse_action(action)?;
            let mut line = String::new();
            io::stdin()
                .lock()
                .take(MAX_INPUT_BYTES)
                .read_line(&mut line)
                .map_err(|error| error.to_string())?;
            let selection: Value = serde_json::from_str(&line).map_err(|_| INVALID_SELECTION)?;
            act_batch(&selection, action)
        }
        [pid, started, action] => {
            let pid = pid.parse().map_err(|_| INVALID_SELECTION)?;
            let started = started.parse().map_err(|_| INVALID_SELECTION)?;
            act(pid, started, parse_action(action)?)
        }
        _ => Err(
            "usage: bingux-helper process-action PID START_TIME ACTION | --batch ACTION".to_owned(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::process::Child;

    struct Sleeper(Child);

    impl Sleeper {
        fn start() -> Self {
            Self(
                Command::new("sleep")
                    .arg("30")
                    .spawn()
                    .expect("sleep starts"),
            )
        }

        fn pid(&self) -> i64 {
            i64::from(self.0.id())
        }

        fn started(&self) -> i64 {
            start_time(self.0.id() as i32).expect("start time")
        }

        fn state(&self) -> char {
            let stat = fs::read_to_string(format!("/proc/{}/stat", self.0.id())).expect("stat");
            stat.rsplit_once(") ")
                .expect("stat format")
                .1
                .chars()
                .next()
                .expect("state")
        }

        fn wait_for(&self, wanted: impl Fn(char) -> bool) -> bool {
            (0..300).any(|_| {
                thread::sleep(Duration::from_millis(10));
                wanted(self.state())
            })
        }
    }

    impl Drop for Sleeper {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    #[test]
    fn signals_a_process_and_refuses_a_reused_pid() {
        let mut sleeper = Sleeper::start();
        let (pid, started) = (sleeper.pid(), sleeper.started());

        // A start time that does not match is a different process: untouched.
        assert_eq!(act(pid, started + 1, Action::Kill), Err(EXITED.to_owned()));
        assert!(sleeper.0.try_wait().expect("status").is_none());

        assert_eq!(
            act(pid, started, Action::Pause),
            Ok("Process paused".to_owned())
        );
        assert!(sleeper.wait_for(|state| state == 'T'));
        assert_eq!(
            act(pid, started, Action::Resume),
            Ok("Process resumed".to_owned())
        );
        assert!(sleeper.wait_for(|state| state != 'T'));
        assert_eq!(
            act(pid, started, Action::End),
            Ok("End requested".to_owned())
        );
        let status = sleeper.0.wait().expect("exits");
        assert_eq!(
            std::os::unix::process::ExitStatusExt::signal(&status),
            Some(libc::SIGTERM)
        );
    }

    #[test]
    fn rejects_pids_that_must_never_be_signalled() {
        for (pid, started) in [(0, 5), (1, 5), (-4, 5), (4242, 0), (4242, -1)] {
            assert_eq!(
                act(pid, started, Action::Kill),
                Err(INVALID_SELECTION.to_owned())
            );
        }
        assert_eq!(
            act(i64::from(i32::MAX) + 1, 5, Action::Kill),
            Err(INVALID_SELECTION.to_owned())
        );
    }

    #[test]
    fn a_batch_continues_after_a_stale_selection() {
        let mut first = Sleeper::start();
        let second = Sleeper::start();
        let live = json!([
            {"pid": first.pid(), "startTime": first.started()},
            {"pid": second.pid(), "startTime": second.started()},
        ]);
        assert_eq!(
            act_batch(&live, Action::Pause),
            Ok("Paused 2 processes".to_owned())
        );
        assert_eq!(
            act_batch(&live, Action::Resume),
            Ok("Resumed 2 processes".to_owned())
        );

        let stale = json!([
            {"pid": first.pid(), "startTime": first.started() + 1},
            {"pid": second.pid(), "startTime": second.started()},
        ]);
        let message = act_batch(&stale, Action::End).expect("batch result");
        assert!(
            message.starts_with("End requested for 1 processes; 1 failed. PID "),
            "{message}"
        );
        assert!(message.ends_with(EXITED), "{message}");
        let mut second = second;
        let status = second.0.wait().expect("second ends");
        assert_eq!(
            std::os::unix::process::ExitStatusExt::signal(&status),
            Some(libc::SIGTERM)
        );
        assert!(first.0.try_wait().expect("status").is_none());
    }

    #[test]
    fn a_batch_of_one_reports_like_a_single_action_and_duplicates_collapse() {
        let sleeper = Sleeper::start();
        let entry = json!({"pid": sleeper.pid(), "startTime": sleeper.started()});
        let twice = json!([entry.clone(), entry]);
        assert_eq!(
            act_batch(&twice, Action::Pause),
            Ok("Process paused".to_owned())
        );
        assert!(sleeper.wait_for(|state| state == 'T'));
    }

    #[test]
    fn validates_the_batch_shape() {
        for bad in [
            json!([]),
            json!({}),
            json!("x"),
            json!([{"pid": 5}]),
            json!([{"pid": "a", "startTime": 1}]),
        ] {
            assert_eq!(
                act_batch(&bad, Action::Kill),
                Err(INVALID_SELECTION.to_owned()),
                "{bad}"
            );
        }
        let oversized = Value::Array(vec![json!({"pid": 5, "startTime": 1}); MAX_BATCH + 1]);
        assert_eq!(
            act_batch(&oversized, Action::Kill),
            Err(INVALID_SELECTION.to_owned())
        );
        // String numbers are accepted, as the Python helper's int() did.
        assert_eq!(integer(Some(&json!("17"))), Some(17));
    }

    #[test]
    fn reads_the_start_time_of_a_command_name_with_parentheses_and_spaces() {
        // The field search starts after the last ") ", so a name like this
        // must not shift the fields.
        let stat = "123 (we ird) name) S 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 987654 20";
        let tail = stat.rsplit_once(") ").expect("tail").1;
        assert_eq!(
            tail.split_whitespace().nth(START_TIME_FIELD),
            Some("987654")
        );
    }

    #[test]
    fn run_rejects_unknown_actions_and_bad_arguments() {
        assert!(run(vec!["5".into(), "5".into(), "explode".into()]).is_err());
        assert_eq!(
            run(vec!["x".into(), "5".into(), "kill".into()]),
            Err(INVALID_SELECTION.to_owned())
        );
        assert!(run(vec![]).unwrap_err().starts_with("usage:"));
    }
}
