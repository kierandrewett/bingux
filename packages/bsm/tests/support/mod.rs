//! Test rig: the real manager runtime and the real headless compositor, joined as in
//! `bsm`, with a fake helper that speaks the real `bsm-helper` protocol over a
//! `SOCK_SEQPACKET` socketpair, a Quickshell-like greeter (Wayland layer surfaces plus the
//! JSON control socket) and Gnoblin-like session bridges (`bsm_session_host_v1`).
//!
//! Multiple Unix users are real: the tests run inside a user namespace with a range of
//! uids mapped (see `headless_flows.rs`), and each client thread switches its own
//! credentials with a raw `setresuid` before it connects, so the compositor's
//! `SO_PEERCRED` check sees the account's uid. Nothing in `bsm` is bypassed.
#![allow(dead_code)]

#[path = "../../../bsm-compositor/tests/common/mod.rs"]
pub mod common;

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::mem::MaybeUninit;
use std::os::fd::OwnedFd;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use parking_lot::Mutex;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use bsm_compositor::calloop::channel as cchan;
use bsm_compositor::{Backend, CapturedFrame, Config, HarnessCmd, HeadlessConfig, HeadlessDrive};
use bsm_manager::core::{
    AccountEntry, Caller, Inhibitor, Method, Policy, PublishRequest, PublishedView, Reply, Rng,
};
use bsm_manager::runtime::{
    self, AccountSource, AppearanceBackend, ControlConfig, DbusCall, DriverTx, HelperEndpoint, RtInput, RuntimeConfig,
    RuntimeHandle, SystemServices,
};
use bsm_proto::control::{ClientMessage, ServerMessage, Snapshot};
use bsm_proto::error::{BsmError, ErrorCode};
use bsm_proto::helper::{HelperEvent, HelperRequest, PamPromptKind};
use bsm_proto::scene::{CoverColor, OutputInfo, SceneEvent};
use bsm_proto::state::{AuthIntent, OperationPhase, PowerAction, SessionState, TransitionKind};
use common::{Client, Fill, Rgba};
use socket2::{Domain, Socket, Type};

pub const W: u32 = 64;
pub const H: u32 = 48;
pub const GREETER_UID: u32 = 900;
pub const ALICE: u32 = 1000;
pub const BOB: u32 = 1001;
pub const PASSWORD: &str = "correct horse";

pub const COVER: Rgba = [13, 26, 38, 255];
pub const GREETER_BAND: Rgba = [200, 60, 60, 255];
pub const GREETER_BODY: Rgba = [40, 60, 200, 255];
pub const SESSION_A: Rgba = [30, 180, 60, 255];
pub const SESSION_B: Rgba = [220, 170, 20, 255];

pub fn in_namespace() -> bool {
    std::env::var_os("BSM_IN_USERNS").is_some()
}

/// Run `f` as `uid` on this thread only: a raw `setresuid` changes the calling thread's
/// credentials, which is what `SO_PEERCRED` records at `connect`.
pub fn become_uid(uid: u32) {
    // SAFETY: raw syscall; affects the calling thread only (unlike the libc wrapper).
    let rc = unsafe { libc::syscall(libc::SYS_setresuid, uid, uid, uid) };
    assert_eq!(rc, 0, "setresuid({uid}) failed: {}", std::io::Error::last_os_error());
}

// ----- fake helper ---------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Opened {
    pub session: String,
    pub generation: u64,
    pub socket_path: PathBuf,
}

#[derive(Default)]
pub struct HelperState {
    pub users: HashMap<String, String>,
    pub opened: Vec<Opened>,
    pub closed: Vec<String>,
    pub greeter_spawns: Vec<(u64, PathBuf)>,
    pub killed_greeters: u32,
    pub locked_hints: Vec<(String, bool)>,
    pub aborted: u32,
    /// `OpenSession` is not answered while set.
    pub hold_open: bool,
    pub auth_log: Vec<(String, bool)>,
    redeemed: Vec<u64>,
    live_auth: Option<(u64, String)>,
    tickets: HashMap<u64, String>,
}

pub struct FakeHelper {
    pub state: Arc<Mutex<HelperState>>,
    sock: Arc<Socket>,
}

impl FakeHelper {
    /// Returns the helper and the descriptor the runtime connects through.
    pub fn start(users: &[(&str, &str)]) -> (Self, OwnedFd) {
        let (mine, theirs) = Socket::pair(Domain::UNIX, Type::SEQPACKET, None).expect("socketpair");
        let mut st = HelperState::default();
        for (u, p) in users {
            st.users.insert((*u).into(), (*p).into());
        }
        let state = Arc::new(Mutex::new(st));
        let sock = Arc::new(mine);
        let (s2, k2) = (state.clone(), sock.clone());
        thread::Builder::new()
            .name("fake-helper".into())
            .spawn(move || {
                let mut buf = vec![MaybeUninit::<u8>::uninit(); 70_000];
                loop {
                    let n = match k2.recv(&mut buf) {
                        Ok(0) | Err(_) => return,
                        Ok(n) => n,
                    };
                    let bytes: Vec<u8> = buf[..n].iter().map(|b| unsafe { b.assume_init() }).collect();
                    let Ok(req) = serde_json::from_slice::<HelperRequest>(&bytes) else { continue };
                    Self::handle(&s2, &k2, req);
                }
            })
            .expect("helper thread");
        (Self { state, sock }, OwnedFd::from(theirs))
    }

    pub fn send(&self, ev: HelperEvent) {
        let _ = self.sock.send(&serde_json::to_vec(&ev).unwrap());
    }

    fn reply(sock: &Socket, ev: HelperEvent) {
        let _ = sock.send(&serde_json::to_vec(&ev).unwrap());
    }

    fn handle(state: &Arc<Mutex<HelperState>>, sock: &Arc<Socket>, req: HelperRequest) {
        let mut st = state.lock();
        match req {
            HelperRequest::Ping { nonce } => Self::reply(sock, HelperEvent::Pong { nonce }),
            HelperRequest::SpawnGreeter { req_id, generation, wayland_socket, .. } => {
                st.greeter_spawns.push((generation, PathBuf::from(wayland_socket)));
                Self::reply(sock, HelperEvent::GreeterSpawned { req_id, generation, pid: std::process::id() });
            }
            HelperRequest::KillGreeter { .. } => st.killed_greeters += 1,
            HelperRequest::AuthStart { req_id, user, intent, service, .. } => {
                // The helper's own rule: Unlock uses the unlock service, Login and Switch the login service.
                let ok = match intent {
                    AuthIntent::Unlock => service == "bsm-unlock",
                    _ => service == "bsm-login",
                };
                if !ok {
                    Self::reply(sock, HelperEvent::AuthDone { req_id, ok: false, code: Some(ErrorCode::InvalidArgument), message: "service".into(), ticket: None });
                    return;
                }
                if let Some((old, _)) = st.live_auth.take() {
                    Self::reply(sock, HelperEvent::AuthDone { req_id: old, ok: false, code: Some(ErrorCode::Cancelled), message: String::new(), ticket: None });
                }
                st.live_auth = Some((req_id, user));
                Self::reply(sock, HelperEvent::AuthPrompt { req_id, kind: PamPromptKind::EchoOff, text: "Password:".into() });
            }
            HelperRequest::AuthResponse { req_id, response } => {
                let Some((live, user)) = st.live_auth.clone() else { return };
                if live != req_id {
                    return;
                }
                st.live_auth = None;
                let ok = st.users.get(&user).is_some_and(|p| *p == response);
                st.auth_log.push((user.clone(), ok));
                if ok {
                    let ticket = 100_000 + req_id;
                    st.tickets.insert(ticket, user);
                    Self::reply(sock, HelperEvent::AuthDone { req_id, ok: true, code: None, message: String::new(), ticket: Some(ticket) });
                } else {
                    Self::reply(sock, HelperEvent::AuthDone { req_id, ok: false, code: Some(ErrorCode::PamFailure), message: "Authentication failure".into(), ticket: None });
                }
            }
            HelperRequest::AuthAbort { req_id } => {
                if st.live_auth.as_ref().is_some_and(|(r, _)| *r == req_id) {
                    st.live_auth = None;
                    st.aborted += 1;
                    Self::reply(sock, HelperEvent::AuthDone { req_id, ok: false, code: Some(ErrorCode::Cancelled), message: String::new(), ticket: None });
                }
            }
            HelperRequest::OpenSession { req_id, ticket, session, generation, socket_path, .. } => {
                // A ticket opens one session, once: the helper's contract.
                if !st.tickets.contains_key(&ticket) || st.redeemed.contains(&ticket) {
                    Self::reply(sock, HelperEvent::SessionFailed { req_id, session, generation, code: ErrorCode::Unauthorised, message: "bad ticket".into() });
                    return;
                }
                st.redeemed.push(ticket);
                st.opened.push(Opened { session: session.clone(), generation, socket_path: PathBuf::from(socket_path) });
                if st.hold_open {
                    return;
                }
                Self::reply(
                    sock,
                    HelperEvent::SessionOpened {
                        req_id,
                        session,
                        generation,
                        host_pid: std::process::id(),
                        leader_pid: std::process::id(),
                        scope: common::my_scope(),
                        logind_session: Some("c1".into()),
                    },
                );
            }
            HelperRequest::CloseSession { session, generation, .. } => {
                st.closed.push(session.clone());
                let s = sock.clone();
                thread::spawn(move || {
                    thread::sleep(Duration::from_millis(30));
                    Self::reply(&s, HelperEvent::SessionExited { session, generation, status: 0 });
                });
            }
            HelperRequest::SetLockedHint { req_id, session, locked, .. } => {
                st.locked_hints.push((session, locked));
                Self::reply(sock, HelperEvent::LockedHintResult { req_id, ok: true, code: None, message: String::new() });
            }
        }
    }

    pub fn opened(&self) -> Vec<Opened> {
        self.state.lock().opened.clone()
    }
    pub fn closed(&self) -> Vec<String> {
        self.state.lock().closed.clone()
    }
}

// ----- other doubles --------------------------------------------------------------------

pub struct Accounts;

impl AccountSource for Accounts {
    fn list(&self) -> Result<Vec<AccountEntry>, BsmError> {
        Ok([(ALICE, "alice", "Alice"), (BOB, "bob", "Bob")]
            .into_iter()
            .map(|(uid, n, d)| AccountEntry { uid, user_name: n.into(), display_name: d.into(), published: None })
            .collect())
    }
}

pub struct NoAppearance;
impl AppearanceBackend for NoAppearance {
    fn publish(&self, _: u32, _: PublishRequest) -> Result<PublishedView, BsmError> {
        Err(BsmError::new(ErrorCode::NotSupported, "not used here"))
    }
}

/// polkit, logind inhibitors and the power call, as a test double (test code only).
pub struct TestSystem {
    pub polkit: AtomicBool,
    pub power_ok: AtomicBool,
    pub power_calls: Mutex<Vec<(Instant, PowerAction)>>,
    pub polkit_asked: Mutex<Vec<String>>,
    pub inhibitors: Mutex<Vec<Inhibitor>>,
    pub sleep_ready: AtomicU64,
}

impl TestSystem {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            polkit: AtomicBool::new(true),
            power_ok: AtomicBool::new(true),
            power_calls: Mutex::new(Vec::new()),
            polkit_asked: Mutex::new(Vec::new()),
            inhibitors: Mutex::new(Vec::new()),
            sleep_ready: AtomicU64::new(0),
        })
    }
}

impl SystemServices for TestSystem {
    fn polkit_check(&self, _: &Caller, action_id: &str) -> Result<bool, BsmError> {
        self.polkit_asked.lock().push(action_id.to_owned());
        Ok(self.polkit.load(Ordering::SeqCst))
    }
    fn list_inhibitors(&self) -> Result<Vec<Inhibitor>, BsmError> {
        Ok(self.inhibitors.lock().clone())
    }
    fn power(&self, action: PowerAction) -> Result<(), BsmError> {
        self.power_calls.lock().push((Instant::now(), action));
        if self.power_ok.load(Ordering::SeqCst) { Ok(()) } else { Err(BsmError::new(ErrorCode::LogindFailure, "refused")) }
    }
    fn sleep_ready(&self) {
        self.sleep_ready.fetch_add(1, Ordering::SeqCst);
    }
}

// ----- client threads -------------------------------------------------------------------

type Job = Box<dyn FnOnce(&mut Client) + Send>;

/// A Wayland client that lives on its own thread, with its own uid. Closing it (drop)
/// closes the connection, which is how tests crash a greeter or a Gnoblin.
pub struct ClientThread {
    jobs: Option<Sender<Job>>,
    join: Option<JoinHandle<()>>,
}

impl ClientThread {
    pub fn spawn(uid: u32, connect: PathBuf, setup: impl FnOnce(&mut Client) + Send + 'static) -> Self {
        let (tx, rx) = mpsc::channel::<Job>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
        let join = thread::Builder::new()
            .name(format!("client-{uid}"))
            .spawn(move || {
                become_uid(uid);
                let mut client = match Client::connect(&connect) {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                setup(&mut client);
                let _ = ready_tx.send(Ok(()));
                while let Ok(job) = rx.recv() {
                    job(&mut client);
                }
            })
            .expect("client thread");
        match ready_rx.recv_timeout(Duration::from_secs(20)) {
            Ok(Ok(())) => {}
            Ok(Err(e)) => panic!("client connect failed: {e}"),
            Err(_) => panic!("client setup timed out"),
        }
        Self { jobs: Some(tx), join: Some(join) }
    }

    pub fn run(&self, f: impl FnOnce(&mut Client) + Send + 'static) {
        let (done_tx, done_rx) = mpsc::channel();
        self.jobs
            .as_ref()
            .expect("client is closed")
            .send(Box::new(move |c| {
                f(c);
                let _ = done_tx.send(());
            }))
            .expect("client thread is gone");
        done_rx.recv_timeout(Duration::from_secs(20)).expect("client job timed out");
    }

    pub fn close(&mut self) {
        self.jobs = None;
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

impl Drop for ClientThread {
    fn drop(&mut self) {
        self.close();
    }
}

// ----- the greeter ----------------------------------------------------------------------

pub struct TestGreeter {
    pub wl: ClientThread,
    writer: UnixStream,
    pub log: Arc<Mutex<Vec<ServerMessage>>>,
    pub cursor: usize,
}

pub fn greeter_fill() -> Fill {
    Box::new(|_, y, _, h| if y < h / 4 { GREETER_BAND } else { GREETER_BODY })
}

pub fn fill_of(c: Rgba) -> Fill {
    Box::new(move |_, _, _, _| c)
}

impl TestGreeter {
    pub fn attach(wayland_socket: &Path, control_socket: &Path) -> Self {
        let wl = ClientThread::spawn(GREETER_UID, wayland_socket.to_path_buf(), |c| c.greeter_map(greeter_fill()));
        let ctl = control_socket.to_path_buf();
        let (tx, rx) = mpsc::channel();
        // The control connection is made with the greeter's credentials as well.
        thread::spawn(move || {
            become_uid(GREETER_UID);
            tx.send(UnixStream::connect(ctl)).ok();
        });
        let stream = rx.recv_timeout(Duration::from_secs(10)).expect("control connect").expect("control socket");
        let log: Arc<Mutex<Vec<ServerMessage>>> = Arc::default();
        let (reader, l2) = (stream.try_clone().unwrap(), log.clone());
        thread::spawn(move || {
            for line in BufReader::new(reader).lines() {
                let Ok(line) = line else { return };
                if let Ok(m) = serde_json::from_str::<ServerMessage>(&line) {
                    l2.lock().push(m);
                }
            }
        });
        let mut g = Self { wl, writer: stream, log, cursor: 0 };
        g.send(ClientMessage::Hello { version: bsm_proto::PROTOCOL_VERSION });
        g
    }

    pub fn send(&mut self, m: ClientMessage) {
        let mut line = serde_json::to_string(&m).unwrap();
        line.push('\n');
        self.writer.write_all(line.as_bytes()).expect("control write");
    }

    pub fn messages(&self) -> Vec<ServerMessage> {
        self.log.lock().clone()
    }

    /// Wait for the first message after `from` matching `pred`.
    pub fn wait_after(&self, from: usize, timeout: Duration, what: &str, pred: impl Fn(&ServerMessage) -> bool) -> (usize, ServerMessage) {
        let deadline = Instant::now() + timeout;
        loop {
            {
                let log = self.log.lock();
                if let Some((i, m)) = log.iter().enumerate().skip(from).find(|(_, m)| pred(m)) {
                    return (i, m.clone());
                }
            }
            if Instant::now() > deadline {
                panic!("timed out waiting for {what}; messages: {:#?}", self.log.lock().iter().skip(from).collect::<Vec<_>>());
            }
            thread::sleep(Duration::from_millis(5));
        }
    }

    pub fn mark(&self) -> usize {
        self.log.lock().len()
    }

    pub fn snapshot(&self) -> Snapshot {
        self.log
            .lock()
            .iter()
            .rev()
            .find_map(|m| match m {
                ServerMessage::Snapshot(s) => Some(s.clone()),
                _ => None,
            })
            .expect("a snapshot")
    }

    /// Answer the first prompt after `from` with `response`.
    pub fn answer(&mut self, from: usize, response: &str) -> u64 {
        let (_, m) = self.wait_after(from, Duration::from_secs(10), "an auth prompt", |m| matches!(m, ServerMessage::AuthPrompt { .. }));
        let ServerMessage::AuthPrompt { challenge_id, .. } = m else { unreachable!() };
        self.send(ClientMessage::AuthResponse { challenge_id, response: response.into() });
        challenge_id
    }

    pub fn login(&mut self, uid: u32, intent: AuthIntent, password: &str) {
        let from = self.mark();
        self.send(ClientMessage::SelectUser { uid });
        self.send(ClientMessage::BeginAuth { request_id: 1, uid, intent });
        self.answer(from, password);
    }
}

// ----- the bridge (Gnoblin side) -------------------------------------------------------

pub struct TestBridge {
    pub ct: ClientThread,
    pub session: String,
    pub color: Rgba,
}

impl TestBridge {
    /// Connect through the brokered socket and run the startup in the documented order,
    /// pausing between the steps so frames are presented in between.
    pub fn start(opened: &Opened, uid: u32, color: Rgba, settle: Duration) -> Self {
        let generation = opened.generation.to_string();
        let ct = ClientThread::spawn(uid, opened.socket_path.clone(), move |c| {
            c.host_bind();
            c.host_create_surfaces();
        });
        let b = Self { ct, session: opened.session.clone(), color };
        b.script(generation, settle);
        b
    }

    fn script(&self, generation: String, settle: Duration) {
        use common::bsm::bsm_session_host_v1::ReportKind as K;
        let color = self.color;
        self.ct.run(move |c| {
            let host = c.host.clone().unwrap();
            let outs: Vec<String> = c.app.host_outputs.iter().map(|o| o.0.clone()).collect();
            host.report(K::GnoblinReady, None, 0, 0, generation.clone());
            c.roundtrip().unwrap();
            c.host_paint(&fill_of(color));
            for o in &outs {
                host.report(K::OutputConfigured, Some(o.clone()), 0, 0, generation.clone());
            }
            c.roundtrip().unwrap();
            host.report(K::BinguxLaunched, None, 0, 0, generation.clone());
            for o in &outs {
                host.report(K::SurfacesMapped, Some(o.clone()), 3, 3, generation.clone());
            }
            c.roundtrip().unwrap();
            host.report(K::DesktopReady, None, 0, 0, generation.clone());
            c.roundtrip().unwrap();
            thread::sleep(settle);
            // A frame after the DesktopReady report.
            c.host_paint(&fill_of(color));
        });
    }

    /// Connect and bind but report nothing (a launch that never becomes ready).
    pub fn connect_only(opened: &Opened, uid: u32, color: Rgba) -> Self {
        let ct = ClientThread::spawn(uid, opened.socket_path.clone(), |c| {
            c.host_bind();
            c.host_create_surfaces();
        });
        Self { ct, session: opened.session.clone(), color }
    }

    pub fn repaint(&self) {
        let color = self.color;
        self.ct.run(move |c| c.host_paint(&fill_of(color)));
    }

    /// The Gnoblin dies: the connection closes.
    pub fn crash(&mut self) {
        self.ct.close();
    }
}

// ----- frames ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct FrameRec {
    pub seq: u64,
    pub time_ms: u64,
    pub output: String,
    pub plan: bsm_compositor::engine::Plan,
    pub top: Rgba,
    pub body: Rgba,
}

#[derive(Default)]
pub struct FrameLog {
    pub frames: Vec<FrameRec>,
    pub last: HashMap<String, CapturedFrame>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Seen {
    Cover,
    Greeter,
    A,
    B,
    Other,
}

pub fn near(a: Rgba, b: Rgba) -> bool {
    (0..3).all(|i| (a[i] as i32 - b[i] as i32).abs() <= 3)
}

/// What is visible at one pixel.
pub fn classify(p: Rgba) -> Seen {
    if near(p, COVER) {
        Seen::Cover
    } else if near(p, GREETER_BAND) || near(p, GREETER_BODY) {
        Seen::Greeter
    } else if near(p, SESSION_A) {
        Seen::A
    } else if near(p, SESSION_B) {
        Seen::B
    } else {
        Seen::Other
    }
}

impl FrameRec {
    pub fn seen(&self) -> Seen {
        classify(self.body)
    }
}

// ----- the rig --------------------------------------------------------------------------

pub struct RigConfig {
    pub policy: Policy,
    pub users: Vec<(&'static str, &'static str)>,
    pub outputs: usize,
}

impl Default for RigConfig {
    fn default() -> Self {
        Self { policy: Policy::default(), users: vec![("alice", PASSWORD), ("bob", PASSWORD)], outputs: 2 }
    }
}

pub struct Rig {
    pub rt: Option<RuntimeHandle>,
    pub tx: DriverTx,
    pub helper: FakeHelper,
    pub system: Arc<TestSystem>,
    pub frames: Arc<Mutex<FrameLog>>,
    stop: Arc<AtomicBool>,
    stepper: Option<JoinHandle<()>>,
    comp: Option<JoinHandle<anyhow::Result<()>>>,
    ctl: cchan::Sender<HarnessCmd>,
    pub dir: tempfile::TempDir,
    pub greeter_socket: PathBuf,
    pub control_socket: PathBuf,
    pub state_dir: PathBuf,
    pub greeter: Option<TestGreeter>,
    pub bridges: Vec<TestBridge>,
    pub art: PathBuf,
}

struct FixedRng(u64);
impl Rng for FixedRng {
    fn next_u64(&mut self) -> u64 {
        // Deterministic ids keep logs comparable between runs; they are never security relevant here.
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 1 | 1
    }
}

impl Rig {
    pub fn start(cfg: RigConfig) -> Rig {
        let dir = tempfile::Builder::new().prefix("bsm-rig").tempdir().unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        let greeter_socket = dir.path().join("greeter-wayland.sock");
        let control_socket = dir.path().join("greeter-control.sock");
        let session_dir = dir.path().join("sessions");
        let state_dir = dir.path().join("state");
        let art = artifacts();

        let outputs: Vec<OutputInfo> = (0..cfg.outputs)
            .map(|i| OutputInfo {
                name: format!("Virtual-{}", i + 1),
                x: (i as u32 * W) as i32,
                y: 0,
                width: W,
                height: H,
                scale_120: 120,
                refresh_mhz: 60_000,
                transform: 0,
            })
            .collect();

        let (ctl_tx, ctl_rx) = cchan::channel::<HarnessCmd>();
        let (cmd_tx, cmd_rx) = cchan::channel();
        let (evt_tx, evt_rx) = mpsc::channel::<SceneEvent>();
        let config = Config {
            backend: Backend::Headless,
            runtime_dir: dir.path().to_path_buf(),
            greeter_socket: greeter_socket.clone(),
            greeter_socket_mode: 0o666,
            greeter_socket_gid: None,
            session_socket_dir: session_dir,
            cover: CoverColor { r: COVER[0] as f32 / 255.0, g: COVER[1] as f32 / 255.0, b: COVER[2] as f32 / 255.0 },
            headless: HeadlessConfig { outputs, drive: HeadlessDrive::Stepped(ctl_rx), draw_cursor: false },
            frame_dump_dir: None,
            seat_name: "seat-test".into(),
            drm_device: None,
        };
        let comp = thread::Builder::new().name("bsm-compositor".into()).spawn(move || bsm_compositor::run(config, cmd_rx, evt_tx)).unwrap();

        let frames: Arc<Mutex<FrameLog>> = Arc::default();
        let stop = Arc::new(AtomicBool::new(false));
        let stepper = {
            let (ctl, frames, stop) = (ctl_tx.clone(), frames.clone(), stop.clone());
            thread::Builder::new()
                .name("stepper".into())
                .spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        let (tx, rx) = mpsc::channel();
                        if ctl.send(HarnessCmd::Step { dt_ms: 16, reply: tx }).is_err() {
                            return;
                        }
                        let Ok(captured) = rx.recv_timeout(Duration::from_secs(5)) else { return };
                        let mut log = frames.lock();
                        for f in captured {
                            log.frames.push(FrameRec {
                                seq: f.seq,
                                time_ms: f.time_ms,
                                output: f.output.clone(),
                                plan: f.plan.clone(),
                                top: f.pixel(f.width / 2, 1),
                                body: f.pixel(f.width / 2, f.height * 3 / 4),
                            });
                            log.last.insert(f.output.clone(), f);
                        }
                        drop(log);
                        thread::sleep(Duration::from_millis(4));
                    }
                })
                .unwrap()
        };

        let (helper, helper_fd) = FakeHelper::start(&cfg.users);
        let system = TestSystem::new();
        let mut policy = cfg.policy;
        policy.greeter_uid = Some(GREETER_UID);
        policy.greeter_wayland_socket = greeter_socket.to_string_lossy().into_owned();
        policy.greeter_control_socket = control_socket.to_string_lossy().into_owned();
        policy.host_exe = common::my_exe();

        let (tx, rx) = runtime::channel();
        let rcfg = RuntimeConfig {
            policy,
            state_dir: state_dir.clone(),
            control: ControlConfig { path: control_socket.clone(), mode: 0o666, gid: None },
            helper: HelperEndpoint::Fd(helper_fd),
            accounts: Arc::new(Accounts),
            appearance: Arc::new(NoAppearance),
            system: system.clone(),
            signals: Box::new(|_| {}),
            proc_root: PathBuf::from("/proc"),
            rng: Some(Box::new(FixedRng(7))),
        };
        let rt = runtime::start(rcfg, tx.clone(), rx, Box::new(move |c| cmd_tx.send(c).is_ok()), evt_rx).expect("runtime");
        assert!(rt.wait_ready(Duration::from_secs(10)), "the manager did not see the compositor");
        Rig {
            rt: Some(rt),
            tx,
            helper,
            system,
            frames,
            stop,
            stepper: Some(stepper),
            comp: Some(comp),
            ctl: ctl_tx,
            dir,
            greeter_socket,
            control_socket,
            state_dir,
            greeter: None,
            bridges: Vec::new(),
            art,
        }
    }

    /// Wait for the greeter spawn request and attach as the greeter.
    pub fn attach_greeter(&mut self) {
        let n = self.greeter_spawn_count();
        self.wait("the helper was asked to spawn the greeter", Duration::from_secs(10), |r| r.greeter_spawn_count() > n.saturating_sub(1) && r.greeter_spawn_count() >= 1);
        let g = TestGreeter::attach(&self.greeter_socket, &self.control_socket);
        self.greeter = Some(g);
        self.wait("a greeter snapshot", Duration::from_secs(10), |r| {
            r.greeter.as_ref().is_some_and(|g| g.messages().iter().any(|m| matches!(m, ServerMessage::Snapshot(_))))
        });
    }

    pub fn greeter_spawn_count(&self) -> usize {
        self.helper.state.lock().greeter_spawns.len()
    }

    pub fn g(&mut self) -> &mut TestGreeter {
        self.greeter.as_mut().expect("greeter attached")
    }

    pub fn wait(&self, what: &str, timeout: Duration, cond: impl Fn(&Rig) -> bool) {
        let deadline = Instant::now() + timeout;
        while !cond(self) {
            if Instant::now() > deadline {
                self.dump(what);
                panic!("timed out: {what}");
            }
            thread::sleep(Duration::from_millis(5));
        }
    }

    fn dump(&self, what: &str) {
        eprintln!("--- timed out: {what}");
        if let Some(rt) = &self.rt {
            eprintln!("runtime: {:?}", rt.snapshot());
        }
        if let Some(g) = &self.greeter {
            for m in g.messages().iter().rev().take(12).rev() {
                eprintln!("greeter got: {m:?}");
            }
        }
        eprintln!("helper opened: {:?} closed: {:?}", self.helper.opened(), self.helper.closed());
    }

    pub fn session_state(&self, sid: &str) -> Option<SessionState> {
        self.rt.as_ref()?.snapshot().sessions.iter().find(|s| s.id == sid).map(|s| s.state)
    }

    pub fn wait_state(&self, sid: &str, state: SessionState) {
        self.wait(&format!("{sid} to be {state:?}"), Duration::from_secs(15), |r| r.session_state(sid) == Some(state));
    }

    pub fn wait_operation(&self, from: usize, phase: OperationPhase, kind: Option<TransitionKind>) -> (usize, ServerMessage) {
        self.greeter.as_ref().expect("greeter").wait_after(from, Duration::from_secs(15), &format!("operation {phase:?}"), |m| {
            matches!(m, ServerMessage::Operation { phase: p, kind: k, .. } if *p == phase && kind.is_none_or(|x| x == *k))
        })
    }

    pub fn frame_mark(&self) -> usize {
        self.frames.lock().frames.len()
    }

    /// Frames of output `Virtual-1` since `mark`.
    pub fn frames_since(&self, mark: usize) -> Vec<FrameRec> {
        self.frames.lock().frames.iter().skip(mark).filter(|f| f.output == "Virtual-1").cloned().collect()
    }

    pub fn all_frames_since(&self, mark: usize) -> Vec<FrameRec> {
        self.frames.lock().frames.iter().skip(mark).cloned().collect()
    }

    pub fn now_seen(&self) -> Seen {
        let log = self.frames.lock();
        let f = log.frames.iter().rev().find(|f| f.output == "Virtual-1").expect("a frame");
        f.seen()
    }

    pub fn wait_seen(&self, want: Seen) {
        self.wait(&format!("the screen to show {want:?}"), Duration::from_secs(10), |r| r.now_seen() == want);
    }

    /// A D-Bus style call as root through the same entry the D-Bus service uses.
    pub fn root_call(&self, method: Method) -> Result<Reply, BsmError> {
        self.call(Caller { uid: 0, pid: 1, bus_name: ":1.1".into(), cgroup: Some("/system.slice/test.service".into()) }, method)
    }

    pub fn call(&self, caller: Caller, method: Method) -> Result<Reply, BsmError> {
        let (rtx, rrx) = async_channel::bounded(1);
        self.tx.send(RtInput::Call(DbusCall { caller, method, reply: rtx })).expect("driver");
        rrx.recv_blocking().expect("a reply")
    }

    /// Who holds keyboard and pointer focus in the compositor right now.
    pub fn focus(&self) -> bsm_compositor::FocusSnapshot {
        let (tx, rx) = mpsc::channel();
        self.ctl.send(HarnessCmd::Focus(tx)).expect("compositor");
        rx.recv_timeout(Duration::from_secs(5)).expect("focus snapshot")
    }

    pub fn save_frame(&self, name: &str) {
        let log = self.frames.lock();
        if let Some(f) = log.last.get("Virtual-1") {
            let _ = std::fs::create_dir_all(&self.art);
            let _ = f.save_png(&self.art.join(format!("{name}.png")));
        }
    }

    /// Open sessions of the helper, in order.
    pub fn opened_for(&self, n: usize) -> Opened {
        self.wait(&format!("session {n} to be opened by the helper"), Duration::from_secs(15), |r| r.helper.opened().len() > n);
        self.helper.opened()[n].clone()
    }

    /// Full login of `uid` as the user would do it: pick, authenticate, watch the launch, reveal.
    /// Returns the session id once it is in the foreground.
    pub fn login(&mut self, uid: u32, color: Rgba, password: &str) -> String {
        let n = self.helper.opened().len();
        self.g().login(uid, AuthIntent::Login, password);
        let opened = self.opened_for(n);
        let bridge = TestBridge::start(&opened, uid, color, Duration::from_millis(80));
        self.bridges.push(bridge);
        self.wait_state(&opened.session, SessionState::Foreground);
        // Let the operation finish.
        self.wait("the operation to complete", Duration::from_secs(10), |r| r.rt.as_ref().unwrap().snapshot().seat == Some(bsm_proto::state::SeatState::UserForeground));
        opened.session
    }

    pub fn bridge(&mut self, sid: &str) -> &mut TestBridge {
        self.bridges.iter_mut().find(|b| b.session == sid).expect("bridge")
    }

    pub fn stop(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.greeter = None;
        self.bridges.clear();
        if let Some(rt) = self.rt.take() {
            rt.shutdown();
        }
        if let Some(s) = self.stepper.take() {
            let _ = s.join();
        }
        let _ = self.ctl.send(HarnessCmd::Stop);
        if let Some(c) = self.comp.take() {
            let _ = c.join();
        }
    }
}

pub fn artifacts() -> PathBuf {
    let base = std::env::var_os("BSM_TEST_ARTIFACTS").map(PathBuf::from).unwrap_or_else(|| std::env::temp_dir().join("bsm-artifacts"));
    base.join("bsm-flows")
}
