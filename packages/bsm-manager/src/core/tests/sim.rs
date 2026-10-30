//! Test harness: a manager, a virtual clock, a fake compositor and a fake helper.

use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

use bsm_proto::control::{ClientMessage, ServerMessage};
use bsm_proto::error::{BsmError, ErrorCode};
use bsm_proto::helper::{HelperEvent, HelperRequest, PamPromptKind};
use bsm_proto::scene::{HostReport, OutputInfo, SceneCommand, SceneEvent, SurfaceEvidence, TransitionPhaseEvent};
use bsm_proto::state::{AuthIntent, Generation, InputOwner, SessionId, SessionState};

use crate::core::*;

pub const GREETER_UID: u32 = 900;
pub const GREETER_PID: u32 = 777;
pub const ALICE: u32 = 1000;
pub const BOB: u32 = 1001;
pub const CAROL: u32 = 1002;
pub const PASSWORD: &str = "correct horse";

pub fn output(name: &str, x: i32) -> OutputInfo {
    OutputInfo { name: name.into(), x, y: 0, width: 1920, height: 1080, scale_120: 120, refresh_mhz: 60000, transform: 0 }
}

pub fn accounts() -> Vec<AccountEntry> {
    [(ALICE, "alice", "Alice"), (BOB, "bob", "Bob"), (CAROL, "carol", "Carol")]
        .into_iter()
        .map(|(uid, n, d)| AccountEntry { uid, user_name: n.into(), display_name: d.into(), published: None })
        .collect()
}

#[derive(Default)]
pub struct Comp {
    /// Answer commands automatically.
    pub auto: bool,
    /// Do not run `BeginTransition` to completion (the test steps it by hand).
    pub hold_transitions: bool,
    /// Do not acknowledge `Protect`.
    pub withhold_protect: bool,
    /// Answer `RequestEvidence` with a fresh frame on every output.
    pub auto_evidence: bool,
}

#[derive(Default)]
pub struct Hlp {
    pub auto: bool,
    /// Do not answer `OpenSession`.
    pub hold_open: bool,
    /// Do not answer `AuthStart` with a prompt.
    pub hold_auth: bool,
}

pub struct Sim {
    pub m: Manager,
    base: Instant,
    pub t: Duration,
    pub timers: BTreeMap<TimerId, Duration>,
    pub all: Vec<Effect>,
    pub comp: Comp,
    pub helper: Hlp,
    pub replies: BTreeMap<CallId, Result<Reply, BsmError>>,
    next_call: u64,
    pub conn: u64,
    next_pid: u32,
    /// Sessions the fake bridge knows about: (generation, host pid).
    pub bridges: BTreeMap<SessionId, (Generation, u32)>,
    /// Frame counters the fake compositor keeps per session and output.
    pub frames: BTreeMap<(SessionId, String), u64>,
    pub acked: BTreeMap<(SessionId, String), bool>,
    pub outputs: Vec<OutputInfo>,
    /// Compositor-side scene as the tests observe it.
    pub transitions: Vec<(u64, TransitionPhaseEvent)>,
    pub last_auth_helper_req: u64,
}

impl Sim {
    pub fn new() -> Self {
        let policy = Policy { greeter_uid: Some(GREETER_UID), ..Policy::default() };
        Self::with_policy(policy)
    }

    pub fn with_policy(policy: Policy) -> Self {
        Self {
            m: Manager::new(policy, Box::new(SplitMix(42))),
            base: Instant::now(),
            t: Duration::ZERO,
            timers: BTreeMap::new(),
            all: Vec::new(),
            comp: Comp { auto: true, auto_evidence: true, ..Default::default() },
            helper: Hlp { auto: true, ..Default::default() },
            replies: BTreeMap::new(),
            next_call: 1,
            conn: 0,
            next_pid: 4000,
            bridges: BTreeMap::new(),
            frames: BTreeMap::new(),
            acked: BTreeMap::new(),
            outputs: vec![output("DP-1", 0), output("DP-2", 1920)],
            transitions: Vec::new(),
            last_auth_helper_req: 0,
        }
    }

    pub fn now(&self) -> Instant {
        self.base + self.t
    }

    /// Index into `all`, to look at only what happens afterwards.
    pub fn mark(&self) -> usize {
        self.all.len()
    }

    pub fn since(&self, mark: usize) -> &[Effect] {
        &self.all[mark..]
    }

    /// Feed one input and everything the fake components answer to the resulting effects.
    pub fn feed(&mut self, input: Input) {
        let mut queue: VecDeque<Input> = VecDeque::new();
        queue.push_back(input);
        let mut guard = 0;
        while let Some(i) = queue.pop_front() {
            guard += 1;
            assert!(guard < 5000, "event storm in the fake loop");
            let fx = self.m.handle(self.now(), i);
            for e in &fx {
                match e {
                    Effect::SetTimer { id, after } => {
                        self.timers.insert(id.clone(), self.t + *after);
                    }
                    Effect::CancelTimer(id) => {
                        self.timers.remove(id);
                    }
                    Effect::DbusReply { call, result } => {
                        self.replies.insert(*call, result.clone());
                    }
                    _ => {}
                }
                self.respond(e, &mut queue);
            }
            self.all.extend(fx);
        }
    }

    fn respond(&mut self, e: &Effect, q: &mut VecDeque<Input>) {
        match e {
            Effect::Scene(c) if self.comp.auto => self.comp_respond(c, q),
            Effect::Helper(r) if self.helper.auto => self.helper_respond(r, q),
            _ => {}
        }
    }

    fn comp_respond(&mut self, c: &SceneCommand, q: &mut VecDeque<Input>) {
        match c {
            SceneCommand::RegisterSession { session, generation, .. } => {
                q.push_back(Input::Scene(SceneEvent::SessionSocketReady {
                    session: session.clone(),
                    generation: *generation,
                    path: format!("/run/bsm/sessions/{session}-{generation:x}.sock"),
                }));
            }
            SceneCommand::Protect { session, generation } if !self.comp.withhold_protect => {
                q.push_back(Input::Scene(SceneEvent::Protected { session: session.clone(), generation: *generation }));
            }
            SceneCommand::BeginTransition { operation, .. } if !self.comp.hold_transitions => {
                for p in [
                    TransitionPhaseEvent::Prepared,
                    TransitionPhaseEvent::Animating,
                    TransitionPhaseEvent::Committed,
                    TransitionPhaseEvent::Completed,
                ] {
                    self.transitions.push((*operation, p));
                    q.push_back(Input::Scene(SceneEvent::TransitionPhase { operation: *operation, phase: p }));
                }
            }
            SceneCommand::RequestEvidence { session, .. } if self.comp.auto_evidence => {
                // A hidden, healthy Gnoblin commits a new frame when asked.
                for o in self.outputs.clone() {
                    if self.acked.get(&(session.clone(), o.name.clone())).copied().unwrap_or(false) {
                        *self.frames.entry((session.clone(), o.name)).or_insert(0) += 1;
                    }
                }
                q.push_back(Input::Scene(SceneEvent::SessionSurfaces {
                    session: session.clone(),
                    generation: self.bridges.get(session).map_or(0, |b| b.0),
                    surfaces: self
                        .outputs
                        .iter()
                        .filter_map(|o| {
                            let key = (session.clone(), o.name.clone());
                            self.acked.get(&key).map(|acked| SurfaceEvidence {
                                output: o.name.clone(),
                                configure_serial: 1,
                                acked: *acked,
                                presented_frames: self.frames.get(&key).copied().unwrap_or(0),
                            })
                        })
                        .collect(),
                }));
            }
            SceneCommand::CancelTransition { operation } if !self.comp.hold_transitions => {
                q.push_back(Input::Scene(SceneEvent::TransitionPhase {
                    operation: *operation,
                    phase: TransitionPhaseEvent::Cancelled,
                }));
            }
            _ => {}
        }
    }

    fn helper_respond(&mut self, r: &HelperRequest, q: &mut VecDeque<Input>) {
        match r {
            HelperRequest::SpawnGreeter { req_id, generation, .. } => {
                q.push_back(Input::Helper(HelperEvent::GreeterSpawned {
                    req_id: *req_id,
                    generation: *generation,
                    pid: GREETER_PID,
                }));
            }
            HelperRequest::AuthStart { req_id, .. } if !self.helper.hold_auth => {
                self.last_auth_helper_req = *req_id;
                q.push_back(Input::Helper(HelperEvent::AuthPrompt {
                    req_id: *req_id,
                    kind: PamPromptKind::EchoOff,
                    text: "Password:".into(),
                }));
            }
            HelperRequest::AuthStart { req_id, .. } => self.last_auth_helper_req = *req_id,
            HelperRequest::AuthResponse { req_id, response } => {
                let ok = response == PASSWORD;
                q.push_back(Input::Helper(HelperEvent::AuthDone {
                    req_id: *req_id,
                    ok,
                    code: if ok { None } else { Some(ErrorCode::PamFailure) },
                    message: String::new(),
                    ticket: if ok { Some(9000 + *req_id) } else { None },
                }));
            }
            HelperRequest::OpenSession { req_id, session, generation, .. } if !self.helper.hold_open => {
                self.next_pid += 1;
                self.bridges.insert(session.clone(), (*generation, self.next_pid));
                q.push_back(Input::Helper(HelperEvent::SessionOpened {
                    req_id: *req_id,
                    session: session.clone(),
                    generation: *generation,
                    host_pid: self.next_pid,
                    leader_pid: self.next_pid + 1,
                    scope: format!("bsm-session-{session}.scope"),
                    logind_session: Some("c1".into()),
                }));
            }
            HelperRequest::CloseSession { session, generation, .. } => {
                self.bridges.remove(session);
                q.push_back(Input::Helper(HelperEvent::SessionExited { session: session.clone(), generation: *generation, status: 0 }));
            }
            HelperRequest::SetLockedHint { req_id, .. } => {
                q.push_back(Input::Helper(HelperEvent::LockedHintResult { req_id: *req_id, ok: true, code: None, message: String::new() }));
            }
            _ => {}
        }
    }

    // ----- time -----

    /// Advance the clock, firing due timers in order.
    pub fn advance(&mut self, d: Duration) {
        let target = self.t + d;
        loop {
            let next = self.timers.iter().filter(|(_, at)| **at <= target).min_by_key(|(_, at)| **at).map(|(id, at)| (id.clone(), *at));
            let Some((id, at)) = next else { break };
            self.timers.remove(&id);
            self.t = at;
            self.feed(Input::Timer(id));
        }
        self.t = target;
    }

    // ----- scenario steps -----

    /// Cold start up to a connected, greeted greeter.
    pub fn boot(&mut self) {
        self.feed(Input::Start { persisted: Persisted::default() });
        self.feed(Input::Accounts(accounts()));
        self.feed(Input::HelperConnected);
        self.feed(Input::Scene(SceneEvent::Ready { outputs: self.outputs.clone() }));
        let ggen = self.m.greeter.generation.expect("greeter generation after spawn request");
        self.feed(Input::Scene(SceneEvent::GreeterConnected { generation: ggen, pid: GREETER_PID }));
        self.conn += 1;
        self.feed(Input::GreeterPeer { conn: self.conn, uid: GREETER_UID, pid: GREETER_PID });
        self.greeter(ClientMessage::Hello { version: bsm_proto::PROTOCOL_VERSION });
    }

    pub fn greeter(&mut self, message: ClientMessage) {
        let conn = self.conn;
        self.feed(Input::Greeter { conn, message });
    }

    pub fn select(&mut self, uid: u32) {
        self.greeter(ClientMessage::SelectUser { uid });
    }

    /// Greeter side of a complete conversation with the right password.
    pub fn authenticate(&mut self, uid: u32, intent: AuthIntent) {
        self.greeter(ClientMessage::BeginAuth { request_id: 77, uid, intent });
        let cid = self.last_challenge_id();
        self.greeter(ClientMessage::AuthResponse { challenge_id: cid, response: PASSWORD.into() });
    }

    /// The helper request id of the newest `AuthStart` (for tests that answer by hand).
    pub fn last_auth_helper_req_manual(&self) -> u64 {
        self.all
            .iter()
            .rev()
            .find_map(|e| match e {
                Effect::Helper(HelperRequest::AuthStart { req_id, .. }) => Some(*req_id),
                _ => None,
            })
            .expect("an AuthStart")
    }

    pub fn last_challenge_id(&self) -> u64 {
        self.all
            .iter()
            .rev()
            .find_map(|e| match e {
                Effect::Greeter(ServerMessage::AuthPrompt { challenge_id, .. }) => Some(*challenge_id),
                _ => None,
            })
            .expect("an auth prompt was sent")
    }

    /// The most recent session of `uid` (any state), by id.
    pub fn sid_of(&self, uid: u32) -> SessionId {
        self.m.sessions().filter(|s| s.uid == uid).max_by_key(|s| s.fg_seq).map(|s| s.id.clone()).expect("session")
    }

    pub fn gen_of(&self, sid: &str) -> Generation {
        self.m.session(sid).expect("session").generation
    }

    /// The fake bridge connects and runs the whole startup in the documented order.
    pub fn bridge_full(&mut self, sid: &str) {
        self.bridge_to_gnoblin_ready(sid);
        self.bridge_outputs(sid);
        self.bridge_bingux(sid);
        self.bridge_present(sid);
    }

    pub fn bridge_to_gnoblin_ready(&mut self, sid: &str) {
        let (gn, pid) = self.bridges[sid];
        self.feed(Input::Scene(SceneEvent::SessionConnected { session: sid.into(), generation: gn, pid }));
        self.report(sid, HostReport::GnoblinReady);
    }

    pub fn report(&mut self, sid: &str, report: HostReport) {
        let gn = self.bridges[sid].0;
        self.feed(Input::Scene(SceneEvent::SessionReport { session: sid.into(), generation: gn, report }));
    }

    pub fn evidence(&mut self, sid: &str) {
        let gn = self.bridges[sid].0;
        let surfaces = self
            .outputs
            .iter()
            .filter_map(|o| {
                let key = (sid.to_owned(), o.name.clone());
                self.acked.get(&key).map(|acked| SurfaceEvidence {
                    output: o.name.clone(),
                    configure_serial: 1,
                    acked: *acked,
                    presented_frames: self.frames.get(&key).copied().unwrap_or(0),
                })
            })
            .collect();
        self.feed(Input::Scene(SceneEvent::SessionSurfaces { session: sid.into(), generation: gn, surfaces }));
    }

    pub fn bridge_outputs(&mut self, sid: &str) {
        for o in self.outputs.clone() {
            self.acked.insert((sid.to_owned(), o.name.clone()), true);
            self.evidence(sid);
            self.report(sid, HostReport::OutputConfigured { output: o.name });
        }
    }

    pub fn bridge_bingux(&mut self, sid: &str) {
        self.report(sid, HostReport::BinguxLaunched);
        for o in self.outputs.clone() {
            self.report(sid, HostReport::SurfacesMapped { output: o.name, mapped: 3, required: 3 });
        }
    }

    /// DesktopReady, then a frame on every output.
    pub fn bridge_present(&mut self, sid: &str) {
        self.report(sid, HostReport::DesktopReady);
        for o in self.outputs.clone() {
            *self.frames.entry((sid.to_owned(), o.name)).or_insert(0) += 1;
        }
        self.evidence(sid);
    }

    /// Full login of `uid`: select, authenticate, start, reveal. Returns the session id.
    pub fn login(&mut self, uid: u32) -> SessionId {
        self.select(uid);
        self.authenticate(uid, AuthIntent::Login);
        let sid = self.sid_of(uid);
        self.bridge_full(&sid);
        sid
    }

    pub fn call(&mut self, caller: Caller, method: Method) -> Result<Reply, BsmError> {
        let id = self.next_call;
        self.next_call += 1;
        self.feed(Input::Dbus { call: id, caller, method });
        self.replies.remove(&id).expect("a reply was sent")
    }

    pub fn session_caller(&self, sid: &str) -> Caller {
        let s = self.m.session(sid).expect("session");
        Caller {
            uid: s.uid,
            pid: 5000,
            bus_name: format!(":1.{}", s.uid),
            cgroup: Some(format!("/user.slice/user-{}.slice/{}/app", s.uid, s.scope.clone().unwrap_or_default())),
        }
    }

    pub fn root() -> Caller {
        Caller { uid: 0, pid: 1, bus_name: ":1.1".into(), cgroup: Some("/system.slice/x.service".into()) }
    }

    pub fn plain_user(uid: u32) -> Caller {
        Caller { uid, pid: 6000, bus_name: ":1.99".into(), cgroup: Some(format!("/user.slice/user-{uid}.slice/session-3.scope")) }
    }

    // ----- observations -----

    pub fn state(&self, sid: &str) -> SessionState {
        self.m.session(sid).expect("session").state
    }

    pub fn scene_cmds(&self, mark: usize) -> Vec<SceneCommand> {
        self.since(mark)
            .iter()
            .filter_map(|e| match e {
                Effect::Scene(c) => Some(c.clone()),
                _ => None,
            })
            .collect()
    }

    pub fn helper_reqs(&self, mark: usize) -> Vec<HelperRequest> {
        self.since(mark)
            .iter()
            .filter_map(|e| match e {
                Effect::Helper(c) => Some(c.clone()),
                _ => None,
            })
            .collect()
    }

    pub fn greeter_msgs(&self, mark: usize) -> Vec<ServerMessage> {
        self.since(mark)
            .iter()
            .filter_map(|e| match e {
                Effect::Greeter(c) => Some(c.clone()),
                _ => None,
            })
            .collect()
    }

    pub fn input_owner(&self) -> InputOwner {
        self.m.input_owner().clone()
    }

    pub fn begin_transitions(&self, mark: usize) -> Vec<SceneCommand> {
        self.scene_cmds(mark).into_iter().filter(|c| matches!(c, SceneCommand::BeginTransition { .. })).collect()
    }

    pub fn set_input_owner_cmds(&self, mark: usize) -> Vec<InputOwner> {
        self.scene_cmds(mark)
            .into_iter()
            .filter_map(|c| match c {
                SceneCommand::SetInputOwner(o) => Some(o),
                _ => None,
            })
            .collect()
    }

    /// Hold compositor transitions so a test can step the phases by hand.
    pub fn hold(&mut self) {
        self.comp.hold_transitions = true;
    }

    pub fn phase(&mut self, operation: u64, phase: TransitionPhaseEvent) {
        self.feed(Input::Scene(SceneEvent::TransitionPhase { operation, phase }));
    }

    pub fn current_op(&self) -> OperationInfo {
        self.m.operation().expect("an operation in flight")
    }
}
