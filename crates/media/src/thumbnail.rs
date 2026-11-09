//! Pictures of a film at a moment: the library's thumbnails and the
//! timeline's seek preview.
//!
//! A second, silent mpv (no sound, no subtitles, keyframe seeks) opens the
//! film, seeks and renders one frame in software at the size asked for. One
//! thread does the work; a newer request replaces one that has not started,
//! so a pointer sweeping over the timeline costs one frame at a time and the
//! last position always wins.
//!
//! Library thumbnails are written where every freedesktop program looks for
//! them (`$XDG_CACHE_HOME/thumbnails/large/<md5 of the URI>.png`, with
//! `Thumb::URI` and `Thumb::MTime`), so Files shows the same frame.

use std::collections::HashSet;
use std::ffi::c_void;
use std::os::raw::c_int;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::mpv::{Event, Handle, cstring, ffi};
use crate::render::Frame;

/// The frame, and the film's length when the worker knows it.
type Reply = Box<dyn FnOnce(Option<Frame>, Option<f64>) + Send>;

struct Request {
    path: PathBuf,
    seconds: Option<f64>,
    side: u32,
    reply: Reply,
}

#[derive(Default)]
struct Queue {
    /// The newest preview request; an older one not started is dropped (its
    /// reply gets nothing).
    preview: Option<Request>,
    /// Library thumbnails, in order.
    library: Vec<Request>,
}

struct Shared {
    queue: Mutex<Queue>,
    cond: Condvar,
}

/// The thumbnailer. Clones share its thread.
#[derive(Clone)]
pub struct Thumbnailer {
    shared: Arc<Shared>,
}

impl Default for Thumbnailer {
    fn default() -> Self {
        Self::new()
    }
}

impl Thumbnailer {
    pub fn new() -> Thumbnailer {
        let shared = Arc::new(Shared {
            queue: Mutex::new(Queue::default()),
            cond: Condvar::new(),
        });
        let worker = shared.clone();
        let _ = std::thread::Builder::new()
            .name("media-thumbs".into())
            .spawn(move || run(worker));
        Thumbnailer { shared }
    }

    /// A frame of `path` at `seconds`, at most `side` pixels on its longer
    /// side, for the seek preview. Replaces a preview not yet started.
    pub fn preview(
        &self,
        path: &Path,
        seconds: f64,
        side: u32,
        reply: impl FnOnce(Option<Frame>) + Send + 'static,
    ) {
        let mut queue = self
            .shared
            .queue
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        queue.preview = Some(Request {
            path: path.to_owned(),
            seconds: Some(seconds),
            side,
            reply: Box::new(move |frame, _| reply(frame)),
        });
        drop(queue);
        self.shared.cond.notify_one();
    }

    /// Makes the library thumbnail of a film (a frame a tenth of the way in),
    /// writes it to the thumbnail cache, and replies with its file and the
    /// film's length.
    pub fn library_thumbnail(
        &self,
        path: &Path,
        modified: i64,
        reply: impl FnOnce(Option<(PathBuf, Option<f64>)>) + Send + 'static,
    ) {
        let source = path.to_owned();
        let request = Request {
            path: path.to_owned(),
            seconds: None,
            side: 256,
            reply: Box::new(move |frame, duration| {
                let written = frame.and_then(|frame| {
                    write_thumbnail(&source, modified, &frame).map(|file| (file, duration))
                });
                reply(written);
            }),
        };
        let mut queue = self
            .shared
            .queue
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        queue.library.push(request);
        drop(queue);
        self.shared.cond.notify_one();
    }
}

fn run(shared: Arc<Shared>) {
    let mut worker: Option<Worker> = None;
    loop {
        let request = {
            let mut queue = shared.queue.lock().unwrap_or_else(PoisonError::into_inner);
            loop {
                if let Some(request) = queue.preview.take() {
                    break request;
                }
                if !queue.library.is_empty() {
                    break queue.library.remove(0);
                }
                // Nothing to do for a while: let mpv go.
                let (next, timeout) = shared
                    .cond
                    .wait_timeout(queue, Duration::from_secs(30))
                    .unwrap_or_else(PoisonError::into_inner);
                queue = next;
                if timeout.timed_out() && queue.preview.is_none() && queue.library.is_empty() {
                    worker = None;
                }
            }
        };
        if worker.is_none() {
            worker = Worker::new();
        }
        let frame = worker
            .as_mut()
            .and_then(|worker| worker.grab(&request.path, request.seconds, request.side));
        let duration = worker.as_ref().and_then(|worker| worker.duration);
        (request.reply)(frame, duration);
    }
}

#[derive(Default)]
struct Wake {
    update: bool,
}

struct Worker {
    handle: Handle,
    context: *mut ffi::mpv_render_context,
    wake: Arc<(Mutex<Wake>, Condvar)>,
    loaded: Option<PathBuf>,
    duration: Option<f64>,
    failed: HashSet<PathBuf>,
}

unsafe extern "C" fn on_update(ctx: *mut c_void) {
    // SAFETY: ctx is the `wake` pair the worker keeps alive until the context is freed.
    let wake = unsafe { &*(ctx as *const (Mutex<Wake>, Condvar)) };
    wake.0.lock().unwrap_or_else(PoisonError::into_inner).update = true;
    wake.1.notify_one();
}

impl Worker {
    fn new() -> Option<Worker> {
        let handle = Handle::new(&[
            ("vo", "libmpv"),
            ("ao", "null"),
            ("aid", "no"),
            ("sid", "no"),
            ("pause", "yes"),
            ("idle", "yes"),
            ("keep-open", "yes"),
            ("hr-seek", "no"),
            ("config", "no"),
            ("load-scripts", "no"),
            ("ytdl", "no"),
            ("osd-level", "0"),
            ("audio-display", "no"),
            ("hwdec", "no"),
            ("msg-level", "all=fatal"),
            ("sub-auto", "no"),
        ])
        .ok()?;
        let api = cstring("sw");
        let mut params = [
            ffi::mpv_render_param {
                type_: ffi::MPV_RENDER_PARAM_API_TYPE,
                data: api.as_ptr() as *mut c_void,
            },
            ffi::mpv_render_param {
                type_: ffi::MPV_RENDER_PARAM_INVALID,
                data: std::ptr::null_mut(),
            },
        ];
        let mut context = std::ptr::null_mut();
        // SAFETY: the parameters outlive the call.
        if unsafe {
            ffi::mpv_render_context_create(&mut context, handle.raw(), params.as_mut_ptr())
        } < 0
        {
            return None;
        }
        let wake = Arc::new((Mutex::new(Wake::default()), Condvar::new()));
        // SAFETY: `wake` outlives the context (freed in Drop first).
        unsafe {
            ffi::mpv_render_context_set_update_callback(
                context,
                Some(on_update),
                Arc::as_ptr(&wake) as *mut c_void,
            );
        }
        Some(Worker {
            handle,
            context,
            wake,
            loaded: None,
            duration: None,
            failed: HashSet::new(),
        })
    }

    /// Waits for an event matching `want`, up to `timeout`. False on a
    /// failure to open the file, or the time running out.
    fn wait_for(&self, timeout: Duration, want: impl Fn(&Event) -> bool) -> bool {
        let end = Instant::now() + timeout;
        loop {
            let left = end.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            let event = self.handle.wait_event(left.as_secs_f64());
            if want(&event) {
                return true;
            }
            if let Event::EndFile { error: Some(_), .. } = event {
                return false;
            }
        }
    }

    fn grab(&mut self, path: &Path, seconds: Option<f64>, side: u32) -> Option<Frame> {
        if self.failed.contains(path) {
            return None;
        }
        if self.loaded.as_deref() != Some(path) {
            use std::os::unix::ffi::OsStrExt;
            self.loaded = None;
            self.handle
                .command_async_bytes(0, &[b"loadfile", path.as_os_str().as_bytes()]);
            if !self.wait_for(Duration::from_secs(8), |event| {
                matches!(event, Event::FileLoaded)
            }) {
                self.failed.insert(path.to_owned());
                return None;
            }
            self.loaded = Some(path.to_owned());
            self.duration = self
                .handle
                .get_double("duration")
                .filter(|duration| *duration > 0.0);
        }
        let at = seconds.unwrap_or_else(|| {
            self.duration
                .map_or(0.0, |duration| (duration * 0.1).min(120.0))
        });
        self.wake
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .update = false;
        self.handle
            .command_async(0, &["seek", &format!("{at:.3}"), "absolute+keyframes"]);
        if !self.wait_for(Duration::from_secs(5), |event| {
            matches!(event, Event::PlaybackRestart)
        }) {
            return None;
        }
        // The frame is ready once mpv says so.
        {
            let (lock, cond) = &*self.wake;
            let mut wake = lock.lock().unwrap_or_else(PoisonError::into_inner);
            let end = Instant::now() + Duration::from_secs(2);
            while !wake.update {
                let left = end.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    break;
                }
                wake = cond
                    .wait_timeout(wake, left)
                    .unwrap_or_else(PoisonError::into_inner)
                    .0;
            }
            wake.update = false;
        }
        // SAFETY: the context belongs to this thread.
        unsafe { ffi::mpv_render_context_update(self.context) };
        let (width, height) = match self.handle.get_node("video-params") {
            Some(node) => (
                node.int("dw").unwrap_or(16) as f64,
                node.int("dh").unwrap_or(9) as f64,
            ),
            None => return None,
        };
        let scale = f64::from(side) / width.max(height).max(1.0);
        let width = ((width * scale).round() as u32).max(16) & !15;
        let height = ((height * scale).round() as u32).max(2);
        let mut pixels = vec![0u8; width as usize * height as usize * 4];
        let format = cstring("rgb0");
        let mut size = [width as c_int, height as c_int];
        let mut stride: usize = width as usize * 4;
        let mut params = [
            ffi::mpv_render_param {
                type_: ffi::MPV_RENDER_PARAM_SW_SIZE,
                data: size.as_mut_ptr().cast(),
            },
            ffi::mpv_render_param {
                type_: ffi::MPV_RENDER_PARAM_SW_FORMAT,
                data: format.as_ptr() as *mut c_void,
            },
            ffi::mpv_render_param {
                type_: ffi::MPV_RENDER_PARAM_SW_STRIDE,
                data: (&mut stride as *mut usize).cast(),
            },
            ffi::mpv_render_param {
                type_: ffi::MPV_RENDER_PARAM_SW_POINTER,
                data: pixels.as_mut_ptr().cast(),
            },
            ffi::mpv_render_param {
                type_: ffi::MPV_RENDER_PARAM_INVALID,
                data: std::ptr::null_mut(),
            },
        ];
        // SAFETY: `pixels` holds width * height * 4 bytes, as the stride says.
        if unsafe { ffi::mpv_render_context_render(self.context, params.as_mut_ptr()) } < 0 {
            return None;
        }
        let pixels = bytes::Bytes::from(pixels);
        Some(Frame {
            width,
            height,
            pixels,
        })
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        // SAFETY: freed once, before the handle.
        unsafe { ffi::mpv_render_context_free(self.context) };
    }
}

/// `$XDG_CACHE_HOME/thumbnails/large`.
fn thumbnail_dir() -> PathBuf {
    crate::library::cache_home().join("thumbnails/large")
}

/// The cached library thumbnail of `path`, if one was made for this
/// modified time (by this app or any other thumbnailer).
pub fn cached(path: &Path, modified: i64) -> Option<PathBuf> {
    let time = UNIX_EPOCH + Duration::from_secs(modified.max(0) as u64);
    bingux_files::thumbs::lookup(path, Some(time), bingux_files::thumbs::Bucket::Large)
}

/// Writes `frame` as the freedesktop thumbnail of `source`.
fn write_thumbnail(source: &Path, modified: i64, frame: &Frame) -> Option<PathBuf> {
    let uri = bingux_files::location::file_uri(source);
    let dir = thumbnail_dir();
    std::fs::create_dir_all(&dir).ok()?;
    let file = dir.join(bingux_files::thumbs::thumbnail_name(&uri));
    let partial = dir.join(format!(
        ".{}.{}.part",
        std::process::id(),
        file.file_name()?.to_string_lossy()
    ));
    let mut rgb = Vec::with_capacity(frame.width as usize * frame.height as usize * 3);
    for pixel in frame.pixels.chunks_exact(4) {
        rgb.extend_from_slice(&pixel[..3]);
    }
    {
        let out = std::fs::File::create(&partial).ok()?;
        let mut encoder =
            png::Encoder::new(std::io::BufWriter::new(out), frame.width, frame.height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.add_text_chunk("Thumb::URI".into(), uri).ok()?;
        encoder
            .add_text_chunk("Thumb::MTime".into(), modified.to_string())
            .ok()?;
        encoder
            .add_text_chunk("Software".into(), "Bingux Media".into())
            .ok()?;
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(&rgb).ok()?;
    }
    std::fs::rename(&partial, &file).ok()?;
    Some(file)
}

/// The modified time of a file in whole seconds, as the thumbnail spec keeps it.
pub fn modified_secs(path: &Path) -> Option<i64> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    Some(
        modified
            .duration_since(SystemTime::UNIX_EPOCH)
            .ok()?
            .as_secs() as i64,
    )
}
