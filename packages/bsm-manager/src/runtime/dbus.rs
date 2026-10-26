//! The system D-Bus service `org.bingux.Session1` (object `/org/bingux/Session1`).
//!
//! What is implemented
//! -------------------
//! * [`spawn`] starts the service on its own thread (one `async-io` executor thread of
//!   zbus plus one emitter thread named `bsm-dbus`) and returns a [`DbusHandle`] once the
//!   well-known name is owned, so the daemon sends `sd_notify READY` only after that.
//! * Every method call resolves the caller **from the bus** (`GetConnectionCredentials`:
//!   unix user id and process id, the unique sender name from the message header and the
//!   unified cgroup path from `/proc/<pid>/cgroup`, `None` when unreadable) and forwards
//!   `RtInput::Call(DbusCall)` through the bounded driver channel. Nothing in the
//!   arguments is ever used for identity. A caller whose uid or pid the bus does not
//!   report is refused with `Unauthorised`.
//! * The driver's answer is awaited for [`Timeouts::call`] (5 s) or [`Timeouts::publish`]
//!   (25 s, `PublishAppearance`). When the driver queue is full the call waits for room
//!   inside the same deadline, then fails with `Timeout`. The service never blocks a
//!   zbus executor thread.
//! * Arguments are validated here (string length caps, `color_scheme`, `clock_format`,
//!   file descriptors) and refused with `InvalidArgument` before the core sees them.
//! * [`DbusHandle::emit`] never blocks: it pushes into a bounded queue and, when the
//!   queue is full, drops the new signal and counts it ([`DbusHandle::dropped_signals`]).
//!   Signals carry revisions, so a client that notices a gap re-reads with `Get*`.
//!
//! Error mapping
//! -------------
//! A `BsmError` becomes the D-Bus error `org.bingux.Session1.Error.<Code>` with the
//! message as text. `<Code>` is the CamelCase form of `ErrorCode::as_str()`, which is
//! also the name of the `ErrorCode` variant: `unauthorised` is `Unauthorised`,
//! `stale_generation` is `StaleGeneration`. [`error_name`] and [`code_from_error_name`]
//! convert both ways; the client uses them to rebuild the code.
//!
//! Wire signatures
//! ---------------
//! The signatures below are the contract; `INTROSPECTION_XML` is the same thing as XML
//! and a test compares it with the live introspection of the running service.
//!
//! ```text
//! GetUsers()              -> a(usssstt)    uid, user_name, display_name, availability,
//!                                          session ("" if none), avatar_rev, wallpaper_rev
//! GetSessions()           -> a(stussbbu)   id, generation, uid, state, availability,
//!                                          foreground, locked, progress 0..=100
//! GetSelectedUser()       -> (bu)          has, uid
//! GetForegroundSession()  -> (bst)         has, id, generation
//! GetStatus()             -> a{sv}         keys: seat s, selected_uid u (absent = none),
//!                                          last_used_uid u (absent = none), foreground s
//!                                          (absent = none), revision t, greeter_connected b,
//!                                          helper_connected b, outputs u, operation
//!                                          (tsssss) = operation_id, kind, phase, state,
//!                                          reason, session (absent = no operation)
//! GetOutputs()            -> a(siiuuuuu)   name, x, y, width, height, scale_120,
//!                                          refresh_mhz, transform
//! GetTransitions()        -> a(tsssss)     operation_id, kind, phase, state, reason ("" none),
//!                                          session ("" none)
//! SelectGreeterUser(u)
//! RequestLock(s)          -> t             "" means the caller's own session
//! RequestSwitchUser()     -> t
//! RequestLogout(s)        -> t             "" means the caller's own session
//! RequestSuspend()        -> t
//! RequestReboot()         -> t
//! RequestPowerOff()       -> t
//! CancelOperation(t)
//! PublishAppearance(color_scheme s, accent s, clock_format s, reduce_motion b,
//!                   show_personal_images b, wallpaper_fd ah, avatar_fd ah)
//! ```
//!
//! `PublishAppearance` takes each image as an array of zero or one unix file
//! descriptors (`ah`): an empty array means "no image of this kind", more than one
//! descriptor is `InvalidArgument`. bsm never opens a path supplied by a user. Every
//! descriptor must be a regular file opened read-only, at most [`MAX_IMAGE_BYTES`]
//! (16 MiB) in size, and not an `O_PATH` descriptor; anything else is refused with
//! `InvalidArgument`. The validated descriptor is duplicated with `F_DUPFD_CLOEXEC` and
//! carried as `core::FdHandle`. It shares its open file description with the sender, so a
//! consumer MUST read with `pread` (`FileExt::read_at`) and must re-apply the size cap
//! while reading: the sender can grow the file after the check.
//!
//! Why not re-open through `/proc/self/fd/<n>`: the re-open is permission checked against
//! the file for the bsm uid, which fails for the usual `0600` image of a user (the reason
//! the user passes a descriptor at all), and `O_NOFOLLOW` fails on that magic link.
//!
//! Signals: `SessionsChanged(t)`, `SelectedUserChanged(t, b, u)`, `AppearanceChanged(u, t)`,
//! `SessionLaunchProgress(s, t, t, as, s, u, u, u, s, s, s, s)`,
//! `SessionReadinessChanged(s, t, b)`, `ForegroundChanged(b, s, t)`, `LockStateChanged(s, b)`,
//! `TransitionChanged(t, s, s, s, s)`, `OperationFinished(t, b, s)`. Enumerations travel as
//! their serde snake_case names.
//!
//! Unverified
//! ----------
//! Tested against a private `dbus-daemon --session` only. The system bus policy file that
//! lets the `bsm` account own the name and everyone call the methods belongs to the
//! packaging slice and is not exercised here. `GetConnectionCredentials` on dbus-broker
//! is assumed to behave like dbus-daemon's.

use std::collections::HashMap;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{SyncSender, TrySendError, sync_channel};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_io::Timer;
use bsm_proto::control::{Appearance, ClockFormat, ColorScheme};
use bsm_proto::error::{BsmError, ErrorCode};
use bsm_proto::paths::{BUS_NAME, BUS_PATH};
use bsm_proto::progress::ProgressModel;
use serde::Serialize;
use serde::de::DeserializeOwned;
use zbus::Connection;
use zbus::message::Header;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{self, StructureBuilder, Value};

use super::api::{DbusCall, RtInput};
use crate::core::{
    Caller, DbusSignal, FdHandle, Method, OperationInfo, PublishRequest, Reply, SessionInfo, StatusInfo, UserInfo,
};
use bsm_proto::scene::OutputInfo;

/// Largest image a caller may hand over.
pub const MAX_IMAGE_BYTES: u64 = 16 * 1024 * 1024;
/// Prefix of every error name of the service.
pub const ERROR_PREFIX: &str = "org.bingux.Session1.Error";

const MAX_SESSION_ID_LEN: usize = 64;
const MAX_ACCENT_LEN: usize = 32;
const MAX_SHORT_ARG_LEN: usize = 16;
const START_TIMEOUT: Duration = Duration::from_secs(15);

/// Which bus the service or client talks to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BusKind {
    System,
    Session,
    /// An explicit address, for tests and private buses.
    Address(String),
}

pub(crate) fn builder(kind: &BusKind) -> zbus::Result<zbus::connection::Builder<'static>> {
    match kind {
        BusKind::System => zbus::connection::Builder::system(),
        BusKind::Session => zbus::connection::Builder::session(),
        BusKind::Address(a) => zbus::connection::Builder::address(a.as_str()),
    }
}

/// How long a method call may wait for the driver. Shortened in tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timeouts {
    pub call: Duration,
    pub publish: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self { call: Duration::from_secs(5), publish: Duration::from_secs(25) }
    }
}

/// Tunables of [`spawn_with`].
#[derive(Debug, Clone, Copy)]
pub struct Config {
    pub timeouts: Timeouts,
    /// Capacity of the outgoing signal queue.
    pub signal_queue: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self { timeouts: Timeouts::default(), signal_queue: 256 }
    }
}

// ---------------------------------------------------------------------------------------
// Names of enumerations and error codes.

/// The serde snake_case name of a protocol enumeration.
pub(crate) fn name_of<T: Serialize>(value: &T) -> String {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(s)) => s,
        _ => String::new(),
    }
}

/// Inverse of [`name_of`]; `None` for an unknown name.
pub(crate) fn parse_name<T: DeserializeOwned>(name: &str) -> Option<T> {
    serde_json::from_value(serde_json::Value::String(name.to_owned())).ok()
}

/// Every error code. `all_codes_are_listed` fails to compile when a variant is added.
pub const ALL_CODES: [ErrorCode; 18] = [
    ErrorCode::Unauthorised,
    ErrorCode::InvalidAccount,
    ErrorCode::NoSession,
    ErrorCode::AlreadyLocked,
    ErrorCode::Busy,
    ErrorCode::StaleGeneration,
    ErrorCode::Cancelled,
    ErrorCode::PamFailure,
    ErrorCode::LogindFailure,
    ErrorCode::OutputNotReady,
    ErrorCode::TransitionInterrupted,
    ErrorCode::Inhibited,
    ErrorCode::ShutdownRejected,
    ErrorCode::Timeout,
    ErrorCode::HelperFailure,
    ErrorCode::InvalidArgument,
    ErrorCode::NotSupported,
    ErrorCode::Internal,
];

fn camel(snake: &str) -> String {
    snake
        .split('_')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_ascii_uppercase().to_string() + c.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

/// Full D-Bus error name for a code.
pub fn error_name(code: ErrorCode) -> String {
    format!("{ERROR_PREFIX}.{}", camel(code.as_str()))
}

/// The code behind a D-Bus error name of this service.
pub fn code_from_error_name(name: &str) -> Option<ErrorCode> {
    let suffix = name.strip_prefix(ERROR_PREFIX)?.strip_prefix('.')?;
    ALL_CODES.into_iter().find(|c| camel(c.as_str()) == suffix)
}

/// The error type of the interface. One variant per `ErrorCode`, named like it, so the
/// derive produces `org.bingux.Session1.Error.<Variant>`.
#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.bingux.Session1.Error")]
pub enum SessionError {
    #[zbus(error)]
    ZBus(zbus::Error),
    Unauthorised(String),
    InvalidAccount(String),
    NoSession(String),
    AlreadyLocked(String),
    Busy(String),
    StaleGeneration(String),
    Cancelled(String),
    PamFailure(String),
    LogindFailure(String),
    OutputNotReady(String),
    TransitionInterrupted(String),
    Inhibited(String),
    ShutdownRejected(String),
    Timeout(String),
    HelperFailure(String),
    InvalidArgument(String),
    NotSupported(String),
    Internal(String),
}

impl From<BsmError> for SessionError {
    fn from(e: BsmError) -> Self {
        let m = e.message;
        match e.code {
            ErrorCode::Unauthorised => Self::Unauthorised(m),
            ErrorCode::InvalidAccount => Self::InvalidAccount(m),
            ErrorCode::NoSession => Self::NoSession(m),
            ErrorCode::AlreadyLocked => Self::AlreadyLocked(m),
            ErrorCode::Busy => Self::Busy(m),
            ErrorCode::StaleGeneration => Self::StaleGeneration(m),
            ErrorCode::Cancelled => Self::Cancelled(m),
            ErrorCode::PamFailure => Self::PamFailure(m),
            ErrorCode::LogindFailure => Self::LogindFailure(m),
            ErrorCode::OutputNotReady => Self::OutputNotReady(m),
            ErrorCode::TransitionInterrupted => Self::TransitionInterrupted(m),
            ErrorCode::Inhibited => Self::Inhibited(m),
            ErrorCode::ShutdownRejected => Self::ShutdownRejected(m),
            ErrorCode::Timeout => Self::Timeout(m),
            ErrorCode::HelperFailure => Self::HelperFailure(m),
            ErrorCode::InvalidArgument => Self::InvalidArgument(m),
            ErrorCode::NotSupported => Self::NotSupported(m),
            ErrorCode::Internal => Self::Internal(m),
        }
    }
}

fn err(code: ErrorCode, message: impl Into<String>) -> BsmError {
    BsmError::new(code, message)
}

// ---------------------------------------------------------------------------------------
// Rows: the wire form of the core's value types. The client decodes with the same types.

pub(crate) type UserRow = (u32, String, String, String, String, u64, u64);
pub(crate) type SessionRow = (String, u64, u32, String, String, bool, bool, u32);
pub(crate) type OutputRow = (String, i32, i32, u32, u32, u32, u32, u32);
pub(crate) type TransitionRow = (u64, String, String, String, String, String);

pub(crate) fn user_row(u: &UserInfo) -> UserRow {
    (
        u.uid,
        u.user_name.clone(),
        u.display_name.clone(),
        name_of(&u.availability),
        u.session.clone().unwrap_or_default(),
        u.avatar_rev,
        u.wallpaper_rev,
    )
}

pub(crate) fn session_row(s: &SessionInfo) -> SessionRow {
    (
        s.id.clone(),
        s.generation,
        s.uid,
        name_of(&s.state),
        name_of(&s.availability),
        s.foreground,
        s.locked,
        s.progress,
    )
}

pub(crate) fn output_row(o: &OutputInfo) -> OutputRow {
    (o.name.clone(), o.x, o.y, o.width, o.height, o.scale_120, o.refresh_mhz, o.transform)
}

pub(crate) fn transition_row(o: &OperationInfo) -> TransitionRow {
    (
        o.id,
        name_of(&o.kind),
        name_of(&o.phase),
        name_of(&o.state),
        o.reason.map(|c| c.as_str().to_owned()).unwrap_or_default(),
        o.session.clone().unwrap_or_default(),
    )
}

/// Keys of the `GetStatus` dictionary.
pub(crate) mod status_key {
    pub const SEAT: &str = "seat";
    pub const SELECTED_UID: &str = "selected_uid";
    pub const LAST_USED_UID: &str = "last_used_uid";
    pub const FOREGROUND: &str = "foreground";
    pub const REVISION: &str = "revision";
    pub const GREETER_CONNECTED: &str = "greeter_connected";
    pub const HELPER_CONNECTED: &str = "helper_connected";
    pub const OUTPUTS: &str = "outputs";
    pub const OPERATION: &str = "operation";
}

pub(crate) fn status_dict(s: &StatusInfo) -> Result<HashMap<String, Value<'static>>, BsmError> {
    use status_key as k;
    let mut m: HashMap<String, Value<'static>> = HashMap::new();
    m.insert(k::SEAT.into(), Value::from(name_of(&s.seat)));
    if let Some(u) = s.selected_uid {
        m.insert(k::SELECTED_UID.into(), Value::from(u));
    }
    if let Some(u) = s.last_used_uid {
        m.insert(k::LAST_USED_UID.into(), Value::from(u));
    }
    if let Some(f) = &s.foreground {
        m.insert(k::FOREGROUND.into(), Value::from(f.clone()));
    }
    m.insert(k::REVISION.into(), Value::from(s.revision));
    m.insert(k::GREETER_CONNECTED.into(), Value::from(s.greeter_connected));
    m.insert(k::HELPER_CONNECTED.into(), Value::from(s.helper_connected));
    m.insert(k::OUTPUTS.into(), Value::from(u32::try_from(s.outputs).unwrap_or(u32::MAX)));
    if let Some(op) = &s.operation {
        let (id, kind, phase, state, reason, session) = transition_row(op);
        let st = StructureBuilder::new()
            .add_field(id)
            .add_field(kind)
            .add_field(phase)
            .add_field(state)
            .add_field(reason)
            .add_field(session)
            .build()
            .map_err(|e| err(ErrorCode::Internal, format!("status operation: {e}")))?;
        m.insert(k::OPERATION.into(), Value::Structure(st));
    }
    Ok(m)
}

// ---------------------------------------------------------------------------------------
// Argument validation.

fn session_arg(s: &str) -> Result<Option<String>, BsmError> {
    if s.is_empty() {
        return Ok(None);
    }
    if s.len() > MAX_SESSION_ID_LEN || !s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
        return Err(err(ErrorCode::InvalidArgument, "malformed session id"));
    }
    Ok(Some(s.to_owned()))
}

/// Check one image descriptor and return a private duplicate of it.
pub(crate) fn validate_image_fd(what: &str, fd: &OwnedFd) -> Result<FdHandle, BsmError> {
    let bad = |why: &str| err(ErrorCode::InvalidArgument, format!("{what}: {why}"));
    let raw = fd.as_raw_fd();
    // SAFETY: `raw` is a descriptor owned by `fd` for the whole function.
    let flags = unsafe { libc::fcntl(raw, libc::F_GETFL) };
    if flags < 0 {
        return Err(bad("not a usable file descriptor"));
    }
    if flags & libc::O_PATH != 0 {
        return Err(bad("an O_PATH descriptor cannot be read"));
    }
    if flags & libc::O_ACCMODE != libc::O_RDONLY {
        return Err(bad("must be opened read-only"));
    }
    let mut st = std::mem::MaybeUninit::<libc::stat>::zeroed();
    // SAFETY: `st` is a valid out pointer, `raw` a valid descriptor.
    if unsafe { libc::fstat(raw, st.as_mut_ptr()) } != 0 {
        return Err(bad("cannot stat"));
    }
    // SAFETY: fstat succeeded and initialised the structure.
    let st = unsafe { st.assume_init() };
    if st.st_mode & libc::S_IFMT != libc::S_IFREG {
        return Err(bad("must be a regular file"));
    }
    if st.st_size < 0 || st.st_size as u64 > MAX_IMAGE_BYTES {
        return Err(bad("larger than 16 MiB"));
    }
    // SAFETY: F_DUPFD_CLOEXEC returns a new descriptor that we own, or -1.
    let dup = unsafe { libc::fcntl(raw, libc::F_DUPFD_CLOEXEC, 3) };
    if dup < 0 {
        return Err(err(ErrorCode::Internal, format!("{what}: cannot duplicate descriptor")));
    }
    // SAFETY: `dup` was just created and nothing else owns it.
    Ok(FdHandle(Arc::new(unsafe { OwnedFd::from_raw_fd(dup) })))
}

fn image_arg(what: &str, fds: Vec<zvariant::OwnedFd>) -> Result<Option<FdHandle>, BsmError> {
    let mut fds = fds;
    match fds.len() {
        0 => Ok(None),
        1 => {
            let fd: OwnedFd = fds.remove(0).into();
            validate_image_fd(what, &fd).map(Some)
        }
        _ => Err(err(ErrorCode::InvalidArgument, format!("{what}: at most one descriptor"))),
    }
}

pub(crate) struct PublishArgs {
    pub color_scheme: String,
    pub accent: String,
    pub clock_format: String,
    pub reduce_motion: bool,
    pub show_personal_images: bool,
}

pub(crate) fn build_publish(
    a: PublishArgs,
    wallpaper_fd: Vec<zvariant::OwnedFd>,
    avatar_fd: Vec<zvariant::OwnedFd>,
) -> Result<PublishRequest, BsmError> {
    let color_scheme = match a.color_scheme.as_str() {
        "dark" => ColorScheme::Dark,
        "light" => ColorScheme::Light,
        _ => return Err(err(ErrorCode::InvalidArgument, "color_scheme must be dark or light")),
    };
    let clock_format = match a.clock_format.as_str() {
        "12h" => ClockFormat::H12,
        "24h" => ClockFormat::H24,
        _ => return Err(err(ErrorCode::InvalidArgument, "clock_format must be 12h or 24h")),
    };
    if a.accent.len() > MAX_ACCENT_LEN || !a.accent.is_ascii() {
        return Err(err(ErrorCode::InvalidArgument, "accent is too long or not ASCII"));
    }
    if a.color_scheme.len() > MAX_SHORT_ARG_LEN || a.clock_format.len() > MAX_SHORT_ARG_LEN {
        return Err(err(ErrorCode::InvalidArgument, "argument too long"));
    }
    let wallpaper = image_arg("wallpaper_fd", wallpaper_fd)?;
    let avatar = image_arg("avatar_fd", avatar_fd)?;
    Ok(PublishRequest {
        appearance: Appearance { color_scheme, accent: a.accent, clock_format, reduce_motion: a.reduce_motion },
        show_personal_images: a.show_personal_images,
        wallpaper,
        avatar,
    })
}

// ---------------------------------------------------------------------------------------
// Caller resolution and the round trip to the driver.

pub(crate) async fn with_timeout<F: Future>(d: Duration, f: F) -> Option<F::Output> {
    futures_lite::future::or(async { Some(f.await) }, async {
        Timer::after(d).await;
        None
    })
    .await
}

pub(crate) fn read_cgroup(pid: u32) -> Option<String> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/cgroup")).ok()?;
    text.lines().find_map(|l| l.strip_prefix("0::")).map(|s| s.trim().to_owned())
}

async fn resolve_caller(conn: &Connection, hdr: &Header<'_>, budget: Duration) -> Result<Caller, BsmError> {
    let sender = hdr.sender().ok_or_else(|| err(ErrorCode::Unauthorised, "call without a sender"))?;
    let bus_name = sender.as_str().to_owned();
    let who = || err(ErrorCode::Unauthorised, "caller identity could not be established");
    let proxy = zbus::fdo::DBusProxy::new(conn).await.map_err(|e| err(ErrorCode::Internal, format!("bus proxy: {e}")))?;
    let name = zbus::names::BusName::Unique(sender.clone());
    let creds = match with_timeout(budget, proxy.get_connection_credentials(name)).await {
        Some(Ok(c)) => c,
        Some(Err(_)) => return Err(who()),
        None => return Err(err(ErrorCode::Timeout, "bus did not report the caller in time")),
    };
    let (Some(uid), Some(pid)) = (creds.unix_user_id(), creds.process_id()) else {
        return Err(who());
    };
    Ok(Caller { uid, pid, bus_name, cgroup: read_cgroup(pid) })
}

struct Service {
    calls: SyncSender<RtInput>,
    timeouts: Timeouts,
}

impl Service {
    async fn run(
        &self,
        conn: &Connection,
        hdr: &Header<'_>,
        method: Method,
        limit: Duration,
    ) -> Result<Reply, BsmError> {
        let deadline = Instant::now() + limit;
        let caller = resolve_caller(conn, hdr, limit).await?;
        let (tx, rx) = async_channel::bounded(1);
        let mut item = RtInput::Call(DbusCall { caller, method, reply: tx });
        loop {
            match self.calls.try_send(item) {
                Ok(()) => break,
                Err(TrySendError::Full(back)) => {
                    if Instant::now() >= deadline {
                        return Err(err(ErrorCode::Timeout, "manager queue is full"));
                    }
                    item = back;
                    Timer::after(Duration::from_millis(5)).await;
                }
                Err(TrySendError::Disconnected(_)) => return Err(err(ErrorCode::Internal, "manager is not running")),
            }
        }
        let left = deadline.saturating_duration_since(Instant::now());
        match with_timeout(left, rx.recv()).await {
            Some(Ok(reply)) => reply,
            Some(Err(_)) => Err(err(ErrorCode::Internal, "manager dropped the call")),
            None => Err(err(ErrorCode::Timeout, "manager did not answer in time")),
        }
    }
}

fn unexpected(r: &Reply) -> SessionError {
    SessionError::Internal(format!("unexpected reply from manager: {r:?}"))
}

fn operation(r: Reply) -> Result<u64, SessionError> {
    match r {
        Reply::Operation(id) => Ok(id),
        other => Err(unexpected(&other)),
    }
}

fn done(r: Reply) -> Result<(), SessionError> {
    match r {
        Reply::Done => Ok(()),
        other => Err(unexpected(&other)),
    }
}

#[allow(clippy::too_many_arguments)]
#[zbus::interface(name = "org.bingux.Session1")]
impl Service {
    async fn get_users(
        &self,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] hdr: Header<'_>,
    ) -> Result<Vec<UserRow>, SessionError> {
        match self.run(conn, &hdr, Method::GetUsers, self.timeouts.call).await? {
            Reply::Users(v) => Ok(v.iter().map(user_row).collect()),
            other => Err(unexpected(&other)),
        }
    }

    async fn get_sessions(
        &self,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] hdr: Header<'_>,
    ) -> Result<Vec<SessionRow>, SessionError> {
        match self.run(conn, &hdr, Method::GetSessions, self.timeouts.call).await? {
            Reply::Sessions(v) => Ok(v.iter().map(session_row).collect()),
            other => Err(unexpected(&other)),
        }
    }

    async fn get_selected_user(
        &self,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] hdr: Header<'_>,
    ) -> Result<(bool, u32), SessionError> {
        match self.run(conn, &hdr, Method::GetSelectedUser, self.timeouts.call).await? {
            Reply::Selected(u) => Ok((u.is_some(), u.unwrap_or(0))),
            other => Err(unexpected(&other)),
        }
    }

    async fn get_foreground_session(
        &self,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] hdr: Header<'_>,
    ) -> Result<(bool, String, u64), SessionError> {
        match self.run(conn, &hdr, Method::GetForegroundSession, self.timeouts.call).await? {
            Reply::Foreground(Some((id, generation))) => Ok((true, id, generation)),
            Reply::Foreground(None) => Ok((false, String::new(), 0)),
            other => Err(unexpected(&other)),
        }
    }

    async fn get_status(
        &self,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] hdr: Header<'_>,
    ) -> Result<HashMap<String, Value<'static>>, SessionError> {
        match self.run(conn, &hdr, Method::GetStatus, self.timeouts.call).await? {
            Reply::Status(s) => Ok(status_dict(&s)?),
            other => Err(unexpected(&other)),
        }
    }

    async fn get_outputs(
        &self,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] hdr: Header<'_>,
    ) -> Result<Vec<OutputRow>, SessionError> {
        match self.run(conn, &hdr, Method::GetOutputs, self.timeouts.call).await? {
            Reply::Outputs(v) => Ok(v.iter().map(output_row).collect()),
            other => Err(unexpected(&other)),
        }
    }

    async fn get_transitions(
        &self,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] hdr: Header<'_>,
    ) -> Result<Vec<TransitionRow>, SessionError> {
        match self.run(conn, &hdr, Method::GetTransitions, self.timeouts.call).await? {
            Reply::Transitions(v) => Ok(v.iter().map(transition_row).collect()),
            other => Err(unexpected(&other)),
        }
    }

    async fn select_greeter_user(
        &self,
        uid: u32,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] hdr: Header<'_>,
    ) -> Result<(), SessionError> {
        done(self.run(conn, &hdr, Method::SelectGreeterUser { uid }, self.timeouts.call).await?)
    }

    async fn request_lock(
        &self,
        session: &str,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] hdr: Header<'_>,
    ) -> Result<u64, SessionError> {
        let session = session_arg(session)?;
        operation(self.run(conn, &hdr, Method::RequestLock { session }, self.timeouts.call).await?)
    }

    async fn request_switch_user(
        &self,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] hdr: Header<'_>,
    ) -> Result<u64, SessionError> {
        operation(self.run(conn, &hdr, Method::RequestSwitchUser, self.timeouts.call).await?)
    }

    async fn request_logout(
        &self,
        session: &str,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] hdr: Header<'_>,
    ) -> Result<u64, SessionError> {
        let session = session_arg(session)?;
        operation(self.run(conn, &hdr, Method::RequestLogout { session }, self.timeouts.call).await?)
    }

    async fn request_suspend(
        &self,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] hdr: Header<'_>,
    ) -> Result<u64, SessionError> {
        operation(self.run(conn, &hdr, Method::RequestSuspend, self.timeouts.call).await?)
    }

    async fn request_reboot(
        &self,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] hdr: Header<'_>,
    ) -> Result<u64, SessionError> {
        operation(self.run(conn, &hdr, Method::RequestReboot, self.timeouts.call).await?)
    }

    async fn request_power_off(
        &self,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] hdr: Header<'_>,
    ) -> Result<u64, SessionError> {
        operation(self.run(conn, &hdr, Method::RequestPowerOff, self.timeouts.call).await?)
    }

    async fn cancel_operation(
        &self,
        operation_id: u64,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] hdr: Header<'_>,
    ) -> Result<(), SessionError> {
        done(self.run(conn, &hdr, Method::CancelOperation { operation: operation_id }, self.timeouts.call).await?)
    }

    #[allow(clippy::too_many_arguments)]
    async fn publish_appearance(
        &self,
        color_scheme: String,
        accent: String,
        clock_format: String,
        reduce_motion: bool,
        show_personal_images: bool,
        wallpaper_fd: Vec<zvariant::OwnedFd>,
        avatar_fd: Vec<zvariant::OwnedFd>,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] hdr: Header<'_>,
    ) -> Result<(), SessionError> {
        let request = build_publish(
            PublishArgs { color_scheme, accent, clock_format, reduce_motion, show_personal_images },
            wallpaper_fd,
            avatar_fd,
        )?;
        done(self.run(conn, &hdr, Method::PublishAppearance(request), self.timeouts.publish).await?)
    }

    #[zbus(signal)]
    async fn sessions_changed(emitter: &SignalEmitter<'_>, revision: u64) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn selected_user_changed(emitter: &SignalEmitter<'_>, revision: u64, has: bool, uid: u32)
    -> zbus::Result<()>;

    #[zbus(signal)]
    async fn appearance_changed(emitter: &SignalEmitter<'_>, uid: u32, revision: u64) -> zbus::Result<()>;

    #[zbus(signal)]
    #[allow(clippy::too_many_arguments)]
    async fn session_launch_progress(
        emitter: &SignalEmitter<'_>,
        session: &str,
        generation: u64,
        revision: u64,
        reached: Vec<String>,
        counted_stage: &str,
        done: u32,
        total: u32,
        fraction: u32,
        label: &str,
        failure_stage: &str,
        failure_code: &str,
        failure_message: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn session_readiness_changed(
        emitter: &SignalEmitter<'_>,
        session: &str,
        generation: u64,
        ready: bool,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn foreground_changed(
        emitter: &SignalEmitter<'_>,
        has: bool,
        session: &str,
        generation: u64,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn lock_state_changed(emitter: &SignalEmitter<'_>, session: &str, locked: bool) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn transition_changed(
        emitter: &SignalEmitter<'_>,
        operation_id: u64,
        kind: &str,
        phase: &str,
        state: &str,
        reason: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn operation_finished(
        emitter: &SignalEmitter<'_>,
        operation_id: u64,
        success: bool,
        reason: &str,
    ) -> zbus::Result<()>;
}

/// The introspection of the interface, kept next to the code so that a change of either
/// is a deliberate act: a test compares it with what the live service reports.
pub const INTROSPECTION_XML: &str = r#"<interface name="org.bingux.Session1">
  <method name="GetUsers">
    <arg type="a(usssstt)" direction="out"/>
  </method>
  <method name="GetSessions">
    <arg type="a(stussbbu)" direction="out"/>
  </method>
  <method name="GetSelectedUser">
    <arg type="b" direction="out"/>
    <arg type="u" direction="out"/>
  </method>
  <method name="GetForegroundSession">
    <arg type="b" direction="out"/>
    <arg type="s" direction="out"/>
    <arg type="t" direction="out"/>
  </method>
  <method name="GetStatus">
    <arg type="a{sv}" direction="out"/>
  </method>
  <method name="GetOutputs">
    <arg type="a(siiuuuuu)" direction="out"/>
  </method>
  <method name="GetTransitions">
    <arg type="a(tsssss)" direction="out"/>
  </method>
  <method name="SelectGreeterUser">
    <arg name="uid" type="u" direction="in"/>
  </method>
  <method name="RequestLock">
    <arg name="session" type="s" direction="in"/>
    <arg type="t" direction="out"/>
  </method>
  <method name="RequestSwitchUser">
    <arg type="t" direction="out"/>
  </method>
  <method name="RequestLogout">
    <arg name="session" type="s" direction="in"/>
    <arg type="t" direction="out"/>
  </method>
  <method name="RequestSuspend">
    <arg type="t" direction="out"/>
  </method>
  <method name="RequestReboot">
    <arg type="t" direction="out"/>
  </method>
  <method name="RequestPowerOff">
    <arg type="t" direction="out"/>
  </method>
  <method name="CancelOperation">
    <arg name="operation_id" type="t" direction="in"/>
  </method>
  <method name="PublishAppearance">
    <arg name="color_scheme" type="s" direction="in"/>
    <arg name="accent" type="s" direction="in"/>
    <arg name="clock_format" type="s" direction="in"/>
    <arg name="reduce_motion" type="b" direction="in"/>
    <arg name="show_personal_images" type="b" direction="in"/>
    <arg name="wallpaper_fd" type="ah" direction="in"/>
    <arg name="avatar_fd" type="ah" direction="in"/>
  </method>
  <signal name="SessionsChanged">
    <arg name="revision" type="t"/>
  </signal>
  <signal name="SelectedUserChanged">
    <arg name="revision" type="t"/>
    <arg name="has" type="b"/>
    <arg name="uid" type="u"/>
  </signal>
  <signal name="AppearanceChanged">
    <arg name="uid" type="u"/>
    <arg name="revision" type="t"/>
  </signal>
  <signal name="SessionLaunchProgress">
    <arg name="session" type="s"/>
    <arg name="generation" type="t"/>
    <arg name="revision" type="t"/>
    <arg name="reached" type="as"/>
    <arg name="counted_stage" type="s"/>
    <arg name="done" type="u"/>
    <arg name="total" type="u"/>
    <arg name="fraction" type="u"/>
    <arg name="label" type="s"/>
    <arg name="failure_stage" type="s"/>
    <arg name="failure_code" type="s"/>
    <arg name="failure_message" type="s"/>
  </signal>
  <signal name="SessionReadinessChanged">
    <arg name="session" type="s"/>
    <arg name="generation" type="t"/>
    <arg name="ready" type="b"/>
  </signal>
  <signal name="ForegroundChanged">
    <arg name="has" type="b"/>
    <arg name="session" type="s"/>
    <arg name="generation" type="t"/>
  </signal>
  <signal name="LockStateChanged">
    <arg name="session" type="s"/>
    <arg name="locked" type="b"/>
  </signal>
  <signal name="TransitionChanged">
    <arg name="operation_id" type="t"/>
    <arg name="kind" type="s"/>
    <arg name="phase" type="s"/>
    <arg name="state" type="s"/>
    <arg name="reason" type="s"/>
  </signal>
  <signal name="OperationFinished">
    <arg name="operation_id" type="t"/>
    <arg name="success" type="b"/>
    <arg name="reason" type="s"/>
  </signal>
</interface>"#;

async fn emit_signal(emitter: &SignalEmitter<'_>, sig: &DbusSignal) -> zbus::Result<()> {
    let code = |c: &Option<ErrorCode>| c.map(|c| c.as_str()).unwrap_or("");
    match sig {
        DbusSignal::SessionsChanged { revision } => Service::sessions_changed(emitter, *revision).await,
        DbusSignal::SelectedUserChanged { revision, uid } => {
            Service::selected_user_changed(emitter, *revision, uid.is_some(), uid.unwrap_or(0)).await
        }
        DbusSignal::AppearanceChanged { uid, revision } => Service::appearance_changed(emitter, *uid, *revision).await,
        DbusSignal::SessionLaunchProgress { model } => emit_progress(emitter, model).await,
        DbusSignal::SessionReadinessChanged { session, generation, ready } => {
            Service::session_readiness_changed(emitter, session, *generation, *ready).await
        }
        DbusSignal::ForegroundChanged { session, generation } => {
            Service::foreground_changed(
                emitter,
                session.is_some(),
                session.as_deref().unwrap_or(""),
                generation.unwrap_or(0),
            )
            .await
        }
        DbusSignal::LockStateChanged { session, locked } => Service::lock_state_changed(emitter, session, *locked).await,
        DbusSignal::TransitionChanged { operation_id, kind, phase, state, reason } => {
            Service::transition_changed(
                emitter,
                *operation_id,
                &name_of(kind),
                &name_of(phase),
                &name_of(state),
                code(reason),
            )
            .await
        }
        DbusSignal::OperationFinished { operation_id, success, reason } => {
            Service::operation_finished(emitter, *operation_id, *success, code(reason)).await
        }
    }
}

async fn emit_progress(emitter: &SignalEmitter<'_>, m: &ProgressModel) -> zbus::Result<()> {
    let (fs, fc, fm) = match &m.failure {
        Some(f) => (name_of(&f.stage), f.code.as_str().to_owned(), f.message.clone()),
        None => (String::new(), String::new(), String::new()),
    };
    Service::session_launch_progress(
        emitter,
        &m.session,
        m.generation,
        m.revision,
        m.reached.iter().map(name_of).collect(),
        &m.counted_stage.map(|s| name_of(&s)).unwrap_or_default(),
        m.done,
        m.total,
        m.fraction(),
        m.label(),
        &fs,
        &fc,
        &fm,
    )
    .await
}

// ---------------------------------------------------------------------------------------
// The running service.

struct Shared {
    tx: async_channel::Sender<DbusSignal>,
    dropped: AtomicU64,
    emitted: AtomicU64,
    failed: AtomicU64,
    owned: AtomicBool,
}

/// Handle to the running service. Dropping it stops the service thread and releases the
/// bus name.
pub struct DbusHandle {
    shared: Arc<Shared>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl DbusHandle {
    /// Queue a signal for emission. Never blocks. When the queue is full the signal is
    /// dropped and counted.
    pub fn emit(&self, signal: DbusSignal) {
        if self.shared.tx.try_send(signal).is_err() {
            self.shared.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// True while the well-known name is owned and the bus connection works.
    pub fn name_acquired(&self) -> bool {
        self.shared.owned.load(Ordering::Acquire)
    }

    /// Signals dropped because the queue was full (or the service had stopped).
    pub fn dropped_signals(&self) -> u64 {
        self.shared.dropped.load(Ordering::Relaxed)
    }

    /// Signals handed to the bus.
    pub fn emitted_signals(&self) -> u64 {
        self.shared.emitted.load(Ordering::Relaxed)
    }

    /// Signals the bus connection refused (connection broken).
    pub fn failed_signals(&self) -> u64 {
        self.shared.failed.load(Ordering::Relaxed)
    }
}

impl Drop for DbusHandle {
    fn drop(&mut self) {
        self.shared.tx.close();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        self.shared.owned.store(false, Ordering::Release);
    }
}

/// Start the service with default timeouts and queue size.
pub fn spawn(kind: BusKind, calls: SyncSender<RtInput>) -> Result<DbusHandle, BsmError> {
    spawn_with(kind, calls, Config::default())
}

/// Start the service. Returns once the name `org.bingux.Session1` is owned; fails when
/// the bus is unreachable or the name is taken.
pub fn spawn_with(kind: BusKind, calls: SyncSender<RtInput>, config: Config) -> Result<DbusHandle, BsmError> {
    let (tx, rx) = async_channel::bounded(config.signal_queue.max(1));
    let shared = Arc::new(Shared {
        tx,
        dropped: AtomicU64::new(0),
        emitted: AtomicU64::new(0),
        failed: AtomicU64::new(0),
        owned: AtomicBool::new(false),
    });
    let (ready_tx, ready_rx) = sync_channel::<Result<(), BsmError>>(1);
    let worker = shared.clone();
    let thread = std::thread::Builder::new()
        .name("bsm-dbus".into())
        .spawn(move || {
            async_io::block_on(serve(kind, calls, config.timeouts, worker, rx, ready_tx));
        })
        .map_err(|e| err(ErrorCode::Internal, format!("cannot start the D-Bus thread: {e}")))?;
    let handle = DbusHandle { shared, thread: Some(thread) };
    match ready_rx.recv_timeout(START_TIMEOUT) {
        Ok(Ok(())) => Ok(handle),
        Ok(Err(e)) => Err(e),
        Err(_) => Err(err(ErrorCode::Timeout, "the D-Bus service did not start in time")),
    }
}

async fn serve(
    kind: BusKind,
    calls: SyncSender<RtInput>,
    timeouts: Timeouts,
    shared: Arc<Shared>,
    rx: async_channel::Receiver<DbusSignal>,
    ready: SyncSender<Result<(), BsmError>>,
) {
    let conn = match connect_service(&kind, calls, timeouts).await {
        Ok(c) => c,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    let emitter = match SignalEmitter::new(&conn, BUS_PATH) {
        Ok(e) => e,
        Err(e) => {
            let _ = ready.send(Err(err(ErrorCode::Internal, format!("signal emitter: {e}"))));
            return;
        }
    };
    shared.owned.store(true, Ordering::Release);
    let _ = ready.send(Ok(()));
    while let Ok(sig) = rx.recv().await {
        match emit_signal(&emitter, &sig).await {
            Ok(()) => {
                shared.emitted.fetch_add(1, Ordering::Relaxed);
            }
            Err(e) => {
                shared.failed.fetch_add(1, Ordering::Relaxed);
                tracing::warn!("cannot emit D-Bus signal: {e}");
                if matches!(e, zbus::Error::InputOutput(_)) {
                    shared.owned.store(false, Ordering::Release);
                }
            }
        }
    }
}

async fn connect_service(kind: &BusKind, calls: SyncSender<RtInput>, timeouts: Timeouts) -> Result<Connection, BsmError> {
    use zbus::fdo::{RequestNameFlags, RequestNameReply};
    let bus = |e: zbus::Error| err(ErrorCode::Internal, format!("D-Bus: {e}"));
    let conn = builder(kind)
        .map_err(bus)?
        .serve_at(BUS_PATH, Service { calls, timeouts })
        .map_err(bus)?
        .build()
        .await
        .map_err(bus)?;
    // The builder's own `name()` would queue behind a current owner and report success;
    // ask without queueing so that a second instance fails instead of idling silently.
    match conn.request_name_with_flags(BUS_NAME, RequestNameFlags::DoNotQueue.into()).await {
        Ok(RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner) => Ok(conn),
        Ok(other) => Err(err(ErrorCode::Internal, format!("D-Bus name {BUS_NAME} not acquired: {other:?}"))),
        Err(zbus::Error::NameTaken) => Err(err(ErrorCode::Internal, format!("D-Bus name {BUS_NAME} is already owned"))),
        Err(e) => Err(bus(e)),
    }
}

#[cfg(test)]
pub(crate) mod testutil {
    //! A private `dbus-daemon --session` and a scripted driver, shared by the tests of
    //! `dbus`, `client` and `system`.

    use std::io::{BufRead, BufReader};
    use std::process::{Child, Command, Stdio};
    use std::sync::mpsc::{Receiver, sync_channel};
    use std::sync::{Arc, Mutex};

    use super::*;

    pub struct PrivateBus {
        child: Child,
        pub address: String,
        _dir: tempfile::TempDir,
    }

    impl PrivateBus {
        pub fn start() -> PrivateBus {
            let dir = tempfile::tempdir().expect("tempdir");
            let socket = dir.path().join("bus");
            let config = dir.path().join("bus.conf");
            std::fs::write(
                &config,
                format!(
                    "<!DOCTYPE busconfig PUBLIC \"-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN\" \
                     \"http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd\">\n\
                     <busconfig><type>session</type><keep_umask/>\n\
                     <listen>unix:path={}</listen><auth>EXTERNAL</auth>\n\
                     <policy context=\"default\"><allow send_destination=\"*\" eavesdrop=\"true\"/>\
                     <allow eavesdrop=\"true\"/><allow own=\"*\"/></policy></busconfig>\n",
                    socket.display()
                ),
            )
            .expect("write bus config");
            let mut child = Command::new("dbus-daemon")
                .arg("--config-file")
                .arg(&config)
                .arg("--print-address")
                .arg("--nofork")
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .expect("dbus-daemon must be installed for these tests");
            let mut line = String::new();
            BufReader::new(child.stdout.take().expect("stdout")).read_line(&mut line).expect("address line");
            let address = line.trim().to_owned();
            assert!(address.starts_with("unix:"), "unexpected bus address {address:?}");
            PrivateBus { child, address, _dir: dir }
        }

        pub fn kind(&self) -> BusKind {
            BusKind::Address(self.address.clone())
        }
    }

    impl Drop for PrivateBus {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    /// What the scripted driver does with a call: answer, or hold it unanswered.
    pub type Script = Box<dyn Fn(&Method) -> Option<Result<Reply, BsmError>> + Send>;

    pub struct Driver {
        pub seen: Arc<Mutex<Vec<(Caller, Method)>>>,
        _thread: std::thread::JoinHandle<()>,
    }

    /// Start a driver thread answering every `RtInput::Call` per `script`. Returns the
    /// sender to give to the service.
    pub fn driver(script: Script) -> (SyncSender<RtInput>, Driver) {
        let (tx, rx): (SyncSender<RtInput>, Receiver<RtInput>) = sync_channel(16);
        let seen: Arc<Mutex<Vec<(Caller, Method)>>> = Arc::default();
        let log = seen.clone();
        let thread = std::thread::spawn(move || {
            let mut held = Vec::new();
            while let Ok(input) = rx.recv() {
                if let RtInput::Call(call) = input {
                    log.lock().unwrap().push((call.caller.clone(), call.method.clone()));
                    match script(&call.method) {
                        Some(result) => {
                            let _ = call.reply.send_blocking(result);
                        }
                        None => held.push(call),
                    }
                }
            }
        });
        (tx, Driver { seen, _thread: thread })
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::*;
    use bsm_proto::paths::BUS_INTERFACE;
    use super::*;
    use bsm_proto::progress::{ProgressFailure, Stage};
    use bsm_proto::state::{OperationPhase, SeatState, SessionAvailability, SessionState, TransitionKind, TransitionState};
    use futures_lite::StreamExt;
    use std::fs::File;
    use std::io::Write;

    fn all_codes_are_listed(c: ErrorCode) {
        // Adding a variant breaks this match; add it to ALL_CODES and SessionError too.
        match c {
            ErrorCode::Unauthorised
            | ErrorCode::InvalidAccount
            | ErrorCode::NoSession
            | ErrorCode::AlreadyLocked
            | ErrorCode::Busy
            | ErrorCode::StaleGeneration
            | ErrorCode::Cancelled
            | ErrorCode::PamFailure
            | ErrorCode::LogindFailure
            | ErrorCode::OutputNotReady
            | ErrorCode::TransitionInterrupted
            | ErrorCode::Inhibited
            | ErrorCode::ShutdownRejected
            | ErrorCode::Timeout
            | ErrorCode::HelperFailure
            | ErrorCode::InvalidArgument
            | ErrorCode::NotSupported
            | ErrorCode::Internal => {}
        }
    }

    #[test]
    fn error_names_match_the_derive_and_round_trip() {
        use zbus::DBusError;
        for code in ALL_CODES {
            all_codes_are_listed(code);
            let e = SessionError::from(BsmError::new(code, "x"));
            assert_eq!(e.name().as_str(), error_name(code), "{code:?}");
            assert_eq!(code_from_error_name(&error_name(code)), Some(code));
        }
        assert_eq!(error_name(ErrorCode::Unauthorised), "org.bingux.Session1.Error.Unauthorised");
        assert_eq!(error_name(ErrorCode::StaleGeneration), "org.bingux.Session1.Error.StaleGeneration");
        assert_eq!(code_from_error_name("org.bingux.Session1.Error.Nope"), None);
        assert_eq!(code_from_error_name("org.freedesktop.DBus.Error.Failed"), None);
    }

    #[test]
    fn enum_names_are_serde_snake_case() {
        assert_eq!(name_of(&Stage::AuthAccepted), "auth_accepted");
        assert_eq!(name_of(&SessionAvailability::RunningLocked), "running_locked");
        assert_eq!(parse_name::<SessionState>("lock_requested"), Some(SessionState::LockRequested));
        assert_eq!(parse_name::<SessionState>("nope"), None);
    }

    fn pipe() -> (OwnedFd, OwnedFd) {
        let mut fds = [0; 2];
        // SAFETY: valid two-element array.
        assert_eq!(unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) }, 0);
        // SAFETY: both descriptors are fresh.
        unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) }
    }

    #[test]
    fn fd_validation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("img");
        std::fs::write(&path, b"png").unwrap();

        let ro: OwnedFd = File::open(&path).unwrap().into();
        assert!(validate_image_fd("t", &ro).is_ok());

        let wo: OwnedFd = File::create(dir.path().join("w")).unwrap().into();
        assert_eq!(validate_image_fd("t", &wo).unwrap_err().code, ErrorCode::InvalidArgument);

        let rw: OwnedFd =
            std::fs::OpenOptions::new().read(true).write(true).open(&path).unwrap().into();
        assert_eq!(validate_image_fd("t", &rw).unwrap_err().code, ErrorCode::InvalidArgument);

        let (r, _w) = pipe();
        assert_eq!(validate_image_fd("t", &r).unwrap_err().code, ErrorCode::InvalidArgument);

        let dirfd: OwnedFd = File::open(dir.path()).unwrap().into();
        assert_eq!(validate_image_fd("t", &dirfd).unwrap_err().code, ErrorCode::InvalidArgument);

        let dev: OwnedFd = File::open("/dev/null").unwrap().into();
        assert_eq!(validate_image_fd("t", &dev).unwrap_err().code, ErrorCode::InvalidArgument);

        // O_PATH descriptor.
        let c = std::ffi::CString::new(path.to_str().unwrap()).unwrap();
        // SAFETY: valid C string.
        let p = unsafe { libc::open(c.as_ptr(), libc::O_PATH | libc::O_CLOEXEC) };
        assert!(p >= 0);
        // SAFETY: fresh descriptor.
        let p = unsafe { OwnedFd::from_raw_fd(p) };
        assert_eq!(validate_image_fd("t", &p).unwrap_err().code, ErrorCode::InvalidArgument);

        // Too large: a sparse file one byte over the cap.
        let big = dir.path().join("big");
        let f = File::create(&big).unwrap();
        f.set_len(MAX_IMAGE_BYTES + 1).unwrap();
        drop(f);
        let big: OwnedFd = File::open(&big).unwrap().into();
        assert_eq!(validate_image_fd("t", &big).unwrap_err().code, ErrorCode::InvalidArgument);
        let exact = dir.path().join("exact");
        File::create(&exact).unwrap().set_len(MAX_IMAGE_BYTES).unwrap();
        let exact: OwnedFd = File::open(&exact).unwrap().into();
        assert!(validate_image_fd("t", &exact).is_ok());
    }

    #[test]
    fn signal_queue_drops_when_full_and_never_blocks() {
        // No consumer: a handle whose queue nobody drains.
        let (tx, _rx) = async_channel::bounded(2);
        let shared = Arc::new(Shared {
            tx,
            dropped: AtomicU64::new(0),
            emitted: AtomicU64::new(0),
            failed: AtomicU64::new(0),
            owned: AtomicBool::new(true),
        });
        let h = DbusHandle { shared, thread: None };
        let start = Instant::now();
        for r in 0..1000 {
            h.emit(DbusSignal::SessionsChanged { revision: r });
        }
        assert!(start.elapsed() < Duration::from_secs(1));
        assert_eq!(h.dropped_signals(), 998);
    }

    fn sample_status() -> StatusInfo {
        StatusInfo {
            seat: SeatState::Greeter,
            selected_uid: Some(1000),
            last_used_uid: None,
            foreground: Some("s0000000a".into()),
            revision: 9,
            operation: Some(OperationInfo {
                id: 4,
                kind: TransitionKind::Lock,
                phase: OperationPhase::Preparing,
                state: TransitionState::WaitingForFrames,
                reason: None,
                session: Some("s0000000a".into()),
            }),
            greeter_connected: true,
            helper_connected: false,
            outputs: 2,
        }
    }

    fn sample_script() -> Script {
        Box::new(|m| {
            Some(match m {
                Method::GetUsers => Ok(Reply::Users(vec![
                    UserInfo {
                        uid: 1000,
                        user_name: "ada".into(),
                        display_name: "Ada L".into(),
                        availability: SessionAvailability::RunningLocked,
                        session: Some("s0000000a".into()),
                        avatar_rev: 3,
                        wallpaper_rev: 4,
                    },
                    UserInfo {
                        uid: 1001,
                        user_name: "bob".into(),
                        display_name: "Bob".into(),
                        availability: SessionAvailability::NotSignedIn,
                        session: None,
                        avatar_rev: 0,
                        wallpaper_rev: 0,
                    },
                ])),
                Method::GetSessions => Ok(Reply::Sessions(vec![SessionInfo {
                    id: "s0000000a".into(),
                    generation: 77,
                    uid: 1000,
                    state: SessionState::LockedBackground,
                    availability: SessionAvailability::RunningLocked,
                    foreground: false,
                    locked: true,
                    progress: 100,
                }])),
                Method::GetSelectedUser => Ok(Reply::Selected(Some(1000))),
                Method::GetForegroundSession => Ok(Reply::Foreground(Some(("s0000000a".into(), 77)))),
                Method::GetStatus => Ok(Reply::Status(sample_status())),
                Method::GetOutputs => Ok(Reply::Outputs(vec![OutputInfo {
                    name: "Virtual-1".into(),
                    x: -10,
                    y: 20,
                    width: 1920,
                    height: 1080,
                    scale_120: 150,
                    refresh_mhz: 60000,
                    transform: 0,
                }])),
                Method::GetTransitions => Ok(Reply::Transitions(vec![
                    OperationInfo {
                        id: 4,
                        kind: TransitionKind::Lock,
                        phase: OperationPhase::Failed,
                        state: TransitionState::Failed,
                        reason: Some(ErrorCode::Inhibited),
                        session: None,
                    },
                ])),
                Method::SelectGreeterUser { uid: 7 } => Err(BsmError::new(ErrorCode::InvalidAccount, "no such account")),
                Method::SelectGreeterUser { .. } => Ok(Reply::Done),
                Method::RequestLock { session: Some(s) } if s == "s0000000a" => Ok(Reply::Operation(11)),
                Method::RequestLock { session: None } => Ok(Reply::Operation(12)),
                Method::RequestLock { .. } => Err(BsmError::new(ErrorCode::NoSession, "unknown session")),
                Method::RequestSwitchUser => Ok(Reply::Operation(13)),
                Method::RequestLogout { session } => {
                    assert!(session.is_none());
                    Err(BsmError::new(ErrorCode::Unauthorised, "not yours"))
                }
                Method::RequestSuspend => Ok(Reply::Operation(14)),
                Method::RequestReboot => Ok(Reply::Operation(15)),
                Method::RequestPowerOff => Err(BsmError::new(ErrorCode::StaleGeneration, "stale")),
                Method::CancelOperation { operation } => {
                    assert_eq!(*operation, 99);
                    Ok(Reply::Done)
                }
                Method::PublishAppearance(r) => {
                    assert_eq!(r.appearance.accent, "#112233");
                    assert!(r.wallpaper.is_some());
                    assert!(r.avatar.is_none());
                    assert!(r.show_personal_images);
                    Ok(Reply::Done)
                }
            })
        })
    }

    struct Rig {
        _bus: PrivateBus,
        handle: DbusHandle,
        driver: Driver,
        conn: zbus::Connection,
    }

    fn rig(script: Script, timeouts: Timeouts) -> Rig {
        let bus = PrivateBus::start();
        let (tx, driver) = driver(script);
        let handle = spawn_with(bus.kind(), tx, Config { timeouts, signal_queue: 64 }).expect("spawn");
        let conn = async_io::block_on(async {
            super::builder(&bus.kind()).unwrap().build().await.unwrap()
        });
        Rig { _bus: bus, handle, driver, conn }
    }

    fn call<B, R>(conn: &zbus::Connection, member: &str, body: &B) -> Result<R, zbus::Error>
    where
        B: Serialize + zvariant::DynamicType,
        R: DeserializeOwned + zvariant::Type,
    {
        async_io::block_on(async {
            let m = conn.call_method(Some(BUS_NAME), BUS_PATH, Some(BUS_INTERFACE), member, body).await?;
            Ok(m.body().deserialize::<R>()?)
        })
    }

    fn error_name_of(e: &zbus::Error) -> String {
        match e {
            zbus::Error::MethodError(n, _, _) => n.to_string(),
            other => panic!("not a method error: {other}"),
        }
    }

    #[test]
    fn introspection_matches_constant() {
        let r = rig(sample_script(), Timeouts::default());
        let xml: String = async_io::block_on(async {
            let m = r
                .conn
                .call_method(Some(BUS_NAME), BUS_PATH, Some("org.freedesktop.DBus.Introspectable"), "Introspect", &())
                .await
                .unwrap();
            m.body().deserialize::<String>().unwrap()
        });
        let live = interface_block(&xml);
        let expect = interface_block(INTROSPECTION_XML);
        assert_eq!(live, expect, "live introspection:\n{live}");
        assert_eq!(BUS_INTERFACE, "org.bingux.Session1");
        assert_eq!(BUS_NAME, "org.bingux.Session1");
        assert_eq!(BUS_PATH, "/org/bingux/Session1");
    }

    fn interface_block(xml: &str) -> String {
        let mut out = Vec::new();
        let mut inside = false;
        for line in xml.lines() {
            let t = line.trim();
            if t.starts_with("<interface name=\"org.bingux.Session1\"") {
                inside = true;
            }
            if inside {
                out.push(t.to_owned());
            }
            if inside && t == "</interface>" {
                break;
            }
        }
        out.join("\n")
    }

    #[test]
    fn methods_return_typed_results_and_the_true_caller() {
        let r = rig(sample_script(), Timeouts::default());
        let c = &r.conn;

        let users: Vec<UserRow> = call(c, "GetUsers", &()).unwrap();
        assert_eq!(users[0], (1000, "ada".into(), "Ada L".into(), "running_locked".into(), "s0000000a".into(), 3, 4));
        assert_eq!(users[1].4, "");
        let sessions: Vec<SessionRow> = call(c, "GetSessions", &()).unwrap();
        assert_eq!(
            sessions,
            vec![("s0000000a".into(), 77, 1000, "locked_background".into(), "running_locked".into(), false, true, 100)]
        );
        assert_eq!(call::<_, (bool, u32)>(c, "GetSelectedUser", &()).unwrap(), (true, 1000));
        assert_eq!(call::<_, (bool, String, u64)>(c, "GetForegroundSession", &()).unwrap(), (true, "s0000000a".into(), 77));
        let outputs: Vec<OutputRow> = call(c, "GetOutputs", &()).unwrap();
        assert_eq!(outputs, vec![("Virtual-1".into(), -10, 20, 1920, 1080, 150, 60000, 0)]);
        let tr: Vec<TransitionRow> = call(c, "GetTransitions", &()).unwrap();
        assert_eq!(tr, vec![(4, "lock".into(), "failed".into(), "failed".into(), "inhibited".into(), "".into())]);

        let status: HashMap<String, zvariant::OwnedValue> = call(c, "GetStatus", &()).unwrap();
        assert_eq!(status["seat"].downcast_ref::<String>().unwrap(), "greeter");
        assert_eq!(status["selected_uid"].downcast_ref::<u32>().unwrap(), 1000);
        assert!(!status.contains_key("last_used_uid"));
        assert_eq!(status["foreground"].downcast_ref::<String>().unwrap(), "s0000000a");
        assert_eq!(status["revision"].downcast_ref::<u64>().unwrap(), 9);
        assert!(status["greeter_connected"].downcast_ref::<bool>().unwrap());
        assert!(!status["helper_connected"].downcast_ref::<bool>().unwrap());
        assert_eq!(status["outputs"].downcast_ref::<u32>().unwrap(), 2);
        assert!(matches!(&*status["operation"], Value::Structure(_)));

        call::<_, ()>(c, "SelectGreeterUser", &(1000u32,)).unwrap();
        assert_eq!(call::<_, u64>(c, "RequestLock", &("s0000000a",)).unwrap(), 11);
        assert_eq!(call::<_, u64>(c, "RequestLock", &("",)).unwrap(), 12);
        assert_eq!(call::<_, u64>(c, "RequestSwitchUser", &()).unwrap(), 13);
        assert_eq!(call::<_, u64>(c, "RequestSuspend", &()).unwrap(), 14);
        assert_eq!(call::<_, u64>(c, "RequestReboot", &()).unwrap(), 15);
        call::<_, ()>(c, "CancelOperation", &(99u64,)).unwrap();

        // Every call carries the identity of this process, resolved from the bus.
        let seen = r.driver.seen.lock().unwrap();
        assert!(!seen.is_empty());
        let my_cgroup = read_cgroup(std::process::id());
        for (caller, _) in seen.iter() {
            // SAFETY: getuid has no preconditions.
            assert_eq!(caller.uid, unsafe { libc::getuid() });
            assert_eq!(caller.pid, std::process::id());
            assert!(caller.bus_name.starts_with(':'));
            assert_eq!(caller.cgroup, my_cgroup);
        }
        if std::fs::read_to_string("/proc/self/cgroup").is_ok_and(|t| t.lines().any(|l| l.starts_with("0::"))) {
            assert!(seen[0].0.cgroup.is_some());
        }
    }

    #[test]
    fn errors_carry_stable_names_and_messages() {
        let r = rig(sample_script(), Timeouts::default());
        let c = &r.conn;
        let e = call::<_, ()>(c, "SelectGreeterUser", &(7u32,)).unwrap_err();
        assert_eq!(error_name_of(&e), "org.bingux.Session1.Error.InvalidAccount");
        assert!(e.to_string().contains("no such account"), "{e}");
        let e = call::<_, u64>(c, "RequestLogout", &("",)).unwrap_err();
        assert_eq!(error_name_of(&e), "org.bingux.Session1.Error.Unauthorised");
        let e = call::<_, u64>(c, "RequestPowerOff", &()).unwrap_err();
        assert_eq!(error_name_of(&e), "org.bingux.Session1.Error.StaleGeneration");
        let e = call::<_, u64>(c, "RequestLock", &("nope",)).unwrap_err();
        assert_eq!(error_name_of(&e), "org.bingux.Session1.Error.NoSession");
        // Malformed session ids never reach the core.
        let e = call::<_, u64>(c, "RequestLock", &("../../etc",)).unwrap_err();
        assert_eq!(error_name_of(&e), "org.bingux.Session1.Error.InvalidArgument");
        let n = r.driver.seen.lock().unwrap().len();
        let e = call::<_, u64>(c, "RequestLock", &("x".repeat(65),)).unwrap_err();
        assert_eq!(error_name_of(&e), "org.bingux.Session1.Error.InvalidArgument");
        assert_eq!(r.driver.seen.lock().unwrap().len(), n);
    }

    #[test]
    fn unanswered_call_times_out_within_the_bound() {
        let r = rig(
            Box::new(|_| None),
            Timeouts { call: Duration::from_millis(300), publish: Duration::from_millis(600) },
        );
        let start = Instant::now();
        let e = call::<_, u64>(&r.conn, "RequestReboot", &()).unwrap_err();
        let took = start.elapsed();
        assert_eq!(error_name_of(&e), "org.bingux.Session1.Error.Timeout");
        assert!(took >= Duration::from_millis(250) && took < Duration::from_secs(3), "{took:?}");
        let start = Instant::now();
        let e = call::<_, ()>(
            &r.conn,
            "PublishAppearance",
            &("dark", "#112233", "24h", false, true, Vec::<zvariant::Fd>::new(), Vec::<zvariant::Fd>::new()),
        )
        .unwrap_err();
        assert_eq!(error_name_of(&e), "org.bingux.Session1.Error.Timeout");
        assert!(start.elapsed() >= Duration::from_millis(550), "publish uses its own bound");
    }

    #[test]
    fn publish_appearance_validates_arguments_and_descriptors() {
        let r = rig(sample_script(), Timeouts::default());
        let c = &r.conn;
        let dir = tempfile::tempdir().unwrap();
        let img = dir.path().join("wall.png");
        let mut f = File::create(&img).unwrap();
        f.write_all(b"png").unwrap();
        drop(f);
        let ro = File::open(&img).unwrap();
        let wo = File::create(dir.path().join("w")).unwrap();
        let (pr, _pw) = pipe();
        let none: Vec<zvariant::Fd> = Vec::new();

        let publish = |scheme: &str, accent: &str, clock: &str, wall: Vec<zvariant::Fd>, avatar: Vec<zvariant::Fd>| {
            call::<_, ()>(c, "PublishAppearance", &(scheme, accent, clock, true, true, wall, avatar))
        };
        let invalid = |e: zbus::Error| assert_eq!(error_name_of(&e), "org.bingux.Session1.Error.InvalidArgument", "{e}");

        publish("dark", "#112233", "24h", vec![zvariant::Fd::from(&ro)], none).unwrap();
        invalid(publish("purple", "#112233", "24h", vec![], vec![]).unwrap_err());
        invalid(publish("dark", "#112233", "13h", vec![], vec![]).unwrap_err());
        invalid(publish("dark", &"#".repeat(40), "24h", vec![], vec![]).unwrap_err());
        invalid(publish("dark", "#112233", "24h", vec![zvariant::Fd::from(&wo)], vec![]).unwrap_err());
        invalid(publish("dark", "#112233", "24h", vec![], vec![zvariant::Fd::from(&wo)]).unwrap_err());
        let pr_file = File::from(pr);
        invalid(publish("dark", "#112233", "24h", vec![zvariant::Fd::from(&pr_file)], vec![]).unwrap_err());
        invalid(
            publish("dark", "#112233", "24h", vec![zvariant::Fd::from(&ro), zvariant::Fd::from(&ro)], vec![])
                .unwrap_err(),
        );
        // Only the one good call reached the driver.
        assert_eq!(r.driver.seen.lock().unwrap().len(), 1);
    }

    #[test]
    fn signals_are_delivered_with_typed_bodies() {
        let r = rig(sample_script(), Timeouts::default());
        let rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .interface(BUS_INTERFACE)
            .unwrap()
            .path(BUS_PATH)
            .unwrap()
            .build();
        let mut stream = async_io::block_on(zbus::MessageStream::for_match_rule(rule, &r.conn, Some(64))).unwrap();

        let mut model = ProgressModel::new("s0000000a".into(), 77);
        model.reach(Stage::AuthAccepted);
        model.reach(Stage::PamSession);
        model.reach(Stage::HostConnected);
        model.reach(Stage::GnoblinReady);
        model.count(Stage::OutputsConfigured, 1, 2);
        let mut failed = model.clone();
        failed.fail(ProgressFailure { stage: Stage::OutputsConfigured, code: ErrorCode::OutputNotReady, message: "slow".into() });

        for s in [
            DbusSignal::SessionsChanged { revision: 5 },
            DbusSignal::SelectedUserChanged { revision: 6, uid: Some(1000) },
            DbusSignal::SelectedUserChanged { revision: 7, uid: None },
            DbusSignal::AppearanceChanged { uid: 1000, revision: 2 },
            DbusSignal::SessionLaunchProgress { model: model.clone() },
            DbusSignal::SessionLaunchProgress { model: failed },
            DbusSignal::SessionReadinessChanged { session: "s0000000a".into(), generation: 77, ready: true },
            DbusSignal::ForegroundChanged { session: Some("s0000000a".into()), generation: Some(77) },
            DbusSignal::ForegroundChanged { session: None, generation: None },
            DbusSignal::LockStateChanged { session: "s0000000a".into(), locked: true },
            DbusSignal::TransitionChanged {
                operation_id: 4,
                kind: TransitionKind::Lock,
                phase: OperationPhase::Failed,
                state: TransitionState::Failed,
                reason: Some(ErrorCode::Busy),
            },
            DbusSignal::OperationFinished { operation_id: 4, success: false, reason: Some(ErrorCode::Busy) },
        ] {
            r.handle.emit(s);
        }

        let mut got: Vec<(String, zbus::Message)> = Vec::new();
        async_io::block_on(async {
            while got.len() < 12 {
                let m = futures_lite::future::or(async { stream.next().await }, async {
                    Timer::after(Duration::from_secs(5)).await;
                    None
                })
                .await
                .expect("signal within 5 s")
                .unwrap();
                let member = m.header().member().unwrap().to_string();
                got.push((member, m));
            }
        });
        let body = |i: usize| got[i].1.body();
        assert_eq!(got[0].0, "SessionsChanged");
        assert_eq!(body(0).deserialize::<(u64,)>().unwrap(), (5,));
        assert_eq!(got[1].0, "SelectedUserChanged");
        assert_eq!(body(1).deserialize::<(u64, bool, u32)>().unwrap(), (6, true, 1000));
        assert_eq!(body(2).deserialize::<(u64, bool, u32)>().unwrap(), (7, false, 0));
        assert_eq!(body(3).deserialize::<(u32, u64)>().unwrap(), (1000, 2));
        type Progress = (String, u64, u64, Vec<String>, String, u32, u32, u32, String, String, String, String);
        let p: Progress = body(4).deserialize().unwrap();
        assert_eq!(
            p,
            (
                "s0000000a".into(),
                77,
                model.revision,
                vec!["auth_accepted".into(), "pam_session".into(), "host_connected".into(), "gnoblin_ready".into()],
                "outputs_configured".into(),
                1,
                2,
                45,
                "Preparing displays".into(),
                "".into(),
                "".into(),
                "".into()
            )
        );
        let p: Progress = body(5).deserialize().unwrap();
        assert_eq!((p.9.as_str(), p.10.as_str(), p.11.as_str()), ("outputs_configured", "output_not_ready", "slow"));
        assert_eq!(body(6).deserialize::<(String, u64, bool)>().unwrap(), ("s0000000a".into(), 77, true));
        assert_eq!(body(7).deserialize::<(bool, String, u64)>().unwrap(), (true, "s0000000a".into(), 77));
        assert_eq!(body(8).deserialize::<(bool, String, u64)>().unwrap(), (false, "".into(), 0));
        assert_eq!(body(9).deserialize::<(String, bool)>().unwrap(), ("s0000000a".into(), true));
        assert_eq!(
            body(10).deserialize::<(u64, String, String, String, String)>().unwrap(),
            (4, "lock".into(), "failed".into(), "failed".into(), "busy".into())
        );
        assert_eq!(body(11).deserialize::<(u64, bool, String)>().unwrap(), (4, false, "busy".into()));
        assert_eq!(r.handle.emitted_signals(), 12);
        assert_eq!(r.handle.dropped_signals(), 0);
    }

    #[test]
    fn emit_through_a_live_service_never_blocks() {
        let bus = PrivateBus::start();
        let (tx, _d) = driver(sample_script());
        let h = spawn_with(bus.kind(), tx, Config { timeouts: Timeouts::default(), signal_queue: 4 }).unwrap();
        assert!(h.name_acquired());
        let start = Instant::now();
        for r in 0..20_000 {
            h.emit(DbusSignal::SessionsChanged { revision: r });
        }
        assert!(start.elapsed() < Duration::from_secs(2), "{:?}", start.elapsed());
    }

    #[test]
    fn a_second_service_cannot_take_the_name() {
        let bus = PrivateBus::start();
        let (tx, _d) = driver(sample_script());
        let _first = spawn(bus.kind(), tx.clone()).unwrap();
        let second = spawn(bus.kind(), tx);
        assert!(second.is_err());
    }
}
