use crate::Event;
use bingux_statusd::{DesktopState, InputSource, OsdRequest, PrivacyState};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
    sync::mpsc::SyncSender,
    thread,
    time::Duration,
};

const INITIAL_RECONNECT_DELAY: Duration = Duration::from_millis(250);
const MAX_RECONNECT_DELAY: Duration = Duration::from_secs(5);
const MAX_BRIDGE_LINE_BYTES: usize = 64 * 1024;
const MAX_INPUT_SOURCES: usize = 32;
const MAX_INPUT_SOURCE_FIELD_BYTES: usize = 128;
const MAX_INPUT_SOURCE_TOTAL_BYTES: usize = 4 * 1024;
const API_MAJOR: u64 = 1;
const INPUT_API_MINOR: u64 = 6;
const PRIVACY_API_MINOR: u64 = 17;
const EVENTS_API_MINOR: u64 = 9;
const OSD_API_MINOR: u64 = 27;
const MONITORS_API_MINOR: u64 = 1;

const PRIVACY_EVENT: &str = "gnoblin.privacy.changed";
const OSD_EVENT: &str = "gnoblin.osd.requested";

/// Start the compositor-socket subscriber for Gnoblin state and OSD requests.
pub fn start_state_subscriber(sender: SyncSender<Event>) {
    thread::spawn(move || run_state_subscriber(sender));
}

fn run_state_subscriber(sender: SyncSender<Event>) {
    let mut reconnect_delay = INITIAL_RECONNECT_DELAY;

    loop {
        match subscribe_to_gnoblin(&sender, &mut reconnect_delay) {
            Ok(()) => return,
            Err(error) => eprintln!("[bingux-statusd] Gnoblin state unavailable: {error}"),
        }

        if sender
            .send(Event::DesktopState(DesktopState::default()))
            .is_err()
        {
            return;
        }

        thread::sleep(reconnect_delay);
        reconnect_delay = std::cmp::min(reconnect_delay * 2, MAX_RECONNECT_DELAY);
    }
}

fn subscribe_to_gnoblin(
    sender: &SyncSender<Event>,
    reconnect_delay: &mut Duration,
) -> Result<(), String> {
    let path = compositor_socket_path()?;
    let stream = UnixStream::connect(&path)
        .map_err(|error| format!("could not connect to {}: {error}", path.display()))?;
    let mut writer = stream
        .try_clone()
        .map_err(|error| format!("could not prepare compositor socket: {error}"))?;
    let mut reader = BufReader::new(stream);

    let hello = read_json_line(&mut reader)?
        .ok_or_else(|| "Gnoblin closed the socket before sending its greeting".to_owned())?;
    let api_minor = validate_hello(&hello)?;
    let methods = string_set(hello.get("methods"));
    let events = string_set(hello.get("events"));

    let subscribed_events = supported_events(api_minor, &events);
    let mut initial_subscriptions = BTreeSet::new();
    let expected_events: BTreeSet<String> = subscribed_events
        .iter()
        .map(|event| (*event).to_owned())
        .collect();
    if api_minor >= EVENTS_API_MINOR && !subscribed_events.is_empty() {
        send_json(
            &mut writer,
            &json!({
                "op": "events",
                "api_version": {"major": API_MAJOR, "minor": api_minor.min(OSD_API_MINOR)},
                "events": subscribed_events,
            }),
        )?;
        initial_subscriptions.insert("events");
    }

    if api_minor >= MONITORS_API_MINOR {
        send_json(
            &mut writer,
            &json!({
                "op": "monitors",
                "api_version": {"major": API_MAJOR, "minor": MONITORS_API_MINOR},
            }),
        )?;
        initial_subscriptions.insert("monitors");
    }

    let mut requested_reads = BTreeSet::new();
    if api_minor >= INPUT_API_MINOR && methods.contains("input.sources") {
        send_api_read(
            &mut writer,
            "input-sources",
            INPUT_API_MINOR,
            "input.sources",
        )?;
        requested_reads.insert("input-sources");
    }
    if api_minor >= INPUT_API_MINOR && methods.contains("input.current_source") {
        send_api_read(
            &mut writer,
            "input-current-source",
            INPUT_API_MINOR,
            "input.current_source",
        )?;
        requested_reads.insert("input-current-source");
    }
    if api_minor >= PRIVACY_API_MINOR && methods.contains("privacy.state") {
        send_api_read(
            &mut writer,
            "privacy-state",
            PRIVACY_API_MINOR,
            "privacy.state",
        )?;
        requested_reads.insert("privacy-state");
    }

    let input_state_available = api_minor >= INPUT_API_MINOR
        && methods.contains("input.sources")
        && methods.contains("input.current_source");
    let can_reset_backoff = input_state_available;
    let mut state = StateSnapshot {
        available: input_state_available,
        ..StateSnapshot::default()
    };
    if requested_reads.is_empty() {
        publish_state(&state, sender)?;
    }
    let mut monitors = BTreeMap::new();

    loop {
        let Some(record) = read_json_line(&mut reader)? else {
            return Err("Gnoblin closed the compositor socket".to_owned());
        };
        let event = record.get("event").and_then(Value::as_str).unwrap_or("");

        if event == "error" {
            let id = record.get("id").and_then(Value::as_str).unwrap_or("");
            let message = record
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("Gnoblin rejected a compositor socket request");
            if requested_reads.contains(id) || id.is_empty() {
                return Err(message.to_owned());
            }
            continue;
        }

        if event == "reply" {
            let id = record.get("id").and_then(Value::as_str).unwrap_or("");
            let result = record
                .get("result")
                .ok_or_else(|| format!("Gnoblin returned no result for {id}"))?;
            let revision = json_revision(result.get("revision"));
            match id {
                "input-sources" => {
                    state.replace_sources(result, revision)?;
                    requested_reads.remove(id);
                    publish_state(&state, sender)?;
                }
                "input-current-source" => {
                    state.replace_current_source(result, revision)?;
                    requested_reads.remove(id);
                    publish_state(&state, sender)?;
                }
                "privacy-state" => {
                    state.replace_privacy(result, revision)?;
                    requested_reads.remove(id);
                    publish_state(&state, sender)?;
                }
                _ => {}
            }
            if can_reset_backoff && requested_reads.is_empty() && initial_subscriptions.is_empty() {
                *reconnect_delay = INITIAL_RECONNECT_DELAY;
            }
            continue;
        }

        match event {
            "monitors" => {
                monitors = parse_monitor_snapshot(&record);
                initial_subscriptions.remove("monitors");
            }
            "gnoblin.monitor.added" | "gnoblin.monitor.changed" => {
                if let Some(monitor) = record.get("monitor").and_then(parse_monitor) {
                    monitors.insert(monitor.id.clone(), monitor);
                }
            }
            "gnoblin.monitor.removed" => {
                if let Some(id) = record.get("monitor_id").and_then(Value::as_str) {
                    monitors.remove(id);
                }
            }
            "gnoblin.input.sources-changed" => {
                let revision = json_revision(record.get("revision"));
                state.replace_sources(&record, revision)?;
                publish_state(&state, sender)?;
            }
            "gnoblin.input.source-changed" => {
                let revision = json_revision(record.get("revision"));
                state.replace_current_source(&record, revision)?;
                publish_state(&state, sender)?;
            }
            PRIVACY_EVENT => {
                let revision = json_revision(record.get("revision"));
                state.replace_privacy(
                    record
                        .get("state")
                        .ok_or_else(|| "Gnoblin sent an invalid privacy event".to_owned())?,
                    revision,
                )?;
                publish_state(&state, sender)?;
            }
            OSD_EVENT => {
                if let Some(request) = osd_request_from_event(&record, &monitors) {
                    sender
                        .send(Event::OsdRequest(request))
                        .map_err(|_| "OSD receiver stopped".to_owned())?;
                }
            }
            "subscribed" => {
                if initial_subscriptions.contains("events") {
                    let accepted = string_set(record.get("events"));
                    if !expected_events.is_subset(&accepted) {
                        return Err(
                            "Gnoblin accepted only part of the statusd event subscription"
                                .to_owned(),
                        );
                    }
                    initial_subscriptions.remove("events");
                }
            }
            _ => {}
        }
        if can_reset_backoff && requested_reads.is_empty() && initial_subscriptions.is_empty() {
            *reconnect_delay = INITIAL_RECONNECT_DELAY;
        }
    }
}

fn compositor_socket_path() -> Result<PathBuf, String> {
    if let Some(path) = env::var_os("GNOBLIN_COMPOSITOR_SOCKET") {
        if !path.is_empty() {
            return Ok(PathBuf::from(path));
        }
    }
    let runtime_dir =
        env::var_os("XDG_RUNTIME_DIR").ok_or_else(|| "XDG_RUNTIME_DIR is not set".to_owned())?;
    Ok(PathBuf::from(runtime_dir)
        .join("gnoblin")
        .join("compositor-v1.sock"))
}

fn validate_hello(hello: &Value) -> Result<u64, String> {
    if hello.get("event").and_then(Value::as_str) != Some("hello") {
        return Err("Gnoblin sent an invalid compositor socket greeting".to_owned());
    }
    if json_revision(hello.get("api_major")) != Some(API_MAJOR) {
        return Err("Gnoblin compositor socket API major version is not supported".to_owned());
    }
    json_revision(hello.get("api_minor"))
        .ok_or_else(|| "Gnoblin greeting has no valid API minor version".to_owned())
}

fn string_set(value: Option<&Value>) -> BTreeSet<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

fn supported_events(api_minor: u64, available: &BTreeSet<String>) -> Vec<&'static str> {
    let mut events = Vec::new();
    if api_minor >= PRIVACY_API_MINOR && available.contains(PRIVACY_EVENT) {
        events.push(PRIVACY_EVENT);
    }
    if api_minor >= OSD_API_MINOR && available.contains(OSD_EVENT) {
        events.push(OSD_EVENT);
    }
    events
}

fn send_api_read(
    writer: &mut UnixStream,
    id: &str,
    api_minor: u64,
    method: &str,
) -> Result<(), String> {
    send_json(
        writer,
        &json!({
            "op": "api",
            "id": id,
            "api_version": {"major": API_MAJOR, "minor": api_minor},
            "method": method,
            "arguments": {},
        }),
    )
}

fn send_json(writer: &mut UnixStream, value: &Value) -> Result<(), String> {
    let mut line = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    line.push(b'\n');
    writer
        .write_all(&line)
        .map_err(|error| format!("could not write to Gnoblin compositor socket: {error}"))
}

fn read_json_line(reader: &mut impl BufRead) -> Result<Option<Value>, String> {
    let mut line = Vec::new();
    loop {
        let (length, has_newline) = {
            let available = reader
                .fill_buf()
                .map_err(|error| format!("could not read Gnoblin compositor socket: {error}"))?;
            if available.is_empty() {
                return if line.is_empty() {
                    Ok(None)
                } else {
                    Err("Gnoblin closed the compositor socket mid-record".to_owned())
                };
            }
            let has_newline = available.contains(&b'\n');
            let length = available
                .iter()
                .position(|byte| *byte == b'\n')
                .map_or(available.len(), |index| index + 1);
            if line.len() + length > MAX_BRIDGE_LINE_BYTES {
                return Err("Gnoblin compositor socket record exceeds 64 KiB".to_owned());
            }
            line.extend_from_slice(&available[..length]);
            (length, has_newline)
        };
        reader.consume(length);
        if has_newline {
            break;
        }
    }
    line.pop();
    serde_json::from_slice(&line)
        .map(Some)
        .map_err(|error| format!("Gnoblin sent invalid compositor socket JSON: {error}"))
}

#[derive(Default)]
struct StateSnapshot {
    available: bool,
    input_sources: Vec<InputSource>,
    current_input_source: Option<InputSource>,
    privacy: PrivacyState,
    sources_revision: u64,
    current_revision: u64,
    privacy_revision: u64,
}

impl StateSnapshot {
    fn replace_sources(&mut self, value: &Value, revision: Option<u64>) -> Result<(), String> {
        let revision =
            revision.ok_or_else(|| "Gnoblin sent input sources without a revision".to_owned())?;
        if revision < self.sources_revision {
            return Ok(());
        }
        let records = value
            .get("sources")
            .and_then(Value::as_array)
            .ok_or_else(|| "Gnoblin sent an invalid input source list".to_owned())?;
        if records.len() > MAX_INPUT_SOURCES {
            return Err("Gnoblin returned too many input sources".to_owned());
        }
        let mut sources = Vec::with_capacity(records.len());
        let mut total_bytes = 0usize;
        let mut current = None;
        for record in records {
            let source = parse_input_source(record)
                .ok_or_else(|| "Gnoblin returned an invalid input source".to_owned())?;
            total_bytes = total_bytes
                .checked_add(source.source_type.len())
                .and_then(|total| total.checked_add(source.id.len()))
                .and_then(|total| total.checked_add(source.short_name.len()))
                .and_then(|total| total.checked_add(source.display_name.len()))
                .ok_or_else(|| "Gnoblin input source size overflowed".to_owned())?;
            if record.get("current").and_then(Value::as_bool) == Some(true) {
                current = Some(source.clone());
            }
            sources.push(source);
        }
        if total_bytes > MAX_INPUT_SOURCE_TOTAL_BYTES {
            return Err("Gnoblin returned too much input source data".to_owned());
        }
        self.input_sources = sources;
        self.sources_revision = revision;
        if current.is_some() && revision >= self.current_revision {
            self.current_input_source = current;
            self.current_revision = revision;
        }
        Ok(())
    }

    fn replace_current_source(
        &mut self,
        value: &Value,
        revision: Option<u64>,
    ) -> Result<(), String> {
        let revision = revision
            .or_else(|| {
                json_revision(
                    value
                        .get("source")
                        .and_then(|source| source.get("revision")),
                )
            })
            .ok_or_else(|| "Gnoblin sent the current input source without a revision".to_owned())?;
        if revision < self.current_revision {
            return Ok(());
        }
        let available = value
            .get("available")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        self.current_input_source = if available {
            Some(
                parse_input_source(
                    value
                        .get("source")
                        .ok_or_else(|| "Gnoblin omitted the current input source".to_owned())?,
                )
                .ok_or_else(|| "Gnoblin returned an invalid current input source".to_owned())?,
            )
        } else {
            None
        };
        self.current_revision = revision;
        Ok(())
    }

    fn replace_privacy(&mut self, value: &Value, revision: Option<u64>) -> Result<(), String> {
        let revision = revision
            .or_else(|| json_revision(value.get("revision")))
            .ok_or_else(|| "Gnoblin sent privacy state without a revision".to_owned())?;
        if revision < self.privacy_revision {
            return Ok(());
        }
        let available = value
            .get("available")
            .and_then(Value::as_object)
            .ok_or_else(|| "Gnoblin sent privacy state without availability details".to_owned())?;
        let is_available = |key: &str| -> Result<bool, String> {
            available
                .get(key)
                .and_then(Value::as_bool)
                .ok_or_else(|| format!("Gnoblin omitted privacy availability for {key}"))
        };
        let read_bool = |key: &str, available: bool| -> Result<bool, String> {
            if !available {
                return Ok(false);
            }
            value
                .get(key)
                .and_then(Value::as_bool)
                .ok_or_else(|| format!("Gnoblin omitted available privacy state for {key}"))
        };
        let screen_sharing_available = is_available("screen_sharing")?;
        let microphone_available = is_available("microphone_in_use")?;
        let location_available = is_available("location_in_use")?;
        self.privacy = PrivacyState {
            available: true,
            screen_sharing_available,
            screen_sharing: read_bool("screen_sharing", screen_sharing_available)?,
            microphone_available,
            microphone_in_use: read_bool("microphone_in_use", microphone_available)?,
            location_available,
            location_in_use: read_bool("location_in_use", location_available)?,
        };
        self.privacy_revision = revision;
        Ok(())
    }

    fn as_desktop_state(&self) -> DesktopState {
        DesktopState {
            available: self.available,
            input_sources: self.input_sources.clone(),
            current_input_source: self.current_input_source.clone(),
            privacy: self.privacy,
        }
    }
}

fn parse_input_source(value: &Value) -> Option<InputSource> {
    let source_type = value.get("type")?.as_str()?;
    let id = value.get("id")?.as_str()?;
    let short_name = value.get("short_name")?.as_str()?;
    let display_name = value.get("name")?.as_str()?;
    let fields = [source_type, id, short_name, display_name];
    if fields.iter().any(|field| {
        field.len() > MAX_INPUT_SOURCE_FIELD_BYTES || field.chars().any(char::is_control)
    }) {
        return None;
    }
    Some(InputSource {
        source_type: source_type.to_owned(),
        id: id.to_owned(),
        short_name: short_name.to_owned(),
        display_name: display_name.to_owned(),
    })
}

fn parse_monitor_snapshot(value: &Value) -> BTreeMap<String, Monitor> {
    value
        .get("monitors")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(parse_monitor)
        .map(|monitor| (monitor.id.clone(), monitor))
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Monitor {
    id: String,
    index: i32,
}

fn parse_monitor(value: &Value) -> Option<Monitor> {
    let id = value.get("id")?.as_str()?;
    let index = value.get("index")?.as_i64()?;
    if id.is_empty()
        || id.len() > MAX_INPUT_SOURCE_FIELD_BYTES
        || !(0..=i32::MAX as i64).contains(&index)
    {
        return None;
    }
    Some(Monitor {
        id: id.to_owned(),
        index: index as i32,
    })
}

fn osd_request_from_event(
    value: &Value,
    monitors: &BTreeMap<String, Monitor>,
) -> Option<OsdRequest> {
    let monitor_id = value.get("monitor_id")?.as_str()?;
    let monitor_index = monitors.get(monitor_id)?.index;
    let output_names = value
        .get("output_names")
        .and_then(Value::as_array)
        .map(|names| {
            names
                .iter()
                .map(|name| name.as_str().map(str::to_owned))
                .collect::<Option<Vec<_>>>()
        })
        .flatten()
        .unwrap_or_else(|| vec![monitor_id.to_owned()]);
    let icon = value
        .get("icon")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let label = value
        .get("label")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    // Gnoblin's native event carries presentation details only. Negative
    // levels tell the existing Bingux OSD protocol to omit its level bar.
    OsdRequest::new(monitor_index, output_names, icon, label, -1.0, -1.0)
}

fn json_revision(value: Option<&Value>) -> Option<u64> {
    value.and_then(Value::as_u64).or_else(|| {
        value
            .and_then(Value::as_i64)
            .and_then(|number| u64::try_from(number).ok())
    })
}

fn publish_state(state: &StateSnapshot, sender: &SyncSender<Event>) -> Result<(), String> {
    sender
        .send(Event::DesktopState(state.as_desktop_state()))
        .map_err(|_| "desktop-state receiver stopped".to_owned())
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_BRIDGE_LINE_BYTES, StateSnapshot, osd_request_from_event, parse_input_source,
        parse_monitor_snapshot, read_json_line, supported_events, validate_hello,
    };
    use serde_json::json;
    use std::{collections::BTreeSet, io::Cursor};

    #[test]
    fn validates_the_socket_greeting_and_selects_only_supported_events() {
        let hello = json!({"event":"hello", "api_major":1, "api_minor":70});
        assert_eq!(validate_hello(&hello), Ok(70));
        assert!(validate_hello(&json!({"event":"hello", "api_major":2, "api_minor":70})).is_err());
        let available = BTreeSet::from([
            "gnoblin.privacy.changed".to_owned(),
            "gnoblin.osd.requested".to_owned(),
        ]);
        assert_eq!(
            supported_events(26, &available),
            vec!["gnoblin.privacy.changed"]
        );
        assert_eq!(
            supported_events(27, &available),
            vec!["gnoblin.privacy.changed", "gnoblin.osd.requested"]
        );
    }

    #[test]
    fn reads_one_bounded_json_line_and_rejects_oversized_records() {
        let mut input = Cursor::new(b"{\"event\":\"hello\"}\n\"next\"\n");
        assert_eq!(
            read_json_line(&mut input).unwrap().unwrap()["event"],
            "hello"
        );
        assert_eq!(read_json_line(&mut input).unwrap().unwrap(), "next");
        let oversized = vec![b'a'; MAX_BRIDGE_LINE_BYTES + 1];
        assert!(read_json_line(&mut Cursor::new(oversized)).is_err());
    }

    #[test]
    fn maps_and_validates_input_source_records() {
        let source = json!({"type":"xkb", "id":"us", "short_name":"US", "name":"English (US)"});
        assert_eq!(
            parse_input_source(&source).unwrap().display_name,
            "English (US)"
        );
        assert!(
            parse_input_source(
                &json!({"type":"xkb", "id":"us\n", "short_name":"US", "name":"English"})
            )
            .is_none()
        );
    }

    #[test]
    fn rejects_stale_snapshots_and_respects_privacy_availability() {
        let mut state = StateSnapshot::default();
        state
            .replace_sources(
                &json!({"revision":4, "sources":[{"type":"xkb", "id":"us", "short_name":"US", "name":"English (US)", "current":true}]}),
                Some(4),
            )
            .unwrap();
        state
            .replace_sources(&json!({"sources":[], "revision":3}), Some(3))
            .unwrap();
        assert_eq!(state.input_sources.len(), 1);
        state
            .replace_privacy(
                &json!({"revision":8, "available":{"screen_sharing":true,"microphone_in_use":false,"location_in_use":true}, "screen_sharing":true,"location_in_use":false}),
                Some(8),
            )
            .unwrap();
        assert!(state.privacy.screen_sharing);
        assert!(!state.privacy.microphone_in_use);
        assert!(!state.privacy.location_in_use);
    }

    #[test]
    fn maps_osd_monitor_id_to_current_index_and_drops_unresolved_requests() {
        let monitors = parse_monitor_snapshot(&json!({"monitors":[{"id":"DP-1", "index":2}]}));
        let request = osd_request_from_event(
            &json!({"monitor_id":"DP-1", "output_names":["DP-1"], "icon":"audio-volume-high-symbolic", "label":"Volume"}),
            &monitors,
        )
        .unwrap();
        assert_eq!(
            bingux_statusd::osd_json(&request).unwrap(),
            "{\"protocolVersion\":2,\"type\":\"osd\",\"monitorIndex\":2,\"outputNames\":[\"DP-1\"],\"icon\":\"audio-volume-high-symbolic\",\"label\":\"Volume\",\"level\":-1.0,\"maxLevel\":-1.0}\n"
        );
        assert!(osd_request_from_event(&json!({"monitor_id":"HDMI-1"}), &monitors).is_none());
    }
}
