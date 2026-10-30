//! The real [`SystemServices`]: polkit, logind inhibitors, logind power calls and the
//! sleep delay inhibitor.
//!
//! What is implemented
//! -------------------
//! * `polkit_check`: `org.freedesktop.PolicyKit1.Authority.CheckAuthorization` with the
//!   subject `("unix-session", {"session-id": <logind session>})` when the manager knows the
//!   caller's hosted session (it survives a one-shot caller that has disconnected), else
//!   `("system-bus-name", {"name": <caller.bus_name>})`, the action id, empty
//!   details, flags 0 (no user interaction) and an empty cancellation id. Only
//!   `is_authorized` counts; a challenge (`is_challenge`) is **not** authorisation.
//! * `list_inhibitors`: `org.freedesktop.login1.Manager.ListInhibitors` (`a(ssssuu)`:
//!   what, who, why, mode, uid, pid) into [`Inhibitor`].
//! * `power`: `Manager.Suspend`, `Reboot` or `PowerOff` with `interactive = false`.
//! * Sleep: at start a `delay` inhibitor for `sleep` is taken and its descriptor held.
//!   `PrepareForSleep(true)` sends `RtInput::Core(Input::PrepareForSleep { start: true })`;
//!   the descriptor stays held until the core calls [`SystemServices::sleep_ready`]
//!   (which closes it), or until the hard cap ([`SystemConfig::sleep_cap`], 5 s) passes,
//!   whichever is first. `PrepareForSleep(false)` sends `start: false` and takes a new
//!   inhibitor (retried every 2 s while that fails). If the driver queue stays full for
//!   2 s the inhibitor is released instead of hanging the system.
//! * Every call is bounded by [`SystemConfig::call_timeout`] (10 s). Failures map to
//!   `BsmError`: access denied, interactive authorisation required and polkit
//!   `NotAuthorized` become `Unauthorised`; a missed deadline is `Timeout`; everything
//!   else is `LogindFailure`.
//!
//! These calls block the calling thread (up to the bound); the runtime runs them on a
//! worker, not on the driver thread, and feeds the result back as an `Input`.
//!
//! Unverified
//! ----------
//! The real logind and polkit are not available on this host's test setup. What is tested
//! is the message building, result parsing, error mapping and the sleep state machine
//! against fake `login1` and `PolicyKit1` services on a private bus. Signatures follow the
//! logind and polkit D-Bus documentation; a first run on the target image must confirm
//! them (`busctl introspect org.freedesktop.login1 /org/freedesktop/login1`, polkit's
//! `CheckAuthorization`) and that a seatless hosted session's caller is judged as
//! intended by `bsm.rules`.

use std::collections::HashMap;
use std::os::fd::OwnedFd;
use std::sync::mpsc::{SyncSender, TrySendError};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use async_io::{Timer, block_on};
use bsm_proto::error::{BsmError, ErrorCode};
use bsm_proto::state::PowerAction;
use futures_lite::StreamExt;
use serde::Serialize;
use serde::de::DeserializeOwned;
use zbus::zvariant::{self, DynamicType, Value};
use zbus::{Connection, Message, MessageStream};

use super::api::{RtInput, SystemServices};
use super::dbus::{BusKind, builder, with_timeout};
use crate::core::{Caller, Inhibitor, Input};

const LOGIND: &str = "org.freedesktop.login1";
const LOGIND_PATH: &str = "/org/freedesktop/login1";
const LOGIND_MANAGER: &str = "org.freedesktop.login1.Manager";
const POLKIT: &str = "org.freedesktop.PolicyKit1";
const POLKIT_PATH: &str = "/org/freedesktop/PolicyKit1/Authority";
const POLKIT_AUTHORITY: &str = "org.freedesktop.PolicyKit1.Authority";

const INHIBIT_WHO: &str = "bsm";
const INHIBIT_WHY: &str = "Protect sessions before sleep";
const NOTIFY_PATIENCE: Duration = Duration::from_secs(2);
const RETAKE_EVERY: Duration = Duration::from_secs(2);

/// Bounds of the real services. Shortened in tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemConfig {
    /// Bound of every D-Bus call.
    pub call_timeout: Duration,
    /// How long the sleep inhibitor is held after `PrepareForSleep(true)` without
    /// `sleep_ready`.
    pub sleep_cap: Duration,
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self { call_timeout: Duration::from_secs(10), sleep_cap: Duration::from_secs(5) }
    }
}

fn err(code: ErrorCode, message: impl Into<String>) -> BsmError {
    BsmError::new(code, message)
}

/// Map a failed call to the error the core understands.
pub(crate) fn map_error(what: &str, e: &zbus::Error) -> BsmError {
    match e {
        zbus::Error::MethodError(name, text, _) => {
            let detail = text.as_deref().unwrap_or("");
            let code = match name.as_str() {
                "org.freedesktop.DBus.Error.AccessDenied"
                | "org.freedesktop.DBus.Error.AuthFailed"
                | "org.freedesktop.DBus.Error.InteractiveAuthorizationRequired"
                | "org.freedesktop.PolicyKit1.Error.NotAuthorized" => ErrorCode::Unauthorised,
                "org.freedesktop.DBus.Error.NoReply"
                | "org.freedesktop.DBus.Error.Timeout"
                | "org.freedesktop.DBus.Error.TimedOut" => ErrorCode::Timeout,
                _ => ErrorCode::LogindFailure,
            };
            err(code, format!("{what}: {name}: {detail}"))
        }
        zbus::Error::InputOutput(io) if io.kind() == std::io::ErrorKind::TimedOut => {
            err(ErrorCode::Timeout, format!("{what}: timed out"))
        }
        other => err(ErrorCode::LogindFailure, format!("{what}: {other}")),
    }
}

#[allow(clippy::too_many_arguments)]
async fn call_on<B, R>(
    conn: &Connection,
    limit: Duration,
    what: &str,
    dest: &str,
    path: &str,
    iface: &str,
    member: &str,
    body: &B,
) -> Result<R, BsmError>
where
    B: Serialize + DynamicType,
    R: DeserializeOwned + zvariant::Type,
{
    let call = conn.call_method(Some(dest), path, Some(iface), member, body);
    let reply = with_timeout(limit, call)
        .await
        .ok_or_else(|| err(ErrorCode::Timeout, format!("{what}: no answer within {} ms", limit.as_millis())))?
        .map_err(|e| map_error(what, &e))?;
    reply
        .body()
        .deserialize::<R>()
        .map_err(|e| err(ErrorCode::LogindFailure, format!("{what}: unexpected reply: {e}")))
}

/// Take the sleep delay inhibitor and return its descriptor.
async fn take_inhibitor(conn: &Connection, limit: Duration) -> Result<OwnedFd, BsmError> {
    let fd: zvariant::OwnedFd = call_on(
        conn,
        limit,
        "Inhibit",
        LOGIND,
        LOGIND_PATH,
        LOGIND_MANAGER,
        "Inhibit",
        &("sleep", INHIBIT_WHO, INHIBIT_WHY, "delay"),
    )
    .await?;
    Ok(fd.into())
}

// ---------------------------------------------------------------------------------------
// The sleep state machine, free of I/O.

/// What a `PrepareForSleep` signal means for the service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SleepAction {
    /// First `true`: tell the core, arm the hard cap.
    Prepare,
    /// `false`: tell the core, disarm the cap, take a new inhibitor.
    Resume,
    /// A repeated `true`: nothing to do.
    Ignore,
}

#[derive(Default)]
pub(crate) struct SleepState {
    held: Option<OwnedFd>,
    preparing: bool,
}

impl SleepState {
    pub(crate) fn signal(&mut self, start: bool) -> SleepAction {
        match (start, self.preparing) {
            (true, false) => {
                self.preparing = true;
                SleepAction::Prepare
            }
            (true, true) => SleepAction::Ignore,
            (false, _) => {
                self.preparing = false;
                SleepAction::Resume
            }
        }
    }

    pub(crate) fn hold(&mut self, fd: OwnedFd) {
        self.held = Some(fd);
    }

    /// Close the descriptor. True when one was held.
    pub(crate) fn release(&mut self) -> bool {
        self.held.take().is_some()
    }

    pub(crate) fn holds(&self) -> bool {
        self.held.is_some()
    }

    pub(crate) fn preparing(&self) -> bool {
        self.preparing
    }
}

struct Inner {
    conn: Connection,
    sleep: Mutex<SleepState>,
    config: SystemConfig,
}

impl Inner {
    fn sleep(&self) -> MutexGuard<'_, SleepState> {
        self.sleep.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// polkit, logind and the sleep inhibitor over D-Bus.
pub struct LogindSystem {
    inner: Arc<Inner>,
    stop: async_channel::Sender<()>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl LogindSystem {
    /// Connect, take the sleep inhibitor and start watching `PrepareForSleep`.
    /// `tx` is the driver's channel.
    pub fn start(bus: BusKind, tx: SyncSender<RtInput>) -> Result<Arc<LogindSystem>, BsmError> {
        Self::start_with(bus, tx, SystemConfig::default())
    }

    pub fn start_with(bus: BusKind, tx: SyncSender<RtInput>, config: SystemConfig) -> Result<Arc<LogindSystem>, BsmError> {
        let (conn, stream, fd) = block_on(async {
            let conn = builder(&bus)
                .map_err(|e| map_error("connect", &e))?
                .build()
                .await
                .map_err(|e| map_error("connect", &e))?;
            let rule = zbus::MatchRule::builder()
                .msg_type(zbus::message::Type::Signal)
                .sender(LOGIND)
                .and_then(|b| b.interface(LOGIND_MANAGER))
                .and_then(|b| b.member("PrepareForSleep"))
                .and_then(|b| b.path(LOGIND_PATH))
                .map_err(|e| map_error("match rule", &e))?
                .build();
            // Subscribe before taking the inhibitor so that no signal is missed.
            let stream = MessageStream::for_match_rule(rule, &conn, Some(16))
                .await
                .map_err(|e| map_error("subscribe PrepareForSleep", &e))?;
            let fd = take_inhibitor(&conn, config.call_timeout).await?;
            Ok::<_, BsmError>((conn, stream, fd))
        })?;
        let inner = Arc::new(Inner { conn, sleep: Mutex::new(SleepState::default()), config });
        inner.sleep().hold(fd);
        let (stop, stop_rx) = async_channel::bounded(1);
        let worker = inner.clone();
        let thread = std::thread::Builder::new()
            .name("bsm-sleep".into())
            .spawn(move || block_on(sleep_loop(worker, stream, tx, stop_rx)))
            .map_err(|e| err(ErrorCode::Internal, format!("cannot start the sleep watcher: {e}")))?;
        Ok(Arc::new(LogindSystem { inner, stop, thread: Mutex::new(Some(thread)) }))
    }

    /// True while the sleep delay inhibitor is held.
    pub fn holds_sleep_inhibitor(&self) -> bool {
        self.inner.sleep().holds()
    }

    fn call<B, R>(&self, what: &str, dest: &str, path: &str, iface: &str, member: &str, body: &B) -> Result<R, BsmError>
    where
        B: Serialize + DynamicType,
        R: DeserializeOwned + zvariant::Type,
    {
        block_on(call_on(&self.inner.conn, self.inner.config.call_timeout, what, dest, path, iface, member, body))
    }
}

impl Drop for LogindSystem {
    fn drop(&mut self) {
        self.stop.close();
        let handle = self.thread.lock().unwrap_or_else(PoisonError::into_inner).take();
        if let Some(t) = handle {
            let _ = t.join();
        }
    }
}

impl SystemServices for LogindSystem {
    fn polkit_check(&self, caller: &Caller, logind_session: Option<&str>, action_id: &str) -> Result<bool, BsmError> {
        // A one-shot caller (`bsmctl reboot`) is gone by the time the check runs, and a
        // `system-bus-name` subject of a vanished name fails with NameHasNoOwner. The caller's
        // logind session, taken from the manager's own registry, outlives it.
        let subject: (&str, HashMap<&str, Value<'_>>) = match logind_session {
            Some(id) => ("unix-session", HashMap::from([("session-id", Value::from(id))])),
            None => ("system-bus-name", HashMap::from([("name", Value::from(caller.bus_name.as_str()))])),
        };
        let details: HashMap<&str, &str> = HashMap::new();
        let (result,): ((bool, bool, HashMap<String, String>),) = self.call(
            "CheckAuthorization",
            POLKIT,
            POLKIT_PATH,
            POLKIT_AUTHORITY,
            "CheckAuthorization",
            &(subject, action_id, details, 0u32, ""),
        )?;
        let (is_authorized, _is_challenge, _details) = result;
        // A challenge means "would be allowed after authentication"; nobody can
        // authenticate here (flags 0), so it is a refusal.
        Ok(is_authorized)
    }

    fn list_inhibitors(&self) -> Result<Vec<Inhibitor>, BsmError> {
        let rows: Vec<(String, String, String, String, u32, u32)> =
            self.call("ListInhibitors", LOGIND, LOGIND_PATH, LOGIND_MANAGER, "ListInhibitors", &())?;
        Ok(rows
            .into_iter()
            .map(|(what, who, why, mode, uid, _pid)| Inhibitor { what, who, why, mode, uid })
            .collect())
    }

    fn power(&self, action: PowerAction) -> Result<(), BsmError> {
        let member = match action {
            PowerAction::Suspend => "Suspend",
            PowerAction::Reboot => "Reboot",
            PowerAction::PowerOff => "PowerOff",
        };
        self.call(member, LOGIND, LOGIND_PATH, LOGIND_MANAGER, member, &(false,))
    }

    fn sleep_ready(&self) {
        if self.inner.sleep().release() {
            tracing::debug!("sleep inhibitor released: sessions are protected");
        }
    }
}

/// Tell the core. Waits for room in the driver queue for a bounded time.
async fn notify(tx: &SyncSender<RtInput>, start: bool) -> bool {
    let deadline = Instant::now() + NOTIFY_PATIENCE;
    let mut item = RtInput::Core(Input::PrepareForSleep { start });
    loop {
        match tx.try_send(item) {
            Ok(()) => return true,
            Err(TrySendError::Full(back)) => {
                if Instant::now() >= deadline {
                    return false;
                }
                item = back;
                Timer::after(Duration::from_millis(10)).await;
            }
            Err(TrySendError::Disconnected(_)) => return false,
        }
    }
}

enum Event {
    Message(Option<zbus::Result<Message>>),
    Tick,
    Stop,
}

async fn sleep_loop(
    inner: Arc<Inner>,
    mut stream: MessageStream,
    tx: SyncSender<RtInput>,
    stop: async_channel::Receiver<()>,
) {
    let mut cap: Option<Instant> = None;
    let mut retake: Option<Instant> = None;
    loop {
        let wake = [cap, retake].into_iter().flatten().min();
        let next = async { Event::Message(stream.next().await) };
        let tick = async {
            match wake {
                Some(t) => {
                    Timer::at(t).await;
                    Event::Tick
                }
                None => futures_lite::future::pending().await,
            }
        };
        let stopped = async {
            let _ = stop.recv().await;
            Event::Stop
        };
        let event = futures_lite::future::or(futures_lite::future::or(next, tick), stopped).await;
        match event {
            Event::Stop | Event::Message(None) => return,
            Event::Message(Some(Err(e))) => tracing::warn!("PrepareForSleep stream error: {e}"),
            Event::Message(Some(Ok(msg))) => {
                let Ok((start,)) = msg.body().deserialize::<(bool,)>() else {
                    tracing::warn!("malformed PrepareForSleep signal ignored");
                    continue;
                };
                let action = inner.sleep().signal(start);
                match action {
                    SleepAction::Ignore => {}
                    SleepAction::Prepare => {
                        cap = Some(Instant::now() + inner.config.sleep_cap);
                        if !inner.sleep().holds() {
                            // The system is already preparing; a late delay lock may still
                            // be honoured, so try, but do not wait for it.
                            retake_inhibitor(&inner, &mut retake).await;
                        }
                        if !notify(&tx, true).await {
                            tracing::error!("manager did not take PrepareForSleep; releasing the inhibitor");
                            inner.sleep().release();
                            cap = None;
                        }
                    }
                    SleepAction::Resume => {
                        cap = None;
                        // Let go of a still-held descriptor from the sleep that just ended.
                        inner.sleep().release();
                        if !notify(&tx, false).await {
                            tracing::error!("manager did not take the resume notice");
                        }
                        retake_inhibitor(&inner, &mut retake).await;
                    }
                }
            }
            Event::Tick => {
                let now = Instant::now();
                if cap.is_some_and(|t| t <= now) {
                    cap = None;
                    if inner.sleep().release() {
                        tracing::warn!("sleep inhibitor released at the hard cap, sessions may not be protected");
                    }
                }
                if retake.is_some_and(|t| t <= now) {
                    retake = None;
                    if !inner.sleep().preparing() {
                        retake_inhibitor(&inner, &mut retake).await;
                    }
                }
            }
        }
    }
}

/// Take a sleep inhibitor if none is held; on failure arm a retry.
async fn retake_inhibitor(inner: &Inner, retake: &mut Option<Instant>) {
    if inner.sleep().holds() {
        *retake = None;
        return;
    }
    match take_inhibitor(&inner.conn, inner.config.call_timeout).await {
        Ok(fd) => {
            inner.sleep().hold(fd);
            *retake = None;
        }
        Err(e) => {
            tracing::warn!("cannot take the sleep inhibitor: {e}");
            *retake = Some(Instant::now() + RETAKE_EVERY);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Read;
    use std::os::unix::net::UnixStream;
    use std::sync::mpsc::{Receiver, sync_channel};

    use zbus::object_server::SignalEmitter;

    use super::*;
    use crate::runtime::dbus::testutil::PrivateBus;

    #[derive(Default)]
    struct FakeState {
        /// Our ends of the inhibitor sockets; EOF on them means the client closed its fd.
        inhibitors: Vec<(UnixStream, [String; 4])>,
        calls: Vec<String>,
        power_error: Option<&'static str>,
        polkit_seen: Vec<(String, String, u32, String, usize)>,
    }

    type Shared = Arc<Mutex<FakeState>>;

    struct FakeLogin(Shared);

    #[zbus::interface(name = "org.freedesktop.login1.Manager")]
    impl FakeLogin {
        async fn inhibit(&self, what: &str, who: &str, why: &str, mode: &str) -> zbus::fdo::Result<zvariant::OwnedFd> {
            let (ours, theirs) = UnixStream::pair().map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
            ours.set_nonblocking(true).unwrap();
            let mut s = self.0.lock().unwrap();
            s.inhibitors.push((ours, [what.into(), who.into(), why.into(), mode.into()]));
            Ok(zvariant::OwnedFd::from(OwnedFd::from(theirs)))
        }

        async fn list_inhibitors(&self) -> Vec<(String, String, String, String, u32, u32)> {
            vec![
                ("sleep".into(), "Ada's editor".into(), "saving".into(), "delay".into(), 1000, 4242),
                ("shutdown:sleep".into(), "backup".into(), "running".into(), "block".into(), 0, 1),
            ]
        }

        async fn suspend(&self, interactive: bool) -> zbus::fdo::Result<()> {
            self.power("Suspend", interactive)
        }

        async fn reboot(&self, interactive: bool) -> zbus::fdo::Result<()> {
            self.power("Reboot", interactive)
        }

        async fn power_off(&self, interactive: bool) -> zbus::fdo::Result<()> {
            self.power("PowerOff", interactive)
        }

        #[zbus(signal)]
        async fn prepare_for_sleep(emitter: &SignalEmitter<'_>, start: bool) -> zbus::Result<()>;
    }

    impl FakeLogin {
        fn power(&self, name: &str, interactive: bool) -> zbus::fdo::Result<()> {
            let mut s = self.0.lock().unwrap();
            s.calls.push(format!("{name}({interactive})"));
            match s.power_error {
                Some("denied") => Err(zbus::fdo::Error::AccessDenied("polkit says no".into())),
                Some("interactive") => Err(zbus::fdo::Error::InteractiveAuthorizationRequired("ask".into())),
                Some(_) => Err(zbus::fdo::Error::Failed("boom".into())),
                None => Ok(()),
            }
        }
    }

    struct FakePolkit(Shared);

    #[zbus::interface(name = "org.freedesktop.PolicyKit1.Authority")]
    impl FakePolkit {
        async fn check_authorization(
            &self,
            subject: (String, HashMap<String, zvariant::OwnedValue>),
            action_id: &str,
            details: HashMap<String, String>,
            flags: u32,
            cancellation_id: &str,
        ) -> zbus::fdo::Result<((bool, bool, HashMap<String, String>),)> {
            let name = subject
                .1
                .get("name")
                .or_else(|| subject.1.get("session-id"))
                .and_then(|v| v.downcast_ref::<String>().ok())
                .unwrap_or_default();
            self.0.lock().unwrap().polkit_seen.push((
                subject.0.clone(),
                name,
                flags,
                cancellation_id.to_owned(),
                details.len(),
            ));
            let extra = HashMap::new();
            match action_id {
                "org.freedesktop.login1.reboot" => Ok(((true, false, extra),)),
                "org.freedesktop.login1.suspend" => Ok(((false, true, extra),)),
                "org.freedesktop.login1.power-off" => Ok(((false, false, extra),)),
                "hang" => {
                    Timer::after(Duration::from_secs(30)).await;
                    Ok(((true, false, extra),))
                }
                _ => Err(zbus::fdo::Error::Failed("polkit is unhappy".into())),
            }
        }
    }

    struct Rig {
        _bus: PrivateBus,
        fake: Shared,
        fake_conn: Connection,
        rx: Receiver<RtInput>,
        tx: SyncSender<RtInput>,
        bus_kind: BusKind,
    }

    fn rig() -> Rig {
        let bus = PrivateBus::start();
        let fake: Shared = Arc::default();
        let fake_conn = block_on(async {
            builder(&bus.kind())
                .unwrap()
                .serve_at(LOGIND_PATH, FakeLogin(fake.clone()))
                .unwrap()
                .serve_at(POLKIT_PATH, FakePolkit(fake.clone()))
                .unwrap()
                .name(LOGIND)
                .unwrap()
                .name(POLKIT)
                .unwrap()
                .build()
                .await
                .unwrap()
        });
        let (tx, rx) = sync_channel(8);
        let bus_kind = bus.kind();
        Rig { _bus: bus, fake, fake_conn, rx, tx, bus_kind }
    }

    impl Rig {
        fn start(&self, config: SystemConfig) -> Arc<LogindSystem> {
            LogindSystem::start_with(self.bus_kind.clone(), self.tx.clone(), config).unwrap()
        }

        fn prepare_for_sleep(&self, start: bool) {
            block_on(async {
                let emitter = SignalEmitter::new(&self.fake_conn, LOGIND_PATH).unwrap();
                FakeLogin::prepare_for_sleep(&emitter, start).await.unwrap();
            });
        }

        fn inhibitor_open(&self, index: usize) -> bool {
            let s = self.fake.lock().unwrap();
            let mut buf = [0u8; 1];
            // A read of 0 bytes is end-of-file: the client closed its descriptor.
            match (&s.inhibitors[index].0).read(&mut buf) {
                Ok(0) => false,
                Ok(_) => true,
                Err(e) => e.kind() == std::io::ErrorKind::WouldBlock,
            }
        }

        fn wait_for(&self, what: &str, limit: Duration, f: impl Fn(&Rig) -> bool) -> Duration {
            let start = Instant::now();
            while !f(self) {
                assert!(start.elapsed() < limit, "timed out waiting for {what}");
                std::thread::sleep(Duration::from_millis(10));
            }
            start.elapsed()
        }

        fn recv_sleep(&self) -> bool {
            match self.rx.recv_timeout(Duration::from_secs(5)).expect("a PrepareForSleep input") {
                RtInput::Core(Input::PrepareForSleep { start }) => start,
                _ => panic!("unexpected input"),
            }
        }
    }

    fn caller() -> Caller {
        Caller { uid: 1000, pid: 4242, bus_name: ":1.99".into(), cgroup: None }
    }

    fn quick() -> SystemConfig {
        SystemConfig { call_timeout: Duration::from_millis(500), sleep_cap: Duration::from_millis(400) }
    }

    #[test]
    fn state_machine_orders_prepare_and_resume() {
        let mut s = SleepState::default();
        assert_eq!(s.signal(true), SleepAction::Prepare);
        assert!(s.preparing());
        assert_eq!(s.signal(true), SleepAction::Ignore);
        assert_eq!(s.signal(false), SleepAction::Resume);
        assert!(!s.preparing());
        assert_eq!(s.signal(false), SleepAction::Resume, "a resume always lets the inhibitor be retaken");
        assert!(!s.release());
        let (a, _b) = UnixStream::pair().unwrap();
        s.hold(a.into());
        assert!(s.holds());
        assert!(s.release());
        assert!(!s.release());
    }

    #[test]
    fn error_mapping() {
        let method = |name: &'static str| {
            // A method error needs a message; build one from a call to ourselves.
            let msg = zbus::message::Message::method_call("/a", "B").unwrap().build(&()).unwrap();
            zbus::Error::MethodError(zbus::names::OwnedErrorName::try_from(name).unwrap(), Some("t".into()), msg)
        };
        for (name, code) in [
            ("org.freedesktop.DBus.Error.AccessDenied", ErrorCode::Unauthorised),
            ("org.freedesktop.DBus.Error.InteractiveAuthorizationRequired", ErrorCode::Unauthorised),
            ("org.freedesktop.PolicyKit1.Error.NotAuthorized", ErrorCode::Unauthorised),
            ("org.freedesktop.DBus.Error.NoReply", ErrorCode::Timeout),
            ("org.freedesktop.DBus.Error.ServiceUnknown", ErrorCode::LogindFailure),
            ("org.freedesktop.DBus.Error.Failed", ErrorCode::LogindFailure),
        ] {
            assert_eq!(map_error("x", &method(name)).code, code, "{name}");
        }
        let io = zbus::Error::InputOutput(Arc::new(std::io::Error::from(std::io::ErrorKind::TimedOut)));
        assert_eq!(map_error("x", &io).code, ErrorCode::Timeout);
        assert_eq!(map_error("x", &zbus::Error::NameTaken).code, ErrorCode::LogindFailure);
    }

    #[test]
    fn polkit_subject_and_results() {
        let r = rig();
        let sys = r.start(quick());
        assert!(sys.polkit_check(&caller(), None, "org.freedesktop.login1.reboot").unwrap());
        // A challenge is not an authorisation.
        assert!(!sys.polkit_check(&caller(), None, "org.freedesktop.login1.suspend").unwrap());
        assert!(!sys.polkit_check(&caller(), None, "org.freedesktop.login1.power-off").unwrap());
        let e = sys.polkit_check(&caller(), None, "something.else").unwrap_err();
        assert_eq!(e.code, ErrorCode::LogindFailure);
        assert!(e.message.contains("polkit is unhappy"), "{e}");
        let seen = r.fake.lock().unwrap().polkit_seen.clone();
        assert_eq!(seen.len(), 4);
        for s in seen {
            assert_eq!(s, ("system-bus-name".into(), ":1.99".into(), 0, "".into(), 0));
        }
    }

    #[test]
    fn a_known_hosted_session_is_checked_as_a_unix_session_that_outlives_the_caller() {
        let r = rig();
        let sys = r.start(quick());
        assert!(sys.polkit_check(&caller(), Some("c7"), "org.freedesktop.login1.reboot").unwrap());
        let seen = r.fake.lock().unwrap().polkit_seen.clone();
        assert_eq!(seen, vec![("unix-session".into(), "c7".into(), 0, "".into(), 0)]);
    }

    #[test]
    fn polkit_signature_is_the_documented_one() {
        let r = rig();
        let xml: String = block_on(async {
            r.fake_conn
                .call_method(
                    Some(POLKIT),
                    POLKIT_PATH,
                    Some("org.freedesktop.DBus.Introspectable"),
                    "Introspect",
                    &(),
                )
                .await
                .unwrap()
                .body()
                .deserialize()
                .unwrap()
        });
        // The fake must be faithful or its tests prove nothing: one struct in, one struct out.
        assert!(xml.contains(r#"name="subject" type="(sa{sv})" direction="in""#), "{xml}");
        assert!(xml.contains(r#"type="a{ss}" direction="in""#), "{xml}");
        assert!(xml.contains(r#"type="(bba{ss})" direction="out""#), "{xml}");
    }

    #[test]
    fn polkit_timeout() {
        let r = rig();
        let sys = r.start(quick());
        let start = Instant::now();
        let e = sys.polkit_check(&caller(), None, "hang").unwrap_err();
        assert_eq!(e.code, ErrorCode::Timeout);
        assert!(start.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn inhibitors_parse() {
        let r = rig();
        let sys = r.start(quick());
        let list = sys.list_inhibitors().unwrap();
        assert_eq!(
            list,
            vec![
                Inhibitor {
                    what: "sleep".into(),
                    who: "Ada's editor".into(),
                    why: "saving".into(),
                    mode: "delay".into(),
                    uid: 1000
                },
                Inhibitor {
                    what: "shutdown:sleep".into(),
                    who: "backup".into(),
                    why: "running".into(),
                    mode: "block".into(),
                    uid: 0
                },
            ]
        );
    }

    #[test]
    fn power_calls_are_non_interactive_and_errors_map() {
        let r = rig();
        let sys = r.start(quick());
        sys.power(PowerAction::Suspend).unwrap();
        sys.power(PowerAction::Reboot).unwrap();
        sys.power(PowerAction::PowerOff).unwrap();
        assert_eq!(r.fake.lock().unwrap().calls, ["Suspend(false)", "Reboot(false)", "PowerOff(false)"]);
        r.fake.lock().unwrap().power_error = Some("denied");
        assert_eq!(sys.power(PowerAction::Reboot).unwrap_err().code, ErrorCode::Unauthorised);
        r.fake.lock().unwrap().power_error = Some("interactive");
        assert_eq!(sys.power(PowerAction::Reboot).unwrap_err().code, ErrorCode::Unauthorised);
        r.fake.lock().unwrap().power_error = Some("other");
        assert_eq!(sys.power(PowerAction::Reboot).unwrap_err().code, ErrorCode::LogindFailure);
    }

    #[test]
    fn missing_logind_fails_start() {
        let bus = PrivateBus::start();
        let (tx, _rx) = sync_channel(1);
        let e = LogindSystem::start_with(bus.kind(), tx, quick()).err().expect("no logind on this bus");
        assert_eq!(e.code, ErrorCode::LogindFailure);
    }

    #[test]
    fn sleep_inhibitor_is_taken_held_and_released_by_sleep_ready() {
        let r = rig();
        let sys = r.start(SystemConfig { call_timeout: Duration::from_secs(2), sleep_cap: Duration::from_secs(30) });
        assert!(sys.holds_sleep_inhibitor());
        {
            let s = r.fake.lock().unwrap();
            assert_eq!(s.inhibitors.len(), 1);
            assert_eq!(s.inhibitors[0].1, ["sleep", "bsm", "Protect sessions before sleep", "delay"]);
        }
        assert!(r.inhibitor_open(0));

        r.prepare_for_sleep(true);
        assert!(r.recv_sleep());
        // Held until the core answers.
        std::thread::sleep(Duration::from_millis(200));
        assert!(r.inhibitor_open(0));
        sys.sleep_ready();
        assert!(!r.inhibitor_open(0), "sleep_ready closes the descriptor");
        assert!(!sys.holds_sleep_inhibitor());
        sys.sleep_ready(); // idempotent

        // Resume: the core is told and a fresh inhibitor is taken.
        r.prepare_for_sleep(false);
        assert!(!r.recv_sleep());
        r.wait_for("a new inhibitor", Duration::from_secs(5), |r| r.fake.lock().unwrap().inhibitors.len() == 2);
        r.wait_for("the new inhibitor held", Duration::from_secs(5), |_| sys.holds_sleep_inhibitor());
        assert!(r.inhibitor_open(1));
    }

    #[test]
    fn sleep_inhibitor_is_released_at_the_hard_cap() {
        let r = rig();
        let sys = r.start(SystemConfig { call_timeout: Duration::from_secs(2), sleep_cap: Duration::from_millis(400) });
        r.prepare_for_sleep(true);
        let t0 = Instant::now();
        assert!(r.recv_sleep());
        let took = r.wait_for("the cap", Duration::from_secs(5), |r| !r.inhibitor_open(0));
        assert!(t0.elapsed() >= Duration::from_millis(350), "released too early: {:?}", t0.elapsed());
        assert!(took < Duration::from_secs(3));
        assert!(!sys.holds_sleep_inhibitor());
    }

    #[test]
    fn a_stuck_driver_does_not_hold_the_system_up() {
        let r = rig();
        let (tx, rx) = sync_channel(1);
        tx.send(RtInput::Shutdown).unwrap(); // the queue is full and nobody reads
        let sys =
            LogindSystem::start_with(r.bus_kind.clone(), tx, SystemConfig { call_timeout: Duration::from_secs(2), sleep_cap: Duration::from_secs(30) })
                .unwrap();
        r.prepare_for_sleep(true);
        let waited = r.wait_for("release without a driver", Duration::from_secs(6), |r| !r.inhibitor_open(0));
        assert!(waited >= Duration::from_millis(1500), "{waited:?}");
        assert!(!sys.holds_sleep_inhibitor());
        drop(rx);
    }

    #[test]
    fn dropping_the_service_stops_the_watcher_and_closes_the_inhibitor() {
        let r = rig();
        let sys = r.start(quick());
        assert!(r.inhibitor_open(0));
        drop(sys);
        r.wait_for("close on drop", Duration::from_secs(3), |r| !r.inhibitor_open(0));
    }
}
