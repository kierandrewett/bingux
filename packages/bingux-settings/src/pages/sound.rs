//! Sound: the output and input devices, what each one is, which one is the
//! default, and the volume of each. Also the applications playing or
//! recording audio.
//!
//! Devices and streams come from `wpctl status`. The kind of each device
//! comes from `pw-dump`, which names the port of each device, such as
//! "Headphones" or "HDMI / DisplayPort 2". Everything is read in the
//! background and polled every five seconds.

use std::collections::HashMap;
use std::time::Duration;

use freya::prelude::*;
use serde_json::Value;

use bingux_ui::scale::*;

use crate::apps;
use crate::routes::{Navigator, Route, use_navigator};
use crate::system::{self, CommandError, background, background_then};
use crate::ui::{
    Group, Icon, Overlays, Status, app_icon, caption, hstack, icon, icon_button, level_row,
    page_column, row, skeleton, status, use_overlays, use_tokens,
};

const TIMEOUT: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_secs(5);
const TREE: &[char] = &['│', '├', '└', '─'];
/// Up to this many devices of one kind are listed on the page. More than
/// this, the page shows the default and opens a list.
const INLINE_DEVICES: usize = 3;

type ModelState = State<Option<Result<Model, CommandError>>>;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Output,
    Input,
}

impl Kind {
    fn target(self) -> &'static str {
        match self {
            Kind::Output => "@DEFAULT_AUDIO_SINK@",
            Kind::Input => "@DEFAULT_AUDIO_SOURCE@",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Kind::Output => "Output",
            Kind::Input => "Input",
        }
    }

    fn none_found(self) -> &'static str {
        match self {
            Kind::Output => "No output devices found.",
            Kind::Input => "No input devices found.",
        }
    }
}

/// What a device is. It picks the device's icon.
#[derive(Clone, Copy, Default, PartialEq)]
enum Form {
    Headphones,
    Speakers,
    Microphone,
    Webcam,
    Display,
    Bluetooth,
    #[default]
    Card,
}

impl Form {
    fn icon(self) -> Icon {
        match self {
            Form::Headphones => Icon::Headphones,
            Form::Speakers => Icon::Speakers,
            Form::Microphone => Icon::AudioInputMicrophone,
            Form::Webcam => Icon::CameraWeb,
            Form::Display => Icon::Display,
            Form::Bluetooth => Icon::Bluetooth,
            Form::Card => Icon::AudioCard,
        }
    }
}

#[derive(Clone, PartialEq)]
struct Device {
    id: u32,
    /// The port, such as "Headphones". The device's own name when the port
    /// is not known.
    name: String,
    /// The hardware behind the port, such as "Fosi Audio K5 Pro". Empty when
    /// it is not known.
    detail: String,
    form: Form,
    is_default: bool,
}

/// Volume of the default device of one kind, as `wpctl get-volume` reports it.
#[derive(Clone, PartialEq)]
struct Level {
    id: u32,
    /// 0.0 to 1.0, as wpctl uses it.
    volume: f64,
    muted: bool,
}

#[derive(Clone, PartialEq)]
struct Stream {
    id: u32,
    name: String,
    /// Devices the stream plays to or records from, while it is not paused.
    playing_to: Vec<String>,
    recording_from: Vec<String>,
    /// The stream has a link, and every link is paused.
    paused: bool,
    /// The stream's own volume and mute state, when WirePlumber reports them.
    level: Option<(f64, bool)>,
    /// The icon name to draw for the application that owns the stream.
    icon: String,
    /// The desktop id of that application, when it is installed.
    app: Option<String>,
}

/// What the streams of one application are doing, for its subtitle.
fn summary_of(streams: &[Stream]) -> String {
    let mut playing_to = Vec::new();
    let mut recording_from = Vec::new();
    for stream in streams {
        for name in &stream.playing_to {
            push_unique(&mut playing_to, name.clone());
        }
        for name in &stream.recording_from {
            push_unique(&mut recording_from, name.clone());
        }
    }
    let mut parts = Vec::new();
    if !playing_to.is_empty() {
        parts.push(format!("Playing to {}", playing_to.join(", ")));
    }
    if !recording_from.is_empty() {
        parts.push(format!("Recording from {}", recording_from.join(", ")));
    }
    if parts.is_empty() {
        "Paused".to_string()
    } else {
        parts.join("; ")
    }
}

#[derive(Clone, Default, PartialEq)]
struct Model {
    outputs: Vec<Device>,
    inputs: Vec<Device>,
    output: Option<Level>,
    input: Option<Level>,
    streams: Vec<Stream>,
}

#[derive(Clone, Copy)]
struct VolumeQueue {
    /// The newest requested percent per node id; an older request for the
    /// same node is dropped.
    pending: State<Vec<(u32, u32)>>,
    /// Whether a `set-volume` is running.
    busy: State<bool>,
}

/// Everything a row needs to change the sound: the loaded model, the
/// overlays for failures, the volume queue and the subpage navigator.
#[derive(Clone, Copy)]
struct Ctx {
    model: ModelState,
    overlays: Overlays,
    queue: VolumeQueue,
    navigator: Navigator,
}

/// Column of the `ID.` separator. wpctl right-aligns ids, so every node line
/// shares this column even when its id has one more digit than the others.
fn id_column(line: &str) -> usize {
    line.find(". ").unwrap_or(0)
}

/// Split `ID. rest` into the object id and the rest of the line.
fn entry(body: &str) -> Option<(u32, &str)> {
    let (id, rest) = body.split_once(". ")?;
    Some((id.trim().parse().ok()?, rest.trim()))
}

/// `*  125. Name  [vol: 0.95]`, where `*` marks the default device.
fn parse_device(body: &str) -> Option<Device> {
    let (is_default, rest) = match body.strip_prefix('*') {
        Some(rest) => (true, rest.trim_start()),
        None => (false, body),
    };
    let (id, name) = entry(rest)?;
    let name = match name.rfind(" [vol") {
        Some(index) => &name[..index],
        None => name,
    };
    Some(Device {
        id,
        name: name.trim().to_string(),
        detail: String::new(),
        form: Form::default(),
        is_default,
    })
}

/// The device a stream link points at: `Fosi Audio K5 Pro:playback_FL [init]`.
fn link_device(target: &str) -> String {
    let head = target.split('[').next().unwrap_or(target).trim();
    match head.rsplit_once(':') {
        Some((device, _port)) => device.trim().to_string(),
        None => head.to_string(),
    }
}

fn push_unique(list: &mut Vec<String>, name: String) {
    if !list.contains(&name) {
        list.push(name);
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Section {
    Sinks,
    Sources,
    Streams,
}

/// Parse the Sinks, Sources and Streams sections of the Audio part of `wpctl status`.
fn parse_status(text: &str) -> Model {
    let mut model = Model::default();
    let mut in_audio = false;
    let mut section: Option<Section> = None;
    let mut node_indent: Option<usize> = None;
    let mut current: Option<usize> = None;

    for line in text.lines() {
        // Top-level headers ("Audio", "Video", "Settings") start in column 0.
        if line.starts_with(char::is_alphanumeric) {
            in_audio = line.starts_with("Audio");
            section = None;
            continue;
        }
        if !in_audio {
            continue;
        }
        let body = line.trim_start_matches(|c: char| c.is_whitespace() || TREE.contains(&c));
        if body.is_empty() {
            continue;
        }
        if body.ends_with(':') {
            section = match body {
                "Sinks:" => Some(Section::Sinks),
                "Sources:" => Some(Section::Sources),
                "Streams:" => {
                    node_indent = None;
                    current = None;
                    Some(Section::Streams)
                }
                _ => None,
            };
            continue;
        }
        match section {
            Some(Section::Sinks) => model.outputs.extend(parse_device(body)),
            Some(Section::Sources) => model.inputs.extend(parse_device(body)),
            Some(Section::Streams) => {
                let indent = id_column(line);
                let Some((id, rest)) = entry(body) else {
                    continue;
                };
                // Application nodes sit at the first entry's indent; their
                // port lines are indented further and carry the links.
                if *node_indent.get_or_insert(indent) == indent {
                    model.streams.push(Stream {
                        id,
                        name: rest.to_string(),
                        playing_to: Vec::new(),
                        recording_from: Vec::new(),
                        paused: false,
                        level: None,
                        icon: String::new(),
                        app: None,
                    });
                    current = Some(model.streams.len() - 1);
                } else if let Some(stream) = current.and_then(|i| model.streams.get_mut(i)) {
                    let paused = rest.contains("[paused]");
                    if let Some(index) = rest.find('>') {
                        let device = link_device(&rest[index + 1..]);
                        if paused {
                            stream.paused = true;
                        } else {
                            push_unique(&mut stream.playing_to, device);
                        }
                    } else if let Some(index) = rest.find('<') {
                        let device = link_device(&rest[index + 1..]);
                        if paused {
                            stream.paused = true;
                        } else {
                            push_unique(&mut stream.recording_from, device);
                        }
                    }
                }
            }
            None => {}
        }
    }
    // A stream with no link at all is not playing or recording anything.
    model.streams.retain(|stream| {
        stream.paused || !stream.playing_to.is_empty() || !stream.recording_from.is_empty()
    });
    model
}

fn parse_volume(text: &str) -> Option<(f64, bool)> {
    let rest = text.trim().strip_prefix("Volume:")?.trim();
    let volume = rest.split_whitespace().next()?.parse::<f64>().ok()?;
    Some((volume, rest.contains("MUTED")))
}

fn read_level(kind: Kind, devices: &[Device]) -> Result<Option<Level>, CommandError> {
    let Some(default) = devices.iter().find(|device| device.is_default) else {
        return Ok(None);
    };
    let text = system::run("wpctl", &["get-volume", kind.target()], TIMEOUT)?;
    let (volume, muted) = parse_volume(&text)
        .ok_or_else(|| CommandError("The volume could not be read.".to_string()))?;
    Ok(Some(Level {
        id: default.id,
        volume,
        muted,
    }))
}

/// What `pw-dump` says about one device node: the port it uses, the hardware
/// behind the port, and the icon that suits it.
#[derive(Clone, Default)]
struct Hardware {
    /// The port's name, such as "Headphones". Empty when the node has none.
    port: String,
    /// The hardware's name, such as "Fosi Audio K5 Pro", or what the node
    /// is when it has no hardware, such as "Network speaker".
    detail: String,
    form: Form,
}

/// One port of a card, from a device's routes.
struct Port {
    index: u64,
    direction: String,
    description: String,
    port_type: String,
    product: String,
}

/// A piece of hardware, with the ports it offers.
struct Card {
    product: String,
    ports: Vec<Port>,
}

impl Card {
    /// The port that a device node with `index` uses in `direction`.
    fn port(&self, index: Option<u64>, direction: &str) -> Option<&Port> {
        let index = index?;
        self.ports
            .iter()
            .find(|port| port.index == index && port.direction == direction)
    }
}

fn field<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// A property that `pw-dump` may write as a number or as text.
fn number(value: Option<&Value>) -> Option<u64> {
    match value? {
        Value::Number(number) => number.as_u64(),
        Value::String(text) => text.parse().ok(),
        _ => None,
    }
}

/// One route of a device. Its `info` list is a count, then key and value
/// pairs, all as text.
fn read_port(route: &Value, product: &str) -> Option<Port> {
    let index = route.get("device")?.as_u64()?;
    let mut props: HashMap<&str, &str> = HashMap::new();
    if let Some(info) = route.get("info").and_then(Value::as_array) {
        for pair in info.get(1..).unwrap_or_default().chunks_exact(2) {
            if let (Some(key), Some(value)) = (pair[0].as_str(), pair[1].as_str()) {
                props.insert(key, value);
            }
        }
    }
    Some(Port {
        index,
        direction: field(route, "direction").to_string(),
        description: field(route, "description").to_string(),
        port_type: props
            .get("port.type")
            .copied()
            .unwrap_or_default()
            .to_string(),
        product: props
            .get("device.product.name")
            .copied()
            .filter(|name| !name.is_empty())
            .unwrap_or(product)
            .to_string(),
    })
}

/// What a device node is, from its name, its bus, the hardware it belongs to
/// and its port. A camera's microphone is a webcam, and a Bluetooth device
/// is Bluetooth whatever its port is.
fn form_of(node: &str, bus: &str, product: &str, port_type: &str, port: &str) -> Form {
    let port_type = port_type.to_ascii_lowercase();
    let port = port.to_ascii_lowercase();
    if bus == "bluetooth" || node.starts_with("bluez_") {
        Form::Bluetooth
    } else if node.starts_with("raop_") {
        Form::Speakers
    } else if product.to_ascii_lowercase().contains("camera") {
        Form::Webcam
    } else if port_type.contains("mic") || port.contains("microphone") {
        Form::Microphone
    } else if port_type.contains("headphone")
        || port_type.contains("headset")
        || port.contains("headphone")
        || port.contains("headset")
    {
        Form::Headphones
    } else if port_type.contains("hdmi") || port_type.contains("displayport") {
        Form::Display
    } else if port_type.contains("speaker") {
        Form::Speakers
    } else {
        Form::Card
    }
}

/// Read every device node's hardware and port from `pw-dump`, keyed by node
/// id. Empty when `pw-dump` is missing or fails: devices then keep the names
/// from `wpctl` and the generic icon.
fn read_hardware() -> HashMap<u32, Hardware> {
    // `system::run` waits for the process to exit before reading its output,
    // and `pw-dump` writes more than a pipe buffer holds, so it would block
    // and time out. `output()` drains both pipes while the process runs.
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(std::process::Command::new("pw-dump").output());
    });
    let Ok(Ok(output)) = receiver.recv_timeout(TIMEOUT) else {
        return HashMap::new();
    };
    if !output.status.success() {
        return HashMap::new();
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let Ok(Value::Array(objects)) = serde_json::from_str::<Value>(&text) else {
        return HashMap::new();
    };
    let object_type = |object: &Value| field(object, "type").to_string();

    let mut cards: HashMap<u64, Card> = HashMap::new();
    for object in &objects {
        if object_type(object) != "PipeWire:Interface:Device" {
            continue;
        }
        let (Some(id), Some(props)) = (
            object.get("id").and_then(Value::as_u64),
            object.pointer("/info/props"),
        ) else {
            continue;
        };
        let product = field(props, "device.description").to_string();
        let routes = object
            .pointer("/info/params/Route")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let ports = routes
            .iter()
            .filter_map(|route| read_port(route, &product))
            .collect();
        cards.insert(id, Card { product, ports });
    }

    let mut hardware = HashMap::new();
    for object in &objects {
        if object_type(object) != "PipeWire:Interface:Node" {
            continue;
        }
        let (Some(id), Some(props)) = (
            object.get("id").and_then(Value::as_u64),
            object.pointer("/info/props"),
        ) else {
            continue;
        };
        let direction = match field(props, "media.class") {
            "Audio/Sink" => "Output",
            "Audio/Source" => "Input",
            _ => continue,
        };
        let node = field(props, "node.name");
        let bus = field(props, "device.bus");
        let card = number(props.get("device.id")).and_then(|card| cards.get(&card));
        let port =
            card.and_then(|card| card.port(number(props.get("card.profile.device")), direction));
        let port_name = port
            .map(|port| port.description.clone())
            .unwrap_or_default();
        let port_type = port.map(|port| port.port_type.as_str()).unwrap_or_default();
        let product = port
            .map(|port| port.product.clone())
            .or_else(|| card.map(|card| card.product.clone()))
            .unwrap_or_default();
        let detail = if node.starts_with("raop_") {
            "Network speaker".to_string()
        } else if bus == "bluetooth" || node.starts_with("bluez_") {
            "Bluetooth".to_string()
        } else {
            product.clone()
        };
        let form = form_of(node, bus, &product, port_type, &port_name);
        hardware.insert(
            id as u32,
            Hardware {
                port: port_name,
                detail,
                form,
            },
        );
    }
    hardware
}

fn describe(device: &mut Device, hardware: &HashMap<u32, Hardware>) {
    match hardware.get(&device.id) {
        Some(info) => {
            if !info.port.is_empty() {
                device.name = info.port.clone();
            }
            device.detail = info.detail.clone();
            device.form = info.form;
        }
        None => device.form = form_of(&device.name, "", "", "", ""),
    }
}

fn load() -> Result<Model, CommandError> {
    let text = system::run("wpctl", &["status"], TIMEOUT)?;
    let mut model = parse_status(&text);
    let hardware = read_hardware();
    for device in model.outputs.iter_mut().chain(model.inputs.iter_mut()) {
        describe(device, &hardware);
    }
    model.output = read_level(Kind::Output, &model.outputs)?;
    model.input = read_level(Kind::Input, &model.inputs)?;
    let apps = apps::installed();
    for stream in &mut model.streams {
        // A stream without a volume control (monitor-only nodes) is shown
        // without a slider rather than failing the page.
        stream.level = system::run("wpctl", &["get-volume", &stream.id.to_string()], TIMEOUT)
            .ok()
            .and_then(|text| parse_volume(&text));
        identify(stream, &apps);
    }
    model.streams.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then(a.id.cmp(&b.id))
    });
    Ok(model)
}

/// The value of `key` in `wpctl inspect` output, which prints `key = "value"`.
fn property<'a>(text: &'a str, key: &str) -> &'a str {
    text.lines()
        .find_map(|line| {
            let (name, value) = line.trim_start_matches(['*', ' ']).split_once('=')?;
            (name.trim() == key).then(|| value.trim().trim_matches('"'))
        })
        .unwrap_or_default()
}

/// Work out which application owns `stream` and which icon to draw for it.
/// The icon the stream names is used when the theme has it; an application
/// that names none, or one the theme lacks, gets its desktop entry's icon.
fn identify(stream: &mut Stream, apps: &[apps::AppEntry]) {
    let text =
        system::run("wpctl", &["inspect", &stream.id.to_string()], TIMEOUT).unwrap_or_default();
    let named = property(&text, "application.icon-name");
    let app = apps::for_stream(
        apps,
        property(&text, "application.id"),
        property(&text, "application.process.binary"),
        &stream.name,
    );
    let theme_has = |name: &str| {
        !name.is_empty() && bingux_ui::app_icon::resolve(name, ICON_APP as u32 * 2).is_some()
    };
    stream.icon = if theme_has(named) {
        named.to_string()
    } else if let Some(app) = app.as_ref().filter(|app| !app.icon.is_empty()) {
        app.icon.clone()
    } else {
        named.to_string()
    };
    stream.app = app.map(|app| app.id);
}

/// Run a `wpctl` write on a background thread, then toast a failure and
/// reload the model so the page shows what the backend now reports.
fn apply(args: Vec<String>, model: ModelState, overlays: Overlays) {
    background_then(
        move || {
            let refs: Vec<&str> = args.iter().map(String::as_str).collect();
            system::run("wpctl", &refs, TIMEOUT)
        },
        move |result| {
            if let Err(e) = result {
                overlays.toast(e.0);
            }
            background(model, load);
        },
    );
}

fn set_mute(ctx: Ctx, id: u32, muted: bool) {
    apply(
        vec![
            "set-mute".to_string(),
            id.to_string(),
            if muted { "1" } else { "0" }.to_string(),
        ],
        ctx.model,
        ctx.overlays,
    );
}

fn update_level(mut model: ModelState, kind: Kind, change: impl FnOnce(&mut Level)) {
    let mut guard = model.write();
    let Some(Ok(loaded)) = guard.as_mut() else {
        return;
    };
    let slot = match kind {
        Kind::Output => &mut loaded.output,
        Kind::Input => &mut loaded.input,
    };
    if let Some(level) = slot.as_mut() {
        change(level);
    }
}

/// Volume writes are coalesced: while one `set-volume` runs, a newer slider
/// position for the same node replaces the pending one, so the last
/// position always wins and a drag never queues a write per pixel.
fn set_volume(queue: VolumeQueue, overlays: Overlays, id: u32, percent: u32) {
    let mut pending = queue.pending;
    let mut busy = queue.busy;
    {
        let mut list = pending.write();
        match list.iter_mut().find(|(node, _)| *node == id) {
            Some(entry) => entry.1 = percent,
            None => list.push((id, percent)),
        }
    }
    if !*busy.read() {
        busy.set(true);
        push_volume(queue, overlays);
    }
}

fn push_volume(queue: VolumeQueue, overlays: Overlays) {
    let mut pending = queue.pending;
    let mut busy = queue.busy;
    let next = {
        let mut list = pending.write();
        if list.is_empty() {
            None
        } else {
            Some(list.remove(0))
        }
    };
    let Some((id, percent)) = next else {
        busy.set(false);
        return;
    };
    background_then(
        move || {
            let id = id.to_string();
            let percent = format!("{percent}%");
            system::run("wpctl", &["set-volume", &id, &percent], TIMEOUT)
        },
        move |result| {
            if let Err(e) = result {
                overlays.toast(e.0);
            }
            push_volume(queue, overlays);
        },
    );
}

/// Optimistic volume or mute for an application stream until the next poll.
fn update_stream(mut model: ModelState, id: u32, change: impl FnOnce(&mut (f64, bool))) {
    let mut guard = model.write();
    let Some(Ok(loaded)) = guard.as_mut() else {
        return;
    };
    if let Some(stream) = loaded.streams.iter_mut().find(|stream| stream.id == id) {
        if let Some(level) = stream.level.as_mut() {
            change(level);
        }
    }
}

/// The icon that shows a volume: crossed out when muted, then three steps up.
fn level_icon(volume: f64, muted: bool) -> Icon {
    if muted || volume <= 0.0 {
        Icon::AudioVolumeMuted
    } else if volume < 0.34 {
        Icon::AudioVolumeLow
    } else if volume < 0.67 {
        Icon::AudioVolumeMedium
    } else {
        Icon::Sound
    }
}

/// The button that mutes and unmutes. Its icon shows the current level.
fn mute_button(volume: f64, muted: bool, on_mute: impl Into<EventHandler<()>>) -> Element {
    let label = if muted { "Unmute" } else { "Mute" };
    icon_button(level_icon(volume, muted), on_mute)
        .tooltip(label)
        .size(BOX_LG)
        .into()
}

/// One device as a row. Pressing it makes it the default.
fn device_row(device: &Device, ctx: Ctx) -> Element {
    let id = device.id;
    let mut item = row(device.name.clone())
        .leading(device.form.icon())
        .selected(device.is_default)
        .on_press(move |_| {
            apply(
                vec!["set-default".to_string(), id.to_string()],
                ctx.model,
                ctx.overlays,
            )
        });
    if !device.detail.is_empty() {
        item = item.subtitle(device.detail.clone());
    }
    item.into()
}

/// The default device, as a row that opens the full device list.
fn current_device_row(devices: &[Device], input: bool, navigator: Navigator) -> Element {
    let (name, detail, form) = match devices.iter().find(|device| device.is_default) {
        Some(device) => (device.name.clone(), device.detail.clone(), device.form),
        None => (
            "Choose a device".to_string(),
            String::new(),
            Form::default(),
        ),
    };
    let mut item = row(name)
        .leading(form.icon())
        .navigates()
        .on_press(move |_| navigator.push(Route::SoundDevices { input }));
    if !detail.is_empty() {
        item = item.subtitle(detail);
    }
    item.into()
}

fn channel_group(kind: Kind, devices: &[Device], level: Option<Level>, ctx: Ctx) -> Group {
    let group = Group::titled(kind.title());
    if devices.is_empty() {
        return group.row(status(Status::Empty(kind.none_found().into())));
    }
    let mut group = if devices.len() <= INLINE_DEVICES {
        group.rows(devices.iter().map(|device| device_row(device, ctx)))
    } else {
        group.row(current_device_row(
            devices,
            kind == Kind::Input,
            ctx.navigator,
        ))
    };

    if let Some(level) = level {
        let id = level.id;
        let mute_to = !level.muted;
        group = group.row(
            level_row("Volume", level.volume * 100.0, move |value: f64| {
                let percent = value.round().clamp(0.0, 100.0) as u32;
                update_level(ctx.model, kind, |current| {
                    current.volume = f64::from(percent) / 100.0;
                });
                set_volume(ctx.queue, ctx.overlays, id, percent);
            })
            // The blank leading box lines the text up with the device rows above.
            .leading(rect().width(Size::px(ICON_LG)))
            .trailing(mute_button(level.volume, level.muted, move |_| {
                update_level(ctx.model, kind, |current| current.muted = mute_to);
                set_mute(ctx, id, mute_to);
            })),
        );
    }
    group
}

/// One row per application: its icon, name, what its streams are connected to
/// and a volume and mute control. The controls change every stream of the
/// application at once.
fn app_row(streams: &[Stream], ctx: Ctx) -> Element {
    let first = &streams[0];
    let ids: Vec<u32> = streams.iter().map(|stream| stream.id).collect();
    match first.level {
        Some((volume, muted)) => {
            let mute_ids = ids.clone();
            level_row(first.name.clone(), volume * 100.0, move |value: f64| {
                let percent = value.round().clamp(0.0, 100.0) as u32;
                for id in &ids {
                    update_stream(ctx.model, *id, |level| {
                        level.0 = f64::from(percent) / 100.0;
                    });
                    set_volume(ctx.queue, ctx.overlays, *id, percent);
                }
            })
            .subtitle(summary_of(streams))
            .leading(app_icon(first.icon.clone(), ICON_APP))
            .trailing(mute_button(volume, muted, move |_| {
                for id in &mute_ids {
                    update_stream(ctx.model, *id, |level| level.1 = !muted);
                    set_mute(ctx, *id, !muted);
                }
            }))
            .into()
        }
        None => Element::from(
            row(first.name.clone())
                .leading_app(first.icon.clone())
                .subtitle(summary_of(streams))
                .value("Volume cannot be changed"),
        ),
    }
}

/// Group streams by application, keeping the order of each group's first
/// stream. Streams of one application share an id, or else a name.
fn by_application(streams: &[Stream]) -> Vec<Vec<Stream>> {
    let key = |stream: &Stream| stream.app.clone().unwrap_or_else(|| stream.name.clone());
    let mut groups: Vec<Vec<Stream>> = Vec::new();
    for stream in streams {
        match groups
            .iter_mut()
            .find(|group| key(&group[0]) == key(stream))
        {
            Some(group) => group.push(stream.clone()),
            None => groups.push(vec![stream.clone()]),
        }
    }
    groups
}

/// Poll the sound state while the page is open, and load it once now.
fn use_sound_model() -> ModelState {
    let model = system::cached("sound");
    use_hook(move || {
        background(model, load);
        spawn(async move {
            loop {
                timer(POLL).await;
                background(model, load);
            }
        });
    });
    model
}

fn use_ctx() -> Ctx {
    let model = use_sound_model();
    let overlays = use_overlays();
    let queue = VolumeQueue {
        pending: use_state(Vec::new),
        busy: use_state(|| false),
    };
    Ctx {
        model,
        overlays,
        queue,
        navigator: use_navigator(),
    }
}

#[derive(PartialEq)]
pub struct SoundPage;

impl Component for SoundPage {
    fn render(&self) -> impl IntoElement {
        let ctx = use_ctx();
        let tokens = use_tokens();
        let Some(loaded) = ctx.model.read().clone() else {
            return page_column().child(skeleton());
        };
        let data = match loaded {
            Ok(data) => data,
            Err(e) => {
                return page_column().child(
                    status(Status::Error(e.0))
                        .action("Try again", move |_| background(ctx.model, load)),
                );
            }
        };

        let mut page = page_column();

        page = page
            .child(channel_group(
                Kind::Output,
                &data.outputs,
                data.output.clone(),
                ctx,
            ))
            .child(channel_group(
                Kind::Input,
                &data.inputs,
                data.input.clone(),
                ctx,
            ));

        let applications = Group::titled("Applications");
        if data.streams.is_empty() {
            page = page.child(
                applications.row(
                    hstack(S3)
                        .child(icon(Icon::Sound, ICON_MD, tokens.muted))
                        .child(caption("No application is playing or recording sound.")),
                ),
            );
        } else {
            let groups = by_application(&data.streams);
            page = page.child(applications.rows(groups.iter().map(|group| app_row(group, ctx))));
        }
        page
    }
}

/// The full list of output or input devices, opened from the Sound page when
/// there are more devices than fit there.
#[derive(PartialEq)]
pub struct DevicePage {
    pub input: bool,
}

impl Component for DevicePage {
    fn render(&self) -> impl IntoElement {
        let ctx = use_ctx();
        let kind = if self.input {
            Kind::Input
        } else {
            Kind::Output
        };

        let Some(loaded) = ctx.model.read().clone() else {
            return page_column().child(skeleton());
        };
        let data = match loaded {
            Ok(data) => data,
            Err(e) => {
                return page_column().child(
                    status(Status::Error(e.0))
                        .action("Try again", move |_| background(ctx.model, load)),
                );
            }
        };

        let page = page_column();
        let devices = if self.input {
            &data.inputs
        } else {
            &data.outputs
        };
        if devices.is_empty() {
            return page
                .child(Group::new(None).row(status(Status::Empty(kind.none_found().into()))));
        }
        page.child(Group::new(None).rows(devices.iter().map(|device| device_row(device, ctx))))
    }
}

/// The sound of one application: a volume control for each of its streams,
/// shown on the application's page. It draws nothing while the application
/// is not playing or recording.
#[derive(PartialEq)]
pub struct AppSound {
    pub app_id: String,
}

impl Component for AppSound {
    fn render(&self) -> impl IntoElement {
        let ctx = use_ctx();

        let Some(Ok(data)) = ctx.model.read().clone() else {
            return Element::from(rect());
        };
        let streams: Vec<&Stream> = data
            .streams
            .iter()
            .filter(|stream| stream.app.as_deref() == Some(self.app_id.as_str()))
            .collect();
        if streams.is_empty() {
            return Element::from(rect());
        }
        let group = page_column();
        group
            .child(
                Group::titled("Sound").rows(
                    streams
                        .iter()
                        .map(|stream| app_row(std::slice::from_ref(*stream), ctx)),
                ),
            )
            .into()
    }
}

/// Start loading this page's data before it is opened.
pub fn prefetch() {
    system::prefetch("sound", load);
}
