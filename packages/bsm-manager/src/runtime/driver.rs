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

pub struct RuntimeConfig {
    pub policy: Policy,
    pub state_dir: PathBuf,
    pub control: ControlConfig,
    pub helper: HelperEndpoint,
    pub accounts: Arc<dyn AccountSource>,
    pub appearance: Arc<dyn AppearanceBackend>,
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
    thread::Builder::new().name("bsm-scene-events".into()).spawn(move || {
        for e in scene_events {
            if fwd_tx.send(RtInput::Core(Input::Scene(e))).is_err() {
                return;
            }
        }
        // The compositor side hung up: bsm has nothing left to manage.
        let _ = fwd_tx.send(RtInput::Shutdown);
    })?;
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
    let persisted = persist::load(&cfg.state_dir);
    let recovered = recovery::probe(&cfg.proc_root, persisted.sessions.clone());
    let join = thread::Builder::new().name("bsm-manager".into()).spawn(move || {
        d.queue.push_back(Input::Start { persisted });
        d.queue.push_back(Input::Recovered(recovered));
        d.drain();
        d.run(rx);
        let _ = std::fs::remove_file(d.greeter.path());
    })?;
    Ok(RuntimeHandle { tx, join: Some(join), ready, state })
}

impl Driver {
    fn run(&mut self, rx: Receiver<RtInput>) {
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
            match rx.recv_timeout(wait) {
                Ok(RtInput::Shutdown) => return,
                Ok(RtInput::Core(i)) => self.queue.push_back(i),
                Ok(RtInput::Call(c)) => self.accept_call(c),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
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
            Effect::Helper(r) => {
                if !self.helper.send(&r) {
                    tracing::warn!("helper request could not be sent");
                    // The reader thread reports the loss; if there was never a connection, say so now.
                    self.queue.push_back(Input::HelperLost);
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
            Effect::ConnVerdict(v) => self.greeter.verdict(v.conn, v.accept),
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
