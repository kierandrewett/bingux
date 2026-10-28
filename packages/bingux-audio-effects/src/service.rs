//! Running the effects: the engine process, the default device and live
//! control.
//!
//! Each direction runs as its own `bingux-effects run <direction>` process,
//! which writes the filter-chain configuration, starts a `pipewire` client
//! with it and makes the virtual device the default. Under systemd that is
//! the user unit `bingux-effects@<direction>.service`, which also starts the
//! effects at login; without a user systemd (a minimal session) the process
//! is started directly and tracked in a pid file.
//!
//! `run` restores the real device as the default when it stops.

use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::catalog::Catalog;
use crate::graph::{self, INPUT_NODE, OUTPUT_NODE, Target};
use crate::preset::{Direction, Preset};
use crate::store::{Store, runtime_dir, write_atomic};

/// One audio node of the running PipeWire graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub id: u32,
    pub name: String,
    pub description: String,
    pub class: String,
}

fn run_command(program: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("{program}: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(format!(
            "{program} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

/// Every audio device and stream node, from `pw-dump`.
pub fn list_nodes() -> Result<Vec<Node>, String> {
    let text = run_command("pw-dump", &["--no-colors"])?;
    let objects: Vec<Value> = serde_json::from_str(&text).map_err(|e| format!("pw-dump: {e}"))?;
    Ok(objects
        .iter()
        .filter(|object| object.get("type").and_then(Value::as_str) == Some("PipeWire:Interface:Node"))
        .filter_map(|object| {
            let props = object.get("info")?.get("props")?;
            let text = |key: &str| props.get(key).and_then(Value::as_str).unwrap_or("").to_string();
            Some(Node {
                id: object.get("id")?.as_u64()? as u32,
                name: text("node.name"),
                description: text("node.description"),
                class: text("media.class"),
            })
        })
        .collect())
}

/// The id of the node named `name`.
pub fn find_node(name: &str) -> Option<u32> {
    list_nodes().ok()?.into_iter().find(|node| node.name == name).map(|node| node.id)
}

/// The class of device a direction's chain works on.
fn device_class(direction: Direction) -> &'static str {
    match direction {
        Direction::Output => "Audio/Sink",
        Direction::Input => "Audio/Source",
    }
}

/// The `node.name` the session uses as the default device of `direction`,
/// leaving out the effects' own virtual devices.
pub fn default_device(direction: Direction) -> Option<String> {
    let key = match direction {
        Direction::Output => "default.audio.sink",
        Direction::Input => "default.audio.source",
    };
    let own = |name: &str| name == OUTPUT_NODE || name == INPUT_NODE;
    for key in [format!("default.configured.{}", &key["default.".len()..]), key.to_string()] {
        let Ok(text) = run_command("pw-metadata", &["-n", "default", "0", &key]) else {
            continue;
        };
        // update: id:0 key:'default.audio.sink' value:'{ "name": "alsa_output.x" }' type:'Spa:String:JSON'
        let value = text
            .split("value:'")
            .nth(1)
            .and_then(|rest| rest.split("' type:").next())
            .and_then(|json| serde_json::from_str::<Value>(json).ok())
            .and_then(|json| json.get("name").and_then(Value::as_str).map(str::to_string));
        if let Some(name) = value.filter(|name| !own(name)) {
            return Some(name);
        }
    }
    // The default is one of ours, or unset: take the first real device.
    list_nodes()
        .ok()?
        .into_iter()
        .find(|node| node.class == device_class(direction) && !own(&node.name))
        .map(|node| node.name)
}

/// Make node `id` the default device of its kind.
pub fn set_default(id: u32) -> Result<(), String> {
    run_command("wpctl", &["set-default", &id.to_string()]).map(|_| ())
}

/// Set node parameters (`node:Control` and a value) on node `id` while it runs.
pub fn set_params(id: u32, params: &[(String, f64)]) -> Result<(), String> {
    if params.is_empty() {
        return Ok(());
    }
    let mut body = String::from("{ params = [");
    for (name, value) in params {
        let value = if value.is_finite() { *value } else { 0.0 };
        body.push_str(&format!(" \"{}\" {value:?}", name.replace('"', "")));
    }
    body.push_str(" ] }");
    run_command("pw-cli", &["s", &id.to_string(), "Props", &body]).map(|_| ())
}

fn pid_path(direction: Direction) -> PathBuf {
    runtime_dir().join(format!("{}.pid", direction.name()))
}

fn process_alive(pid: i32) -> bool {
    // SAFETY: signal 0 only checks that the process exists.
    pid > 0 && unsafe { libc::kill(pid, 0) } == 0
}

fn read_pid(direction: Direction) -> Option<i32> {
    std::fs::read_to_string(pid_path(direction)).ok()?.trim().parse().ok().filter(|pid| process_alive(*pid))
}

/// The meter file of a direction (see `bingux_dsp::meters`).
pub fn meter_path(direction: Direction) -> PathBuf {
    runtime_dir().join(format!("{}.meters", direction.name()))
}

/// The readings of the plugin in meter `slot`, or `None` before the chain
/// has run.
pub fn read_meters(direction: Direction, slot: u32) -> Option<[f32; bingux_dsp::meters::VALUES]> {
    let bytes = std::fs::read(meter_path(direction)).ok()?;
    bingux_dsp::meters::read(&bytes, slot as usize)
}

/// The analyser's spectrum, in dBFS for 64 log-spaced bands from 20 Hz to
/// 20 kHz, or `None` while the analyser has published nothing.
pub fn read_spectrum(direction: Direction) -> Option<[f32; bingux_dsp::meters::BANDS]> {
    let bytes = std::fs::read(meter_path(direction)).ok()?;
    bingux_dsp::meters::read_spectrum(&bytes)
}

/// The `bingux-effects` program: `BINGUX_EFFECTS_BIN`, one beside the running
/// program, or the one on `PATH`.
pub fn effects_program() -> PathBuf {
    if let Some(path) = std::env::var_os("BINGUX_EFFECTS_BIN") {
        return PathBuf::from(path);
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(beside) = exe.parent().map(|dir| dir.join("bingux-effects"))
        && beside.is_file()
    {
        return beside;
    }
    PathBuf::from("bingux-effects")
}

fn unit_name(direction: Direction) -> String {
    format!("bingux-effects@{}.service", direction.name())
}

/// Whether a systemd user manager can be used.
fn systemd_usable() -> bool {
    std::env::var("BINGUX_EFFECTS_SERVICE").map_or(true, |value| value != "process")
        && run_command("systemctl", &["--user", "show", "--property=Version"]).is_ok()
}

fn unit_text(program: &Path) -> String {
    format!(
        "[Unit]\n\
         Description=Bingux sound effects (%i)\n\
         After=pipewire.service wireplumber.service\n\
         PartOf=pipewire.service\n\n\
         [Service]\n\
         ExecStart={} run %i\n\
         Restart=on-failure\n\
         RestartSec=2\n\n\
         [Install]\n\
         WantedBy=default.target\n",
        program.display()
    )
}

/// Write the systemd user unit that runs a direction, when it is missing or
/// names another program.
fn ensure_unit() -> Result<(), String> {
    let Some(home) = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| Path::new(&home).join(".config")))
    else {
        return Err("No home directory.".to_string());
    };
    let path = home.join("systemd/user/bingux-effects@.service");
    let text = unit_text(&effects_program());
    if std::fs::read_to_string(&path).is_ok_and(|current| current == text) {
        return Ok(());
    }
    write_atomic(&path, &text).map_err(|e| e.0)?;
    run_command("systemctl", &["--user", "daemon-reload"]).map(|_| ())
}

/// Whether the effects of `direction` are running.
pub fn is_running(direction: Direction) -> bool {
    if read_pid(direction).is_some() {
        return true;
    }
    systemd_usable()
        && run_command("systemctl", &["--user", "is-active", "--quiet", &unit_name(direction)]).is_ok()
}

/// Start the effects of `direction`, or restart them so they read the
/// preset and settings again. Returns once the process has been started, not
/// once the virtual device exists.
pub fn start(direction: Direction) -> Result<(), String> {
    if systemd_usable() {
        ensure_unit()?;
        let unit = unit_name(direction);
        return run_command("systemctl", &["--user", "enable", "--now", &unit])
            .and_then(|_| run_command("systemctl", &["--user", "restart", &unit]))
            .map(|_| ());
    }
    stop_process(direction);
    std::fs::create_dir_all(runtime_dir()).map_err(|e| e.to_string())?;
    let mut command = Command::new(effects_program());
    command
        .args(["run", direction.name()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // SAFETY: setsid is async-signal-safe; it detaches the engine from this
    // program's session so it keeps running when the app closes.
    unsafe {
        command.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    command.spawn().map(|_| ()).map_err(|e| format!("Could not start bingux-effects: {e}"))
}

/// Stop the effects of `direction` and give back the real default device.
pub fn stop(direction: Direction) -> Result<(), String> {
    if systemd_usable() {
        let unit = unit_name(direction);
        // Not enabled any more: they do not come back at the next login.
        let _ = run_command("systemctl", &["--user", "disable", "--now", &unit]);
        let _ = run_command("systemctl", &["--user", "stop", &unit]);
    }
    stop_process(direction);
    Ok(())
}

fn stop_process(direction: Direction) {
    let Some(pid) = read_pid(direction) else {
        return;
    };
    // SAFETY: a plain signal to a process this program started.
    unsafe { libc::kill(pid, libc::SIGTERM) };
    let deadline = Instant::now() + Duration::from_secs(4);
    while process_alive(pid) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Make the running state match the saved one for `direction`: running when
/// the effects are on, stopped when off.
pub fn apply(store: &Store, direction: Direction) -> Result<(), String> {
    if store.state().of(direction).enabled {
        start(direction)
    } else {
        stop(direction)
    }
}

/// The write end of the pipe the signal handler wakes `run` through.
static WAKE: AtomicI32 = AtomicI32::new(-1);

extern "C" fn on_signal(_: libc::c_int) {
    let fd = WAKE.load(Ordering::Relaxed);
    if fd >= 0 {
        let byte = 1u8;
        // SAFETY: write is async-signal-safe.
        unsafe { libc::write(fd, (&raw const byte).cast(), 1) };
    }
}

/// The preset the engine uses for the saved state: the preset of the
/// device when one is set to load for it, else the chosen preset.
pub fn active_preset(store: &Store, direction: Direction, device: Option<&str>) -> Preset {
    let state = store.state();
    let direction_state = state.of(direction);
    let name = device
        .and_then(|device| direction_state.autoload.get(device))
        .filter(|name| store.exists(direction, name))
        .unwrap_or(&direction_state.preset);
    store
        .load(direction, name)
        .unwrap_or_else(|_| Preset::new("Flat", direction))
}

/// Run the engine of `direction` until it is told to stop. This is what
/// `bingux-effects run` does.
pub fn run(store: &Store, direction: Direction) -> Result<(), String> {
    store.ensure_factory().map_err(|e| e.0)?;
    let state = store.state();
    let catalog = Catalog::load();
    let device = state.of(direction).device.clone().or_else(|| default_device(direction));
    // Remember the device the effects started on, so the Sound page can show it.
    if state.of(direction).device.is_none()
        && let Some(found) = device.clone()
    {
        let _ = store.update_state(|state| state.of_mut(direction).device = Some(found));
    }
    let preset = active_preset(store, direction, device.as_deref());
    let rendered = graph::render(
        &preset,
        &catalog,
        &Target {
            device: device.clone(),
            description: None,
        },
    );
    let directory = runtime_dir();
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let config_path = directory.join(format!("{}.conf", direction.name()));
    write_atomic(&config_path, &rendered.config).map_err(|e| e.0)?;
    // A fresh, zeroed meter file, so readings from an earlier run do not show.
    let meters = meter_path(direction);
    std::fs::write(&meters, bingux_dsp::meters::empty_file()).map_err(|e| e.to_string())?;
    std::fs::write(pid_path(direction), std::process::id().to_string()).map_err(|e| e.to_string())?;

    let mut pipe = [0i32; 2];
    // SAFETY: plain libc calls; the descriptors are used only here and in the handler.
    unsafe {
        if libc::pipe2(pipe.as_mut_ptr(), libc::O_CLOEXEC | libc::O_NONBLOCK) != 0 {
            return Err("Could not create a pipe.".to_string());
        }
        WAKE.store(pipe[1], Ordering::Relaxed);
        libc::signal(libc::SIGTERM, on_signal as *const () as libc::sighandler_t);
        libc::signal(libc::SIGINT, on_signal as *const () as libc::sighandler_t);
    }

    let log = std::fs::File::create(directory.join(format!("{}.log", direction.name()))).ok();
    let mut command = Command::new("pipewire");
    command
        .arg("-c")
        .arg(&config_path)
        .env(bingux_dsp::meters::ENV, &meters)
        .stdin(Stdio::null())
        .stdout(log.as_ref().and_then(|file| file.try_clone().ok()).map_or_else(Stdio::null, Stdio::from))
        .stderr(log.map_or_else(Stdio::null, Stdio::from));
    let mut child = command.spawn().map_err(|e| format!("Could not start pipewire: {e}"))?;

    // Wait for the virtual device, then make it the default.
    let own = graph::node_name(direction);
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut routed = None;
    while Instant::now() < deadline && routed.is_none() {
        if let Some(id) = find_node(own) {
            routed = Some(id);
        } else {
            std::thread::sleep(Duration::from_millis(150));
        }
        if child.try_wait().ok().flatten().is_some() {
            break;
        }
    }
    if let Some(id) = routed {
        let _ = set_default(id);
    }

    // Sleep until the engine process ends or this one is told to stop.
    let pidfd = unsafe { libc::syscall(libc::SYS_pidfd_open, child.id() as libc::c_int, 0) } as libc::c_int;
    let mut fds = [
        libc::pollfd { fd: pipe[0], events: libc::POLLIN, revents: 0 },
        libc::pollfd { fd: pidfd, events: libc::POLLIN, revents: 0 },
    ];
    let polled = if pidfd >= 0 { 2 } else { 1 };
    let asked = loop {
        // SAFETY: `fds` holds `polled` valid entries.
        let ready = unsafe { libc::poll(fds.as_mut_ptr(), polled, if pidfd >= 0 { -1 } else { 500 }) };
        if ready > 0 && fds[0].revents != 0 {
            break true;
        }
        if (ready > 0 && pidfd >= 0 && fds[1].revents != 0) || child.try_wait().ok().flatten().is_some() {
            break false;
        }
    };
    if asked {
        let _ = child.kill();
    }
    let _ = child.wait();
    // Give the real device back, if the effects were its default.
    if let Some(device) = device.as_deref()
        && let Some(id) = find_node(device)
    {
        let _ = set_default(id);
    }
    let _ = std::fs::remove_file(pid_path(direction));
    if asked { Ok(()) } else { Err("The effects engine ended unexpectedly.".to_string()) }
}
