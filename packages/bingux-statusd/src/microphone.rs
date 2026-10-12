use serde::Serialize;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    io::{self, Read},
    os::unix::{io::AsRawFd, process::CommandExt},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const MAX_SNAPSHOT_BYTES: usize = 1024 * 1024;
const MAX_CAPTURES: usize = 32;
const MAX_FIELD_BYTES: usize = 128;
const MAX_SUBSCRIBE_LINE_BYTES: usize = 8 * 1024;
const SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(3);
const RECONNECT_DELAY: Duration = Duration::from_secs(2);
const EVENT_DEBOUNCE: Duration = Duration::from_millis(150);

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Capture {
    pub app: String,
    #[serde(rename = "appId")]
    pub app_id: String,
    pub device: String,
    pub muted: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CaptureSnapshot {
    pub available: bool,
    pub captures: Vec<Capture>,
}

/// Parse the two bounded pactl JSON snapshots into the shell capture contract.
pub fn parse_snapshot(
    sources_json: &str,
    outputs_json: &str,
) -> Result<CaptureSnapshot, &'static str> {
    if sources_json.len() > MAX_SNAPSHOT_BYTES || outputs_json.len() > MAX_SNAPSHOT_BYTES {
        return Err("pactl snapshot exceeds the input limit");
    }
    let sources: Vec<Value> =
        serde_json::from_str(sources_json).map_err(|_| "invalid sources JSON")?;
    let outputs: Vec<Value> =
        serde_json::from_str(outputs_json).map_err(|_| "invalid source outputs JSON")?;
    let mut sources_by_index = BTreeMap::new();
    for source in sources {
        if let Some(index) = source.get("index").and_then(Value::as_u64) {
            sources_by_index.insert(index, source);
        }
    }

    let mut captures = Vec::new();
    for stream in outputs {
        if captures.len() == MAX_CAPTURES {
            break;
        }
        let properties = stream.get("properties").and_then(Value::as_object);
        if ["stream.monitor", "stream.capture.sink", "resample.peaks"]
            .iter()
            .any(|key| property_is_true(properties, key))
            || stream.get("corked").and_then(Value::as_bool) == Some(true)
        {
            continue;
        }
        let source = stream
            .get("source")
            .and_then(Value::as_u64)
            .and_then(|index| sources_by_index.get(&index));
        if source.is_some_and(is_monitor_source) {
            continue;
        }
        let app = property(properties, "application.name")
            .or_else(|| property(properties, "application.process.binary"))
            .unwrap_or("Unknown application");
        let app_id = property(properties, "application.id")
            .or_else(|| property(properties, "application.desktop"))
            .or_else(|| property(properties, "application.process.binary"))
            .unwrap_or("");
        let device = source
            .and_then(|value| {
                value
                    .get("description")
                    .and_then(Value::as_str)
                    .or_else(|| value.get("name").and_then(Value::as_str))
            })
            .unwrap_or("Audio input");
        if !valid_field(app) || !valid_field(app_id) || !valid_field(device) {
            return Err("capture field exceeds the protocol limit");
        }
        let capture = Capture {
            app: app.to_owned(),
            app_id: app_id.to_owned(),
            device: device.to_owned(),
            muted: stream.get("mute").and_then(Value::as_bool).unwrap_or(false)
                || source
                    .and_then(|value| value.get("mute").and_then(Value::as_bool))
                    .unwrap_or(false),
        };
        if !captures.contains(&capture) {
            captures.push(capture);
        }
    }
    Ok(CaptureSnapshot {
        available: true,
        captures,
    })
}

fn property<'a>(
    properties: Option<&'a serde_json::Map<String, Value>>,
    key: &str,
) -> Option<&'a str> {
    properties
        .and_then(|properties| properties.get(key))
        .and_then(Value::as_str)
}
fn property_is_true(properties: Option<&serde_json::Map<String, Value>>, key: &str) -> bool {
    properties
        .and_then(|properties| properties.get(key))
        .is_some_and(|value| {
            value.as_bool() == Some(true)
                || value
                    .as_str()
                    .is_some_and(|text| text.eq_ignore_ascii_case("true"))
        })
}
fn is_monitor_source(source: &Value) -> bool {
    if source
        .get("name")
        .and_then(Value::as_str)
        .is_some_and(|name| name.ends_with(".monitor"))
    {
        return true;
    }
    match source
        .get("monitor_of_sink")
        .or_else(|| source.get("monitor_of_sink_name"))
    {
        None | Some(Value::Null) => false,
        Some(Value::String(value)) => !value.is_empty() && value != "4294967295",
        Some(Value::Number(value)) => value.as_u64() != Some(4_294_967_295),
        Some(_) => true,
    }
}
fn valid_field(value: &str) -> bool {
    value.len() <= MAX_FIELD_BYTES && !value.chars().any(char::is_control)
}

/// Start the optional PulseAudio/PipeWire capture watcher. It only owns the
/// additive capture fields; Gnoblin privacy state remains independent.
pub fn start(publish: impl Fn(CaptureSnapshot) -> bool + Send + 'static) {
    thread::spawn(move || run(publish));
}

fn run(publish: impl Fn(CaptureSnapshot) -> bool) {
    loop {
        let result = watch_once(&publish);
        if !publish(CaptureSnapshot::default()) {
            return;
        }
        if let Err(error) = result {
            eprintln!("[bingux-statusd] microphone state unavailable: {error}");
        }
        thread::sleep(RECONNECT_DELAY);
    }
}

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

fn watch_once(publish: &impl Fn(CaptureSnapshot) -> bool) -> Result<(), String> {
    let mut subscribe = TerminateChild(spawn_pactl(&["subscribe"])?);
    let initial = snapshot()?;
    if !publish(initial.clone()) {
        return Err("metrics receiver stopped".to_owned());
    }
    let stdout = subscribe
        .0
        .stdout
        .take()
        .ok_or("pactl subscribe has no stdout")?;
    let mut reader = SubscriptionReader::new(stdout);
    let mut previous = initial;
    loop {
        let Some(line) = reader.next_line(Duration::from_secs(60))? else {
            continue;
        };
        if !affects_capture(&line) {
            continue;
        }
        let deadline = Instant::now() + EVENT_DEBOUNCE;
        while Instant::now() < deadline {
            if reader
                .next_line(deadline.saturating_duration_since(Instant::now()))?
                .is_none()
            {
                break;
            }
        }
        let current = snapshot()?;
        if current != previous {
            if !publish(current.clone()) {
                return Err("metrics receiver stopped".to_owned());
            }
            previous = current;
        }
    }
}
fn affects_capture(event: &str) -> bool {
    matches!(
        event
            .trim()
            .rsplit_once(" on ")
            .and_then(|(_, tail)| tail.split_once(' ').map(|(facility, _)| facility)),
        Some("source" | "source-output" | "server")
    )
}

fn snapshot() -> Result<CaptureSnapshot, String> {
    let sources = pactl_json("sources")?;
    let outputs = pactl_json("source-outputs")?;
    parse_snapshot(&sources, &outputs).map_err(str::to_owned)
}

fn pactl_json(kind: &str) -> Result<String, String> {
    run_command(&["-f", "json", "list", kind])
}

fn run_command(arguments: &[&str]) -> Result<String, String> {
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
