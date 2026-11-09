//! The app's one player, library and thumbnailer, made before the window
//! opens, and how the window hears about them.
//!
//! [`engine`] is the process's engine. The window's root calls [`use_live`],
//! which keeps a [`Live`] (the player's status and queue, the library) in
//! states that redraw what reads them. Everything that waits on the disk or
//! the network runs on a worker or on the engine's own threads.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use bingux_media::library::{Library, Scrobble, Snapshot, kind_of};
use bingux_media::settings::{self, Settings};
use bingux_media::thumbnail::Thumbnailer;
use bingux_media::{
    Frame, FrameSink, Hooks, Item, Kind, Options, Player, Queue, Status, lastfm, mpris,
};
use bingux_ui::media::{VideoFrame, VideoFrames};
use bingux_ui::task::Feed;
use freya::prelude::*;

/// Everything that plays and remembers.
pub struct Engine {
    pub player: Player,
    pub library: Arc<Library>,
    pub thumbs: Thumbnailer,
    pub frames: VideoFrames,
    pub settings: Arc<Mutex<Settings>>,
    snapshot: Mutex<Arc<Snapshot>>,
    scanning: AtomicBool,
    changed: Feed<()>,
    announced: AtomicBool,
    _mpris: Mutex<Option<mpris::Session>>,
}

static ENGINE: OnceLock<Engine> = OnceLock::new();

/// The process's engine. Made by [`start`] before the window.
pub fn engine() -> &'static Engine {
    ENGINE.get().expect("the engine starts before the window")
}

struct Sink(VideoFrames);

impl FrameSink for Sink {
    fn size(&self) -> (u32, u32) {
        self.0.size()
    }
    fn push(&self, frame: Frame) {
        self.0.push(VideoFrame {
            width: frame.width,
            height: frame.height,
            pixels: frame.pixels,
        });
    }
    fn clear(&self) {
        self.0.clear();
    }
}

/// Resume points, play counts and scrobbles.
struct AppHooks {
    library: Arc<Library>,
    settings: Arc<Mutex<Settings>>,
}

fn scrobble_of(item: &Item, at: i64) -> Scrobble {
    Scrobble {
        artist: item.artist.clone(),
        title: item.title.clone(),
        album: item.album.clone(),
        album_artist: item.album_artist.clone(),
        duration: item.duration,
        at,
    }
}

impl AppHooks {
    fn scrobbling(&self) -> bool {
        self.settings
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .scrobble
    }
}

impl Hooks for AppHooks {
    fn resume_at(&self, item: &Item) -> Option<f64> {
        let resume = self
            .settings
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .resume;
        resume.then(|| self.library.position(&item.path)).flatten()
    }

    fn save_position(&self, item: &Item, position: Option<f64>, duration: Option<f64>) {
        self.library.set_position(&item.path, position, duration);
        if let Some(engine) = ENGINE.get() {
            engine.refresh_snapshot();
        }
    }

    fn started(&self, item: &Item) {
        if item.kind != Kind::Audio || !self.scrobbling() {
            return;
        }
        let play = scrobble_of(item, now());
        if let Some(account) = lastfm::load().filter(lastfm::Account::signed_in)
            && lastfm::scrobblable(&play)
        {
            let _ = lastfm::now_playing(&account, &play);
        }
    }

    fn listened(&self, item: &Item, at: SystemTime) {
        let at = at
            .duration_since(UNIX_EPOCH)
            .map_or(0, |time| time.as_secs() as i64);
        self.library.record_play(&item.path, at);
        if item.kind == Kind::Audio && self.scrobbling() {
            let play = scrobble_of(item, at);
            if lastfm::scrobblable(&play) {
                self.library.queue_scrobble(&play);
            }
            send_scrobbles(&self.library);
        }
    }
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |time| time.as_secs() as i64)
}

/// Sends the plays waiting for Last.fm. Called on the player's thread.
pub fn send_scrobbles(library: &Library) {
    let Some(account) = lastfm::load().filter(lastfm::Account::signed_in) else {
        return;
    };
    loop {
        let batch = library.pending_scrobbles(lastfm::BATCH);
        if batch.is_empty() {
            return;
        }
        let plays: Vec<Scrobble> = batch.iter().map(|(_, play)| play.clone()).collect();
        match lastfm::scrobble(&account, &plays) {
            Ok(()) => {
                library.forget_scrobbles(&batch.iter().map(|(id, _)| *id).collect::<Vec<_>>())
            }
            Err(error) => {
                eprintln!("[media] scrobbles wait: {error}");
                return;
            }
        }
    }
}

/// The languages to prefer for audio and subtitles, from the locale.
fn languages() -> Vec<String> {
    let locale = std::env::var("LC_ALL")
        .or_else(|_| std::env::var("LC_MESSAGES"))
        .or_else(|_| std::env::var("LANG"))
        .unwrap_or_default();
    let code = locale
        .split(['_', '.', '@'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let mut languages = Vec::new();
    if code.len() == 2 && code != "c" {
        languages.push(code);
    }
    if !languages.iter().any(|code| code == "en") {
        languages.push("en".into());
    }
    languages
}

/// A player listener that keeps the screen on while a film plays (not for
/// music: the screen may sleep with a song going). The hold is asked for and
/// let go on a thread of its own, since it is a bus call.
fn keep_awake() -> impl Fn(bingux_media::Change) + Send + Sync + 'static {
    use bingux_ui::platform::inhibit::{Inhibitor, What, inhibit};
    let (send, receive) = std::sync::mpsc::channel::<bool>();
    let _ = std::thread::Builder::new()
        .name("media-awake".into())
        .spawn(move || {
            let mut hold: Option<Inhibitor> = None;
            for want in receive {
                if want && hold.is_none() {
                    hold = inhibit("Media", "Playing a video", What::IDLE)
                        .map_err(|error| eprintln!("[media] the screen may sleep: {error}"))
                        .ok();
                } else if !want {
                    hold = None;
                }
            }
        });
    let send = Mutex::new(send);
    let held = AtomicBool::new(false);
    move |_| {
        let Some(engine) = ENGINE.get() else {
            return;
        };
        let status = engine.player.status();
        let want = status.state == bingux_media::PlayState::Playing
            && (status.has_video
                || status
                    .item
                    .as_ref()
                    .is_some_and(|item| item.kind == Kind::Video));
        if held.swap(want, Ordering::AcqRel) != want {
            let _ = send
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .send(want);
        }
    }
}

/// Starts the engine. Before the window: it waits for mpv and the index.
pub fn start() {
    let settings = Arc::new(Mutex::new(settings::load()));
    let library = Arc::new(Library::open_default().unwrap_or_else(|error| {
        eprintln!("[media] the library index cannot be opened ({error}); using one in memory");
        Library::in_memory(&std::env::temp_dir()).expect("an in-memory index")
    }));
    let frames = VideoFrames::new();
    let options = {
        let settings = settings.lock().unwrap_or_else(PoisonError::into_inner);
        Options {
            volume: settings.volume,
            muted: settings.muted,
            levelling: settings.levelling(),
            languages: languages(),
            audio_output: std::env::var("BINGUX_MEDIA_AO").unwrap_or_default(),
        }
    };
    let hooks = Arc::new(AppHooks {
        library: library.clone(),
        settings: settings.clone(),
    });
    let player = match Player::new(Arc::new(Sink(frames.clone())), hooks, &options) {
        Ok(player) => player,
        Err(error) => {
            eprintln!("[media] cannot start the player: {error}");
            std::process::exit(1);
        }
    };
    if !settings
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .subtitles
    {
        player.select_subtitle(None);
    }
    frames.on_resize({
        let player = player.clone();
        move || player.picture_resized()
    });
    let snapshot = Arc::new(library.snapshot());
    let built = ENGINE.set(Engine {
        player: player.clone(),
        library,
        thumbs: Thumbnailer::new(),
        frames,
        settings,
        snapshot: Mutex::new(snapshot),
        scanning: AtomicBool::new(false),
        changed: Feed::default(),
        announced: AtomicBool::new(false),
        _mpris: Mutex::new(None),
    });
    if built.is_err() {
        return;
    }
    let shared = engine();
    player.subscribe(|_| engine().announce());
    player.subscribe(keep_awake());
    let session = mpris::Session::start(player, |request| match request {
        mpris::Request::Raise => {
            bingux_ui::app::open_uri("bingux-media://");
        }
        mpris::Request::Quit => {
            engine().player.shutdown();
            std::process::exit(0);
        }
        mpris::Request::Open(uri) => {
            use std::os::unix::ffi::OsStringExt;
            let path = match uri.strip_prefix("file://") {
                Some(rest) => PathBuf::from(std::ffi::OsString::from_vec(
                    bingux_files::location::percent_decode(rest),
                )),
                None => PathBuf::from(&uri),
            };
            bingux_ui::app::open_uri(&crate::open_address(&path));
        }
    });
    *shared._mpris.lock().unwrap_or_else(PoisonError::into_inner) = session;
    shared.scan();
}

impl Engine {
    fn announce(&self) {
        if !self.announced.swap(true, Ordering::AcqRel) {
            self.changed.push(());
        }
    }

    /// The library as last read.
    pub fn snapshot(&self) -> Arc<Snapshot> {
        self.snapshot
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Reads the index again (after a resume point or a play count changed).
    pub fn refresh_snapshot(&self) {
        let fresh = Arc::new(self.library.snapshot());
        *self.snapshot.lock().unwrap_or_else(PoisonError::into_inner) = fresh;
        self.announce();
    }

    pub fn is_scanning(&self) -> bool {
        self.scanning.load(Ordering::Acquire)
    }

    /// Brings the library up to date on a thread of its own, then makes the
    /// films' thumbnails that are missing.
    pub fn scan(&'static self) {
        if self.scanning.swap(true, Ordering::AcqRel) {
            return;
        }
        self.announce();
        let folders = self
            .settings
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .library_folders();
        let _ = std::thread::Builder::new()
            .name("media-scan".into())
            .spawn(move || {
                let report = self.library.scan(&folders, &|_| {});
                if report.changed() {
                    eprintln!(
                        "[media] library: {} added, {} updated, {} removed",
                        report.added, report.updated, report.removed
                    );
                }
                self.refresh_snapshot();
                self.scanning.store(false, Ordering::Release);
                self.announce();
                for video in self
                    .snapshot()
                    .videos
                    .iter()
                    // A thumbnail kept from before the length was learnt is
                    // made again, which learns it.
                    .filter(|video| video.thumbnail.is_none() || video.duration.is_none())
                {
                    let path = video.path.clone();
                    self.thumbs
                        .library_thumbnail(&video.path, video.modified, move |made| {
                            if let Some((_, duration)) = made {
                                if let Some(duration) = duration {
                                    engine().library.set_duration(&path, duration);
                                }
                                engine().refresh_snapshot();
                            }
                        });
                }
            });
    }

    /// Changes the settings and saves them on a worker.
    pub fn change_settings(&self, change: impl FnOnce(&mut Settings)) {
        let copy = {
            let mut settings = self.settings.lock().unwrap_or_else(PoisonError::into_inner);
            change(&mut settings);
            settings.clone()
        };
        let _ = std::thread::Builder::new()
            .name("media-settings".into())
            .spawn(move || {
                if let Err(error) = settings::save(&copy) {
                    eprintln!("[media] settings not saved: {error}");
                }
            });
        self.announce();
    }

    pub fn settings(&self) -> Settings {
        self.settings
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Plays files and folders given by path (dragged in, opened from
    /// Files): the media in them, in name order.
    pub fn play_paths(&self, paths: &[PathBuf]) {
        let mut items = Vec::new();
        let snapshot = self.snapshot();
        for path in paths {
            collect(path, &mut items, &snapshot);
        }
        if !items.is_empty() {
            self.player.play_items(items, 0);
        }
    }
}

/// The items for a file or the media inside a folder, using the library's
/// tags when it has the file.
fn collect(path: &Path, items: &mut Vec<Item>, snapshot: &Snapshot) {
    if path.is_dir() {
        let Ok(entries) = std::fs::read_dir(path) else {
            return;
        };
        let mut children: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
        children.sort();
        for child in children {
            if !child
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with('.'))
            {
                collect(&child, items, snapshot);
            }
        }
        return;
    }
    let Some(kind) = kind_of(path) else {
        return;
    };
    let known = snapshot
        .tracks
        .iter()
        .find(|track| track.path == path)
        .map(|track| track.item());
    let known = known.or_else(|| {
        snapshot
            .videos
            .iter()
            .find(|video| video.path == path)
            .map(|video| video.item())
    });
    items.push(known.unwrap_or_else(|| Item::for_path(path.to_owned(), kind)));
}

/// What the window draws from.
#[derive(Clone, Copy)]
pub struct Live {
    pub status: State<Status>,
    pub queue: State<Queue>,
    pub library: State<Arc<Snapshot>>,
    pub scanning: State<bool>,
}

/// Keeps the window's copy of the engine up to date. Call once, in the root.
pub fn use_live() -> Live {
    let live = use_hook(|| {
        let engine = engine();
        Live {
            status: State::create(engine.player.status()),
            queue: State::create(engine.player.queue()),
            library: State::create(engine.snapshot()),
            scanning: State::create(engine.is_scanning()),
        }
    });
    use_hook(move || {
        spawn(async move {
            let engine = engine();
            loop {
                engine.changed.next().await;
                engine.announced.store(false, Ordering::Release);
                let mut live = live;
                live.status.set_if_modified(engine.player.status());
                live.queue.set_if_modified(engine.player.queue());
                let snapshot = engine.snapshot();
                if !Arc::ptr_eq(&*live.library.peek(), &snapshot) {
                    live.library.set(snapshot);
                }
                live.scanning.set_if_modified(engine.is_scanning());
            }
        })
    });
    use_provide_context(|| live)
}

/// The window's copy, below the root.
pub fn use_media() -> Live {
    use_consume::<Live>()
}
