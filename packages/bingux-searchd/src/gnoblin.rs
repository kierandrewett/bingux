use futures_util::{FutureExt, StreamExt};
use std::{sync::mpsc::SyncSender, thread, time::Duration};
use zbus::{Connection, Proxy};

const BUS_NAME: &str = "org.gnoblin.Shell";
const OBJECT_PATH: &str = "/org/gnoblin/Shell";
const INTERFACE: &str = "org.gnoblin.Shell";
const SUPER_RELEASE_PROTOCOL_VERSION: u32 = 1;
const INITIAL_RECONNECT_DELAY: Duration = Duration::from_millis(250);
const MAX_RECONNECT_DELAY: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub enum Event {
    Ready,
    Unavailable,
    SuperReleased { monotonic_usec: u64 },
}

pub fn start_super_release_subscriber(sender: SyncSender<Event>) {
    thread::spawn(move || {
        async_io::block_on(run_super_release_subscriber(sender));
    });
}

async fn run_super_release_subscriber(sender: SyncSender<Event>) {
    let mut reconnect_delay = INITIAL_RECONNECT_DELAY;

    loop {
        match subscribe_to_gnoblin(&sender, &mut reconnect_delay).await {
            Ok(()) => return,
            Err(error) => eprintln!("[bingux-searchd] Gnoblin Super release unavailable: {error}"),
        }

        if sender.send(Event::Unavailable).is_err() {
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
    let mut owner_changes = proxy
        .receive_owner_changed()
        .await
        .map_err(|error| error.to_string())?;
    let mut signals = proxy
        .receive_all_signals()
        .await
        .map_err(|error| error.to_string())?;

    *reconnect_delay = INITIAL_RECONNECT_DELAY;
    sender
        .send(Event::Ready)
        .map_err(|_| "search event receiver stopped".to_owned())?;

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

                if let Some(monotonic_usec) = super_release_timestamp(&signal) {
                    sender
                        .send(Event::SuperReleased { monotonic_usec })
                        .map_err(|_| "search event receiver stopped".to_owned())?;
                }
            }
        }
    }
}

fn super_release_timestamp(signal: &zbus::Message) -> Option<u64> {
    let header = signal.header();
    let member = header.member().as_ref()?.as_str();
    if member != "SuperReleased" {
        return None;
    }

    let (protocol_version, monotonic_usec): (u32, u64) = signal.body().deserialize().ok()?;
    (protocol_version == SUPER_RELEASE_PROTOCOL_VERSION).then_some(monotonic_usec)
}

#[cfg(test)]
mod tests {
    use super::SUPER_RELEASE_PROTOCOL_VERSION;

    #[test]
    fn supports_only_the_documented_gnoblin_signal_version() {
        assert_eq!(SUPER_RELEASE_PROTOCOL_VERSION, 1);
    }
}
