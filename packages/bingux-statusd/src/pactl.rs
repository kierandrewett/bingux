//! Bounded `pactl` plumbing shared by the audio watchers: one-shot commands with
//! an output limit and a deadline, and a line reader for `pactl subscribe`.

use std::{
    io::{self, Read},
    os::unix::{io::AsRawFd, process::CommandExt},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub(crate) const MAX_SNAPSHOT_BYTES: usize = 1024 * 1024;
const MAX_SUBSCRIBE_LINE_BYTES: usize = 8 * 1024;
const SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(3);

struct TerminateChild(Child);

impl Drop for TerminateChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct SubscriptionReader {
    stdout: std::process::ChildStdout,
    pending: Vec<u8>,
}

impl SubscriptionReader {
    fn new(stdout: std::process::ChildStdout) -> Self {
        Self {
            stdout,
            pending: Vec::new(),
        }
    }

    fn next_line(&mut self, timeout: Duration) -> Result<Option<String>, String> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(newline) = self.pending.iter().position(|byte| *byte == b'\n') {
                if newline + 1 > MAX_SUBSCRIBE_LINE_BYTES {
                    return Err("pactl subscribe event exceeds the input limit".to_owned());
                }
                let line = self.pending.drain(..=newline).collect::<Vec<_>>();
                return String::from_utf8(line)
                    .map(Some)
                    .map_err(|_| "invalid pactl event".to_owned());
            }
            if self.pending.len() > MAX_SUBSCRIBE_LINE_BYTES {
                return Err("pactl subscribe event exceeds the input limit".to_owned());
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(None);
            }
            let mut descriptor = libc::pollfd {
                fd: self.stdout.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            let milliseconds = remaining.as_millis().min(i32::MAX as u128) as i32;
            let polled = unsafe { libc::poll(&mut descriptor, 1, milliseconds) };
            if polled < 0 {
                if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(io::Error::last_os_error().to_string());
            }
            if polled == 0 {
                return Ok(None);
            }
            if descriptor.revents & (libc::POLLERR | libc::POLLNVAL) != 0 {
                return Err("pactl subscribe pipe failed".to_owned());
            }
            let mut bytes = [0_u8; 1024];
            match self.stdout.read(&mut bytes) {
                Ok(0) => return Err("pactl subscribe closed".to_owned()),
                Ok(count) => {
                    self.pending.extend_from_slice(&bytes[..count]);
                    if self.pending.len() > MAX_SUBSCRIBE_LINE_BYTES
                        && !self.pending.contains(&b'\n')
                    {
                        return Err("pactl subscribe event exceeds the input limit".to_owned());
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Err(error.to_string()),
            }
        }
    }
}

/// A running `pactl subscribe`, killed when dropped.
pub(crate) struct Subscription {
    _child: TerminateChild,
    reader: SubscriptionReader,
}

impl Subscription {
    pub(crate) fn open() -> Result<Self, String> {
        let mut child = TerminateChild(spawn_pactl(&["subscribe"])?);
        let stdout = child
            .0
            .stdout
            .take()
            .ok_or("pactl subscribe has no stdout")?;
        Ok(Self {
            _child: child,
            reader: SubscriptionReader::new(stdout),
        })
    }

    /// Block until `relevant` accepts an event line, then swallow the burst of
    /// events that follows within `debounce` so one snapshot covers it.
    pub(crate) fn wait_for(
        &mut self,
        relevant: impl Fn(&str) -> bool,
        debounce: Duration,
    ) -> Result<(), String> {
        loop {
            let Some(line) = self.reader.next_line(Duration::from_secs(60))? else {
                continue;
            };
            if !relevant(&line) {
                continue;
            }
            let deadline = Instant::now() + debounce;
            while Instant::now() < deadline {
                if self
                    .reader
                    .next_line(deadline.saturating_duration_since(Instant::now()))?
                    .is_none()
                {
                    break;
                }
            }
            return Ok(());
        }
    }
}

pub(crate) fn run_command(arguments: &[&str]) -> Result<String, String> {
    let mut child = TerminateChild(spawn_pactl(arguments)?);
    let mut stdout = child.0.stdout.take().ok_or("pactl has no stdout")?;
    set_nonblocking(&stdout)?;
    let deadline = Instant::now() + SNAPSHOT_TIMEOUT;
    let mut output = Vec::new();
    let mut exited = false;
    loop {
        read_available(&mut stdout, &mut output, MAX_SNAPSHOT_BYTES, deadline)?;
        if !exited {
            if let Some(status) = child.0.try_wait().map_err(|error| error.to_string())? {
                if !status.success() {
                    return Err(format!("pactl exited with {status}"));
                }
                exited = true;
            } else if Instant::now() >= deadline {
                return Err("pactl snapshot timed out".to_owned());
            }
        }
        if exited {
            if Instant::now() >= deadline {
                return Err("pactl output did not close".to_owned());
            }
            match stdout.read(&mut [0_u8; 1]) {
                Ok(0) => break,
                Ok(count) => {
                    return Err(format!(
                        "unexpected pactl output after drain: {count} bytes"
                    ));
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(1));
                    continue;
                }
                Err(error) => return Err(error.to_string()),
            }
        }
        wait_for_readable(
            stdout.as_raw_fd(),
            deadline.saturating_duration_since(Instant::now()),
        )?;
    }
    String::from_utf8(output).map_err(|_| "pactl output is not UTF-8".to_owned())
}

fn set_nonblocking(file: &impl AsRawFd) -> Result<(), String> {
    let flags = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETFL) };
    if flags < 0
        || unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
    {
        return Err(io::Error::last_os_error().to_string());
    }
    Ok(())
}

fn read_available(
    stdout: &mut impl Read,
    output: &mut Vec<u8>,
    limit: usize,
    deadline: Instant,
) -> Result<(), String> {
    let mut bytes = [0_u8; 4096];
    loop {
        if Instant::now() >= deadline {
            return Err("pactl snapshot timed out".to_owned());
        }
        let remaining = limit
            .checked_add(1)
            .and_then(|maximum| maximum.checked_sub(output.len()))
            .ok_or_else(|| "pactl snapshot exceeds the input limit".to_owned())?;
        let read_limit = bytes.len().min(remaining);
        match stdout.read(&mut bytes[..read_limit]) {
            Ok(0) => return Ok(()),
            Ok(count) => output.extend_from_slice(&bytes[..count]),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(()),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error.to_string()),
        }
    }
}
fn wait_for_readable(file_descriptor: i32, timeout: Duration) -> Result<(), String> {
    let mut descriptor = libc::pollfd {
        fd: file_descriptor,
        events: libc::POLLIN,
        revents: 0,
    };
    let milliseconds = timeout.as_millis().min(10).min(i32::MAX as u128) as i32;
    let result = unsafe { libc::poll(&mut descriptor, 1, milliseconds) };
    if result < 0 && io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
        return Err(io::Error::last_os_error().to_string());
    }
    Ok(())
}
fn spawn_pactl(arguments: &[&str]) -> Result<Child, String> {
    let parent = unsafe { libc::getpid() };
    let mut command = Command::new("pactl");
    command
        .args(arguments)
        .env("LC_ALL", "C")
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    // The child must not outlive statusd if the daemon exits between fork and exec.
    unsafe {
        command.pre_exec(move || {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 || libc::getppid() != parent
            {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    command.spawn().map_err(|error| error.to_string())
}

/// The facility an event line names: `sink` for `Event 'change' on sink #5`.
pub(crate) fn event_facility(event: &str) -> Option<&str> {
    event
        .trim()
        .rsplit_once(" on ")
        .and_then(|(_, tail)| tail.split_once(' ').map(|(facility, _)| facility))
}
