use crate::Event;
use bingux_statusd::{DesktopState, InputSource, OSD_PROTOCOL_VERSION, OsdRequest, PrivacyState};
use futures_util::{FutureExt, StreamExt};
use std::{sync::mpsc::SyncSender, thread, time::Duration};
use zbus::{Connection, Proxy};

const BUS_NAME: &str = "org.gnoblin.Shell";
const OBJECT_PATH: &str = "/org/gnoblin/Shell";
const INTERFACE: &str = "org.gnoblin.Shell";
// Older installed Gnoblin builds expose OSD on org.gnoblin.Shell, but do not
// yet expose input-source state there.  The hot-loaded compatibility service
// intentionally has its own name so we can retain the canonical shell proxy
// (and its OSD subscription) while sourcing just keyboard state from it.
const INPUT_SOURCE_BUS_NAME: &str = "org.gnoblin.InputSources";
const INPUT_SOURCE_OBJECT_PATH: &str = "/org/gnoblin/InputSources";
const INPUT_SOURCE_INTERFACE: &str = "org.gnoblin.InputSources";
const INITIAL_RECONNECT_DELAY: Duration = Duration::from_millis(250);
const MAX_RECONNECT_DELAY: Duration = Duration::from_secs(5);
// Reject oversized OSD messages before deserialization and bound text-bearing
// input-source state before it reaches the shell socket.
const MAX_INPUT_SOURCES: usize = 32;
const MAX_INPUT_SOURCE_FIELD_BYTES: usize = 128;
const MAX_INPUT_SOURCE_TOTAL_BYTES: usize = 4 * 1024;
const MAX_INPUT_SOURCE_BODY_BYTES: usize = 64 * 1024;
const MAX_OSD_SIGNAL_BODY_BYTES: u32 = 4 * 1024;

type InputSourceTuple = (String, String, String, String);
// Gnoblin org.gnoblin.Shell.OsdRequested payload: (uissddas).
type OsdRequestTuple = (u32, i32, String, String, f64, f64, Vec<String>);

/// Start the session-bus subscriber for Gnoblin desktop state and OSD requests.
pub fn start_state_subscriber(sender: SyncSender<Event>) {
    thread::spawn(move || {
        async_io::block_on(run_state_subscriber(sender));
    });
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
    let input_source_proxy = Proxy::new(
        &connection,
        INPUT_SOURCE_BUS_NAME,
        INPUT_SOURCE_OBJECT_PATH,
        INPUT_SOURCE_INTERFACE,
    )
    .await
    .map_err(|error| error.to_string())?;

    // Install both subscriptions before reading the initial state. A later
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
    let mut input_source_owner_changes = input_source_proxy
        .receive_owner_changed()
        .await
        .map_err(|error| error.to_string())?;
    let mut input_source_signals = input_source_proxy
        .receive_all_signals()
        .await
        .map_err(|error| error.to_string())?;

    publish_snapshot(&proxy, &input_source_proxy, sender).await?;
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
                        publish_snapshot(&proxy, &input_source_proxy, sender).await?;
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
                    publish_snapshot(&proxy, &input_source_proxy, sender).await?;
                } else if is_osd_signal(&signal)
                    && has_supported_osd_body_size(signal.header().primary().body_len())
                {
                    if let Ok(request) = signal.body().deserialize::<OsdRequestTuple>() {
                        publish_osd_request(request, sender)?;
                    }
                }
            }
            owner_change = input_source_owner_changes.next().fuse() => {
                match owner_change {
                    Some(Some(_)) => publish_snapshot(&proxy, &input_source_proxy, sender).await?,
                    // The canonical proxy can still provide input sources. Do
                    // not clear the published desktop state merely because the
                    // optional compatibility service is being reloaded.
                    Some(None) => {}
                    None => return Err("Gnoblin input-source owner stream closed".to_owned()),
                }
            }
            signal = input_source_signals.next().fuse() => {
                let Some(signal) = signal else {
                    return Err("Gnoblin input-source signal stream closed".to_owned());
                };

                if is_input_source_service_signal(&signal) {
                    publish_snapshot(&proxy, &input_source_proxy, sender).await?;
                }
            }
        }
    }
}

async fn publish_snapshot(
    proxy: &Proxy<'_>,
    input_source_proxy: &Proxy<'_>,
    sender: &SyncSender<Event>,
) -> Result<(), String> {
    let state = read_snapshot(proxy, input_source_proxy).await?;
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

async fn read_snapshot(
    proxy: &Proxy<'_>,
    input_source_proxy: &Proxy<'_>,
) -> Result<DesktopState, String> {
    let (input_sources, current_input_source) = match read_input_sources(proxy).await {
        Ok(snapshot) => snapshot,
        Err(error) if is_unknown_method(&error) => read_input_sources(input_source_proxy)
            .await
            .map_err(|error| error.to_string())?,
        Err(error) => return Err(error.to_string()),
    };

    let privacy = match read_privacy_state(proxy).await {
        Ok(privacy) => privacy,
        // Privacy did not exist on the older installed Gnoblin snapshot. Do
        // not hide working input-source state for that one, known omission.
        Err(error) if is_unknown_method(&error) => PrivacyState::default(),
        Err(error) => return Err(error.to_string()),
    };

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

fn is_unknown_method(error: &zbus::Error) -> bool {
    matches!(
        error,
        zbus::Error::MethodError(name, _, _)
            if name.as_str() == "org.freedesktop.DBus.Error.UnknownMethod"
    )
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

fn is_input_source_service_signal(signal: &zbus::Message) -> bool {
    is_input_source_service_signal_name(signal.header().member().as_ref().map(|name| name.as_str()))
}

fn is_input_source_service_signal_name(name: Option<&str>) -> bool {
    matches!(name, Some("InputSourceChanged" | "InputSourcesChanged"))
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
        InputSourceTuple, input_source_or_none, input_source_tuple_is_valid,
        input_sources_are_valid, is_desktop_state_signal_name, is_input_source_service_signal_name,
        is_osd_signal_name, osd_request_from_tuple,
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

    #[test]
    fn recognises_only_input_source_compatibility_service_signals() {
        assert!(is_input_source_service_signal_name(Some(
            "InputSourceChanged"
        )));
        assert!(is_input_source_service_signal_name(Some(
            "InputSourcesChanged"
        )));
        assert!(!is_input_source_service_signal_name(Some(
            "PrivacyStateChanged"
        )));
        assert!(!is_input_source_service_signal_name(Some("OsdRequested")));
        assert!(!is_input_source_service_signal_name(None));
    }

    fn empty_input_source() -> InputSourceTuple {
        (String::new(), String::new(), String::new(), String::new())
    }

    fn output_names() -> Vec<String> {
        vec!["DP-1".to_owned()]
    }
}
