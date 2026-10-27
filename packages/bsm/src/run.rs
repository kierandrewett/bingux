//! Resolve settings and run: the compositor on the calling thread, the manager runtime on
//! its own threads, readiness and watchdog for systemd.

use std::ffi::CString;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use bsm_compositor::{Backend, HeadlessConfig, HeadlessDrive};
use bsm_manager::core::{Caller, Inhibitor, Policy};
use bsm_manager::runtime::dbus::{self, BusKind};
use bsm_manager::runtime::{self, ControlConfig, HelperEndpoint, RtInput, RuntimeConfig, SystemServices};
use bsm_proto::error::{BsmError, ErrorCode};
use bsm_proto::scene::{CoverColor, OutputInfo, SceneCommand};
use bsm_proto::state::PowerAction;

use crate::adapters::{PublisherBackend, SystemAccounts, system_publisher};
use crate::cli::{Cli, DbusMode, parse_backend, parse_dbus};
use crate::config::{DEFAULT_PATH, FileConfig};
use crate::notify;

/// Everything resolved from the command line and the configuration file.
#[derive(Debug, Clone)]
pub struct Settings {
    pub backend: BackendName,
    pub runtime_dir: PathBuf,
    pub state_dir: PathBuf,
    pub helper_socket: PathBuf,
    pub greeter_user: String,
    pub greeter_socket_mode: u32,
    pub cover: CoverColor,
    pub dbus: DbusMode,
    /// polkit, logind inhibitors and power calls go to the system bus whatever `dbus` is.
    pub logind: bool,
    pub outputs: Vec<(u32, u32)>,
    pub frame_dump: Option<PathBuf>,
    pub hidden_users: Vec<String>,
    pub file: FileConfig,
    pub config_path: PathBuf,
}

/// `Backend` has no `Debug`/`Clone` we can rely on, so keep the name and convert late.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendName {
    Drm,
    Winit,
    Headless,
}

impl BackendName {
    pub fn backend(self) -> Backend {
        match self {
            Self::Drm => Backend::Drm,
            Self::Winit => Backend::Winit,
            Self::Headless => Backend::Headless,
        }
    }
}

pub fn resolve(cli: Cli) -> Result<Settings, String> {
    let (config_path, required) = match &cli.config {
        Some(p) => (p.clone(), true),
        None => (PathBuf::from(DEFAULT_PATH), false),
    };
    let file = FileConfig::load(&config_path, required).map_err(|e| e.to_string())?;
    let backend_name = cli.backend.clone().or_else(|| file.backend.clone()).unwrap_or_else(|| "drm".into());
    let backend = match parse_backend(&backend_name).map_err(|e| format!("backend: {e}"))? {
        Backend::Drm => BackendName::Drm,
        Backend::Winit => BackendName::Winit,
        Backend::Headless => BackendName::Headless,
    };
    let dbus_name = cli.dbus.clone().or_else(|| file.dbus.clone()).unwrap_or_else(|| "system".into());
    let dbus = parse_dbus(&dbus_name).map_err(|e| format!("dbus: {e}"))?;
    let services = cli.system_services.clone().or_else(|| file.system_services.clone()).unwrap_or_else(|| {
        if dbus == DbusMode::None { "none".into() } else { "logind".into() }
    });
    if services != "logind" && services != "none" {
        return Err(format!("system_services must be logind or none, not {services:?}"));
    }
    let cover = file.cover_color().map_err(|e| e.to_string())?.unwrap_or(CoverColor { r: 0.05, g: 0.06, b: 0.09 });
    let greeter_socket_mode = file.socket_mode().map_err(|e| e.to_string())?;
    Ok(Settings {
        backend,
        runtime_dir: cli.runtime_dir.clone().or_else(|| file.runtime_dir.clone()).unwrap_or_else(|| bsm_proto::paths::RUNTIME_DIR.into()),
        state_dir: cli.state_dir.clone().or_else(|| file.state_dir.clone()).unwrap_or_else(|| bsm_proto::paths::STATE_DIR.into()),
        helper_socket: cli
            .helper_socket
            .clone()
            .or_else(|| file.helper_socket.clone())
            .unwrap_or_else(|| bsm_proto::paths::HELPER_SOCKET.into()),
        greeter_user: file.greeter_user.clone().unwrap_or_else(|| bsm_proto::paths::GREETER_USER.into()),
        greeter_socket_mode,
        cover,
        dbus,
        logind: services == "logind",
        outputs: cli.outputs.clone().unwrap_or_else(|| vec![(1280, 720)]),
        frame_dump: cli.frame_dump.clone(),
        hidden_users: file.hidden_users.clone(),
        file,
        config_path,
    })
}

impl Settings {
    pub fn describe(&self) -> String {
        format!(
            "backend = {:?}\nruntime_dir = {}\nstate_dir = {}\nhelper_socket = {}\ngreeter_user = {}\nbus = {:?}\nconfig = {}\n",
            self.backend,
            self.runtime_dir.display(),
            self.state_dir.display(),
            self.helper_socket.display(),
            self.greeter_user,
            self.dbus,
            self.config_path.display()
        )
    }

    pub fn greeter_socket(&self) -> PathBuf {
        self.runtime_dir.join("greeter-wayland.sock")
    }
    pub fn control_socket(&self) -> PathBuf {
        self.runtime_dir.join("greeter-control.sock")
    }
    pub fn session_dir(&self) -> PathBuf {
        self.runtime_dir.join("sessions")
    }
}

/// `(uid, gid)` of an account, through NSS.
pub fn lookup_user(name: &str) -> Option<(u32, u32)> {
    let c = CString::new(name).ok()?;
    let mut pwd: libc::passwd = unsafe { std::mem::zeroed() };
    let mut buf = vec![0u8; 16384];
    let mut out: *mut libc::passwd = std::ptr::null_mut();
    // SAFETY: all pointers are valid for the call; the result points into `buf` and `pwd`.
    let rc = unsafe { libc::getpwnam_r(c.as_ptr(), &mut pwd, buf.as_mut_ptr() as *mut libc::c_char, buf.len(), &mut out) };
    if rc == 0 && !out.is_null() { Some((pwd.pw_uid, pwd.pw_gid)) } else { None }
}

/// The process name is `bsm`: the binary is named `bsm` and exec'd directly. This keeps it
/// true when someone starts it through a wrapper that renames the process.
pub fn ensure_comm() {
    let current = std::fs::read_to_string("/proc/self/comm").unwrap_or_default();
    if current.trim() != "bsm" {
        // SAFETY: PR_SET_NAME reads a NUL terminated string of at most 16 bytes.
        unsafe { libc::prctl(libc::PR_SET_NAME, c"bsm".as_ptr()) };
        tracing::warn!("process name was {:?}; set to \"bsm\"", current.trim());
    }
}

pub fn init_logging() {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_env("BSM_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    let b = tracing_subscriber::fmt().with_env_filter(filter).with_target(false).with_writer(std::io::stderr);
    if std::env::var("BSM_LOG_FORMAT").is_ok_and(|v| v == "json") {
        b.json().init();
    } else {
        b.init();
    }
}

/// Stand-in used only with `--system-services none`: the
/// requests that need polkit and logind are refused with a stable error, never granted.
struct NoSystemBus;

impl SystemServices for NoSystemBus {
    fn polkit_check(&self, _: &Caller, _: &str) -> Result<bool, BsmError> {
        Err(BsmError::new(ErrorCode::NotSupported, "polkit is not available without a bus"))
    }
    fn list_inhibitors(&self) -> Result<Vec<Inhibitor>, BsmError> {
        Err(BsmError::new(ErrorCode::NotSupported, "logind is not available without a bus"))
    }
    fn power(&self, _: PowerAction) -> Result<(), BsmError> {
        Err(BsmError::new(ErrorCode::NotSupported, "logind is not available without a bus"))
    }
    fn sleep_ready(&self) {}
}

fn block_signals() {
    // SAFETY: standard signal mask manipulation before any thread exists.
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        libc::sigaddset(&mut set, libc::SIGTERM);
        libc::sigaddset(&mut set, libc::SIGINT);
        libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut());
    }
}

fn spawn_signal_thread(tx: runtime::DriverTx) -> std::io::Result<()> {
    std::thread::Builder::new().name("bsm-signals".into()).spawn(move || {
        // SAFETY: waits for exactly the signals blocked in `block_signals`.
        let sig = unsafe {
            let mut set: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut set);
            libc::sigaddset(&mut set, libc::SIGTERM);
            libc::sigaddset(&mut set, libc::SIGINT);
            let mut sig = 0;
            libc::sigwait(&set, &mut sig);
            sig
        };
        tracing::info!(signal = sig, "stopping");
        notify::notify("STOPPING=1");
        let _ = tx.send(RtInput::Shutdown);
    })?;
    Ok(())
}

fn headless_outputs(sizes: &[(u32, u32)]) -> Vec<OutputInfo> {
    let mut x = 0i32;
    sizes
        .iter()
        .enumerate()
        .map(|(i, (w, h))| {
            let o = OutputInfo {
                name: format!("Virtual-{}", i + 1),
                x,
                y: 0,
                width: *w,
                height: *h,
                scale_120: 120,
                refresh_mhz: 60_000,
                transform: 0,
            };
            x += *w as i32;
            o
        })
        .collect()
}

/// Run until stopped. Returns the process exit code.
pub fn run(s: Settings) -> Result<(), String> {
    ensure_comm();
    block_signals();
    let (greeter_uid, greeter_gid) = match lookup_user(&s.greeter_user) {
        Some(ids) => (ids.0, Some(ids.1)),
        None if s.backend == BackendName::Drm => {
            return Err(format!("the greeter account {:?} does not exist", s.greeter_user));
        }
        None => {
            // Development: the greeter runs as whoever runs bsm.
            let uid = unsafe { libc::getuid() };
            tracing::warn!("greeter account {:?} does not exist; using uid {uid} for development", s.greeter_user);
            (uid, None)
        }
    };
    let mut policy = s.file.apply(Policy::default());
    policy.greeter_uid = Some(greeter_uid);
    policy.greeter_wayland_socket = s.greeter_socket().to_string_lossy().into_owned();
    policy.greeter_control_socket = s.control_socket().to_string_lossy().into_owned();
    policy.hostname = std::fs::read_to_string("/etc/hostname").map(|h| h.trim().to_owned()).unwrap_or_else(|_| "bingux".into());

    let (tx, rx) = runtime::channel();
    spawn_signal_thread(tx.clone()).map_err(|e| format!("signal thread: {e}"))?;

    // The bus first: the name must be ours before anything is promised.
    let dbus_handle: Option<Arc<dbus::DbusHandle>> = match &s.dbus {
        DbusMode::None => None,
        mode => {
            let kind = match mode {
                DbusMode::System => BusKind::System,
                DbusMode::Session => BusKind::Session,
                DbusMode::Address(a) => BusKind::Address(a.clone()),
                DbusMode::None => unreachable!(),
            };
            Some(Arc::new(dbus::spawn(kind, tx.clone()).map_err(|e| format!("D-Bus service: {e}"))?))
        }
    };
    // polkit and logind always live on the system bus, wherever our own service is.
    let system: Arc<dyn SystemServices> = if s.logind {
        runtime::system::LogindSystem::start(BusKind::System, tx.clone()).map_err(|e| format!("logind: {e}"))?
    } else {
        tracing::warn!("system services are off: power actions will be refused");
        Arc::new(NoSystemBus)
    };

    let publisher = system_publisher(None);
    let mut accounts_policy = bsm_appearance::accounts::AccountPolicy::system();
    accounts_policy.extra_hidden = s.hidden_users.clone();
    let accounts = Arc::new(SystemAccounts { policy: accounts_policy, publisher: publisher.clone() });
    let appearance = Arc::new(PublisherBackend { publisher });

    let (cmd_tx, cmd_rx) = bsm_compositor::calloop::channel::channel::<SceneCommand>();
    let (evt_tx, evt_rx) = std::sync::mpsc::channel();
    let signals_handle = dbus_handle.clone();
    let cfg = RuntimeConfig {
        policy,
        state_dir: s.state_dir.clone(),
        control: ControlConfig { path: s.control_socket(), mode: s.greeter_socket_mode, gid: greeter_gid },
        helper: HelperEndpoint::Path { path: s.helper_socket.clone(), peer_uid: (s.backend == BackendName::Drm).then_some(0) },
        accounts,
        appearance,
        system,
        signals: Box::new(move |sig| {
            if let Some(h) = &signals_handle {
                h.emit(sig);
            }
        }),
        proc_root: PathBuf::from("/proc"),
        rng: None,
    };
    let handle = runtime::start(cfg, tx, rx, Box::new(move |c| cmd_tx.send(c).is_ok()), evt_rx)
        .map_err(|e| format!("cannot start the session manager: {e}"))?;
    let handle = Arc::new(handle);

    // Readiness and watchdog.
    let alive = Arc::new(AtomicBool::new(true));
    {
        let (handle, dbus_handle, alive) = (handle.clone(), dbus_handle.clone(), alive.clone());
        std::thread::Builder::new()
            .name("bsm-notify".into())
            .spawn(move || {
                let ready = handle.wait_ready(Duration::from_secs(120))
                    && dbus_handle.as_ref().is_none_or(|d| {
                        let deadline = std::time::Instant::now() + Duration::from_secs(30);
                        while !d.name_acquired() && std::time::Instant::now() < deadline {
                            std::thread::sleep(Duration::from_millis(50));
                        }
                        d.name_acquired()
                    });
                if ready {
                    tracing::info!("ready");
                    notify::notify("READY=1\nSTATUS=ready");
                } else {
                    tracing::error!("not ready after 120 s");
                }
                if let Some(every) = notify::watchdog_interval() {
                    while alive.load(Ordering::Relaxed) && !handle.is_finished() {
                        notify::notify("WATCHDOG=1");
                        std::thread::sleep(every);
                    }
                }
            })
            .map_err(|e| format!("notify thread: {e}"))?;
    }

    let result = run_compositor(&s, cmd_rx, evt_tx);
    alive.store(false, Ordering::Relaxed);
    match Arc::try_unwrap(handle) {
        Ok(h) => h.shutdown(),
        Err(h) => {
            let _ = h.sender().send(RtInput::Shutdown);
        }
    }
    result
}

fn run_compositor(
    s: &Settings,
    cmd_rx: bsm_compositor::calloop::channel::Channel<SceneCommand>,
    evt_tx: std::sync::mpsc::Sender<bsm_proto::scene::SceneEvent>,
) -> Result<(), String> {
    let mut config = bsm_compositor::Config {
        backend: s.backend.backend(),
        runtime_dir: s.runtime_dir.clone(),
        greeter_socket: s.greeter_socket(),
        greeter_socket_mode: s.greeter_socket_mode,
        greeter_socket_gid: lookup_user(&s.greeter_user).map(|ids| ids.1),
        session_socket_dir: s.session_dir(),
        cover: s.cover,
        frame_dump_dir: s.frame_dump.clone(),
        ..Default::default()
    };
    if s.backend == BackendName::Headless {
        config.headless = HeadlessConfig { outputs: headless_outputs(&s.outputs), drive: HeadlessDrive::Wall, draw_cursor: false };
    }
    bsm_compositor::run(config, cmd_rx, evt_tx).map_err(|e| format!("compositor: {e:#}"))
}
