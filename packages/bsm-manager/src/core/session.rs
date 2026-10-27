//! Session registry, the launch gate (progress and reveal evidence) and crash handling.

use std::collections::{BTreeMap, BTreeSet};

use bsm_proto::error::ErrorCode;
use bsm_proto::helper::{HelperEvent, HelperRequest};
use bsm_proto::progress::{ProgressFailure, ProgressModel, Stage};
use bsm_proto::scene::{HostReport, SceneCommand, SurfaceEvidence};
use bsm_proto::state::{Generation, InputOwner, SessionId, SessionState};

use super::tables;
use super::types::*;
use super::{Fx, Manager};

/// Evidence collected for the launch gate of one generation.
#[derive(Debug, Clone, Default)]
pub(crate) struct Launch {
    pub gnoblin_ready: bool,
    pub configured: BTreeSet<String>,
    pub bingux_launched: bool,
    pub mapped: BTreeMap<String, (u32, u32)>,
    /// Presented-frame counts per output at the moment the bridge reported `DesktopReady`.
    pub desktop_report: Option<BTreeMap<String, u64>>,
    /// `outputs_epoch` when `OutputsConfigured` was reached.
    pub configured_epoch: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct Session {
    pub id: SessionId,
    pub generation: Generation,
    pub uid: u32,
    pub user_name: String,
    pub state: SessionState,
    pub scope: Option<String>,
    pub host_pid: Option<u32>,
    pub logind: Option<String>,
    pub socket_path: Option<String>,
    /// Single-use ticket from the authentication that created this session.
    pub(crate) ticket: Option<u64>,
    pub(crate) open_sent: bool,
    pub(crate) open_req: Option<u64>,
    pub progress: Option<ProgressModel>,
    pub(crate) launch: Launch,
    pub surfaces: Vec<SurfaceEvidence>,
    pub host_connected: bool,
    /// Order of the last time this session was foreground; the most recent wins.
    pub fg_seq: u64,
    pub(crate) recovered: bool,
    pub(crate) armed_stage: Option<Stage>,
}

impl Session {
    pub fn launching(&self) -> bool {
        matches!(
            self.state,
            SessionState::Creating
                | SessionState::PamOpening
                | SessionState::GnoblinStarting
                | SessionState::BinguxStarting
                | SessionState::OutputReady
        )
    }
}

enum Eval {
    Reached,
    Wait,
    Count(u32, u32),
}

impl Manager {
    pub(super) fn set_session_state(&mut self, sid: &str, to: SessionState, fx: &mut Fx) -> bool {
        let Some(s) = self.sessions.get_mut(sid) else { return false };
        if !tables::session_legal(s.state, to) {
            let msg = format!("illegal session transition {:?} -> {:?} refused", s.state, to);
            Self::log(fx, LogLevel::Error, Some(sid), msg);
            return false;
        }
        s.state = to;
        true
    }

    /// Session of this id and generation, or `None` for a stale callback.
    pub(super) fn session_gen(&mut self, sid: &str, gn: Generation) -> Option<&mut Session> {
        self.sessions.get_mut(sid).filter(|s| s.generation == gn)
    }

    // ----- creation -----

    /// Create the session record for an authenticated new login and start its launch.
    pub(super) fn create_session(&mut self, uid: u32, generation: Generation, ticket: u64, fx: &mut Fx) -> SessionId {
        let id = self.alloc_session_id();
        let user_name = self.users.get(&uid).map(|u| u.profile.user_name.clone()).unwrap_or_default();
        let mut progress = ProgressModel::new(id.clone(), generation);
        progress.reach(Stage::AuthAccepted);
        self.sessions.insert(
            id.clone(),
            Session {
                id: id.clone(),
                generation,
                uid,
                user_name,
                state: SessionState::Creating,
                scope: None,
                host_pid: None,
                logind: None,
                socket_path: None,
                ticket: Some(ticket),
                open_sent: false,
                open_req: None,
                progress: Some(progress),
                launch: Launch::default(),
                surfaces: Vec::new(),
                host_connected: false,
                fg_seq: 0,
                recovered: false,
                armed_stage: None,
            },
        );
        fx.push(Effect::Scene(SceneCommand::RegisterSession {
            session: id.clone(),
            generation,
            uid,
            scope: String::new(),
            host_pid: 0,
            host_exe: self.policy.host_exe.clone(),
        }));
        self.arm_stage_timer(&id, fx);
        Self::log(fx, LogLevel::Info, Some(&id), format!("session created for uid {uid}"));
        id
    }

    pub(super) fn recover_session(&mut self, p: PersistedSession, fx: &mut Fx) {
        self.sessions.insert(
            p.id.clone(),
            Session {
                id: p.id.clone(),
                generation: p.generation,
                uid: p.uid,
                user_name: p.user_name.clone(),
                // Reopened protected, never foreground: the user authenticates to resume.
                state: SessionState::LockedBackground,
                scope: Some(p.scope.clone()),
                host_pid: Some(p.host_pid),
                logind: None,
                socket_path: None,
                ticket: None,
                open_sent: true,
                open_req: None,
                progress: None,
                launch: Launch::default(),
                surfaces: Vec::new(),
                host_connected: false,
                fg_seq: 0,
                recovered: true,
                armed_stage: None,
            },
        );
        self.protected.insert(p.id.clone());
        fx.push(Effect::Scene(SceneCommand::RegisterSession {
            session: p.id.clone(),
            generation: p.generation,
            uid: p.uid,
            scope: String::new(),
            host_pid: 0,
            host_exe: self.policy.host_exe.clone(),
        }));
        fx.push(Effect::SetTimer { id: TimerId::Recovery(p.id.clone(), p.generation), after: self.policy.recovery_reconnect });
        Self::log(fx, LogLevel::Info, Some(&p.id), "session recovered protected; waiting for its bridge to reconnect");
    }

    pub(super) fn on_recovery_timeout(&mut self, sid: &str, gn: Generation, fx: &mut Fx) {
        let Some(s) = self.session_gen(sid, gn) else { return };
        if s.recovered && !s.host_connected && s.state.is_running() {
            Self::log(fx, LogLevel::Warn, Some(sid), "recovered session did not reconnect; closing it");
            self.session_lost(sid, gn, "the recovered session did not reconnect", fx);
        }
    }

    // ----- helper and compositor events about sessions -----

    pub(super) fn on_socket_ready(&mut self, sid: &str, gn: Generation, path: String, fx: &mut Fx) {
        let output_hints: Vec<(u32, u32)> = self.outputs.iter().map(|o| (o.width, o.height)).collect();
        let req = self.helper_req();
        let Some(s) = self.session_gen(sid, gn) else {
            Self::log(fx, LogLevel::Warn, Some(sid), "stale SessionSocketReady ignored");
            return;
        };
        s.socket_path = Some(path.clone());
        if s.recovered {
            let (scope, pid) = (s.scope.clone().unwrap_or_default(), s.host_pid.unwrap_or(0));
            fx.push(Effect::Scene(SceneCommand::BindHost { session: sid.into(), generation: gn, scope, host_pid: pid }));
            fx.push(Effect::Scene(SceneCommand::Protect { session: sid.into(), generation: gn }));
            return;
        }
        if s.state != SessionState::Creating || s.open_sent {
            return;
        }
        let Some(ticket) = s.ticket.take() else { return };
        s.open_sent = true;
        s.open_req = Some(req);
        fx.push(Effect::Helper(HelperRequest::OpenSession {
            req_id: req,
            ticket,
            session: sid.into(),
            generation: gn,
            socket_path: path,
            output_hints,
        }));
        self.set_session_state(sid, SessionState::PamOpening, fx);
    }

    pub(super) fn on_helper(&mut self, e: HelperEvent, fx: &mut Fx) {
        match e {
            HelperEvent::Pong { .. } => {}
            HelperEvent::AuthPrompt { req_id, kind, text } => self.on_auth_prompt(req_id, kind, text, fx),
            HelperEvent::AuthDone { req_id, ok, code, message, ticket } => {
                self.on_auth_done(req_id, ok, code, message, ticket, fx)
            }
            HelperEvent::SessionOpened { req_id, session, generation, host_pid, leader_pid: _, scope, logind_session } => {
                self.on_session_opened(req_id, &session, generation, host_pid, scope, logind_session, fx)
            }
            HelperEvent::SessionFailed { req_id, session, generation, code, message } => {
                let current = self.sessions.get(&session).is_some_and(|s| s.generation == generation && s.open_req == Some(req_id));
                if current && self.sessions.get(&session).is_some_and(|s| s.launching()) {
                    self.fail_launch(&session, generation, code, &message, fx);
                } else {
                    Self::log(fx, LogLevel::Warn, Some(&session), "stale SessionFailed ignored");
                }
            }
            HelperEvent::SessionExited { session, generation, status } => {
                self.on_session_exited(&session, generation, status, fx)
            }
            HelperEvent::GreeterSpawned { req_id, generation, pid } => {
                self.on_helper_greeter_spawned(req_id, generation, pid, fx)
            }
            HelperEvent::GreeterExited { generation, status } => {
                if self.greeter.generation == Some(generation) {
                    Self::log(fx, LogLevel::Warn, None, format!("greeter exited with status {status}"));
                    self.greeter_lost(fx);
                }
            }
            HelperEvent::LockedHintResult { ok, message, .. } => {
                if !ok {
                    Self::log(fx, LogLevel::Warn, None, format!("SetLockedHint failed: {message}"));
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn on_session_opened(
        &mut self,
        req_id: u64,
        sid: &str,
        gn: Generation,
        host_pid: u32,
        scope: String,
        logind: Option<String>,
        fx: &mut Fx,
    ) {
        let ok = self
            .sessions
            .get(sid)
            .is_some_and(|s| s.generation == gn && s.state == SessionState::PamOpening && s.open_req == Some(req_id));
        if !ok {
            Self::log(fx, LogLevel::Warn, Some(sid), "stale SessionOpened ignored");
            return;
        }
        if scope.is_empty() || host_pid == 0 {
            self.fail_launch(sid, gn, ErrorCode::HelperFailure, "the helper reported a session without a scope or bridge pid", fx);
            return;
        }
        if let Some(s) = self.sessions.get_mut(sid) {
            s.scope = Some(scope.clone());
            s.host_pid = Some(host_pid);
            s.logind = logind;
        }
        fx.push(Effect::Scene(SceneCommand::BindHost { session: sid.into(), generation: gn, scope, host_pid }));
        self.reach_stage(sid, Stage::PamSession, fx);
        self.set_session_state(sid, SessionState::GnoblinStarting, fx);
        self.advance_launch(sid, fx);
    }

    pub(super) fn on_session_connected(&mut self, sid: &str, gn: Generation, pid: u32, fx: &mut Fx) {
        let Some(s) = self.session_gen(sid, gn) else {
            Self::log(fx, LogLevel::Warn, Some(sid), "stale SessionConnected ignored");
            return;
        };
        if !matches!(s.state, SessionState::GnoblinStarting) && !(s.recovered && s.state.is_running()) {
            let msg = format!("SessionConnected in state {:?} ignored", s.state);
            Self::log(fx, LogLevel::Warn, Some(sid), msg);
            return;
        }
        if s.host_pid.is_some_and(|p| p != 0 && p != pid) {
            Self::log(fx, LogLevel::Warn, Some(sid), "connected pid differs from the helper's bridge pid; ignored");
            return;
        }
        s.host_connected = true;
        if s.recovered {
            fx.push(Effect::CancelTimer(TimerId::Recovery(sid.into(), gn)));
            self.try_reveal(fx);
        } else {
            self.advance_launch(sid, fx);
        }
    }

    pub(super) fn on_connection_rejected(&mut self, sid: &str, gn: Generation, reason: String, fx: &mut Fx) {
        // Audit only. A probe by another process must not be able to fail a launch.
        let msg = format!("brokered connection rejected: {reason}");
        Self::log(fx, LogLevel::Warn, Some(sid), msg);
        let _ = gn;
    }

    pub(super) fn on_surfaces(&mut self, sid: &str, gn: Generation, surfaces: Vec<SurfaceEvidence>, fx: &mut Fx) {
        let Some(s) = self.session_gen(sid, gn) else { return };
        s.surfaces = surfaces;
        if s.launching() {
            self.advance_launch(sid, fx);
        } else {
            self.try_reveal(fx);
        }
    }

    pub(super) fn on_report(&mut self, sid: &str, gn: Generation, report: HostReport, fx: &mut Fx) {
        let names: Vec<String> = self.outputs.iter().map(|o| o.name.clone()).collect();
        let Some(s) = self.sessions.get_mut(sid).filter(|s| s.generation == gn) else {
            Self::log(fx, LogLevel::Warn, Some(sid), "report for a stale generation ignored");
            return;
        };
        if !s.launching() || s.state == SessionState::Creating || s.state == SessionState::PamOpening {
            Self::log(fx, LogLevel::Warn, Some(sid), "report outside the launch window ignored");
            return;
        }
        match report {
            HostReport::GnoblinReady => {
                if s.host_connected {
                    s.launch.gnoblin_ready = true;
                }
            }
            HostReport::OutputConfigured { output } => {
                if s.launch.gnoblin_ready && names.contains(&output) {
                    s.launch.configured.insert(output);
                }
            }
            HostReport::BinguxLaunched => {
                if s.launch.gnoblin_ready {
                    s.launch.bingux_launched = true;
                }
            }
            HostReport::SurfacesMapped { output, mapped, required } => {
                if s.launch.gnoblin_ready && names.contains(&output) {
                    s.launch.mapped.insert(output, (mapped, required));
                }
            }
            HostReport::DesktopReady => {
                // The freshness anchor: only valid once the surfaces stage is reached, so the
                // frames that count are strictly later than a report made with surfaces mapped.
                if s.progress.as_ref().and_then(|p| p.pending()) == Some(Stage::DesktopReady) {
                    let snap = names
                        .iter()
                        .map(|n| (n.clone(), s.surfaces.iter().find(|e| &e.output == n).map_or(0, |e| e.presented_frames)))
                        .collect();
                    s.launch.desktop_report = Some(snap);
                } else {
                    Self::log(fx, LogLevel::Warn, Some(sid), "DesktopReady before the surfaces stage ignored");
                }
            }
        }
        self.advance_launch(sid, fx);
    }

    pub(super) fn on_session_gone(&mut self, sid: &str, gn: Generation, fx: &mut Fx) {
        let Some(s) = self.session_gen(sid, gn) else { return };
        if !s.state.is_running() {
            return;
        }
        s.host_connected = false;
        if s.recovered {
            // Reconnect window; the recovery timer decides.
            return;
        }
        self.session_lost(sid, gn, "the session's bridge connection closed", fx);
    }

    fn on_session_exited(&mut self, sid: &str, gn: Generation, status: i32, fx: &mut Fx) {
        let Some(s) = self.session_gen(sid, gn) else {
            Self::log(fx, LogLevel::Warn, Some(sid), "SessionExited for an unknown session ignored");
            return;
        };
        match s.state {
            SessionState::Stopping | SessionState::Failed => self.terminate_session(sid, fx),
            _ => {
                let msg = format!("the session exited unexpectedly (status {status})");
                self.session_lost(sid, gn, &msg, fx);
            }
        }
    }

    // ----- the launch gate -----

    pub(super) fn reach_stage(&mut self, sid: &str, stage: Stage, fx: &mut Fx) -> bool {
        let Some(s) = self.sessions.get_mut(sid) else { return false };
        let Some(p) = s.progress.as_mut() else { return false };
        if !p.reach(stage) {
            return false;
        }
        let msg = format!("stage reached: {stage:?}");
        Self::log(fx, LogLevel::Info, Some(sid), msg);
        true
    }

    fn evaluate(&self, s: &Session, stage: Stage, names: &[String]) -> Eval {
        let total = names.len() as u32;
        let evidence = |n: &String| s.surfaces.iter().find(|e| &e.output == n);
        match stage {
            // Reached explicitly by the events that carry their evidence.
            Stage::AuthAccepted | Stage::PamSession => Eval::Wait,
            Stage::HostConnected => {
                if s.host_connected && s.state == SessionState::GnoblinStarting { Eval::Reached } else { Eval::Wait }
            }
            Stage::GnoblinReady => {
                if s.launch.gnoblin_ready { Eval::Reached } else { Eval::Wait }
            }
            Stage::OutputsConfigured => {
                let done = names.iter().filter(|n| s.launch.configured.contains(*n) && evidence(n).is_some_and(|e| e.acked)).count() as u32;
                if total > 0 && done == total { Eval::Reached } else { Eval::Count(done, total) }
            }
            Stage::BinguxLaunched => {
                if s.launch.bingux_launched { Eval::Reached } else { Eval::Wait }
            }
            Stage::SurfacesMapped => {
                let done = names
                    .iter()
                    .filter(|n| s.launch.mapped.get(*n).is_some_and(|(m, r)| *r > 0 && m >= r))
                    .count() as u32;
                if total > 0 && done == total { Eval::Reached } else { Eval::Count(done, total) }
            }
            Stage::DesktopReady => {
                let Some(snap) = &s.launch.desktop_report else { return Eval::Count(0, total) };
                let done = names
                    .iter()
                    .filter(|n| {
                        evidence(n).is_some_and(|e| e.acked && e.presented_frames > snap.get(*n).copied().unwrap_or(0))
                    })
                    .count() as u32;
                if total > 0 && done == total { Eval::Reached } else { Eval::Count(done, total) }
            }
            Stage::ReadyToReveal => {
                if s.launch.configured_epoch == Some(self.outputs_epoch) { Eval::Reached } else { Eval::Wait }
            }
        }
    }

    /// Advance as many stages as the evidence allows, in order, then refresh counts and timers.
    pub(super) fn advance_launch(&mut self, sid: &str, fx: &mut Fx) {
        let names: Vec<String> = self.outputs.iter().map(|o| o.name.clone()).collect();
        loop {
            let Some(s) = self.sessions.get(sid) else { return };
            if !s.launching() {
                break;
            }
            let Some(stage) = s.progress.as_ref().and_then(|p| p.pending()) else { break };
            match self.evaluate(s, stage, &names) {
                Eval::Reached => {
                    if !self.reach_stage(sid, stage, fx) {
                        break;
                    }
                    match stage {
                        Stage::GnoblinReady => {
                            self.set_session_state(sid, SessionState::BinguxStarting, fx);
                        }
                        Stage::OutputsConfigured => {
                            let epoch = self.outputs_epoch;
                            if let Some(s) = self.sessions.get_mut(sid) {
                                s.launch.configured_epoch = Some(epoch);
                            }
                            self.set_session_state(sid, SessionState::OutputReady, fx);
                        }
                        Stage::ReadyToReveal => {
                            self.set_session_state(sid, SessionState::ReadyBackground, fx);
                        }
                        _ => {}
                    }
                }
                Eval::Count(done, total) => {
                    if let Some(p) = self.sessions.get_mut(sid).and_then(|s| s.progress.as_mut()) {
                        p.count(stage, done, total);
                    }
                    break;
                }
                Eval::Wait => break,
            }
        }
        self.arm_stage_timer(sid, fx);
        if self.sessions.get(sid).is_some_and(|s| s.state == SessionState::ReadyBackground) {
            self.try_reveal(fx);
        }
    }

    /// Keep exactly one timeout armed, for the stage that is pending.
    pub(super) fn arm_stage_timer(&mut self, sid: &str, fx: &mut Fx) {
        let Some(s) = self.sessions.get_mut(sid) else { return };
        let pending = if s.launching() { s.progress.as_ref().and_then(|p| p.pending()) } else { None };
        if s.armed_stage == pending {
            return;
        }
        if let Some(old) = s.armed_stage.take() {
            fx.push(Effect::CancelTimer(TimerId::Stage(sid.into(), s.generation, old)));
        }
        if let Some(stage) = pending {
            s.armed_stage = Some(stage);
            fx.push(Effect::SetTimer {
                id: TimerId::Stage(sid.into(), s.generation, stage),
                after: self.policy.stage_timeout_for(stage),
            });
        }
    }

    pub(super) fn on_stage_timeout(&mut self, sid: &str, gn: Generation, stage: Stage, fx: &mut Fx) {
        let Some(s) = self.session_gen(sid, gn) else { return };
        if !s.launching() || s.progress.as_ref().and_then(|p| p.pending()) != Some(stage) {
            return;
        }
        let msg = format!("timed out while {}", stage.label_while_pending().to_lowercase());
        self.fail_launch(sid, gn, ErrorCode::Timeout, &msg, fx);
    }

    /// Displays changed. A launch that already configured its outputs cannot be revealed.
    pub(super) fn on_outputs_changed(&mut self, fx: &mut Fx) {
        let launching: Vec<(SessionId, Generation, bool)> = self
            .sessions
            .values()
            .filter(|s| s.launching() && s.state != SessionState::Creating)
            .map(|s| (s.id.clone(), s.generation, s.launch.configured_epoch.is_some()))
            .collect();
        for (sid, gn, configured) in launching {
            if configured {
                self.fail_launch(&sid, gn, ErrorCode::OutputNotReady, "the displays changed while the session started", fx);
            } else {
                self.advance_launch(&sid, fx);
            }
        }
        self.try_reveal(fx);
    }

    // ----- failure and teardown -----

    pub(super) fn fail_launch(&mut self, sid: &str, gn: Generation, code: ErrorCode, message: &str, fx: &mut Fx) {
        let Some(s) = self.session_gen(sid, gn) else { return };
        if !s.launching() {
            return;
        }
        let stage = s.progress.as_ref().and_then(|p| p.pending()).unwrap_or(Stage::ReadyToReveal);
        if let Some(p) = s.progress.as_mut() {
            p.fail(ProgressFailure { stage, code, message: message.to_owned() });
        }
        let model = s.progress.clone();
        let msg = format!("launch failed at {stage:?}: {code:?}: {message}");
        Self::log(fx, LogLevel::Error, Some(sid), msg);
        self.retained_failure = model;
        self.begin_cleanup(sid, SessionState::Failed, fx);
        // The reveal operation that depended on this launch ends visibly failed.
        if self.op.as_ref().is_some_and(|o| o.session.as_ref().is_some_and(|(s, g)| s == sid && *g == gn)) {
            self.op_fail(code, fx);
        }
    }

    /// Move to Failed or Stopping and start cleanup: hide, close, and wait for the exit.
    pub(super) fn begin_cleanup(&mut self, sid: &str, to: SessionState, fx: &mut Fx) {
        let Some(s) = self.sessions.get(sid) else { return };
        let gn = s.generation;
        let armed = s.armed_stage;
        if !self.set_session_state(sid, to, fx) {
            return;
        }
        if let Some(stage) = armed {
            fx.push(Effect::CancelTimer(TimerId::Stage(sid.into(), gn, stage)));
        }
        if let Some(s) = self.sessions.get_mut(sid) {
            s.armed_stage = None;
        }
        fx.push(Effect::CancelTimer(TimerId::Recovery(sid.into(), gn)));
        fx.push(Effect::Scene(SceneCommand::Protect { session: sid.into(), generation: gn }));
        fx.push(Effect::Helper(HelperRequest::CloseSession { session: sid.into(), generation: gn, grace_ms: self.policy.close_grace_ms }));
        fx.push(Effect::SetTimer {
            id: TimerId::Close(sid.into(), gn),
            after: self.policy.close_timeout + std::time::Duration::from_millis(self.policy.close_grace_ms as u64),
        });
        if self.foreground.as_deref() == Some(sid) {
            self.foreground = None;
        }
        self.protected.remove(sid);
        if matches!(&self.input_owner, InputOwner::Session(o) if o == sid) {
            self.input_owner = InputOwner::Greeter;
            fx.push(Effect::Scene(SceneCommand::SetInputOwner(InputOwner::Greeter)));
        }
    }

    pub(super) fn on_close_timeout(&mut self, sid: &str, gn: Generation, fx: &mut Fx) {
        if self.session_gen(sid, gn).is_some_and(|s| matches!(s.state, SessionState::Stopping | SessionState::Failed)) {
            Self::log(fx, LogLevel::Warn, Some(sid), "session did not report its exit in time; removing it");
            self.terminate_session(sid, fx);
        }
    }

    pub(super) fn terminate_session(&mut self, sid: &str, fx: &mut Fx) {
        let Some(s) = self.sessions.get(sid) else { return };
        let gn = s.generation;
        if !self.set_session_state(sid, SessionState::Terminated, fx) {
            return;
        }
        fx.push(Effect::CancelTimer(TimerId::Close(sid.into(), gn)));
        fx.push(Effect::Scene(SceneCommand::UnregisterSession { session: sid.into(), generation: gn }));
        self.protected.remove(sid);
        self.sessions.remove(sid);
        Self::log(fx, LogLevel::Info, Some(sid), "session terminated");
        self.finish_bg_logout(sid, gn, fx);
    }

    /// The session died or its bridge went away without being asked. Only this session
    /// is affected; the seat returns to the greeter.
    pub(super) fn session_lost(&mut self, sid: &str, gn: Generation, why: &str, fx: &mut Fx) {
        let Some(s) = self.session_gen(sid, gn) else { return };
        if !s.state.is_running() {
            return;
        }
        if s.launching() {
            self.fail_launch(sid, gn, ErrorCode::HelperFailure, why, fx);
            return;
        }
        let uid = s.uid;
        let was_visible = matches!(s.state, SessionState::Foreground | SessionState::LockRequested);
        let was_greeter_target = s.state == SessionState::LockedGreeterForeground;
        Self::log(fx, LogLevel::Error, Some(sid), format!("session lost: {why}"));
        // Anything in flight that involved it ends at once, and the scene falls back to the greeter.
        let involved = self.op.as_ref().is_some_and(|o| o.session.as_ref().is_some_and(|(s, g)| s == sid && *g == gn));
        if involved {
            self.op_abort(ErrorCode::TransitionInterrupted, fx);
        }
        self.begin_cleanup(sid, SessionState::Failed, fx);
        if self.sleep_prep.as_mut().is_some_and(|set| set.remove(&(sid.to_owned(), gn))) {
            self.maybe_sleep_ready(fx);
        }
        if was_visible || was_greeter_target || involved {
            fx.push(Effect::Scene(SceneCommand::ShowGreeterOnly));
            if self.input_owner != InputOwner::Greeter {
                self.input_owner = InputOwner::Greeter;
                fx.push(Effect::Scene(SceneCommand::SetInputOwner(InputOwner::Greeter)));
            }
        }
        if self.selected_uid != Some(uid) && self.challenge.is_some() {
            // The identity on screen changes, so the conversation for the old one ends.
            self.fail_challenge(ErrorCode::Cancelled, "the displayed account changed", fx);
        }
        self.selected_uid = Some(uid);
        fx.push(Effect::Greeter(bsm_proto::control::ServerMessage::Error {
            request_id: None,
            code: ErrorCode::HelperFailure,
            message: "Your session ended unexpectedly.".into(),
        }));
    }

    /// True when every current output has an acknowledged surface with a presented frame.
    pub(super) fn surfaces_ready(&self, s: &Session) -> bool {
        s.host_connected
            && !self.outputs.is_empty()
            && self.outputs.iter().all(|o| s.surfaces.iter().any(|e| e.output == o.name && e.acked && e.presented_frames >= 1))
    }
}
