//! The pure session manager core.
//!
//! [`Manager::handle`] takes the current monotonic time and one [`Input`] and returns
//! the [`Effect`]s to execute, in order. It performs no I/O and reads no clock.
//! Randomness comes from an injected [`Rng`].
//!
//! The state is a handful of deliberately separate fields, never merged into
//! booleans:
//!
//! | field            | meaning                                                              |
//! | ---------------- | -------------------------------------------------------------------- |
//! | `selected_uid`   | whose identity the greeter shows. Changing it reveals nothing.       |
//! | `last_used_uid`  | account that last reached the foreground. Persisted.                 |
//! | `foreground`     | the seat's active session; stays set while that session is locked.   |
//! | `protected`      | sessions whose protection the compositor acknowledged.               |
//! | `challenge`      | the authentication target of the one conversation in flight.         |
//! | `input_owner`    | who the manager last told the compositor owns physical input.        |
//!
//! Submodules: `session` (launch gate and session lifecycle), `auth` (greeter
//! protocol and challenges), `ops` (operations: lock, reveal, logout), `power`
//! (suspend, reboot, power off) and `dbus` (per-caller authorisation).

mod auth;
mod dbus;
mod ops;
mod power;
mod session;
pub mod tables;
#[cfg(test)]
mod tests;
pub mod types;

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::Instant;

use bsm_proto::control::{GreeterMode, ServerMessage, Snapshot, UserProfile};
use bsm_proto::error::ErrorCode;
use bsm_proto::progress::ProgressModel;
use bsm_proto::scene::{OutputInfo, SceneCommand, SceneEvent};
use bsm_proto::state::{
    Generation, InputOwner, Revision, SeatState, SessionAvailability, SessionId,
    SessionState, SessionSummary, TransitionState,
};

pub use ops::Requester;
pub use session::Session;
pub use types::*;

use auth::Challenge;
use ops::Operation;

pub(crate) type Fx = Vec<Effect>;

/// Static account data plus what was last published for it.
#[derive(Debug, Clone)]
struct UserEntry {
    profile: UserProfile,
}

/// State of the greeter process and its two connections.
#[derive(Debug, Default, Clone)]
struct GreeterState {
    /// Generation of the greeter bsm-helper was asked to spawn.
    generation: Option<Generation>,
    spawn_req: Option<u64>,
    pid: Option<u32>,
    /// The Wayland connection was authorised and connected for `generation`.
    wayland: bool,
    /// Verified control connection and whether it said hello.
    control: Option<u64>,
    hello: bool,
    respawn_pending: bool,
}

/// What was last published, to emit only real changes.
#[derive(Debug, Default, Clone)]
struct Published {
    snapshot: Option<Snapshot>,
    sessions: Vec<SessionSummary>,
    selected: Option<u32>,
    foreground: Option<(SessionId, Generation)>,
    locked: BTreeMap<SessionId, bool>,
    ready: BTreeMap<SessionId, bool>,
    progress: Option<(SessionId, Generation, Revision)>,
    persisted: Option<Persisted>,
}

pub struct Manager {
    policy: Policy,
    rng: Box<dyn Rng>,
    now: Option<Instant>,
    started: bool,

    users: BTreeMap<u32, UserEntry>,
    sessions: BTreeMap<SessionId, Session>,

    seat: SeatState,
    selected_uid: Option<u32>,
    last_used_uid: Option<u32>,
    foreground: Option<SessionId>,
    protected: BTreeSet<SessionId>,
    challenge: Option<Challenge>,
    input_owner: InputOwner,
    op: Option<Operation>,
    history: VecDeque<OperationInfo>,
    /// Progress of a failed launch, shown until the greeter goes back or retries.
    retained_failure: Option<ProgressModel>,
    switching_user: bool,

    compositor_ready: bool,
    outputs: Vec<OutputInfo>,
    outputs_epoch: u64,
    greeter: GreeterState,
    helper_up: bool,

    next_op: u64,
    next_challenge: u64,
    next_helper_req: u64,
    fg_seq: u64,
    revision: Revision,
    sessions_revision: Revision,
    selected_revision: Revision,

    failures: BTreeMap<u32, (u32, Instant)>,
    last_publish: BTreeMap<u32, Instant>,
    publishing: BTreeSet<u32>,
    appearance_rev: BTreeMap<u32, Revision>,
    shutting_down: bool,
    published: Published,
    /// Sleep preparation started by logind rather than by a bsm operation.
    sleep_prep: Option<BTreeSet<(SessionId, Generation)>>,
}

impl Manager {
    pub fn new(policy: Policy, rng: Box<dyn Rng>) -> Self {
        Self {
            policy,
            rng,
            now: None,
            started: false,
            users: BTreeMap::new(),
            sessions: BTreeMap::new(),
            seat: SeatState::Booting,
            selected_uid: None,
            last_used_uid: None,
            foreground: None,
            protected: BTreeSet::new(),
            challenge: None,
            input_owner: InputOwner::None,
            op: None,
            history: VecDeque::new(),
            retained_failure: None,
            switching_user: false,
            compositor_ready: false,
            outputs: Vec::new(),
            outputs_epoch: 0,
            greeter: GreeterState::default(),
            helper_up: false,
            next_op: 1,
            next_challenge: 1,
            next_helper_req: 1,
            fg_seq: 0,
            revision: 0,
            sessions_revision: 0,
            selected_revision: 0,
            failures: BTreeMap::new(),
            last_publish: BTreeMap::new(),
            publishing: BTreeSet::new(),
            appearance_rev: BTreeMap::new(),
            shutting_down: false,
            published: Published::default(),
            sleep_prep: None,
        }
    }

    // ----- read-only accessors (used by the runtime, the tests and diagnostics) -----

    pub fn policy(&self) -> &Policy {
        &self.policy
    }
    pub fn seat(&self) -> SeatState {
        self.seat
    }
    pub fn selected_uid(&self) -> Option<u32> {
        self.selected_uid
    }
    pub fn last_used_uid(&self) -> Option<u32> {
        self.last_used_uid
    }
    pub fn foreground(&self) -> Option<&SessionId> {
        self.foreground.as_ref()
    }
    pub fn protected(&self) -> &BTreeSet<SessionId> {
        &self.protected
    }
    pub fn input_owner(&self) -> &InputOwner {
        &self.input_owner
    }
    pub fn session(&self, id: &str) -> Option<&Session> {
        self.sessions.get(id)
    }
    pub fn sessions(&self) -> impl Iterator<Item = &Session> {
        self.sessions.values()
    }
    /// The running (not stopping, not failed) session of an account.
    pub fn session_of_uid(&self, uid: u32) -> Option<&Session> {
        self.running_session(uid)
    }
    pub fn operation(&self) -> Option<OperationInfo> {
        self.op.as_ref().map(|o| o.info())
    }
    pub fn history(&self) -> impl Iterator<Item = &OperationInfo> {
        self.history.iter()
    }
    pub fn outputs(&self) -> &[OutputInfo] {
        &self.outputs
    }
    pub fn greeter_wayland_connected(&self) -> bool {
        self.greeter.wayland
    }
    pub fn greeter_control_connected(&self) -> bool {
        self.greeter.control.is_some() && self.greeter.hello
    }
    pub fn retained_failure(&self) -> Option<&ProgressModel> {
        self.retained_failure.as_ref()
    }
    pub fn revision(&self) -> Revision {
        self.revision
    }
    /// The current greeter snapshot (revision included).
    pub fn snapshot(&self) -> Snapshot {
        let mut s = self.build_snapshot();
        s.revision = self.revision;
        s
    }

    /// Feed one input; returns the effects in execution order.
    pub fn handle(&mut self, now: Instant, input: Input) -> Vec<Effect> {
        self.now = Some(now);
        let mut fx: Fx = Vec::new();
        match input {
            Input::Start { persisted } => self.on_start(persisted, &mut fx),
            Input::Scene(e) => self.on_scene(e, &mut fx),
            Input::Helper(e) => self.on_helper(e, &mut fx),
            Input::HelperConnected => self.on_helper_connected(&mut fx),
            Input::HelperLost => self.on_helper_lost(&mut fx),
            Input::GreeterPeer { conn, uid, pid } => self.on_greeter_peer(conn, uid, pid, &mut fx),
            Input::GreeterDisconnected { conn } => self.on_greeter_disconnected(conn, &mut fx),
            Input::Greeter { conn, message } => self.on_greeter_message(conn, message, &mut fx),
            Input::Dbus { call, caller, method } => self.on_dbus(call, caller, method, &mut fx),
            Input::Timer(id) => self.on_timer(id, &mut fx),
            Input::Accounts(a) => self.on_accounts(a, &mut fx),
            Input::Recovered(r) => self.on_recovered(r, &mut fx),
            Input::AppearancePublished { call, uid, result } => self.on_appearance_published(call, uid, result, &mut fx),
            Input::PolkitResult { operation, authorised } => self.on_polkit(operation, authorised, &mut fx),
            Input::InhibitorsResult { operation, result } => self.on_inhibitors(operation, result, &mut fx),
            Input::PowerResult { operation, result } => self.on_power_result(operation, result, &mut fx),
            Input::PrepareForSleep { start } => self.on_prepare_for_sleep(start, &mut fx),
        }
        self.settle(&mut fx);
        #[cfg(debug_assertions)]
        if let Err(e) = self.check_invariants() {
            panic!("manager invariant violated: {e}");
        }
        fx
    }

    pub(crate) fn now(&self) -> Instant {
        self.now.expect("handle() sets the time before any handler runs")
    }

    pub(crate) fn log(fx: &mut Fx, level: LogLevel, session: Option<&str>, message: impl Into<String>) {
        fx.push(Effect::Log(LogRecord { level, session: session.map(str::to_owned), message: message.into() }));
    }

    fn alloc_session_id(&mut self) -> SessionId {
        loop {
            let id = format!("s{:08x}", self.rng.next_u64() as u32);
            if !self.sessions.contains_key(&id) {
                return id;
            }
        }
    }

    fn alloc_generation(&mut self) -> Generation {
        loop {
            let g = self.rng.next_u64();
            if g != 0 {
                return g;
            }
        }
    }

    fn helper_req(&mut self) -> u64 {
        let r = self.next_helper_req;
        self.next_helper_req += 1;
        r
    }

    // ----- start, accounts, recovery -----

    fn on_start(&mut self, persisted: Persisted, fx: &mut Fx) {
        if self.started {
            return;
        }
        self.started = true;
        self.last_used_uid = persisted.last_used_uid;
        fx.push(Effect::LoadAccounts);
    }

    fn on_accounts(&mut self, accounts: Vec<AccountEntry>, _fx: &mut Fx) {
        let mut users = BTreeMap::new();
        for a in &accounts {
            users.insert(a.uid, UserEntry { profile: profile_from(a) });
        }
        self.users = users;
        let valid = |uid: Option<u32>, users: &BTreeMap<u32, UserEntry>| uid.filter(|u| users.contains_key(u));
        let keep_selected = valid(self.selected_uid, &self.users);
        let can_change_selection = self.challenge.is_none() && self.op.is_none();
        if keep_selected.is_none() || (can_change_selection && self.selected_uid.is_none()) {
            self.selected_uid = valid(self.last_used_uid, &self.users).or_else(|| self.users.keys().next().copied());
        }
    }

    fn on_recovered(&mut self, recovered: Vec<RecoveredSession>, fx: &mut Fx) {
        for r in recovered {
            if !r.alive {
                Self::log(fx, LogLevel::Info, Some(&r.session.id), "recovered session is gone; dropped");
                continue;
            }
            if self.sessions.contains_key(&r.session.id) || self.running_session(r.session.uid).is_some() {
                continue;
            }
            self.recover_session(r.session, fx);
        }
    }

    // ----- scene events that are not about one session -----

    fn on_scene(&mut self, e: SceneEvent, fx: &mut Fx) {
        match e {
            SceneEvent::Ready { outputs } => {
                self.compositor_ready = true;
                self.update_outputs(outputs, fx);
                fx.push(Effect::Scene(SceneCommand::ShowGreeterOnly));
                self.maybe_spawn_greeter(fx);
            }
            SceneEvent::OutputsChanged { outputs } => self.update_outputs(outputs, fx),
            SceneEvent::GreeterConnected { generation, pid } => {
                if self.greeter.generation == Some(generation) && self.greeter.pid.is_none_or(|p| p == pid) {
                    self.greeter.wayland = true;
                    self.greeter.pid = Some(pid);
                    self.try_reveal(fx);
                } else {
                    Self::log(fx, LogLevel::Warn, None, "greeter connection for an unknown generation ignored");
                }
            }
            SceneEvent::GreeterGone { generation } => {
                if self.greeter.generation == Some(generation) {
                    fx.push(Effect::Helper(bsm_proto::helper::HelperRequest::KillGreeter { generation }));
                    self.greeter_lost(fx);
                }
            }
            SceneEvent::SessionSocketReady { session, generation, path } => {
                self.on_socket_ready(&session, generation, path, fx)
            }
            SceneEvent::SessionConnected { session, generation, pid } => {
                self.on_session_connected(&session, generation, pid, fx)
            }
            SceneEvent::SessionConnectionRejected { session, generation, reason } => {
                self.on_connection_rejected(&session, generation, reason, fx)
            }
            SceneEvent::SessionSurfaces { session, generation, surfaces } => {
                self.on_surfaces(&session, generation, surfaces, fx)
            }
            SceneEvent::SessionReport { session, generation, report } => self.on_report(&session, generation, report, fx),
            SceneEvent::SessionGone { session, generation } => self.on_session_gone(&session, generation, fx),
            SceneEvent::Protected { session, generation } => self.on_protected(&session, generation, fx),
            SceneEvent::InputOwnerChanged(owner) => {
                // The compositor may refuse a SetInputOwner (protected or not presentable). The
                // manager records the actual owner; it never assumes its request was honoured.
                self.input_owner = owner;
            }
            SceneEvent::TransitionPhase { operation, phase } => self.on_transition_phase(operation, phase, fx),
        }
    }

    fn update_outputs(&mut self, outputs: Vec<OutputInfo>, fx: &mut Fx) {
        if self.outputs == outputs {
            return;
        }
        self.outputs = outputs;
        self.outputs_epoch += 1;
        self.on_outputs_changed(fx);
    }

    // ----- helper connection -----

    fn on_helper_connected(&mut self, fx: &mut Fx) {
        self.helper_up = true;
        if !self.greeter.wayland {
            self.greeter.generation = None;
            self.greeter.spawn_req = None;
        }
        self.maybe_spawn_greeter(fx);
    }

    fn on_helper_lost(&mut self, fx: &mut Fx) {
        self.helper_up = false;
        Self::log(fx, LogLevel::Error, None, "helper connection lost");
        // A conversation or a launch that depended on the helper cannot finish.
        if self.challenge.is_some() {
            self.fail_challenge(ErrorCode::HelperFailure, "the authentication helper is not available", fx);
        }
        let launching: Vec<(SessionId, Generation)> =
            self.sessions.values().filter(|s| s.launching() && s.open_sent).map(|s| (s.id.clone(), s.generation)).collect();
        for (sid, gn) in launching {
            self.fail_launch(&sid, gn, ErrorCode::HelperFailure, "the session helper connection was lost", fx);
        }
    }

    // ----- greeter process -----

    fn maybe_spawn_greeter(&mut self, fx: &mut Fx) {
        if !self.helper_up || !self.compositor_ready || self.greeter.generation.is_some() || self.greeter.respawn_pending {
            return;
        }
        let generation = self.alloc_generation();
        let req_id = self.helper_req();
        self.greeter.generation = Some(generation);
        self.greeter.spawn_req = Some(req_id);
        self.greeter.pid = None;
        fx.push(Effect::Helper(bsm_proto::helper::HelperRequest::SpawnGreeter {
            req_id,
            generation,
            wayland_socket: self.policy.greeter_wayland_socket.clone(),
            control_socket: self.policy.greeter_control_socket.clone(),
        }));
    }

    /// The greeter is gone (crash, kill, helper report). Nothing it protected is released.
    fn greeter_lost(&mut self, fx: &mut Fx) {
        self.greeter.wayland = false;
        self.greeter.generation = None;
        self.greeter.spawn_req = None;
        self.greeter.pid = None;
        if self.greeter.control.take().is_some() {
            fx.push(Effect::DropGreeter);
        }
        self.greeter.hello = false;
        // The conversation belonged to the greeter that is gone.
        if self.challenge.is_some() {
            self.fail_challenge(ErrorCode::Cancelled, "the greeter went away", fx);
        }
        if !self.greeter.respawn_pending {
            self.greeter.respawn_pending = true;
            fx.push(Effect::SetTimer { id: TimerId::GreeterRespawn, after: self.policy.greeter_respawn_delay });
        }
        Self::log(fx, LogLevel::Warn, None, "greeter lost; sessions stay protected until it returns");
    }

    fn on_helper_greeter_spawned(&mut self, req_id: u64, generation: Generation, pid: u32, fx: &mut Fx) {
        if self.greeter.generation != Some(generation) || self.greeter.spawn_req != Some(req_id) {
            Self::log(fx, LogLevel::Warn, None, "stale GreeterSpawned ignored");
            return;
        }
        self.greeter.pid = Some(pid);
        fx.push(Effect::Scene(SceneCommand::RegisterGreeter {
            generation,
            uid: self.policy.greeter_uid.unwrap_or(0),
            pid,
        }));
    }

    fn on_timer(&mut self, id: TimerId, fx: &mut Fx) {
        match id {
            TimerId::GreeterRespawn => {
                self.greeter.respawn_pending = false;
                self.maybe_spawn_greeter(fx);
            }
            TimerId::Stage(sid, gn, stage) => self.on_stage_timeout(&sid, gn, stage, fx),
            TimerId::Protect(sid, gn) => self.on_protect_timeout(&sid, gn, fx),
            TimerId::Frames(op) => self.on_frames_timeout(op, fx),
            TimerId::Auth(challenge) => self.on_auth_timeout(challenge, fx),
            TimerId::Close(sid, gn) => self.on_close_timeout(&sid, gn, fx),
            TimerId::Transition(op) => self.on_transition_timeout(op, fx),
            TimerId::Checks(op) => self.on_checks_timeout(op, fx),
            TimerId::Power(op) => self.on_power_timeout(op, fx),
            TimerId::Recovery(sid, gn) => self.on_recovery_timeout(&sid, gn, fx),
        }
    }

    // ----- derived views -----

    /// Availability of the account as the picker shows it, derived from real sessions only.
    fn availability_of(&self, uid: u32) -> (SessionAvailability, Option<SessionId>) {
        if let Some(s) = self.running_session(uid) {
            let a = match s.state {
                SessionState::Foreground => SessionAvailability::RunningForeground,
                SessionState::LockRequested | SessionState::LockedBackground | SessionState::LockedGreeterForeground => {
                    SessionAvailability::RunningLocked
                }
                _ => SessionAvailability::Starting,
            };
            return (a, Some(s.id.clone()));
        }
        let dying = self.sessions.values().any(|s| s.uid == uid && matches!(s.state, SessionState::Failed | SessionState::Stopping));
        if dying { (SessionAvailability::Unavailable, None) } else { (SessionAvailability::NotSignedIn, None) }
    }

    pub(crate) fn running_session(&self, uid: u32) -> Option<&Session> {
        self.sessions
            .values()
            .filter(|s| s.uid == uid && s.state.is_running())
            .max_by_key(|s| s.fg_seq)
    }

    fn greeter_target(&self) -> Option<&Session> {
        self.sessions.values().find(|s| s.state == SessionState::LockedGreeterForeground)
    }

    /// True when the greeter, not a user's desktop, is what the seat shows and who owns input.
    pub(crate) fn greeter_authoritative(&self) -> bool {
        if self.seat == SeatState::ShuttingDown {
            return false;
        }
        !self.sessions.values().any(|s| matches!(s.state, SessionState::Foreground | SessionState::LockRequested))
    }

    fn progress_view(&self) -> Option<ProgressModel> {
        // The launch in flight, or the one that just failed.
        if let Some(op) = &self.op {
            if let Some((sid, _)) = &op.session {
                if let Some(p) = self.sessions.get(sid).and_then(|s| s.progress.clone()) {
                    if op.kind == ops::OpKind::Reveal && op.is_new_login() {
                        return Some(p);
                    }
                }
            }
        }
        self.retained_failure.clone()
    }

    fn build_snapshot(&self) -> Snapshot {
        let users = self
            .users
            .values()
            .map(|u| {
                let mut p = u.profile.clone();
                let (a, s) = self.availability_of(p.uid);
                p.availability = a;
                p.session = s;
                p
            })
            .collect();
        let mode = match self.greeter_target() {
            Some(s) => GreeterMode::Unlock { session: s.id.clone(), uid: s.uid },
            None => GreeterMode::Login,
        };
        Snapshot {
            revision: 0,
            mode,
            selected_uid: self.selected_uid,
            users,
            progress: self.progress_view(),
            reduce_motion: self.policy.reduce_motion,
            hostname: self.policy.hostname.clone(),
        }
    }

    fn session_summaries(&self) -> Vec<SessionSummary> {
        self.sessions
            .values()
            .filter(|s| s.state != SessionState::Terminated)
            .map(|s| SessionSummary {
                id: s.id.clone(),
                generation: s.generation,
                uid: s.uid,
                state: s.state,
                availability: match s.state {
                    SessionState::Foreground => SessionAvailability::RunningForeground,
                    SessionState::LockRequested
                    | SessionState::LockedBackground
                    | SessionState::LockedGreeterForeground => SessionAvailability::RunningLocked,
                    SessionState::Failed | SessionState::Stopping => SessionAvailability::Unavailable,
                    _ => SessionAvailability::Starting,
                },
            })
            .collect()
    }

    fn compute_seat(&self) -> SeatState {
        if self.power_committed() {
            return SeatState::ShuttingDown;
        }
        if !self.compositor_ready {
            return SeatState::Booting;
        }
        if let Some(op) = &self.op {
            match op.kind {
                ops::OpKind::Reveal => {
                    if op.tstate == TransitionState::WaitingForAuth {
                        return SeatState::Authenticating;
                    }
                    if op.is_new_login() && matches!(op.tstate, TransitionState::WaitingForFrames) {
                        return SeatState::StartingSession;
                    }
                    return SeatState::RevealingSession;
                }
                ops::OpKind::Lock | ops::OpKind::SwitchUser if !op.locked_scene_ready => return SeatState::SecuringLock,
                ops::OpKind::Power if op.action == Some(bsm_proto::state::PowerAction::Suspend) && !op.committed => {
                    return SeatState::SecuringLock;
                }
                _ => {}
            }
        }
        if self.sessions.values().any(|s| s.state == SessionState::Foreground) {
            return SeatState::UserForeground;
        }
        if self.sessions.values().any(|s| s.state == SessionState::LockRequested) {
            return SeatState::SecuringLock;
        }
        if !self.greeter.wayland {
            return SeatState::Recovery;
        }
        if self.switching_user && self.sessions.values().any(|s| s.state.is_protected()) {
            return SeatState::SwitchingUser;
        }
        if self.greeter_target().is_some() {
            return SeatState::LockedGreeter;
        }
        SeatState::Greeter
    }

    fn persisted_view(&self) -> Persisted {
        Persisted {
            version: 1,
            last_used_uid: self.last_used_uid,
            sessions: self
                .sessions
                .values()
                .filter(|s| s.state.is_running() && !s.launching() && s.scope.is_some())
                .map(|s| PersistedSession {
                    id: s.id.clone(),
                    generation: s.generation,
                    uid: s.uid,
                    user_name: s.user_name.clone(),
                    scope: s.scope.clone().unwrap_or_default(),
                    host_pid: s.host_pid.unwrap_or(0),
                })
                .collect(),
        }
    }

    /// Recompute derived state and publish only what changed.
    fn settle(&mut self, fx: &mut Fx) {
        self.seat = self.compute_seat();

        // Persistence: last user and the registry of running sessions.
        let persisted = self.persisted_view();
        if self.published.persisted.as_ref() != Some(&persisted) {
            if self.published.persisted.is_some() || persisted.last_used_uid.is_some() || !persisted.sessions.is_empty() {
                fx.push(Effect::Persist(persisted.clone()));
            }
            self.published.persisted = Some(persisted);
        }

        // D-Bus signals.
        let sums = self.session_summaries();
        if sums != self.published.sessions {
            self.sessions_revision += 1;
            self.published.sessions = sums;
            fx.push(Effect::Dbus(DbusSignal::SessionsChanged { revision: self.sessions_revision }));
        }
        if self.published.selected != self.selected_uid {
            self.selected_revision += 1;
            self.published.selected = self.selected_uid;
            fx.push(Effect::Dbus(DbusSignal::SelectedUserChanged { revision: self.selected_revision, uid: self.selected_uid }));
        }
        let fg = self.foreground.as_ref().and_then(|id| self.sessions.get(id)).map(|s| (s.id.clone(), s.generation));
        if self.published.foreground != fg {
            fx.push(Effect::Dbus(DbusSignal::ForegroundChanged {
                session: fg.as_ref().map(|f| f.0.clone()),
                generation: fg.as_ref().map(|f| f.1),
            }));
            self.published.foreground = fg;
        }
        let mut locked: BTreeMap<SessionId, bool> = BTreeMap::new();
        let mut ready: BTreeMap<SessionId, bool> = BTreeMap::new();
        for s in self.sessions.values().filter(|s| s.state.is_running()) {
            locked.insert(s.id.clone(), s.state.is_protected());
            ready.insert(s.id.clone(), !s.launching());
        }
        for (id, l) in &locked {
            if self.published.locked.get(id) != Some(l) && (*l || self.published.locked.contains_key(id)) {
                fx.push(Effect::Dbus(DbusSignal::LockStateChanged { session: id.clone(), locked: *l }));
            }
        }
        for (id, was) in &self.published.locked {
            if *was && !locked.contains_key(id) {
                fx.push(Effect::Dbus(DbusSignal::LockStateChanged { session: id.clone(), locked: false }));
            }
        }
        self.published.locked = locked;
        for (id, r) in &ready {
            if self.published.ready.get(id) != Some(r) {
                let gn = self.sessions.get(id).map(|s| s.generation).unwrap_or(0);
                fx.push(Effect::Dbus(DbusSignal::SessionReadinessChanged { session: id.clone(), generation: gn, ready: *r }));
            }
        }
        self.published.ready = ready;

        // Greeter-facing progress and snapshot.
        let progress = self.progress_view();
        let pkey = progress.as_ref().map(|p| (p.session.clone(), p.generation, p.revision));
        if self.published.progress != pkey {
            if let Some(p) = progress.clone() {
                fx.push(Effect::Dbus(DbusSignal::SessionLaunchProgress { model: p.clone() }));
                fx.push(Effect::Greeter(ServerMessage::Progress(p)));
            }
            self.published.progress = pkey;
        }
        let snap = self.build_snapshot();
        if self.published.snapshot.as_ref() != Some(&snap) {
            self.revision += 1;
            let mut out = snap.clone();
            out.revision = self.revision;
            self.published.snapshot = Some(snap);
            fx.push(Effect::Greeter(ServerMessage::Snapshot(out)));
        }
    }

    /// The security invariants, as a checkable predicate. Run after every input in
    /// debug builds and by the tests.
    pub fn check_invariants(&self) -> Result<(), String> {
        if let InputOwner::Session(id) = &self.input_owner {
            let s = self.sessions.get(id).ok_or("input owner session does not exist")?;
            if s.state != SessionState::Foreground {
                return Err(format!("input owner {id} is {:?}, not Foreground", s.state));
            }
            if self.protected.contains(id) {
                return Err(format!("input owner {id} is protected"));
            }
            if self.foreground.as_ref() != Some(id) {
                return Err(format!("input owner {id} is not the foreground session"));
            }
        }
        let fg_count = self.sessions.values().filter(|s| s.state == SessionState::Foreground).count();
        if fg_count > 1 {
            return Err("more than one Foreground session".into());
        }
        for s in self.sessions.values() {
            if matches!(s.state, SessionState::LockedBackground | SessionState::LockedGreeterForeground)
                && !self.protected.contains(&s.id)
            {
                return Err(format!("{} is locked but not acknowledged protected", s.id));
            }
            if s.state == SessionState::Foreground && self.protected.contains(&s.id) {
                return Err(format!("{} is Foreground and protected", s.id));
            }
        }
        let mut seen = BTreeSet::new();
        for s in self.sessions.values().filter(|s| s.state.is_running()) {
            if !seen.insert(s.uid) {
                return Err(format!("two running sessions for uid {}", s.uid));
            }
        }
        if let Some(f) = &self.foreground {
            if !self.sessions.get(f).is_some_and(|s| s.state.is_running()) {
                return Err("foreground session is not running".into());
            }
        }
        if let Some(c) = &self.challenge {
            if self.selected_uid != Some(c.uid) {
                return Err("a challenge exists for an account that is not selected".into());
            }
            if self.op.is_none() {
                return Err("a challenge exists without an operation".into());
            }
        }
        Ok(())
    }

    pub(crate) fn user_known(&self, uid: u32) -> bool {
        self.users.contains_key(&uid)
    }
}
