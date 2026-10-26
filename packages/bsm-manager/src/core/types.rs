//! Inputs, effects and value types of the pure session manager core.
//!
//! The core never reads a clock, never touches a socket and never draws a
//! random number itself. Everything it learns arrives as an [`Input`]; everything
//! it wants done leaves as an [`Effect`]. The I/O runtime (`crate::runtime`) turns
//! the world into inputs and executes the effects in order.

use std::fmt;
use std::time::Duration;

use bsm_proto::control::{Appearance, ClientMessage, ServerMessage, UserProfile};
use bsm_proto::error::{BsmError, ErrorCode};
use bsm_proto::helper::{HelperEvent, HelperRequest};
use bsm_proto::progress::{ProgressModel, Stage};
use bsm_proto::scene::{OutputInfo, SceneCommand, SceneEvent};
use bsm_proto::state::{
    Generation, OperationId, OperationPhase, PowerAction, Revision, SeatState, SessionAvailability, SessionId,
    SessionState, TransitionKind, TransitionState,
};
use serde::{Deserialize, Serialize};

/// Identifier of one D-Bus method call in flight. The runtime keeps the reply channel.
pub type CallId = u64;

/// Source of randomness for session ids and generations. The runtime uses the OS;
/// tests use a deterministic generator.
pub trait Rng: Send {
    fn next_u64(&mut self) -> u64;
}

/// Deterministic generator for tests (splitmix64).
#[derive(Debug, Clone)]
pub struct SplitMix(pub u64);

impl Rng for SplitMix {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

/// Which greeter power actions are allowed without an authenticated user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GreeterPower {
    pub suspend: bool,
    pub reboot: bool,
    pub poweroff: bool,
}

impl Default for GreeterPower {
    fn default() -> Self {
        Self { suspend: true, reboot: true, poweroff: true }
    }
}

impl GreeterPower {
    pub fn allows(&self, action: PowerAction) -> bool {
        match action {
            PowerAction::Suspend => self.suspend,
            PowerAction::Reboot => self.reboot,
            PowerAction::PowerOff => self.poweroff,
        }
    }
}

/// Timeouts and policy. All durations are per stage or per phase and end in a
/// visible failure, never in an endless wait.
#[derive(Debug, Clone)]
pub struct Policy {
    /// Uid of the restricted greeter account. Control and D-Bus callers with this
    /// uid are the greeter. `None` disables greeter recognition (nobody is the greeter).
    pub greeter_uid: Option<u32>,
    pub greeter_power: GreeterPower,
    /// PAM service names handed to the helper.
    pub login_service: String,
    pub unlock_service: String,
    /// Absolute path the bridge's `/proc/<pid>/exe` must resolve to.
    pub host_exe: String,
    pub hostname: String,
    /// Paths handed to the helper when it spawns the greeter.
    pub greeter_wayland_socket: String,
    pub greeter_control_socket: String,
    /// Global reduced motion; the revealing account's own setting is honoured too.
    pub reduce_motion: bool,
    /// Default time for a launch stage (`GnoblinReady` uses `gnoblin_ready_timeout`).
    pub stage_timeout: Duration,
    pub gnoblin_ready_timeout: Duration,
    /// Wait for the compositor to acknowledge `Protect`.
    pub protect_timeout: Duration,
    /// Wait for frames of an existing session before an unlock may begin.
    pub frames_timeout: Duration,
    /// Lifetime of one authentication challenge.
    pub auth_timeout: Duration,
    /// Extra time after a transition's nominal duration before it is declared dead.
    pub transition_grace: Duration,
    /// Wait for polkit and the inhibitor query.
    pub check_timeout: Duration,
    /// Wait for logind to accept a power action.
    pub power_timeout: Duration,
    /// Grace given to the helper before it kills a session, and our own cutoff after it.
    pub close_grace_ms: u32,
    pub close_timeout: Duration,
    pub greeter_respawn_delay: Duration,
    /// A recovered session whose bridge does not reconnect within this time is closed.
    pub recovery_reconnect: Duration,
    /// After `max_failures` consecutive failed authentications of an account, further
    /// attempts are refused for `failure_cooldown` after the last one.
    pub max_failures: u32,
    pub failure_cooldown: Duration,
    /// Minimum spacing of appearance publications per uid.
    pub publish_interval: Duration,
    /// Overrides of the default transition durations (milliseconds).
    pub duration_overrides: Vec<(TransitionKind, u32)>,
    /// Number of finished operations kept for `GetTransitions`.
    pub history_len: usize,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            greeter_uid: None,
            greeter_power: GreeterPower::default(),
            login_service: "bsm-login".into(),
            unlock_service: "bsm-unlock".into(),
            host_exe: "/usr/libexec/bsm/bsm-host".into(),
            hostname: "bingux".into(),
            greeter_wayland_socket: bsm_proto::paths::GREETER_WAYLAND_SOCKET.into(),
            greeter_control_socket: bsm_proto::paths::GREETER_CONTROL_SOCKET.into(),
            reduce_motion: false,
            stage_timeout: Duration::from_secs(120),
            gnoblin_ready_timeout: Duration::from_secs(120),
            protect_timeout: Duration::from_secs(3),
            frames_timeout: Duration::from_secs(10),
            auth_timeout: Duration::from_secs(120),
            transition_grace: Duration::from_secs(5),
            check_timeout: Duration::from_secs(15),
            power_timeout: Duration::from_secs(30),
            close_grace_ms: 5000,
            close_timeout: Duration::from_secs(15),
            greeter_respawn_delay: Duration::from_secs(1),
            recovery_reconnect: Duration::from_secs(30),
            max_failures: 5,
            failure_cooldown: Duration::from_secs(30),
            publish_interval: Duration::from_millis(500),
            duration_overrides: Vec::new(),
            history_len: 32,
        }
    }
}

impl Policy {
    pub fn stage_timeout_for(&self, stage: Stage) -> Duration {
        if stage == Stage::GnoblinReady { self.gnoblin_ready_timeout } else { self.stage_timeout }
    }

    pub fn duration_ms(&self, kind: TransitionKind) -> u32 {
        self.duration_overrides
            .iter()
            .find(|(k, _)| *k == kind)
            .map(|(_, ms)| *ms)
            .unwrap_or_else(|| bsm_proto::transition::default_duration_ms(kind))
    }
}

/// Appearance as published (cache file paths are under `GREETER_CACHE_DIR`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishedView {
    pub appearance: Appearance,
    pub avatar_path: Option<String>,
    pub avatar_rev: Revision,
    pub wallpaper_path: Option<String>,
    pub wallpaper_blur_path: Option<String>,
    pub wallpaper_rev: Revision,
    pub show_personal_images: bool,
}

/// One eligible account handed over by the account source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountEntry {
    pub uid: u32,
    pub user_name: String,
    pub display_name: String,
    pub published: Option<PublishedView>,
}

/// A source image handed over by the caller as an open file descriptor. bsm never
/// opens a path supplied by a user: the publisher runs as the user, opens its own file
/// and passes the descriptor. The core only carries the handle; the runtime validates
/// that it is a regular, size-capped file and hands it to the converter.
#[derive(Clone)]
pub struct FdHandle(pub std::sync::Arc<std::os::fd::OwnedFd>);

impl PartialEq for FdHandle {
    fn eq(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for FdHandle {}
impl fmt::Debug for FdHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FdHandle")
    }
}

/// Unvalidated appearance publication from a user's shell. The uid is never part of
/// it: it is the D-Bus peer uid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishRequest {
    pub appearance: Appearance,
    pub show_personal_images: bool,
    pub wallpaper: Option<FdHandle>,
    pub avatar: Option<FdHandle>,
}

/// Safe metadata persisted between runs. No secret and no resumable grant.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Persisted {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub last_used_uid: Option<u32>,
    #[serde(default)]
    pub sessions: Vec<PersistedSession>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedSession {
    pub id: SessionId,
    pub generation: Generation,
    pub uid: u32,
    pub user_name: String,
    pub scope: String,
    pub host_pid: u32,
}

/// A session found in the persisted registry at start. `alive` is the runtime's probe
/// of the session scope (not a claim by the session itself).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveredSession {
    pub session: PersistedSession,
    pub alive: bool,
}

/// One logind inhibitor as reported by `ListInhibitors`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inhibitor {
    pub what: String,
    pub who: String,
    pub why: String,
    pub mode: String,
    pub uid: u32,
}

/// The identity of a D-Bus caller as the runtime resolved it from the bus. The
/// manager maps it to a principal; the caller never names its own role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Caller {
    pub uid: u32,
    pub pid: u32,
    /// Unique bus name, the polkit subject.
    pub bus_name: String,
    /// Unified cgroup path of the caller's pid, if readable.
    pub cgroup: Option<String>,
}

/// What a caller is, as decided by the manager.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Principal {
    Root,
    Greeter,
    Session(SessionId),
    /// A local process of an ordinary uid that is in no hosted session.
    User(u32),
}

/// A request made through D-Bus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Method {
    GetUsers,
    GetSessions,
    GetSelectedUser,
    GetForegroundSession,
    GetStatus,
    GetOutputs,
    GetTransitions,
    SelectGreeterUser { uid: u32 },
    RequestLock { session: Option<SessionId> },
    RequestSwitchUser,
    RequestLogout { session: Option<SessionId> },
    RequestSuspend,
    RequestReboot,
    RequestPowerOff,
    CancelOperation { operation: OperationId },
    PublishAppearance(PublishRequest),
}

impl Method {
    pub fn name(&self) -> &'static str {
        match self {
            Method::GetUsers => "GetUsers",
            Method::GetSessions => "GetSessions",
            Method::GetSelectedUser => "GetSelectedUser",
            Method::GetForegroundSession => "GetForegroundSession",
            Method::GetStatus => "GetStatus",
            Method::GetOutputs => "GetOutputs",
            Method::GetTransitions => "GetTransitions",
            Method::SelectGreeterUser { .. } => "SelectGreeterUser",
            Method::RequestLock { .. } => "RequestLock",
            Method::RequestSwitchUser => "RequestSwitchUser",
            Method::RequestLogout { .. } => "RequestLogout",
            Method::RequestSuspend => "RequestSuspend",
            Method::RequestReboot => "RequestReboot",
            Method::RequestPowerOff => "RequestPowerOff",
            Method::CancelOperation { .. } => "CancelOperation",
            Method::PublishAppearance(_) => "PublishAppearance",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserInfo {
    pub uid: u32,
    pub user_name: String,
    pub display_name: String,
    pub availability: SessionAvailability,
    pub session: Option<SessionId>,
    pub avatar_rev: Revision,
    pub wallpaper_rev: Revision,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionInfo {
    pub id: SessionId,
    pub generation: Generation,
    pub uid: u32,
    pub state: SessionState,
    pub availability: SessionAvailability,
    pub foreground: bool,
    pub locked: bool,
    /// Launch progress fraction 0..=100 while starting, 100 once ready.
    pub progress: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationInfo {
    pub id: OperationId,
    pub kind: TransitionKind,
    pub phase: OperationPhase,
    pub state: TransitionState,
    pub reason: Option<ErrorCode>,
    pub session: Option<SessionId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusInfo {
    pub seat: SeatState,
    pub selected_uid: Option<u32>,
    pub last_used_uid: Option<u32>,
    pub foreground: Option<SessionId>,
    pub revision: Revision,
    pub operation: Option<OperationInfo>,
    pub greeter_connected: bool,
    pub helper_connected: bool,
    pub outputs: usize,
}

/// Successful answer to a D-Bus call.
#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    Users(Vec<UserInfo>),
    Sessions(Vec<SessionInfo>),
    /// `GetSelectedUser`: the uid, or `None`.
    Selected(Option<u32>),
    Foreground(Option<(SessionId, Generation)>),
    Status(StatusInfo),
    Outputs(Vec<OutputInfo>),
    Transitions(Vec<OperationInfo>),
    Operation(OperationId),
    Done,
}

/// Signals the manager wants emitted on D-Bus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DbusSignal {
    SessionsChanged { revision: Revision },
    SelectedUserChanged { revision: Revision, uid: Option<u32> },
    AppearanceChanged { uid: u32, revision: Revision },
    SessionLaunchProgress { model: ProgressModel },
    SessionReadinessChanged { session: SessionId, generation: Generation, ready: bool },
    ForegroundChanged { session: Option<SessionId>, generation: Option<Generation> },
    LockStateChanged { session: SessionId, locked: bool },
    TransitionChanged { operation_id: OperationId, kind: TransitionKind, phase: OperationPhase, state: TransitionState, reason: Option<ErrorCode> },
    OperationFinished { operation_id: OperationId, success: bool, reason: Option<ErrorCode> },
}

/// Timer identities. Setting a timer that is already armed re-arms it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TimerId {
    Stage(SessionId, Generation, Stage),
    Protect(SessionId, Generation),
    Frames(OperationId),
    Auth(u64),
    Close(SessionId, Generation),
    Transition(OperationId),
    Checks(OperationId),
    Power(OperationId),
    GreeterRespawn,
    Recovery(SessionId, Generation),
}

/// What the runtime does with a greeter control connection that announced a peer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnVerdict {
    pub conn: u64,
    pub accept: bool,
}

/// Structured log record for the runtime to emit. Never carries secrets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogRecord {
    pub level: LogLevel,
    pub session: Option<SessionId>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

/// Everything the core can ask the world to do, in execution order.
#[derive(Clone, PartialEq)]
pub enum Effect {
    Scene(SceneCommand),
    Helper(HelperRequest),
    /// To the greeter control connection, if one is attached.
    Greeter(ServerMessage),
    Dbus(DbusSignal),
    DbusReply { call: CallId, result: Result<Reply, BsmError> },
    ConnVerdict(ConnVerdict),
    /// Close the greeter control connection (protocol violation).
    DropGreeter,
    SetTimer { id: TimerId, after: Duration },
    CancelTimer(TimerId),
    Persist(Persisted),
    /// Enumerate eligible accounts; answered with `Input::Accounts`.
    LoadAccounts,
    /// Validate, convert and publish; answered with `Input::AppearancePublished`.
    PublishAppearance { call: Option<CallId>, uid: u32, request: PublishRequest },
    /// polkit `CheckAuthorization` for `caller`; answered with `Input::PolkitResult`.
    Polkit { operation: OperationId, action_id: String, caller: Caller },
    /// `ListInhibitors`; answered with `Input::InhibitorsResult`.
    QueryInhibitors { operation: OperationId, action: PowerAction },
    /// Call logind. Answered with `Input::PowerResult`.
    SystemPower { operation: OperationId, action: PowerAction },
    /// All sessions are protected: release the sleep delay inhibitor.
    SleepReady,
    Log(LogRecord),
}

impl fmt::Debug for Effect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Effect::Scene(c) => write!(f, "Scene({c:?})"),
            Effect::Helper(HelperRequest::AuthResponse { req_id, .. }) => {
                write!(f, "Helper(AuthResponse {{ req_id: {req_id}, response: <redacted> }})")
            }
            Effect::Helper(r) => write!(f, "Helper({r:?})"),
            Effect::Greeter(m) => write!(f, "Greeter({m:?})"),
            Effect::Dbus(s) => write!(f, "Dbus({s:?})"),
            Effect::DbusReply { call, result } => write!(f, "DbusReply({call}, {result:?})"),
            Effect::ConnVerdict(v) => write!(f, "ConnVerdict({v:?})"),
            Effect::DropGreeter => write!(f, "DropGreeter"),
            Effect::SetTimer { id, after } => write!(f, "SetTimer({id:?}, {after:?})"),
            Effect::CancelTimer(id) => write!(f, "CancelTimer({id:?})"),
            Effect::Persist(p) => write!(f, "Persist({p:?})"),
            Effect::LoadAccounts => write!(f, "LoadAccounts"),
            Effect::PublishAppearance { call, uid, .. } => write!(f, "PublishAppearance({call:?}, {uid})"),
            Effect::Polkit { operation, action_id, .. } => write!(f, "Polkit({operation}, {action_id})"),
            Effect::QueryInhibitors { operation, action } => write!(f, "QueryInhibitors({operation}, {action:?})"),
            Effect::SystemPower { operation, action } => write!(f, "SystemPower({operation}, {action:?})"),
            Effect::SleepReady => write!(f, "SleepReady"),
            Effect::Log(l) => write!(f, "Log({l:?})"),
        }
    }
}

/// Everything the core can be told.
#[derive(Clone)]
pub enum Input {
    /// First input. `persisted` is what the previous run left (or the default).
    Start { persisted: Persisted },
    Scene(SceneEvent),
    Helper(HelperEvent),
    HelperConnected,
    HelperLost,
    /// A connection to the greeter control socket passed uid verification; its peer pid
    /// is checked against the greeter process bsm started. The runtime waits for the verdict.
    GreeterPeer { conn: u64, uid: u32, pid: u32 },
    /// The verified connection is closed or lost.
    GreeterDisconnected { conn: u64 },
    Greeter { conn: u64, message: ClientMessage },
    Dbus { call: CallId, caller: Caller, method: Method },
    Timer(TimerId),
    Accounts(Vec<AccountEntry>),
    Recovered(Vec<RecoveredSession>),
    AppearancePublished { call: Option<CallId>, uid: u32, result: Result<PublishedView, BsmError> },
    PolkitResult { operation: OperationId, authorised: bool },
    InhibitorsResult { operation: OperationId, result: Result<Vec<Inhibitor>, BsmError> },
    PowerResult { operation: OperationId, result: Result<(), BsmError> },
    /// logind `PrepareForSleep(start)`.
    PrepareForSleep { start: bool },
}

impl fmt::Debug for Input {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Input::Greeter { conn, message: ClientMessage::AuthResponse { challenge_id, .. } } => {
                write!(f, "Greeter({conn}, AuthResponse {{ challenge_id: {challenge_id}, response: <redacted> }})")
            }
            Input::Greeter { conn, message } => write!(f, "Greeter({conn}, {message:?})"),
            Input::Start { persisted } => write!(f, "Start({persisted:?})"),
            Input::Scene(e) => write!(f, "Scene({e:?})"),
            Input::Helper(e) => write!(f, "Helper({e:?})"),
            Input::HelperConnected => write!(f, "HelperConnected"),
            Input::HelperLost => write!(f, "HelperLost"),
            Input::GreeterPeer { conn, uid, pid } => write!(f, "GreeterPeer({conn}, uid {uid}, pid {pid})"),
            Input::GreeterDisconnected { conn } => write!(f, "GreeterDisconnected({conn})"),
            Input::Dbus { call, caller, method } => write!(f, "Dbus({call}, {caller:?}, {})", method.name()),
            Input::Timer(t) => write!(f, "Timer({t:?})"),
            Input::Accounts(a) => write!(f, "Accounts({} accounts)", a.len()),
            Input::Recovered(r) => write!(f, "Recovered({} sessions)", r.len()),
            Input::AppearancePublished { call, uid, result } => {
                write!(f, "AppearancePublished({call:?}, {uid}, ok={})", result.is_ok())
            }
            Input::PolkitResult { operation, authorised } => write!(f, "PolkitResult({operation}, {authorised})"),
            Input::InhibitorsResult { operation, result } => {
                write!(f, "InhibitorsResult({operation}, {:?})", result.as_ref().map(|v| v.len()))
            }
            Input::PowerResult { operation, result } => write!(f, "PowerResult({operation}, {result:?})"),
            Input::PrepareForSleep { start } => write!(f, "PrepareForSleep({start})"),
        }
    }
}

/// Convenience for building account-bound profiles in tests and the runtime.
pub fn profile_from(entry: &AccountEntry) -> UserProfile {
    let view = entry.published.clone().unwrap_or(PublishedView {
        appearance: Appearance::default(),
        avatar_path: None,
        avatar_rev: 0,
        wallpaper_path: None,
        wallpaper_blur_path: None,
        wallpaper_rev: 0,
        show_personal_images: true,
    });
    UserProfile {
        uid: entry.uid,
        user_name: entry.user_name.clone(),
        display_name: entry.display_name.clone(),
        avatar_path: view.avatar_path,
        avatar_rev: view.avatar_rev,
        wallpaper_path: view.wallpaper_path,
        wallpaper_blur_path: view.wallpaper_blur_path,
        wallpaper_rev: view.wallpaper_rev,
        show_personal_images: view.show_personal_images,
        appearance: view.appearance,
        availability: SessionAvailability::NotSignedIn,
        session: None,
    }
}

/// True when `cgroup` (a unified cgroup path as in `/proc/<pid>/cgroup`) lies in `scope`.
/// `scope` is either a unit name such as `bsm-session-s0000000a.scope`, which must equal
/// one path component, or an absolute cgroup path that must be a prefix on a component
/// boundary. An empty scope never matches.
pub fn cgroup_in_scope(cgroup: &str, scope: &str) -> bool {
    if scope.is_empty() {
        return false;
    }
    if let Some(abs) = scope.strip_prefix('/') {
        let abs = abs.trim_end_matches('/');
        if abs.is_empty() {
            return false;
        }
        let cg = cgroup.trim_start_matches('/');
        return cg == abs || cg.strip_prefix(abs).is_some_and(|rest| rest.starts_with('/'));
    }
    cgroup.split('/').any(|c| c == scope)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_matches_on_component_boundaries() {
        let cg = "/user.slice/user-1000.slice/user@1000.service/app.slice/bsm-s1.scope/gnoblin";
        assert!(cgroup_in_scope(cg, "bsm-s1.scope"));
        assert!(!cgroup_in_scope(cg, "bsm-s1"));
        assert!(!cgroup_in_scope(cg, "sm-s1.scope"));
        assert!(!cgroup_in_scope(cg, ""));
        assert!(cgroup_in_scope(cg, "/user.slice/user-1000.slice"));
        assert!(!cgroup_in_scope(cg, "/user.slice/user-100"));
        assert!(!cgroup_in_scope(cg, "/"));
        assert!(cgroup_in_scope("/a/b", "/a/b"));
    }

    #[test]
    fn debug_never_shows_secrets() {
        let secret = "hunter2-very-secret";
        let i = Input::Greeter { conn: 1, message: ClientMessage::AuthResponse { challenge_id: 3, response: secret.into() } };
        assert!(!format!("{i:?}").contains(secret));
        let e = Effect::Helper(HelperRequest::AuthResponse { req_id: 1, response: secret.into() });
        assert!(!format!("{e:?}").contains(secret));
    }
}
