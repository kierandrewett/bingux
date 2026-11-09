//! The picture: mpv draws each video frame, at the size the window shows it,
//! into memory that the window then draws as an image.
//!
//! One thread per player does it. mpv tells it a frame is due (its update
//! callback); the thread draws the frame at the size the [`FrameSink`] asks
//! for, letterboxed and with the subtitles in it, and hands the pixels over.
//! A change of size draws the current frame again at once, so a paused film
//! follows the window.
//!
//! Two ways to draw, chosen when the thread starts:
//!
//! | Backend | How | When |
//! | ------- | --- | ---- |
//! | `gl` | mpv's GPU renderer into an off-screen EGL context, read back | the default; any OpenGL or OpenGL ES 3 driver, llvmpipe too |
//! | `sw` | mpv's software renderer (`MPV_RENDER_API_TYPE_SW`) | no EGL, or `BINGUX_MEDIA_RENDER=sw` |
//!
//! The pixels go into buffers from a small pool: a frame's buffer comes back
//! when the window drops the image made from it, so playing allocates nothing
//! once the size is stable.

mod egl;

use std::ffi::c_void;
use std::os::raw::c_int;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread::JoinHandle;

use bytes::Bytes;

use crate::mpv::{Handle, cstring, ffi};

/// One picture: `width * height` pixels of 4 bytes, R, G, B and one unused
/// byte (draw it as opaque RGBA), rows `width * 4` bytes apart.
#[derive(Clone)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub pixels: Bytes,
}

/// Where the pictures go: the window's video surface.
pub trait FrameSink: Send + Sync {
    /// The size the picture is shown at now, in device pixels; `(0, 0)` while
    /// it is not shown.
    fn size(&self) -> (u32, u32);
    /// A new picture. Called on the render thread.
    fn push(&self, frame: Frame);
    /// No picture: the file has no video, or nothing is open.
    fn clear(&self) {}
}

/// Which renderer draws the frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    Gl,
    Software,
}

impl Backend {
    pub fn name(self) -> &'static str {
        match self {
            Backend::Gl => "gl",
            Backend::Software => "sw",
        }
    }
}

#[derive(Default)]
struct Wake {
    update: bool,
    resize: bool,
    stop: bool,
}

pub(crate) struct Shared {
    wake: Mutex<Wake>,
    cond: Condvar,
    backend: Mutex<Option<Backend>>,
}

impl Shared {
    fn poke(&self, change: impl FnOnce(&mut Wake)) {
        change(&mut self.wake.lock().unwrap_or_else(PoisonError::into_inner));
        self.cond.notify_one();
    }
}

/// The render thread of one player.
pub struct Renderer {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl Renderer {
    /// Starts the thread. The render context is made on it, before mpv opens
    /// any file, so the first video frame finds it.
    pub fn start(handle: Arc<Handle>, sink: Arc<dyn FrameSink>) -> Renderer {
        let shared = Arc::new(Shared {
            wake: Mutex::new(Wake::default()),
            cond: Condvar::new(),
            backend: Mutex::new(None),
        });
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("media-render".into())
            .spawn({
                let shared = shared.clone();
                move || run(handle, sink, shared, ready_tx)
            })
            .ok();
        // Wait (briefly, once, at start-up) until the context exists.
        let _ = ready_rx.recv_timeout(std::time::Duration::from_secs(5));
        Renderer { shared, thread }
    }

    /// The window shows the picture at a new size: draw it again.
    pub fn resized(&self) {
        self.shared.poke(|wake| wake.resize = true);
    }

    /// The backend in use, once the thread has made one.
    pub fn backend(&self) -> Option<Backend> {
        *self
            .shared
            .backend
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        self.shared.poke(|wake| wake.stop = true);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

unsafe extern "C" fn on_update(ctx: *mut c_void) {
    // SAFETY: ctx is the `Shared` the thread keeps alive until the context is freed.
    let shared = unsafe { &*(ctx as *const Shared) };
    shared.poke(|wake| wake.update = true);
}

/// A drawing target the thread reuses.
trait Draw {
    fn context(&self) -> *mut ffi::mpv_render_context;
    /// Draws the current frame at `width` x `height` into `out` (exactly
    /// `width * height * 4` bytes).
    fn draw(&mut self, width: u32, height: u32, out: &mut [u8]) -> bool;
}

fn run(
    handle: Arc<Handle>,
    sink: Arc<dyn FrameSink>,
    shared: Arc<Shared>,
    ready: std::sync::mpsc::Sender<()>,
) {
    let wanted = std::env::var("BINGUX_MEDIA_RENDER").ok();
    let mut drawer: Option<(Box<dyn Draw>, Backend)> = None;
    if wanted.as_deref() != Some("sw") {
        match GlDraw::new(&handle) {
            Ok(gl) => drawer = Some((Box::new(gl), Backend::Gl)),
            Err(error) => {
                eprintln!("[media] GPU renderer unavailable, drawing in software: {error}")
            }
        }
    }
    if drawer.is_none() {
        match SwDraw::new(&handle) {
            Ok(sw) => drawer = Some((Box::new(sw), Backend::Software)),
            Err(error) => eprintln!("[media] no video renderer: {error}"),
        }
    }
    let Some((mut drawer, backend)) = drawer else {
        let _ = ready.send(());
        return;
    };
    *shared
        .backend
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = Some(backend);
    // SAFETY: the callback's context is `shared`, alive until after the
    // context is freed below.
    unsafe {
        ffi::mpv_render_context_set_update_callback(
            drawer.context(),
            Some(on_update),
            Arc::as_ptr(&shared) as *mut c_void,
        );
    }
    let _ = ready.send(());

    let pool = Pool::default();
    let mut last_size = (0u32, 0u32);
    loop {
        let (update, resize) = {
            let mut wake = shared.wake.lock().unwrap_or_else(PoisonError::into_inner);
            while !wake.update && !wake.resize && !wake.stop {
                wake = shared
                    .cond
                    .wait(wake)
                    .unwrap_or_else(PoisonError::into_inner);
            }
            if wake.stop {
                break;
            }
            let pending = (wake.update, wake.resize);
            wake.update = false;
            wake.resize = false;
            pending
        };
        let mut frame_due = false;
        if update {
            // SAFETY: the context belongs to this thread.
            let flags = unsafe { ffi::mpv_render_context_update(drawer.context()) };
            frame_due = flags & ffi::MPV_RENDER_UPDATE_FRAME != 0;
        }
        let (width, height) = fit(sink.size());
        if width == 0 || height == 0 {
            // Not on screen: a frame that is due is still taken, so mpv's
            // clock does not wait for a window that does not look.
            if frame_due {
                let mut scratch = [0u8; 16 * 16 * 4];
                drawer.draw(16, 16, &mut scratch);
            }
            continue;
        }
        let size_changed = (width, height) != last_size;
        if !(frame_due || (resize && size_changed) || (resize && !frame_due)) {
            continue;
        }
        let mut buffer = pool.take(width as usize * height as usize * 4);
        if drawer.draw(width, height, buffer.as_mut_slice()) {
            last_size = (width, height);
            sink.push(Frame {
                width,
                height,
                pixels: buffer.into_bytes(),
            });
            // SAFETY: the context belongs to this thread.
            unsafe { ffi::mpv_render_context_report_swap(drawer.context()) };
        }
    }
    drop(drawer);
}

/// The size to draw at: at most 4096 a side, the width a multiple of 16 so a
/// row is a multiple of 64 bytes (mpv's software scaler's fast path).
fn fit((width, height): (u32, u32)) -> (u32, u32) {
    let width = width.min(4096) & !15;
    let height = height.min(4096);
    if width == 0 || height == 0 {
        (0, 0)
    } else {
        (width, height)
    }
}

// ---- the GPU path ----------------------------------------------------------------

struct GlDraw {
    egl: egl::Context,
    context: *mut ffi::mpv_render_context,
    fbo: u32,
    texture: u32,
    size: (u32, u32),
}

impl GlDraw {
    fn new(handle: &Handle) -> Result<GlDraw, String> {
        let egl = egl::Context::new()?;
        let software = ["llvmpipe", "softpipe", "swrast", "SwiftShader"]
            .iter()
            .any(|name| egl.renderer.contains(name));
        if software && std::env::var("BINGUX_MEDIA_RENDER").as_deref() != Ok("gl") {
            // A GPU renderer emulated on the CPU is slower than mpv's own
            // multithreaded software scaler (60 against 15 frames a second for
            // 1080p60 on llvmpipe, 4 cores).
            return Err(format!("{} draws on the CPU", egl.renderer));
        }
        let api = cstring("opengl");
        let mut init = ffi::mpv_opengl_init_params {
            get_proc_address: Some(egl::proc_address),
            get_proc_address_ctx: std::ptr::null_mut(),
        };
        let mut params = [
            ffi::mpv_render_param {
                type_: ffi::MPV_RENDER_PARAM_API_TYPE,
                data: api.as_ptr() as *mut c_void,
            },
            ffi::mpv_render_param {
                type_: ffi::MPV_RENDER_PARAM_OPENGL_INIT_PARAMS,
                data: (&mut init as *mut ffi::mpv_opengl_init_params).cast(),
            },
            ffi::mpv_render_param {
                type_: ffi::MPV_RENDER_PARAM_INVALID,
                data: std::ptr::null_mut(),
            },
        ];
        let mut context = std::ptr::null_mut();
        // SAFETY: the parameters live across the call; the GL context is current.
        let code = unsafe {
            ffi::mpv_render_context_create(&mut context, handle.raw(), params.as_mut_ptr())
        };
        if code < 0 {
            return Err(format!(
                "mpv cannot draw with {}: {}",
                egl.description,
                crate::mpv::Error::of_code(code)
            ));
        }
        eprintln!(
            "[media] drawing video with mpv's GPU renderer on {}",
            egl.description
        );
        Ok(GlDraw {
            egl,
            context,
            fbo: 0,
            texture: 0,
            size: (0, 0),
        })
    }

    fn target(&mut self, width: u32, height: u32) -> bool {
        if self.size == (width, height) && self.fbo != 0 {
            return true;
        }
        let gl = &self.egl.gl;
        // SAFETY: GL calls on this thread's current context.
        unsafe {
            if self.fbo != 0 {
                (gl.DeleteFramebuffers)(1, &self.fbo);
                (gl.DeleteTextures)(1, &self.texture);
            }
            (gl.GenTextures)(1, &mut self.texture);
            (gl.BindTexture)(egl::GL_TEXTURE_2D, self.texture);
            (gl.TexParameteri)(
                egl::GL_TEXTURE_2D,
                egl::GL_TEXTURE_MIN_FILTER,
                egl::GL_LINEAR,
            );
            (gl.TexParameteri)(
                egl::GL_TEXTURE_2D,
                egl::GL_TEXTURE_MAG_FILTER,
                egl::GL_LINEAR,
            );
            (gl.TexImage2D)(
                egl::GL_TEXTURE_2D,
                0,
                egl::GL_RGBA8,
                width as c_int,
                height as c_int,
                0,
                egl::GL_RGBA,
                egl::GL_UNSIGNED_BYTE,
                std::ptr::null(),
            );
            (gl.GenFramebuffers)(1, &mut self.fbo);
            (gl.BindFramebuffer)(egl::GL_FRAMEBUFFER, self.fbo);
            (gl.FramebufferTexture2D)(
                egl::GL_FRAMEBUFFER,
                egl::GL_COLOR_ATTACHMENT0,
                egl::GL_TEXTURE_2D,
                self.texture,
                0,
            );
            if (gl.CheckFramebufferStatus)(egl::GL_FRAMEBUFFER) != egl::GL_FRAMEBUFFER_COMPLETE {
                eprintln!("[media] the off-screen framebuffer is incomplete at {width}x{height}");
                self.size = (0, 0);
                return false;
            }
        }
        self.size = (width, height);
        true
    }
}

impl Draw for GlDraw {
    fn context(&self) -> *mut ffi::mpv_render_context {
        self.context
    }

    fn draw(&mut self, width: u32, height: u32, out: &mut [u8]) -> bool {
        if !self.target(width, height) {
            return false;
        }
        let mut fbo = ffi::mpv_opengl_fbo {
            fbo: self.fbo as c_int,
            w: width as c_int,
            h: height as c_int,
            internal_format: egl::GL_RGBA8,
        };
        // The texture's first row is the picture's top once read back.
        let mut flip: c_int = 0;
        let mut params = [
            ffi::mpv_render_param {
                type_: ffi::MPV_RENDER_PARAM_OPENGL_FBO,
                data: (&mut fbo as *mut ffi::mpv_opengl_fbo).cast(),
            },
            ffi::mpv_render_param {
                type_: ffi::MPV_RENDER_PARAM_FLIP_Y,
                data: (&mut flip as *mut c_int).cast(),
            },
            ffi::mpv_render_param {
                type_: ffi::MPV_RENDER_PARAM_INVALID,
                data: std::ptr::null_mut(),
            },
        ];
        let gl = &self.egl.gl;
        // SAFETY: the context and framebuffer belong to this thread; `out`
        // holds width * height * 4 bytes.
        unsafe {
            if ffi::mpv_render_context_render(self.context, params.as_mut_ptr()) < 0 {
                return false;
            }
            (gl.BindFramebuffer)(egl::GL_FRAMEBUFFER, self.fbo);
            (gl.PixelStorei)(egl::GL_PACK_ALIGNMENT, 4);
            (gl.ReadPixels)(
                0,
                0,
                width as c_int,
                height as c_int,
                egl::GL_RGBA,
                egl::GL_UNSIGNED_BYTE,
                out.as_mut_ptr().cast(),
            );
            if std::env::var_os("BINGUX_MEDIA_DEBUG").is_some() {
                let middle = (height as usize / 2 * width as usize + width as usize / 2) * 4;
                eprintln!(
                    "[media] GL frame {width}x{height}: error {:#x}, middle pixel {:?}",
                    (gl.GetError)(),
                    out.get(middle..middle + 4)
                );
            }
        }
        true
    }
}

impl Drop for GlDraw {
    fn drop(&mut self) {
        // SAFETY: freed on the thread that made it, before the GL context goes.
        unsafe {
            ffi::mpv_render_context_free(self.context);
            if self.fbo != 0 {
                (self.egl.gl.DeleteFramebuffers)(1, &self.fbo);
                (self.egl.gl.DeleteTextures)(1, &self.texture);
            }
        }
    }
}

// ---- the software path ------------------------------------------------------------

struct SwDraw {
    context: *mut ffi::mpv_render_context,
}

impl SwDraw {
    fn new(handle: &Handle) -> Result<SwDraw, String> {
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
        // SAFETY: the parameters live across the call.
        let code = unsafe {
            ffi::mpv_render_context_create(&mut context, handle.raw(), params.as_mut_ptr())
        };
        if code < 0 {
            return Err(format!(
                "mpv has no software renderer: {}",
                crate::mpv::Error::of_code(code)
            ));
        }
        eprintln!("[media] drawing video with mpv's software renderer");
        Ok(SwDraw { context })
    }
}

impl Draw for SwDraw {
    fn context(&self) -> *mut ffi::mpv_render_context {
        self.context
    }

    fn draw(&mut self, width: u32, height: u32, out: &mut [u8]) -> bool {
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
                data: out.as_mut_ptr().cast(),
            },
            ffi::mpv_render_param {
                type_: ffi::MPV_RENDER_PARAM_INVALID,
                data: std::ptr::null_mut(),
            },
        ];
        // SAFETY: `out` is width * height * 4 bytes, as the stride says.
        unsafe { ffi::mpv_render_context_render(self.context, params.as_mut_ptr()) >= 0 }
    }
}

impl Drop for SwDraw {
    fn drop(&mut self) {
        // SAFETY: freed once, on the thread that made it.
        unsafe { ffi::mpv_render_context_free(self.context) }
    }
}

// ---- buffers ---------------------------------------------------------------------

/// Spare pixel buffers. A buffer handed to the window comes back here when the
/// last image made from it is dropped.
#[derive(Clone, Default)]
struct Pool {
    spare: Arc<Mutex<Vec<Vec<u8>>>>,
}

/// How many spare buffers are kept: the frame on screen, the one being drawn
/// and one in flight.
const POOL_KEEP: usize = 3;

impl Pool {
    fn take(&self, len: usize) -> Pooled {
        let reused = {
            let mut spare = self.spare.lock().unwrap_or_else(PoisonError::into_inner);
            let found = spare.iter().position(|buffer| buffer.len() == len);
            match found {
                Some(index) => Some(spare.swap_remove(index)),
                None => {
                    // A size change: the old buffers will not fit again.
                    spare.clear();
                    None
                }
            }
        };
        Pooled {
            buffer: reused.unwrap_or_else(|| vec![0u8; len]),
            pool: self.clone(),
        }
    }
}

struct Pooled {
    buffer: Vec<u8>,
    pool: Pool,
}

impl Pooled {
    fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.buffer
    }

    fn into_bytes(self) -> Bytes {
        Bytes::from_owner(self)
    }
}

impl AsRef<[u8]> for Pooled {
    fn as_ref(&self) -> &[u8] {
        &self.buffer
    }
}

impl Drop for Pooled {
    fn drop(&mut self) {
        let buffer = std::mem::take(&mut self.buffer);
        if buffer.is_empty() {
            return;
        }
        let mut spare = self
            .pool
            .spare
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if spare.len() < POOL_KEEP
            && spare
                .first()
                .is_none_or(|first| first.len() == buffer.len())
        {
            spare.push(buffer);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_are_cut_to_a_64_byte_row() {
        assert_eq!(fit((1441, 900)), (1440, 900));
        assert_eq!(fit((15, 900)), (0, 0));
        assert_eq!(fit((9000, 9000)), (4096, 4096));
    }

    #[test]
    fn a_dropped_frame_returns_its_buffer() {
        let pool = Pool::default();
        let first = pool.take(64).into_bytes();
        let address = first.as_ptr();
        drop(first);
        let again = pool.take(64);
        assert_eq!(again.buffer.as_ptr(), address);
    }
}
