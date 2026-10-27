//! The compositor core of `bsm`, the Bingux Session Manager.
//!
//! One thread runs [`run`]. It owns the display, the Wayland server, the scene, input
//! routing and every full-screen transition. It takes [`bsm_proto::scene::SceneCommand`]s
//! from the manager and answers with [`bsm_proto::scene::SceneEvent`]s; it never decides
//! policy. See `docs/bsm/compositor.md` for what is implemented and what is verified.
//!
//! Connections are brokered, never discovered: there is no shared Wayland socket. The
//! greeter and every hosted session get their own socket, are checked against the
//! credentials the manager registered, and see only the globals of their role
//! ([`roles::may_see`]).

pub mod engine;
pub mod harness;
pub mod peer;
pub mod protocol;
pub mod roles;

mod broker;
mod commands;
mod gfx;
#[cfg(feature = "backend-drm")]
mod gfx_drm;
#[cfg(feature = "backend-winit")]
mod gfx_winit;
mod host;
mod input;
mod render;
mod state;

use std::path::PathBuf;

use anyhow::Result;
use bsm_proto::scene::{CoverColor, OutputInfo, SceneCommand, SceneEvent};

pub use harness::{CapturedFrame, FocusSnapshot, FocusTarget, Harness, HarnessCmd, HarnessConfig};
pub use smithay::reexports::calloop;

/// Liveness of the compositor thread, for the service watchdog (review F3). The compositor
/// beats once per pass of its event loop (at least every 250 ms while idle); a wedged loop
/// stops beating, and whoever sends `WATCHDOG=1` can see it through [`Heartbeat::age`].
#[derive(Clone)]
pub struct Heartbeat(std::sync::Arc<HeartbeatInner>);

struct HeartbeatInner {
    base: std::time::Instant,
    last_ms: std::sync::atomic::AtomicU64,
}

impl Heartbeat {
    pub fn new() -> Self {
        Self(std::sync::Arc::new(HeartbeatInner {
            base: std::time::Instant::now(),
            last_ms: std::sync::atomic::AtomicU64::new(0),
        }))
    }

    /// Called by the compositor thread.
    pub(crate) fn beat(&self) {
        let ms = self.0.base.elapsed().as_millis() as u64;
        self.0.last_ms.store(ms, std::sync::atomic::Ordering::Relaxed);
    }

    /// Time since the last beat (since creation if it never beat).
    pub fn age(&self) -> std::time::Duration {
        let last = self.0.last_ms.load(std::sync::atomic::Ordering::Relaxed);
        self.0.base.elapsed().saturating_sub(std::time::Duration::from_millis(last))
    }
}

impl Default for Heartbeat {
    fn default() -> Self {
        Self::new()
    }
}

/// Which display backend `run` drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// seatd, udev, DRM/KMS, GBM, GLES and libinput. The installed system.
    Drm,
    /// A window in an existing Wayland desktop. Development.
    Winit,
    /// Pixman into memory with configurable fake outputs. Tests and CI.
    Headless,
}

/// How the headless backend gets its time and when it renders.
pub enum HeadlessDrive {
    /// Wall clock, a frame every 16 ms.
    Wall,
    /// A test drives time and frames through [`harness::HarnessCmd`]. Nothing renders on
    /// its own and the clock only moves when the harness says so.
    Stepped(calloop::channel::Channel<harness::HarnessCmd>),
}

pub struct HeadlessConfig {
    pub outputs: Vec<OutputInfo>,
    pub drive: HeadlessDrive,
    /// Compose the pointer into the frame. Tests that compare whole frames turn it off.
    pub draw_cursor: bool,
}

impl Default for HeadlessConfig {
    fn default() -> Self {
        Self {
            outputs: vec![OutputInfo {
                name: "Virtual-1".into(),
                x: 0,
                y: 0,
                width: 1280,
                height: 720,
                scale_120: 120,
                refresh_mhz: 60_000,
                transform: 0,
            }],
            drive: HeadlessDrive::Wall,
            draw_cursor: true,
        }
    }
}

pub struct Config {
    pub backend: Backend,
    /// Directory for runtime files. Defaults to `/run/bsm`.
    pub runtime_dir: PathBuf,
    /// Greeter Wayland socket.
    pub greeter_socket: PathBuf,
    /// Mode of the greeter socket (0660).
    pub greeter_socket_mode: u32,
    /// Group that owns the greeter socket, when bsm is allowed to chown it. Normally the
    /// packaging (tmpfiles, socket directory) handles ownership and this stays `None`.
    pub greeter_socket_gid: Option<u32>,
    /// Directory of per-session sockets.
    pub session_socket_dir: PathBuf,
    /// The opaque colour behind everything.
    pub cover: CoverColor,
    pub headless: HeadlessConfig,
    /// Write every composed frame of every output as a PNG here (headless and tests).
    pub frame_dump_dir: Option<PathBuf>,
    /// Seat name for libseat / libinput (DRM backend).
    pub seat_name: String,
    /// Explicit primary DRM node (DRM backend). Default: the seat's boot GPU.
    pub drm_device: Option<PathBuf>,
    /// Requests one client may have handled in one pass of the event loop before it is
    /// disconnected (default `roles::MAX_REQUESTS_PER_PASS`).
    pub request_pass_limit: u64,
    /// Beaten by the compositor thread; hand a clone to the watchdog.
    pub heartbeat: Option<Heartbeat>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            backend: Backend::Drm,
            runtime_dir: PathBuf::from(bsm_proto::paths::RUNTIME_DIR),
            greeter_socket: PathBuf::from(bsm_proto::paths::GREETER_WAYLAND_SOCKET),
            greeter_socket_mode: 0o660,
            greeter_socket_gid: None,
            session_socket_dir: PathBuf::from(bsm_proto::paths::SESSION_SOCKET_DIR),
            cover: CoverColor { r: 0.05, g: 0.06, b: 0.09 },
            headless: HeadlessConfig::default(),
            frame_dump_dir: None,
            seat_name: "seat0".into(),
            drm_device: None,
            request_pass_limit: roles::MAX_REQUESTS_PER_PASS,
            heartbeat: None,
        }
    }
}

/// Runs the compositor event loop on the calling thread until the command channel is
/// closed, the backend ends, or the harness stops it.
pub fn run(
    config: Config,
    cmd_rx: calloop::channel::Channel<SceneCommand>,
    evt_tx: std::sync::mpsc::Sender<SceneEvent>,
) -> Result<()> {
    state::run(config, cmd_rx, evt_tx)
}
