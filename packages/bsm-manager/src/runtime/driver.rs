//! The driver thread: owns the `Manager`, turns the outside world into `Input`s and
//! executes the `Effect`s the core returns, in order.
//!
//! Nothing here decides policy. Slow or blocking work (accounts, image conversion,
//! polkit, logind) runs on short-lived worker threads with a cap on how many may be in
//! flight; their results come back as inputs. The compositor thread never waits on this
//! thread: commands go through the `SceneSink` (a channel send).

use std::collections::{BTreeMap, VecDeque};
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, sync_channel};
use std::sync::Arc;

use parking_lot::{Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use bsm_proto::error::{BsmError, ErrorCode};
use bsm_proto::scene::SceneEvent;
use bsm_proto::state::SeatState;

use super::api::*;
use super::greeter_server::{ControlConfig, GreeterServer};
use super::helper_client::{HelperClient, HelperEndpoint};
use super::{persist, recovery, rng::OsRng};
use crate::core::*;

/// How long the driver keeps a D-Bus reply channel before answering `Timeout` itself.
pub const CALL_DEADLINE: Duration = Duration::from_secs(30);
const MAX_WORKERS: usize = 16;
const QUEUE: usize = 1024;
const CALL_QUEUE: usize = 64;
/// Calls taken per round, so that system inputs keep being read between them.
const CALLS_PER_ROUND: usize = 4;

pub struct RuntimeConfig {
    pub policy: Policy,
    pub state_dir: PathBuf,
    pub control: ControlConfig,
    pub helper: HelperEndpoint,
    pub accounts: Arc<dyn AccountSource>,
    pub appearance: Arc<dyn AppearanceBackend>,
    /// D-Bus calls, read after the system sources (F7).
    pub calls: Receiver<RtInput>,
    pub system: Arc<dyn SystemServices>,
    /// Receives every D-Bus signal the core wants emitted.
    pub signals: Box<dyn Fn(DbusSignal) + Send>,
    /// Where `/proc` is (tests point it at a fake tree).
    pub proc_root: PathBuf,
    /// Randomness; the operating system's when `None`.
    pub rng: Option<Box<dyn Rng>>,
}

/// Create the driver's input channel. The D-Bus service and the system services need the
/// sender before the runtime starts.
pub fn channel() -> (DriverTx, Receiver<RtInput>) {
    sync_channel(QUEUE)
}

/// The separate, smaller queue of D-Bus calls (F7). `sys` is the sender returned by
/// [`channel`]: it is only used to wake the driver.
pub fn call_channel(sys: &DriverTx) -> (CallTx, Receiver<RtInput>) {
    let (tx, rx) = sync_channel(CALL_QUEUE);
    (CallTx::new(tx, sys.clone()), rx)
}

pub struct RuntimeHandle {
    tx: DriverTx,
    join: Option<JoinHandle<()>>,
    ready: Arc<(Mutex<bool>, Condvar)>,
    state: Arc<Mutex<SharedState>>,
}

/// Read-only view for diagnostics and tests, updated by the driver after each input.
#[derive(Debug, Clone, Default)]
pub struct SharedState {
    pub seat: Option<SeatState>,
    pub sessions: Vec<SessionInfo>,
}

impl RuntimeHandle {
    pub fn sender(&self) -> DriverTx {
        self.tx.clone()
    }

    /// True once the compositor reported its outputs (the seat left `Booting`).
    pub fn wait_ready(&self, timeout: Duration) -> bool {
        let (lock, cv) = &*self.ready;
        let mut g = lock.lock();
        cv.wait_while_for(&mut g, |r| !*r, timeout);
        *g
    }

    pub fn snapshot(&self) -> SharedState {
        self.state.lock().clone()
    }

    /// Stop the driver. Sessions are not closed: bsm restarting must not end them.
    pub fn shutdown(mut self) {
        let _ = self.tx.send(RtInput::Shutdown);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }

    /// Wait for the driver to end by itself (the compositor went away).
    pub fn join(mut self) {
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }

    pub fn is_finished(&self) -> bool {
        self.join.as_ref().is_none_or(|j| j.is_finished())
    }
}

struct Driver {
    m: Manager,
    cfg_state_dir: PathBuf,
    scene: SceneSink,
    helper: HelperClient,
    greeter: GreeterServer,
    accounts: Arc<dyn AccountSource>,
    appearance: Arc<dyn AppearanceBackend>,
    system: Arc<dyn SystemServices>,
    signals: Box<dyn Fn(DbusSignal) + Send>,
    tx: DriverTx,
    timers: BTreeMap<TimerId, Instant>,
    calls: BTreeMap<CallId, (async_channel::Sender<Result<Reply, BsmError>>, Instant)>,
    next_call: CallId,
    queue: VecDeque<Input>,
    in_flight: Arc<AtomicUsize>,
    ready: Arc<(Mutex<bool>, Condvar)>,
    state: Arc<Mutex<SharedState>>,
    compositor_gone: bool,
}

/// Start everything: control socket, helper client, the scene event forwarder and the driver.
pub fn start(
    cfg: RuntimeConfig,
    tx: DriverTx,
    rx: Receiver<RtInput>,
    scene: SceneSink,
    scene_events: Receiver<SceneEvent>,
) -> io::Result<RuntimeHandle> {
    let rng: Box<dyn Rng> = match cfg.rng {
        Some(r) => r,
        None => Box::new(OsRng::new()?),
    };
    let greeter = GreeterServer::spawn(cfg.control.clone(), tx.clone())?;
    let helper = HelperClient::spawn(cfg.helper, tx.clone())?;
    let fwd_tx = tx.clone();
    thread::Builder::new().name("bsm-scene-events".into()).spawn(move || forward_scene_events(scene_events, fwd_tx))?;
    let ready = Arc::new((Mutex::new(false), Condvar::new()));
    let state = Arc::new(Mutex::new(SharedState::default()));
    let mut d = Driver {
        m: Manager::new(cfg.policy, rng),
        cfg_state_dir: cfg.state_dir.clone(),
        scene,
        helper,
        greeter,
        accounts: cfg.accounts,
        appearance: cfg.appearance,
        system: cfg.system,
        signals: cfg.signals,
        tx: tx.clone(),
        timers: BTreeMap::new(),
        calls: BTreeMap::new(),
        next_call: 1,
        queue: VecDeque::new(),
        in_flight: Arc::new(AtomicUsize::new(0)),
        ready: ready.clone(),
        state: state.clone(),
        compositor_gone: false,
    };
    let calls = cfg.calls;
    let persisted = persist::load(&cfg.state_dir);
    let recovered = recovery::probe(&cfg.proc_root, persisted.sessions.clone());
    let join = thread::Builder::new().name("bsm-manager".into()).spawn(move || {
        d.queue.push_back(Input::Start { persisted });
        d.queue.push_back(Input::Recovered(recovered));
        d.drain();
        d.run(rx, calls);
        let _ = std::fs::remove_file(d.greeter.path());
    })?;
    Ok(RuntimeHandle { tx, join: Some(join), ready, state })
}

/// Carries compositor events into the driver (F6). A hosted client can make the compositor emit a
/// `SessionReport` for every request it sends, so reports are the one kind that may be thrown away:
/// an exact repeat of the last report of that kind is dropped, and when the driver queue is full
/// reports are dropped and counted instead of blocking the forwarder (which would let the
/// compositor's unbounded channel grow). Every other event, and every `DesktopReady` (a
/// freshness anchor), is delivered, waiting for room if it must.
fn forward_scene_events(events: Receiver<SceneEvent>, tx: DriverTx) {
    use bsm_proto::scene::HostReport;
    use std::collections::HashMap;
    use std::sync::mpsc::TrySendError;
    let mut last: HashMap<(String, u64, u8, String), HostReport> = HashMap::new();
    let mut dropped: u64 = 0;
    for e in events {
        if let SceneEvent::SessionReport { session, generation, report } = &e {
            let (kind, output) = match report {
                HostReport::GnoblinReady => (0, String::new()),
                HostReport::OutputConfigured { output } => (1, output.clone()),
                HostReport::BinguxLaunched => (2, String::new()),
                HostReport::SurfacesMapped { output, .. } => (3, output.clone()),
                HostReport::DesktopReady => (4, String::new()),
            };
            if kind != 4 {
                let key = (session.clone(), *generation, kind, output);
                if last.get(&key) == Some(report) {
                    continue;
                }
                if last.len() > 1024 {
                    last.clear();
                }
                last.insert(key, report.clone());
            }
            match tx.try_send(RtInput::Core(Input::Scene(e))) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) => {
                    dropped += 1;
                    if dropped.is_power_of_two() {
                        tracing::warn!(dropped, "session reports dropped: the manager queue is full");
                    }
                }
                Err(TrySendError::Disconnected(_)) => return,
            }
            continue;
        }
        if tx.send(RtInput::Core(Input::Scene(e))).is_err() {
            return;
        }
    }
    // The compositor side hung up: bsm has nothing left to manage.
    let _ = tx.send(RtInput::Shutdown);
}

impl Driver {
    fn run(&mut self, rx: Receiver<RtInput>, calls: Receiver<RtInput>) {
        let mut backlog = false;
        loop {
            let now = Instant::now();
            let next_deadline = self
                .timers
                .values()
                .copied()
                .chain(self.calls.values().map(|(_, d)| *d))
                .min();
            let wait = match next_deadline {
                Some(t) => t.saturating_duration_since(now).min(Duration::from_secs(1)),
                None => Duration::from_secs(1),
            };
            // System sources first, all of them, then a few calls (F7).
            let mut first = true;
            let wait = if backlog { Duration::ZERO } else { wait };
            loop {
                let got = if first { rx.recv_timeout(wait) } else { rx.try_recv().map_err(|e| match e {
                    std::sync::mpsc::TryRecvError::Empty => RecvTimeoutError::Timeout,
                    std::sync::mpsc::TryRecvError::Disconnected => RecvTimeoutError::Disconnected,
                }) };
                first = false;
                match got {
                    Ok(RtInput::Shutdown) => return,
                    Ok(RtInput::Core(i)) => self.queue.push_back(i),
                    Ok(RtInput::Call(c)) => self.accept_call(c),
                    Ok(RtInput::Wake) => {}
                    Err(RecvTimeoutError::Timeout) => break,
                    Err(RecvTimeoutError::Disconnected) => return,
                }
                if self.queue.len() > 256 {
                    break;
                }
            }
            backlog = false;
            for n in 0..CALLS_PER_ROUND {
                match calls.try_recv() {
                    Ok(RtInput::Call(c)) => {
                        self.accept_call(c);
                        // Probably more: do not sleep before the next round.
                        backlog = n + 1 == CALLS_PER_ROUND;
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
            self.fire_due();
            self.drain();
            if self.compositor_gone {
                tracing::error!("the compositor is gone; the session manager stops");
                return;
            }
        }
    }

    fn accept_call(&mut self, c: DbusCall) {
        let id = self.next_call;
        self.next_call += 1;
        self.calls.insert(id, (c.reply, Instant::now() + CALL_DEADLINE));
        self.queue.push_back(Input::Dbus { call: id, caller: c.caller, method: c.method });
    }

    fn fire_due(&mut self) {
        let now = Instant::now();
        let due: Vec<TimerId> = self.timers.iter().filter(|(_, at)| **at <= now).map(|(id, _)| id.clone()).collect();
        for id in due {
            self.timers.remove(&id);
            self.queue.push_back(Input::Timer(id));
        }
        let late: Vec<CallId> = self.calls.iter().filter(|(_, (_, d))| *d <= now).map(|(id, _)| *id).collect();
        for id in late {
            if let Some((reply, _)) = self.calls.remove(&id) {
                let _ = reply.try_send(Err(BsmError::new(ErrorCode::Timeout, "the session manager did not answer in time")));
            }
        }
    }

    fn drain(&mut self) {
        let mut guard = 0usize;
        while let Some(input) = self.queue.pop_front() {
            guard += 1;
            if guard > 100_000 {
                tracing::error!("input storm; dropping the rest of the queue");
                self.queue.clear();
                break;
            }
            let fx = self.m.handle(Instant::now(), input);
            for e in fx {
                self.exec(e);
            }
            self.publish_state();
        }
    }

    fn publish_state(&mut self) {
        let seat = self.m.seat();
        if seat != SeatState::Booting {
            let (lock, cv) = &*self.ready;
            let mut g = lock.lock();
            if !*g {
                *g = true;
                cv.notify_all();
            }
        }
        let mut st = self.state.lock();
        st.seat = Some(seat);
        st.sessions = self.m.session_infos();
    }

    fn spawn_job(&self, name: &str, job: impl FnOnce() + Send + 'static, rejected: impl FnOnce()) {
        if self.in_flight.fetch_add(1, Ordering::SeqCst) >= MAX_WORKERS {
            self.in_flight.fetch_sub(1, Ordering::SeqCst);
            rejected();
            return;
        }
        let counter = self.in_flight.clone();
        let spawned = thread::Builder::new().name(format!("bsm-{name}")).spawn(move || {
            job();
            counter.fetch_sub(1, Ordering::SeqCst);
        });
        if spawned.is_err() {
            self.in_flight.fetch_sub(1, Ordering::SeqCst);
            rejected();
        }
    }

    fn feed_back(tx: &SyncSender<RtInput>, input: Input) {
        let _ = tx.send(RtInput::Core(input));
    }

    fn exec(&mut self, e: Effect) {
        match e {
            Effect::Scene(c) => {
                if !(self.scene)(c) {
                    self.compositor_gone = true;
                }
            }
            Effect::Helper(mut r) => {
                let sent = self.helper.send(&r);
                // F11: the answer is wiped as soon as it was handed to the helper.
                if let bsm_proto::helper::HelperRequest::AuthResponse { response, .. } = &mut r {
                    zeroize::Zeroize::zeroize(response);
                }
                if !sent {
                    // The helper client closes a failed connection and its reader reports the loss.
                    tracing::warn!("helper request could not be sent");
                }
            }
            Effect::Greeter(m) => {
                let _ = self.greeter.send(&m);
            }
            Effect::Dbus(s) => (self.signals)(s),
            Effect::DbusReply { call, result } => {
                if let Some((reply, _)) = self.calls.remove(&call) {
                    let _ = reply.try_send(result);
                }
            }
            Effect::ConnVerdict(v) => {
                if !self.greeter.verdict(v.conn, v.accept) {
                    self.queue.push_back(Input::GreeterDisconnected { conn: v.conn });
                }
            }
            Effect::DropGreeter => self.greeter.drop_active(),
            Effect::SetTimer { id, after } => {
                self.timers.insert(id, Instant::now() + after);
            }
            Effect::CancelTimer(id) => {
                self.timers.remove(&id);
            }
            Effect::Persist(p) => {
                if let Err(e) = persist::save(&self.cfg_state_dir, &p) {
                    tracing::error!("cannot persist state: {e}");
                }
            }
            Effect::LoadAccounts => {
                let (accounts, tx) = (self.accounts.clone(), self.tx.clone());
                self.spawn_job(
                    "accounts",
                    move || {
                        let mut tries = 0;
                        loop {
                            match accounts.list() {
                                Ok(list) => return Self::feed_back(&tx, Input::Accounts(list)),
                                Err(e) => {
                                    tries += 1;
                                    tracing::warn!("cannot list accounts: {e}");
                                    if tries >= 30 {
                                        return Self::feed_back(&tx, Input::Accounts(Vec::new()));
                                    }
                                    thread::sleep(Duration::from_secs(2));
                                }
                            }
                        }
                    },
                    || tracing::error!("no worker for the account listing"),
                );
            }
            Effect::PublishAppearance { call, uid, request } => {
                let (backend, tx) = (self.appearance.clone(), self.tx.clone());
                let tx2 = self.tx.clone();
                self.spawn_job(
                    "appearance",
                    move || {
                        let result = backend.publish(uid, request);
                        Self::feed_back(&tx, Input::AppearancePublished { call, uid, result });
                    },
                    move || {
                        Self::feed_back(
                            &tx2,
                            Input::AppearancePublished { call, uid, result: Err(BsmError::new(ErrorCode::Busy, "too many requests")) },
                        )
                    },
                );
            }
            Effect::Polkit { operation, action_id, caller } => {
                let (sys, tx, tx2) = (self.system.clone(), self.tx.clone(), self.tx.clone());
                self.spawn_job(
                    "polkit",
                    move || {
                        let authorised = match sys.polkit_check(&caller, &action_id) {
                            Ok(a) => a,
                            Err(e) => {
                                tracing::warn!("polkit check failed: {e}");
                                false
                            }
                        };
                        Self::feed_back(&tx, Input::PolkitResult { operation, authorised });
                    },
                    move || Self::feed_back(&tx2, Input::PolkitResult { operation, authorised: false }),
                );
            }
            Effect::QueryInhibitors { operation, action: _ } => {
                let (sys, tx, tx2) = (self.system.clone(), self.tx.clone(), self.tx.clone());
                self.spawn_job(
                    "inhibitors",
                    move || Self::feed_back(&tx, Input::InhibitorsResult { operation, result: sys.list_inhibitors() }),
                    move || {
                        Self::feed_back(
                            &tx2,
                            Input::InhibitorsResult { operation, result: Err(BsmError::new(ErrorCode::Busy, "too many requests")) },
                        )
                    },
                );
            }
            Effect::SystemPower { operation, action } => {
                let (sys, tx, tx2) = (self.system.clone(), self.tx.clone(), self.tx.clone());
                self.spawn_job(
                    "power",
                    move || Self::feed_back(&tx, Input::PowerResult { operation, result: sys.power(action) }),
                    move || {
                        Self::feed_back(
                            &tx2,
                            Input::PowerResult { operation, result: Err(BsmError::new(ErrorCode::Busy, "too many requests")) },
                        )
                    },
                );
            }
            Effect::SleepReady => self.system.sleep_ready(),
            Effect::Log(l) => {
                let sid = l.session.as_deref().unwrap_or("-");
                match l.level {
                    LogLevel::Info => tracing::info!(session = sid, "{}", l.message),
                    LogLevel::Warn => tracing::warn!(session = sid, "{}", l.message),
                    LogLevel::Error => tracing::error!(session = sid, "{}", l.message),
                }
            }
        }
    }
}

#[cfg(test)]
mod f6_tests {
    use super::*;
    use bsm_proto::scene::HostReport;
    use std::sync::mpsc::channel;

    fn report(sid: &str, r: HostReport) -> SceneEvent {
        SceneEvent::SessionReport { session: sid.into(), generation: 1, report: r }
    }

    #[test]
    fn a_flood_of_identical_reports_reaches_the_manager_once() {
        let (evt_tx, evt_rx) = channel();
        let (tx, rx) = sync_channel(1024);
        for _ in 0..100_000 {
            evt_tx.send(report("s1", HostReport::GnoblinReady)).unwrap();
        }
        evt_tx.send(report("s1", HostReport::OutputConfigured { output: "DP-1".into() })).unwrap();
        evt_tx.send(report("s1", HostReport::OutputConfigured { output: "DP-1".into() })).unwrap();
        evt_tx.send(report("s2", HostReport::GnoblinReady)).unwrap();
        drop(evt_tx);
        forward_scene_events(evt_rx, tx);
        let got: Vec<_> = rx.try_iter().collect();
        // Three distinct reports and the final shutdown.
        assert_eq!(got.len(), 4, "{}", got.len());
    }

    #[test]
    fn a_full_queue_drops_reports_but_never_other_events() {
        let (evt_tx, evt_rx) = channel();
        let (tx, rx) = sync_channel(4);
        for n in 0..50u32 {
            evt_tx.send(report("s1", HostReport::SurfacesMapped { output: "DP-1".into(), mapped: n, required: 99 })).unwrap();
        }
        drop(evt_tx);
        // Nobody reads the queue: the forwarder must still finish (it does not block on reports).
        let t = std::thread::spawn(move || forward_scene_events(evt_rx, tx));
        let deadline = Instant::now() + Duration::from_secs(3);
        while !t.is_finished() && Instant::now() < deadline {
            // The final Shutdown is a blocking send: drain one slot so it can go through.
            std::thread::sleep(Duration::from_millis(20));
            let _ = rx.try_recv();
        }
        assert!(t.is_finished());
    }

    #[test]
    fn desktop_ready_is_never_deduplicated() {
        let (evt_tx, evt_rx) = channel();
        let (tx, rx) = sync_channel(64);
        for _ in 0..3 {
            evt_tx.send(report("s1", HostReport::DesktopReady)).unwrap();
        }
        drop(evt_tx);
        forward_scene_events(evt_rx, tx);
        assert_eq!(rx.try_iter().count(), 4);
    }
}
