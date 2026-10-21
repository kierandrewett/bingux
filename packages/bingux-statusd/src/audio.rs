//! Volume and microphone feedback for the shell OSD.
//!
//! Gnoblin runs the audio keys as plain commands and raises no OSD event for
//! them, so the shell has nothing to draw. This watcher follows the default
//! output and input through `pactl subscribe`. It reports a level change or a
//! mute toggle whichever tool made it: a key binding, the Sound settings page
//! or an application. It reports nothing when the default device itself
//! changes, because a different device has a different level.

use crate::pactl::{MAX_SNAPSHOT_BYTES, Subscription, event_facility, run_command};
use serde_json::Value;
use std::{thread, time::Duration};

const RECONNECT_DELAY: Duration = Duration::from_secs(2);
const EVENT_DEBOUNCE: Duration = Duration::from_millis(50);
/// PulseAudio's `PA_VOLUME_NORM`: the raw value of 100 percent.
const VOLUME_NORM: f64 = 65_536.0;
/// A step below one raw unit in a hundred is rounding, not a user change.
const LEVEL_EPSILON: f64 = 0.004;

/// One OSD request: the icon, the device name, and the level against its maximum.
#[derive(Clone, Debug, PartialEq)]
pub struct Feedback {
    pub icon: String,
    pub label: String,
    pub level: f64,
    pub max_level: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Device {
    pub name: String,
    pub description: String,
    /// 1.0 is 100 percent. It can exceed 1.0 on an amplified device.
    pub volume: f64,
    pub muted: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    pub output: Option<Device>,
    pub input: Option<Device>,
}

/// Read the default output and input from the three bounded `pactl` snapshots.
pub fn parse_state(
    info_json: &str,
    sinks_json: &str,
    sources_json: &str,
) -> Result<State, &'static str> {
    if [info_json, sinks_json, sources_json]
        .iter()
        .any(|text| text.len() > MAX_SNAPSHOT_BYTES)
    {
        return Err("pactl snapshot exceeds the input limit");
    }
    let info: Value = serde_json::from_str(info_json).map_err(|_| "invalid server info JSON")?;
    let sinks: Vec<Value> = serde_json::from_str(sinks_json).map_err(|_| "invalid sinks JSON")?;
    let sources: Vec<Value> =
        serde_json::from_str(sources_json).map_err(|_| "invalid sources JSON")?;
    Ok(State {
        output: default_device(&info, "default_sink_name", &sinks),
        input: default_device(&info, "default_source_name", &sources),
    })
}

fn default_device(info: &Value, key: &str, devices: &[Value]) -> Option<Device> {
    let name = info.get(key)?.as_str()?;
    // A default source that is a sink monitor records playback, not a microphone.
    if name.is_empty() || name.ends_with(".monitor") {
        return None;
    }
    let device = devices
        .iter()
        .find(|device| device.get("name").and_then(Value::as_str) == Some(name))?;
    let volume = device
        .get("volume")?
        .as_object()?
        .values()
        .filter_map(|channel| channel.get("value")?.as_f64())
        .fold(None, |highest: Option<f64>, value| {
            Some(highest.map_or(value, |current| current.max(value)))
        })?
        / VOLUME_NORM;
    let description = device
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or(name);
    Some(Device {
        name: name.to_owned(),
        description: description.to_owned(),
        volume,
        muted: device.get("mute").and_then(Value::as_bool).unwrap_or(false),
    })
}

/// The feedback for each device whose level or mute state moved between two
/// snapshots of the same default device.
pub fn changes(previous: &State, current: &State) -> Vec<Feedback> {
    let mut feedback = Vec::new();
    if let Some(device) = moved(previous.output.as_ref(), current.output.as_ref()) {
        feedback.push(output_feedback(device));
    }
    if let Some(device) = moved(previous.input.as_ref(), current.input.as_ref()) {
        feedback.push(input_feedback(device));
    }
    feedback
}

fn moved<'a>(previous: Option<&Device>, current: Option<&'a Device>) -> Option<&'a Device> {
    let (previous, current) = (previous?, current?);
    (previous.name == current.name
        && (previous.muted != current.muted
            || (previous.volume - current.volume).abs() > LEVEL_EPSILON))
        .then_some(current)
}

fn output_feedback(device: &Device) -> Feedback {
    let icon = if device.muted || device.volume <= 0.0 {
        "audio-volume-muted-symbolic"
    } else if device.volume > 1.0 {
        "audio-volume-overamplified-symbolic"
    } else if device.volume < 1.0 / 3.0 {
        "audio-volume-low-symbolic"
    } else if device.volume < 2.0 / 3.0 {
        "audio-volume-medium-symbolic"
    } else {
        "audio-volume-high-symbolic"
    };
    feedback(device, icon)
}

fn input_feedback(device: &Device) -> Feedback {
    let icon = if device.muted || device.volume <= 0.0 {
        "microphone-sensitivity-muted-symbolic"
    } else if device.volume < 1.0 / 3.0 {
        "microphone-sensitivity-low-symbolic"
    } else if device.volume < 2.0 / 3.0 {
        "microphone-sensitivity-medium-symbolic"
    } else {
        "microphone-sensitivity-high-symbolic"
    };
    feedback(device, icon)
}

fn feedback(device: &Device, icon: &str) -> Feedback {
    Feedback {
        icon: icon.to_owned(),
        label: device.description.clone(),
        // A muted device shows an empty bar, as GNOME does.
        level: if device.muted { 0.0 } else { device.volume },
        max_level: 1.0,
    }
}

/// Start the optional default-device watcher. `publish` returns false once the
/// receiver has stopped.
pub fn start(publish: impl Fn(Feedback) -> bool + Send + 'static) {
    thread::spawn(move || run(publish));
}

fn run(publish: impl Fn(Feedback) -> bool) {
    loop {
        match watch_once(&publish) {
            Ok(()) => return,
            Err(error) => eprintln!("[bingux-statusd] audio feedback unavailable: {error}"),
        }
        thread::sleep(RECONNECT_DELAY);
    }
}

fn watch_once(publish: &impl Fn(Feedback) -> bool) -> Result<(), String> {
    // Subscribe before the first snapshot so no change falls between the two.
    let mut subscription = Subscription::open()?;
    let mut previous = snapshot()?;
    loop {
        subscription.wait_for(affects_levels, EVENT_DEBOUNCE)?;
        let current = snapshot()?;
        for item in changes(&previous, &current) {
            if !publish(item) {
                return Ok(());
            }
        }
        previous = current;
    }
}

fn affects_levels(event: &str) -> bool {
    matches!(event_facility(event), Some("sink" | "source" | "server"))
}

fn snapshot() -> Result<State, String> {
    let info = run_command(&["-f", "json", "info"])?;
    let sinks = run_command(&["-f", "json", "list", "sinks"])?;
    let sources = run_command(&["-f", "json", "list", "sources"])?;
    parse_state(&info, &sinks, &sources).map_err(str::to_owned)
}
