//! The bridge proper: one `calloop` loop on the main thread that owns the
//! Wayland connection to bsm and reacts to Gnoblin, PipeWire, EIS and
//! `ready.sock` events.
//!
//! Per bsm output the bridge walks these phases:
//!
//! ```text
//! WaitConfigure -> WaitMonitor -> WaitStream -> WaitLayout -> Applied
//!   configure       RecordVirtual   PipeWire node   stream      DisplayConfig
//!   acknowledged    requested       connected       streaming   applied
//! ```
//!
//! Only `Applied` outputs are reported as configured. Virtual monitors are
//! created one at a time because Mutter names them `Meta-N` in creation order
//! and the bridge assigns each new connector to the monitor it just created.

use std::cell::Cell;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::rc::Rc;
use std::os::fd::OwnedFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result, anyhow};
use calloop::generic::Generic;
use calloop::{EventLoop, Interest, LoopHandle, LoopSignal, Mode, PostAction, RegistrationToken};
use wayland_client::globals::{GlobalList, GlobalListContents, registry_queue_init};
use wayland_client::protocol::{
    wl_buffer, wl_callback, wl_compositor, wl_keyboard, wl_output, wl_pointer, wl_region, wl_registry, wl_seat, wl_shm,
    wl_shm_pool, wl_surface, wl_touch,
};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, WEnum};

use crate::frames::{BufferSet, BufferTag, OutputGate, shm_image};
use crate::gnoblin::{self, Layout};
use crate::input::{Ei, LogicalRect};
use crate::msg::{CursorImage, Msg, Rect, StreamStatus};
use crate::proto::bsm_session_host_v1::{self, BsmSessionHostV1, ReportKind};
use crate::proto::bsm_session_surface_v1::{self, BsmSessionSurfaceV1};
use crate::readiness::{ReadyLine, Readiness, Report};
use crate::report_queue::ReportQueue;
use crate::ready_sock::{self, LineConn, Lines};
use crate::stream;

/// Process exit status. Documented in `docs/bsm/bridge.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    /// bsm closed the session or the bridge was terminated.
    Clean = 0,
    /// Unexpected internal error.
    Internal = 1,
    /// Usage or configuration error.
    Usage = 2,
    /// The connection to bsm was lost.
    BsmLost = 3,
    /// Gnoblin (or Mutter, PipeWire) is gone: the session is lost.
    GnoblinLost = 4,
    /// bsm raised a protocol error.
    Protocol = 5,
}

pub struct Config {
    pub host_socket: PathBuf,
    pub runtime_dir: PathBuf,
    pub env_generation: Option<String>,
    pub env_session_id: Option<String>,
    pub background_interval: Duration,
    /// Executable names allowed to report on `ready.sock` (`*` allows all).
    pub ready_peers: Vec<String>,
    /// How long to keep trying to reach bsm again after the connection was lost.
    pub recovery_window: Duration,
    /// A `ready.sock` connection that sends no valid line in this time is closed.
    pub ready_idle: Duration,
    /// From the user's Bingux configuration: whether the dock is enabled.
    pub dock_enabled: bool,
    pub test: Option<TestMode>,
}

/// Test-only behaviour of `--test-pattern` (see `bridge/testing.rs`).
#[derive(Clone)]
pub struct TestMode {
    /// Feed the Bingux readiness events itself after this delay.
    pub auto_ready: Option<Duration>,
    /// Append forwarded input events as JSON lines to this file.
    pub input_log: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Phase {
    WaitConfigure,
    WaitMonitor,
    Requested,
    WaitStream,
    WaitLayout,
    Applied,
}

#[derive(Debug, Clone, Copy)]
struct Geometry {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    scale_120: u32,
    refresh_mhz: u32,
    transform: u32,
}

#[derive(Debug, Clone, Copy)]
struct Configure {
    width: u32,
    height: u32,
    scale_120: u32,
    transform: u32,
}

struct Output {
    geom: Geometry,
    surface: wl_surface::WlSurface,
    role: BsmSessionSurfaceV1,
    configure: Option<Configure>,
    phase: Phase,
    epoch: u64,
    connector: Option<String>,
    gate: Arc<OutputGate>,
    set: Option<Arc<BufferSet>>,
    retired: Vec<Arc<BufferSet>>,
    /// The slot holding the newest committed frame.
    last_slot: Option<(Arc<BufferSet>, usize)>,
    presented: u64,
    committed: u64,
    cursor: Option<CursorBuf>,
    cursor_hidden: bool,
    /// The monitor's rectangle in Gnoblin's logical space once laid out.
    logical: Option<LogicalRect>,
    /// Its wayland objects belong to a connection to bsm that is gone.
    stale: bool,
    /// The newest frame of the previous connection (memory only), presented
    /// again on the new surface so bsm sees a frame after the reconnect.
    carry: Option<(Arc<BufferSet>, usize)>,
    /// Re-announced by bsm after a reconnect and waiting for its configure.
    reattached: bool,
    /// Position, scale or transform differ from before the reconnect.
    relayout_needed: bool,
}

struct CursorBuf {
    buffer: wl_buffer::WlBuffer,
    hot_x: i32,
    hot_y: i32,
}

#[derive(Default)]
struct PendingScroll {
    dx: f64,
    dy: f64,
    disc_x: i32,
    disc_y: i32,
    stop_x: bool,
    stop_y: bool,
}

#[derive(Default)]
#[cfg_attr(not(feature = "test-pattern"), allow(dead_code))]
struct TestState {
    log: Option<std::fs::File>,
    tick: u64,
    /// Connectors the generated Bingux lines were last sent for.
    ready_for: Vec<String>,
}

#[cfg(feature = "test-pattern")]
mod testing;

#[cfg(not(feature = "test-pattern"))]
impl Bridge {
    fn is_test(&self) -> bool {
        false
    }
    fn test_input(&mut self, _v: serde_json::Value) {}
    fn test_layers(&mut self) {}
    fn start_test_timers(&mut self) -> Result<()> {
        Ok(())
    }
}

pub struct Bridge {
    cfg: Config,
    conn: Connection,
    qh: QueueHandle<Bridge>,
    handle: LoopHandle<'static, Bridge>,
    signal: LoopSignal,
    tx: calloop::channel::Sender<Msg>,

    compositor: wl_compositor::WlCompositor,
    shm: wl_shm::WlShm,
    host: BsmSessionHostV1,
    pointer: Option<wl_pointer::WlPointer>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    touch: Option<wl_touch::WlTouch>,
    cursor_surface: wl_surface::WlSurface,

    session_id: String,
    generation: String,
    uid: u32,
    readiness: Readiness,

    outputs: BTreeMap<String, Output>,
    surfaces: HashMap<wayland_client::backend::ObjectId, String>,
    monitor_queue: VecDeque<String>,
    in_flight: Option<String>,
    next_epoch: u64,
    next_set: u64,

    input_active: bool,
    pointer_focus: Option<String>,
    pointer_enter_serial: u32,
    touch_outputs: HashMap<i32, String>,
    scroll: PendingScroll,

    gn: Option<gnoblin::Handle>,
    pw: Option<stream::StreamThread>,
    ei: Option<Ei>,
    ei_token: Option<RegistrationToken>,
    /// The virtual monitors are being recreated; the old EIS connection ends and a new one follows.
    rebuilding: bool,
    gnoblin_ready: bool,
    bus_up: bool,
    ready_conns: usize,

    #[cfg_attr(not(feature = "test-pattern"), allow(dead_code))]
    test: TestState,

    exit: Option<Exit>,

    /// The connection to bsm is up. While it is not, Gnoblin and the apps keep
    /// running, nothing is forwarded and the bridge reconnects (see `on_bsm_lost`).
    connected: bool,
    wl_token: Option<RegistrationToken>,
    lost_since: Option<Instant>,
    /// Re-announcing outputs after a reconnect; `sync_cb` marks the end of the burst.
    reconnecting: bool,
    sync_cb: Option<wl_callback::WlCallback>,
    reports: ReportQueue,
}

/// Connect to bsm, run the loop and return the exit status.
pub fn run(cfg: Config) -> Result<Exit> {
    let stream = UnixStream::connect(&cfg.host_socket)
        .with_context(|| format!("connect to bsm at {}", cfg.host_socket.display()))?;
    let conn = Connection::from_socket(stream).context("wayland connection")?;
    let (globals, queue) = registry_queue_init::<Bridge>(&conn).context("read bsm globals")?;
    let qh = queue.handle();

    let mut event_loop: EventLoop<'static, Bridge> = EventLoop::try_new()?;
    let handle = event_loop.handle();
    let signal = event_loop.get_signal();
    let (tx, rx) = calloop::channel::channel::<Msg>();

    let mut bridge = Bridge::new(cfg, conn.clone(), &globals, qh, handle.clone(), signal.clone(), tx.clone())?;

    handle
        .insert_source(rx, |event, _, b| {
            if let calloop::channel::Event::Msg(m) = event {
                b.on_msg(m);
            }
        })
        .map_err(|e| anyhow!("insert channel: {e}"))?;
    let wl_token = calloop_wayland_source::WaylandSource::new(conn.clone(), queue)
        .insert(handle.clone())
        .map_err(|e| anyhow!("insert wayland source: {e}"))?;
    let signals = calloop::signals::Signals::new(&[calloop::signals::Signal::SIGTERM, calloop::signals::Signal::SIGINT])?;
    handle
        .insert_source(signals, |_, _, b| b.on_msg(Msg::Terminate))
        .map_err(|e| anyhow!("insert signals: {e}"))?;

    bridge.start()?;
    bridge.wl_token = Some(wl_token);

    loop {
        let result = event_loop.run(None, &mut bridge, |b| {
            if b.exit.is_some() {
                b.signal.stop();
            }
        });
        match result {
            Ok(()) => break,
            Err(e) => {
                if bridge.exit.is_some() {
                    break;
                }
                if bridge.connected {
                    // The connection to bsm failed (bsm died or restarted, or raised
                    // a protocol error): keep the session and try again.
                    bridge.on_bsm_lost(&e.to_string());
                    if bridge.exit.is_none() {
                        continue;
                    }
                } else {
                    log::error!("event loop: {e}");
                    bridge.fail(Exit::Internal);
                }
                break;
            }
        }
    }
    bridge.shutdown();
    Ok(bridge.exit.unwrap_or(Exit::Clean))
}

/// What is bound on the brokered connection.
struct Bound {
    compositor: wl_compositor::WlCompositor,
    shm: wl_shm::WlShm,
    host: BsmSessionHostV1,
    cursor_surface: wl_surface::WlSurface,
}

fn bind_session(globals: &GlobalList, qh: &QueueHandle<Bridge>) -> Result<Bound> {
    let compositor = globals
        .bind::<wl_compositor::WlCompositor, _, _>(qh, 4..=6, ())
        .context("bsm does not offer wl_compositor")?;
    let shm = globals.bind::<wl_shm::WlShm, _, _>(qh, 1..=1, ()).context("bsm does not offer wl_shm")?;
    let host = globals
        .bind::<BsmSessionHostV1, _, _>(qh, 1..=1, ())
        .context("bsm does not offer bsm_session_host_v1 (is this a brokered session socket?)")?;
    // The seat's capabilities event creates the pointer, keyboard and touch objects.
    let _seat = globals.bind::<wl_seat::WlSeat, _, _>(qh, 1..=8, ()).ok();
    // Outputs are bound for completeness; geometry comes from the host protocol.
    let outputs: Vec<(u32, u32)> = globals
        .contents()
        .with_list(|l| l.iter().filter(|g| g.interface == "wl_output").map(|g| (g.name, g.version)).collect());
    for (name, version) in outputs {
        globals.registry().bind::<wl_output::WlOutput, _, _>(name, version.min(4), qh, ());
    }
    let cursor_surface = compositor.create_surface(qh, ());
    Ok(Bound { compositor, shm, host, cursor_surface })
}

impl Bridge {
    fn new(
        cfg: Config,
        conn: Connection,
        globals: &GlobalList,
        qh: QueueHandle<Bridge>,
        handle: LoopHandle<'static, Bridge>,
        signal: LoopSignal,
        tx: calloop::channel::Sender<Msg>,
    ) -> Result<Self> {
        let Bound { compositor, shm, host, cursor_surface } = bind_session(globals, &qh)?;
        let test_log = match cfg.test.as_ref().and_then(|t| t.input_log.as_ref()) {
            Some(p) => Some(std::fs::File::create(p).with_context(|| format!("create {}", p.display()))?),
            None => None,
        };
        Ok(Self {
            cfg,
            conn,
            qh,
            handle,
            signal,
            tx,
            compositor,
            shm,
            host,
            pointer: None,
            keyboard: None,
            touch: None,
            cursor_surface,
            session_id: String::new(),
            generation: String::new(),
            uid: 0,
            readiness: Readiness::new(""),
            outputs: BTreeMap::new(),
            surfaces: HashMap::new(),
            monitor_queue: VecDeque::new(),
            in_flight: None,
            next_epoch: 0,
            next_set: 0,
            input_active: false,
            pointer_focus: None,
            pointer_enter_serial: 0,
            touch_outputs: HashMap::new(),
            scroll: PendingScroll::default(),
            gn: None,
            pw: None,
            ei: None,
            ei_token: None,
            rebuilding: false,
            gnoblin_ready: false,
            bus_up: false,
            ready_conns: 0,
            test: TestState { log: test_log, ..TestState::default() },
            exit: None,
            connected: true,
            wl_token: None,
            lost_since: None,
            reconnecting: false,
            sync_cb: None,
            reports: ReportQueue::default(),
        })
    }

    /// Start the worker threads and the `ready.sock` listener.
    fn start(&mut self) -> Result<()> {
        self.start_ready_socket()?;
        // Frame statistics for the logs (debug level) every 5 s.
        self.handle
            .insert_source(calloop::timer::Timer::from_duration(Duration::from_secs(5)), |_, _, b| {
                for (n, o) in &b.outputs {
                    log::debug!("{n}: committed={} presented={} {}", o.committed, o.presented, o.gate.stats.summary());
                }
                calloop::timer::TimeoutAction::ToDuration(Duration::from_secs(5))
            })
            .map_err(|e| anyhow!("insert stats timer: {e}"))?;
        self.handle
            .insert_source(calloop::timer::Timer::from_duration(Duration::from_millis(100)), |_, _, b| {
                b.flush_reports();
                calloop::timer::TimeoutAction::ToDuration(Duration::from_millis(100))
            })
            .map_err(|e| anyhow!("insert report timer: {e}"))?;
        if self.is_test() {
            return self.start_test_timers();
        }
        self.gn = Some(gnoblin::spawn(self.tx.clone())?);
        self.pw = Some(stream::spawn(self.tx.clone())?);
        crate::layers::spawn(self.tx.clone())?;
        Ok(())
    }

    fn start_ready_socket(&mut self) -> Result<()> {
        let (listener, path) = ready_sock::bind(&self.cfg.runtime_dir)?;
        log::info!("ready socket {}", path.display());
        self.handle
            .insert_source(
                Generic::new(listener, Interest::READ, Mode::Level),
                |_, listener, b| {
                    let l: &UnixListener = &*listener;
                    if let Err(e) = b.accept_ready(l) {
                        log::warn!("ready.sock accept: {e}");
                    }
                    Ok(PostAction::Continue)
                },
            )
            .map_err(|e| anyhow!("insert ready socket: {e}"))?;
        Ok(())
    }

    fn accept_ready(&mut self, listener: &UnixListener) -> std::io::Result<()> {
        loop {
            let (stream, _) = match listener.accept() {
                Ok(x) => x,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return Ok(()),
                Err(e) => return Err(e),
            };
            // SAFETY: getuid never fails.
            let uid = unsafe { libc::getuid() };
            if !ready_sock::peer_uid_is(&stream, uid) {
                log::warn!("ready.sock: rejected a peer with another uid");
                continue;
            }
            let names = ready_sock::peer_pid(&stream).and_then(ready_sock::peer_names);
            if !ready_sock::peer_allowed(names.as_deref(), &self.cfg.ready_peers) {
                log::warn!("ready.sock: refused a same-uid peer that is not the Bingux shell ({names:?})");
                continue;
            }
            if self.ready_conns >= ready_sock::MAX_CONNECTIONS {
                log::warn!("ready.sock: too many connections");
                continue;
            }
            let mut conn = LineConn::new(stream)?;
            self.ready_conns += 1;
            let fd = conn.stream.try_clone()?;
            let seen = Rc::new(Cell::new(false));
            let closed = Rc::new(Cell::new(false));
            let token = {
                let (seen, closed) = (seen.clone(), closed.clone());
                self.handle.insert_source(Generic::new(fd, Interest::READ, Mode::Level), move |_, _, b| {
                    match conn.read() {
                        Lines::Ok { lines, open } => {
                            if !lines.is_empty() {
                                seen.set(true);
                            }
                            for l in lines {
                                b.on_ready_line(l);
                            }
                            if open {
                                Ok(PostAction::Continue)
                            } else {
                                closed.set(true);
                                b.ready_conns -= 1;
                                Ok(PostAction::Remove)
                            }
                        }
                        Lines::Violation(why) => {
                            log::warn!("ready.sock: closing connection: {why}");
                            closed.set(true);
                            b.ready_conns -= 1;
                            Ok(PostAction::Remove)
                        }
                    }
                })
            };
            // A connection that never sends a valid line is closed.
            if let Ok(token) = token {
                let _ = self.handle.insert_source(
                    calloop::timer::Timer::from_duration(self.cfg.ready_idle),
                    move |_, _, b| {
                        if !seen.get() && !closed.get() {
                            log::warn!("ready.sock: closing a connection that sent nothing");
                            closed.set(true);
                            b.ready_conns -= 1;
                            b.handle.remove(token);
                        }
                        calloop::timer::TimeoutAction::Drop
                    },
                );
            }
        }
    }

    fn on_ready_line(&mut self, line: ReadyLine) {
        log::debug!("ready.sock: {line:?}");
        let reports = self.readiness.ready_line(&line);
        self.send_reports(reports);
    }

    // ----- reports -----

    /// Queue reports for bsm: identical repeats are dropped and the rate is
    /// capped (see `report_queue.rs`). Nothing is sent while bsm is away; the
    /// current state is reported again after the reconnect.
    fn send_reports(&mut self, reports: Vec<Report>) {
        if !self.connected {
            return;
        }
        let now = Instant::now();
        for r in reports {
            for out in self.reports.submit(r, now) {
                self.transmit(out);
            }
        }
        let _ = self.conn.flush();
    }

    fn flush_reports(&mut self) {
        if !self.connected || !self.reports.has_pending() {
            return;
        }
        for out in self.reports.flush(Instant::now()) {
            self.transmit(out);
        }
        let _ = self.conn.flush();
    }

    fn transmit(&mut self, r: Report) {
        let g = self.generation.clone();
        log::info!("report {r:?}");
        match r {
            Report::GnoblinReady => self.host.report(ReportKind::GnoblinReady, None, 0, 0, g),
            Report::OutputConfigured { output, a, b } => {
                self.host.report(ReportKind::OutputConfigured, Some(output), a, b, g)
            }
            Report::BinguxLaunched => self.host.report(ReportKind::BinguxLaunched, None, 0, 0, g),
            Report::SurfacesMapped { output, a, b } => {
                self.host.report(ReportKind::SurfacesMapped, Some(output), a, b, g)
            }
            Report::DesktopReady { a, b } => {
                self.host.report(ReportKind::DesktopReady, None, a, b, g);
                // bsm counts a frame presented after this report: make sure one follows.
                let names: Vec<String> = self.outputs.keys().cloned().collect();
                for n in names {
                    self.recommit_latest(&n);
                }
            }
        }
    }

    // ----- messages from threads -----

    fn on_msg(&mut self, msg: Msg) {
        match msg {
            Msg::GnoblinBus => {
                self.bus_up = true;
                self.pump_monitors();
            }
            Msg::GnoblinReady(r) => {
                self.gnoblin_ready = r;
                let reports = self.readiness.set_gnoblin_ready(r);
                self.send_reports(reports);
            }
            Msg::GnoblinLost(why) => {
                log::error!("session lost: {why}");
                self.fail(Exit::GnoblinLost);
            }
            Msg::Eis(fd) => self.attach_ei(fd),
            Msg::MonitorStream { output, epoch, node_id } => self.on_monitor_stream(&output, epoch, node_id),
            Msg::MonitorApplied { output, epoch, placed } => self.on_monitor_applied(&output, epoch, placed),
            Msg::MonitorFailed { output, epoch, reason } => {
                if self.outputs.get(&output).is_some_and(|o| o.epoch == epoch) {
                    log::error!("virtual monitor for {output} failed: {reason}");
                    self.fail(Exit::GnoblinLost);
                }
            }
            Msg::Layers(layers) => {
                log::debug!("gnoblin layers: {layers:?}");
                let reports = self.readiness.set_layers(layers);
                self.send_reports(reports);
            }
            Msg::Stream { output, epoch, status } => self.on_stream(&output, epoch, status),
            Msg::Frame { output, epoch, set, slot, damage, seq: _ } => self.on_frame(&output, epoch, set, slot, &damage),
            Msg::Cursor { output, epoch, image } => self.on_cursor(&output, epoch, Some(image)),
            Msg::CursorHidden { output, epoch } => self.on_cursor(&output, epoch, None),
            Msg::LockRequested => self.run_bsmctl("lock"),
            Msg::Action(a) => match a.as_str() {
                "logout" | "reboot" | "poweroff" | "suspend" => self.run_bsmctl(&a),
                other => log::warn!("unsupported session action {other:?} requested by Gnoblin"),
            },
            Msg::Terminate => {
                log::info!("terminating");
                self.exit.get_or_insert(Exit::Clean);
            }
        }
    }

    /// Gnoblin's in-session requests are forwarded to bsm through `bsmctl`,
    /// which identifies the caller by its cgroup; the bridge claims nothing.
    fn run_bsmctl(&self, action: &str) {
        let program = std::env::var_os("BSMCTL").unwrap_or_else(|| "bsmctl".into());
        log::info!("forwarding {action:?} to bsm via bsmctl");
        match std::process::Command::new(program)
            .arg(action)
            .stdin(std::process::Stdio::null())
            .spawn()
        {
            Ok(mut child) => {
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
            }
            Err(e) => log::error!("cannot run bsmctl {action}: {e}"),
        }
    }

    fn fail(&mut self, code: Exit) {
        if self.exit.is_none() {
            self.exit = Some(code);
        }
    }

    fn shutdown(&mut self) {
        self.release_all_input();
        for o in self.outputs.values() {
            log::info!("output stats: {}", o.gate.stats.summary());
        }
        if let Some(pw) = self.pw.take() {
            let _ = pw.tx.send(stream::Cmd::Quit);
            let _ = pw.join.join();
        }
        if let Some(gn) = self.gn.take() {
            let _ = gn.cmd.try_send(gnoblin::Cmd::Shutdown);
            let _ = gn.join.join();
        }
        let _ = std::fs::remove_file(ready_sock::socket_path(&self.cfg.runtime_dir));
        if self.connected {
            self.host.destroy();
            let _ = self.conn.flush();
        }
    }

    // ----- losing and regaining bsm -----

    /// The connection to bsm failed. The session lives on: Gnoblin and the apps
    /// keep running, input is released and revoked, and the bridge reconnects to
    /// the same socket path with backoff for `recovery_window`. The Wayland
    /// objects of the old connection are forgotten (never destroyed: they are
    /// dead); the last frame of every output is kept to present again.
    fn on_bsm_lost(&mut self, why: &str) {
        log::warn!("lost the connection to bsm ({why}); keeping the session and reconnecting");
        self.set_input_active(false);
        self.connected = false;
        self.lost_since = Some(Instant::now());
        if let Some(t) = self.wl_token.take() {
            self.handle.remove(t);
        }
        self.pointer = None;
        self.keyboard = None;
        self.touch = None;
        self.pointer_focus = None;
        self.touch_outputs.clear();
        self.surfaces.clear();
        self.sync_cb = None;
        self.reports.reset();
        for o in self.outputs.values_mut() {
            o.stale = true;
            o.gate.set_buffers(None);
            o.carry = o.last_slot.take();
            o.set = None;
            o.retired.clear();
            o.cursor = None;
            o.presented = 0;
            o.reattached = false;
        }
        self.schedule_reconnect();
    }

    fn schedule_reconnect(&mut self) {
        let window = self.cfg.recovery_window;
        let start = self.lost_since.unwrap_or_else(Instant::now);
        let mut delay = Duration::from_millis(100);
        let timer = calloop::timer::Timer::from_duration(Duration::from_millis(50));
        let inserted = self.handle.insert_source(timer, move |_, _, b| {
            use calloop::timer::TimeoutAction;
            if b.exit.is_some() || b.connected {
                return TimeoutAction::Drop;
            }
            if start.elapsed() > window {
                log::error!("bsm did not come back within {window:?}: giving up");
                b.fail(Exit::BsmLost);
                return TimeoutAction::Drop;
            }
            match b.try_reconnect() {
                Ok(()) => TimeoutAction::Drop,
                Err(e) => {
                    log::debug!("reconnect: {e:#}");
                    let next = delay;
                    delay = (delay * 2).min(Duration::from_secs(2));
                    TimeoutAction::ToDuration(next)
                }
            }
        });
        if let Err(e) = inserted {
            log::error!("cannot schedule the reconnect: {e}");
            self.fail(Exit::Internal);
        }
    }

    /// One attempt to connect again. `Ok` means the new connection is bound and
    /// installed; the handshake continues through the normal events (`identity`,
    /// `output_added`, `configure`), with `reconnecting` set until the compositor's
    /// first burst of events is over.
    fn try_reconnect(&mut self) -> Result<()> {
        let stream = UnixStream::connect(&self.cfg.host_socket)
            .with_context(|| format!("connect to {}", self.cfg.host_socket.display()))?;
        let conn = Connection::from_socket(stream).context("wayland connection")?;
        let (globals, queue) = registry_queue_init::<Bridge>(&conn).context("read bsm globals")?;
        let qh = queue.handle();
        let bound = bind_session(&globals, &qh)?;
        let token = calloop_wayland_source::WaylandSource::new(conn.clone(), queue)
            .insert(self.handle.clone())
            .map_err(|e| anyhow!("insert wayland source: {e}"))?;
        log::info!("reconnected to bsm after {:?}", self.lost_since.map(|t| t.elapsed()).unwrap_or_default());
        self.conn = conn;
        self.qh = qh;
        self.compositor = bound.compositor;
        self.shm = bound.shm;
        self.host = bound.host;
        self.cursor_surface = bound.cursor_surface;
        self.wl_token = Some(token);
        self.connected = true;
        self.reconnecting = true;
        // Everything the compositor announces next (identity, outputs) ends at this callback.
        self.sync_cb = Some(self.conn.display().sync(&self.qh, ()));
        let _ = self.conn.flush();
        Ok(())
    }

    /// The first burst after a reconnect is over: outputs bsm no longer has are gone.
    fn on_sync_done(&mut self) {
        self.sync_cb = None;
        if !self.reconnecting {
            return;
        }
        self.reconnecting = false;
        let gone: Vec<String> = self.outputs.iter().filter(|(_, o)| o.stale).map(|(n, _)| n.clone()).collect();
        for name in gone {
            log::info!("output {name} did not come back");
            self.on_output_removed(&name);
        }
        self.lost_since = None;
    }

    // ----- outputs -----

    fn on_output_added(&mut self, name: String, geom: Geometry) {
        if self.outputs.get(&name).is_some_and(|o| !o.stale) {
            log::warn!("duplicate output {name}");
            return;
        }
        log::info!("output {name}: {}x{}+{}+{} scale {}/120 {} mHz", geom.width, geom.height, geom.x, geom.y, geom.scale_120, geom.refresh_mhz);
        let surface = self.compositor.create_surface(&self.qh, ());
        let role = self.host.get_session_surface(&surface, name.clone(), &self.qh, name.clone());
        self.surfaces.insert(surface.id(), name.clone());
        if let Some(o) = self.outputs.get_mut(&name) {
            // The same output again after a reconnect: keep its virtual monitor.
            let old = o.geom;
            o.relayout_needed =
                (old.x, old.y, old.scale_120, old.transform) != (geom.x, geom.y, geom.scale_120, geom.transform);
            o.geom = geom;
            o.surface = surface;
            o.role = role;
            o.stale = false;
            o.reattached = true;
            o.gate.set_active(false);
            return;
        }
        let gate = OutputGate::new(self.cfg.background_interval);
        gate.set_active(self.input_active);
        self.outputs.insert(
            name.clone(),
            Output {
                geom,
                surface,
                role,
                configure: None,
                phase: Phase::WaitConfigure,
                epoch: 0,
                connector: None,
                gate,
                set: None,
                retired: Vec::new(),
                last_slot: None,
                presented: 0,
                committed: 0,
                cursor: None,
                cursor_hidden: false,
                logical: None,
                stale: false,
                carry: None,
                reattached: false,
                relayout_needed: false,
            },
        );
        let reports = self.readiness.output_added(&name);
        self.send_reports(reports);
    }

    fn on_output_changed(&mut self, name: &str, geom: Geometry) {
        let Some(o) = self.outputs.get_mut(name) else { return };
        let old = o.geom;
        o.geom = geom;
        if old.x != geom.x || old.y != geom.y || old.scale_120 != geom.scale_120 || old.transform != geom.transform {
            // Position and scale change in place; the size follows with a configure.
            self.relayout();
        }
    }

    fn on_output_removed(&mut self, name: &str) {
        let Some(o) = self.outputs.remove(name) else { return };
        self.surfaces.remove(&o.surface.id());
        if let Some(pw) = &self.pw {
            let _ = pw.tx.send(stream::Cmd::Remove { output: name.to_owned() });
        }
        if !o.stale {
            o.role.destroy();
            o.surface.destroy();
            if let Some(set) = &o.set {
                set.destroy();
            }
            for set in &o.retired {
                set.destroy();
            }
        }
        self.monitor_queue.retain(|n| n != name);
        if self.in_flight.as_deref() == Some(name) {
            self.in_flight = None;
        }
        let reports = self.readiness.output_removed(name);
        self.send_reports(reports);
        if !self.is_test() {
            // Mutter cannot remove one virtual monitor: restart them without it.
            self.rebuild_monitors();
        }
    }

    fn on_configure(&mut self, name: &str, serial: u32, c: Configure) {
        let Some(o) = self.outputs.get_mut(name) else { return };
        log::info!("configure {name}: {}x{} scale {}/120 serial {serial}", c.width, c.height, c.scale_120);
        o.role.ack_configure(serial);
        let size_changed = o.configure.is_none_or(|old| old.width != c.width || old.height != c.height);
        o.configure = Some(c);
        // Opaque region and the acknowledging commit (no buffer).
        let lw = (c.width as u64 * 120 / c.scale_120.max(1) as u64) as i32;
        let lh = (c.height as u64 * 120 / c.scale_120.max(1) as u64) as i32;
        let region = self.compositor.create_region(&self.qh, ());
        region.add(0, 0, lw, lh);
        o.surface.set_opaque_region(Some(&region));
        region.destroy();
        o.surface.commit();

        let new_set = size_changed || o.set.is_none();
        if new_set {
            self.next_set += 1;
            match BufferSet::new(&self.shm, &self.qh, name, self.next_set, c.width, c.height) {
                Ok(set) => {
                    if let Some(old) = o.gate.set_buffers(Some(set.clone())) {
                        o.retired.push(old);
                    }
                    o.set = Some(set.clone());
                    o.last_slot = None;
                    // After a reconnect: present the last frame again on the new surface.
                    if let Some((old, slot)) = o.carry.take() {
                        if (old.width, old.height) == (set.width, set.height) {
                            if let Some(dst_slot) = set.claim() {
                                // SAFETY: claimed just now; the old frame is memory only.
                                unsafe { set.slot_mut(dst_slot) }.copy_from_slice(old.slot(slot));
                                Self::commit(o, &set, dst_slot, &[]);
                            }
                        }
                    }
                }
                Err(e) => {
                    log::error!("buffers for {name}: {e:#}");
                    self.fail(Exit::Internal);
                    return;
                }
            }
        }
        let phase = o.phase;
        if phase == Phase::WaitConfigure {
            o.phase = Phase::WaitMonitor;
            self.monitor_queue.push_back(name.to_owned());
            self.pump_monitors();
        } else if size_changed {
            log::info!("{name} changed size: restarting the virtual monitors");
            self.rebuild_monitors();
        } else if std::mem::take(&mut o.reattached) {
            // Back after a reconnect with the same size: the virtual monitor stays.
            let connector = o.connector.clone();
            let relayout = std::mem::take(&mut o.relayout_needed);
            let applied = phase == Phase::Applied;
            if applied {
                if let Some(c) = connector {
                    let reports = self.readiness.output_configured(name, &c);
                    self.send_reports(reports);
                }
            }
            if relayout {
                self.relayout();
            }
        } else {
            self.relayout();
        }
    }

    // ----- virtual monitors -----

    fn pump_monitors(&mut self) {
        if self.in_flight.is_some() {
            return;
        }
        if !self.is_test() && !self.bus_up {
            return;
        }
        let test = self.is_test();
        while let Some(name) = self.monitor_queue.pop_front() {
            let Some(o) = self.outputs.get_mut(&name) else { continue };
            let Some(c) = o.configure else { continue };
            if o.phase != Phase::WaitMonitor {
                continue;
            }
            self.next_epoch += 1;
            o.epoch = self.next_epoch;
            o.phase = Phase::Requested;
            self.in_flight = Some(name.clone());
            let epoch = o.epoch;
            let refresh = o.geom.refresh_mhz;
            if test {
                let connector = format!("Test-{epoch}");
                o.phase = Phase::WaitLayout;
                let lw = c.width as f64 * 120.0 / c.scale_120.max(1) as f64;
                let lh = c.height as f64 * 120.0 / c.scale_120.max(1) as f64;
                let placed = gnoblin::Placed { connector, x: o.geom.x, y: o.geom.y, w: lw, h: lh };
                self.on_monitor_applied(&name, epoch, placed);
                return;
            }
            if let Some(gn) = &self.gn {
                let _ = gn.cmd.try_send(gnoblin::Cmd::AddMonitor {
                    output: name,
                    epoch,
                    width: c.width,
                    height: c.height,
                    refresh_mhz: refresh,
                });
            }
            return;
        }
    }

    fn on_monitor_stream(&mut self, name: &str, epoch: u64, node_id: u32) {
        let Some(o) = self.outputs.get_mut(name) else { return };
        if o.epoch != epoch || o.phase != Phase::Requested {
            return;
        }
        let Some(c) = o.configure else { return };
        o.phase = Phase::WaitStream;
        if let Some(pw) = &self.pw {
            let _ = pw.tx.send(stream::Cmd::Add {
                output: name.to_owned(),
                epoch,
                node_id,
                width: c.width,
                height: c.height,
                refresh_mhz: o.geom.refresh_mhz,
                gate: o.gate.clone(),
            });
        }
    }

    fn on_stream(&mut self, name: &str, epoch: u64, status: StreamStatus) {
        let Some(o) = self.outputs.get_mut(name) else { return };
        if o.epoch != epoch {
            return;
        }
        match status {
            StreamStatus::Streaming { width, height } => {
                let Some(c) = o.configure else { return };
                if (width, height) != (c.width, c.height) {
                    log::error!("{name}: Mutter negotiated {width}x{height}, bsm wants {}x{}", c.width, c.height);
                    self.fail(Exit::GnoblinLost);
                    return;
                }
                if o.phase == Phase::WaitStream {
                    o.phase = Phase::WaitLayout;
                    self.relayout();
                }
            }
            StreamStatus::Ended => {
                log::error!("{name}: stream ended");
                self.fail(Exit::GnoblinLost);
            }
            StreamStatus::Failed(why) => {
                log::error!("{name}: stream failed: {why}");
                self.fail(Exit::GnoblinLost);
            }
        }
    }

    /// Send the layout of every monitor that exists to Gnoblin.
    fn relayout(&mut self) {
        let layout: Vec<Layout> = self
            .outputs
            .iter()
            .filter(|(_, o)| o.phase >= Phase::WaitLayout)
            .map(|(n, o)| Layout {
                output: n.clone(),
                x: o.geom.x,
                y: o.geom.y,
                scale_120: o.configure.map_or(o.geom.scale_120, |c| c.scale_120),
                transform: o.configure.map_or(o.geom.transform, |c| c.transform),
            })
            .collect();
        if layout.is_empty() {
            return;
        }
        if let Some(gn) = &self.gn {
            let _ = gn.cmd.try_send(gnoblin::Cmd::ApplyLayout(layout));
        }
    }

    fn on_monitor_applied(&mut self, name: &str, epoch: u64, placed: gnoblin::Placed) {
        let Some(o) = self.outputs.get_mut(name) else { return };
        if o.epoch != epoch {
            return;
        }
        let connector = placed.connector.as_str();
        o.connector = Some(connector.to_owned());
        o.logical = Some(LogicalRect { x: placed.x as f64, y: placed.y as f64, w: placed.w, h: placed.h });

        let first = o.phase != Phase::Applied;
        o.phase = Phase::Applied;
        if first {
            if self.in_flight.as_deref() == Some(name) {
                self.in_flight = None;
            }
            let reports = self.readiness.output_configured(name, connector);
            self.send_reports(reports);
            self.test_layers();
            self.pump_monitors();
        }
    }

    /// Drop every virtual monitor and create them again, one at a time.
    fn rebuild_monitors(&mut self) {
        self.rebuilding = true;
        if let Some(ei) = self.ei.as_mut() {
            ei.release_all();
        }
        if let Some(gn) = &self.gn {
            let _ = gn.cmd.try_send(gnoblin::Cmd::Reset);
        }
        self.in_flight = None;
        self.monitor_queue.clear();
        let names: Vec<String> = self.outputs.keys().cloned().collect();
        for n in names {
            if let Some(pw) = &self.pw {
                let _ = pw.tx.send(stream::Cmd::Remove { output: n.clone() });
            }
            let reports = self.readiness.output_unconfigured(&n);
            self.send_reports(reports);
            let o = self.outputs.get_mut(&n).unwrap();
            o.connector = None;
            o.epoch = 0;
            if o.configure.is_some() {
                o.phase = Phase::WaitMonitor;
                self.monitor_queue.push_back(n);
            } else {
                o.phase = Phase::WaitConfigure;
            }
        }
        self.pump_monitors();
    }

    // ----- frames -----

    fn on_frame(&mut self, name: &str, epoch: u64, set: Arc<BufferSet>, slot: usize, damage: &[Rect]) {
        if !self.connected {
            set.release(slot);
            return;
        }
        let Some(o) = self.outputs.get_mut(name) else {
            set.release(slot);
            return;
        };
        let current = o.set.as_ref().is_some_and(|s| s.id == set.id);
        if o.epoch != epoch || o.phase < Phase::WaitLayout || !current || o.configure.is_none() {
            set.release(slot);
            return;
        }
        Self::commit(o, &set, slot, damage);
        let _ = self.conn.flush();
    }

    fn commit(o: &mut Output, set: &Arc<BufferSet>, slot: usize, damage: &[Rect]) {
        o.surface.attach(Some(set.buffer(slot)), 0, 0);
        let (w, h) = (set.width as i32, set.height as i32);
        let mut any = false;
        for r in damage {
            let x = r.x.clamp(0, w);
            let y = r.y.clamp(0, h);
            let rw = (r.x + r.w).clamp(0, w) - x;
            let rh = (r.y + r.h).clamp(0, h) - y;
            if rw > 0 && rh > 0 {
                o.surface.damage_buffer(x, y, rw, rh);
                any = true;
            }
        }
        if !any {
            o.surface.damage_buffer(0, 0, w, h);
        }
        o.surface.commit();
        let previous = o.last_slot.as_ref().filter(|(s, _)| s.id == set.id).map(|(_, i)| *i);
        set.pin(slot, previous);
        o.last_slot = Some((set.clone(), slot));
        o.committed += 1;
    }

    /// Present the newest frame again (copied to a fresh slot).
    fn recommit_latest(&mut self, name: &str) {
        if !self.connected {
            return;
        }
        let Some(o) = self.outputs.get_mut(name) else { return };
        let Some((set, last)) = o.last_slot.clone() else {
            log::debug!("{name}: no frame to present again");
            return;
        };
        if o.set.as_ref().is_none_or(|s| s.id != set.id) {
            log::debug!("{name}: the newest frame belongs to a retired buffer set");
            return;
        }
        let Some(slot) = set.claim() else {
            log::warn!("{name}: no free slot to present the newest frame again ({})", o.gate.stats.summary());
            return;
        };
        // SAFETY: `slot` is claimed; `last` is pinned so nobody writes it.
        let dst = unsafe { set.slot_mut(slot) };
        dst.copy_from_slice(set.slot(last));
        Self::commit(o, &set, slot, &[]);
        let _ = self.conn.flush();
    }

    fn on_buffer_release(&mut self, tag: &BufferTag) {
        let Some(o) = self.outputs.get_mut(&tag.output) else { return };
        if let Some(set) = o.set.as_ref().filter(|s| s.id == tag.set_id) {
            set.release(tag.slot);
        } else if let Some(set) = o.retired.iter().find(|s| s.id == tag.set_id) {
            set.release(tag.slot);
        }
        let mut i = 0;
        while i < o.retired.len() {
            if o.retired[i].all_free() {
                o.retired.remove(i).destroy();
            } else {
                i += 1;
            }
        }
    }

    // ----- cursor -----

    fn on_cursor(&mut self, name: &str, epoch: u64, image: Option<CursorImage>) {
        let Some(o) = self.outputs.get_mut(name) else { return };
        if o.epoch != epoch {
            return;
        }
        match image {
            Some(img) => {
                let tag = BufferTag { output: "<cursor>".into(), set_id: u64::MAX, slot: 0 };
                match shm_image(&self.shm, &self.qh, img.width, img.height, &img.pixels, tag) {
                    Ok(buffer) => {
                        if let Some(old) = o.cursor.replace(CursorBuf { buffer, hot_x: img.hotspot_x, hot_y: img.hotspot_y }) {
                            old.buffer.destroy();
                        }
                        o.cursor_hidden = false;
                    }
                    Err(e) => log::warn!("cursor image: {e:#}"),
                }
            }
            None => {
                o.cursor_hidden = true;
            }
        }
        if self.pointer_focus.as_deref() == Some(name) {
            self.apply_cursor();
        }
    }

    fn apply_cursor(&mut self) {
        if !self.connected {
            return;
        }
        let (Some(pointer), Some(focus)) = (self.pointer.clone(), self.pointer_focus.clone()) else { return };
        let Some(o) = self.outputs.get(&focus) else { return };
        if o.cursor_hidden {
            pointer.set_cursor(self.pointer_enter_serial, None, 0, 0);
        } else if let Some(c) = &o.cursor {
            let scale = (o.configure.map_or(120, |c| c.scale_120) as f64 / 120.0).round().max(1.0) as i32;
            self.cursor_surface.set_buffer_scale(scale);
            self.cursor_surface.attach(Some(&c.buffer), 0, 0);
            self.cursor_surface.damage_buffer(0, 0, i32::MAX, i32::MAX);
            self.cursor_surface.commit();
            pointer.set_cursor(self.pointer_enter_serial, Some(&self.cursor_surface), c.hot_x / scale, c.hot_y / scale);
        }
        let _ = self.conn.flush();
    }

    // ----- input -----

    fn attach_ei(&mut self, fd: OwnedFd) {
        self.rebuilding = false;
        if let Some(t) = self.ei_token.take() {
            self.handle.remove(t);
        }
        let stream = UnixStream::from(fd);
        match Ei::new(stream) {
            Ok(ei) => {
                let dup = match ei.fd().try_clone_to_owned() {
                    Ok(d) => d,
                    Err(e) => {
                        log::error!("dup EIS fd: {e}");
                        return;
                    }
                };
                self.ei = Some(ei);
                let token = self.handle.insert_source(Generic::new(dup, Interest::READ, Mode::Level), |_, _, b| {
                    if let Some(ei) = b.ei.as_mut() {
                        if let Err(e) = ei.pump() {
                            b.ei = None;
                            b.ei_token = None;
                            if b.rebuilding {
                                // The sessions were stopped on purpose; a new EIS connection follows.
                                log::debug!("old EIS connection closed: {e:#}");
                            } else {
                                log::error!("{e:#}");
                                b.fail(Exit::GnoblinLost);
                            }
                            return Ok(PostAction::Remove);
                        }
                    }
                    Ok(PostAction::Continue)
                });
                self.ei_token = token.ok();
                // Handshake starts with the server's greeting; pump once in case it is already here.
                if let Some(ei) = self.ei.as_mut() {
                    let _ = ei.pump();
                }
            }
            Err(e) => log::error!("EIS: {e:#}"),
        }
    }

    fn release_all_input(&mut self) {
        if let Some(ei) = self.ei.as_mut() {
            ei.release_all();
        }
        self.test_input(serde_json::json!({"ev": "release_all"}));
        self.pointer_focus = None;
        self.touch_outputs.clear();
    }

    fn set_input_active(&mut self, active: bool) {
        if self.input_active == active {
            return;
        }
        log::info!("input {}", if active { "active" } else { "inactive" });
        self.input_active = active;
        for o in self.outputs.values() {
            o.gate.set_active(active);
        }
        if !active {
            self.release_all_input();
        }
    }

    fn region(&self, output: &str) -> Option<LogicalRect> {
        let want = self.outputs.get(output)?.logical?;
        if self.is_test() {
            return Some(want);
        }
        self.ei.as_ref()?.region_for(&want)
    }

    fn forward_key(&mut self, code: u32, pressed: bool) {
        if !self.input_active {
            return;
        }
        self.test_input(serde_json::json!({"ev":"key","code":code,"pressed":pressed}));
        if let Some(ei) = self.ei.as_mut() {
            ei.key(code, pressed);
        }
    }

    fn forward_pointer(&mut self, x: f64, y: f64) {
        if !self.input_active {
            return;
        }
        let Some(focus) = self.pointer_focus.clone() else { return };
        self.test_input(serde_json::json!({"ev":"motion","output":focus,"x":x,"y":y}));
        if let Some(region) = self.region(&focus) {
            if let Some(ei) = self.ei.as_mut() {
                ei.pointer_abs(&region, x, y);
            }
        } else {
            log::debug!("no EIS region for {focus} yet");
        }
    }

    fn flush_scroll(&mut self) {
        let s = std::mem::take(&mut self.scroll);
        if !self.input_active {
            return;
        }
        if s.disc_x != 0 || s.disc_y != 0 {
            self.test_input(serde_json::json!({"ev":"scroll_discrete","x":s.disc_x,"y":s.disc_y}));
            if let Some(ei) = self.ei.as_mut() {
                ei.scroll_discrete(s.disc_x, s.disc_y);
            }
        } else if s.dx != 0.0 || s.dy != 0.0 {
            self.test_input(serde_json::json!({"ev":"scroll","x":s.dx,"y":s.dy}));
            if let Some(ei) = self.ei.as_mut() {
                ei.scroll(s.dx as f32, s.dy as f32);
            }
        }
        if s.stop_x || s.stop_y {
            if let Some(ei) = self.ei.as_mut() {
                ei.scroll_stop(s.stop_x, s.stop_y);
            }
        }
    }

}

impl Bridge {
    fn on_identity(&mut self, session_id: String, generation: String, uid: u32) {
        log::info!("identity: session {session_id} generation {generation} uid {uid}");
        if let Some(env) = &self.cfg.env_generation {
            if *env != generation {
                log::warn!("BSM_GENERATION={env} differs from the generation bsm assigned ({generation}); using bsm's");
            }
        }
        if let Some(env) = &self.cfg.env_session_id {
            if *env != session_id {
                log::warn!("BSM_SESSION_ID={env} differs from the session bsm assigned ({session_id})");
            }
        }
        if !self.generation.is_empty() {
            // A reconnect: it must be the same session and generation.
            if session_id != self.session_id || generation != self.generation {
                log::error!(
                    "bsm identifies this connection as session {session_id} generation {generation}, not {} {}: exiting",
                    self.session_id,
                    self.generation
                );
                self.fail(Exit::Protocol);
                return;
            }
            let reports = self.readiness.resync();
            self.send_reports(reports);
            return;
        }
        self.session_id = session_id;
        self.generation = generation.clone();
        self.uid = uid;
        self.readiness = Readiness::new(&generation);
        self.readiness.set_dock_enabled(self.cfg.dock_enabled);
        if self.is_test() {
            self.gnoblin_ready = true;
            let reports = self.readiness.set_gnoblin_ready(true);
            self.send_reports(reports);
        }
    }
}

// ----- wayland dispatch -----

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for Bridge {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

macro_rules! ignore_events {
    ($($iface:ty => $data:ty),* $(,)?) => {$(
        impl Dispatch<$iface, $data> for Bridge {
            fn event(_: &mut Self, _: &$iface, _: <$iface as Proxy>::Event, _: &$data, _: &Connection, _: &QueueHandle<Self>) {}
        }
    )*};
}

ignore_events! {
    wl_compositor::WlCompositor => (),
    wl_surface::WlSurface => (),
    wl_region::WlRegion => (),
    wl_shm_pool::WlShmPool => (),
    wl_output::WlOutput => (),
    wl_shm::WlShm => (),
}

impl Dispatch<wl_callback::WlCallback, ()> for Bridge {
    fn event(state: &mut Self, _: &wl_callback::WlCallback, event: wl_callback::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let wl_callback::Event::Done { .. } = event {
            state.on_sync_done();
        }
    }
}

impl Dispatch<wl_buffer::WlBuffer, BufferTag> for Bridge {
    fn event(state: &mut Self, buffer: &wl_buffer::WlBuffer, event: wl_buffer::Event, tag: &BufferTag, _: &Connection, _: &QueueHandle<Self>) {
        if let wl_buffer::Event::Release = event {
            if tag.set_id == u64::MAX {
                // A cursor image the compositor no longer uses.
                let current = state.outputs.values().any(|o| o.cursor.as_ref().is_some_and(|c| c.buffer == *buffer));
                if !current {
                    buffer.destroy();
                }
            } else {
                state.on_buffer_release(tag);
            }
        }
    }
}

impl Dispatch<BsmSessionHostV1, ()> for Bridge {
    fn event(state: &mut Self, _: &BsmSessionHostV1, event: bsm_session_host_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        use bsm_session_host_v1::Event as E;
        match event {
            E::Identity { session_id, generation, uid } => state.on_identity(session_id, generation, uid),
            E::OutputAdded { name, x, y, width, height, scale_120, refresh_mhz, transform } => state.on_output_added(
                name,
                Geometry { x, y, width, height, scale_120, refresh_mhz, transform },
            ),
            E::OutputChanged { name, x, y, width, height, scale_120, refresh_mhz, transform } => state.on_output_changed(
                &name,
                Geometry { x, y, width, height, scale_120, refresh_mhz, transform },
            ),
            E::OutputRemoved { name } => state.on_output_removed(&name),
            E::InputState { active } => state.set_input_active(active != 0),
            E::Closed => {
                log::info!("bsm closed the session connection");
                state.exit.get_or_insert(Exit::Clean);
            }
        }
    }
}

impl Dispatch<BsmSessionSurfaceV1, String> for Bridge {
    fn event(state: &mut Self, _: &BsmSessionSurfaceV1, event: bsm_session_surface_v1::Event, name: &String, _: &Connection, _: &QueueHandle<Self>) {
        use bsm_session_surface_v1::Event as E;
        match event {
            E::Configure { serial, width, height, scale_120, transform } => {
                state.on_configure(name, serial, Configure { width, height, scale_120, transform })
            }
            E::Presented { .. } => {
                if let Some(o) = state.outputs.get_mut(name) {
                    o.presented += 1;
                }
            }
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for Bridge {
    fn event(state: &mut Self, seat: &wl_seat::WlSeat, event: wl_seat::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        if let wl_seat::Event::Capabilities { capabilities: WEnum::Value(caps) } = event {
            if caps.contains(wl_seat::Capability::Pointer) && state.pointer.is_none() {
                state.pointer = Some(seat.get_pointer(qh, ()));
            } else if !caps.contains(wl_seat::Capability::Pointer) {
                state.pointer = None;
            }
            if caps.contains(wl_seat::Capability::Keyboard) && state.keyboard.is_none() {
                state.keyboard = Some(seat.get_keyboard(qh, ()));
            } else if !caps.contains(wl_seat::Capability::Keyboard) {
                state.keyboard = None;
            }
            if caps.contains(wl_seat::Capability::Touch) && state.touch.is_none() {
                state.touch = Some(seat.get_touch(qh, ()));
            } else if !caps.contains(wl_seat::Capability::Touch) {
                state.touch = None;
            }
        }
    }
}

const BTN_PRESSED: u32 = 1;

impl Dispatch<wl_pointer::WlPointer, ()> for Bridge {
    fn event(state: &mut Self, _: &wl_pointer::WlPointer, event: wl_pointer::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        use wl_pointer::Event as E;
        match event {
            E::Enter { serial, surface, surface_x, surface_y } => {
                state.pointer_enter_serial = serial;
                state.pointer_focus = state.surfaces.get(&surface.id()).cloned();
                state.apply_cursor();
                state.forward_pointer(surface_x, surface_y);
            }
            E::Leave { .. } => state.pointer_focus = None,
            E::Motion { surface_x, surface_y, .. } => state.forward_pointer(surface_x, surface_y),
            E::Button { button, state: WEnum::Value(s), .. } => {
                if state.input_active {
                    let pressed = s as u32 == BTN_PRESSED;
                    state.test_input(serde_json::json!({"ev":"button","code":button,"pressed":pressed}));
                    if let Some(ei) = state.ei.as_mut() {
                        ei.button(button, pressed);
                    }
                }
            }
            E::Axis { axis: WEnum::Value(axis), value, .. } => match axis {
                wl_pointer::Axis::VerticalScroll => state.scroll.dy += value,
                wl_pointer::Axis::HorizontalScroll => state.scroll.dx += value,
                _ => {}
            },
            E::AxisValue120 { axis: WEnum::Value(axis), value120 } => match axis {
                wl_pointer::Axis::VerticalScroll => state.scroll.disc_y += value120,
                wl_pointer::Axis::HorizontalScroll => state.scroll.disc_x += value120,
                _ => {}
            },
            E::AxisStop { axis: WEnum::Value(axis), .. } => match axis {
                wl_pointer::Axis::VerticalScroll => state.scroll.stop_y = true,
                wl_pointer::Axis::HorizontalScroll => state.scroll.stop_x = true,
                _ => {}
            },
            E::Frame => state.flush_scroll(),
            _ => {}
        }
    }
}

const KEY_PRESSED: u32 = 1;

impl Dispatch<wl_keyboard::WlKeyboard, ()> for Bridge {
    fn event(state: &mut Self, _: &wl_keyboard::WlKeyboard, event: wl_keyboard::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        use wl_keyboard::Event as E;
        match event {
            // The keymap is not used: key codes are evdev codes and Gnoblin's
            // own keymap interprets them (see docs/bsm/bridge.md).
            E::Keymap { fd, .. } => drop(fd),
            E::Key { key, state: WEnum::Value(s), .. } => state.forward_key(key, s as u32 == KEY_PRESSED),
            E::Leave { .. } => {
                if let Some(ei) = state.ei.as_mut() {
                    ei.release_all();
                }
                state.test_input(serde_json::json!({"ev": "release_all"}));
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_touch::WlTouch, ()> for Bridge {
    fn event(state: &mut Self, _: &wl_touch::WlTouch, event: wl_touch::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        use wl_touch::Event as E;
        match event {
            E::Down { surface, id, x, y, .. } => {
                if let Some(out) = state.surfaces.get(&surface.id()).cloned() {
                    state.touch_outputs.insert(id, out.clone());
                    if state.input_active {
                        state.test_input(serde_json::json!({"ev":"touch_down","id":id,"output":out,"x":x,"y":y}));
                        if let Some(region) = state.region(&out) {
                            if let Some(ei) = state.ei.as_mut() {
                                ei.touch_down(id as u32, &region, x, y);
                            }
                        }
                    }
                }
            }
            E::Motion { id, x, y, .. } => {
                if let Some(out) = state.touch_outputs.get(&id).cloned() {
                    if state.input_active {
                        state.test_input(serde_json::json!({"ev":"touch_motion","id":id,"output":out,"x":x,"y":y}));
                        if let Some(region) = state.region(&out) {
                            if let Some(ei) = state.ei.as_mut() {
                                ei.touch_motion(id as u32, &region, x, y);
                            }
                        }
                    }
                }
            }
            E::Up { id, .. } => {
                state.touch_outputs.remove(&id);
                if state.input_active {
                    state.test_input(serde_json::json!({"ev":"touch_up","id":id}));
                    if let Some(ei) = state.ei.as_mut() {
                        ei.touch_up(id as u32);
                    }
                }
            }
            E::Cancel => {
                state.touch_outputs.clear();
                if let Some(ei) = state.ei.as_mut() {
                    ei.release_all();
                }
            }
            _ => {}
        }
    }
}
