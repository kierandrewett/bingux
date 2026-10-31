//! Running the effects: the engine process, the default device and live
//! control.
//!
//! Each direction runs as its own `bingux-effects run <direction>` process,
//! the *supervisor*. It starts one `pipewire` process for each stage of the
//! chain (see `graph`): the head, the tail and one for every effect, and joins
//! them with links. Under systemd that is the user unit
//! `bingux-effects@<direction>.service`, which also starts the effects at
//! login; without a user systemd (a minimal session) the process is started
//! directly and tracked in a pid file.
//!
//! The preset file is the one source of truth. Whoever changes it (the
//! Settings app) sends `SIGHUP`; the supervisor reads the preset again, works
//! out from `chain::plan` what differs from what runs, and does only that:
//!
//! - a control moved: set live in the stage, nothing else touches the sound;
//! - an effect switched off, on or moved: some links change, every stage
//!   keeps its state;
//! - an effect added: its stage starts beside the running ones and joins the
//!   chain when it is ready;
//! - an effect removed: its stage stops and the links close over the gap.
//!
//! A stage that exits unexpectedly leaves the chain (the rest keeps playing)
//! and is started again after a pause, up to three times.
//!
//! `run` makes the head the default device and restores the real device as
//! the default when it stops.

use std::collections::HashMap;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::catalog::{Catalog, PluginInfo};
use crate::chain::{self, Link, Plan, Running};
use crate::effects;
use crate::graph::{self, INPUT_NODE, OUTPUT_NODE, Role, Target};
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
        .filter(|object| {
            object.get("type").and_then(Value::as_str) == Some("PipeWire:Interface:Node")
        })
        .filter_map(|object| {
            let props = object.get("info")?.get("props")?;
            let text = |key: &str| {
                props
                    .get(key)
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string()
            };
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
    list_nodes()
        .ok()?
        .into_iter()
        .find(|node| node.name == name)
        .map(|node| node.id)
}

/// The class of device a direction's chain works on.
fn device_class(direction: Direction) -> &'static str {
    match direction {
        Direction::Output => "Audio/Sink",
        Direction::Input => "Audio/Source",
    }
}

/// Whether `name` is one of the engine's own nodes.
fn is_own(name: &str) -> bool {
    name == OUTPUT_NODE
        || name == INPUT_NODE
        || name.starts_with("bingux_fx.")
        || name.starts_with("bingux_effects_")
}

/// The `node.name` the session uses as the default device of `direction`,
/// leaving out the effects' own virtual devices.
pub fn default_device(direction: Direction) -> Option<String> {
    let key = match direction {
        Direction::Output => "default.audio.sink",
        Direction::Input => "default.audio.source",
    };
    for key in [
        format!("default.configured.{}", &key["default.".len()..]),
        key.to_string(),
    ] {
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
        if let Some(name) = value.filter(|name| !is_own(name)) {
            return Some(name);
        }
    }
    // The default is one of ours, or unset: take the first real device.
    list_nodes()
        .ok()?
        .into_iter()
        .find(|node| node.class == device_class(direction) && !is_own(&node.name))
        .map(|node| node.name)
}

/// Make node `id` the default device of its kind.
pub fn set_default(id: u32) -> Result<(), String> {
    run_command("wpctl", &["set-default", &id.to_string()]).map(|_| ())
}

/// Set node parameters (`node:symbol` and a value) on node `id` while it runs.
pub fn set_params(id: u32, params: &[(String, f64)]) -> Result<(), String> {
    if params.is_empty() {
        return Ok(());
    }
    run_command(
        "pw-cli",
        &["s", &id.to_string(), "Props", &props_json(params)],
    )
    .map(|_| ())
}

/// The `Props` value that sets `params`.
fn props_json(params: &[(String, f64)]) -> String {
    let mut body = String::from("{ params = [");
    for (name, value) in params {
        let value = if value.is_finite() { *value } else { 0.0 };
        body.push_str(&format!(" \"{}\" {value:?}", name.replace('"', "")));
    }
    body.push_str(" ] }");
    body
}

fn pid_path(direction: Direction) -> PathBuf {
    runtime_dir().join(format!("{}.pid", direction.name()))
}

fn process_alive(pid: i32) -> bool {
    // SAFETY: signal 0 only checks that the process exists.
    pid > 0 && unsafe { libc::kill(pid, 0) } == 0
}

fn read_pid(direction: Direction) -> Option<i32> {
    std::fs::read_to_string(pid_path(direction))
        .ok()?
        .trim()
        .parse()
        .ok()
        .filter(|pid| process_alive(*pid))
}

/// The status file of a direction (see [`Status`]).
pub fn status_path(direction: Direction) -> PathBuf {
    runtime_dir().join(format!("{}.status.json", direction.name()))
}

/// What the running stack reports, written by the supervisor after each change.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Status {
    /// The real device the chain works on.
    pub device: Option<String>,
    pub stages: Vec<StageStatus>,
}

/// One effect's stage.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StageStatus {
    pub id: String,
    pub state: StageState,
    /// Why the stage is not running, in plain language.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StageState {
    /// In the audio path.
    Running,
    /// Running but switched off.
    Off,
    /// Starting.
    Starting,
    /// Could not start or stopped.
    Failed,
}

/// What the supervisor of `direction` last reported, or `None` when it is not running.
pub fn read_status(direction: Direction) -> Option<Status> {
    serde_json::from_str(&std::fs::read_to_string(status_path(direction)).ok()?).ok()
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
        && run_command(
            "systemctl",
            &["--user", "is-active", "--quiet", &unit_name(direction)],
        )
        .is_ok()
}

/// Start the effects of `direction`. Returns once the process has been
/// started, not once the virtual device exists.
pub fn start(direction: Direction) -> Result<(), String> {
    if systemd_usable() {
        ensure_unit()?;
        let unit = unit_name(direction);
        return run_command("systemctl", &["--user", "enable", "--now", &unit]).map(|_| ());
    }
    if read_pid(direction).is_some() {
        return Ok(());
    }
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
    command
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Could not start bingux-effects: {e}"))
}

/// Tell the running supervisor of `direction` that the preset or the settings
/// changed, so it reads them again and changes only what differs. Starts the
/// effects when they are not running.
pub fn reload(direction: Direction) -> Result<(), String> {
    match read_pid(direction) {
        // SAFETY: a plain signal to a process this program started.
        Some(pid) => {
            unsafe { libc::kill(pid, libc::SIGHUP) };
            Ok(())
        }
        None => start(direction),
    }
}

/// Stop the effects of `direction` and give back the real default device.
pub fn stop(direction: Direction) -> Result<(), String> {
    if systemd_usable() {
        let unit = unit_name(direction);
        // Not enabled any more: they do not come back at the next login.
        let _ = run_command("systemctl", &["--user", "disable", "--now", &unit]);
        let _ = run_command("systemctl", &["--user", "stop", &unit]);
    }
    if let Some(pid) = read_pid(direction) {
        // SAFETY: a plain signal to a process this program started.
        unsafe { libc::kill(pid, libc::SIGTERM) };
        let deadline = Instant::now() + Duration::from_secs(6);
        while process_alive(pid) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    Ok(())
}

/// Make the running state match the saved one for `direction`: running when
/// the effects are on, stopped when off.
pub fn apply(store: &Store, direction: Direction) -> Result<(), String> {
    if store.state().of(direction).enabled {
        reload(direction)
    } else {
        stop(direction)
    }
}

/// The write end of the pipe the signal handler wakes `run` through.
static WAKE: AtomicI32 = AtomicI32::new(-1);

extern "C" fn on_signal(signal: libc::c_int) {
    let fd = WAKE.load(Ordering::Relaxed);
    if fd >= 0 {
        let byte = if signal == libc::SIGHUP { b'h' } else { b't' };
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

/// How long a stage may take to appear in the graph.
const READY_TIMEOUT: Duration = Duration::from_secs(8);
/// The longest a new stage is given to start processing before it joins the chain.
const WARM_UP: Duration = Duration::from_millis(800);
/// A stage that keeps exiting is not started again after this many tries.
const MAX_RESTARTS: u32 = 3;

/// One port of the graph, as `pw-dump` shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Port {
    id: u32,
    node: u32,
    /// `node.name:port.name`, the name `pw-link` shows.
    name: String,
    output: bool,
}

/// The nodes, ports and links of the graph at one moment.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Snapshot {
    nodes: Vec<Node>,
    /// Node id and its state (`running`, `idle`, ...).
    states: Vec<(u32, String)>,
    ports: Vec<Port>,
    /// Link id, output port id, input port id.
    links: Vec<(u32, u32, u32)>,
}

impl Snapshot {
    fn read() -> Result<Self, String> {
        let text = run_command("pw-dump", &["--no-colors"])?;
        let objects: Vec<Value> =
            serde_json::from_str(&text).map_err(|e| format!("pw-dump: {e}"))?;
        Ok(Self::from_objects(&objects))
    }

    fn from_objects(objects: &[Value]) -> Self {
        let mut snapshot = Self::default();
        let kind = |object: &Value| {
            object
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        };
        for object in objects
            .iter()
            .filter(|object| kind(object) == "PipeWire:Interface:Node")
        {
            let Some(props) = object.get("info").and_then(|info| info.get("props")) else {
                continue;
            };
            let text = |key: &str| {
                props
                    .get(key)
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string()
            };
            if let Some(id) = object.get("id").and_then(Value::as_u64) {
                let state = object
                    .get("info")
                    .and_then(|info| info.get("state"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                snapshot.states.push((id as u32, state.to_string()));
                snapshot.nodes.push(Node {
                    id: id as u32,
                    name: text("node.name"),
                    description: text("node.description"),
                    class: text("media.class"),
                });
            }
        }
        for object in objects
            .iter()
            .filter(|object| kind(object) == "PipeWire:Interface:Port")
        {
            let Some(info) = object.get("info") else {
                continue;
            };
            let props = info.get("props");
            let node = props
                .and_then(|props| props.get("node.id"))
                .and_then(Value::as_u64);
            let name = props
                .and_then(|props| props.get("port.name"))
                .and_then(Value::as_str);
            let (Some(id), Some(node), Some(name)) =
                (object.get("id").and_then(Value::as_u64), node, name)
            else {
                continue;
            };
            let node_name = snapshot
                .nodes
                .iter()
                .find(|candidate| u64::from(candidate.id) == node)
                .map_or("", |node| node.name.as_str());
            snapshot.ports.push(Port {
                id: id as u32,
                node: node as u32,
                name: format!("{node_name}:{name}"),
                output: info.get("direction").and_then(Value::as_str) == Some("output"),
            });
        }
        for object in objects
            .iter()
            .filter(|object| kind(object) == "PipeWire:Interface:Link")
        {
            let Some(info) = object.get("info") else {
                continue;
            };
            let number = |key: &str| info.get(key).and_then(Value::as_u64).map(|n| n as u32);
            if let (Some(id), Some(output), Some(input)) = (
                object.get("id").and_then(Value::as_u64),
                number("output-port-id"),
                number("input-port-id"),
            ) {
                snapshot.links.push((id as u32, output, input));
            }
        }
        snapshot
    }

    /// Whether the node named `name` is processing sound.
    fn is_running(&self, name: &str) -> bool {
        self.nodes
            .iter()
            .find(|node| node.name == name)
            .is_some_and(|node| {
                self.states
                    .iter()
                    .any(|(id, state)| *id == node.id && state == "running")
            })
    }

    fn port(&self, name: &str) -> Option<&Port> {
        self.ports.iter().find(|port| port.name == name)
    }

    fn port_name(&self, id: u32) -> Option<&str> {
        self.ports
            .iter()
            .find(|port| port.id == id)
            .map(|port| port.name.as_str())
    }

    fn has_ports(&self, names: &[String]) -> bool {
        names.iter().all(|name| self.port(name).is_some())
    }

    /// The links between two of the engine's own ports.
    fn own_links(&self) -> Vec<(u32, Link)> {
        self.links
            .iter()
            .filter_map(|&(id, output, input)| {
                let (output, input) = (self.port_name(output)?, self.port_name(input)?);
                (own_port(output) && own_port(input)).then(|| {
                    (
                        id,
                        Link {
                            output: output.to_string(),
                            input: input.to_string(),
                        },
                    )
                })
            })
            .collect()
    }
}

/// Whether the port belongs to one of the engine's stages.
fn own_port(port: &str) -> bool {
    let node = port.split(':').next().unwrap_or("");
    is_own(node)
}

/// A long-lived `pw-cli` the supervisor writes commands to. Changing links
/// through one open connection takes a fraction of a millisecond a command,
/// where starting `pw-link` for each takes some twenty.
struct Session {
    child: Child,
    stdin: std::process::ChildStdin,
}

impl Session {
    fn start() -> Option<Self> {
        let mut child = Command::new("pw-cli")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let stdin = child.stdin.take()?;
        Some(Self { child, stdin })
    }

    /// Send `commands`, one line each, in a single write so they reach the
    /// server together.
    fn send(&mut self, commands: &[String]) -> bool {
        use std::io::Write;
        if commands.is_empty() {
            return true;
        }
        let mut text = commands.join("\n");
        text.push('\n');
        self.stdin
            .write_all(text.as_bytes())
            .and_then(|()| self.stdin.flush())
            .is_ok()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A stage and the `pipewire` process that runs it.
struct Stage {
    role: Role,
    child: Option<Child>,
    /// What the stage was started with and set to since (effects only).
    running: Option<Running>,
    node_id: Option<u32>,
    started: Instant,
    ready: bool,
    failure: Option<String>,
    restarts: u32,
    retry_at: Option<Instant>,
}

struct Engine {
    direction: Direction,
    catalog: Catalog,
    target: Target,
    directory: PathBuf,
    head: Stage,
    tail: Stage,
    effects: HashMap<String, Stage>,
    /// Effects that cannot run, with the reason.
    skipped: HashMap<String, String>,
    order: Vec<String>,
    default_set: bool,
    session: Option<Session>,
}

impl Stage {
    fn new(role: Role) -> Self {
        Self {
            role,
            child: None,
            running: None,
            node_id: None,
            started: Instant::now(),
            ready: false,
            failure: None,
            restarts: 0,
            retry_at: None,
        }
    }

    fn alive(&mut self) -> bool {
        self.child
            .as_mut()
            .is_some_and(|child| child.try_wait().ok().flatten().is_none())
    }

    /// Stop the process and wait for it.
    fn kill(&mut self) {
        if let Some(mut child) = self.child.take() {
            // SAFETY: a plain signal to a process this program started.
            unsafe { libc::kill(child.id() as i32, libc::SIGTERM) };
            let deadline = Instant::now() + Duration::from_secs(2);
            while child.try_wait().ok().flatten().is_none() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            if child.try_wait().ok().flatten().is_none() {
                let _ = child.kill();
            }
            let _ = child.wait();
        }
        self.ready = false;
    }
}

fn spawn_pipewire(config: &Path, log: &Path) -> std::io::Result<Child> {
    let file = std::fs::File::create(log)?;
    let mut command = Command::new("pipewire");
    command
        .arg("-c")
        .arg(config)
        .stdin(Stdio::null())
        .stdout(file.try_clone()?)
        .stderr(file);
    // SAFETY: prctl is async-signal-safe; the stage dies with the supervisor.
    unsafe {
        command.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            Ok(())
        });
    }
    command.spawn()
}

impl Engine {
    fn new(
        direction: Direction,
        catalog: Catalog,
        device: Option<String>,
    ) -> std::io::Result<Self> {
        let directory = runtime_dir().join(direction.name());
        std::fs::create_dir_all(&directory)?;
        Ok(Self {
            direction,
            catalog,
            target: Target { device },
            directory,
            head: Stage::new(Role::Head),
            tail: Stage::new(Role::Tail),
            effects: HashMap::new(),
            skipped: HashMap::new(),
            order: Vec::new(),
            default_set: false,
            session: Session::start(),
        })
    }

    fn start_end(&mut self, role: &Role) {
        let rendered = graph::render_end(self.direction, role, &self.target);
        let file = if *role == Role::Head { "head" } else { "tail" };
        let config = self.directory.join(format!("{file}.conf"));
        let log = self.directory.join(format!("{file}.log"));
        let result = write_atomic(&config, &rendered.config)
            .map_err(|e| e.0)
            .and_then(|()| spawn_pipewire(&config, &log).map_err(|e| e.to_string()));
        let stage = if *role == Role::Head {
            &mut self.head
        } else {
            &mut self.tail
        };
        stage.started = Instant::now();
        match result {
            Ok(child) => stage.child = Some(child),
            Err(message) => stage.failure = Some(message),
        }
    }

    fn start_effect(&mut self, slot_id: &str, preset: &Preset) {
        let Some(slot) = preset.slot(slot_id) else {
            return;
        };
        let Some(effect) = effects::find(&slot.kind) else {
            return;
        };
        let plugin: Option<&PluginInfo> = self.catalog.plugin_of(effect);
        let mut stage = self
            .effects
            .remove(slot_id)
            .unwrap_or_else(|| Stage::new(Role::Effect(slot_id.to_string())));
        if stage.running.as_ref() != Some(&Running::of(slot)) {
            stage.restarts = 0;
            stage.retry_at = None;
        }
        stage.started = Instant::now();
        stage.ready = false;
        stage.node_id = None;
        match graph::render_effect(self.direction, slot, effect, plugin) {
            Err(skipped) => {
                self.skipped.insert(slot_id.to_string(), skipped.reason);
                stage.child = None;
                stage.running = None;
            }
            Ok(rendered) => {
                self.skipped.remove(slot_id);
                let config = self.directory.join(format!("{slot_id}.conf"));
                let log = self.directory.join(format!("{slot_id}.log"));
                match write_atomic(&config, &rendered.config)
                    .map_err(|e| e.0)
                    .and_then(|()| spawn_pipewire(&config, &log).map_err(|e| e.to_string()))
                {
                    Ok(child) => {
                        stage.child = Some(child);
                        stage.running = Some(Running::of(slot));
                        stage.failure = None;
                    }
                    Err(message) => stage.failure = Some(message),
                }
            }
        }
        self.effects.insert(slot_id.to_string(), stage);
    }

    /// Wait until every stage that was started shows its ports in the graph, or
    /// has exited, or the time is up. A stage that never appears is marked failed.
    fn wait_ready(&mut self) {
        let direction = self.direction;
        loop {
            let snapshot = Snapshot::read().unwrap_or_default();
            let now = Instant::now();
            let mut pending = false;
            let mut check = |stage: &mut Stage| {
                if stage.ready || stage.child.is_none() {
                    return;
                }
                if snapshot.has_ports(&graph::input_ports(direction, &stage.role))
                    && snapshot.has_ports(&graph::output_ports(direction, &stage.role))
                {
                    stage.ready = true;
                } else if !stage.alive() {
                    stage.failure = Some(failure_reason(direction, &stage.role));
                    stage.child = None;
                } else if now.duration_since(stage.started) > READY_TIMEOUT {
                    stage.failure = Some("The plugin did not start in time.".to_string());
                    stage.kill();
                } else {
                    pending = true;
                }
            };
            check(&mut self.head);
            check(&mut self.tail);
            for stage in self.effects.values_mut() {
                check(stage);
            }
            if !pending {
                return;
            }
            std::thread::sleep(Duration::from_millis(40));
        }
    }

    /// Find the node ids of stages that are ready and have none yet.
    fn resolve_nodes(&mut self) {
        if self
            .effects
            .values()
            .all(|stage| stage.node_id.is_some() || !stage.ready)
        {
            return;
        }
        let Ok(nodes) = list_nodes() else {
            return;
        };
        let direction = self.direction;
        for stage in self
            .effects
            .values_mut()
            .filter(|stage| stage.ready && stage.node_id.is_none())
        {
            let name = graph::names(direction, &stage.role).capture;
            stage.node_id = nodes
                .iter()
                .find(|node| node.name == name)
                .map(|node| node.id);
        }
    }

    /// Bring the running stages to `preset`.
    fn reconcile(&mut self, preset: &Preset) {
        let running: Vec<Running> = self
            .effects
            .values()
            .filter(|stage| stage.child.is_some() && stage.failure.is_none())
            .filter_map(|stage| stage.running.clone())
            .collect();
        let plan: Plan = chain::plan(&running, preset, &self.catalog);
        // Stages that failed and whose slot is the same stay stopped until their retry time.
        let now = Instant::now();
        let start: Vec<String> = plan
            .start
            .iter()
            .filter(|id| match self.effects.get(*id) {
                Some(stage) if stage.failure.is_some() => {
                    let unchanged =
                        stage.running.as_ref() == preset.slot(id).map(Running::of).as_ref();
                    !(unchanged
                        && (stage.restarts >= MAX_RESTARTS
                            || stage.retry_at.is_some_and(|at| at > now)))
                }
                _ => true,
            })
            .cloned()
            .collect();
        // A stage that starts again under its own name has to be gone first. One
        // that only leaves stays until the links have closed over the gap.
        let mut leaving = Vec::new();
        for id in &plan.stop {
            if let Some(mut stage) = self.effects.remove(id) {
                if start.contains(id) {
                    stage.kill();
                } else {
                    leaving.push(stage);
                }
            }
        }
        // Effects that left the preset or cannot run any more are forgotten.
        self.effects.retain(|id, _| preset.slot(id).is_some());
        self.skipped.retain(|id, _| preset.slot(id).is_some());
        for id in &start {
            self.start_effect(id, preset);
        }
        self.wait_ready();
        self.warm_up(&start, &plan.order);
        self.resolve_nodes();
        let mut params: HashMap<u32, Vec<(String, f64)>> = HashMap::new();
        for change in &plan.params {
            let Some(effect) = preset
                .slot(&change.id)
                .and_then(|slot| effects::find(&slot.kind))
            else {
                continue;
            };
            if let Some(stage) = self.effects.get_mut(&change.id)
                && let Some(node) = stage.node_id
            {
                for (name, value) in graph::live_params(effect, &change.symbol, change.value) {
                    params.entry(node).or_default().push((name, value));
                }
            }
        }
        let commands: Vec<String> = params
            .iter()
            .map(|(node, values)| format!("set-param {node} Props {}", props_json(values)))
            .collect();
        self.send(&commands);
        // The stored controls are what the stage now has.
        for slot in &preset.plugins {
            if let Some(stage) = self.effects.get_mut(&slot.id)
                && let Some(running) = stage.running.as_mut()
            {
                running.controls = slot.controls.clone();
            }
        }
        self.order = plan
            .order
            .iter()
            .filter(|id| {
                self.effects
                    .get(*id)
                    .is_some_and(|stage| stage.ready && stage.failure.is_none())
            })
            .cloned()
            .collect();
        self.apply_links();
        for stage in &mut leaving {
            stage.kill();
        }
        self.write_status(preset);
    }

    /// Feed the stages that just started from the head, and wait until they
    /// are processing. A stage that is linked in cold takes a few cycles to
    /// start, and the sound behind it breaks for that long; one that is already
    /// running joins the chain with only the links changing.
    fn warm_up(&mut self, started: &[String], order: &[String]) {
        let fresh: Vec<&String> = started
            .iter()
            .filter(|id| {
                order.contains(id) && self.effects.get(*id).is_some_and(|stage| stage.ready)
            })
            .collect();
        if fresh.is_empty() {
            return;
        }
        let Ok(snapshot) = Snapshot::read() else {
            return;
        };
        let head = graph::output_ports(self.direction, &Role::Head);
        let mut commands = Vec::new();
        for id in &fresh {
            let inputs = graph::input_ports(self.direction, &Role::Effect((*id).clone()));
            for channel in 0..2 {
                if let (Some(output), Some(input)) = (
                    snapshot.port(&head[channel]),
                    snapshot.port(&inputs[channel]),
                ) {
                    commands.push(format!(
                        "create-link {} {} {} {} object.linger=true",
                        output.node, output.id, input.node, input.id
                    ));
                }
            }
        }
        self.send(&commands);
        let deadline = Instant::now() + WARM_UP;
        while Instant::now() < deadline {
            let Ok(snapshot) = Snapshot::read() else {
                return;
            };
            let direction = self.direction;
            if fresh.iter().all(|id| {
                snapshot.is_running(&graph::names(direction, &Role::Effect((*id).clone())).capture)
            }) {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Make the links between the stages what the order says. The changes go
    /// to the server in one write, so the break in the sound, if there is
    /// one, lasts for the server to handle a handful of requests.
    fn apply_links(&mut self) {
        let wanted = chain::links(self.direction, &self.order);
        let Ok(snapshot) = Snapshot::read() else {
            return;
        };
        let current = snapshot.own_links();
        let current_links: Vec<Link> = current.iter().map(|(_, link)| link.clone()).collect();
        let (remove, add) = chain::diff_links(&current_links, &wanted);
        let destroy: Vec<String> = current
            .iter()
            .filter(|(_, link)| remove.contains(link))
            .map(|(id, _)| format!("destroy {id}"))
            .collect();
        let create: Vec<String> = add
            .iter()
            .filter_map(|link| {
                let (output, input) = (snapshot.port(&link.output)?, snapshot.port(&link.input)?);
                Some(format!(
                    "create-link {} {} {} {} object.linger=true",
                    output.node, output.id, input.node, input.id
                ))
            })
            .collect();
        // Remove first, then make: an input never hears two sources at once.
        let mut commands = destroy;
        commands.extend(create);
        self.send(&commands);
    }

    /// Send commands to the `pw-cli` session, starting a new one if it has gone.
    fn send(&mut self, commands: &[String]) {
        for _ in 0..2 {
            if self.session.is_none() {
                self.session = Session::start();
            }
            if self
                .session
                .as_mut()
                .is_some_and(|session| session.send(commands))
            {
                return;
            }
            self.session = None;
        }
    }

    fn write_status(&self, preset: &Preset) {
        let stages = preset
            .plugins
            .iter()
            .map(|slot| {
                let stage = self.effects.get(&slot.id);
                let (state, reason) = if let Some(reason) = self.skipped.get(&slot.id) {
                    (StageState::Failed, Some(reason.clone()))
                } else if let Some(stage) = stage {
                    if let Some(reason) = &stage.failure {
                        (StageState::Failed, Some(reason.clone()))
                    } else if !stage.ready {
                        (StageState::Starting, None)
                    } else if self.order.contains(&slot.id) {
                        (StageState::Running, None)
                    } else {
                        (StageState::Off, None)
                    }
                } else {
                    (StageState::Failed, Some("It is not available.".to_string()))
                };
                StageStatus {
                    id: slot.id.clone(),
                    state,
                    reason,
                }
            })
            .collect();
        let status = Status {
            device: self.target.device.clone(),
            stages,
        };
        if let Ok(text) = serde_json::to_string(&status) {
            let _ = write_atomic(&status_path(self.direction), &text);
        }
    }

    /// Notice a stage that exited and take it out of the chain. Returns whether
    /// anything changed.
    fn reap(&mut self) -> bool {
        let direction = self.direction;
        let mut changed = false;
        for stage in self.effects.values_mut() {
            if stage.child.is_some() && !stage.alive() {
                stage.failure = Some(failure_reason(direction, &stage.role));
                stage.child = None;
                stage.ready = false;
                if stage.started.elapsed() > Duration::from_secs(60) {
                    stage.restarts = 0;
                }
                stage.restarts += 1;
                stage.retry_at =
                    Some(Instant::now() + Duration::from_secs(1 << stage.restarts.min(4)));
                changed = true;
            }
        }
        changed
    }

    /// The soonest time a failed stage is due to start again.
    fn next_retry(&self) -> Option<Instant> {
        self.effects
            .values()
            .filter(|stage| stage.failure.is_some() && stage.restarts < MAX_RESTARTS)
            .filter_map(|stage| stage.retry_at)
            .min()
    }
}

/// The last line of a stage's log, as the reason it stopped.
fn failure_reason(direction: Direction, role: &Role) -> String {
    let id = match role {
        Role::Head => "head".to_string(),
        Role::Tail => "tail".to_string(),
        Role::Effect(id) => id.clone(),
    };
    let log = runtime_dir()
        .join(direction.name())
        .join(format!("{id}.log"));
    let text = std::fs::read_to_string(log).unwrap_or_default();
    let line = text
        .lines()
        .rev()
        .find(|line| line.starts_with("[E]") || line.starts_with("[W]"))
        .unwrap_or("");
    if line.is_empty() {
        "The plugin stopped.".to_string()
    } else {
        format!(
            "The plugin stopped: {}",
            line.rsplit("] ").next().unwrap_or(line)
        )
    }
}

/// Run the engine of `direction` until it is told to stop. This is what
/// `bingux-effects run` does.
pub fn run(store: &Store, direction: Direction) -> Result<(), String> {
    store.ensure_factory().map_err(|e| e.0)?;
    let catalog = Catalog::load();
    let state = store.state();
    let device = state
        .of(direction)
        .device
        .clone()
        .or_else(|| default_device(direction));
    // Remember the device the effects started on, so the Sound page can show it.
    if state.of(direction).device.is_none()
        && let Some(found) = device.clone()
    {
        let _ = store.update_state(|state| state.of_mut(direction).device = Some(found));
    }
    let directory = runtime_dir();
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    std::fs::write(pid_path(direction), std::process::id().to_string())
        .map_err(|e| e.to_string())?;

    let mut pipe = [0i32; 2];
    // SAFETY: plain libc calls; the descriptors are used only here and in the handler.
    unsafe {
        if libc::pipe2(pipe.as_mut_ptr(), libc::O_CLOEXEC | libc::O_NONBLOCK) != 0 {
            return Err("Could not create a pipe.".to_string());
        }
        WAKE.store(pipe[1], Ordering::Relaxed);
        for signal in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
            libc::signal(signal, on_signal as *const () as libc::sighandler_t);
        }
    }

    let mut engine = Engine::new(direction, catalog, device.clone()).map_err(|e| e.to_string())?;
    engine.start_end(&Role::Head);
    engine.start_end(&Role::Tail);
    let preset = active_preset(store, direction, device.as_deref());
    let mut current = preset.clone();
    engine.reconcile(&preset);

    // Make the head the default device now that the chain can carry sound.
    if !engine.head.ready {
        let reason = engine
            .head
            .failure
            .clone()
            .unwrap_or_else(|| "The effects device did not start.".to_string());
        engine.shutdown(device.as_deref());
        return Err(reason);
    }
    if let Some(id) = find_node(graph::node_name(direction)) {
        let _ = set_default(id);
        engine.default_set = true;
    }

    let mut asked = false;
    loop {
        let timeout = engine.next_retry().map_or(500, |at| {
            at.saturating_duration_since(Instant::now())
                .as_millis()
                .clamp(10, 500) as i32
        });
        let mut fds = [libc::pollfd {
            fd: pipe[0],
            events: libc::POLLIN,
            revents: 0,
        }];
        // SAFETY: `fds` holds one valid entry.
        unsafe { libc::poll(fds.as_mut_ptr(), 1, timeout) };
        let mut reload = false;
        if fds[0].revents != 0 {
            let mut bytes = [0u8; 64];
            loop {
                // SAFETY: reading into a local buffer from a non-blocking pipe.
                let read = unsafe { libc::read(pipe[0], bytes.as_mut_ptr().cast(), bytes.len()) };
                if read <= 0 {
                    break;
                }
                for byte in &bytes[..read as usize] {
                    if *byte == b'h' {
                        reload = true;
                    } else {
                        asked = true;
                    }
                }
            }
        }
        if asked {
            break;
        }
        if !engine.head.alive() || !engine.tail.alive() {
            engine.shutdown(device.as_deref());
            return Err("The effects engine ended unexpectedly.".to_string());
        }
        let exited = engine.reap();
        let due = engine.next_retry().is_some_and(|at| at <= Instant::now());
        if reload {
            current = active_preset(store, direction, device.as_deref());
        }
        if reload || exited || due {
            engine.reconcile(&current);
        }
    }
    engine.shutdown(device.as_deref());
    Ok(())
}

impl Engine {
    /// Stop every stage and give the real device back as the default.
    fn shutdown(&mut self, device: Option<&str>) {
        for stage in self.effects.values_mut() {
            stage.kill();
        }
        self.head.kill();
        self.tail.kill();
        if self.default_set
            && let Some(device) = device
            && let Some(id) = find_node(device)
        {
            let _ = set_default(id);
        }
        let _ = std::fs::remove_file(pid_path(self.direction));
        let _ = std::fs::remove_file(status_path(self.direction));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dump() -> Vec<Value> {
        serde_json::from_str(
            r#"[
              {"id":10,"type":"PipeWire:Interface:Node","info":{"props":{"node.name":"bingux_fx.output.a","media.class":"Stream/Input/Audio"}}},
              {"id":11,"type":"PipeWire:Interface:Node","info":{"props":{"node.name":"alsa_output.usb","media.class":"Audio/Sink"}}},
              {"id":20,"type":"PipeWire:Interface:Port","info":{"direction":"input","props":{"node.id":10,"port.name":"input_FL"}}},
              {"id":21,"type":"PipeWire:Interface:Port","info":{"direction":"output","props":{"node.id":10,"port.name":"monitor_FL"}}},
              {"id":22,"type":"PipeWire:Interface:Port","info":{"direction":"input","props":{"node.id":11,"port.name":"playback_FL"}}},
              {"id":23,"type":"PipeWire:Interface:Port","info":{"direction":"output","props":{"node.id":11,"port.name":"monitor_FL"}}},
              {"id":30,"type":"PipeWire:Interface:Link","info":{"output-port-id":21,"input-port-id":22}},
              {"id":31,"type":"PipeWire:Interface:Link","info":{"output-port-id":23,"input-port-id":20}}
            ]"#,
        )
        .unwrap()
    }

    #[test]
    fn a_snapshot_names_ports_like_pw_link_and_finds_only_the_engines_own_links() {
        let snapshot = Snapshot::from_objects(&dump());
        assert_eq!(
            snapshot.port("bingux_fx.output.a:input_FL").map(|port| (
                port.id,
                port.node,
                port.output
            )),
            Some((20, 10, false))
        );
        assert!(snapshot.has_ports(&["alsa_output.usb:playback_FL".to_string()]));
        assert!(!snapshot.has_ports(&["nothing:in".to_string()]));
        assert!(
            snapshot.own_links().is_empty(),
            "a link from or to a device is not the chain's"
        );
    }

    #[test]
    fn own_ports_are_recognised_by_node_name() {
        assert!(own_port("bingux_effects_output.out:output_FL"));
        assert!(own_port("bingux_fx.input.tail:input_FR"));
        assert!(!own_port("alsa_output.usb:playback_FL"));
    }

    #[test]
    fn params_become_one_props_value() {
        assert_eq!(
            props_json(&[("fx:g_out".to_string(), 0.5), ("fx:g_in".to_string(), 2.0)]),
            "{ params = [ \"fx:g_out\" 0.5 \"fx:g_in\" 2.0 ] }"
        );
    }
}
