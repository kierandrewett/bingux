//! Sound: the output and input devices, what each one is, which one is the
//! default, and the volume of each. Also the applications playing or
//! recording audio.
//!
//! Devices and streams come from `wpctl status`. The kind of each device
//! comes from `pw-dump`, which names the port of each device, such as
//! "Headphones" or "HDMI / DisplayPort 2". Everything is read in the
//! background and polled every five seconds.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

use freya::prelude::*;
use serde_json::Value;

use bingux_audio_effects as engine;
use bingux_ui::prelude::{PageHead, Window, popup, use_window};
use bingux_ui::style::*;
use bingux_ui::theme::Tokens;
use bingux_ui::typography::CAPTION;
use bingux_ui::window::{PopupFlags, WindowParent};

use crate::apps;
use crate::routes;
use crate::system::{self, CommandError, background_then};
use crate::ui::{
    Group, Icon, IconButton, LevelRow, MenuItem, Overlays, Status, action_button, app_icon,
    dropdown, hstack, icon_button, level_row, menu_item, page_column, row, status, switch_row,
    use_overlays, use_tokens,
};
use bingux_ui::prelude::use_router;

use self::meter::{LevelMeter, Source};
use super::loading::revealed;

mod applications;
mod effects;
mod meter;

pub use self::applications::ApplicationsPage;

const TIMEOUT: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_secs(5);
const TREE: &[char] = &['│', '├', '└', '─'];
/// The top of a volume slider once the boost is on, in percent.
const ABOVE_MAX: f64 = 150.0;

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

    fn device_title(self) -> &'static str {
        match self {
            Kind::Output => "Output device",
            Kind::Input => "Input device",
        }
    }

    /// The title of this kind's volume slider.
    fn volume_title(self) -> &'static str {
        match self {
            Kind::Output => "Output volume",
            Kind::Input => "Input volume",
        }
    }
}

impl Kind {
    /// The meter that shows what this kind of device carries.
    fn source(self) -> Source {
        match self {
            Kind::Output => Source::Output,
            Kind::Input => Source::Input,
        }
    }
}

/// What a device is. It picks the device's icon.
#[derive(Clone, Copy, Default, PartialEq, serde::Serialize, serde::Deserialize)]
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
            Form::Card => Icon::Sound,
        }
    }
}

#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
struct Device {
    id: u32,
    /// The port, such as "Headphones". The device's own name when the port
    /// is not known.
    name: String,
    /// The hardware behind the port, such as "Fosi Audio K5 Pro". Empty when
    /// it is not known.
    detail: String,
    form: Form,
    /// The node's `node.name`, which the effects engine names its device by.
    #[serde(default)]
    node_name: String,
    is_default: bool,
}

impl Device {
    /// The port and the hardware, "Headphones - Fosi Audio K5 Pro", or the
    /// name alone when the hardware is not known or repeats the name.
    fn full_name(&self) -> String {
        let detail = self.detail.trim();
        if detail.is_empty() || detail == self.name {
            self.name.clone()
        } else {
            format!("{} - {detail}", self.name)
        }
    }
}

/// Volume of the default device of one kind, as `wpctl get-volume` reports it.
#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
struct Level {
    id: u32,
    /// 0.0 to 1.0, as wpctl uses it.
    volume: f64,
    muted: bool,
}

#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
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

#[derive(Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
struct Model {
    outputs: Vec<Device>,
    inputs: Vec<Device>,
    output: Option<Level>,
    input: Option<Level>,
    mix: Option<Mix>,
    streams: Vec<Stream>,
    /// Whether the output may go above 100%, from GNOME's
    /// `allow-volume-above-100-percent` key.
    above_100: bool,
}

#[derive(Clone, Copy)]
struct VolumeQueue {
    /// Writes waiting to run, one per key. A newer write with the same key
    /// replaces the one waiting.
    pending: State<Vec<Job>>,
    /// Whether a `set-volume` is running.
    busy: State<bool>,
}

/// One write to the audio backend: the program and its arguments. The key
/// names what is written, such as one stream's volume or the output's mix.
#[derive(Clone, PartialEq)]
struct Job {
    key: String,
    program: &'static str,
    args: Vec<String>,
}

/// Everything a row needs to change the sound: the loaded model, the
/// overlays for failures and the volume queue.
#[derive(Clone, Copy)]
struct Ctx {
    model: ModelState,
    overlays: Overlays,
    window: Window,
    queue: VolumeQueue,
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
        node_name: String::new(),
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
    let target = default.id.to_string();
    let text = system::run("wpctl", &["get-volume", &target], TIMEOUT)?;
    let (volume, muted) = parse_volume(&text)
        .ok_or_else(|| CommandError("Could not read the volume.".to_string()))?;
    Ok(Some(Level {
        id: default.id,
        volume,
        muted,
    }))
}

/// The output's channels as pactl reports them, each with its volume as a
/// fraction. Volumes are written per channel, so balance, fade and subwoofer
/// can each be set on their own.
#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
struct Mix {
    /// The default sink's name, which pactl writes to.
    sink: String,
    channels: Vec<(String, f64)>,
}

impl Mix {
    /// The `pactl` arguments that write every channel, in the sink's order.
    fn args(&self) -> Vec<String> {
        let mut args = vec!["set-sink-volume".to_string(), "@DEFAULT_SINK@".to_string()];
        args.extend(
            self.channels
                .iter()
                .map(|(_, volume)| format!("{}%", (volume * 100.0).round() as u32)),
        );
        args
    }

    fn has(&self, test: fn(&str) -> bool) -> bool {
        self.channels.iter().any(|(name, _)| test(name))
    }
}

fn is_lfe(name: &str) -> bool {
    name == "lfe"
}

fn is_rear(name: &str) -> bool {
    name.starts_with("rear") || name.starts_with("side")
}

fn is_front(name: &str) -> bool {
    !is_rear(name) && !is_lfe(name)
}

fn is_left(name: &str) -> bool {
    name.contains("left")
}

fn is_right(name: &str) -> bool {
    name.contains("right")
}

fn non_lfe(name: &str) -> bool {
    !is_lfe(name)
}

/// The largest volume among the channels `keep` picks.
fn peak_of(channels: &[(String, f64)], keep: fn(&str) -> bool) -> Option<f64> {
    channels
        .iter()
        .filter(|(name, _)| keep(name))
        .map(|(_, volume)| *volume)
        .reduce(f64::max)
}

/// The output as the sliders show it: a master volume, a balance and a fade
/// from -1 to 1 (0 is the centre) and a subwoofer from 0 to 1. Each is read
/// back from the channels, so the sliders always match what the sink holds.
#[derive(Clone, Copy)]
struct Shape {
    volume: f64,
    balance: f64,
    fade: f64,
    sub: f64,
}

impl Shape {
    fn of(channels: &[(String, f64)]) -> Self {
        let volume = peak_of(channels, non_lfe).unwrap_or(0.0);
        let balance = match (peak_of(channels, is_left), peak_of(channels, is_right)) {
            (Some(left), Some(right)) if left.max(right) > 0.0 => (right - left) / left.max(right),
            _ => 0.0,
        };
        let fade = match (peak_of(channels, is_front), peak_of(channels, is_rear)) {
            (Some(front), Some(rear)) if front.max(rear) > 0.0 => (front - rear) / front.max(rear),
            _ => 0.0,
        };
        let sub = match peak_of(channels, is_lfe) {
            Some(lfe) if volume > 0.0 => (lfe / volume).clamp(0.0, 1.0),
            _ => 1.0,
        };
        Self {
            volume,
            balance,
            fade,
            sub,
        }
    }

    /// The channel volumes for these settings, in the order of `names`. The
    /// louder side keeps its full volume, so the master reads back the same.
    /// No channel goes above 100%, or 150% with the boost on.
    fn render(&self, names: &[&str], above: bool) -> Vec<f64> {
        let top = if above { 1.5 } else { 1.0 };
        names
            .iter()
            .map(|name| {
                let value = if is_lfe(name) {
                    self.volume * self.sub
                } else {
                    let mut gain = 1.0;
                    if is_left(name) {
                        gain *= 1.0 - self.balance.max(0.0);
                    }
                    if is_right(name) {
                        gain *= 1.0 - (-self.balance).max(0.0);
                    }
                    if is_rear(name) {
                        gain *= 1.0 - self.fade.max(0.0);
                    } else {
                        gain *= 1.0 + self.fade.min(0.0);
                    }
                    self.volume * gain
                };
                value.clamp(0.0, top)
            })
            .collect()
    }
}

/// The default output's channels from `pactl`. Its channel map and the
/// per-channel volumes come from one `pactl list sinks`. Returns `None` when
/// `pactl` is missing; the slider then uses `wpctl` for the master volume.
fn read_mix(sink: Option<&str>) -> Option<Mix> {
    let info: Value =
        serde_json::from_str(&system::run("pactl", &["--format=json", "info"], TIMEOUT).ok()?)
            .ok()?;
    let sink = match sink {
        Some(name) => name.to_string(),
        None => info.get("default_sink_name")?.as_str()?.to_string(),
    };
    let list: Vec<Value> = serde_json::from_str(
        &system::run("pactl", &["--format=json", "list", "sinks"], TIMEOUT).ok()?,
    )
    .ok()?;
    let found = list.iter().find(|entry| field(entry, "name") == sink)?;
    let names: Vec<String> = match found.get("channel_map")? {
        Value::Array(items) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        Value::String(text) => text
            .split(',')
            .map(|name| name.trim().to_string())
            .collect(),
        _ => return None,
    };
    let volumes = found.get("volume")?;
    let channels: Vec<(String, f64)> = names
        .into_iter()
        .map(|name| {
            let percent = volumes
                .get(&name)
                .and_then(|entry| entry.get("value_percent"))
                .and_then(Value::as_str)
                .and_then(|text| text.trim_end_matches('%').parse::<f64>().ok())
                .unwrap_or(100.0);
            (name, percent / 100.0)
        })
        .collect();
    if channels.is_empty() {
        return None;
    }
    Some(Mix { sink, channels })
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
    /// The node's `node.name`.
    node_name: String,
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
                node_name: node.to_string(),
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
            device.node_name = info.node_name.clone();
            device.form = info.form;
        }
        None => device.form = form_of(&device.name, "", "", "", ""),
    }
}

/// While the effects run they are the default device, a filter that wpctl
/// lists apart from the sinks and sources. Mark the real device behind each
/// direction's effects as the default, so the page shows and controls it.
/// Returns the output's real device, by node name.
fn apply_effects(model: &mut Model) -> Option<String> {
    let state = engine::store::Store::default().state();
    let mut output = None;
    for (direction, devices) in [
        (engine::Direction::Output, &mut model.outputs),
        (engine::Direction::Input, &mut model.inputs),
    ] {
        let saved = state.of(direction);
        if !saved.enabled || devices.is_empty() || devices.iter().any(|device| device.is_default) {
            continue;
        }
        let at = devices
            .iter()
            .position(|device| Some(device.node_name.as_str()) == saved.device.as_deref())
            .unwrap_or(0);
        devices[at].is_default = true;
        if direction == engine::Direction::Output {
            output = Some(devices[at].node_name.clone());
        }
    }
    output
}

/// Point the effects of `direction` at another device, and use the preset
/// that is set to follow that device, if there is one.
fn set_effects_device(ctx: Ctx, direction: engine::Direction, device: String) {
    use engine::store::Store;
    let model = ctx.model;
    let overlays = ctx.overlays;
    begin_write();
    background_then(
        move || {
            let store = Store::default();
            store
                .update_state(|state| {
                    let saved = state.of_mut(direction);
                    if let Some(preset) = saved.autoload.get(&device).cloned() {
                        saved.preset = preset;
                    }
                    saved.device = Some(device);
                })
                .map_err(|e| e.0)?;
            engine::service::start(direction)
        },
        move |result: Result<(), String>| {
            end_write();
            if let Err(message) = result {
                overlays.toast(message);
            }
            reload(model);
        },
    );
}

fn load() -> Result<Model, CommandError> {
    load_model(false)
}

/// The model with each stream's volume, icon and application, which costs two
/// commands for every stream.
fn load_streams() -> Result<Model, CommandError> {
    load_model(true)
}

fn load_model(streams: bool) -> Result<Model, CommandError> {
    let text = system::run("wpctl", &["status"], TIMEOUT)?;
    let mut model = parse_status(&text);
    let hardware = read_hardware();
    for device in model.outputs.iter_mut().chain(model.inputs.iter_mut()) {
        describe(device, &hardware);
    }
    // While the effects run, they own the default device; the sound page
    // shows and controls the real device behind them.
    let routed = apply_effects(&mut model);
    model.output = read_level(Kind::Output, &model.outputs)?;
    model.input = read_level(Kind::Input, &model.inputs)?;
    model.mix = read_mix(routed.as_deref());
    if let (Some(mix), Some(level)) = (&model.mix, model.output.as_mut()) {
        level.volume = Shape::of(&mix.channels).volume;
    }
    model.above_100 =
        system::gsettings_get("org.gnome.desktop.sound", "allow-volume-above-100-percent")
            .is_ok_and(|value| value.trim() == "true");
    if streams {
        let apps = apps::installed();
        for stream in &mut model.streams {
            // A stream without a volume control (monitor-only nodes) is shown
            // without a slider rather than failing the page.
            stream.level = system::run("wpctl", &["get-volume", &stream.id.to_string()], TIMEOUT)
                .ok()
                .and_then(|text| parse_volume(&text));
            identify(stream, &apps);
        }
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
        !name.is_empty() && bingux_ui::app_icon::resolve(name, IconSize::App as u32 * 2).is_some()
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

/// The sound state, loaded in the background. Each load takes a new epoch and
/// only the newest load may write its result. A load is also dropped if a
/// `wpctl` write was pending when it started, or began or ended after it
/// started, so a slow read cannot put back a device the user just chose.
static EPOCH: AtomicU64 = AtomicU64::new(0);
/// Writes begun or finished, and writes still running.
static WRITES: AtomicU64 = AtomicU64::new(0);
static PENDING: AtomicU64 = AtomicU64::new(0);

fn begin_write() {
    PENDING.fetch_add(1, Ordering::SeqCst);
    WRITES.fetch_add(1, Ordering::SeqCst);
}

/// Call on the background thread once the `wpctl` write has returned.
fn end_write() {
    WRITES.fetch_add(1, Ordering::SeqCst);
    PENDING.fetch_sub(1, Ordering::SeqCst);
}

fn reload(model: ModelState) {
    let epoch = EPOCH.fetch_add(1, Ordering::SeqCst) + 1;
    // Read the write count before the pending count, so a write that begins
    // between the two reads is caught by the check below.
    let writes = WRITES.load(Ordering::SeqCst);
    let settled = PENDING.load(Ordering::SeqCst) == 0;
    let streams = detailed();
    background_then(
        move || {
            if streams {
                system::load_persistent("sound-streams", load_streams)
            } else {
                system::load_persistent("sound", load)
            }
        },
        move |result| {
            let current = settled
                && WRITES.load(Ordering::SeqCst) == writes
                && EPOCH.load(Ordering::SeqCst) == epoch;
            if current {
                let mut model = model;
                model.set(Some(result));
            }
        },
    );
}

/// Run a `wpctl` write on a background thread, then toast a failure and
/// reload the model so the page shows what the backend now reports.
fn apply(args: Vec<String>, model: ModelState, overlays: Overlays) {
    begin_write();
    background_then(
        move || {
            let refs: Vec<&str> = args.iter().map(String::as_str).collect();
            let result = system::run("wpctl", &refs, TIMEOUT);
            end_write();
            result
        },
        move |result| {
            if let Err(e) = result {
                overlays.toast(e.0);
            }
            reload(model);
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

/// Volume writes are coalesced: while one write runs, a newer write with the
/// same key replaces the waiting one, so the last value always wins and a
/// drag never queues a write per pixel.
fn set_volume(queue: VolumeQueue, overlays: Overlays, job: Job) {
    let mut pending = queue.pending;
    let mut busy = queue.busy;
    {
        let mut list = pending.write();
        match list.iter_mut().find(|waiting| waiting.key == job.key) {
            Some(entry) => *entry = job,
            None => list.push(job),
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
    let Some(job) = next else {
        busy.set(false);
        return;
    };
    background_then(
        move || {
            let args: Vec<&str> = job.args.iter().map(String::as_str).collect();
            system::run(job.program, &args, TIMEOUT)
        },
        move |result| {
            if let Err(e) = result {
                overlays.toast(e.0);
            }
            push_volume(queue, overlays);
        },
    );
}

/// A `wpctl` volume write for one node. Without the boost, the limit holds
/// the volume at 100%.
fn wpctl_volume_job(id: u32, percent: u32, above: bool) -> Job {
    let limit = if above { "1.5" } else { "1.0" };
    Job {
        key: format!("wpctl:{id}"),
        program: "wpctl",
        args: vec![
            "set-volume".to_string(),
            "-l".to_string(),
            limit.to_string(),
            id.to_string(),
            format!("{percent}%"),
        ],
    }
}

/// Optimistic volume for every stream of one application, then the write.
fn set_stream_volume(ctx: Ctx, ids: &[u32], percent: u32, above: bool) {
    for id in ids {
        update_stream(ctx.model, *id, |level| level.0 = f64::from(percent) / 100.0);
        set_volume(
            ctx.queue,
            ctx.overlays,
            wpctl_volume_job(*id, percent, above),
        );
    }
}

/// Optimistic mute or unmute for every stream of one application.
fn toggle_stream_mute(ctx: Ctx, ids: &[u32], muted: bool) {
    for id in ids {
        update_stream(ctx.model, *id, |level| level.1 = !muted);
        set_mute(ctx, *id, !muted);
    }
}

/// Change the output's balance, fade or subwoofer or its master volume. The
/// channels are rewritten together so every speaker moves in step.
fn update_mix(ctx: Ctx, change: impl FnOnce(&mut Shape)) {
    let mut model = ctx.model;
    let job = {
        let mut guard = model.write();
        let Some(Ok(loaded)) = guard.as_mut() else {
            return;
        };
        let above = loaded.above_100;
        let Some(mix) = loaded.mix.as_mut() else {
            return;
        };
        let mut shape = Shape::of(&mix.channels);
        change(&mut shape);
        let values = {
            let names: Vec<&str> = mix.channels.iter().map(|(name, _)| name.as_str()).collect();
            shape.render(&names, above)
        };
        for ((_, volume), value) in mix.channels.iter_mut().zip(values) {
            *volume = value;
        }
        let master = Shape::of(&mix.channels).volume;
        if let Some(level) = loaded.output.as_mut() {
            level.volume = master;
        }
        Job {
            key: "pactl:sink".to_string(),
            program: "pactl",
            args: mix.args(),
        }
    };
    set_volume(ctx.queue, ctx.overlays, job);
}

/// The balance, fade and subwoofer rows, shown when the output has the
/// channels for them, then a row that opens the test sheet.
fn mix_rows(mix: &Mix, ctx: Ctx) -> Vec<Element> {
    let shape = Shape::of(&mix.channels);
    let mut rows: Vec<Element> = Vec::new();
    if mix.has(is_left) && mix.has(is_right) {
        rows.push(
            mix_slider(
                "Balance",
                (shape.balance + 1.0) * 100.0,
                move |value: f64| {
                    let balance = centre_snapped(value / 100.0 - 1.0);
                    update_mix(ctx, move |shape| shape.balance = balance);
                },
            )
            .max(200.0)
            .mark(100.0)
            .value_text(side_text(shape.balance, "Left", "Right"))
            .into(),
        );
    }
    if mix.has(is_front) && mix.has(is_rear) {
        rows.push(
            mix_slider("Fade", (shape.fade + 1.0) * 50.0, move |value: f64| {
                let fade = centre_snapped(value / 50.0 - 1.0);
                update_mix(ctx, move |shape| shape.fade = fade);
            })
            .max(100.0)
            .mark(50.0)
            .value_text(side_text(shape.fade, "Rear", "Front"))
            .into(),
        );
    }
    if mix.has(is_lfe) {
        rows.push(
            mix_slider("Subwoofer", shape.sub * 100.0, move |value: f64| {
                let sub = value / 100.0;
                update_mix(ctx, move |shape| shape.sub = sub);
            })
            .value_text(format!("{:.0}%", shape.sub * 100.0))
            .into(),
        );
    }
    rows
}

/// A balance, fade or subwoofer slider. Its blank control box is the width of
/// a mute button, so every track starts and ends at one x-position within the
/// card, as the volume slider's does.
fn mix_slider(title: &str, value: f64, on_moved: impl Into<EventHandler<f64>>) -> LevelRow {
    level_row(title, value, on_moved).control(rect().tw_layout(&[w(Dim::Box9)]))
}

/// Snap a balance or fade within 2% of its centre onto the centre.
fn centre_snapped(value: f64) -> f64 {
    if value.abs() < 0.02 { 0.0 } else { value }
}

/// A slider's value as words: "Centre", or the side with its amount.
fn side_text(value: f64, low: &str, high: &str) -> String {
    if value.abs() < 0.005 {
        "Centre".to_string()
    } else if value < 0.0 {
        format!("{low} {:.0}%", -value * 100.0)
    } else {
        format!("{high} {:.0}%", value * 100.0)
    }
}

/// Name a channel as a person reads it.
fn channel_label(name: &str) -> String {
    match name {
        "front-left" => "Front left",
        "front-right" => "Front right",
        "front-center" => "Centre",
        "lfe" => "Subwoofer",
        "rear-left" => "Rear left",
        "rear-right" => "Rear right",
        "side-left" => "Side left",
        "side-right" => "Side right",
        other => other,
    }
    .to_string()
}

/// The short channel names `pw-play` takes, such as "FL".
fn short_channel(name: &str) -> &str {
    match name {
        "front-left" => "FL",
        "front-right" => "FR",
        "front-center" => "FC",
        "lfe" => "LFE",
        "rear-left" => "RL",
        "rear-right" => "RR",
        "side-left" => "SL",
        "side-right" => "SR",
        other => other,
    }
}

/// A one-second 440 Hz tone on channel `lit` alone, as a 16-bit PCM WAV with
/// one channel per speaker. A 20 ms fade in and out keeps it from clicking.
fn tone_wav(channels: usize, lit: usize) -> Vec<u8> {
    const RATE: u32 = 48_000;
    const FRAMES: usize = RATE as usize;
    let edge = (RATE / 50) as usize;
    let data_len = (FRAMES * channels * 2) as u32;
    let mut wav = Vec::with_capacity(44 + data_len as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&(channels as u16).to_le_bytes());
    wav.extend_from_slice(&RATE.to_le_bytes());
    wav.extend_from_slice(&(RATE * channels as u32 * 2).to_le_bytes());
    wav.extend_from_slice(&(channels as u16 * 2).to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    for frame in 0..FRAMES {
        let envelope = frame.min(FRAMES - 1 - frame).min(edge) as f32 / edge as f32;
        let t = frame as f32 / RATE as f32;
        let phase = 2.0 * std::f32::consts::PI * 440.0 * t;
        let sample = (0.3 * envelope * phase.sin() * i16::MAX as f32) as i16;
        for channel in 0..channels {
            let value = if channel == lit { sample } else { 0 };
            wav.extend_from_slice(&value.to_le_bytes());
        }
    }
    wav
}

/// Play the tone on one speaker of the sink: `pw-play`, or `paplay` when
/// `pw-play` is missing or fails.
fn play_tone(sink: &str, names: &[String], lit: usize) -> Result<(), CommandError> {
    let path = std::env::temp_dir().join(format!("bingux-sound-test-{}-{lit}.wav", names.len()));
    std::fs::write(&path, tone_wav(names.len(), lit))
        .map_err(|e| CommandError(format!("Could not write the test tone: {e}")))?;
    let file = path.to_string_lossy().into_owned();
    let short: Vec<&str> = names.iter().map(|name| short_channel(name)).collect();
    let map = short.join(",");
    if system::run(
        "pw-play",
        &[
            "--target",
            sink,
            "--channel-map",
            map.as_str(),
            file.as_str(),
        ],
        TIMEOUT,
    )
    .is_ok()
    {
        return Ok(());
    }
    let device = format!("--device={sink}");
    let channel_map = format!("--channel-map={}", names.join(","));
    system::run(
        "paplay",
        &[device.as_str(), channel_map.as_str(), file.as_str()],
        TIMEOUT,
    )
    .map(|_| ())
    .map_err(|_| CommandError("Install pw-play or paplay to test the speakers.".to_string()))
}

/// Where each speaker sits in the room, in pixels from its centre. The front
/// pair and the centre are at the top, the sides level with the middle, and
/// the rear pair and subwoofer at the bottom, as the speakers sit in a room.
fn speaker_offset(name: &str) -> (f32, f32) {
    match name {
        "front-left" => (-115.0, -85.0),
        "front-right" => (115.0, -85.0),
        "front-center" => (0.0, -130.0),
        "side-left" => (-150.0, 0.0),
        "side-right" => (150.0, 0.0),
        "rear-left" => (-115.0, 85.0),
        "rear-right" => (115.0, 85.0),
        "rear-center" => (0.0, 130.0),
        "lfe" => (60.0, 140.0),
        _ => (0.0, 0.0),
    }
}

/// Play one channel: its freedesktop voice sample through that speaker only.
/// A channel without a sample (the subwoofer) plays the tone instead. Runs on
/// the task pool; `pw-play` routes to one channel, `paplay` is the fallback.
fn play_channel(sink: &str, names: &[String], name: &str) -> Result<(), CommandError> {
    let sample = format!("/usr/share/sounds/freedesktop/stereo/audio-channel-{name}.oga");
    if !std::path::Path::new(&sample).exists() {
        let lit = names.iter().position(|n| n == name).unwrap_or(0);
        return play_tone(sink, names, lit);
    }
    let channel = short_channel(name);
    // WirePlumber would upmix the mono sample onto both ports. `stream.dont-remix`
    // keeps it on the one channel named by `--channel-map`, checked with pw-link.
    if system::run(
        "pw-play",
        &[
            "-P",
            "{ stream.dont-remix = true }",
            "--target",
            sink,
            "--channel-map",
            channel,
            sample.as_str(),
        ],
        TIMEOUT,
    )
    .is_ok()
    {
        return Ok(());
    }
    let device = format!("--device={sink}");
    let channel_map = format!("--channel-map={name}");
    system::run(
        "paplay",
        &[device.as_str(), channel_map.as_str(), sample.as_str()],
        TIMEOUT,
    )
    .map(|_| ())
    .map_err(|_| CommandError("Install pw-play or paplay to test the speakers.".to_string()))
}

/// The speaker test: a room with one round button per speaker, each placed
/// where that speaker sits. The centre shows the channel while it plays.
#[derive(Clone, PartialEq)]
struct SpeakerTest {
    mix: Mix,
    overlays: Overlays,
}

impl Component for SpeakerTest {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let playing = use_state(|| None::<String>);
        let names: Vec<String> = self
            .mix
            .channels
            .iter()
            .map(|(name, _)| name.clone())
            .collect();
        let centre = Dim::SpeakerRoom.px() / 2.0;
        let half = Dim::SpeakerButton.px() / 2.0;
        let playing_name = playing.read().clone();
        let label = match &playing_name {
            Some(name) => channel_label(name),
            None => "Select a speaker".to_string(),
        };

        let mut room = rect().tw(
            &tokens,
            &[
                w(Dim::SpeakerRoom),
                h(Dim::SpeakerRoom),
                rounded(Radius::Pill),
                bg(Paint::Control),
            ],
        );
        room = room.child(
            rect()
                .position(Position::new_absolute().top(centre - 12.0).left(0.0))
                .tw_layout(&[w_full()])
                .child(
                    hstack(S0)
                        .tw_layout(&[w_full(), justify(Align::Center)])
                        .child(CAPTION.label(&tokens, label)),
                ),
        );
        for name in &names {
            let (dx, dy) = speaker_offset(name);
            let sink = self.mix.sink.clone();
            let names = names.clone();
            let channel = name.clone();
            let overlays = self.overlays;
            let mut playing = playing;
            let speaker = icon_button(Icon::Speakers, move |_| {
                if playing.read().is_some() {
                    return;
                }
                playing.set(Some(channel.clone()));
                let (sink, names, channel) = (sink.clone(), names.clone(), channel.clone());
                let mut playing = playing;
                background_then(
                    move || play_channel(&sink, &names, &channel),
                    move |result| {
                        playing.set(None);
                        if let Err(e) = result {
                            overlays.toast(e.0);
                        }
                    },
                );
            })
            .size(Dim::SpeakerButton)
            .filled()
            .active(playing_name.as_deref() == Some(name.as_str()))
            .tooltip(&channel_label(name));
            // The room is a diagram: each speaker sits at its place in it.
            room = room.child(
                rect()
                    .position(
                        Position::new_absolute()
                            .left(centre + dx - half)
                            .top(centre + dy - half),
                    )
                    .child(speaker),
            );
        }
        rect()
            .tw_layout(&[w_full(), h_full(), center()])
            .child(room)
    }
}

/// The test sheet: a room with one button per speaker of the default output.
fn open_test(ctx: Ctx, mix: Mix) {
    let overlays = ctx.overlays;
    ctx.window.open(
        popup("Test speakers", SpeakerTest { mix, overlays })
            .width(Dim::SpeakerPopupWidth)
            .flags(PopupFlags::CANCEL | PopupFlags::GRABBER),
    );
}

/// Optimistic default device: the new default shows at once, and a reload
/// after the write confirms it. A failure puts the old list back and toasts
/// the error.
fn set_default(ctx: Ctx, kind: Kind, id: u32) {
    let previous = ctx.model.read().clone();
    let Some(Ok(loaded)) = previous.clone() else {
        return;
    };
    let devices = match kind {
        Kind::Output => &loaded.outputs,
        Kind::Input => &loaded.inputs,
    };
    if devices
        .iter()
        .any(|device| device.id == id && device.is_default)
    {
        return;
    }
    // With the effects on they own the default device; the choice moves the
    // effects to the other device instead.
    let direction = match kind {
        Kind::Output => engine::Direction::Output,
        Kind::Input => engine::Direction::Input,
    };
    if engine::store::Store::default()
        .state()
        .of(direction)
        .enabled
        && let Some(device) = devices.iter().find(|device| device.id == id)
    {
        set_effects_device(ctx, direction, device.node_name.clone());
        return;
    }
    let mut next = loaded;
    let list = match kind {
        Kind::Output => &mut next.outputs,
        Kind::Input => &mut next.inputs,
    };
    for device in list.iter_mut() {
        device.is_default = device.id == id;
    }
    let level = match kind {
        Kind::Output => &mut next.output,
        Kind::Input => &mut next.input,
    };
    if let Some(level) = level.as_mut() {
        level.id = id;
    }
    let model = ctx.model;
    begin_write();
    system::optimistic_then(
        ctx.model,
        Some(ctx.overlays),
        previous,
        Some(Ok(next)),
        move || {
            let result = system::run("wpctl", &["set-default", &id.to_string()], TIMEOUT);
            end_write();
            result.map(|_| ())
        },
        move || reload(model),
    );
}

/// Optimistic switch for volume above 100%. Turning it off also brings any
/// volume above 100% down to 100%, so the sound matches what the slider allows.
fn set_above_100(ctx: Ctx, above: bool) {
    let previous = ctx.model.read().clone();
    let Some(Ok(loaded)) = previous.clone() else {
        return;
    };
    let mut next = loaded;
    next.above_100 = above;
    let mut lowered: Vec<u32> = Vec::new();
    let mut mix_args: Option<Vec<String>> = None;
    if !above {
        if let Some(mix) = next.mix.as_mut() {
            if mix.channels.iter().any(|(_, volume)| *volume > 1.0) {
                for (_, volume) in mix.channels.iter_mut() {
                    *volume = volume.min(1.0);
                }
                mix_args = Some(mix.args());
                let master = Shape::of(&mix.channels).volume;
                if let Some(level) = next.output.as_mut() {
                    level.volume = master;
                }
            }
        } else if let Some(level) = next.output.as_mut() {
            if level.volume > 1.0 {
                level.volume = 1.0;
                lowered.push(level.id);
            }
        }
        for stream in next.streams.iter_mut() {
            if let Some((volume, _)) = stream.level.as_mut() {
                if *volume > 1.0 {
                    *volume = 1.0;
                    lowered.push(stream.id);
                }
            }
        }
    }
    system::optimistic(
        ctx.model,
        Some(ctx.overlays),
        previous,
        Some(Ok(next)),
        move || -> Result<(), CommandError> {
            let value = if above { "true" } else { "false" };
            system::gsettings_set(
                "org.gnome.desktop.sound",
                "allow-volume-above-100-percent",
                value,
            )?;
            if let Some(args) = mix_args {
                let refs: Vec<&str> = args.iter().map(String::as_str).collect();
                system::run("pactl", &refs, TIMEOUT)?;
            }
            for id in lowered {
                system::run(
                    "wpctl",
                    &["set-volume", "-l", "1.0", &id.to_string(), "1.0"],
                    TIMEOUT,
                )?;
            }
            Ok(())
        },
    );
}

/// A volume slider in percent. With the boost on it runs to 150%, with a
/// tick at 100% where the sound starts to clip.
fn volume_slider(
    title: impl Into<String>,
    percent: f64,
    above: bool,
    on_moved: impl Into<EventHandler<f64>>,
) -> LevelRow {
    let slider = level_row(title, percent, on_moved);
    if above {
        slider
            .max(ABOVE_MAX)
            .mark(100.0)
            .mark_label("100%")
            .danger_from(100.0)
    } else {
        slider
    }
}

/// The percent a slider position means, kept within the slider's top.
fn percent_of(value: f64, above: bool) -> u32 {
    let top = if above { ABOVE_MAX } else { 100.0 };
    value.round().clamp(0.0, top) as u32
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
fn mute_button(volume: f64, muted: bool, on_mute: impl Into<EventHandler<()>>) -> IconButton {
    let label = if muted { "Unmute" } else { "Mute" };
    icon_button(level_icon(volume, muted), on_mute)
        .tooltip(label)
        .size(Dim::Control)
}

/// The devices of one kind as a dropdown, each with its icon before its full
/// name, such as "Headphones - Fosi Audio K5 Pro", as GNOME Settings names
/// them. Choosing an entry makes it the default. The right-click menu makes
/// another device the default, and for the output it plays a test tone
/// through each speaker.
fn device_dropdown_row(kind: Kind, devices: &[Device], test: Option<Mix>, ctx: Ctx) -> Element {
    let default = devices.iter().find(|device| device.is_default);
    let selected = default
        .map(|device| device.id.to_string())
        .unwrap_or_default();
    let options: Vec<(String, String)> = devices
        .iter()
        .map(|device| (device.id.to_string(), device.full_name()))
        .collect();
    let icons: Vec<Icon> = devices.iter().map(|device| device.form.icon()).collect();
    let mut item = row(kind.device_title()).trailing(
        dropdown(options, selected, move |value: String| {
            if let Ok(id) = value.parse::<u32>() {
                set_default(ctx, kind, id);
            }
        })
        .icons(icons)
        .min_width(Dim::Field),
    );
    let mut menu = Vec::new();
    for device in devices.iter().filter(|device| !device.is_default) {
        let id = device.id;
        menu.push(menu_item(
            format!("Set as default: {}", device.full_name()),
            move |_| set_default(ctx, kind, id),
        ));
    }
    if let Some(mix) = test {
        menu.push(menu_item("Test speakers", move |_| {
            open_test(ctx, mix.clone())
        }));
    }
    if !menu.is_empty() {
        item = item.context_menu(menu);
    }
    item.into()
}

/// A group for one kind of device: its dropdown, the volume and, for the
/// output, the balance, fade and subwoofer sliders when the device has those
/// channels. The boost switch sits under the output. As in GNOME Settings, the
/// title line carries a short level meter and, for the output, the speaker
/// test.
fn channel_group(
    kind: Kind,
    devices: &[Device],
    level: Option<Level>,
    mix: Option<&Mix>,
    above: bool,
    ctx: Ctx,
    tokens: Tokens,
) -> Group {
    let group = Group::titled(kind.title());
    if devices.is_empty() {
        return group.row(status(Status::Empty(kind.none_found().into())));
    }
    let test = mix
        .filter(|_| kind == Kind::Output)
        .cloned()
        .map(|test_mix| {
            action_button("Test…", move |_| open_test(ctx, test_mix.clone())).compact()
        });
    let meter = level.as_ref().map(|level| LevelMeter {
        node: level.id,
        source: kind.source(),
        compact: true,
        live: true,
    });
    let group = if meter.is_some() || test.is_some() {
        group.trailing(
            rect()
                .tw(&tokens, &[flex_row(), items(Align::Center), gap(S3)])
                .maybe_child(meter)
                .maybe_child(test),
        )
    } else {
        group
    };
    let mut group = group.row(device_dropdown_row(kind, devices, mix.cloned(), ctx));
    let has_mix = mix.is_some();

    if let Some(level) = level {
        let id = level.id;
        let mute_to = !level.muted;
        group = group.row(
            volume_slider(
                kind.volume_title(),
                level.volume * 100.0,
                above,
                move |value: f64| {
                    let percent = percent_of(value, above);
                    if has_mix {
                        update_mix(ctx, move |shape| shape.volume = f64::from(percent) / 100.0);
                    } else {
                        update_level(ctx.model, kind, |current| {
                            current.volume = f64::from(percent) / 100.0;
                        });
                        set_volume(
                            ctx.queue,
                            ctx.overlays,
                            wpctl_volume_job(id, percent, above),
                        );
                    }
                },
            )
            .control(mute_button(level.volume, level.muted, move |_| {
                update_level(ctx.model, kind, |current| current.muted = mute_to);
                set_mute(ctx, id, mute_to);
            })),
        );
    }
    if let Some(mix) = mix {
        group = group.rows(mix_rows(mix, ctx));
    }
    // The boost only applies to the output, so the switch is shown there alone.
    if kind == Kind::Output {
        group = group.row(switch_row(
            "Allow volume above 100%",
            above,
            move |on: bool| set_above_100(ctx, on),
        ));
    }
    group
}

/// Menu items that move every stream in `ids` to another device. Playing
/// streams move to an output and recording streams to an input.
fn route_items(ctx: Ctx, ids: &[u32], recording: bool) -> Vec<MenuItem> {
    let guard = ctx.model.read();
    let Some(Ok(data)) = guard.as_ref() else {
        return Vec::new();
    };
    let (kind, devices) = if recording {
        (Kind::Input, &data.inputs)
    } else {
        (Kind::Output, &data.outputs)
    };
    if devices.len() < 2 {
        return Vec::new();
    }
    devices
        .iter()
        .map(|device| {
            let ids = ids.to_vec();
            let device_id = device.id;
            menu_item(
                format!(
                    "{} {}",
                    if recording { "Record from" } else { "Play to" },
                    device.full_name()
                ),
                move |_| move_streams(ctx, kind, ids.clone(), device_id),
            )
        })
        .collect()
}

/// Move streams to the device `device`. The device's node name is read from
/// `wpctl inspect`, which `pactl move-*` takes as the target.
fn move_streams(ctx: Ctx, kind: Kind, ids: Vec<u32>, device: u32) {
    begin_write();
    background_then(
        move || -> Result<(), CommandError> {
            let result = (|| {
                let text = system::run("wpctl", &["inspect", &device.to_string()], TIMEOUT)?;
                let name = property(&text, "node.name").to_string();
                if name.is_empty() {
                    return Err(CommandError("The device has no name.".to_string()));
                }
                let verb = match kind {
                    Kind::Output => "move-sink-input",
                    Kind::Input => "move-source-output",
                };
                for id in ids {
                    system::run("pactl", &[verb, &id.to_string(), &name], TIMEOUT)?;
                }
                Ok(())
            })();
            end_write();
            result
        },
        move |result| {
            if let Err(e) = result {
                ctx.overlays.toast(e.0);
            }
            reload(ctx.model);
        },
    );
}

/// One row per application: its icon, name, what its streams are connected to
/// and a volume and mute control. The controls change every stream of the
/// application at once. A thin meter under the slider shows what it plays.
/// A row that is not `live` belongs to an application that has just stopped:
/// it keeps its place and size, dimmed, with no meter running.
fn app_row(streams: &[Stream], above: bool, ctx: Ctx, live: bool) -> Element {
    let first = &streams[0];
    let ids: Vec<u32> = streams.iter().map(|stream| stream.id).collect();
    let recording = streams
        .iter()
        .any(|stream| !stream.recording_from.is_empty());
    match first.level {
        Some((volume, muted)) => {
            let slider_ids = ids.clone();
            let mute_ids = ids.clone();
            let menu_mute_ids = ids.clone();
            let reset_ids = ids.clone();
            let mut menu = vec![
                menu_item(if muted { "Unmute" } else { "Mute" }, move |_| {
                    toggle_stream_mute(ctx, &menu_mute_ids, muted)
                }),
                menu_item("Reset to 100%", move |_| {
                    set_stream_volume(ctx, &reset_ids, 100, above)
                }),
            ];
            if live {
                menu.extend(route_items(ctx, &ids, recording));
            }
            let subtitle = if live {
                summary_of(streams)
            } else {
                "Stopped".to_string()
            };
            volume_slider(
                first.name.clone(),
                volume * 100.0,
                above,
                move |value: f64| {
                    set_stream_volume(ctx, &slider_ids, percent_of(value, above), above);
                },
            )
            .enabled(live)
            .subtitle(subtitle)
            .leading(app_icon(first.icon.clone(), IconSize::App))
            .footer(LevelMeter {
                node: first.id,
                source: Source::Stream,
                compact: false,
                live,
            })
            .control(
                mute_button(volume, muted, move |_| {
                    toggle_stream_mute(ctx, &mute_ids, muted)
                })
                .enabled(live),
            )
            .context_menu(if live { menu } else { Vec::new() })
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
    let mut groups: Vec<Vec<Stream>> = Vec::new();
    for stream in streams {
        match groups
            .iter_mut()
            .find(|group| application_key(&group[0]) == application_key(stream))
        {
            Some(group) => group.push(stream.clone()),
            None => groups.push(vec![stream.clone()]),
        }
    }
    groups
}

/// What the streams of one application have in common.
fn application_key(stream: &Stream) -> String {
    stream.app.clone().unwrap_or_else(|| stream.name.clone())
}

/// How many applications play and record, such as "2 playing, 1 recording".
fn activity_summary(streams: &[Stream]) -> String {
    let groups = by_application(streams);
    let playing = groups
        .iter()
        .filter(|group| group.iter().any(|stream| !stream.playing_to.is_empty()))
        .count();
    let recording = groups
        .iter()
        .filter(|group| group.iter().any(|stream| !stream.recording_from.is_empty()))
        .count();
    match (playing, recording) {
        (0, 0) => "Nothing playing".to_string(),
        (playing, 0) => format!("{playing} playing"),
        (0, recording) => format!("{recording} recording"),
        (playing, recording) => format!("{playing} playing, {recording} recording"),
    }
}

/// How many mounted pages need the detailed model: each stream's volume,
/// icon and application. The Sound page itself needs only the summary, which
/// `wpctl status` alone gives, so it does not run a command for each stream.
static DETAILED: AtomicUsize = AtomicUsize::new(0);

fn detailed() -> bool {
    DETAILED.load(Ordering::SeqCst) > 0
}

/// Poll the sound state while the page is open, and load it once now.
/// `detail` is true for a page that shows the volume of each stream.
fn use_sound_model(detail: bool) -> ModelState {
    use_hook(|| {
        if detail {
            DETAILED.fetch_add(1, Ordering::SeqCst);
        }
    });
    use_drop(move || {
        if detail {
            DETAILED.fetch_sub(1, Ordering::SeqCst);
        }
    });
    let key = if detail { "sound-streams" } else { "sound" };
    let model = use_hook(move || system::cached_persistent::<Model, CommandError>(key));
    use_hook(move || {
        reload(model);
        let focus = system::window_focus();
        spawn(async move {
            loop {
                timer(POLL).await;
                if *focus.peek() {
                    reload(model);
                }
            }
        });
    });
    model
}

fn use_ctx(detail: bool) -> Ctx {
    let model = use_sound_model(detail);
    let overlays = use_overlays();
    let window = use_window();
    let queue = VolumeQueue {
        pending: use_state(Vec::new),
        busy: use_state(|| false),
    };
    Ctx {
        model,
        overlays,
        window,
        queue,
    }
}

/// The quick switch for the output effects: turn them on or off in the
/// background, then show the preset that is in use, or "Off".
fn set_output_effects(mut shown: State<Option<String>>, overlays: Overlays, on: bool) {
    use engine::store::Store;
    let direction = engine::Direction::Output;
    let previous = shown.peek().clone();
    // Shown at once from what the effects page last read; the worker reports
    // the preset really in use when it has made the change.
    shown.set(on.then(|| effects::active_preset_cached(direction)));
    background_then(
        move || {
            let store = Store::default();
            store
                .update_state(|state| state.of_mut(direction).enabled = on)
                .map_err(|e| e.0)?;
            engine::service::apply(&store, direction)?;
            Ok(effects::summary(direction))
        },
        move |result: Result<Option<String>, String>| match result {
            Ok(summary) => shown.set(summary),
            Err(message) => {
                overlays.toast(message);
                shown.set(previous);
            }
        },
    );
}

#[derive(PartialEq)]
pub struct SoundPage;

impl Component for SoundPage {
    fn render(&self) -> impl IntoElement {
        let ctx = use_ctx(false);
        let tokens = use_tokens();
        // The applications and the effects are subpages, so this page stays
        // short and runs no meter for them. Hooks come before the early
        // returns, so every path runs the same ones.
        let router = use_router();
        let effects_on = use_state(effects::summary_cached);
        use_hook(move || {
            let mut effects_on = effects_on;
            background_then(
                || effects::summary(engine::Direction::Output),
                move |summary| effects_on.set(summary),
            );
        });
        // Which speakers the output has decides which of Balance, Fade and
        // Subwoofer show, so before the first read lands there is no layout to
        // draw. The page is hidden, then fades in at its final layout.
        let data = match ctx.model.read().clone() {
            None => return revealed(page_column().child(PageHead::new()), true),
            Some(Ok(data)) => data,
            Some(Err(e)) => {
                return revealed(
                    page_column().child(PageHead::new()).child(
                        status(Status::Error(e.0)).action("Try again", move |_| reload(ctx.model)),
                    ),
                    false,
                );
            }
        };

        let mut page = page_column().child(PageHead::new());

        page = page
            .child(channel_group(
                Kind::Output,
                &data.outputs,
                data.output.clone(),
                data.mix.as_ref(),
                data.above_100,
                ctx,
                tokens,
            ))
            .child(channel_group(
                Kind::Input,
                &data.inputs,
                data.input.clone(),
                None,
                false,
                ctx,
                tokens,
            ));

        let overlays = ctx.overlays;
        let preset = effects_on.read().clone();
        page = page.child(
            Group::new(None::<String>)
                .row(
                    row("Application volumes")
                        .value(activity_summary(&data.streams))
                        .navigates()
                        .on_press(move |_| router.open(routes::sound_applications())),
                )
                .row(
                    row("Effects")
                        .value(preset.clone().unwrap_or_else(|| "Off".to_string()))
                        .navigates()
                        .on_press(move |_| effects::open(ctx.window)),
                )
                .row(switch_row(
                    "Output effects",
                    preset.is_some(),
                    move |on: bool| {
                        set_output_effects(effects_on, overlays, on);
                    },
                )),
        );
        revealed(page, false)
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
        let ctx = use_ctx(true);

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
            .child(Group::titled("Sound").rows(
                streams.iter().map(|stream| {
                    app_row(std::slice::from_ref(*stream), data.above_100, ctx, true)
                }),
            ))
            .into()
    }
}

/// Start loading this page's data before it is opened.
pub fn prefetch() {
    system::prefetch_persistent("sound", load);
}
