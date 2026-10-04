use crate::Event;
use bingux_statusd::{DesktopState, InputSource, OSD_PROTOCOL_VERSION, OsdRequest, PrivacyState};
use futures_util::{FutureExt, StreamExt};
use serde_json::Value;
use std::{
    collections::HashMap,
    env,
    io::{self, BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
    sync::mpsc::SyncSender,
    thread,
    time::Duration,
};
use zbus::{Connection, Proxy};

const BUS_NAME: &str = "org.gnoblin.Shell";
const OBJECT_PATH: &str = "/org/gnoblin/Shell";
const INTERFACE: &str = "org.gnoblin.Shell";
const INITIAL_RECONNECT_DELAY: Duration = Duration::from_millis(250);
const MAX_RECONNECT_DELAY: Duration = Duration::from_secs(5);
// Reject oversized OSD messages before deserialization and bound text-bearing
// input-source state before it reaches the shell socket.
const MAX_INPUT_SOURCES: usize = 32;
const MAX_INPUT_SOURCE_FIELD_BYTES: usize = 128;
const MAX_INPUT_SOURCE_TOTAL_BYTES: usize = 4 * 1024;
const MAX_INPUT_SOURCE_BODY_BYTES: usize = 64 * 1024;
const MAX_OSD_SIGNAL_BODY_BYTES: u32 = 4 * 1024;
const GNOBLIN_COMPOSITOR_SOCKET_ENV: &str = "GNOBLIN_COMPOSITOR_SOCKET";
const COMPOSITOR_SOCKET_NAME: &str = "compositor-v1.sock";
const MAX_COMPOSITOR_RECORD_BYTES: usize = 64 * 1024;
const OSD_REQUEST_EVENT: &str = "gnoblin.osd.requested";
const OSD_REQUEST_API_MINOR: i64 = 27;
const MAX_MONITOR_ID_BYTES: usize = 128;
const MAX_ACTIVE_MONITORS: usize = 64;

type InputSourceTuple = (String, String, String, String);
// Gnoblin org.gnoblin.Shell.OsdRequested payload: (uissddas).
type OsdRequestTuple = (u32, i32, String, String, f64, f64, Vec<String>);

#[derive(Clone, Debug)]
struct MonitorRoute {
    index: i32,
    output_names: Vec<String>,
}

/// Start the session-bus subscriber for Gnoblin desktop state and OSD requests.
pub fn start_state_subscriber(sender: SyncSender<Event>) {
    thread::spawn(move || {
        async_io::block_on(run_state_subscriber(sender));
    });
}

/// Start the native compositor-socket subscriber for standalone Gnoblin OSDs.
pub fn start_osd_event_subscriber(sender: SyncSender<Event>) {
    thread::spawn(move || {
        let mut reconnect_delay = INITIAL_RECONNECT_DELAY;

        loop {
            match subscribe_to_compositor_osd(&sender) {
                Ok(()) => return,
                Err(error) => eprintln!("[bingux-statusd] Gnoblin OSD events unavailable: {error}"),
            }

            thread::sleep(reconnect_delay);
            reconnect_delay = std::cmp::min(reconnect_delay * 2, MAX_RECONNECT_DELAY);
        }
    });
}

fn subscribe_to_compositor_osd(sender: &SyncSender<Event>) -> Result<(), String> {
    let socket_path = compositor_socket_path()?;
    subscribe_to_compositor_osd_at(&socket_path, sender)
}

fn subscribe_to_compositor_osd_at(
    socket_path: &std::path::Path,
    sender: &SyncSender<Event>,
) -> Result<(), String> {
    let stream = UnixStream::connect(socket_path)
        .map_err(|error| format!("cannot connect to {}: {error}", socket_path.display()))?;
    let mut reader = BufReader::new(
        stream
            .try_clone()
            .map_err(|error| format!("cannot clone compositor socket: {error}"))?,
    );
    let mut writer = stream;
    let mut monitor_routes = HashMap::new();
    let mut hello_received = false;
    let mut osd_subscription_ready = false;

    loop {
        let record = read_compositor_record(&mut reader)
            .map_err(|error| format!("cannot read compositor socket: {error}"))?
            .ok_or_else(|| "compositor socket closed".to_owned())?;
        let event = record.get("event").and_then(Value::as_str).unwrap_or("");

        if !hello_received {
            if event != "hello" {
                return Err("compositor did not send a hello record".to_owned());
            }
            if !native_api_supports_osd_event(&record) {
                eprintln!(
                    "[bingux-statusd] compositor has no native OSD event API; keeping the compatibility OSD path"
                );
                loop {
                    match read_compositor_record(&mut reader) {
                        Ok(Some(_)) => {}
                        Ok(None) => return Err("legacy compositor socket closed".to_owned()),
                        Err(error) => {
                            return Err(format!("cannot read legacy compositor socket: {error}"));
                        }
                    }
                }
            }

            writer
                .write_all(b"{\"op\":\"monitors\",\"api_version\":{\"major\":1,\"minor\":27}}\n")
                .and_then(|()| {
                    writer.write_all(
                        b"{\"op\":\"events\",\"api_version\":{\"major\":1,\"minor\":27},\"events\":[\"gnoblin.osd.requested\"]}\n",
                    )
                })
                .map_err(|error| format!("cannot subscribe to compositor OSD events: {error}"))?;
            hello_received = true;
            continue;
        }

        match event {
            "monitors" => {
                monitor_routes = monitor_routes_from_snapshot(&record)
                    .ok_or_else(|| "compositor returned an invalid monitor snapshot".to_owned())?;
            }
            "subscribed" => {
                osd_subscription_ready = record
                    .get("events")
                    .and_then(Value::as_array)
                    .is_some_and(|events| {
                        events
                            .iter()
                            .any(|name| name.as_str() == Some(OSD_REQUEST_EVENT))
                    });
                if !osd_subscription_ready {
                    return Err("compositor did not subscribe to OSD request events".to_owned());
                }
            }
            OSD_REQUEST_EVENT if osd_subscription_ready => {
                if let Some(request) = osd_request_from_event(&record, &monitor_routes) {
                    sender
                        .send(Event::OsdRequest(request))
                        .map_err(|_| "OSD receiver stopped".to_owned())?;
                }
            }
            "error" => {
                let message = record
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("compositor socket request failed");
                return Err(message.to_owned());
            }
            _ => {}
        }
    }
}

fn compositor_socket_path() -> Result<PathBuf, String> {
    if let Some(path) = env::var_os(GNOBLIN_COMPOSITOR_SOCKET_ENV) {
        let path = PathBuf::from(path);
        if path.is_absolute() {
            return Ok(path);
        }
        return Err(format!(
            "{GNOBLIN_COMPOSITOR_SOCKET_ENV} must be an absolute path"
        ));
    }

    let runtime_directory = env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .ok_or_else(|| "XDG_RUNTIME_DIR is not set".to_owned())?;
    if !runtime_directory.is_absolute() {
        return Err("XDG_RUNTIME_DIR must be an absolute path".to_owned());
    }

    Ok(runtime_directory
        .join("gnoblin")
        .join(COMPOSITOR_SOCKET_NAME))
}

fn read_compositor_record(reader: &mut impl BufRead) -> io::Result<Option<Value>> {
    let mut line = Vec::with_capacity(1024);
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            if line.is_empty() {
                return Ok(None);
            }
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "truncated compositor socket record",
            ));
        }

        let newline = available.iter().position(|byte| *byte == b'\n');
        let consumed = newline.map_or(available.len(), |index| index + 1);
        if line.len().saturating_add(consumed) > MAX_COMPOSITOR_RECORD_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "compositor socket record exceeds 64 KiB",
            ));
        }
        line.extend_from_slice(&available[..consumed]);
        reader.consume(consumed);

        if newline.is_some() {
            break;
        }
    }

    line.pop();
    if line.last() == Some(&b'\r') {
        line.pop();
    }
    serde_json::from_slice(&line)
        .map(Some)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn native_api_supports_osd_event(hello: &Value) -> bool {
    hello.get("api_major").and_then(Value::as_i64) == Some(1)
        && hello
            .get("api_minor")
            .and_then(Value::as_i64)
            .is_some_and(|minor| minor >= OSD_REQUEST_API_MINOR)
        && hello
            .get("events")
            .and_then(Value::as_array)
            .is_some_and(|events| {
                events
                    .iter()
                    .any(|name| name.as_str() == Some(OSD_REQUEST_EVENT))
            })
}

fn monitor_routes_from_snapshot(snapshot: &Value) -> Option<HashMap<String, MonitorRoute>> {
    if snapshot.get("event").and_then(Value::as_str) != Some("monitors") {
        return None;
    }
    let monitors = snapshot.get("monitors")?.as_array()?;
    if monitors.len() > MAX_ACTIVE_MONITORS {
        return None;
    }

    let mut routes = HashMap::with_capacity(monitors.len());
    for monitor in monitors {
        let Some(id) = monitor.get("id").and_then(Value::as_str) else {
            continue;
        };
        let Some(index) = monitor
            .get("index")
            .and_then(Value::as_i64)
            .and_then(|index| i32::try_from(index).ok())
            .filter(|index| *index >= 0)
        else {
            continue;
        };
        if id.is_empty() || id.len() > MAX_MONITOR_ID_BYTES || id.chars().any(char::is_control) {
            continue;
        }

        let output_names = match monitor.get("output_names") {
            Some(names) => names
                .as_array()?
                .iter()
                .map(|name| name.as_str().map(str::to_owned))
                .collect::<Option<Vec<_>>>()?,
            None => vec![id.to_owned()],
        };
        routes.insert(
            id.to_owned(),
            MonitorRoute {
                index,
                output_names,
            },
        );
    }

    Some(routes)
}

fn osd_request_from_event(
    record: &Value,
    monitor_routes: &HashMap<String, MonitorRoute>,
) -> Option<OsdRequest> {
    if record.get("event").and_then(Value::as_str) != Some(OSD_REQUEST_EVENT) {
        return None;
    }
    let monitor_id = record.get("monitor_id").and_then(Value::as_str)?;
    let route = monitor_routes.get(monitor_id)?;
    let output_names = match record.get("output_names") {
        Some(names) => names
            .as_array()?
            .iter()
            .map(|name| name.as_str().map(str::to_owned))
            .collect::<Option<Vec<_>>>()?,
        None => route.output_names.clone(),
    };
    let icon = optional_event_string(record, "icon")?;
    let label = optional_event_string(record, "label")?;

    // Mutter's show-osd event contains icon and text but no numeric level.
    // Keep the level unavailable so Bingux does not render a fabricated meter.
    OsdRequest::new(route.index, output_names, icon, label, -1.0, -1.0)
}

fn optional_event_string(record: &Value, name: &str) -> Option<String> {
    match record.get(name) {
        None => Some(String::new()),
        Some(Value::String(value)) => Some(value.clone()),
        Some(_) => None,
    }
}

async fn run_state_subscriber(sender: SyncSender<Event>) {
    let mut reconnect_delay = INITIAL_RECONNECT_DELAY;

    loop {
        match subscribe_to_gnoblin(&sender, &mut reconnect_delay).await {
            Ok(()) => return,
            Err(error) => eprintln!("[bingux-statusd] Gnoblin state unavailable: {error}"),
        }

        if sender
            .send(Event::DesktopState(DesktopState::default()))
            .is_err()
        {
            return;
        }

        async_io::Timer::after(reconnect_delay).await;
        reconnect_delay = std::cmp::min(reconnect_delay * 2, MAX_RECONNECT_DELAY);
    }
}

async fn subscribe_to_gnoblin(
    sender: &SyncSender<Event>,
    reconnect_delay: &mut Duration,
) -> Result<(), String> {
    let connection = Connection::session()
        .await
        .map_err(|error| error.to_string())?;
    let proxy = Proxy::new(&connection, BUS_NAME, OBJECT_PATH, INTERFACE)
        .await
        .map_err(|error| error.to_string())?;
    // Install the subscription before reading the initial state. A later
    // relevant signal triggers a complete re-read, so a signal queued during
    // the snapshot cannot make the published state stale.
    let mut owner_changes = proxy
        .receive_owner_changed()
        .await
        .map_err(|error| error.to_string())?;
    let mut signals = proxy
        .receive_all_signals()
        .await
        .map_err(|error| error.to_string())?;
    publish_snapshot(&proxy, sender).await?;
    *reconnect_delay = INITIAL_RECONNECT_DELAY;

    loop {
        futures_util::select! {
            owner_change = owner_changes.next().fuse() => {
                match owner_change {
                    Some(Some(_)) => {
                        proxy
                            .call_method("Ping", &())
                            .await
                            .map_err(|error| error.to_string())?;
                        publish_snapshot(&proxy, sender).await?;
                    }
                    Some(None) => sender
                        .send(Event::DesktopState(DesktopState::default()))
                        .map_err(|_| "desktop-state receiver stopped".to_owned())?,
                    None => return Err("Gnoblin session owner stream closed".to_owned()),
                }
            }
            signal = signals.next().fuse() => {
                let Some(signal) = signal else {
                    return Err("Gnoblin session signal stream closed".to_owned());
                };

                if is_desktop_state_signal(&signal) {
                    publish_snapshot(&proxy, sender).await?;
                } else if is_osd_signal(&signal)
                    && has_supported_osd_body_size(signal.header().primary().body_len())
                {
                    if let Ok(request) = signal.body().deserialize::<OsdRequestTuple>() {
                        publish_osd_request(request, sender)?;
                    }
                }
            }
        }
    }
}

async fn publish_snapshot(proxy: &Proxy<'_>, sender: &SyncSender<Event>) -> Result<(), String> {
    let state = read_snapshot(proxy).await?;
    sender
        .send(Event::DesktopState(state))
        .map_err(|_| "desktop-state receiver stopped".to_owned())
}

fn publish_osd_request(request: OsdRequestTuple, sender: &SyncSender<Event>) -> Result<(), String> {
    let Some(request) = osd_request_from_tuple(request) else {
        return Ok(());
    };

    sender
        .send(Event::OsdRequest(request))
        .map_err(|_| "OSD receiver stopped".to_owned())
}

async fn read_snapshot(proxy: &Proxy<'_>) -> Result<DesktopState, String> {
    let (input_sources, current_input_source) = read_input_sources(proxy)
        .await
        .map_err(|error| error.to_string())?;
    let privacy = read_privacy_state(proxy)
        .await
        .map_err(|error| error.to_string())?;

    if !input_sources_are_valid(&input_sources)
        || !input_source_tuple_is_valid(&current_input_source)
    {
        return Err("Gnoblin returned an oversized or invalid input source".to_owned());
    }

    Ok(DesktopState {
        available: true,
        input_sources: input_sources.into_iter().map(input_source).collect(),
        current_input_source: input_source_or_none(current_input_source),
        privacy,
    })
}

async fn read_input_sources(
    proxy: &Proxy<'_>,
) -> Result<(Vec<InputSourceTuple>, InputSourceTuple), zbus::Error> {
    let input_sources_reply = proxy.call_method("ListInputSources", &()).await?;
    if input_sources_reply.body().len() > MAX_INPUT_SOURCE_BODY_BYTES {
        return Err(zbus::Error::ExcessData);
    }
    let input_sources: Vec<InputSourceTuple> = input_sources_reply.body().deserialize()?;

    let current_input_source_reply = proxy.call_method("GetCurrentInputSource", &()).await?;
    if current_input_source_reply.body().len() > MAX_INPUT_SOURCE_BODY_BYTES {
        return Err(zbus::Error::ExcessData);
    }
    let current_input_source: InputSourceTuple = current_input_source_reply.body().deserialize()?;

    Ok((input_sources, current_input_source))
}

async fn read_privacy_state(proxy: &Proxy<'_>) -> Result<PrivacyState, zbus::Error> {
    let privacy_reply = proxy.call_method("GetPrivacyState", &()).await?;
    if privacy_reply.body().len() > MAX_INPUT_SOURCE_BODY_BYTES {
        return Err(zbus::Error::ExcessData);
    }
    let (screen_sharing, microphone_in_use, location_in_use): (bool, bool, bool) =
        privacy_reply.body().deserialize()?;

    Ok(PrivacyState {
        screen_sharing,
        microphone_in_use,
        location_in_use,
    })
}

fn osd_request_from_tuple(request: OsdRequestTuple) -> Option<OsdRequest> {
    let (protocol_version, monitor_index, icon, label, level, max_level, output_names) = request;
    if protocol_version != OSD_PROTOCOL_VERSION {
        return None;
    }

    OsdRequest::new(monitor_index, output_names, icon, label, level, max_level)
}

fn has_supported_osd_body_size(body_len: u32) -> bool {
    body_len <= MAX_OSD_SIGNAL_BODY_BYTES
}
fn input_source(source: InputSourceTuple) -> InputSource {
    let (source_type, id, short_name, display_name) = source;

    InputSource {
        source_type,
        id,
        short_name,
        display_name,
    }
}

fn input_source_or_none(source: InputSourceTuple) -> Option<InputSource> {
    if source.0.is_empty() && source.1.is_empty() && source.2.is_empty() && source.3.is_empty() {
        None
    } else {
        Some(input_source(source))
    }
}

fn input_sources_are_valid(sources: &[InputSourceTuple]) -> bool {
    sources.len() <= MAX_INPUT_SOURCES
        && sources
            .iter()
            .try_fold(0usize, |total, source| {
                if !input_source_tuple_is_valid(source) {
                    return None;
                }

                total
                    .checked_add(source.0.len())
                    .and_then(|total| total.checked_add(source.1.len()))
                    .and_then(|total| total.checked_add(source.2.len()))
                    .and_then(|total| total.checked_add(source.3.len()))
            })
            .is_some_and(|total| total <= MAX_INPUT_SOURCE_TOTAL_BYTES)
}

fn input_source_tuple_is_valid(source: &InputSourceTuple) -> bool {
    [&source.0, &source.1, &source.2, &source.3]
        .into_iter()
        .all(|value| {
            value.len() <= MAX_INPUT_SOURCE_FIELD_BYTES && !value.chars().any(char::is_control)
        })
}

fn is_osd_signal(signal: &zbus::Message) -> bool {
    is_osd_signal_name(signal.header().member().as_ref().map(|name| name.as_str()))
}

fn is_osd_signal_name(name: Option<&str>) -> bool {
    matches!(name, Some("OsdRequested"))
}

fn is_desktop_state_signal(signal: &zbus::Message) -> bool {
    is_desktop_state_signal_name(signal.header().member().as_ref().map(|name| name.as_str()))
}

fn is_desktop_state_signal_name(name: Option<&str>) -> bool {
    matches!(
        name,
        Some("InputSourceChanged" | "InputSourcesChanged" | "PrivacyStateChanged")
    )
}

#[cfg(test)]
mod tests {
    use super::{
        InputSourceTuple, OSD_REQUEST_API_MINOR, OSD_REQUEST_EVENT, input_source_or_none,
        input_source_tuple_is_valid, input_sources_are_valid, is_desktop_state_signal_name,
        is_osd_signal_name, monitor_routes_from_snapshot, native_api_supports_osd_event,
        osd_request_from_event, osd_request_from_tuple, subscribe_to_compositor_osd_at,
    };
    use crate::Event;
    use bingux_statusd::osd_json;
    use serde_json::json;
    use std::{
        io::{BufRead, BufReader, Write},
        net::Shutdown,
        os::unix::net::UnixListener,
        sync::mpsc,
        thread,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn treats_an_empty_gnoblin_input_source_as_unavailable() {
        assert_eq!(input_source_or_none(empty_input_source()), None);
    }

    #[test]
    fn accepts_v2_osd_requests_with_finite_values() {
        assert!(
            osd_request_from_tuple((
                2,
                0,
                "audio-volume-high-symbolic".to_owned(),
                String::new(),
                0.75,
                1.0,
                output_names(),
            ))
            .is_some()
        );
    }

    #[test]
    fn rejects_unsupported_or_non_finite_osd_requests() {
        assert!(
            osd_request_from_tuple((1, 0, String::new(), String::new(), 0.5, 1.0, output_names()))
                .is_none()
        );
        assert!(
            osd_request_from_tuple((
                2,
                0,
                String::new(),
                String::new(),
                f64::NAN,
                1.0,
                output_names()
            ))
            .is_none()
        );
        assert!(
            osd_request_from_tuple((
                2,
                0,
                String::new(),
                String::new(),
                0.5,
                f64::NEG_INFINITY,
                output_names(),
            ))
            .is_none()
        );
    }

    #[test]
    fn native_osd_event_uses_connector_names_without_fabricating_a_level() {
        let routes = monitor_routes_from_snapshot(&json!({
            "event": "monitors",
            "monitors": [{ "id": "DP-1", "index": 2 }]
        }))
        .unwrap();
        let request = osd_request_from_event(
            &json!({
                "event": "gnoblin.osd.requested",
                "monitor_id": "DP-1",
                "output_names": ["DP-1", "DP-2"],
                "icon": "audio-volume-high-symbolic",
                "label": "Volume"
            }),
            &routes,
        )
        .unwrap();

        let record: serde_json::Value = serde_json::from_str(&osd_json(&request).unwrap()).unwrap();
        assert_eq!(record["monitorIndex"], 2);
        assert_eq!(record["outputNames"], json!(["DP-1", "DP-2"]));
        assert_eq!(record["level"], -1.0);
        assert_eq!(record["maxLevel"], -1.0);
    }

    #[test]
    fn native_osd_event_uses_monitor_snapshot_when_connector_names_are_omitted() {
        let routes = monitor_routes_from_snapshot(&json!({
            "event": "monitors",
            "monitors": [{
                "id": "DP-1",
                "index": 1,
                "output_names": ["DP-1", "DP-2"]
            }]
        }))
        .unwrap();
        let request = osd_request_from_event(
            &json!({
                "event": "gnoblin.osd.requested",
                "monitor_id": "DP-1"
            }),
            &routes,
        )
        .unwrap();

        let record: serde_json::Value = serde_json::from_str(&osd_json(&request).unwrap()).unwrap();
        assert_eq!(record["monitorIndex"], 1);
        assert_eq!(record["outputNames"], json!(["DP-1", "DP-2"]));
        assert_eq!(record["icon"], "");
        assert_eq!(record["label"], "");
    }

    #[test]
    fn rejects_native_osd_events_for_unknown_monitors() {
        let routes = monitor_routes_from_snapshot(&json!({
            "event": "monitors",
            "monitors": [{ "id": "DP-1", "index": 0 }]
        }))
        .unwrap();
        assert!(
            osd_request_from_event(
                &json!({
                    "event": "gnoblin.osd.requested",
                    "monitor_id": "HDMI-1",
                    "output_names": ["HDMI-1"]
                }),
                &routes
            )
            .is_none()
        );
    }

    #[test]
    fn requires_the_advertised_native_osd_event_version() {
        assert!(native_api_supports_osd_event(&json!({
            "api_major": 1,
            "api_minor": 27,
            "events": ["gnoblin.osd.requested"]
        })));
        assert!(!native_api_supports_osd_event(&json!({
            "api_major": 1,
            "api_minor": 26,
            "events": ["gnoblin.osd.requested"]
        })));
        assert!(!native_api_supports_osd_event(&json!({
            "api_major": 1,
            "api_minor": 27,
            "events": []
        })));
    }

    #[test]
    fn subscribes_to_native_osd_events_and_forwards_a_request() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let socket_path = std::env::temp_dir().join(format!(
            "bingux-statusd-osd-{}-{nonce}.sock",
            std::process::id()
        ));
        let listener = UnixListener::bind(&socket_path).unwrap();
        let (sender, receiver) = mpsc::sync_channel(4);
        let subscriber =
            thread::spawn(move || subscribe_to_compositor_osd_at(&socket_path, &sender));

        let (mut peer, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(peer.try_clone().unwrap());
        peer.write_all(
            b"{\"event\":\"hello\",\"api_major\":1,\"api_minor\":73,\"events\":[\"gnoblin.osd.requested\"]}\n",
        )
        .unwrap();
        let mut monitors_request = String::new();
        let mut events_request = String::new();
        reader.read_line(&mut monitors_request).unwrap();
        reader.read_line(&mut events_request).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&monitors_request).unwrap()["op"],
            "monitors"
        );
        let events_request: serde_json::Value = serde_json::from_str(&events_request).unwrap();
        assert_eq!(events_request["op"], "events");
        assert_eq!(
            events_request["api_version"]["minor"],
            OSD_REQUEST_API_MINOR
        );
        assert_eq!(events_request["events"], json!([OSD_REQUEST_EVENT]));

        peer.write_all(
            b"{\"event\":\"monitors\",\"monitors\":[{\"id\":\"DP-1\",\"index\":1}]}\n{\"event\":\"subscribed\",\"events\":[\"gnoblin.osd.requested\"]}\n{\"event\":\"gnoblin.osd.requested\",\"monitor_id\":\"DP-1\",\"output_names\":[\"DP-1\"],\"icon\":\"audio-volume-high-symbolic\",\"label\":\"Volume\"}\n",
        )
        .unwrap();
        peer.shutdown(Shutdown::Write).unwrap();

        let event = receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("native OSD event was not forwarded");
        let Event::OsdRequest(request) = event else {
            panic!("received a non-OSD event");
        };
        let record: serde_json::Value = serde_json::from_str(&osd_json(&request).unwrap()).unwrap();
        assert_eq!(record["monitorIndex"], 1);
        assert_eq!(record["level"], -1.0);
        assert_eq!(record["maxLevel"], -1.0);

        assert!(subscriber.join().unwrap().is_err());
        drop(reader);
        drop(listener);
        let socket_path = std::env::temp_dir().join(format!(
            "bingux-statusd-osd-{}-{nonce}.sock",
            std::process::id()
        ));
        std::fs::remove_file(socket_path).unwrap();
    }

    #[test]
    fn selects_only_the_gnoblin_osd_signal() {
        assert!(is_osd_signal_name(Some("OsdRequested")));
        assert!(!is_osd_signal_name(Some("InputSourceChanged")));
        assert!(!is_osd_signal_name(Some("OsdClosed")));
        assert!(!is_osd_signal_name(None));
    }
    #[test]
    fn rejects_oversized_input_source_state() {
        let oversized_source = ("x".repeat(129), String::new(), String::new(), String::new());
        assert!(!input_sources_are_valid(&[oversized_source]));
        assert!(!input_source_tuple_is_valid(&(
            String::new(),
            String::new(),
            String::new(),
            "line\nbreak".to_owned(),
        )));
        assert!(!input_sources_are_valid(
            &(0..33)
                .map(|index| (
                    index.to_string(),
                    String::new(),
                    String::new(),
                    String::new()
                ))
                .collect::<Vec<InputSourceTuple>>()
        ));
    }

    #[test]
    fn recognises_only_gnoblin_desktop_state_signals() {
        assert!(is_desktop_state_signal_name(Some("InputSourceChanged")));
        assert!(is_desktop_state_signal_name(Some("InputSourcesChanged")));
        assert!(is_desktop_state_signal_name(Some("PrivacyStateChanged")));
        assert!(!is_desktop_state_signal_name(Some("SuperReleased")));
        assert!(!is_desktop_state_signal_name(None));
    }

    fn empty_input_source() -> InputSourceTuple {
        (String::new(), String::new(), String::new(), String::new())
    }

    fn output_names() -> Vec<String> {
        vec!["DP-1".to_owned()]
    }
}
