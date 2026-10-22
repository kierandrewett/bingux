//! Sound: output and input devices with volume and mute through `wpctl`, and
//! the applications playing or recording audio, read from `wpctl status`.
//! Everything is read in the background and polled every five seconds.

use std::time::Duration;

use freya::prelude::*;

use bingux_ui::scale::*;

use crate::apps;
use crate::system::{self, CommandError, background, background_then};
use crate::ui::{Group, Status, dropdown, page_column, row, skeleton, slider, status, switch_row};

const TIMEOUT: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_secs(5);
const TREE: &[char] = &['│', '├', '└', '─'];

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

#[derive(Clone, PartialEq)]
struct Device {
    id: u32,
    name: String,
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
    playing_to: Vec<String>,
    recording_from: Vec<String>,
    /// The stream's own volume, when WirePlumber reports one.
    level: Option<(f64, bool)>,
    /// The icon name to draw for the application that owns the stream.
    icon: String,
    /// The desktop id of that application, when it is installed.
    app: Option<String>,
}

impl Stream {
    fn summary(&self) -> String {
        let mut parts = Vec::new();
        if !self.playing_to.is_empty() {
            parts.push(format!("Plays to {}", self.playing_to.join(", ")));
        }
        if !self.recording_from.is_empty() {
            parts.push(format!("Records from {}", self.recording_from.join(", ")));
        }
        if parts.is_empty() {
            "Not connected".to_string()
        } else {
            parts.join("; ")
        }
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
                        level: None,
                        icon: String::new(),
                        app: None,
                    });
                    current = Some(model.streams.len() - 1);
                } else if let Some(stream) = current.and_then(|i| model.streams.get_mut(i)) {
                    if let Some(index) = rest.find('>') {
                        push_unique(&mut stream.playing_to, link_device(&rest[index + 1..]));
                    } else if let Some(index) = rest.find('<') {
                        push_unique(&mut stream.recording_from, link_device(&rest[index + 1..]));
                    }
                }
            }
            None => {}
        }
    }
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
        .ok_or_else(|| CommandError(format!("wpctl printed an unreadable volume: {text}")))?;
    Ok(Some(Level {
        id: default.id,
        volume,
        muted,
    }))
}

fn load() -> Result<Model, CommandError> {
    let text = system::run("wpctl", &["status"], TIMEOUT)?;
    let mut model = parse_status(&text);
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

/// Run a `wpctl` write on a background thread, then set or clear the error
/// and reload the model so the page shows what the backend now reports.
fn apply(
    args: Vec<String>,
    model: State<Option<Result<Model, CommandError>>>,
    mut error: State<Option<String>>,
) {
    background_then(
        move || {
            let refs: Vec<&str> = args.iter().map(String::as_str).collect();
            system::run("wpctl", &refs, TIMEOUT)
        },
        move |result| {
            match result {
                Ok(_) => error.set(None),
                Err(e) => error.set(Some(e.0)),
            }
            background(model, load);
        },
    );
}

fn update_level(
    mut model: State<Option<Result<Model, CommandError>>>,
    kind: Kind,
    change: impl FnOnce(&mut Level),
) {
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
fn set_volume(queue: VolumeQueue, error: State<Option<String>>, id: u32, percent: u32) {
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
        push_volume(queue, error);
    }
}

fn push_volume(queue: VolumeQueue, mut error: State<Option<String>>) {
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
            match result {
                Ok(_) => error.set(None),
                Err(e) => error.set(Some(e.0)),
            }
            push_volume(queue, error);
        },
    );
}

/// Optimistic slider position for an application stream until the next poll.
fn update_stream(mut model: State<Option<Result<Model, CommandError>>>, id: u32, volume: f64) {
    let mut guard = model.write();
    let Some(Ok(loaded)) = guard.as_mut() else {
        return;
    };
    if let Some(stream) = loaded.streams.iter_mut().find(|stream| stream.id == id) {
        if let Some(level) = stream.level.as_mut() {
            level.0 = volume;
        }
    }
}

/// A volume slider for the shared control column: the track leaves room for
/// the percentage, so the slider and its label end at the row's right edge.
fn volume_slider(value: f64, on_moved: impl Into<EventHandler<f64>>) -> Element {
    slider(value, on_moved)
        .width(FIELD_WIDTH - S2 - BOX_LG)
        .show_value()
        .into()
}

fn channel_group(
    kind: Kind,
    devices: &[Device],
    level: Option<Level>,
    model: State<Option<Result<Model, CommandError>>>,
    error: State<Option<String>>,
    queue: VolumeQueue,
) -> Group {
    let group = Group::titled(kind.title());
    if devices.is_empty() {
        return group.row(status(Status::Empty(kind.none_found().into())));
    }

    let selected = devices
        .iter()
        .find(|device| device.is_default)
        .map(|device| device.id.to_string())
        .unwrap_or_default();
    // One device is a fact. Several share a dropdown at the slider width, so
    // every control on the page has the same width and right edge.
    let device = match devices {
        [only] => row("Device").value(only.name.clone()),
        _ => {
            let options: Vec<(String, String)> = devices
                .iter()
                .map(|device| (device.id.to_string(), device.name.clone()))
                .collect();
            row("Device").trailing(
                dropdown(options, selected, move |key: String| {
                    if let Ok(id) = key.parse::<u32>() {
                        apply(
                            vec!["set-default".to_string(), id.to_string()],
                            model,
                            error,
                        );
                    }
                })
                .min_width(FIELD_WIDTH),
            )
        }
    };
    let mut group = group.row(device);

    if let Some(level) = level {
        let id = level.id;
        group = group
            .row(row("Volume").trailing(volume_slider(
                (level.volume * 100.0).min(100.0),
                move |value: f64| {
                    let percent = value.round().clamp(0.0, 100.0) as u32;
                    update_level(model, kind, |level| {
                        level.volume = f64::from(percent) / 100.0;
                    });
                    set_volume(queue, error, id, percent);
                },
            )))
            .row(switch_row("Mute", level.muted, move |next: bool| {
                update_level(model, kind, |level| level.muted = next);
                apply(
                    vec![
                        "set-mute".to_string(),
                        id.to_string(),
                        if next { "1" } else { "0" }.to_string(),
                    ],
                    model,
                    error,
                );
            }));
    }
    group
}

/// One application stream: its icon, name, what it is connected to and a
/// volume slider when WirePlumber reports a volume.
fn stream_row(
    stream: &Stream,
    model: State<Option<Result<Model, CommandError>>>,
    error: State<Option<String>>,
    queue: VolumeQueue,
) -> Element {
    let id = stream.id;
    let base = row(stream.name.clone())
        .leading_app(stream.icon.clone())
        .subtitle(stream.summary());
    match stream.level {
        Some((volume, _)) => Element::from(base.trailing(volume_slider(
            (volume * 100.0).min(100.0),
            move |value: f64| {
                let percent = value.round().clamp(0.0, 100.0) as u32;
                update_stream(model, id, f64::from(percent) / 100.0);
                set_volume(queue, error, id, percent);
            },
        ))),
        None => Element::from(base.value("No volume control")),
    }
}

#[derive(PartialEq)]
pub struct SoundPage;

impl Component for SoundPage {
    fn render(&self) -> impl IntoElement {
        let model = system::cached("sound");
        let error = use_state::<Option<String>>(|| None);
        let queue = VolumeQueue {
            pending: use_state(Vec::new),
            busy: use_state(|| false),
        };

        use_hook(move || {
            background(model, load);
            spawn(async move {
                loop {
                    timer(POLL).await;
                    background(model, load);
                }
            });
        });

        let Some(loaded) = model.read().clone() else {
            return page_column().child(skeleton());
        };
        let data = match loaded {
            Ok(data) => data,
            Err(e) => {
                return page_column().child(
                    status(Status::Error(e.0))
                        .action("Try again", move |_| background(model, load)),
                );
            }
        };

        let mut page = page_column();
        if let Some(message) = error.read().clone() {
            page = page.child(status(Status::Error(message)));
        }

        page = page
            .child(channel_group(
                Kind::Output,
                &data.outputs,
                data.output.clone(),
                model,
                error,
                queue,
            ))
            .child(channel_group(
                Kind::Input,
                &data.inputs,
                data.input.clone(),
                model,
                error,
                queue,
            ));

        let applications = Group::titled("Applications");
        if data.streams.is_empty() {
            page = page.child(applications.row(status(Status::Empty(
                "No applications are playing or recording audio.".into(),
            ))));
        } else {
            page = page.child(
                applications.rows(
                    data.streams
                        .iter()
                        .map(|stream| stream_row(stream, model, error, queue)),
                ),
            );
        }
        page
    }
}

/// The sound of one application: a volume slider for each of its streams,
/// shown on the application's page. It draws nothing while the application
/// is not playing or recording.
#[derive(PartialEq)]
pub struct AppSound {
    pub app_id: String,
}

impl Component for AppSound {
    fn render(&self) -> impl IntoElement {
        let model = system::cached("sound");
        let error = use_state::<Option<String>>(|| None);
        let queue = VolumeQueue {
            pending: use_state(Vec::new),
            busy: use_state(|| false),
        };
        use_hook(move || {
            background(model, load);
            spawn(async move {
                loop {
                    timer(POLL).await;
                    background(model, load);
                }
            });
        });

        let Some(Ok(data)) = model.read().clone() else {
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
        let mut group = page_column();
        if let Some(message) = error.read().clone() {
            group = group.child(status(Status::Error(message)));
        }
        group
            .child(
                Group::titled("Sound").rows(
                    streams
                        .iter()
                        .map(|stream| stream_row(stream, model, error, queue)),
                ),
            )
            .into()
    }
}

/// Start loading this page's data before it is opened.
pub fn prefetch() {
    system::prefetch("sound", load);
}
