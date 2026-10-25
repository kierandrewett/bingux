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
use crate::routes::{Navigator, Route, use_navigator};
use crate::system::{self, CommandError, background, background_then};
use crate::ui::{
    Group, Overlays, PageHead, ResponseKind, Row, SpinnerSize, Status, Tokens, action_strip,
    caption, centered_caption, dialog, entry_row, heading, page_column, row, skeleton_shape,
    spinner, status, status_page, strip_action, switch_row, use_overlays, use_page_busy,
    use_tokens,
};
use bingux_ui::layout::{hstack, vstack};
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
            CommandError("The Bluetooth service is not running.".into())
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
        .ok_or_else(|| CommandError("Bluetooth status could not be read.".into()))
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
    std::fs::read_dir("/sys/class/bluetooth").is_ok_and(|mut entries| entries.next().is_some())
}

/// Bluetooth blocked by a switch or by a system setting, from the radio
/// kill switches the kernel reports.
fn radio_block() -> Option<Block> {
    for entry in std::fs::read_dir("/sys/class/rfkill").ok()?.flatten() {
        let path = entry.path();
        let kind = std::fs::read_to_string(path.join("type")).unwrap_or_default();
        if kind.trim() != "bluetooth" {
            continue;
        }
        let set =
            |name: &str| std::fs::read_to_string(path.join(name)).is_ok_and(|v| v.trim() == "1");
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
    std::fs::read_to_string("/etc/hostname")
        .map(|name| name.trim().to_string())
        .ok()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "This computer".to_string())
}

fn load() -> Result<Model, CommandError> {
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
    (
        &["did not answer"],
        "Bluetooth did not answer. Try again in a moment.",
    ),
    (
        &["d-bus", "rdynamic"],
        "The Bluetooth service is not running.",
    ),
    (
        &["inprogress", "in progress"],
        "Bluetooth is busy. Try again in a moment.",
    ),
    (
        &["authentic", "rejected"],
        "The device did not accept the pairing.",
    ),
    (
        &["notavailable", "not available", "doesnotexist", "not found"],
        "The device is out of range or turned off.",
    ),
    (
        &["timeout", "timed out"],
        "The device did not respond in time.",
    ),
    (
        &[
            "connectionattempt",
            "page-timeout",
            "host is down",
            "failed to connect",
        ],
        "The device did not respond. Turn it on, bring it closer and try again.",
    ),
];

fn plain_reason(text: &str) -> String {
    let lower = text.to_lowercase();
    for (keys, reason) in REASONS {
        if keys.iter().any(|key| lower.contains(key)) {
            return reason.to_string();
        }
    }
    if lower.contains("org.") || lower.contains("bluez") || lower.contains("bluetoothd") {
        return "The action did not finish.".to_string();
    }
    text.trim().to_string()
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
        ctx.overlays.toast("Wait for the current action to finish.");
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
                    format!("{failed} {reason}"),
                    "Try again",
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
        move || run_steps(&steps).map_err(|reason| format!("{failed} {reason}")),
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
    let word = if on { "on" } else { "off" };
    Job {
        mac: String::new(),
        busy: format!("Turning Bluetooth {word}…"),
        steps: vec![step(&["power", word], TIMEOUT)],
        done: String::new(),
        failed: format!("Bluetooth could not be turned {word}."),
    }
}

fn visibility_job(on: bool) -> Job {
    let word = if on { "on" } else { "off" };
    Job {
        mac: String::new(),
        busy: "Changing visibility…".to_string(),
        steps: vec![step(&["discoverable", word], TIMEOUT)],
        done: String::new(),
        failed: "Could not change visibility.".to_string(),
    }
}

/// Set the device's Alias over D-Bus; `bluetoothctl` has no rename command.
/// An empty alias makes BlueZ show the device's own name again.
fn rename_job(device: &Device, alias: &str) -> Job {
    let path = format!("/org/bluez/hci0/dev_{}", device.mac.replace(':', "_"));
    let done = if alias.is_empty() {
        format!("{} uses its own name again.", device.name)
    } else {
        format!("Renamed to {alias}.")
    };
    Job {
        mac: device.mac.clone(),
        busy: "Renaming…".to_string(),
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
        failed: format!("Could not rename {}.", device.name),
    }
}

fn trust_job(device: &Device, on: bool) -> Job {
    let word = if on { "trust" } else { "untrust" };
    Job {
        mac: device.mac.clone(),
        busy: "Saving…".to_string(),
        steps: vec![step(&[word, device.mac.as_str()], TIMEOUT)],
        done: String::new(),
        failed: format!("Could not change automatic connection for {}.", device.name),
    }
}

fn remove_job(device: &Device) -> Job {
    Job {
        mac: device.mac.clone(),
        busy: "Forgetting…".to_string(),
        steps: vec![step(&["remove", device.mac.as_str()], TIMEOUT)],
        done: format!("{} was forgotten.", device.name),
        failed: format!("Could not forget {}.", device.name),
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
            busy: "Disconnecting…".to_string(),
            steps: vec![step(&["disconnect", mac], TIMEOUT)],
            done: format!("Disconnected from {name}."),
            failed: format!("Could not disconnect from {name}."),
        }
    } else if device.paired {
        Job {
            mac: device.mac.clone(),
            busy: "Connecting…".to_string(),
            steps: vec![step(&["connect", mac], CONNECT_TIMEOUT)],
            done: format!("Connected to {name}."),
            failed: format!("Could not connect to {name}."),
        }
    } else {
        Job {
            mac: device.mac.clone(),
            busy: "Pairing…".to_string(),
            steps: vec![
                step(&["--timeout", "30", "pair", mac], PAIR_TIMEOUT),
                step(&["connect", mac], CONNECT_TIMEOUT),
            ],
            done: format!("Connected to {name}."),
            failed: format!("Could not set up {name}."),
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
    }

    fn stop(&self) {
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
                let spawned = Command::new("bluetoothctl")
                    .args(["--timeout", SCAN_SECONDS, "scan", "on"])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn();
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
        })) => Some(format!("Visible as {alias}")),
        Some(Ok(Model {
            adapter: Some(Adapter { powered: true, .. }),
            ..
        })) => Some("Not visible to other devices".to_string()),
        Some(Ok(Model {
            adapter: Some(_), ..
        })) => Some("Off".to_string()),
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
fn device_kind(name: &str) -> &'static str {
    match name {
        "audio-headphones" | "audio-headset" => "Headphones",
        "audio-card" | "audio-speakers" => "Speakers",
        "input-keyboard" => "Keyboard",
        "input-mouse" => "Mouse",
        "input-tablet" => "Tablet",
        "input-gaming" => "Game controller",
        "phone" => "Phone",
        "computer" => "Computer",
        "video-display" => "Display",
        "printer" => "Printer",
        "camera-photo" | "camera-video" | "camera-web" => "Camera",
        _ => "Other",
    }
}

fn battery_icon(level: u8) -> Icon {
    if level > 50 {
        Icon::BatteryLevel100
    } else {
        Icon::BatteryLevel50
    }
}

/// The state of a device in words.
fn device_word(device: &Device) -> &'static str {
    if device.connected {
        "Connected"
    } else if device.paired {
        "Not connected"
    } else {
        "Not set up"
    }
}

/// The state of a device in words, with its battery level when known.
fn device_state(device: &Device) -> String {
    let state = device_word(device);
    match device.battery {
        Some(level) => format!("{state} · {level}% battery"),
        None => state.to_string(),
    }
}

/// The actions for one device, as a strip: connect, disconnect or pair, and
/// forget once the device is paired.
fn device_strip(device: &Device, ctx: Ctx) -> Element {
    let job = device_job(device);
    let (icon, label) = device_action(device);
    let mut actions = vec![strip_action(icon, label, move |_| {
        run_job(ctx, job.clone())
    })];
    if device.paired {
        let forget = device.clone();
        actions.push(strip_action(Icon::UserTrash, "Forget", move |_| {
            confirm_forget(ctx, &forget)
        }));
    }
    action_strip(actions).into()
}

/// The one action a device allows, with its icon and label.
fn device_action(device: &Device) -> (Icon, &'static str) {
    if device.connected {
        (Icon::WindowClose, "Disconnect")
    } else if device.paired {
        (Icon::Bluetooth, "Connect")
    } else {
        (Icon::ListAdd, "Pair")
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
            menu_item("Forget", move |_| confirm_forget(ctx, &forget))
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
        "Unnamed device".to_string()
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
    navigator: Navigator,
) -> Row {
    let route = Route::Device {
        mac: device.mac.clone(),
        name: device_name(device),
    };
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
                battery_icon(level),
                IconSize::Control,
                Paint::Muted.color(&tokens),
            ))
            .child(caption(format!("{level}%")));
    }
    let status =
        busy_label.or_else(|| (!device.connected).then(|| device_word(device).to_string()));
    if let Some(status) = status {
        glyphs = glyphs.child(caption(status));
    }
    row(device_name(device))
        .leading(device_icon(&device.icon))
        .trailing(Element::from(glyphs))
        .navigates()
        .on_press(move |_| navigator.push(route.clone()))
        .context_menu(device_menu(ctx, device))
}

/// Read Bluetooth now, then again while the page is open: every 2.5 seconds
/// while scanning and every 8 seconds otherwise.
fn use_polling(model: State<Loaded>, scanner: Option<Scanner>) {
    use_hook(move || {
        refresh(model);
        spawn(async move {
            loop {
                let wait = match &scanner {
                    Some(scanner) if scanner.running() => POLL_SCANNING,
                    _ => POLL_IDLE,
                };
                timer(wait).await;
                refresh(model);
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
        let navigator = use_navigator();
        let model = system::use_cached_persistent("bluetooth", load);
        let busy_state = use_state::<Option<Busy>>(|| None);
        let power_pending = use_state::<Option<bool>>(|| None);
        let visible_pending = use_state::<Option<bool>>(|| None);
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
            head = head.switch("Bluetooth", shown, move |next: bool| {
                set_switch_job(ctx, power_pending, power_job(next), next)
            });
        }

        let Some(loaded) = model.read().clone() else {
            return page_column()
                .child(head.clone())
                .child(skeleton_shape(&[1, 3, 1]));
        };
        let data = match loaded {
            Ok(data) => data,
            Err(e) => {
                return page_column().child(head.clone()).child(
                    status(Status::Error(e.0)).action("Try again", move |_| refresh(model)),
                );
            }
        };
        let Some(adapter) = data.adapter.clone() else {
            return page_column().child(head.clone()).child(
                status_page(Icon::Bluetooth, "No Bluetooth adapter found")
                    .description("This computer has no Bluetooth adapter that Settings can use."),
            );
        };
        if let Some(block) = data.blocked {
            let reason = match block {
                Block::Hardware => {
                    "A hardware switch has turned Bluetooth off. Turn it on to use Bluetooth."
                }
                Block::Software => {
                    "A system setting has blocked Bluetooth. Turn off \"Airplane mode\" or unblock Bluetooth to use it."
                }
            };
            return page_column()
                .child(head.clone())
                .child(status_page(Icon::Bluetooth, "Bluetooth is blocked").description(reason));
        }

        let busy = busy_state.read().clone();
        let ctx = Ctx {
            model,
            busy: busy_state,
            overlays,
        };

        let mut page = page_column().child(head);
        if adapter.powered {
            page = page.child(Group::new(None::<String>).row(switch_row(
                "Make this computer visible to other devices",
                (*visible_pending.read()).unwrap_or(adapter.discoverable),
                move |next: bool| set_switch_job(ctx, visible_pending, visibility_job(next), next),
            )));
        }

        if !adapter.powered {
            return page.child(
                status_page(Icon::Bluetooth, "Bluetooth is off")
                    .description("Turn on Bluetooth to find and connect devices.")
                    .action("Turn on", move |_| run_job(ctx, power_job(true))),
            );
        }

        let scanning = scanner.running();
        let (mut mine, nearby): (Vec<&Device>, Vec<&Device>) =
            data.devices.iter().partition(|device| device.paired);
        mine.sort_by_key(|device| (!device.connected, device.name.to_lowercase()));
        let no_mine = mine.is_empty();

        if !no_mine {
            page = page.child(
                Group::titled("My devices").rows(mine.into_iter().map(|device| {
                    Element::from(device_row(ctx, device, busy.as_ref(), tokens, navigator))
                })),
            );
        }
        // Nearby devices carries the scanning spinner. It stays on screen
        // while a scan runs, so the page does not jump when the first device
        // appears.
        if !nearby.is_empty() || scanning {
            let mut group = Group::titled("Nearby devices");
            if scanning {
                group = group.trailing(spinner(SpinnerSize::Small));
            }
            page = page.child(if nearby.is_empty() {
                group.row(status(Status::Empty("Looking for nearby devices.".into())))
            } else {
                group.rows(nearby.into_iter().map(|device| {
                    Element::from(device_row(ctx, device, busy.as_ref(), tokens, navigator))
                }))
            });
        } else if no_mine {
            page = page.child(Group::new(None::<String>).row(status(Status::Empty(
                "No devices found. Turn on the device and make it visible.".into(),
            ))));
        }
        page
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
        let navigator = use_navigator();
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
                navigator.pop();
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

        let yes_no = |value: bool| if value { "Yes" } else { "No" };
        let mut details = Group::titled("Details");
        if let Some(level) = device.battery {
            details = details.row(
                row("Battery")
                    .leading(battery_icon(level))
                    .value(format!("{level}%")),
            );
        }
        details = details
            .row(row("Type").value(device_kind(&device.icon)))
            .row(row("Address").value(device.mac.clone()))
            .row(row("Paired").value(yes_no(device.paired)));
        let rename_device = device.clone();
        details = details.row(entry_row(
            "Name",
            device.name.clone(),
            move |name: String| run_job(ctx, rename_job(&rename_device, name.trim())),
        ));
        let trust_device = device.clone();
        details = details.row(switch_row(
            "Connects automatically",
            device.trusted,
            move |on: bool| run_job(ctx, trust_job(&trust_device, on)),
        ));

        page_column()
            .child(identity)
            .maybe_child((adapter_on && busy.is_none()).then(|| device_strip(device, ctx)))
            .child(details)
    }
}

/// Ask before forgetting a device, then forget it.
fn confirm_forget(ctx: Ctx, device: &Device) {
    let job = remove_job(device);
    ctx.overlays.show_dialog(
        dialog(
            format!("Forget \"{}\"?", device_name(device)),
            "This computer forgets the pairing. Pair again to use the device.",
        )
        .cancel("Cancel")
        .response("Forget", ResponseKind::Destructive, move |_| {
            run_job(ctx, job.clone())
        }),
    );
}

/// Start loading this page's data before it is opened.
pub fn prefetch() {
    system::prefetch_persistent("bluetooth", load);
}
