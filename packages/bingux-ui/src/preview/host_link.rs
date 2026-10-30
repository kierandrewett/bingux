//! The link to `bingux-preview-host`: the process that plays media and renders
//! documents for a preview.
//!
//! A [`HostLink`] starts the helper, talks to it, and ends it. It never blocks
//! the UI thread: everything that waits (the process starting, the file
//! opening, the socket) is on two threads of the link, and the component reads
//! what arrived from a [`Feed`], which wakes its task and nothing else.
//!
//! * The **supervisor** thread starts the process (so `PR_SET_PDEATHSIG`
//!   ties its life to a thread that outlives it, see
//!   [`die_with_parent`](crate::runtime::die_with_parent)), opens files, writes
//!   packets from a bounded queue, watches the open time and ends the process.
//! * The **reader** thread reads packets. A video frame or a page is copied out
//!   of its shared memory there, the host is told it may reuse the slot, and
//!   the UI gets the pixels. Only the newest frame is kept: a UI that is behind
//!   skips pictures, it never queues them.
//!
//! The helper dying, crashing, not being installed or taking too long is an
//! [`LinkEvent::Died`]: the viewer shows an error state with "Open with", and the
//! file manager is untouched. At most [`MAX_LIVE`] helpers live in a process
//! (more only after [`allow_helpers`]); starting another ends the oldest.

use std::collections::VecDeque;
use std::fs::File;
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, LazyLock, Mutex, MutexGuard, Weak};
use std::time::{Duration, Instant};

use bingux_preview_core::host::channel::{Channel, RecvError};
use bingux_preview_core::host::protocol::{ErrorCode, Message, Packet, VERSION};
use bingux_preview_core::host::shm::Shared;
use bytes::Bytes;

use crate::task::Feed;

/// How many helpers one process runs at once unless it asks for more with
/// [`allow_helpers`]: a docked pane and a popup.
pub const MAX_LIVE: usize = 2;

static LIMIT: AtomicUsize = AtomicUsize::new(MAX_LIVE);

/// Let this process run `count` helpers at once instead of [`MAX_LIVE`]. For a
/// process that shows many previews side by side, the taxonomy's page of
/// specimens and the tests: with the default, mounting a third preview ends the
/// oldest and it shows "stopped to make room". It never lowers the limit; an app
/// that shows one pane and one popup has no use for it.
pub fn allow_helpers(count: usize) {
    LIMIT.fetch_max(count, Ordering::Relaxed);
}

/// How long the helper has to open a file before the link gives up.
const OPEN_TIME: Duration = Duration::from_secs(15);
/// How long a closing helper has to leave before it is killed.
const LEAVE_TIME: Duration = Duration::from_millis(600);
/// The most packets waiting to be sent.
const OUTBOX_LIMIT: usize = 128;
/// The environment a helper is given; everything else is dropped.
const ENV_KEEP: &[&str] = &[
    "XDG_RUNTIME_DIR",
    "XDG_CACHE_HOME",
    "XDG_DATA_DIRS",
    "HOME",
    "LANG",
    "LC_ALL",
    "LC_MESSAGES",
    "DBUS_SESSION_BUS_ADDRESS",
    "BINGUX_PREVIEW_DEBUG",
];
/// Variables the helper is given when they start with one of these.
const ENV_PREFIXES: &[&str] = &["GST_", "PIPEWIRE_", "PULSE_"];

/// A decoded picture: a video frame.
#[derive(Clone)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub pts_ns: u64,
    /// `width * height * 4` bytes of opaque RGBA.
    pub pixels: Bytes,
}

/// A rendered page.
#[derive(Clone)]
pub struct PageImage {
    /// The request that asked for it.
    pub request: u32,
    pub page: u32,
    pub width: u32,
    pub height: u32,
    pub pixels: Bytes,
}

/// Why the link ended on its own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub code: ErrorCode,
    /// In words for a person.
    pub message: String,
}

/// What a [`HostLink`] tells its owner.
pub enum LinkEvent {
    /// A packet that is not pixels: `Opened`, `State`, `Position`, `Error`, ...
    Packet(Packet),
    /// A newer frame is waiting in [`HostLink::take_frame`].
    Frame,
    Page(PageImage),
    /// The helper is gone: it was not installed, it crashed, it was too slow,
    /// or it was ended to make room. Nothing more arrives.
    Died(Failure),
}

/// What the supervisor sends.
enum Outgoing {
    Packet(Packet),
    /// Open `file` and send the open request with it. For media, a sidecar
    /// subtitle file beside it (`name.srt`, `name.vtt`) goes along when there is one.
    OpenFiles { packet: Packet, file: PathBuf },
}

#[derive(Default)]
struct Outbox {
    queue: VecDeque<Outgoing>,
    closing: bool,
}

struct Inner {
    outbox: Mutex<Outbox>,
    wake: Condvar,
    events: Feed<LinkEvent>,
    frame: Mutex<Option<Frame>>,
    /// A `Frame` event is in the feed and has not been taken.
    frame_announced: AtomicBool,
    /// The link has ended (the helper is gone or was ended).
    over: AtomicBool,
    /// The reader thread has ended: the socket is closed.
    reader_done: AtomicBool,
    /// Milliseconds since `start` by which the open must have been answered,
    /// plus one; zero when no open is waiting.
    open_deadline: AtomicU64,
    start: Instant,
    /// The process id, for the tests and for the debug log.
    pid: Mutex<Option<u32>>,
}

/// The helper and the way to talk to it. Dropping it ends the helper.
pub struct HostLink {
    inner: Arc<Inner>,
}

static LIVE: LazyLock<Mutex<Vec<Weak<Inner>>>> = LazyLock::new(|| Mutex::new(Vec::new()));

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The helper's program, found the way a packaged install lays it out: the
/// `BINGUX_PREVIEW_HOST` variable (a file that must exist), else the copy beside
/// this executable (`/usr/bin`, or the `bin` of an install prefix, where the
/// packages put both), else a directory on `PATH`. In a debug build run from a
/// cargo target directory the copy beside `deps/` counts too, so `cargo test`
/// finds the helper built next to it; a release build never looks there.
pub fn host_program() -> Option<PathBuf> {
    host_program_from(std::env::var_os("BINGUX_PREVIEW_HOST").map(PathBuf::from), std::env::current_exe().ok(), std::env::var_os("PATH"))
}

fn host_program_from(explicit: Option<PathBuf>, exe: Option<PathBuf>, path: Option<std::ffi::OsString>) -> Option<PathBuf> {
    const NAME: &str = "bingux-preview-host";
    if let Some(explicit) = explicit.filter(|path| !path.as_os_str().is_empty()) {
        return explicit.is_file().then_some(explicit);
    }
    if let Some(dir) = exe.as_deref().and_then(Path::parent) {
        let beside = dir.join(NAME);
        if beside.is_file() {
            return Some(beside);
        }
        // `cargo test` runs from `target/<profile>/deps`; the binary is a level up.
        #[cfg(debug_assertions)]
        {
            if dir.file_name().is_some_and(|name| name == "deps")
                && let Some(above) = dir.parent().map(|parent| parent.join(NAME)).filter(|candidate| candidate.is_file())
            {
                return Some(above);
            }
        }
    }
    std::env::split_paths(&path?).map(|dir| dir.join(NAME)).find(|candidate| candidate.is_file())
}

/// Whether previews of films, songs and documents can run: the helper is installed.
pub fn host_available() -> bool {
    host_program().is_some()
}

impl HostLink {
    /// Start a helper. Returns at once; the process starts on the link's own
    /// thread, and a helper that cannot start is an [`LinkEvent::Died`].
    pub fn start() -> HostLink {
        let inner = Arc::new(Inner::new());
        make_room(&inner);
        let thread = Arc::clone(&inner);
        let spawned = std::thread::Builder::new().name("bingux-preview-link".into()).spawn(move || supervise(&thread));
        if let Err(error) = spawned {
            inner.finish(Failure { code: ErrorCode::Internal, message: format!("The preview could not start ({error}).") });
        }
        HostLink { inner }
    }

    /// Ask the helper to play `file` with frames at most `max` pixels. A
    /// sidecar subtitle file goes along when there is one.
    pub fn open_media(&self, file: &Path, want_video: bool, max: (u32, u32), autoplay: bool) -> u32 {
        let request = 1;
        self.inner.push(Outgoing::OpenFiles {
            packet: Packet { request, message: Message::OpenMedia { want_video, max_w: max.0, max_h: max.1, autoplay, subtitle_file: false } },
            file: file.to_owned(),
        });
        request
    }

    pub fn open_document(&self, file: &Path) -> u32 {
        let request = 1;
        self.inner.push(Outgoing::OpenFiles { packet: Packet { request, message: Message::OpenDocument }, file: file.to_owned() });
        request
    }

    /// Queue a packet. Repeated seeks, sizes and volumes replace the one that
    /// is still waiting; the queue is bounded.
    pub fn send(&self, packet: Packet) {
        self.inner.push(Outgoing::Packet(packet));
    }

    pub fn send_all(&self, packets: impl IntoIterator<Item = Packet>) {
        for packet in packets {
            self.send(packet);
        }
    }

    /// The next event, waiting for one; `None` once the link has ended and
    /// everything it said was read.
    pub async fn next(&self) -> Option<LinkEvent> {
        self.inner.events.next().await
    }

    /// The newest frame, if one arrived since the last call.
    pub fn take_frame(&self) -> Option<Frame> {
        self.inner.frame_announced.store(false, Ordering::Release);
        lock(&self.inner.frame).take()
    }

    /// Whether the helper has ended.
    pub fn is_over(&self) -> bool {
        self.inner.over.load(Ordering::Acquire) || self.inner.reader_done.load(Ordering::Acquire)
    }

    /// The helper's process id once it started: for tests and the debug log.
    pub fn pid(&self) -> Option<u32> {
        *lock(&self.inner.pid)
    }

    /// Ask the helper to leave and end it. Called when the viewer goes away.
    pub fn close(&self) {
        let mut outbox = lock(&self.inner.outbox);
        if !outbox.closing {
            outbox.closing = true;
            // Nothing else matters now: drop what is waiting, say goodbye.
            outbox.queue.clear();
            outbox.queue.push_back(Outgoing::Packet(Packet { request: 0, message: Message::Close }));
        }
        drop(outbox);
        self.inner.wake.notify_all();
    }
}

impl Drop for HostLink {
    fn drop(&mut self) {
        self.close();
    }
}

impl Inner {
    fn new() -> Inner {
        Inner {
            outbox: Mutex::new(Outbox::default()),
            wake: Condvar::new(),
            events: Feed::default(),
            frame: Mutex::new(None),
            frame_announced: AtomicBool::new(false),
            over: AtomicBool::new(false),
            reader_done: AtomicBool::new(false),
            open_deadline: AtomicU64::new(0),
            start: Instant::now(),
            pid: Mutex::new(None),
        }
    }

    fn push(&self, outgoing: Outgoing) {
        let mut outbox = lock(&self.outbox);
        if outbox.closing {
            return;
        }
        if let Outgoing::Packet(packet) = &outgoing {
            // A newer value of the same kind makes an older one pointless.
            let same = |waiting: &Outgoing| match (waiting, &packet.message) {
                (Outgoing::Packet(old), new) => std::mem::discriminant(&old.message) == std::mem::discriminant(new) && is_latest_wins(new),
                _ => false,
            };
            if let Some(at) = outbox.queue.iter().position(same) {
                outbox.queue.remove(at);
            }
        }
        if outbox.queue.len() >= OUTBOX_LIMIT {
            // Page renders are the only thing that piles up, and the viewer asks again.
            let stale = outbox.queue.iter().position(|waiting| matches!(waiting, Outgoing::Packet(Packet { message: Message::RenderPage { .. } | Message::PageText { .. }, .. })));
            match stale {
                Some(at) => {
                    outbox.queue.remove(at);
                }
                None => return,
            }
        }
        outbox.queue.push_back(outgoing);
        drop(outbox);
        self.wake.notify_all();
    }

    /// End the link with `failure`, once.
    fn finish(&self, failure: Failure) {
        if !self.over.swap(true, Ordering::AcqRel) {
            self.events.push(LinkEvent::Died(failure));
            self.events.close();
        }
        self.wake.notify_all();
    }

    fn deadline_in(&self, time: Duration) {
        let at = self.start.elapsed() + time;
        self.open_deadline.store(at.as_millis() as u64 + 1, Ordering::Release);
    }
}

/// Whether a newer packet of this kind replaces an older one still waiting.
fn is_latest_wins(message: &Message) -> bool {
    matches!(message, Message::Seek { .. } | Message::SetSize { .. } | Message::SetVolume { .. } | Message::SetRate { .. } | Message::SetMute { .. })
}

/// A subtitle file beside `media`, named like it: `film.srt` or `film.vtt`.
fn sidecar_of(media: &Path) -> Option<PathBuf> {
    ["srt", "vtt"].iter().map(|extension| media.with_extension(extension)).find(|candidate| candidate != media && candidate.is_file())
}

/// End the oldest helpers until there is room for one more.
fn make_room(new: &Arc<Inner>) {
    let mut live = lock(&LIVE);
    live.retain(|link| link.upgrade().is_some_and(|link| !link.over.load(Ordering::Acquire)));
    while live.len() >= LIMIT.load(Ordering::Relaxed) {
        let oldest = live.remove(0);
        if let Some(oldest) = oldest.upgrade() {
            oldest.finish(Failure { code: ErrorCode::Internal, message: "This preview was stopped to make room for another.".to_owned() });
        }
    }
    live.push(Arc::downgrade(new));
}

/// Start the process with the socket as descriptor 3 and the allowed
/// environment, tied to this thread's life.
fn spawn_host(program: &Path, theirs: &Channel) -> std::io::Result<Child> {
    let mut command = Command::new(program);
    // A unit test starts helpers side by side and must not play through the machine's sound server.
    // Two helpers that opened a film at once on the default output (a PipeWire sink) stood at their
    // first picture, their audio sink not prerolled, until the helper's own open timer (20 s) ended
    // them: "The preview took too long and was stopped". Alone, each opened in under a second.
    #[cfg(test)]
    command.args(["--audio-sink", "fakesink"]);
    command.env_clear();
    for (name, value) in std::env::vars_os() {
        let keep = name.to_str().is_some_and(|name| ENV_KEEP.contains(&name) || ENV_PREFIXES.iter().any(|prefix| name.starts_with(prefix)));
        if keep {
            command.env(name, value);
        }
    }
    command.stdin(Stdio::null()).stdout(Stdio::null());
    if std::env::var_os("BINGUX_PREVIEW_DEBUG").is_none() {
        command.stderr(Stdio::null());
    }
    let child_end = theirs.as_raw_fd();
    // SAFETY: only dup2 and fcntl run between fork and exec, both
    // async-signal-safe. The child's end of the pair becomes descriptor 3 and
    // loses close-on-exec; if it is already 3 only the flag is cleared.
    unsafe {
        command.pre_exec(move || {
            if child_end == 3 {
                let flags = libc::fcntl(3, libc::F_GETFD);
                if flags < 0 || libc::fcntl(3, libc::F_SETFD, flags & !libc::FD_CLOEXEC) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
            } else if libc::dup2(child_end, 3) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    crate::runtime::die_with_parent(&mut command);
    command.spawn()
}

/// The supervisor thread: starts the helper and runs until it is gone.
fn supervise(inner: &Arc<Inner>) {
    let Some(program) = host_program() else {
        inner.finish(Failure { code: ErrorCode::NoDecoder, message: "The preview helper (bingux-preview-host) is not installed.".to_owned() });
        return;
    };
    let (ours, theirs) = match Channel::pair() {
        Ok(pair) => pair,
        Err(error) => {
            inner.finish(Failure { code: ErrorCode::Internal, message: format!("The preview could not start ({error}).") });
            return;
        }
    };
    let mut child = match spawn_host(&program, &theirs) {
        Ok(child) => child,
        Err(error) => {
            inner.finish(Failure { code: ErrorCode::Internal, message: format!("The preview helper did not start ({error}).") });
            return;
        }
    };
    drop(theirs);
    *lock(&inner.pid) = Some(child.id());

    let reader_channel = match ours.try_clone() {
        Ok(channel) => channel,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            inner.finish(Failure { code: ErrorCode::Internal, message: format!("The preview could not start ({error}).") });
            return;
        }
    };
    let reading = Arc::clone(inner);
    let reader = std::thread::Builder::new().name("bingux-preview-read".into()).spawn(move || read(&reading, &reader_channel));
    if reader.is_err() {
        let _ = child.kill();
        let _ = child.wait();
        inner.finish(Failure { code: ErrorCode::Internal, message: "The preview could not start (no thread).".to_owned() });
        return;
    }

    let _ = ours.send(&Packet { request: 0, message: Message::Hello { version: VERSION } }, &[]);
    let mut leaving: Option<Instant> = None;
    loop {
        let item = {
            let mut outbox = lock(&inner.outbox);
            loop {
                if let Some(item) = outbox.queue.pop_front() {
                    break Some(item);
                }
                if inner.over.load(Ordering::Acquire) || inner.reader_done.load(Ordering::Acquire) || leaving.is_some_and(|at| at.elapsed() > LEAVE_TIME) {
                    break None;
                }
                if matches!(child.try_wait(), Ok(Some(_)) | Err(_)) {
                    break None;
                }
                let deadline = inner.open_deadline.load(Ordering::Acquire);
                if deadline != 0 && inner.start.elapsed() > Duration::from_millis(deadline - 1) {
                    inner.finish(Failure { code: ErrorCode::Internal, message: "The preview took too long to open and was stopped.".to_owned() });
                    break None;
                }
                if outbox.closing && leaving.is_none() {
                    leaving = Some(Instant::now());
                }
                let (guard, _) = inner.wake.wait_timeout(outbox, Duration::from_millis(100)).unwrap_or_else(|poisoned| poisoned.into_inner());
                outbox = guard;
            }
        };
        let Some(item) = item else { break };
        let closing = matches!(&item, Outgoing::Packet(Packet { message: Message::Close, .. }));
        match item {
            Outgoing::Packet(packet) => {
                if ours.send(&packet, &[]).is_err() {
                    break;
                }
            }
            Outgoing::OpenFiles { mut packet, file } => {
                let mut paths = vec![file.clone()];
                if let Message::OpenMedia { subtitle_file, .. } = &mut packet.message {
                    if let Some(sidecar) = sidecar_of(&file) {
                        *subtitle_file = true;
                        paths.push(sidecar);
                    }
                }
                let opened: Result<Vec<File>, std::io::Error> = paths.iter().map(File::open).collect();
                match opened {
                    Ok(files) => {
                        inner.deadline_in(OPEN_TIME);
                        let raw: Vec<i32> = files.iter().map(AsRawFd::as_raw_fd).collect();
                        if ours.send(&packet, &raw).is_err() {
                            break;
                        }
                    }
                    Err(error) => {
                        inner.events.push(LinkEvent::Packet(Packet {
                            request: packet.request,
                            message: Message::Error { code: ErrorCode::NotFound, message: format!("The file could not be opened ({}).", bingux_preview_core::text::plain(&error)) },
                        }));
                    }
                }
            }
        }
        if closing {
            leaving = Some(Instant::now());
        }
    }

    // Ending: wait a moment for the helper to leave on its own, then end it.
    ours.shutdown();
    let deadline = Instant::now() + LEAVE_TIME;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                break child.wait().ok();
            }
        }
    };
    let closing = lock(&inner.outbox).closing;
    if !closing {
        let what = match status {
            Some(status) if status.code() == Some(124) => "The preview took too long and was stopped.".to_owned(),
            Some(status) => format!("The preview helper stopped unexpectedly ({status})."),
            None => "The preview helper stopped unexpectedly.".to_owned(),
        };
        inner.finish(Failure { code: ErrorCode::Internal, message: what });
    } else {
        inner.over.store(true, Ordering::Release);
        inner.events.close();
    }
}

/// Copy `size` bytes out of the shared memory in `fd`, or `None` when the
/// memory is not what the message promised.
fn copy_out(fd: OwnedFd, size: u32) -> Option<(Shared, Vec<u8>)> {
    let shared = Shared::map(fd, size as usize).ok()?;
    let bytes = shared.copy_to(size as usize);
    Some((shared, bytes))
}

/// The reader thread: turns what the helper sends into events.
fn read(inner: &Arc<Inner>, channel: &Channel) {
    // The memory of each frame slot, kept between frames.
    let mut slots: [Option<Shared>; 3] = [None, None, None];
    loop {
        let received = match channel.recv() {
            Ok(received) => received,
            Err(RecvError::Closed) => break,
            Err(error) => {
                inner.finish(Failure { code: ErrorCode::Internal, message: format!("The preview helper sent something unreadable ({error}).") });
                break;
            }
        };
        let Packet { request, message } = received.packet;
        let mut fds = received.fds;
        match message {
            Message::FrameReady { slot, width, height, stride, size, pts_ns } => {
                let valid = (slot as usize) < slots.len() && stride == width.saturating_mul(4) && size == stride.saturating_mul(height) && size > 0;
                if !valid {
                    inner.finish(Failure { code: ErrorCode::Internal, message: "The preview helper sent a frame that does not fit.".to_owned() });
                    break;
                }
                let pixels = if let Some(fd) = (!fds.is_empty()).then(|| fds.remove(0)) {
                    // First use of the slot, or a new size: map what came.
                    copy_out(fd, size).map(|(shared, bytes)| {
                        slots[slot as usize] = Some(shared);
                        bytes
                    })
                } else {
                    slots[slot as usize].as_ref().filter(|shared| shared.len() >= size as usize).map(|shared| shared.copy_to(size as usize))
                };
                // The pixels are ours now: the host may write the slot again.
                let _ = channel.send(&Packet { request: 0, message: Message::FrameDone { slot } }, &[]);
                let Some(pixels) = pixels else { continue };
                *lock(&inner.frame) = Some(Frame { width, height, pts_ns, pixels: Bytes::from(pixels) });
                if !inner.frame_announced.swap(true, Ordering::AcqRel) {
                    inner.events.push(LinkEvent::Frame);
                }
                inner.open_deadline.store(0, Ordering::Release);
            }
            Message::PageReady { page, width, height, stride, size } => {
                let valid = stride == width.saturating_mul(4) && size == stride.saturating_mul(height) && size > 0 && !fds.is_empty();
                if !valid {
                    inner.finish(Failure { code: ErrorCode::Internal, message: "The preview helper sent a page that does not fit.".to_owned() });
                    break;
                }
                if let Some((_, bytes)) = copy_out(fds.remove(0), size) {
                    inner.events.push(LinkEvent::Page(PageImage { request, page, width, height, pixels: Bytes::from(bytes) }));
                }
            }
            Message::Ready { version, .. } if version != VERSION => {
                inner.finish(Failure { code: ErrorCode::Internal, message: "The preview helper speaks another version.".to_owned() });
                break;
            }
            Message::Ready { .. } => {}
            message => {
                if matches!(message, Message::Opened { .. } | Message::Error { .. }) {
                    inner.open_deadline.store(0, Ordering::Release);
                }
                inner.events.push(LinkEvent::Packet(Packet { request, message }));
            }
        }
    }
    // The helper closed its end, or sent something unreadable: the supervisor
    // reaps the process and says why.
    inner.reader_done.store(true, Ordering::Release);
    inner.wake.notify_all();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_helper_is_found_by_variable_beside_the_executable_beside_deps_or_on_the_path() {
        let dir = std::env::temp_dir().join(format!("bingux-host-find-{}", std::process::id()));
        let deps = dir.join("debug/deps");
        std::fs::create_dir_all(&deps).unwrap();
        let program = dir.join("debug/bingux-preview-host");
        std::fs::write(&program, "").unwrap();
        // Nothing found anywhere.
        // Only a debug build looks above `deps/`: a release build is run from an install.
        #[cfg(debug_assertions)]
        assert_eq!(host_program_from(None, Some(deps.join("test")), None), Some(program.clone()), "above deps/");
        #[cfg(not(debug_assertions))]
        assert_eq!(host_program_from(None, Some(deps.join("test")), None), None, "a release build does not look above deps/");
        assert_eq!(host_program_from(None, Some(dir.join("debug/bingux-files")), None), Some(program.clone()), "beside");
        assert_eq!(host_program_from(Some(program.clone()), None, None), Some(program.clone()), "by variable");
        assert_eq!(host_program_from(Some(dir.join("missing")), Some(dir.join("debug/bingux-files")), None), None, "an explicit path is not second-guessed");
        assert_eq!(host_program_from(None, Some(PathBuf::from("/nowhere/app")), Some(dir.join("debug").into())), Some(program), "on PATH");
        assert_eq!(host_program_from(None, Some(PathBuf::from("/nowhere/app")), None), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn newer_seeks_replace_older_ones_and_the_queue_is_bounded() {
        let inner = Inner::new();
        let packet = |message| Outgoing::Packet(Packet { request: 1, message });
        inner.push(packet(Message::Seek { ns: 1, accurate: false }));
        inner.push(packet(Message::Play));
        inner.push(packet(Message::Seek { ns: 2, accurate: false }));
        inner.push(packet(Message::SetVolume { permille: 100 }));
        inner.push(packet(Message::SetVolume { permille: 200 }));
        {
            let outbox = lock(&inner.outbox);
            let kinds: Vec<_> = outbox
                .queue
                .iter()
                .map(|item| match item {
                    Outgoing::Packet(packet) => packet.message.clone(),
                    Outgoing::OpenFiles { packet, .. } => packet.message.clone(),
                })
                .collect();
            assert_eq!(kinds, vec![Message::Play, Message::Seek { ns: 2, accurate: false }, Message::SetVolume { permille: 200 }]);
        }
        for page in 0..(OUTBOX_LIMIT as u32 * 2) {
            inner.push(packet(Message::RenderPage { page, scale_milli: 1000, max_w: 0, max_h: 0 }));
        }
        assert!(lock(&inner.outbox).queue.len() <= OUTBOX_LIMIT);
        // A close drops everything waiting and leaves one packet.
        let link = HostLink { inner: Arc::new(inner) };
        link.close();
        assert_eq!(lock(&link.inner.outbox).queue.len(), 1);
        link.send(Packet { request: 1, message: Message::Play });
        assert_eq!(lock(&link.inner.outbox).queue.len(), 1, "nothing is queued after a close");
    }
}
