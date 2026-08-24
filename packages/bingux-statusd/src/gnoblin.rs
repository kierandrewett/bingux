use crate::Event;
use bingux_statusd::{DesktopState, InputSource, PrivacyState};
use futures_util::{FutureExt, StreamExt};
use std::{
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

type InputSourceTuple = (String, String, String, String);

/// Start the session-bus subscriber that supplies Gnoblin desktop state.
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

        if sender.send(Event::DesktopState(DesktopState::default())).is_err() {
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
    let connection = Connection::session().await.map_err(|error| error.to_string())?;
    let proxy = Proxy::new(&connection, BUS_NAME, OBJECT_PATH, INTERFACE)
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

    publish_snapshot(&proxy, sender).await?;
    *reconnect_delay = INITIAL_RECONNECT_DELAY;

    loop {
        futures_util::select! {
            owner_change = owner_changes.next().fuse() => {
                match owner_change {
                    Some(_) => return Err("Gnoblin session service owner changed".to_owned()),
                    None => return Err("Gnoblin session owner stream closed".to_owned()),
                }
            }
            signal = signals.next().fuse() => {
                let Some(signal) = signal else {
                    return Err("Gnoblin session signal stream closed".to_owned());
                };

                if is_desktop_state_signal(&signal) {
                    publish_snapshot(&proxy, sender).await?;
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

async fn read_snapshot(proxy: &Proxy<'_>) -> Result<DesktopState, String> {
    let input_sources: Vec<InputSourceTuple> = proxy
        .call("ListInputSources", &())
        .await
        .map_err(|error| error.to_string())?;
    let current_input_source: InputSourceTuple = proxy
        .call("GetCurrentInputSource", &())
        .await
        .map_err(|error| error.to_string())?;
    let (screen_sharing, microphone_in_use, location_in_use): (bool, bool, bool) = proxy
        .call("GetPrivacyState", &())
        .await
        .map_err(|error| error.to_string())?;

    Ok(DesktopState {
        available: true,
        input_sources: input_sources.into_iter().map(input_source).collect(),
        current_input_source: input_source_or_none(current_input_source),
        privacy: PrivacyState {
            screen_sharing,
            microphone_in_use,
            location_in_use,
        },
    })
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
    use super::{InputSourceTuple, input_source_or_none, is_desktop_state_signal_name};

    #[test]
    fn treats_an_empty_gnoblin_input_source_as_unavailable() {
        assert_eq!(input_source_or_none(empty_input_source()), None);
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
}
