//! The player on the session bus, as MPRIS 2: the shell's media controls,
//! the lock screen, media keys handled by the desktop, `playerctl` and
//! KDE Connect all drive it through this.
//!
//! [`Session::start`] puts a [`Player`] on the bus as
//! `org.mpris.MediaPlayer2.bingux_media` at `/org/mpris/MediaPlayer2`, with
//! the root interface (Raise, Quit, the identity and the file types it opens)
//! and the player interface (playback, seeking, volume, rate, shuffle, loop
//! status and the current item's metadata with its cover art). Every change
//! the player makes is sent as `PropertiesChanged`, a jump as `Seeked`. The
//! session runs on a thread of its own; a desktop without a session bus just
//! has no session (the player works the same).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use zbus::object_server::SignalEmitter;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};
use zbus::{connection, interface};

use crate::player::{Change, Player, Precision};
use crate::queue::Repeat;
use crate::status::{PlayState, Status};

/// The well-known name on the session bus.
pub const BUS_NAME: &str = "org.mpris.MediaPlayer2.bingux_media";
const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";
const NO_TRACK: &str = "/org/mpris/MediaPlayer2/TrackList/NoTrack";

/// What the desktop asks of the application rather than the player.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// Bring the window forward.
    Raise,
    /// Close the application.
    Quit,
    /// Play a file or URI (`OpenUri`).
    Open(String),
}

/// The MIME types the player says it opens.
pub const MIME_TYPES: &[&str] = &[
    "audio/mpeg",
    "audio/flac",
    "audio/x-flac",
    "audio/ogg",
    "audio/x-vorbis+ogg",
    "audio/x-opus+ogg",
    "audio/opus",
    "audio/mp4",
    "audio/x-m4a",
    "audio/aac",
    "audio/wav",
    "audio/x-wav",
    "audio/x-aiff",
    "audio/x-ape",
    "audio/x-wavpack",
    "audio/x-musepack",
    "audio/x-ms-wma",
    "audio/x-matroska",
    "video/mp4",
    "video/x-matroska",
    "video/webm",
    "video/x-msvideo",
    "video/quicktime",
    "video/x-ms-wmv",
    "video/x-flv",
    "video/mpeg",
    "video/mp2t",
    "video/ogg",
    "video/3gpp",
];

/// A running MPRIS session. Dropping it leaves the bus.
pub struct Session {
    _thread: std::thread::JoinHandle<()>,
}

struct Root {
    requests: Arc<dyn Fn(Request) + Send + Sync>,
}

#[interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    fn raise(&self) {
        (self.requests)(Request::Raise);
    }

    fn quit(&self) {
        (self.requests)(Request::Quit);
    }

    #[zbus(property)]
    fn can_quit(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_raise(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn identity(&self) -> String {
        "Media".to_owned()
    }

    #[zbus(property)]
    fn desktop_entry(&self) -> String {
        "bingux-media".to_owned()
    }

    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<String> {
        vec!["file".to_owned()]
    }

    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<String> {
        MIME_TYPES.iter().map(|mime| (*mime).to_owned()).collect()
    }
}

struct PlayerInterface {
    player: Player,
    requests: Arc<dyn Fn(Request) + Send + Sync>,
}

fn micros(seconds: f64) -> i64 {
    (seconds * 1_000_000.0).round() as i64
}

fn track_id(status: &Status) -> OwnedObjectPath {
    let path = status
        .item
        .as_ref()
        .map(|item| format!("/dev/drewett/bingux/Media/track/{}", item.id))
        .unwrap_or_else(|| NO_TRACK.to_owned());
    ObjectPath::try_from(path)
        .map(OwnedObjectPath::from)
        .unwrap_or_else(|_| OwnedObjectPath::from(ObjectPath::from_static_str_unchecked(NO_TRACK)))
}

fn value(value: Value<'_>) -> Option<OwnedValue> {
    value.try_to_owned().ok()
}

/// MPRIS metadata of what is playing.
fn metadata(status: &Status) -> HashMap<String, OwnedValue> {
    let mut map = HashMap::new();
    let Some(item) = &status.item else {
        if let Some(id) = value(Value::from(ObjectPath::from_static_str_unchecked(NO_TRACK))) {
            map.insert("mpris:trackid".to_owned(), id);
        }
        return map;
    };
    let mut put = |key: &str, entry: Option<OwnedValue>| {
        if let Some(entry) = entry {
            map.insert(key.to_owned(), entry);
        }
    };
    put(
        "mpris:trackid",
        value(Value::from(track_id(status).into_inner())),
    );
    if let Some(duration) = status.duration.or(item.duration) {
        put("mpris:length", value(Value::from(micros(duration))));
    }
    let title = status.title().unwrap_or_else(|| item.title.clone());
    put("xesam:title", value(Value::from(title)));
    if let Some(artist) = status.artist() {
        put("xesam:artist", value(Value::from(vec![artist])));
    }
    let album = if item.album.is_empty() {
        status.tags.album.clone().unwrap_or_default()
    } else {
        item.album.clone()
    };
    if !album.is_empty() {
        put("xesam:album", value(Value::from(album)));
    }
    if !item.album_artist.is_empty() {
        put(
            "xesam:albumArtist",
            value(Value::from(vec![item.album_artist.clone()])),
        );
    }
    if let Some(track) = item.track {
        put("xesam:trackNumber", value(Value::from(track as i32)));
    }
    if let Some(artwork) = &item.artwork {
        put(
            "mpris:artUrl",
            value(Value::from(bingux_files::location::file_uri(artwork))),
        );
    }
    put(
        "xesam:url",
        value(Value::from(bingux_files::location::file_uri(&item.path))),
    );
    map
}

fn playback_status(status: &Status) -> &'static str {
    match status.state {
        PlayState::Playing => "Playing",
        PlayState::Paused => "Paused",
        PlayState::Stopped => "Stopped",
    }
}

fn loop_status(repeat: Repeat) -> &'static str {
    match repeat {
        Repeat::Off => "None",
        Repeat::All => "Playlist",
        Repeat::One => "Track",
    }
}

#[interface(name = "org.mpris.MediaPlayer2.Player")]
impl PlayerInterface {
    fn next(&self) {
        self.player.next();
    }

    fn previous(&self) {
        self.player.previous();
    }

    fn pause(&self) {
        self.player.pause();
    }

    fn play_pause(&self) {
        self.player.toggle();
    }

    fn stop(&self) {
        self.player.pause();
        self.player.seek(0.0, Precision::Exact);
    }

    fn play(&self) {
        self.player.play();
    }

    /// Seek by `offset` microseconds.
    fn seek(&self, offset: i64) {
        let status = self.player.status();
        let target = status.position + offset as f64 / 1_000_000.0;
        // Past the end goes to the next item, as the specification says.
        if status.duration.is_some_and(|duration| target >= duration) {
            self.player.next();
        } else {
            self.player.seek(target.max(0.0), Precision::Exact);
        }
    }

    fn set_position(&self, track: ObjectPath<'_>, position: i64) {
        let status = self.player.status();
        if track.as_str() != track_id(&status).as_str() || position < 0 {
            return;
        }
        let seconds = position as f64 / 1_000_000.0;
        if status.duration.is_none_or(|duration| seconds <= duration) {
            self.player.seek(seconds, Precision::Exact);
        }
    }

    fn open_uri(&self, uri: String) {
        (self.requests)(Request::Open(uri));
    }

    #[zbus(signal)]
    async fn seeked(emitter: &SignalEmitter<'_>, position: i64) -> zbus::Result<()>;

    #[zbus(property)]
    fn playback_status(&self) -> String {
        playback_status(&self.player.status()).to_owned()
    }

    #[zbus(property)]
    fn loop_status(&self) -> String {
        loop_status(self.player.status().repeat).to_owned()
    }

    #[zbus(property)]
    fn set_loop_status(&self, value: String) {
        let repeat = match value.as_str() {
            "Track" => Repeat::One,
            "Playlist" => Repeat::All,
            _ => Repeat::Off,
        };
        self.player.set_repeat(repeat);
    }

    #[zbus(property)]
    fn rate(&self) -> f64 {
        self.player.status().speed
    }

    #[zbus(property)]
    fn set_rate(&self, value: f64) {
        // A rate of 0 means pause, by the specification.
        if value <= 0.0 {
            self.player.pause();
        } else {
            self.player.set_speed(value);
        }
    }

    #[zbus(property)]
    fn shuffle(&self) -> bool {
        self.player.status().shuffle
    }

    #[zbus(property)]
    fn set_shuffle(&self, value: bool) {
        self.player.set_shuffle(value);
    }

    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        metadata(&self.player.status())
    }

    #[zbus(property)]
    fn volume(&self) -> f64 {
        let status = self.player.status();
        if status.muted {
            0.0
        } else {
            status.volume / 100.0
        }
    }

    #[zbus(property)]
    fn set_volume(&self, value: f64) {
        self.player.set_volume(value.clamp(0.0, 1.0) * 100.0);
    }

    #[zbus(property(emits_changed_signal = "false"))]
    fn position(&self) -> i64 {
        micros(self.player.status().position)
    }

    #[zbus(property)]
    fn minimum_rate(&self) -> f64 {
        0.25
    }

    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        4.0
    }

    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        let queue = self.player.queue();
        let status = self.player.status();
        status.repeat == Repeat::All
            || queue
                .current()
                .is_some_and(|current| current + 1 < queue.items().len())
    }

    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        self.player.status().item.is_some()
    }

    #[zbus(property)]
    fn can_play(&self) -> bool {
        !self.player.queue().items().is_empty()
    }

    #[zbus(property)]
    fn can_pause(&self) -> bool {
        self.player.status().item.is_some()
    }

    #[zbus(property)]
    fn can_seek(&self) -> bool {
        self.player.status().duration.is_some()
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn can_control(&self) -> bool {
        true
    }
}

/// What was last told to the bus, to send only what changed.
#[derive(Default, PartialEq)]
struct Told {
    status: &'static str,
    loop_status: &'static str,
    rate: f64,
    shuffle: bool,
    metadata: Option<(OwnedObjectPath, Option<i64>, Option<String>, Option<String>)>,
    volume: f64,
    queue_len: usize,
    current: Option<usize>,
    can_seek: bool,
}

impl Session {
    /// Puts `player` on the session bus. `requests` gets Raise, Quit and
    /// OpenUri, on the session's thread.
    pub fn start(
        player: Player,
        requests: impl Fn(Request) + Send + Sync + 'static,
    ) -> Option<Session> {
        let requests: Arc<dyn Fn(Request) + Send + Sync> = Arc::new(requests);
        let (sender, receiver) = async_channel::unbounded::<Change>();
        let pending = Arc::new(Mutex::new(false));
        player.subscribe({
            let pending = pending.clone();
            move |change| {
                // Position steps are read on demand; only real changes and seeks wake the bus.
                let mut waiting = pending.lock().unwrap_or_else(PoisonError::into_inner);
                if matches!(change, Change::Seeked(_)) || !*waiting {
                    *waiting = !matches!(change, Change::Seeked(_));
                    let _ = sender.try_send(change);
                }
            }
        });
        let thread = std::thread::Builder::new()
            .name("media-mpris".into())
            .spawn(move || {
                zbus::block_on(async move {
                    let connection = connection::Builder::session()
                        .and_then(|builder| builder.name(BUS_NAME))
                        .and_then(|builder| {
                            builder.serve_at(
                                OBJECT_PATH,
                                Root {
                                    requests: requests.clone(),
                                },
                            )
                        })
                        .and_then(|builder| {
                            builder.serve_at(
                                OBJECT_PATH,
                                PlayerInterface {
                                    player: player.clone(),
                                    requests: requests.clone(),
                                },
                            )
                        });
                    let connection = match connection {
                        Ok(builder) => builder.build().await,
                        Err(error) => Err(error),
                    };
                    let connection = match connection {
                        Ok(connection) => connection,
                        Err(error) => {
                            eprintln!("[media] no media controls on the session bus: {error}");
                            return;
                        }
                    };
                    let Ok(interface) = connection
                        .object_server()
                        .interface::<_, PlayerInterface>(OBJECT_PATH)
                        .await
                    else {
                        return;
                    };
                    let mut told = Told::default();
                    while let Ok(change) = receiver.recv().await {
                        *pending.lock().unwrap_or_else(PoisonError::into_inner) = false;
                        let emitter = interface.signal_emitter();
                        if let Change::Seeked(seconds) = change {
                            let _ = PlayerInterface::seeked(emitter, micros(seconds)).await;
                            continue;
                        }
                        let status = player.status();
                        let queue = player.queue();
                        let now = Told {
                            status: playback_status(&status),
                            loop_status: loop_status(status.repeat),
                            rate: status.speed,
                            shuffle: status.shuffle,
                            metadata: status.item.as_ref().map(|item| {
                                (
                                    track_id(&status),
                                    status.duration.or(item.duration).map(micros),
                                    status.title(),
                                    item.artwork
                                        .as_ref()
                                        .map(|art| art.to_string_lossy().into_owned()),
                                )
                            }),
                            volume: if status.muted { 0.0 } else { status.volume },
                            queue_len: queue.items().len(),
                            current: queue.current(),
                            can_seek: status.duration.is_some(),
                        };
                        let iface = interface.get().await;
                        if now.status != told.status {
                            let _ = iface.playback_status_changed(emitter).await;
                            let _ = iface.can_play_changed(emitter).await;
                            let _ = iface.can_pause_changed(emitter).await;
                        }
                        if now.loop_status != told.loop_status {
                            let _ = iface.loop_status_changed(emitter).await;
                        }
                        if now.rate != told.rate {
                            let _ = iface.rate_changed(emitter).await;
                        }
                        if now.shuffle != told.shuffle {
                            let _ = iface.shuffle_changed(emitter).await;
                        }
                        if now.metadata != told.metadata {
                            let _ = iface.metadata_changed(emitter).await;
                        }
                        if now.volume != told.volume {
                            let _ = iface.volume_changed(emitter).await;
                        }
                        if (now.queue_len, now.current) != (told.queue_len, told.current) {
                            let _ = iface.can_go_next_changed(emitter).await;
                            let _ = iface.can_go_previous_changed(emitter).await;
                        }
                        if now.can_seek != told.can_seek {
                            let _ = iface.can_seek_changed(emitter).await;
                        }
                        told = now;
                    }
                });
            })
            .ok()?;
        Some(Session { _thread: thread })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queue::Item;

    #[test]
    fn metadata_names_the_item() {
        let status = Status {
            item: Some(Item {
                id: 4,
                title: "First Light".into(),
                artist: "Aurora Lane".into(),
                album: "Northern Lights".into(),
                track: Some(1),
                path: "/m/a.flac".into(),
                ..Item::default()
            }),
            duration: Some(40.0),
            ..Status::default()
        };
        let map = metadata(&status);
        assert_eq!(
            map.get("mpris:length")
                .map(|value| i64::try_from(value.clone()).unwrap()),
            Some(40_000_000)
        );
        assert_eq!(
            map.get("xesam:title")
                .map(|value| String::try_from(value.clone()).unwrap())
                .as_deref(),
            Some("First Light")
        );
        assert!(map.contains_key("mpris:trackid"));
    }

    #[test]
    fn loop_status_names() {
        assert_eq!(loop_status(Repeat::One), "Track");
        assert_eq!(loop_status(Repeat::All), "Playlist");
    }
}
