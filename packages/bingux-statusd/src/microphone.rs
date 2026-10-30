use crate::pactl::{MAX_SNAPSHOT_BYTES, Subscription, event_facility, run_command};
use serde::Serialize;
use serde_json::Value;
use std::{collections::BTreeMap, thread, time::Duration};

const MAX_CAPTURES: usize = 32;
const MAX_FIELD_BYTES: usize = 128;
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

fn watch_once(publish: &impl Fn(CaptureSnapshot) -> bool) -> Result<(), String> {
    let mut subscription = Subscription::open()?;
    let initial = snapshot()?;
    if !publish(initial.clone()) {
        return Err("metrics receiver stopped".to_owned());
    }
    let mut previous = initial;
    loop {
        subscription.wait_for(affects_capture, EVENT_DEBOUNCE)?;
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
        event_facility(event),
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
