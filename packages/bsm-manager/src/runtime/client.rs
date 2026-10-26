//! Blocking typed client of `org.bingux.Session1`, for `bsmctl` and tests.
//!
//! What is implemented
//! -------------------
//! * [`Client`] connects to the system bus (or any [`BusKind`]) and offers one function
//!   per method of the interface, returning the core value types ([`UserInfo`],
//!   [`SessionInfo`], [`StatusInfo`], [`OperationInfo`], [`OutputInfo`], operation ids).
//!   The wire format is documented in `runtime::dbus`; both sides share the row types.
//! * D-Bus errors of the service come back as [`ClientError`] with the stable
//!   [`ErrorCode`] rebuilt from `org.bingux.Session1.Error.<Code>`. Transport failures
//!   have `code: None`; a client-side deadline is `Some(ErrorCode::Timeout)`.
//! * [`Client::signals`] subscribes to the service's signals and yields them as
//!   [`DbusSignal`]. `SessionLaunchProgress` is rebuilt into a full
//!   `bsm_proto::progress::ProgressModel` (the derived `fraction` and `label` fields of the
//!   signal are not needed: the model computes them).
//! * [`monitor_event`] maps a signal to `bsm_proto::control::MonitorEvent`.
//! * Request methods return the operation id and do not wait. [`Client::wait_finished`]
//!   and [`Client::request_and_wait`] wait for `OperationFinished`; both subscribe before
//!   the request is made and fall back to the transition history (`GetTransitions`) at the
//!   deadline, so a signal lost to a full queue does not hang the caller.
//!
//! Every call is bounded by [`Client::set_timeout`] (default 40 s, longer than the
//! service's 25 s bound for `PublishAppearance`).
//!
//! Unverified
//! ----------
//! Exercised against the service of `runtime::dbus` on a private bus only.

use std::collections::HashMap;
use std::os::fd::BorrowedFd;
use std::time::{Duration, Instant};

use async_io::{Timer, block_on};
use bsm_proto::control::{Appearance, MonitorEvent};
use bsm_proto::error::ErrorCode;
use bsm_proto::paths::{BUS_INTERFACE, BUS_NAME, BUS_PATH};
use bsm_proto::progress::{ProgressFailure, ProgressModel};
use bsm_proto::scene::OutputInfo;
use bsm_proto::state::{Generation, OperationId, SessionId};
use futures_lite::StreamExt;
use serde::Serialize;
use serde::de::DeserializeOwned;
use zbus::zvariant::{self, DynamicType, OwnedValue, Value};
use zbus::{Connection, Message, MessageStream};

use super::dbus::{
    ALL_CODES, BusKind, OutputRow, SessionRow, TransitionRow, UserRow, builder, code_from_error_name, name_of,
    parse_name, status_key,
};
use crate::core::{DbusSignal, OperationInfo, SessionInfo, StatusInfo, UserInfo};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(40);
const SIGNAL_QUEUE: usize = 1024;

/// An error of a client call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientError {
    /// The service's stable code; `None` for transport and decoding failures.
    pub code: Option<ErrorCode>,
    pub message: String,
}

impl ClientError {
    fn local(code: Option<ErrorCode>, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.code {
            Some(c) => write!(f, "{}: {}", c.as_str(), self.message),
            None => write!(f, "{}", self.message),
        }
    }
}

impl std::error::Error for ClientError {}

impl From<zbus::Error> for ClientError {
    fn from(e: zbus::Error) -> Self {
        match &e {
            zbus::Error::MethodError(name, text, _) => {
                let code = code_from_error_name(name.as_str());
                let message = text.clone().unwrap_or_else(|| name.to_string());
                Self { code, message: if code.is_some() { message } else { format!("{name}: {message}") } }
            }
            _ => Self { code: None, message: e.to_string() },
        }
    }
}

impl From<zvariant::Error> for ClientError {
    fn from(e: zvariant::Error) -> Self {
        Self::local(None, format!("malformed reply: {e}"))
    }
}

type Result<T, E = ClientError> = std::result::Result<T, E>;

fn malformed(what: &str) -> ClientError {
    ClientError::local(None, format!("malformed reply: {what}"))
}

fn parse_code(s: &str) -> Result<Option<ErrorCode>> {
    if s.is_empty() {
        return Ok(None);
    }
    ALL_CODES.into_iter().find(|c| c.as_str() == s).map(Some).ok_or_else(|| malformed("unknown error code"))
}

fn parse_enum<T: DeserializeOwned>(s: &str, what: &str) -> Result<T> {
    parse_name(s).ok_or_else(|| malformed(what))
}

fn some(s: String) -> Option<String> {
    if s.is_empty() { None } else { Some(s) }
}

fn user_info(r: UserRow) -> Result<UserInfo> {
    let (uid, user_name, display_name, availability, session, avatar_rev, wallpaper_rev) = r;
    Ok(UserInfo {
        uid,
        user_name,
        display_name,
        availability: parse_enum(&availability, "availability")?,
        session: some(session),
        avatar_rev,
        wallpaper_rev,
    })
}

fn session_info(r: SessionRow) -> Result<SessionInfo> {
    let (id, generation, uid, state, availability, foreground, locked, progress) = r;
    Ok(SessionInfo {
        id,
        generation,
        uid,
        state: parse_enum(&state, "session state")?,
        availability: parse_enum(&availability, "availability")?,
        foreground,
        locked,
        progress,
    })
}

fn output_info(r: OutputRow) -> OutputInfo {
    let (name, x, y, width, height, scale_120, refresh_mhz, transform) = r;
    OutputInfo { name, x, y, width, height, scale_120, refresh_mhz, transform }
}

fn operation_info(r: TransitionRow) -> Result<OperationInfo> {
    let (id, kind, phase, state, reason, session) = r;
    Ok(OperationInfo {
        id,
        kind: parse_enum(&kind, "transition kind")?,
        phase: parse_enum(&phase, "operation phase")?,
        state: parse_enum(&state, "transition state")?,
        reason: parse_code(&reason)?,
        session: some(session),
    })
}

fn dict_get<T>(m: &HashMap<String, OwnedValue>, key: &str) -> Result<T>
where
    for<'a> T: TryFrom<&'a Value<'a>>,
    for<'a> <T as TryFrom<&'a Value<'a>>>::Error: Into<zvariant::Error>,
{
    let v = m.get(key).ok_or_else(|| malformed(key))?;
    v.downcast_ref::<T>().map_err(|_| malformed(key))
}

fn dict_opt<T>(m: &HashMap<String, OwnedValue>, key: &str) -> Result<Option<T>>
where
    for<'a> T: TryFrom<&'a Value<'a>>,
    for<'a> <T as TryFrom<&'a Value<'a>>>::Error: Into<zvariant::Error>,
{
    if m.contains_key(key) { dict_get(m, key).map(Some) } else { Ok(None) }
}

fn status_info(m: HashMap<String, OwnedValue>) -> Result<StatusInfo> {
    use status_key as k;
    let operation = match m.get(k::OPERATION) {
        None => None,
        Some(v) => {
            let Value::Structure(s) = &**v else { return Err(malformed(k::OPERATION)) };
            let f = s.fields();
            if f.len() != 6 {
                return Err(malformed(k::OPERATION));
            }
            let text = |i: usize| f[i].downcast_ref::<String>().map_err(|_| malformed(k::OPERATION));
            let id = f[0].downcast_ref::<u64>().map_err(|_| malformed(k::OPERATION))?;
            Some(operation_info((id, text(1)?, text(2)?, text(3)?, text(4)?, text(5)?))?)
        }
    };
    Ok(StatusInfo {
        seat: parse_enum(&dict_get::<String>(&m, k::SEAT)?, "seat")?,
        selected_uid: dict_opt(&m, k::SELECTED_UID)?,
        last_used_uid: dict_opt(&m, k::LAST_USED_UID)?,
        foreground: dict_opt(&m, k::FOREGROUND)?,
        revision: dict_get(&m, k::REVISION)?,
        operation,
        greeter_connected: dict_get(&m, k::GREETER_CONNECTED)?,
        helper_connected: dict_get(&m, k::HELPER_CONNECTED)?,
        outputs: dict_get::<u32>(&m, k::OUTPUTS)? as usize,
    })
}

/// Rebuild a [`DbusSignal`] from a signal message of the service. `None` for anything
/// else, and for a signal whose body does not parse.
pub fn decode_signal(msg: &Message) -> Option<DbusSignal> {
    let hdr = msg.header();
    if hdr.message_type() != zbus::message::Type::Signal || hdr.interface()?.as_str() != BUS_INTERFACE {
        return None;
    }
    let member = hdr.member()?.as_str().to_owned();
    let body = msg.body();
    Some(match member.as_str() {
        "SessionsChanged" => {
            let (revision,): (u64,) = body.deserialize().ok()?;
            DbusSignal::SessionsChanged { revision }
        }
        "SelectedUserChanged" => {
            let (revision, has, uid): (u64, bool, u32) = body.deserialize().ok()?;
            DbusSignal::SelectedUserChanged { revision, uid: has.then_some(uid) }
        }
        "AppearanceChanged" => {
            let (uid, revision): (u32, u64) = body.deserialize().ok()?;
            DbusSignal::AppearanceChanged { uid, revision }
        }
        "SessionLaunchProgress" => {
            type P = (String, u64, u64, Vec<String>, String, u32, u32, u32, String, String, String, String);
            let (session, generation, revision, reached, counted, done, total, _fraction, _label, fs, fc, fm): P =
                body.deserialize().ok()?;
            let failure = if fs.is_empty() {
                None
            } else {
                Some(ProgressFailure {
                    stage: parse_name(&fs)?,
                    code: parse_code(&fc).ok()??,
                    message: fm,
                })
            };
            DbusSignal::SessionLaunchProgress {
                model: ProgressModel {
                    session,
                    generation,
                    revision,
                    reached: reached.iter().map(|s| parse_name(s)).collect::<Option<Vec<_>>>()?,
                    counted_stage: if counted.is_empty() { None } else { Some(parse_name(&counted)?) },
                    done,
                    total,
                    failure,
                },
            }
        }
        "SessionReadinessChanged" => {
            let (session, generation, ready): (String, u64, bool) = body.deserialize().ok()?;
            DbusSignal::SessionReadinessChanged { session, generation, ready }
        }
        "ForegroundChanged" => {
            let (has, session, generation): (bool, String, u64) = body.deserialize().ok()?;
            DbusSignal::ForegroundChanged {
                session: has.then_some(session),
                generation: has.then_some(generation),
            }
        }
        "LockStateChanged" => {
            let (session, locked): (String, bool) = body.deserialize().ok()?;
            DbusSignal::LockStateChanged { session, locked }
        }
        "TransitionChanged" => {
            let (operation_id, kind, phase, state, reason): (u64, String, String, String, String) =
                body.deserialize().ok()?;
            DbusSignal::TransitionChanged {
                operation_id,
                kind: parse_name(&kind)?,
                phase: parse_name(&phase)?,
                state: parse_name(&state)?,
                reason: parse_code(&reason).ok()?,
            }
        }
        "OperationFinished" => {
            let (operation_id, success, reason): (u64, bool, String) = body.deserialize().ok()?;
            DbusSignal::OperationFinished { operation_id, success, reason: parse_code(&reason).ok()? }
        }
        _ => return None,
    })
}

/// The monitor event a signal stands for. Signals that `bsmctl monitor` does not show
/// (`SelectedUserChanged`, `AppearanceChanged`, `SessionReadinessChanged`) map to `None`.
pub fn monitor_event(signal: &DbusSignal) -> Option<MonitorEvent> {
    Some(match signal {
        DbusSignal::SessionsChanged { revision } => MonitorEvent::SessionsChanged { revision: *revision },
        DbusSignal::SessionLaunchProgress { model } => MonitorEvent::Progress {
            session: model.session.clone(),
            generation: model.generation,
            model: model.clone(),
        },
        DbusSignal::ForegroundChanged { session, generation } => {
            MonitorEvent::ForegroundChanged { session: session.clone(), generation: *generation }
        }
        DbusSignal::LockStateChanged { session, locked } => {
            MonitorEvent::LockStateChanged { session: session.clone(), locked: *locked }
        }
        DbusSignal::TransitionChanged { operation_id, kind, phase, reason, .. } => {
            MonitorEvent::Transition { operation_id: *operation_id, kind: *kind, phase: *phase, reason: *reason }
        }
        DbusSignal::OperationFinished { operation_id, success, reason } => {
            MonitorEvent::OperationFinished { operation_id: *operation_id, success: *success, reason: *reason }
        }
        DbusSignal::SelectedUserChanged { .. }
        | DbusSignal::AppearanceChanged { .. }
        | DbusSignal::SessionReadinessChanged { .. } => return None,
    })
}

/// How an operation ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Finished {
    pub operation_id: OperationId,
    pub success: bool,
    pub reason: Option<ErrorCode>,
}

/// Arguments of [`Client::publish_appearance`]. The descriptors must be regular files
/// opened read-only by the caller; see `runtime::dbus` for what the service accepts.
pub struct Publish<'a> {
    pub appearance: &'a Appearance,
    pub show_personal_images: bool,
    pub wallpaper: Option<BorrowedFd<'a>>,
    pub avatar: Option<BorrowedFd<'a>>,
}

/// A live subscription to the service's signals. Iterating blocks.
pub struct SignalStream {
    stream: MessageStream,
}

impl SignalStream {
    /// The next signal, or `None` when `timeout` passes first (or the bus connection ended).
    pub fn next_timeout(&mut self, timeout: Duration) -> Option<DbusSignal> {
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let msg = block_on(futures_lite::future::or(self.stream.next(), async {
                Timer::after(left).await;
                None
            }))?;
            if let Some(sig) = msg.ok().and_then(|m| decode_signal(&m)) {
                return Some(sig);
            }
            if Instant::now() >= deadline {
                return None;
            }
        }
    }

    /// Read signals until `OperationFinished` for `op` arrives or `timeout` passes.
    pub fn wait_finished(&mut self, op: OperationId, timeout: Duration) -> Result<Finished> {
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.next_timeout(left) {
                Some(DbusSignal::OperationFinished { operation_id, success, reason }) if operation_id == op => {
                    return Ok(Finished { operation_id, success, reason });
                }
                Some(_) => {}
                None => {
                    return Err(ClientError::local(
                        Some(ErrorCode::Timeout),
                        format!("operation {op} did not finish within {} ms", timeout.as_millis()),
                    ));
                }
            }
        }
    }
}

impl Iterator for SignalStream {
    type Item = DbusSignal;

    fn next(&mut self) -> Option<DbusSignal> {
        loop {
            let msg = block_on(self.stream.next())?;
            if let Some(sig) = msg.ok().and_then(|m| decode_signal(&m)) {
                return Some(sig);
            }
        }
    }
}

/// Connection to the service.
pub struct Client {
    conn: Connection,
    timeout: Duration,
}

impl Client {
    /// Connect to the system bus.
    pub fn system() -> Result<Self> {
        Self::connect(BusKind::System)
    }

    pub fn connect(kind: BusKind) -> Result<Self> {
        let conn = block_on(async { builder(&kind)?.build().await })?;
        Ok(Self { conn, timeout: DEFAULT_TIMEOUT })
    }

    /// Bound of every call (default 40 s).
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }

    fn call<B, R>(&self, member: &str, body: &B) -> Result<R>
    where
        B: Serialize + DynamicType,
        R: DeserializeOwned + zvariant::Type,
    {
        let timeout = self.timeout;
        block_on(async {
            let call = self.conn.call_method(Some(BUS_NAME), BUS_PATH, Some(BUS_INTERFACE), member, body);
            let reply = futures_lite::future::or(async { Some(call.await) }, async {
                Timer::after(timeout).await;
                None
            })
            .await
            .ok_or_else(|| ClientError::local(Some(ErrorCode::Timeout), format!("{member}: no answer in time")))??;
            Ok(reply.body().deserialize::<R>()?)
        })
    }

    pub fn users(&self) -> Result<Vec<UserInfo>> {
        self.call::<_, Vec<UserRow>>("GetUsers", &())?.into_iter().map(user_info).collect()
    }

    pub fn sessions(&self) -> Result<Vec<SessionInfo>> {
        self.call::<_, Vec<SessionRow>>("GetSessions", &())?.into_iter().map(session_info).collect()
    }

    pub fn selected_user(&self) -> Result<Option<u32>> {
        let (has, uid): (bool, u32) = self.call("GetSelectedUser", &())?;
        Ok(has.then_some(uid))
    }

    pub fn foreground_session(&self) -> Result<Option<(SessionId, Generation)>> {
        let (has, id, generation): (bool, String, u64) = self.call("GetForegroundSession", &())?;
        Ok(has.then_some((id, generation)))
    }

    pub fn status(&self) -> Result<StatusInfo> {
        status_info(self.call("GetStatus", &())?)
    }

    pub fn outputs(&self) -> Result<Vec<OutputInfo>> {
        Ok(self.call::<_, Vec<OutputRow>>("GetOutputs", &())?.into_iter().map(output_info).collect())
    }

    pub fn transitions(&self) -> Result<Vec<OperationInfo>> {
        self.call::<_, Vec<TransitionRow>>("GetTransitions", &())?.into_iter().map(operation_info).collect()
    }

    pub fn select_greeter_user(&self, uid: u32) -> Result<()> {
        self.call("SelectGreeterUser", &(uid,))
    }

    /// `None` locks the caller's own session.
    pub fn request_lock(&self, session: Option<&str>) -> Result<OperationId> {
        self.call("RequestLock", &(session.unwrap_or(""),))
    }

    pub fn request_switch_user(&self) -> Result<OperationId> {
        self.call("RequestSwitchUser", &())
    }

    /// `None` logs out the caller's own session.
    pub fn request_logout(&self, session: Option<&str>) -> Result<OperationId> {
        self.call("RequestLogout", &(session.unwrap_or(""),))
    }

    pub fn request_suspend(&self) -> Result<OperationId> {
        self.call("RequestSuspend", &())
    }

    pub fn request_reboot(&self) -> Result<OperationId> {
        self.call("RequestReboot", &())
    }

    pub fn request_power_off(&self) -> Result<OperationId> {
        self.call("RequestPowerOff", &())
    }

    pub fn cancel_operation(&self, operation: OperationId) -> Result<()> {
        self.call("CancelOperation", &(operation,))
    }

    pub fn publish_appearance(&self, p: &Publish<'_>) -> Result<()> {
        fn fds<'f>(fd: Option<BorrowedFd<'f>>) -> Vec<zvariant::Fd<'f>> {
            fd.into_iter().map(zvariant::Fd::from).collect()
        }
        self.call(
            "PublishAppearance",
            &(
                name_of(&p.appearance.color_scheme),
                p.appearance.accent.as_str(),
                name_of(&p.appearance.clock_format),
                p.appearance.reduce_motion,
                p.show_personal_images,
                fds(p.wallpaper),
                fds(p.avatar),
            ),
        )
    }

    /// Subscribe to the service's signals. Signals sent after this returns are queued
    /// (up to 1024) until read.
    pub fn signals(&self) -> Result<SignalStream> {
        let rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender(BUS_NAME)?
            .interface(BUS_INTERFACE)?
            .path(BUS_PATH)?
            .build();
        let stream = block_on(MessageStream::for_match_rule(rule, &self.conn, Some(SIGNAL_QUEUE)))?;
        Ok(SignalStream { stream })
    }

    fn finished_from_history(&self, op: OperationId) -> Result<Option<Finished>> {
        Ok(self.transitions()?.into_iter().find(|o| o.id == op && o.phase.is_final()).map(|o| Finished {
            operation_id: op,
            success: o.phase == bsm_proto::state::OperationPhase::Completed,
            reason: o.reason,
        }))
    }

    /// Wait until operation `op` finished. Subscribes first, then checks the history, so
    /// an operation that ended before the call is still reported.
    pub fn wait_finished(&self, op: OperationId, timeout: Duration) -> Result<Finished> {
        let mut signals = self.signals()?;
        if let Some(f) = self.finished_from_history(op)? {
            return Ok(f);
        }
        self.wait_on(&mut signals, op, timeout)
    }

    /// Make a request and wait for its end. The subscription exists before `request`
    /// runs, so the end of a fast operation cannot be missed.
    pub fn request_and_wait(
        &self,
        timeout: Duration,
        request: impl FnOnce(&Client) -> Result<OperationId>,
    ) -> Result<Finished> {
        let mut signals = self.signals()?;
        let op = request(self)?;
        self.wait_on(&mut signals, op, timeout)
    }

    fn wait_on(&self, signals: &mut SignalStream, op: OperationId, timeout: Duration) -> Result<Finished> {
        match signals.wait_finished(op, timeout) {
            Err(e) if e.code == Some(ErrorCode::Timeout) => match self.finished_from_history(op) {
                Ok(Some(f)) => Ok(f),
                _ => Err(e),
            },
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Method, Reply};
    use crate::runtime::dbus::testutil::*;
    use crate::runtime::dbus::{Config, Timeouts, spawn_with};
    use bsm_proto::error::BsmError;
    use bsm_proto::progress::Stage;
    use bsm_proto::state::{
        OperationPhase, SeatState, SessionAvailability, SessionState, TransitionKind, TransitionState,
    };

    fn status() -> StatusInfo {
        StatusInfo {
            seat: SeatState::UserForeground,
            selected_uid: None,
            last_used_uid: Some(1000),
            foreground: Some("s0000000a".into()),
            revision: 21,
            operation: Some(OperationInfo {
                id: 8,
                kind: TransitionKind::Suspend,
                phase: OperationPhase::Authorising,
                state: TransitionState::WaitingForAuth,
                reason: Some(ErrorCode::Inhibited),
                session: None,
            }),
            greeter_connected: false,
            helper_connected: true,
            outputs: 3,
        }
    }

    fn users() -> Vec<UserInfo> {
        vec![UserInfo {
            uid: 1000,
            user_name: "ada".into(),
            display_name: "Ada".into(),
            availability: SessionAvailability::Starting,
            session: Some("s0000000a".into()),
            avatar_rev: 1,
            wallpaper_rev: 2,
        }]
    }

    fn sessions() -> Vec<SessionInfo> {
        vec![SessionInfo {
            id: "s0000000a".into(),
            generation: u64::MAX - 1,
            uid: 1000,
            state: SessionState::BinguxStarting,
            availability: SessionAvailability::Starting,
            foreground: false,
            locked: false,
            progress: 35,
        }]
    }

    fn outputs() -> Vec<OutputInfo> {
        vec![OutputInfo {
            name: "DP-1".into(),
            x: 0,
            y: 0,
            width: 3840,
            height: 2160,
            scale_120: 180,
            refresh_mhz: 59940,
            transform: 1,
        }]
    }

    fn transitions() -> Vec<OperationInfo> {
        vec![
            OperationInfo {
                id: 8,
                kind: TransitionKind::Suspend,
                phase: OperationPhase::Authorising,
                state: TransitionState::WaitingForAuth,
                reason: None,
                session: None,
            },
            OperationInfo {
                id: 5,
                kind: TransitionKind::Lock,
                phase: OperationPhase::Completed,
                state: TransitionState::Committed,
                reason: None,
                session: Some("s0000000a".into()),
            },
            OperationInfo {
                id: 6,
                kind: TransitionKind::PowerFade,
                phase: OperationPhase::Failed,
                state: TransitionState::Failed,
                reason: Some(ErrorCode::ShutdownRejected),
                session: None,
            },
        ]
    }

    fn script() -> Script {
        Box::new(|m| {
            Some(match m {
                Method::GetUsers => Ok(Reply::Users(users())),
                Method::GetSessions => Ok(Reply::Sessions(sessions())),
                Method::GetSelectedUser => Ok(Reply::Selected(None)),
                Method::GetForegroundSession => Ok(Reply::Foreground(None)),
                Method::GetStatus => Ok(Reply::Status(status())),
                Method::GetOutputs => Ok(Reply::Outputs(outputs())),
                Method::GetTransitions => Ok(Reply::Transitions(transitions())),
                Method::SelectGreeterUser { .. } => Ok(Reply::Done),
                Method::RequestLock { session } => {
                    Ok(Reply::Operation(if session.is_some() { 100 } else { 101 }))
                }
                Method::RequestSwitchUser => Ok(Reply::Operation(102)),
                Method::RequestLogout { .. } => Err(BsmError::new(ErrorCode::NoSession, "no session")),
                Method::RequestSuspend => Ok(Reply::Operation(5)),
                Method::RequestReboot => Ok(Reply::Operation(6)),
                Method::RequestPowerOff => Ok(Reply::Operation(77)),
                Method::CancelOperation { .. } => Ok(Reply::Done),
                Method::PublishAppearance(r) => {
                    assert!(r.wallpaper.is_some() && r.avatar.is_some());
                    assert_eq!(r.appearance.accent, "#aabbcc");
                    Ok(Reply::Done)
                }
            })
        })
    }

    struct Rig {
        _bus: PrivateBus,
        handle: crate::runtime::dbus::DbusHandle,
        _driver: Driver,
        client: Client,
    }

    fn rig() -> Rig {
        let bus = PrivateBus::start();
        let (tx, driver) = driver(script());
        let handle = spawn_with(bus.kind(), tx, Config { timeouts: Timeouts::default(), signal_queue: 64 }).unwrap();
        let client = Client::connect(bus.kind()).unwrap();
        Rig { _bus: bus, handle, _driver: driver, client }
    }

    #[test]
    fn typed_getters_round_trip() {
        let r = rig();
        let c = &r.client;
        assert_eq!(c.users().unwrap(), users());
        assert_eq!(c.sessions().unwrap(), sessions());
        assert_eq!(c.selected_user().unwrap(), None);
        assert_eq!(c.foreground_session().unwrap(), None);
        assert_eq!(c.status().unwrap(), status());
        assert_eq!(c.outputs().unwrap(), outputs());
        assert_eq!(c.transitions().unwrap(), transitions());
        c.select_greeter_user(1000).unwrap();
        assert_eq!(c.request_lock(Some("s0000000a")).unwrap(), 100);
        assert_eq!(c.request_lock(None).unwrap(), 101);
        assert_eq!(c.request_switch_user().unwrap(), 102);
        assert_eq!(c.request_power_off().unwrap(), 77);
        c.cancel_operation(3).unwrap();
    }

    #[test]
    fn errors_rebuild_the_code() {
        let r = rig();
        let e = r.client.request_logout(None).unwrap_err();
        assert_eq!(e.code, Some(ErrorCode::NoSession));
        assert_eq!(e.message, "no session");
        let e = r.client.request_lock(Some("bad id")).unwrap_err();
        assert_eq!(e.code, Some(ErrorCode::InvalidArgument));
    }

    #[test]
    fn a_missing_service_is_a_transport_error() {
        let bus = PrivateBus::start();
        let c = Client::connect(bus.kind()).unwrap();
        let e = c.users().unwrap_err();
        assert_eq!(e.code, None);
    }

    #[test]
    fn publish_sends_descriptors() {
        let r = rig();
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a");
        std::fs::write(&p, b"x").unwrap();
        let a = std::fs::File::open(&p).unwrap();
        let b = std::fs::File::open(&p).unwrap();
        use std::os::fd::AsFd;
        let ap = Appearance { accent: "#aabbcc".into(), ..Appearance::default() };
        r.client
            .publish_appearance(&Publish {
                appearance: &ap,
                show_personal_images: true,
                wallpaper: Some(a.as_fd()),
                avatar: Some(b.as_fd()),
            })
            .unwrap();
        let w = std::fs::OpenOptions::new().write(true).open(&p).unwrap();
        let e = r
            .client
            .publish_appearance(&Publish {
                appearance: &ap,
                show_personal_images: true,
                wallpaper: Some(w.as_fd()),
                avatar: None,
            })
            .unwrap_err();
        assert_eq!(e.code, Some(ErrorCode::InvalidArgument));
    }

    #[test]
    fn signals_rebuild_core_values_and_monitor_events() {
        let r = rig();
        let mut sig = r.client.signals().unwrap();
        let mut model = ProgressModel::new("s0000000a".into(), 9);
        model.reach(Stage::AuthAccepted);
        model.reach(Stage::PamSession);
        model.reach(Stage::HostConnected);
        model.reach(Stage::GnoblinReady);
        model.count(Stage::OutputsConfigured, 1, 2);
        let mut failed = ProgressModel::new("s0000000b".into(), 10);
        failed.reach(Stage::AuthAccepted);
        failed.fail(ProgressFailure { stage: Stage::PamSession, code: ErrorCode::PamFailure, message: "boom".into() });
        let sent = vec![
            DbusSignal::SessionsChanged { revision: 3 },
            DbusSignal::SelectedUserChanged { revision: 4, uid: Some(1000) },
            DbusSignal::SelectedUserChanged { revision: 5, uid: None },
            DbusSignal::AppearanceChanged { uid: 1000, revision: 6 },
            DbusSignal::SessionLaunchProgress { model },
            DbusSignal::SessionLaunchProgress { model: failed },
            DbusSignal::SessionReadinessChanged { session: "s0000000a".into(), generation: 9, ready: true },
            DbusSignal::ForegroundChanged { session: Some("s0000000a".into()), generation: Some(9) },
            DbusSignal::ForegroundChanged { session: None, generation: None },
            DbusSignal::LockStateChanged { session: "s0000000a".into(), locked: false },
            DbusSignal::TransitionChanged {
                operation_id: 12,
                kind: TransitionKind::UnlockSlideUp,
                phase: OperationPhase::Animating,
                state: TransitionState::Animating,
                reason: None,
            },
            DbusSignal::OperationFinished { operation_id: 12, success: false, reason: Some(ErrorCode::Cancelled) },
        ];
        for s in &sent {
            r.handle.emit(s.clone());
        }
        for want in &sent {
            let got = sig.next_timeout(Duration::from_secs(5)).expect("signal");
            assert_eq!(&got, want);
            let ev = monitor_event(&got);
            match want {
                DbusSignal::SelectedUserChanged { .. }
                | DbusSignal::AppearanceChanged { .. }
                | DbusSignal::SessionReadinessChanged { .. } => assert!(ev.is_none()),
                DbusSignal::SessionLaunchProgress { model } => {
                    let Some(MonitorEvent::Progress { session, generation, model: m }) = ev else { panic!() };
                    assert_eq!((&session, generation, &m), (&model.session, model.generation, model));
                }
                DbusSignal::TransitionChanged { operation_id, kind, phase, reason, .. } => assert_eq!(
                    ev,
                    Some(MonitorEvent::Transition {
                        operation_id: *operation_id,
                        kind: *kind,
                        phase: *phase,
                        reason: *reason
                    })
                ),
                DbusSignal::OperationFinished { operation_id, success, reason } => assert_eq!(
                    ev,
                    Some(MonitorEvent::OperationFinished {
                        operation_id: *operation_id,
                        success: *success,
                        reason: *reason
                    })
                ),
                _ => assert!(ev.is_some()),
            }
        }
        assert!(sig.next_timeout(Duration::from_millis(100)).is_none());
    }

    #[test]
    fn request_and_wait_sees_an_immediate_finish_and_falls_back_to_history() {
        let r = rig();
        // The signal is sent while the request is in flight, before the id is known.
        let done = r
            .client
            .request_and_wait(Duration::from_secs(5), |c| {
                r.handle.emit(DbusSignal::OperationFinished { operation_id: 5, success: true, reason: None });
                c.request_suspend()
            })
            .unwrap();
        assert_eq!(done, Finished { operation_id: 5, success: true, reason: None });

        // No signal at all: op 6 is final in the history (failed, reason set).
        let done = r.client.request_and_wait(Duration::from_millis(300), |c| c.request_reboot()).unwrap();
        assert_eq!(done, Finished { operation_id: 6, success: false, reason: Some(ErrorCode::ShutdownRejected) });

        // wait_finished on an operation that already ended finds it in the history.
        assert!(r.client.wait_finished(5, Duration::from_millis(300)).unwrap().success);

        // Never finishes (op 8 is in progress in the history, op 77 unknown): a timeout.
        let e = r.client.request_and_wait(Duration::from_millis(300), |c| c.request_power_off()).unwrap_err();
        assert_eq!(e.code, Some(ErrorCode::Timeout));
    }
}
