//! The player: one mpv instance, its picture and its queue.
//!
//! [`Player`] is cheap to clone and every method may be called from any
//! thread without waiting: a change is queued to mpv and comes back as a
//! [`Change`] once mpv has made it. The window and the MPRIS server both hold
//! a clone and read [`Player::status`] when told something changed.
//!
//! mpv is set up as a music and film player and nothing else: no
//! configuration files, scripts or key bindings of its own, no on-screen
//! display (the window draws the controls), sidecar subtitles found by name
//! (`film.srt`, `film.en.ass`), gapless audio, ReplayGain, exact seeking, and
//! the next file opened before the current one ends.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime};

use crate::mpv::{self, Event, Format, Handle, Node, Value};
use crate::queue::{Item, Kind, Op, Queue, Repeat};
use crate::render::{Backend, FrameSink, Renderer};
use crate::status::{self, PlayState, SleepLeft, Status};

/// What the app does around playback: resume points, play counts, scrobbles.
/// Every method is called on the player's event thread and may take a moment
/// (a database write), never on the window's thread.
pub trait Hooks: Send + Sync {
    /// Where to start `item`, for an item that was left part way through.
    fn resume_at(&self, _item: &Item) -> Option<f64> {
        None
    }
    /// How far into `item` playback is, while it plays and when it stops;
    /// `None` when it played to the end.
    fn save_position(&self, _item: &Item, _position: Option<f64>, _duration: Option<f64>) {}
    /// `item` started playing.
    fn started(&self, _item: &Item) {}
    /// `item` was listened to long enough to count as a play (half of it, or
    /// four minutes), which started at `at`.
    fn listened(&self, _item: &Item, _at: SystemTime) {}
}

/// No hooks.
pub struct NoHooks;
impl Hooks for NoHooks {}

/// What changed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Change {
    /// Anything in [`Status`].
    Status,
    /// The queue's items or order.
    Queue,
    /// The position jumped (a seek), to this many seconds.
    Seeked(f64),
}

/// How loudness is evened out between songs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Levelling {
    Off,
    /// Each song's ReplayGain: songs from different albums play at one level.
    #[default]
    Track,
    /// The album's ReplayGain: an album keeps its quiet and loud songs.
    Album,
    /// A filter that evens the level as it plays, for files with no ReplayGain.
    Automatic,
}

/// A sleep timer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sleep {
    Off,
    After(Duration),
    EndOfItem,
}

/// How exactly to seek.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Precision {
    /// To the exact moment (slower on a film: it decodes from the keyframe before).
    Exact,
    /// To the nearest keyframe: for a scrubber being dragged.
    Fast,
}

/// The player's starting settings.
#[derive(Clone, Debug)]
pub struct Options {
    pub volume: f64,
    pub muted: bool,
    pub levelling: Levelling,
    /// Preferred audio and subtitle languages, most preferred first.
    pub languages: Vec<String>,
    /// mpv's audio output (`pipewire`, `pulse`, `alsa`, `null`); empty: mpv's choice.
    pub audio_output: String,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            volume: 100.0,
            muted: false,
            levelling: Levelling::Track,
            languages: Vec::new(),
            audio_output: String::new(),
        }
    }
}

type Listener = Arc<dyn Fn(Change) + Send + Sync>;

/// A play being counted: when it started and how long it has played.
struct Listen {
    item_id: u64,
    at: SystemTime,
    seconds: f64,
    last_position: Option<f64>,
    counted: bool,
}

struct Timer {
    until: Option<Instant>,
    end_of_item: bool,
    /// The volume before the fade, while the last seconds fade out.
    fading_from: Option<f64>,
}

struct State {
    status: Status,
    queue: Queue,
    notified_position: f64,
    listen: Option<Listen>,
    timer: Option<Timer>,
    last_saved: Instant,
    seed: u64,
    paused: bool,
    /// mpv has nothing open.
    idle: bool,
}

struct Inner {
    handle: Arc<Handle>,
    renderer: Mutex<Option<Renderer>>,
    state: Mutex<State>,
    listeners: Mutex<Vec<Listener>>,
    hooks: Arc<dyn Hooks>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

/// The player. Clones share it; the last one dropped stops it.
#[derive(Clone)]
pub struct Player {
    inner: Arc<Inner>,
}

const PAUSE: u64 = 1;
const TIME_POS: u64 = 2;
const DURATION: u64 = 3;
const PLAYLIST_POS: u64 = 4;
const VOLUME: u64 = 5;
const MUTE: u64 = 6;
const SPEED: u64 = 7;
const TRACK_LIST: u64 = 8;
const CHAPTER_LIST: u64 = 9;
const CHAPTER: u64 = 10;
const CACHE: u64 = 11;
const SEEKING: u64 = 12;
const PAUSED_FOR_CACHE: u64 = 13;
const EOF_REACHED: u64 = 14;
const IDLE: u64 = 15;
const VIDEO_PARAMS: u64 = 16;
const METADATA: u64 = 17;
const LOOP_A: u64 = 18;
const LOOP_B: u64 = 19;

/// The position is told to listeners when it moved this far (seconds), so a
/// playing song redraws its timeline a few times a second, not every frame.
const POSITION_STEP: f64 = 0.25;
/// How often the resume point of a film is written while it plays.
const SAVE_EVERY: Duration = Duration::from_secs(10);
/// The sleep timer's fade-out.
const SLEEP_FADE: Duration = Duration::from_secs(8);
/// Previous goes to the start of the item after this many seconds.
const RESTART_AFTER: f64 = 3.0;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Player {
    /// Starts mpv, its render thread (drawing into `sink`) and its event thread.
    pub fn new(
        sink: Arc<dyn FrameSink>,
        hooks: Arc<dyn Hooks>,
        options: &Options,
    ) -> Result<Player, mpv::Error> {
        let volume = format!("{:.0}", options.volume.clamp(0.0, 100.0));
        let languages = options.languages.join(",");
        let mut settings: Vec<(&str, &str)> = vec![
            ("vo", "libmpv"),
            ("idle", "yes"),
            ("keep-open", "yes"),
            ("config", "no"),
            ("load-scripts", "no"),
            ("ytdl", "no"),
            ("osc", "no"),
            ("terminal", "no"),
            ("input-default-bindings", "no"),
            ("input-vo-keyboard", "no"),
            ("osd-level", "0"),
            ("osd-bar", "no"),
            ("audio-display", "no"),
            ("audio-client-name", "Media"),
            ("gapless-audio", "weak"),
            ("prefetch-playlist", "yes"),
            ("hr-seek", "yes"),
            ("sub-auto", "fuzzy"),
            ("audio-file-auto", "no"),
            ("hwdec", "auto-safe"),
            ("volume-max", "100"),
            ("replaygain-clip", "no"),
            ("save-position-on-quit", "no"),
            // 16-bit integer intermediate frames: float ones draw black on
            // some drivers (llvmpipe in a surfaceless context), and SDR loses
            // nothing.
            ("fbo-format", "rgba16"),
            ("msg-level", "all=fatal"),
        ];
        settings.push(("volume", &volume));
        settings.push(("mute", if options.muted { "yes" } else { "no" }));
        if !languages.is_empty() {
            settings.push(("alang", &languages));
            settings.push(("slang", &languages));
        }
        if !options.audio_output.is_empty() {
            settings.push(("ao", &options.audio_output));
        }
        // BINGUX_MEDIA_MPV_LOG=v (or debug) prints mpv's own log on stderr.
        let log = std::env::var("BINGUX_MEDIA_MPV_LOG")
            .ok()
            .map(|level| format!("all={level}"));
        if let Some(log) = &log {
            settings.push(("terminal", "yes"));
            settings.push(("msg-level", log));
        }
        // BINGUX_MEDIA_MPV_OPTIONS="name=value;name=value" sets any other mpv
        // option, last (for trying a driver workaround without a rebuild).
        let extra = std::env::var("BINGUX_MEDIA_MPV_OPTIONS").unwrap_or_default();
        for pair in extra.split(';') {
            if let Some((name, value)) = pair.split_once('=') {
                settings.push((name.trim(), value.trim()));
            }
        }
        let handle = Arc::new(Handle::new(&settings)?);
        for (id, name, format) in [
            (PAUSE, "pause", Format::Flag),
            (TIME_POS, "time-pos", Format::Double),
            (DURATION, "duration", Format::Double),
            (PLAYLIST_POS, "playlist-pos", Format::Int),
            (VOLUME, "volume", Format::Double),
            (MUTE, "mute", Format::Flag),
            (SPEED, "speed", Format::Double),
            (TRACK_LIST, "track-list", Format::Node),
            (CHAPTER_LIST, "chapter-list", Format::Node),
            (CHAPTER, "chapter", Format::Int),
            (CACHE, "demuxer-cache-state", Format::Node),
            (SEEKING, "seeking", Format::Flag),
            (PAUSED_FOR_CACHE, "paused-for-cache", Format::Flag),
            (EOF_REACHED, "eof-reached", Format::Flag),
            (IDLE, "idle-active", Format::Flag),
            (VIDEO_PARAMS, "video-params", Format::Node),
            (METADATA, "metadata", Format::Node),
            (LOOP_A, "ab-loop-a", Format::Node),
            (LOOP_B, "ab-loop-b", Format::Node),
        ] {
            handle.observe(id, name, format);
        }
        handle.add_hook("on_load");
        let renderer = Renderer::start(handle.clone(), sink);
        let status = Status {
            volume: options.volume,
            muted: options.muted,
            speed: 1.0,
            ..Status::default()
        };
        let inner = Arc::new(Inner {
            handle: handle.clone(),
            renderer: Mutex::new(Some(renderer)),
            state: Mutex::new(State {
                status,
                queue: Queue::default(),
                notified_position: 0.0,
                listen: None,
                timer: None,
                last_saved: Instant::now(),
                seed: SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .map(|time| time.as_nanos() as u64)
                    .unwrap_or(1),
                paused: false,
                idle: true,
            }),
            listeners: Mutex::new(Vec::new()),
            hooks,
            thread: Mutex::new(None),
        });
        let player = Player { inner };
        player.set_levelling(options.levelling);
        let weak = Arc::downgrade(&player.inner);
        let thread = std::thread::Builder::new()
            .name("media-events".into())
            .spawn(move || event_loop(handle, weak))
            .ok();
        *lock(&player.inner.thread) = thread;
        Ok(player)
    }

    /// Calls `listener` on the player's event thread after each change.
    pub fn subscribe(&self, listener: impl Fn(Change) + Send + Sync + 'static) {
        lock(&self.inner.listeners).push(Arc::new(listener));
    }

    pub fn status(&self) -> Status {
        lock(&self.inner.state).status.clone()
    }

    pub fn queue(&self) -> Queue {
        lock(&self.inner.state).queue.clone()
    }

    /// The renderer that draws the picture.
    pub fn backend(&self) -> Option<Backend> {
        lock(&self.inner.renderer)
            .as_ref()
            .and_then(Renderer::backend)
    }

    /// The window shows the picture at a new size.
    pub fn picture_resized(&self) {
        if let Some(renderer) = lock(&self.inner.renderer).as_ref() {
            renderer.resized();
        }
    }

    fn mpv(&self) -> &Handle {
        &self.inner.handle
    }

    fn apply(&self, ops: Vec<Op>) {
        let handle = self.mpv();
        for op in ops {
            match op {
                Op::Replace(paths, start) => {
                    handle.command_async(0, &["stop"]);
                    for path in &paths {
                        handle.command_async_bytes(0, &[b"loadfile", path_bytes(path), b"append"]);
                    }
                    handle.command_async(0, &["playlist-play-index", &start.to_string()]);
                    handle.set_async("pause", "no");
                }
                Op::Append(path) => {
                    handle.command_async_bytes(0, &[b"loadfile", path_bytes(&path), b"append"])
                }
                Op::Move(from, to) => {
                    handle.command_async(0, &["playlist-move", &from.to_string(), &to.to_string()])
                }
                Op::Remove(index) => {
                    handle.command_async(0, &["playlist-remove", &index.to_string()])
                }
                Op::PlayIndex(index) => {
                    handle.command_async(0, &["playlist-play-index", &index.to_string()]);
                    handle.set_async("pause", "no");
                }
                Op::Stop => handle.command_async(0, &["stop"]),
            }
        }
    }

    fn change_queue(&self, change: impl FnOnce(&mut Queue, u64) -> Vec<Op>) {
        let ops = {
            let mut state = lock(&self.inner.state);
            state.seed = state
                .seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let seed = state.seed;
            let ops = change(&mut state.queue, seed);
            let current = state.queue.current();
            state.status.current = current;
            state.status.item = state.queue.current_item().cloned();
            state.status.shuffle = state.queue.shuffle();
            state.status.ended = false;
            ops
        };
        self.apply(ops);
        notify(&self.inner, Change::Queue);
        notify(&self.inner, Change::Status);
    }

    /// Plays `items` from `start`, replacing the queue.
    pub fn play_items(&self, items: Vec<Item>, start: usize) {
        self.change_queue(|queue, seed| queue.replace(items, start, seed));
    }

    /// Adds to the queue: after the current item (`next`) or at the end.
    pub fn enqueue(&self, items: Vec<Item>, next: bool) {
        self.change_queue(|queue, _| queue.add(items, next));
    }

    pub fn play_index(&self, index: usize) {
        self.change_queue(|queue, _| queue.play(index));
    }

    pub fn remove(&self, index: usize) {
        self.change_queue(|queue, _| queue.remove(index));
    }

    pub fn reorder(&self, from: usize, to: usize) {
        self.change_queue(|queue, _| queue.reorder(from, to));
    }

    pub fn clear_upcoming(&self) {
        self.change_queue(|queue, _| queue.clear_upcoming());
    }

    pub fn clear(&self) {
        self.change_queue(|queue, _| queue.clear());
    }

    pub fn set_shuffle(&self, on: bool) {
        self.change_queue(|queue, seed| queue.set_shuffle(on, seed));
    }

    pub fn set_repeat(&self, repeat: Repeat) {
        {
            let mut state = lock(&self.inner.state);
            state.queue.set_repeat(repeat);
            state.status.repeat = repeat;
        }
        let (playlist, file) = match repeat {
            Repeat::Off => ("no", "no"),
            Repeat::All => ("inf", "no"),
            Repeat::One => ("no", "inf"),
        };
        self.mpv().set_async("loop-playlist", playlist);
        self.mpv().set_async("loop-file", file);
        notify(&self.inner, Change::Status);
    }

    pub fn play(&self) {
        let (ended, empty) = {
            let state = lock(&self.inner.state);
            (state.status.ended, state.queue.items().is_empty())
        };
        if empty {
            return;
        }
        if ended {
            // Play after the end starts the queue again.
            self.play_index(0);
            return;
        }
        self.mpv().set_async("pause", "no");
    }

    pub fn pause(&self) {
        self.mpv().set_async("pause", "yes");
    }

    pub fn toggle(&self) {
        let state = lock(&self.inner.state).status.state;
        match state {
            PlayState::Playing => self.pause(),
            _ => self.play(),
        }
    }

    /// Stops and empties the queue.
    pub fn stop(&self) {
        self.clear();
    }

    pub fn next(&self) {
        self.mpv().command_async(0, &["playlist-next", "force"]);
    }

    /// The start of the item, or the item before when near its start.
    pub fn previous(&self) {
        let (position, current) = {
            let state = lock(&self.inner.state);
            (state.status.position, state.queue.current())
        };
        if position > RESTART_AFTER || current.unwrap_or(0) == 0 {
            self.seek(0.0, Precision::Exact);
        } else {
            self.mpv().command_async(0, &["playlist-prev", "force"]);
        }
    }

    /// Seeks to `seconds` from the start.
    pub fn seek(&self, seconds: f64, precision: Precision) {
        let flags = match precision {
            Precision::Exact => "absolute+exact",
            Precision::Fast => "absolute+keyframes",
        };
        {
            let mut state = lock(&self.inner.state);
            let duration = state.status.duration.unwrap_or(f64::MAX);
            state.status.position = seconds.clamp(0.0, duration);
            state.notified_position = state.status.position;
        }
        self.mpv()
            .command_async(0, &["seek", &format!("{:.3}", seconds.max(0.0)), flags]);
        notify(&self.inner, Change::Status);
    }

    /// Seeks by `delta` seconds (negative: back).
    pub fn seek_by(&self, delta: f64) {
        let target = {
            let state = lock(&self.inner.state);
            let end = state.status.duration.unwrap_or(f64::MAX);
            (state.status.position + delta).clamp(0.0, end)
        };
        self.seek(target, Precision::Exact);
    }

    /// Seeks to a fraction (0 to 1) of the item.
    pub fn seek_fraction(&self, fraction: f64, precision: Precision) {
        let duration = lock(&self.inner.state).status.duration;
        if let Some(duration) = duration {
            self.seek(duration * fraction.clamp(0.0, 1.0), precision);
        }
    }

    /// 0 to 100.
    pub fn set_volume(&self, volume: f64) {
        let volume = volume.clamp(0.0, 100.0);
        {
            let mut state = lock(&self.inner.state);
            state.status.volume = volume;
            // A volume change cancels a sleep fade's hold on the volume.
            if let Some(timer) = state.timer.as_mut() {
                timer.fading_from = None;
            }
        }
        self.mpv().set_async("volume", &format!("{volume:.1}"));
        if volume > 0.0 {
            self.mpv().set_async("mute", "no");
        }
        notify(&self.inner, Change::Status);
    }

    pub fn set_muted(&self, muted: bool) {
        lock(&self.inner.state).status.muted = muted;
        self.mpv()
            .set_async("mute", if muted { "yes" } else { "no" });
        notify(&self.inner, Change::Status);
    }

    /// Playback speed; the pitch stays (mpv's `scaletempo2`).
    pub fn set_speed(&self, speed: f64) {
        let speed = speed.clamp(0.25, 4.0);
        lock(&self.inner.state).status.speed = speed;
        self.mpv().set_async("speed", &format!("{speed:.2}"));
        notify(&self.inner, Change::Status);
    }

    pub fn select_audio(&self, id: i64) {
        self.mpv().set_async("aid", &id.to_string());
    }

    /// A subtitle stream, or none.
    pub fn select_subtitle(&self, id: Option<i64>) {
        self.mpv()
            .set_async("sid", &id.map_or("no".to_owned(), |id| id.to_string()));
    }

    /// Loads a subtitle file for the film playing and shows it.
    pub fn add_subtitle(&self, path: &Path) {
        self.mpv()
            .command_async_bytes(0, &[b"sub-add", path_bytes(path), b"select"]);
    }

    /// Moves the subtitles in time (seconds; positive shows them later).
    pub fn set_subtitle_delay(&self, seconds: f64) {
        self.mpv().set_async("sub-delay", &format!("{seconds:.3}"));
    }

    pub fn go_to_chapter(&self, index: usize) {
        self.mpv().set_async("chapter", &index.to_string());
    }

    /// Sets the A point of a loop, then the B point, then clears both.
    pub fn cycle_loop(&self) {
        self.mpv().command_async(0, &["ab-loop"]);
    }

    /// Steps one frame forward (pausing) or back.
    pub fn frame_step(&self, forward: bool) {
        self.mpv().command_async(
            0,
            &[if forward {
                "frame-step"
            } else {
                "frame-back-step"
            }],
        );
    }

    pub fn set_levelling(&self, levelling: Levelling) {
        let handle = self.mpv();
        let (replaygain, filter) = match levelling {
            Levelling::Off => ("no", ""),
            Levelling::Track => ("track", ""),
            Levelling::Album => ("album", ""),
            Levelling::Automatic => ("no", "@level:lavfi=[dynaudnorm=f=250:g=15:p=0.9]"),
        };
        handle.set_async("replaygain", replaygain);
        // The player's only audio filter: setting the list replaces it.
        handle.set_async("af", filter);
    }

    pub fn set_sleep(&self, sleep: Sleep) {
        {
            let mut state = lock(&self.inner.state);
            if let Some(Timer {
                fading_from: Some(volume),
                ..
            }) = state.timer.take()
            {
                self.mpv().set_async("volume", &format!("{volume:.1}"));
            }
            state.timer = match sleep {
                Sleep::Off => None,
                Sleep::After(after) => Some(Timer {
                    until: Some(Instant::now() + after),
                    end_of_item: false,
                    fading_from: None,
                }),
                Sleep::EndOfItem => Some(Timer {
                    until: None,
                    end_of_item: true,
                    fading_from: None,
                }),
            };
            state.status.sleep = sleep_left(state.timer.as_ref());
        }
        notify(&self.inner, Change::Status);
    }

    /// Stops the threads. Also done when the last clone is dropped.
    pub fn shutdown(&self) {
        save_now(&self.inner, false);
        self.mpv().command_async(0, &["quit"]);
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        // The renderer frees its context before the handle goes.
        lock(&self.renderer).take();
        self.handle.command_async(0, &["quit"]);
        if let Some(thread) = lock(&self.thread).take() {
            let _ = thread.join();
        }
    }
}

fn path_bytes(path: &Path) -> &[u8] {
    use std::os::unix::ffi::OsStrExt;
    path.as_os_str().as_bytes()
}

fn notify(inner: &Inner, change: Change) {
    let listeners: Vec<Listener> = lock(&inner.listeners).clone();
    for listener in listeners {
        listener(change);
    }
}

fn play_state(paused: bool, idle: bool) -> PlayState {
    match (idle, paused) {
        (true, _) => PlayState::Stopped,
        (false, true) => PlayState::Paused,
        (false, false) => PlayState::Playing,
    }
}

fn sleep_left(timer: Option<&Timer>) -> Option<SleepLeft> {
    let timer = timer?;
    if timer.end_of_item {
        return Some(SleepLeft::EndOfItem);
    }
    let left = timer.until?.saturating_duration_since(Instant::now());
    Some(SleepLeft::Seconds(left.as_secs() as u32))
}

/// Whether an item's place is worth remembering: films, and long recordings.
fn resumable(item: &Item, duration: Option<f64>) -> bool {
    item.kind == Kind::Video || duration.is_some_and(|duration| duration > 20.0 * 60.0)
}

fn save_now(inner: &Inner, finished: bool) {
    let (item, position, duration) = {
        let mut state = lock(&inner.state);
        state.last_saved = Instant::now();
        let Some(item) = state.status.item.clone() else {
            return;
        };
        (item, state.status.position, state.status.duration)
    };
    if resumable(&item, duration) {
        inner
            .hooks
            .save_position(&item, (!finished).then_some(position), duration);
    }
}

fn event_loop(handle: Arc<Handle>, inner: std::sync::Weak<Inner>) {
    loop {
        let event = handle.wait_event(0.5);
        if matches!(event, Event::Shutdown) {
            return;
        }
        let Some(inner) = inner.upgrade() else {
            return;
        };
        let mut changes: Vec<Change> = Vec::new();
        match event {
            Event::Property { id, value, .. } => property(&inner, id, value, &mut changes),
            Event::Hook { name, id } => {
                if name == "on_load" {
                    resume_point(&inner, &handle);
                }
                handle.continue_hook(id);
            }
            Event::StartFile(_) => {
                let mut state = lock(&inner.state);
                let status = &mut state.status;
                status.tracks.clear();
                status.chapters.clear();
                status.chapter = None;
                status.buffered.clear();
                status.error = None;
                status.tags = Default::default();
                status.has_video = false;
                status.video_size = None;
                status.ended = false;
                changes.push(Change::Status);
            }
            Event::FileLoaded => {
                let item = {
                    let mut state = lock(&inner.state);
                    let item = state.status.item.clone();
                    state.listen = item.as_ref().map(|item| Listen {
                        item_id: item.id,
                        at: SystemTime::now(),
                        seconds: 0.0,
                        last_position: None,
                        counted: false,
                    });
                    item
                };
                if let Some(item) = item {
                    inner.hooks.started(&item);
                }
            }
            Event::EndFile { eof, error, .. } => {
                save_now(&inner, eof);
                let mut state = lock(&inner.state);
                if let Some(error) = error {
                    let name = state
                        .status
                        .item
                        .as_ref()
                        .map(|item| item.title.clone())
                        .unwrap_or_default();
                    eprintln!("[media] cannot play {name}: {error}");
                    state.status.error = Some(error);
                    changes.push(Change::Status);
                }
                if eof && state.timer.as_ref().is_some_and(|timer| timer.end_of_item) {
                    state.timer = None;
                    state.status.sleep = None;
                    drop(state);
                    handle.set_async("pause", "yes");
                    changes.push(Change::Status);
                }
            }
            Event::Seek => {}
            Event::PlaybackRestart => {
                let position = {
                    let mut state = lock(&inner.state);
                    state.status.buffering = false;
                    if let Some(listen) = state.listen.as_mut() {
                        listen.last_position = None;
                    }
                    state.status.position
                };
                changes.push(Change::Seeked(position));
                changes.push(Change::Status);
            }
            _ => {}
        }
        tick(&inner, &handle, &mut changes);
        changes.dedup();
        for change in changes {
            notify(&inner, change);
        }
    }
}

/// The resume point of the file about to open, set as its start.
fn resume_point(inner: &Inner, handle: &Handle) {
    let Some(Node::String(path)) = handle.get_node("path") else {
        return;
    };
    let item = {
        let state = lock(&inner.state);
        let path = PathBuf::from(&path);
        let current = state
            .queue
            .current_item()
            .filter(|item| item.path == path)
            .cloned();
        current.or_else(|| {
            state
                .queue
                .items()
                .iter()
                .find(|item| item.path == path)
                .cloned()
        })
    };
    let Some(item) = item else {
        return;
    };
    if let Some(seconds) = inner.hooks.resume_at(&item) {
        let _ = handle.set("file-local-options/start", &format!("{seconds:.3}"));
    }
}

fn property(inner: &Inner, id: u64, value: Value, changes: &mut Vec<Change>) {
    let mut state = lock(&inner.state);
    let state = &mut *state;
    let status = &mut state.status;
    let flag = |value: &Value| matches!(value, Value::Flag(true));
    match id {
        PAUSE => {
            state.paused = flag(&value);
            status.state = play_state(state.paused, state.idle);
            changes.push(Change::Status);
        }
        TIME_POS => {
            if let Value::Double(position) = value {
                status.position = position;
                if let Some(listen) = state.listen.as_mut() {
                    if let Some(last) = listen.last_position {
                        let step = position - last;
                        // A jump is a seek, not listening.
                        if step > 0.0 && step < 2.0 {
                            listen.seconds += step;
                        }
                    }
                    listen.last_position = Some(position);
                }
                if (position - state.notified_position).abs() >= POSITION_STEP {
                    state.notified_position = position;
                    changes.push(Change::Status);
                }
            }
        }
        DURATION => {
            status.duration = match value {
                Value::Double(duration) if duration > 0.0 => Some(duration),
                _ => None,
            };
            changes.push(Change::Status);
        }
        PLAYLIST_POS => {
            let index = match value {
                Value::Int(index) if index >= 0 => Some(index as usize),
                _ => None,
            };
            if index.is_some() || status.item.is_some() {
                state.queue.follow(index);
                status.current = state.queue.current();
                status.item = state.queue.current_item().cloned();
                status.position = 0.0;
                state.notified_position = 0.0;
                changes.push(Change::Status);
                changes.push(Change::Queue);
            }
        }
        VOLUME => {
            if let Value::Double(volume) = value
                && state
                    .timer
                    .as_ref()
                    .is_none_or(|timer| timer.fading_from.is_none())
            {
                status.volume = volume;
                changes.push(Change::Status);
            }
        }
        MUTE => {
            status.muted = flag(&value);
            changes.push(Change::Status);
        }
        SPEED => {
            if let Value::Double(speed) = value {
                status.speed = speed;
                changes.push(Change::Status);
            }
        }
        TRACK_LIST => {
            if let Value::Node(node) = value {
                status.tracks = status::tracks_of(&node);
                status.has_video = status
                    .tracks
                    .iter()
                    .any(|track| track.kind == status::TrackKind::Video && !track.album_art);
                changes.push(Change::Status);
            }
        }
        CHAPTER_LIST => {
            status.chapters = match value {
                Value::Node(node) => status::chapters_of(&node),
                _ => Vec::new(),
            };
            changes.push(Change::Status);
        }
        CHAPTER => {
            status.chapter = match value {
                Value::Int(index) if index >= 0 => Some(index as usize),
                _ => None,
            };
            changes.push(Change::Status);
        }
        CACHE => {
            let buffered = match value {
                Value::Node(node) => status::buffered_of(&node),
                _ => Vec::new(),
            };
            // Ranges move by fractions of a second all the time; tell only
            // a change a person could see.
            let visible = buffered.len() != status.buffered.len()
                || buffered
                    .iter()
                    .zip(&status.buffered)
                    .any(|(a, b)| (a.0 - b.0).abs() > 1.0 || (a.1 - b.1).abs() > 1.0);
            if visible {
                status.buffered = buffered;
                changes.push(Change::Status);
            }
        }
        SEEKING | PAUSED_FOR_CACHE => {
            let busy = flag(&value);
            if status.buffering != busy {
                status.buffering = busy;
                changes.push(Change::Status);
            }
        }
        EOF_REACHED => {
            // keep-open pauses on the last frame of the last item.
            let ended = flag(&value)
                && state
                    .queue
                    .current()
                    .is_some_and(|current| current + 1 >= state.queue.items().len());
            if ended != status.ended {
                status.ended = ended;
                changes.push(Change::Status);
            }
        }
        IDLE => {
            state.idle = flag(&value);
            status.state = play_state(state.paused, state.idle);
            if state.idle {
                status.position = 0.0;
                status.duration = None;
                if state.queue.items().is_empty() {
                    status.item = None;
                    status.current = None;
                }
            }
            changes.push(Change::Status);
        }
        VIDEO_PARAMS => {
            status.video_size = match &value {
                Value::Node(node) => node
                    .int("dw")
                    .zip(node.int("dh"))
                    .filter(|(width, height)| *width > 0 && *height > 0)
                    .map(|(width, height)| (width as u32, height as u32)),
                _ => None,
            };
            changes.push(Change::Status);
        }
        METADATA => {
            if let Value::Node(node) = value {
                status.tags = status::tags_of(&node);
                // A file opened by itself (dragged in) is named by its tags.
                let tags = status.tags.clone();
                if let Some(current) = state.queue.current() {
                    let mut items: Vec<Item> = state.queue.items().to_vec();
                    if let Some(item) = items.get_mut(current)
                        && item.artist.is_empty()
                        && item.album.is_empty()
                        && tags.title.is_some()
                    {
                        if let Some(title) = tags.title.clone() {
                            item.title = title;
                        }
                        item.artist = tags.artist.clone().unwrap_or_default();
                        item.album = tags.album.clone().unwrap_or_default();
                        let updated = item.clone();
                        state.queue.update_item(updated.clone());
                        state.status.item = Some(updated);
                        changes.push(Change::Queue);
                    }
                }
                changes.push(Change::Status);
            }
        }
        LOOP_A | LOOP_B => {
            let point = match value {
                Value::Node(Node::Double(seconds)) => Some(seconds),
                Value::Node(Node::Int(seconds)) => Some(seconds as f64),
                _ => None,
            };
            if id == LOOP_A {
                status.loop_a = point;
            } else {
                status.loop_b = point;
            }
            changes.push(Change::Status);
        }
        _ => {}
    }
}

/// Work the event thread does every wake-up: count plays, save the resume
/// point, run the sleep timer.
fn tick(inner: &Inner, handle: &Handle, changes: &mut Vec<Change>) {
    let mut counted: Option<(Item, SystemTime)> = None;
    let mut save = false;
    {
        let mut state = lock(&inner.state);
        let state = &mut *state;
        let playing = state.status.state == PlayState::Playing;
        // A play counts after half of the item, or four minutes, of listening.
        if let (Some(listen), Some(item)) = (state.listen.as_mut(), state.status.item.as_ref())
            && listen.item_id == item.id
            && !listen.counted
        {
            let duration = state.status.duration.or(item.duration).unwrap_or(0.0);
            if duration > 30.0 && listen.seconds >= (duration / 2.0).min(240.0) {
                listen.counted = true;
                counted = Some((item.clone(), listen.at));
            }
        }
        if playing && state.last_saved.elapsed() >= SAVE_EVERY {
            save = true;
        }
        if let Some(timer) = state.timer.as_mut()
            && let Some(until) = timer.until
        {
            let now = Instant::now();
            let left = until.saturating_duration_since(now);
            if left.is_zero() {
                let volume = timer.fading_from.unwrap_or(state.status.volume);
                state.timer = None;
                handle.set_async("pause", "yes");
                handle.set_async("volume", &format!("{volume:.1}"));
                state.status.volume = volume;
            } else if left <= SLEEP_FADE && playing {
                let from = *timer.fading_from.get_or_insert(state.status.volume);
                let level = from * left.as_secs_f64() / SLEEP_FADE.as_secs_f64();
                handle.set_async("volume", &format!("{level:.1}"));
            }
            let shown = sleep_left(state.timer.as_ref());
            if shown != state.status.sleep {
                state.status.sleep = shown;
                changes.push(Change::Status);
            }
        }
    }
    if let Some((item, at)) = counted {
        inner.hooks.listened(&item, at);
    }
    if save {
        save_now(inner, false);
    }
}
