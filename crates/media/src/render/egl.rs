//! An off-screen OpenGL context for mpv to draw into, with no window.
//!
//! The picture is drawn by mpv's GPU renderer (scaling, colour conversion,
//! tone mapping and the subtitles, as in the mpv player) into a texture of an
//! EGL context that belongs to the render thread alone, and read back into
//! memory. The window's own renderer (Vulkan, OpenGL or software) never
//! shares state with it, so neither can upset the other.
//!
//! The display is Mesa's surfaceless platform when it is there (every Mesa
//! driver, llvmpipe included), else the first EGL device (the NVIDIA driver),
//! else the default display.

#![allow(non_upper_case_globals, clippy::upper_case_acronyms)]

use std::ffi::{CStr, c_void};
use std::os::raw::{c_char, c_int, c_uint};
use std::ptr;

type EGLDisplay = *mut c_void;
type EGLConfig = *mut c_void;
type EGLContext = *mut c_void;
type EGLSurface = *mut c_void;
type EGLDeviceEXT = *mut c_void;
type EGLint = i32;
type EGLenum = c_uint;
type EGLBoolean = c_uint;
type EGLAttrib = isize;

const EGL_NONE: EGLint = 0x3038;
const EGL_EXTENSIONS: EGLint = 0x3055;
const EGL_RENDERABLE_TYPE: EGLint = 0x3040;
const EGL_SURFACE_TYPE: EGLint = 0x3033;
const EGL_OPENGL_BIT: EGLint = 0x0008;
const EGL_OPENGL_ES3_BIT: EGLint = 0x0040;
const EGL_OPENGL_API: EGLenum = 0x30A2;
const EGL_OPENGL_ES_API: EGLenum = 0x30A0;
const EGL_CONTEXT_CLIENT_VERSION: EGLint = 0x3098;
const EGL_PLATFORM_SURFACELESS_MESA: EGLenum = 0x31DD;
const EGL_PLATFORM_DEVICE_EXT: EGLenum = 0x313F;

#[link(name = "EGL")]
unsafe extern "C" {
    fn eglGetProcAddress(name: *const c_char) -> *mut c_void;
    fn eglGetDisplay(native: *mut c_void) -> EGLDisplay;
    fn eglInitialize(display: EGLDisplay, major: *mut EGLint, minor: *mut EGLint) -> EGLBoolean;
    fn eglTerminate(display: EGLDisplay) -> EGLBoolean;
    fn eglQueryString(display: EGLDisplay, name: EGLint) -> *const c_char;
    fn eglBindAPI(api: EGLenum) -> EGLBoolean;
    fn eglChooseConfig(
        display: EGLDisplay,
        attribs: *const EGLint,
        configs: *mut EGLConfig,
        size: EGLint,
        count: *mut EGLint,
    ) -> EGLBoolean;
    fn eglCreateContext(
        display: EGLDisplay,
        config: EGLConfig,
        share: EGLContext,
        attribs: *const EGLint,
    ) -> EGLContext;
    fn eglDestroyContext(display: EGLDisplay, context: EGLContext) -> EGLBoolean;
    fn eglMakeCurrent(
        display: EGLDisplay,
        draw: EGLSurface,
        read: EGLSurface,
        context: EGLContext,
    ) -> EGLBoolean;
    fn eglGetError() -> EGLint;
}

type GetPlatformDisplay =
    unsafe extern "C" fn(EGLenum, *mut c_void, *const EGLAttrib) -> EGLDisplay;
type QueryDevices = unsafe extern "C" fn(EGLint, *mut EGLDeviceEXT, *mut EGLint) -> EGLBoolean;

/// The GL calls the reader makes, looked up once.
#[allow(non_snake_case)]
pub(crate) struct Gl {
    pub GenFramebuffers: unsafe extern "C" fn(c_int, *mut c_uint),
    pub DeleteFramebuffers: unsafe extern "C" fn(c_int, *const c_uint),
    pub BindFramebuffer: unsafe extern "C" fn(c_uint, c_uint),
    pub FramebufferTexture2D: unsafe extern "C" fn(c_uint, c_uint, c_uint, c_uint, c_int),
    pub CheckFramebufferStatus: unsafe extern "C" fn(c_uint) -> c_uint,
    pub GenTextures: unsafe extern "C" fn(c_int, *mut c_uint),
    pub DeleteTextures: unsafe extern "C" fn(c_int, *const c_uint),
    pub BindTexture: unsafe extern "C" fn(c_uint, c_uint),
    pub TexImage2D: unsafe extern "C" fn(
        c_uint,
        c_int,
        c_int,
        c_int,
        c_int,
        c_int,
        c_uint,
        c_uint,
        *const c_void,
    ),
    pub TexParameteri: unsafe extern "C" fn(c_uint, c_uint, c_int),
    pub ReadPixels: unsafe extern "C" fn(c_int, c_int, c_int, c_int, c_uint, c_uint, *mut c_void),
    pub PixelStorei: unsafe extern "C" fn(c_uint, c_int),
    pub GetString: unsafe extern "C" fn(c_uint) -> *const u8,
    pub GetError: unsafe extern "C" fn() -> c_uint,
}

pub(crate) const GL_FRAMEBUFFER: c_uint = 0x8D40;
pub(crate) const GL_TEXTURE_2D: c_uint = 0x0DE1;
pub(crate) const GL_RGBA: c_uint = 0x1908;
pub(crate) const GL_RGBA8: c_int = 0x8058;
pub(crate) const GL_UNSIGNED_BYTE: c_uint = 0x1401;
pub(crate) const GL_COLOR_ATTACHMENT0: c_uint = 0x8CE0;
pub(crate) const GL_FRAMEBUFFER_COMPLETE: c_uint = 0x8CD5;
pub(crate) const GL_TEXTURE_MIN_FILTER: c_uint = 0x2801;
pub(crate) const GL_TEXTURE_MAG_FILTER: c_uint = 0x2800;
pub(crate) const GL_LINEAR: c_int = 0x2601;
pub(crate) const GL_PACK_ALIGNMENT: c_uint = 0x0D05;
pub(crate) const GL_RENDERER: c_uint = 0x1F01;

/// A current, window-less GL context. Lives on the thread that made it.
pub(crate) struct Context {
    display: EGLDisplay,
    context: EGLContext,
    pub gl: Gl,
    pub description: String,
    /// The driver's `GL_RENDERER` (`llvmpipe (LLVM 19.1.7, 256 bits)`).
    pub renderer: String,
}

/// mpv's `get_proc_address`.
pub(crate) unsafe extern "C" fn proc_address(
    _ctx: *mut c_void,
    name: *const c_char,
) -> *mut c_void {
    // SAFETY: mpv passes a NUL-terminated name.
    unsafe { eglGetProcAddress(name) }
}

fn lookup(name: &CStr) -> Option<*mut c_void> {
    // SAFETY: a NUL-terminated name.
    let pointer = unsafe { eglGetProcAddress(name.as_ptr()) };
    (!pointer.is_null()).then_some(pointer)
}

fn has_extension(display: EGLDisplay, name: &str) -> bool {
    // SAFETY: a null display asks for the client extensions; the string is static.
    let text = unsafe { eglQueryString(display, EGL_EXTENSIONS) };
    !text.is_null()
        && unsafe { CStr::from_ptr(text) }
            .to_string_lossy()
            .split(' ')
            .any(|ext| ext == name)
}

impl Context {
    /// Makes a context and makes it current on this thread, or says why not.
    pub fn new() -> Result<Context, String> {
        for (label, display) in candidate_displays() {
            if display.is_null() {
                continue;
            }
            let (mut major, mut minor) = (0, 0);
            // SAFETY: a display from EGL; out-parameters are locals.
            if unsafe { eglInitialize(display, &mut major, &mut minor) } == 0 {
                continue;
            }
            match Self::on(display) {
                Ok((context, gl, api)) => {
                    // SAFETY: a context is current; the string is static.
                    let renderer = unsafe {
                        let text = (gl.GetString)(GL_RENDERER);
                        if text.is_null() {
                            String::new()
                        } else {
                            CStr::from_ptr(text.cast()).to_string_lossy().into_owned()
                        }
                    };
                    return Ok(Context {
                        display,
                        context,
                        gl,
                        description: format!("EGL {major}.{minor} {label}, {api}, {renderer}"),
                        renderer,
                    });
                }
                Err(error) => {
                    eprintln!("[media] EGL {label}: {error}");
                    // SAFETY: initialised above, nothing made on it survives.
                    unsafe { eglTerminate(display) };
                }
            }
        }
        Err("no EGL display can make an off-screen OpenGL context".into())
    }

    fn on(display: EGLDisplay) -> Result<(EGLContext, Gl, &'static str), String> {
        // Desktop OpenGL first (mpv's best path), then OpenGL ES 3.
        let attempts: [(EGLenum, EGLint, &[EGLint], &'static str); 2] = [
            (EGL_OPENGL_API, EGL_OPENGL_BIT, &[EGL_NONE], "OpenGL"),
            (
                EGL_OPENGL_ES_API,
                EGL_OPENGL_ES3_BIT,
                &[EGL_CONTEXT_CLIENT_VERSION, 3, EGL_NONE],
                "OpenGL ES 3",
            ),
        ];
        for (api, bit, context_attribs, name) in attempts {
            // SAFETY: plain EGL calls with valid arguments.
            unsafe {
                if eglBindAPI(api) == 0 {
                    continue;
                }
                let mut config: EGLConfig = ptr::null_mut();
                let mut count = 0;
                // Surface type 0: any config, since nothing is drawn to a surface.
                let attribs = [EGL_RENDERABLE_TYPE, bit, EGL_SURFACE_TYPE, 0, EGL_NONE];
                if eglChooseConfig(display, attribs.as_ptr(), &mut config, 1, &mut count) == 0
                    || count == 0
                {
                    continue;
                }
                let context =
                    eglCreateContext(display, config, ptr::null_mut(), context_attribs.as_ptr());
                if context.is_null() {
                    continue;
                }
                if eglMakeCurrent(display, ptr::null_mut(), ptr::null_mut(), context) == 0 {
                    eglDestroyContext(display, context);
                    continue;
                }
                match load_gl() {
                    Some(gl) => return Ok((context, gl, name)),
                    None => {
                        eglMakeCurrent(display, ptr::null_mut(), ptr::null_mut(), ptr::null_mut());
                        eglDestroyContext(display, context);
                    }
                }
            }
        }
        // SAFETY: no preconditions.
        Err(format!("no OpenGL context (EGL error {:#x})", unsafe {
            eglGetError()
        }))
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        // SAFETY: the context is current on this thread and made on this display.
        unsafe {
            eglMakeCurrent(
                self.display,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
            );
            eglDestroyContext(self.display, self.context);
            eglTerminate(self.display);
        }
    }
}

fn candidate_displays() -> Vec<(&'static str, EGLDisplay)> {
    let mut displays = Vec::new();
    let platform: Option<GetPlatformDisplay> = lookup(c"eglGetPlatformDisplay")
        .or_else(|| lookup(c"eglGetPlatformDisplayEXT"))
        // SAFETY: the EGL 1.5 / EXT_platform_base signature.
        .map(|pointer| unsafe { std::mem::transmute::<*mut c_void, GetPlatformDisplay>(pointer) });
    if let Some(get) = platform {
        if has_extension(ptr::null_mut(), "EGL_MESA_platform_surfaceless") {
            // SAFETY: the surfaceless platform takes EGL_DEFAULT_DISPLAY.
            displays.push(("surfaceless", unsafe {
                get(EGL_PLATFORM_SURFACELESS_MESA, ptr::null_mut(), ptr::null())
            }));
        }
        if has_extension(ptr::null_mut(), "EGL_EXT_platform_device")
            && let Some(query) = lookup(c"eglQueryDevicesEXT")
        {
            // SAFETY: EXT_device_enumeration's signature.
            let query = unsafe { std::mem::transmute::<*mut c_void, QueryDevices>(query) };
            let mut device: EGLDeviceEXT = ptr::null_mut();
            let mut count = 0;
            // SAFETY: room for one device.
            if unsafe { query(1, &mut device, &mut count) } != 0 && count > 0 {
                // SAFETY: a device from the enumeration.
                displays.push(("device", unsafe {
                    get(EGL_PLATFORM_DEVICE_EXT, device, ptr::null())
                }));
            }
        }
    }
    // SAFETY: EGL_DEFAULT_DISPLAY.
    displays.push(("default", unsafe { eglGetDisplay(ptr::null_mut()) }));
    displays
}

fn load_gl() -> Option<Gl> {
    macro_rules! get {
        ($name:literal) => {
            // SAFETY: the signature of the named GL entry point.
            unsafe { std::mem::transmute::<*mut c_void, _>(lookup($name)?) }
        };
    }
    Some(Gl {
        GenFramebuffers: get!(c"glGenFramebuffers"),
        DeleteFramebuffers: get!(c"glDeleteFramebuffers"),
        BindFramebuffer: get!(c"glBindFramebuffer"),
        FramebufferTexture2D: get!(c"glFramebufferTexture2D"),
        CheckFramebufferStatus: get!(c"glCheckFramebufferStatus"),
        GenTextures: get!(c"glGenTextures"),
        DeleteTextures: get!(c"glDeleteTextures"),
        BindTexture: get!(c"glBindTexture"),
        TexImage2D: get!(c"glTexImage2D"),
        TexParameteri: get!(c"glTexParameteri"),
        ReadPixels: get!(c"glReadPixels"),
        PixelStorei: get!(c"glPixelStorei"),
        GetString: get!(c"glGetString"),
        GetError: get!(c"glGetError"),
    })
}
