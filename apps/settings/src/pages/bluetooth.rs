//! Bluetooth: the adapter's power and visibility, the devices it knows, and a
//! scan that runs while this page is open. Everything goes through
//! `bluetoothctl` on background threads; the scan is one child process per
//! round, and it is killed when the scan stops or the page closes.

use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use freya::prelude::*;

use crate::icons::{self, Icon};
use crate::pages::loading::revealed;
use crate::routes;
use crate::system::{self, CommandError, background, background_then};
use crate::ui::{
    CANCEL_ID, Group, Overlays, PageHead, ResponseKind, Row, SpinnerSize, Status, Tokens,
    action_strip, caption, centered_caption, dialog, entry_row, heading, page_column, row,
    skeleton_shape, spinner, status, status_page, strip_action, switch_row, use_overlays,
    use_page_busy, use_tokens,
};
use bingux_ui::layout::{hstack, vstack};
use bingux_ui::prelude::{Router, i18n_id, tr, use_router};
use bingux_ui::shell::{MenuItem, menu_item};
use bingux_ui::style::*;

const TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(45);
const PAIR_TIMEOUT: Duration = Duration::from_secs(30);
/// Length of one scan round; a new round starts when it ends.
const SCAN_SECONDS: &str = "25";
/// Refresh while scanning, so new devices appear as they are found.
const POLL_SCANNING: Duration = Duration::from_millis(2500);
const POLL_IDLE: Duration = Duration::from_secs(8);

#[derive(Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
enum Block {
    Software,
    Hardware,
}

#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
struct Adapter {
    powered: bool,
    discoverable: bool,
    /// The name other devices see; the computer's name when BlueZ has none.
    alias: String,
}

#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
struct Device {
    mac: String,
    name: String,
    paired: bool,
    connected: bool,
    trusted: bool,
    battery: Option<u8>,
    /// The freedesktop icon name BlueZ reports, such as `audio-headset`.
    icon: String,
}

#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
struct Model {
    /// `None` when BlueZ has no controller.
    adapter: Option<Adapter>,
    /// Set when a switch on the computer or a system setting blocks Bluetooth.
    blocked: Option<Block>,
    devices: Vec<Device>,
}

type Loaded = Option<Result<Model, CommandError>>;

/// The action in progress. An empty `mac` is an adapter-wide action.
#[derive(Clone, PartialEq)]
struct Busy {
    mac: String,
    label: String,
}

/// Everything the event handlers need. All of it is `Copy`.
#[derive(Clone, Copy)]
struct Ctx {
    model: State<Loaded>,
    busy: State<Option<Busy>>,
    overlays: Overlays,
}

#[derive(Clone)]
struct Step {
    /// The program to run: `bluetoothctl`, or `busctl` for a property BlueZ
    /// has no `bluetoothctl` command for.
    program: &'static str,
    args: Vec<String>,
    timeout: Duration,
}

fn step(args: &[&str], timeout: Duration) -> Step {
    Step {
        program: "bluetoothctl",
        args: args.iter().map(|part| part.to_string()).collect(),
        timeout,
    }
}

/// A user action: one or more commands, run in order.
#[derive(Clone)]
struct Job {
    mac: String,
    busy: String,
    steps: Vec<Step>,
    /// A toast on success. Empty when the visible change is enough.
    done: String,
    /// The start of the toast on failure; the reason follows it.
    failed: String,
}

/// Run a `bluetoothctl` command.
fn bluetoothctl(args: &[&str], timeout: Duration) -> Result<String, CommandError> {
    run_tool("bluetoothctl", args, timeout)
}

/// Run a Bluetooth-related program and map its errors to user text.
fn run_tool(program: &str, args: &[&str], timeout: Duration) -> Result<String, CommandError> {
    system::run(program, args, timeout).map_err(|error| {
        // Without bluetoothd, bluetoothctl aborts on a D-Bus assertion and
        // prints a build-flag complaint that means nothing to a user.
        if error.0.contains("D-Bus") || error.0.contains("rdynamic") {
            CommandError(tr!("bluetooth-error-unavailable"))
        } else {
            error
        }
    })
}

/// The value of `key: value` in a `bluetoothctl` report, trimmed.
fn field<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().find_map(|line| {
        let (name, value) = line.trim().split_once(':')?;
        (name.trim() == key).then(|| value.trim())
    })
}

fn flag(text: &str, key: &str) -> Result<bool, CommandError> {
    field(text, key)
        .map(|value| value == "yes")
        .ok_or_else(|| CommandError(tr!("bluetooth-error-status-read")))
}

/// `Battery Percentage` reads `0x5a (90)`; the percentage is in brackets.
fn battery(value: &str) -> Option<u8> {
    value
        .split('(')
        .nth(1)?
        .trim_end_matches(')')
        .trim()
        .parse()
        .ok()
}

/// Whether the kernel has a Bluetooth controller. Without one, and without
/// bluetoothd, `bluetoothctl` waits for its whole timeout, so check first.
fn has_controller() -> bool {
    crate::system::read_dir("/sys/class/bluetooth")
        .is_ok_and(|mut entries| entries.next().is_some())
}

/// Bluetooth blocked by a switch or by a system setting, from the radio
/// kill switches the kernel reports.
fn radio_block() -> Option<Block> {
    for entry in crate::system::read_dir("/sys/class/rfkill").ok()?.flatten() {
        let path = entry.path();
        let kind = crate::system::read_to_string(path.join("type")).unwrap_or_default();
        if kind.trim() != "bluetooth" {
            continue;
        }
        let set = |name: &str| {
            crate::system::read_to_string(path.join(name)).is_ok_and(|v| v.trim() == "1")
        };
        if set("hard") {
            return Some(Block::Hardware);
        }
        if set("soft") {
            return Some(Block::Software);
        }
    }
    None
}

fn hostname() -> String {
    crate::system::read_to_string("/etc/hostname")
        .map(|name| name.trim().to_string())
        .ok()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| tr!("bluetooth-this-computer"))
}

/// Snapshot builds only: `BINGUX_SETTINGS_FAKE_BLUETOOTH=1` stands in a
/// computer with Bluetooth on and two paired devices, so the page can be
/// checked, and its layout held still, without an adapter.
#[cfg(feature = "snapshot")]
fn fake_model() -> Option<Model> {
    std::env::var_os("BINGUX_SETTINGS_FAKE_BLUETOOTH")?;
    let device = |mac: &str, name: &str, icon: &str, connected: bool| Device {
        mac: mac.to_string(),
        name: bingux_ui::stress::data(name),
        paired: true,
        connected,
        trusted: true,
        battery: None,
        icon: icon.to_string(),
    };
    Some(Model {
        adapter: Some(Adapter {
            powered: true,
            discoverable: false,
            alias: "This computer".to_string(),
        }),
        blocked: None,
        devices: vec![
            device("00:11:22:33:44:55", "Headphones", "audio-headset", true),
            device("00:11:22:33:44:66", "Keyboard", "input-keyboard", false),
        ],
    })
}

#[cfg(not(feature = "snapshot"))]
fn fake_model() -> Option<Model> {
    None
}

fn load() -> Result<Model, CommandError> {
    if let Some(model) = fake_model() {
        return Ok(model);
    }
    let empty = Model {
        adapter: None,
        blocked: None,
        devices: Vec::new(),
    };
    if !has_controller() || bluetoothctl(&["list"], TIMEOUT)?.trim().is_empty() {
        return Ok(empty);
    }
    let show = bluetoothctl(&["show"], TIMEOUT)?;
    let adapter = Adapter {
        powered: flag(&show, "Powered")?,
        discoverable: flag(&show, "Discoverable")?,
        alias: field(&show, "Alias")
            .filter(|alias| !alias.is_empty())
            .map_or_else(hostname, str::to_string),
    };

    let listing = bluetoothctl(&["devices"], TIMEOUT)?;
    let mut devices = Vec::new();
    for line in listing.lines() {
        let Some(rest) = line.trim().strip_prefix("Device ") else {
            continue;
        };
        let (mac, known_name) = rest.split_once(' ').unwrap_or((rest, ""));
        let info = bluetoothctl(&["info", mac], TIMEOUT)?;
        devices.push(Device {
            mac: mac.to_string(),
            name: field(&info, "Alias").unwrap_or(known_name).to_string(),
            paired: flag(&info, "Paired")?,
            connected: flag(&info, "Connected")?,
            trusted: flag(&info, "Trusted").unwrap_or(false),
            icon: field(&info, "Icon").unwrap_or_default().to_string(),
            battery: field(&info, "Battery Percentage").and_then(battery),
        });
    }
    devices.sort_by_key(|device| device.name.to_lowercase());

    Ok(Model {
        adapter: Some(adapter),
        blocked: radio_block(),
        devices,
    })
}

/// Set while a read is running, so polls do not pile up threads.
static READING: AtomicBool = AtomicBool::new(false);

/// Read Bluetooth in the background, unless a read is already running.
fn refresh(model: State<Loaded>) {
    if READING.swap(true, Ordering::SeqCst) {
        return;
    }
    background(model, || {
        let result = system::load_persistent("bluetooth", load);
        READING.store(false, Ordering::SeqCst);
        result
    });
}

/// Turn a `bluetoothctl` answer into success, or a reason a person can act
/// on. An answer that says the device is already connected or paired is a
/// success.
fn verdict(text: &str, failed: bool) -> Result<(), String> {
    let lower = text.to_lowercase();
    if lower.contains("already") {
        Ok(())
    } else if failed || lower.contains("failed") || lower.contains("error") {
        Err(plain_reason(text))
    } else {
        Ok(())
    }
}

/// Known BlueZ failures, in words for a person. The first match wins.
const REASONS: &[(&[&str], &str)] = &[
    (&["did not answer"], i18n_id!("bluetooth-error-no-answer")),
    (
        &["d-bus", "rdynamic"],
        i18n_id!("bluetooth-error-unavailable"),
    ),
    (
        &["inprogress", "in progress"],
        i18n_id!("bluetooth-error-busy"),
    ),
    (
        &["authentic", "rejected"],
        i18n_id!("bluetooth-error-pairing-rejected"),
    ),
    (
        &["notavailable", "not available", "doesnotexist", "not found"],
        i18n_id!("bluetooth-error-out-of-range"),
    ),
    (
        &["timeout", "timed out"],
        i18n_id!("bluetooth-error-timeout"),
    ),
    (
        &[
            "connectionattempt",
            "page-timeout",
            "host is down",
            "failed to connect",
        ],
        i18n_id!("bluetooth-error-no-response"),
    ),
];

fn plain_reason(text: &str) -> String {
    let lower = text.to_lowercase();
    for (keys, reason) in REASONS {
        if keys.iter().any(|key| lower.contains(key)) {
            return bingux_ui::i18n::text(reason);
        }
    }
    if lower.contains("org.") || lower.contains("bluez") || lower.contains("bluetoothd") {
        return tr!("bluetooth-error-generic");
    }
    tr!("bluetooth-error-try-later")
}

fn run_steps(steps: &[Step]) -> Result<(), String> {
    for step in steps {
        let args: Vec<&str> = step.args.iter().map(String::as_str).collect();
        match run_tool(step.program, &args, step.timeout) {
            Ok(text) => verdict(&text, false)?,
            Err(error) => verdict(&error.0, true)?,
        }
    }
    Ok(())
}

/// Run a job in the background, toast the outcome, and read Bluetooth again.
/// One job runs at a time; a second request says so instead of queuing.
fn run_job(ctx: Ctx, job: Job) {
    if ctx.busy.read().is_some() {
        ctx.overlays.toast(tr!("bluetooth-toast-busy"));
        return;
    }
    let Ctx {
        mut busy,
        model,
        overlays,
    } = ctx;
    busy.set(Some(Busy {
        mac: job.mac.clone(),
        label: job.busy.clone(),
    }));
    let retry = job.clone();
    let done = job.done.clone();
    let failed = job.failed.clone();
    background_then(
        move || run_steps(&job.steps),
        move |result| {
            busy.set(None);
            match result {
                Ok(()) => {
                    if !done.is_empty() {
                        overlays.toast(done);
                    }
                }
                Err(reason) => overlays.toast_with_action(
                    tr!(
                        "bluetooth-toast-failed",
                        failed = failed.as_str(),
                        reason = reason.as_str()
                    ),
                    tr!("bluetooth-action-retry"),
                    move |_| run_job(ctx, retry.clone()),
                ),
            }
            refresh(model);
        },
    );
}

/// Flip a switch at once and run its bluetoothctl job in the background. The
/// switch keeps the new value until the job has run and Bluetooth has been
/// read again. A failure puts it back and toasts the reason.
fn set_switch_job(ctx: Ctx, pending: State<Option<bool>>, job: Job, on: bool) {
    let mut pending = pending;
    let mut model = ctx.model;
    let Job { steps, failed, .. } = job;
    bingux_ui::runtime::optimistic_then(
        pending,
        Some(ctx.overlays),
        None,
        Some(on),
        move || {
            run_steps(&steps).map_err(|reason| {
                tr!(
                    "bluetooth-toast-failed",
                    failed = failed.as_str(),
                    reason = reason.as_str()
                )
            })
        },
        move || {
            background_then(
                || system::load_persistent("bluetooth", load),
                move |loaded| {
                    model.set(Some(loaded));
                    pending.set(None);
                },
            );
        },
    );
}

fn power_job(on: bool) -> Job {
    let (word, busy, failed) = if on {
        (
            "on",
            tr!("bluetooth-busy-power-on"),
            tr!("bluetooth-failed-power-on"),
        )
    } else {
        (
            "off",
            tr!("bluetooth-busy-power-off"),
            tr!("bluetooth-failed-power-off"),
        )
    };
    Job {
        mac: String::new(),
        busy,
        steps: vec![step(&["power", word], TIMEOUT)],
        done: String::new(),
        failed,
    }
}

/// Set the device's Alias over D-Bus; `bluetoothctl` has no rename command.
/// An empty alias makes BlueZ show the device's own name again.
fn rename_job(device: &Device, alias: &str) -> Job {
    let path = format!("/org/bluez/hci0/dev_{}", device.mac.replace(':', "_"));
    let done = if alias.is_empty() {
        tr!("bluetooth-toast-alias-reset", name = device.name.as_str())
    } else {
        tr!("bluetooth-toast-renamed", alias = alias)
    };
    Job {
        mac: device.mac.clone(),
        busy: tr!("bluetooth-busy-rename"),
        steps: vec![Step {
            program: "busctl",
            args: [
                "--system",
                "set-property",
                "org.bluez",
                path.as_str(),
                "org.bluez.Device1",
                "Alias",
                "s",
                alias,
            ]
            .iter()
            .map(|part| part.to_string())
            .collect(),
            timeout: TIMEOUT,
        }],
        done,
        failed: tr!("bluetooth-failed-rename", name = device.name.as_str()),
    }
}

fn trust_job(device: &Device, on: bool) -> Job {
    let word = if on { "trust" } else { "untrust" };
    Job {
        mac: device.mac.clone(),
        busy: tr!("bluetooth-busy-save"),
        steps: vec![step(&[word, device.mac.as_str()], TIMEOUT)],
        done: String::new(),
        failed: tr!("bluetooth-failed-trust", name = device.name.as_str()),
    }
}

fn remove_job(device: &Device) -> Job {
    Job {
        mac: device.mac.clone(),
        busy: tr!("bluetooth-busy-forget"),
        steps: vec![step(&["remove", device.mac.as_str()], TIMEOUT)],
        done: tr!("bluetooth-toast-forgot", name = device.name.as_str()),
        failed: tr!("bluetooth-failed-forget", name = device.name.as_str()),
    }
}

/// The action a device allows: connect or disconnect when set up, and pair
/// otherwise (then connect). Pairing that needs a passkey fails here, because
/// bluetoothctl has no agent to answer it.
fn device_job(device: &Device) -> Job {
    let mac = device.mac.as_str();
    let name = &device.name;
    if device.connected {
        Job {
            mac: device.mac.clone(),
            busy: tr!("bluetooth-busy-disconnect"),
            steps: vec![step(&["disconnect", mac], TIMEOUT)],
            done: tr!("bluetooth-toast-disconnected", name = name),
            failed: tr!("bluetooth-failed-disconnect", name = name),
        }
    } else if device.paired {
        Job {
            mac: device.mac.clone(),
            busy: tr!("bluetooth-busy-connect"),
            steps: vec![step(&["connect", mac], CONNECT_TIMEOUT)],
            done: tr!("bluetooth-toast-connected", name = name),
            failed: tr!("bluetooth-failed-connect", name = name),
        }
    } else {
        Job {
            mac: device.mac.clone(),
            busy: tr!("bluetooth-busy-pair"),
            steps: vec![
                step(&["--timeout", "30", "pair", mac], PAIR_TIMEOUT),
                step(&["connect", mac], CONNECT_TIMEOUT),
            ],
            done: tr!("bluetooth-toast-connected", name = name),
            failed: tr!("bluetooth-failed-pair", name = name),
        }
    }
}

/// Keeps `bluetoothctl scan on` running in rounds while started. Each round
/// is one child process. `stop` kills the child; a loop from an earlier start
/// sees its generation has changed and exits, so no process outlives its page.
#[derive(Clone)]
struct Scanner(Arc<Mutex<ScanSlot>>);

#[derive(Default)]
struct ScanSlot {
    /// Bumped on every start and stop.
    generation: u64,
    running: bool,
    child: Option<Child>,
}

impl Scanner {
    fn new() -> Self {
        Self(Arc::default())
    }

    fn lock(&self) -> MutexGuard<'_, ScanSlot> {
        self.0.lock().unwrap_or_else(|poison| poison.into_inner())
    }

    fn running(&self) -> bool {
        self.lock().running
    }

    fn start(&self) {
        let generation = {
            let mut slot = self.lock();
            if slot.running {
                return;
            }
            slot.running = true;
            slot.generation += 1;
            slot.generation
        };
        let scanner = self.clone();
        std::thread::spawn(move || scanner.run_rounds(generation));
        // A computer other devices can pair with has to be visible. It is
        // visible exactly while this page scans: BlueZ's own three minute
        // limit would end it while the page is still open, so it is lifted
        // here and visibility is switched off again in `stop`.
        std::thread::spawn(|| {
            let _ = bluetoothctl(&["discoverable-timeout", "0"], TIMEOUT);
            let _ = bluetoothctl(&["discoverable", "on"], TIMEOUT);
        });
    }

    fn stop(&self) {
        let was_running = self.lock().running;
        if was_running {
            std::thread::spawn(|| {
                let _ = bluetoothctl(&["discoverable", "off"], TIMEOUT);
            });
        }
        let child = {
            let mut slot = self.lock();
            slot.running = false;
            slot.generation += 1;
            slot.child.take()
        };
        // Reaping waits for the child to exit, so it runs off the UI thread.
        if let Some(mut child) = child {
            std::thread::spawn(move || {
                let _ = child.kill();
                let _ = child.wait();
            });
        }
    }

    fn run_rounds(&self, generation: u64) {
        loop {
            {
                let mut slot = self.lock();
                if slot.generation != generation {
                    return;
                }
                let mut command = Command::new("bluetoothctl");
                command
                    .args(["--timeout", SCAN_SECONDS, "scan", "on"])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                bingux_ui::runtime::die_with_parent(&mut command);
                let spawned = command.spawn();
                match spawned {
                    Ok(child) => slot.child = Some(child),
                    Err(_) => {
                        slot.running = false;
                        return;
                    }
                }
            }
            // Wait for this round to end. `stop` kills and takes the child.
            loop {
                std::thread::sleep(Duration::from_millis(200));
                let mut slot = self.lock();
                if slot.generation != generation {
                    return;
                }
                let ended = match slot.child.as_mut() {
                    Some(child) => !matches!(child.try_wait(), Ok(None)),
                    None => true,
                };
                if ended {
                    slot.child = None;
                    break;
                }
            }
        }
    }
}

fn powered(model: &Loaded) -> bool {
    matches!(
        model,
        Some(Ok(Model {
            adapter: Some(Adapter { powered: true, .. }),
            ..
        }))
    )
}

/// The header line: visibility while on, or that Bluetooth is off.
fn header_subtitle(model: &Loaded) -> Option<String> {
    match model {
        Some(Ok(Model {
            adapter:
                Some(Adapter {
                    powered: true,
                    discoverable: true,
                    alias,
                    ..
                }),
            ..
        })) => Some(tr!("bluetooth-header-visible", alias = alias)),
        Some(Ok(Model {
            adapter: Some(Adapter { powered: true, .. }),
            ..
        })) => None,
        Some(Ok(Model {
            adapter: Some(_), ..
        })) => Some(tr!("bluetooth-header-off")),
        _ => None,
    }
}

/// The bundled glyph for the icon name BlueZ reports.
fn device_icon(name: &str) -> Icon {
    match name {
        "audio-headphones" | "audio-headset" => Icon::Headphones,
        "audio-card" | "audio-speakers" => Icon::Speakers,
        "input-keyboard" => Icon::Keyboard,
        "input-mouse" => Icon::Mouse,
        "input-tablet" => Icon::InputTablet,
        "input-gaming" => Icon::Gamepad,
        "phone" => Icon::Phone,
        "computer" => Icon::Computer,
        "video-display" => Icon::Display,
        "printer" => Icon::Printer,
        "camera-photo" | "camera-video" | "camera-web" => Icon::CameraWeb,
        _ => Icon::Bluetooth,
    }
}

/// What kind of device the icon name stands for, in words.
fn device_kind(name: &str) -> String {
    match name {
        "audio-headphones" | "audio-headset" => tr!("bluetooth-kind-headphones"),
        "audio-card" | "audio-speakers" => tr!("bluetooth-kind-speakers"),
        "input-keyboard" => tr!("bluetooth-kind-keyboard"),
        "input-mouse" => tr!("bluetooth-kind-mouse"),
        "input-tablet" => tr!("bluetooth-kind-tablet"),
        "input-gaming" => tr!("bluetooth-kind-game-controller"),
        "phone" => tr!("bluetooth-kind-phone"),
        "computer" => tr!("bluetooth-kind-computer"),
        "video-display" => tr!("bluetooth-kind-display"),
        "printer" => tr!("bluetooth-kind-printer"),
        "camera-photo" | "camera-video" | "camera-web" => tr!("bluetooth-kind-camera"),
        _ => tr!("bluetooth-kind-other"),
    }
}

/// The state of a device in words.
fn device_word(device: &Device) -> String {
    if device.connected {
        tr!("bluetooth-state-connected")
    } else if device.paired {
        tr!("bluetooth-state-not-connected")
    } else {
        tr!("bluetooth-state-not-paired")
    }
}

/// The state of a device in words, with its battery level when known.
fn device_state(device: &Device) -> String {
    let state = device_word(device);
    match device.battery {
        Some(level) => tr!(
            "bluetooth-state-battery",
            state = state.as_str(),
            level = level
        ),
        None => state,
    }
}

/// The actions for one device, as a strip: connect, disconnect or pair, and
/// forget once the device is paired.
fn device_strip(device: &Device, ctx: Ctx, adapter_on: bool, idle: bool) -> Element {
    let job = device_job(device);
    let (icon, label) = device_action(device);
    // Connecting needs the adapter and no job running; Forget stays reachable
    // whenever the device is paired, and waits only for a running job.
    let mut actions = vec![
        strip_action(icon, label, move |_| run_job(ctx, job.clone())).enabled(adapter_on && idle),
    ];
    if device.paired {
        let forget = device.clone();
        actions.push(
            strip_action(Icon::UserTrash, tr!("bluetooth-action-forget"), move |_| {
                confirm_forget(ctx, &forget)
            })
            .destructive()
            .enabled(idle),
        );
    }
    action_strip(actions).into()
}

/// The one action a device allows, with its icon and label.
fn device_action(device: &Device) -> (Icon, String) {
    if device.connected {
        (Icon::WindowClose, tr!("bluetooth-action-disconnect"))
    } else if device.paired {
        (Icon::Bluetooth, tr!("bluetooth-action-connect"))
    } else {
        (Icon::ListAdd, tr!("bluetooth-action-pair"))
    }
}

/// The right-click menu for one device: its one action, and Forget once paired.
fn device_menu(ctx: Ctx, device: &Device) -> Vec<MenuItem> {
    let (icon, label) = device_action(device);
    let job = device_job(device);
    let mut items = vec![menu_item(label, move |_| run_job(ctx, job.clone())).icon(icon)];
    if device.paired {
        let forget = device.clone();
        items.push(
            menu_item(tr!("bluetooth-action-forget"), move |_| {
                confirm_forget(ctx, &forget)
            })
            .icon(Icon::UserTrash)
            .destructive(),
        );
    }
    items
}

/// The name to show for a device. BlueZ uses the address when a device has
/// no name of its own.
fn device_name(device: &Device) -> String {
    let as_address = device.name.replace('-', ":");
    if device.name.is_empty() || as_address.eq_ignore_ascii_case(&device.mac) {
        tr!("bluetooth-device-unnamed")
    } else {
        device.name.clone()
    }
}

/// One device in a list: its icon, name and state. The row opens the
/// device's page, where the connect or pair action lives.
fn device_row(
    ctx: Ctx,
    device: &Device,
    busy: Option<&Busy>,
    tokens: Tokens,
    router: Router,
) -> Row {
    let route = routes::device(&device.mac.clone(), &device_name(device));
    let busy_label = busy
        .filter(|busy| busy.mac == device.mac)
        .map(|busy| busy.label.clone());
    let mut glyphs = hstack(S2);
    if device.connected && busy_label.is_none() {
        glyphs = glyphs.child(icons::icon(
            Icon::Check,
            IconSize::Control,
            Paint::Accent.color(&tokens),
        ));
    }
    if let Some(level) = device.battery {
        glyphs = glyphs
            .child(icons::icon(
                Icon::battery_level(level),
                IconSize::Control,
                Paint::Muted.color(&tokens),
            ))
            .child(caption(tr!("bluetooth-battery-percent", level = level)));
    }
    let status = busy_label.or_else(|| (!device.connected).then(|| device_word(device)));
    if let Some(status) = status {
        glyphs = glyphs.child(caption(status));
    }
    row(device_name(device))
        .leading(device_icon(&device.icon))
        .trailing(Element::from(glyphs))
        .navigates()
        .on_press(move |_| router.open(route.clone()))
        .context_menu(device_menu(ctx, device))
}

/// Read Bluetooth now, then again while the page is open: every 2.5 seconds
/// while scanning and every 8 seconds otherwise.
fn use_polling(model: State<Loaded>, scanner: Option<Scanner>) {
    use_hook(move || {
        refresh(model);
        let focus = system::window_focus();
        spawn(async move {
            loop {
                let wait = match &scanner {
                    Some(scanner) if scanner.running() => POLL_SCANNING,
                    _ => POLL_IDLE,
                };
                timer(wait).await;
                if *focus.peek() {
                    refresh(model);
                }
            }
        });
    });
}

#[derive(PartialEq)]
pub struct BluetoothPage;

impl Component for BluetoothPage {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let overlays = use_overlays();
        let router = use_router();
        let model = system::use_cached_persistent("bluetooth", load);
        let busy_state = use_state::<Option<Busy>>(|| None);
        let power_pending = use_state::<Option<bool>>(|| None);
        let scanner = use_hook(Scanner::new);
        use_polling(model, Some(scanner.clone()));

        // Scan while Bluetooth is on; stop when it is off, and when the page closes.
        {
            let scanner = scanner.clone();
            use_side_effect(move || {
                if powered(&model.read()) {
                    scanner.start();
                } else {
                    scanner.stop();
                }
            });
        }
        {
            let scanner = scanner.clone();
            use_drop(move || scanner.stop());
        }
        use_page_busy(
            busy_state
                .read()
                .as_ref()
                .is_some_and(|busy| busy.mac.is_empty()),
        );
        // The master switch sits at the right of the page title, as in GNOME.
        let master = match &*model.read() {
            Some(Ok(data)) if data.blocked.is_none() => data.adapter.as_ref().map(|a| a.powered),
            _ => None,
        };
        let mut head = PageHead {
            subtitle: header_subtitle(&model.read()),
            ..PageHead::new()
        };
        if let Some(on) = master {
            let ctx = Ctx {
                model,
                busy: busy_state,
                overlays,
            };
            let shown = (*power_pending.read()).unwrap_or(on);
            head = head.switch(tr!("bluetooth-master-switch"), shown, move |next: bool| {
                set_switch_job(ctx, power_pending, power_job(next), next)
            });
        }

        // How many devices are paired is not known before the read lands, so
        // the lists are not drawn from a guess: the page shows its head, then
        // fades the lists in at their final places.
        let Some(loaded) = model.read().clone() else {
            return revealed(page_column().child(head.clone()), true);
        };
        let data = match loaded {
            Ok(data) => data,
            Err(e) => {
                return page_column().child(head.clone()).child(
                    status(Status::Error(e.0))
                        .action(&tr!("bluetooth-action-retry"), move |_| refresh(model)),
                );
            }
        };
        let Some(adapter) = data.adapter.clone() else {
            return page_column().child(head.clone()).child(
                status_page(Icon::Bluetooth, tr!("bluetooth-empty-no-adapter"))
                    .description(tr!("bluetooth-empty-no-adapter-body")),
            );
        };
        if let Some(block) = data.blocked {
            let reason = match block {
                Block::Hardware => tr!("bluetooth-blocked-hardware-body"),
                Block::Software => tr!("bluetooth-blocked-software-body"),
            };
            return page_column().child(head.clone()).child(
                status_page(Icon::Bluetooth, tr!("bluetooth-blocked-title")).description(reason),
            );
        }

        let busy = busy_state.read().clone();
        let ctx = Ctx {
            model,
            busy: busy_state,
            overlays,
        };

        let mut page = page_column().child(head);
        if !adapter.powered {
            return page.child(
                status_page(Icon::Bluetooth, tr!("bluetooth-off-title"))
                    .description(tr!("bluetooth-off-body"))
                    .action(&tr!("bluetooth-off-action"), move |_| {
                        run_job(ctx, power_job(true))
                    }),
            );
        }

        let scanning = scanner.running();
        let (mut mine, nearby): (Vec<&Device>, Vec<&Device>) =
            data.devices.iter().partition(|device| device.paired);
        mine.sort_by_key(|device| (!device.connected, device.name.to_lowercase()));
        let no_mine = mine.is_empty();

        if !no_mine {
            page = page.child(Group::titled(&tr!("bluetooth-group-mine")).rows(
                mine.into_iter().map(|device| {
                    Element::from(device_row(ctx, device, busy.as_ref(), tokens, router))
                }),
            ));
        }
        // Nearby devices carries the scanning spinner. It stays on screen
        // while a scan runs, so the page does not jump when the first device
        // appears.
        if !nearby.is_empty() || scanning {
            let mut group = Group::titled(&tr!("bluetooth-group-nearby"));
            if scanning {
                group = group.trailing(spinner(SpinnerSize::Small));
            }
            page = page.child(if nearby.is_empty() {
                group.row(status(Status::Empty(tr!("bluetooth-scanning"))))
            } else {
                group.rows(nearby.into_iter().map(|device| {
                    Element::from(device_row(ctx, device, busy.as_ref(), tokens, router))
                }))
            });
        } else if no_mine {
            page = page.child(
                status_page(Icon::Bluetooth, tr!("bluetooth-empty-title"))
                    .description(tr!("bluetooth-empty-body")),
            );
        }
        revealed(page, false)
    }
}

/// One device: its state and its one action, its battery, what BlueZ knows
/// about it, whether it connects by itself, and Forget.
#[derive(PartialEq)]
pub struct BluetoothDevicePage {
    pub mac: String,
}

impl Component for BluetoothDevicePage {
    fn render(&self) -> impl IntoElement {
        let overlays = use_overlays();
        let router = use_router();
        let tokens = use_tokens();
        let model = system::use_cached_persistent("bluetooth", load);
        let busy_state = use_state::<Option<Busy>>(|| None);
        use_polling(model, None);
        use_page_busy(busy_state.read().is_some());

        // Go back to the list once BlueZ no longer knows the device.
        let mac = self.mac.clone();
        use_side_effect(move || {
            if let Some(Ok(data)) = &*model.read()
                && !data.devices.iter().any(|device| device.mac == mac)
            {
                router.pop();
            }
        });

        let Some(Ok(data)) = model.read().clone() else {
            return page_column().child(skeleton_shape(&[5]));
        };
        let Some(device) = data.devices.iter().find(|device| device.mac == self.mac) else {
            return page_column();
        };
        let adapter_on = data.adapter.as_ref().is_some_and(|adapter| adapter.powered);
        let busy = busy_state.read().clone();
        let ctx = Ctx {
            model,
            busy: busy_state,
            overlays,
        };

        let identity = vstack(S2)
            .tw_layout(&[w_full(), items(Align::Center), py(S2)])
            .child(icons::icon(
                device_icon(&device.icon),
                IconSize::Hero,
                Paint::Muted.color(&tokens),
            ))
            .child(heading(device_name(device)))
            .child(centered_caption(device_state(device)));

        let yes_no = |value: bool| {
            if value {
                tr!("bluetooth-value-yes")
            } else {
                tr!("bluetooth-value-no")
            }
        };
        let mut details = Group::titled(&tr!("bluetooth-group-details"));
        if let Some(level) = device.battery {
            details = details.row(
                row(tr!("bluetooth-detail-battery"))
                    .leading(Icon::battery_level(level))
                    .value(tr!("bluetooth-battery-percent", level = level)),
            );
        }
        details = details
            .row(row(tr!("bluetooth-detail-type")).value(device_kind(&device.icon)))
            .row(row(tr!("bluetooth-detail-address")).mono_value(device.mac.clone()))
            .row(row(tr!("bluetooth-detail-paired")).value(yes_no(device.paired)));
        let rename_device = device.clone();
        details = details.row(entry_row(
            tr!("bluetooth-detail-name"),
            device.name.clone(),
            move |name: String| run_job(ctx, rename_job(&rename_device, name.trim())),
        ));
        let trust_device = device.clone();
        details = details.row(switch_row(
            tr!("bluetooth-detail-trust"),
            device.trusted,
            move |on: bool| run_job(ctx, trust_job(&trust_device, on)),
        ));

        page_column()
            .child(identity)
            .child(bingux_ui::motion::reveal_block_if(
                S10,
                (adapter_on || device.paired)
                    .then(|| device_strip(device, ctx, adapter_on, busy.is_none())),
            ))
            .child(details)
    }
}

/// Ask before forgetting a device, then forget it.
fn confirm_forget(ctx: Ctx, device: &Device) {
    let job = remove_job(device);
    ctx.overlays.show_dialog(
        dialog(
            tr!("bluetooth-forget-title", name = device_name(device)),
            tr!("bluetooth-forget-body"),
        )
        .cancel(tr!("bluetooth-forget-cancel"))
        .default_action(CANCEL_ID)
        .response(
            tr!("bluetooth-action-forget"),
            ResponseKind::Destructive,
            move |_| run_job(ctx, job.clone()),
        ),
    );
}

/// Start loading this page's data before it is opened.
pub fn prefetch() {
    system::prefetch_persistent("bluetooth", load);
}
