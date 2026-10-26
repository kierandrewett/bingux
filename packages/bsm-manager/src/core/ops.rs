//! Operations: one at a time, each with explicit phases, a transition state and
//! the compositor transition it drives. Lock, reveal (new login, unlock and resume)
//! and logout live here; power lives in `power.rs`.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use bsm_proto::error::{BsmError, ErrorCode};
use bsm_proto::helper::HelperRequest;
use bsm_proto::scene::{SceneCommand, TransitionPhaseEvent};
use bsm_proto::control::ServerMessage;
use bsm_proto::state::{
    Generation, InputOwner, OperationId, OperationPhase, PowerAction, SessionId, SessionState, TransitionKind,
    TransitionState,
};

use super::auth::Plan;
use super::tables;
use super::types::*;
use super::{Fx, Manager};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OpKind {
    /// New login, unlock or resume: the greeter is revealed away to show a session.
    Reveal,
    Lock,
    SwitchUser,
    Logout,
    Power,
}

/// Who asked for an operation. Only the requester (or root) may cancel it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Requester {
    Greeter,
    Session(SessionId),
    Root,
    /// bsm itself (for example sleep preparation started by logind).
    System,
}

#[derive(Debug, Clone)]
pub(super) struct Operation {
    pub id: OperationId,
    pub kind: OpKind,
    pub requester: Requester,
    pub transition: TransitionKind,
    pub phase: OperationPhase,
    pub tstate: TransitionState,
    /// The session this operation acts on (target of a reveal, subject of a lock or logout).
    pub session: Option<(SessionId, Generation)>,
    /// `BeginTransition` was sent to the compositor.
    pub began: bool,
    pub cancel_requested: bool,
    /// A power action failed after the cover was committed: the operation ends with this
    /// code once the compositor has taken the cover away again.
    pub pending_fail: Option<ErrorCode>,
    /// The irreversible point passed.
    pub committed: bool,
    pub reason: Option<ErrorCode>,
    pub new_login: bool,
    pub frames_armed: bool,
    pub locked_scene_ready: bool,
    pub await_exit: Option<(SessionId, Generation)>,
    // power
    pub action: Option<PowerAction>,
    pub polkit: Option<bool>,
    pub inhibitors: Option<Vec<Inhibitor>>,
    pub waiting_protect: BTreeSet<(SessionId, Generation)>,
    pub prev_input: InputOwner,
    #[allow(dead_code)]
    pub started: Instant,
}

impl Operation {
    pub fn is_new_login(&self) -> bool {
        self.kind == OpKind::Reveal && self.new_login
    }

    pub fn info(&self) -> OperationInfo {
        OperationInfo {
            id: self.id,
            kind: self.transition,
            phase: self.phase,
            state: self.tstate,
            reason: self.reason,
            session: self.session.as_ref().map(|s| s.0.clone()),
        }
    }
}

impl Manager {
    // ----- creation and phase bookkeeping -----

    pub(super) fn new_operation(
        &mut self,
        kind: OpKind,
        requester: Requester,
        session: Option<(SessionId, Generation)>,
        fx: &mut Fx,
    ) -> OperationId {
        let id = self.next_op;
        self.next_op += 1;
        let transition = match kind {
            OpKind::Reveal => TransitionKind::NewLoginFade,
            OpKind::Lock => TransitionKind::Lock,
            OpKind::SwitchUser => TransitionKind::SwitchUser,
            OpKind::Logout => TransitionKind::LogoutFade,
            OpKind::Power => TransitionKind::PowerFade,
        };
        let op = Operation {
            id,
            kind,
            requester,
            transition,
            phase: OperationPhase::Requested,
            tstate: TransitionState::Idle,
            session,
            began: false,
            cancel_requested: false,
            pending_fail: None,
            committed: false,
            reason: None,
            new_login: false,
            frames_armed: false,
            locked_scene_ready: false,
            await_exit: None,
            action: None,
            polkit: None,
            inhibitors: None,
            waiting_protect: BTreeSet::new(),
            prev_input: self.input_owner.clone(),
            started: self.now(),
        };
        self.op = Some(op);
        self.emit_op(fx);
        id
    }

    fn emit_op(&self, fx: &mut Fx) {
        let Some(op) = &self.op else { return };
        fx.push(Effect::Greeter(ServerMessage::Operation {
            operation_id: op.id,
            kind: op.transition,
            phase: op.phase,
            reason: op.reason,
        }));
        fx.push(Effect::Dbus(DbusSignal::TransitionChanged {
            operation_id: op.id,
            kind: op.transition,
            phase: op.phase,
            state: op.tstate,
            reason: op.reason,
        }));
    }

    /// Change phase and transition state through the legal tables. Returns false (and
    /// logs) for an illegal change, leaving the operation unchanged.
    pub(super) fn op_set(&mut self, phase: Option<OperationPhase>, tstate: Option<TransitionState>, fx: &mut Fx) -> bool {
        let Some(op) = self.op.as_mut() else { return false };
        if let Some(p) = phase {
            if p != op.phase && !tables::phase_legal(op.phase, p) {
                let msg = format!("illegal operation phase {:?} -> {:?} refused", op.phase, p);
                Self::log(fx, LogLevel::Error, None, msg);
                return false;
            }
        }
        if let Some(t) = tstate {
            if t != op.tstate && !tables::transition_legal(op.tstate, t) {
                let msg = format!("illegal transition state {:?} -> {:?} refused", op.tstate, t);
                Self::log(fx, LogLevel::Error, None, msg);
                return false;
            }
        }
        let mut changed = false;
        if let Some(p) = phase {
            changed |= p != op.phase;
            op.phase = p;
        }
        if let Some(t) = tstate {
            changed |= t != op.tstate;
            op.tstate = t;
        }
        if changed {
            self.emit_op(fx);
        }
        true
    }

    pub(super) fn op_authorising(&mut self, fx: &mut Fx) {
        self.op_set(Some(OperationPhase::Authorising), Some(TransitionState::WaitingForAuth), fx);
    }

    /// Final phases. The operation leaves `self.op`, timers are cancelled, history kept.
    fn op_finish(&mut self, phase: OperationPhase, tstate: TransitionState, reason: Option<ErrorCode>, fx: &mut Fx) {
        let Some(mut op) = self.op.take() else { return };
        // A conversation never outlives the operation it belongs to.
        if self.challenge.as_ref().is_some_and(|c| c.op == op.id) {
            let c = self.challenge.take().expect("checked");
            fx.push(Effect::CancelTimer(TimerId::Auth(c.id)));
            fx.push(Effect::Helper(HelperRequest::AuthAbort { req_id: c.helper_req }));
            fx.push(Effect::Greeter(ServerMessage::AuthResult {
                challenge_id: c.id,
                ok: false,
                code: Some(reason.unwrap_or(ErrorCode::Cancelled)),
                message: "the request ended".into(),
            }));
        }
        if !tables::phase_legal(op.phase, phase) && op.phase != phase {
            let msg = format!("illegal final phase {:?} -> {:?}; forcing the final state", op.phase, phase);
            Self::log(fx, LogLevel::Error, None, msg);
        }
        op.phase = phase;
        op.reason = reason;
        if tables::transition_legal(op.tstate, tstate) || op.tstate == tstate {
            op.tstate = tstate;
        }
        self.op = Some(op.clone());
        self.emit_op(fx);
        self.op = None;
        fx.push(Effect::Dbus(DbusSignal::OperationFinished {
            operation_id: op.id,
            success: phase == OperationPhase::Completed,
            reason,
        }));
        for t in [TimerId::Transition(op.id), TimerId::Frames(op.id), TimerId::Checks(op.id), TimerId::Power(op.id)] {
            fx.push(Effect::CancelTimer(t));
        }
        let mut info = op.info();
        if phase == OperationPhase::Completed {
            info.state = TransitionState::Idle;
        }
        self.history.push_back(info);
        while self.history.len() > self.policy.history_len {
            self.history.pop_front();
        }
        let msg = format!("operation {} finished: {:?} {:?}", op.id, phase, reason);
        Self::log(fx, LogLevel::Info, op.session.as_ref().map(|s| s.0.as_str()), msg);
    }

    pub(super) fn op_complete(&mut self, fx: &mut Fx) {
        self.op_finish(OperationPhase::Completed, TransitionState::Idle, None, fx);
    }

    pub(super) fn op_fail(&mut self, code: ErrorCode, fx: &mut Fx) {
        self.op_finish(OperationPhase::Failed, TransitionState::Failed, Some(code), fx);
    }

    pub(super) fn op_cancelled(&mut self, fx: &mut Fx) {
        self.op_finish(OperationPhase::Cancelled, TransitionState::Cancelled, Some(ErrorCode::Cancelled), fx);
    }

    /// End operation `id` after a refused or failed challenge. A cancelled challenge is a
    /// cancelled operation; anything else a failed one.
    pub(super) fn op_end_failed(&mut self, id: OperationId, code: ErrorCode, fx: &mut Fx) {
        if self.op.as_ref().is_none_or(|o| o.id != id) {
            return;
        }
        if code == ErrorCode::Cancelled {
            self.op_cancelled(fx);
        } else {
            self.op_fail(code, fx);
        }
    }

    /// Abort a running transition and fall back to the greeter scene: used when its source or
    /// target disappears or the compositor stops answering. The cover plus greeter is the
    /// authorised fallback; the last desktop buffer never is.
    pub(super) fn op_abort(&mut self, code: ErrorCode, fx: &mut Fx) {
        let Some(op) = &self.op else { return };
        if op.began {
            fx.push(Effect::Scene(SceneCommand::CancelTransition { operation: op.id }));
        }
        fx.push(Effect::Scene(SceneCommand::ShowGreeterOnly));
        self.op_fail(code, fx);
    }

    // ----- gate for new transitions -----

    /// May a new operation start now? A running compositor transition or an operation past its
    /// irreversible point is never interrupted; earlier phases are cancelled immediately.
    pub(super) fn interrupt_current(&mut self, fx: &mut Fx) -> Result<(), BsmError> {
        let Some(op) = &self.op else { return Ok(()) };
        if op.committed || op.began || matches!(op.kind, OpKind::Lock | OpKind::SwitchUser) {
            return Err(BsmError::new(ErrorCode::Busy, "another transition is running"));
        }
        self.cancel_immediate(fx);
        Ok(())
    }

    /// Cancel an operation that has not begun a transition. Nothing to undo on screen.
    fn cancel_immediate(&mut self, fx: &mut Fx) {
        let Some(op) = self.op.clone() else { return };
        match op.kind {
            OpKind::Reveal => {
                if self.challenge.as_ref().is_some_and(|c| c.op == op.id) {
                    self.fail_challenge(ErrorCode::Cancelled, "cancelled", fx);
                    return;
                }
                if op.is_new_login() {
                    if let Some((sid, gn)) = &op.session {
                        // Back from a launch in flight: the half-started session is closed.
                        if self.sessions.get(sid).is_some_and(|s| s.generation == *gn && s.launching()) {
                            self.begin_cleanup(sid, SessionState::Stopping, fx);
                        }
                    }
                }
                self.op_cancelled(fx);
            }
            OpKind::Power => {
                self.cancel_power_checks(fx);
                self.op_cancelled(fx);
            }
            _ => self.op_cancelled(fx),
        }
    }

    pub(super) fn cancel_operation(&mut self, who: &Requester, id: OperationId, fx: &mut Fx) -> Result<(), BsmError> {
        let Some(op) = &self.op else {
            // A failed launch kept for the greeter is dismissed this way ("Back").
            if matches!(who, Requester::Greeter | Requester::Root) && self.retained_failure.is_some() {
                self.retained_failure = None;
                return Ok(());
            }
            return Err(BsmError::new(ErrorCode::NoSession, "no such operation"));
        };
        if op.id != id {
            return Err(BsmError::new(ErrorCode::NoSession, "no such operation"));
        }
        if *who != Requester::Root && *who != op.requester {
            return Err(BsmError::new(ErrorCode::Unauthorised, "only the requester may cancel this operation"));
        }
        if op.committed {
            return Err(BsmError::new(ErrorCode::Busy, "the operation is past the point of no return"));
        }
        if matches!(op.kind, OpKind::Lock | OpKind::SwitchUser) {
            return Err(BsmError::new(ErrorCode::Busy, "a lock cannot be cancelled"));
        }
        if op.began {
            // The compositor decides: it answers with Cancelled (scene restored) or, if the
            // commit won the race, with Committed, which the manager then honours.
            if !op.cancel_requested {
                fx.push(Effect::Scene(SceneCommand::CancelTransition { operation: op.id }));
                if let Some(op) = self.op.as_mut() {
                    op.cancel_requested = true;
                }
            }
            return Ok(());
        }
        self.cancel_immediate(fx);
        Ok(())
    }

    // ----- lock and switch user -----

    /// Lock `sid`: protect first, wait for the acknowledgement, then show the greeter.
    pub(super) fn request_lock(
        &mut self,
        requester: Requester,
        sid: &str,
        kind: OpKind,
        fx: &mut Fx,
    ) -> Result<OperationId, BsmError> {
        let (gn, uid, state) = match self.sessions.get(sid) {
            Some(s) => (s.generation, s.uid, s.state),
            None => return Err(BsmError::new(ErrorCode::NoSession, "no such session")),
        };
        match state {
            SessionState::Foreground => {}
            SessionState::LockRequested | SessionState::LockedBackground | SessionState::LockedGreeterForeground => {
                return Err(BsmError::new(ErrorCode::AlreadyLocked, "the session is already locked"));
            }
            _ => return Err(BsmError::new(ErrorCode::Busy, "the session is not in the foreground")),
        }
        self.interrupt_current(fx)?;
        let id = self.new_operation(kind, requester, Some((sid.to_owned(), gn)), fx);
        self.op_set(Some(OperationPhase::Preparing), Some(TransitionState::WaitingForFrames), fx);
        if let Some(op) = self.op.as_mut() {
            op.waiting_protect.insert((sid.to_owned(), gn));
        }
        self.selected_uid = Some(uid);
        if kind == OpKind::SwitchUser {
            self.switching_user = true;
        }
        self.begin_protect(sid, gn, fx);
        Ok(id)
    }

    /// Revoke input and protect `sid`; the session becomes `LockRequested` and stays there
    /// until the compositor acknowledges.
    pub(super) fn begin_protect(&mut self, sid: &str, gn: Generation, fx: &mut Fx) {
        if matches!(&self.input_owner, InputOwner::Session(o) if o == sid) {
            self.input_owner = InputOwner::None;
            fx.push(Effect::Scene(SceneCommand::SetInputOwner(InputOwner::None)));
        }
        if self.sessions.get(sid).is_some_and(|s| s.state == SessionState::Foreground) {
            self.set_session_state(sid, SessionState::LockRequested, fx);
        }
        fx.push(Effect::Scene(SceneCommand::Protect { session: sid.to_owned(), generation: gn }));
        fx.push(Effect::SetTimer { id: TimerId::Protect(sid.to_owned(), gn), after: self.policy.protect_timeout });
    }

    pub(super) fn on_protected(&mut self, sid: &str, gn: Generation, fx: &mut Fx) {
        let Some(s) = self.session_gen(sid, gn) else {
            Self::log(fx, LogLevel::Warn, Some(sid), "stale Protected ignored");
            return;
        };
        match s.state {
            SessionState::LockRequested => {}
            SessionState::LockedBackground | SessionState::LockedGreeterForeground => return,
            _ => {
                Self::log(fx, LogLevel::Warn, Some(sid), "Protected for a session that did not ask for it ignored");
                return;
            }
        }
        let uid = s.uid;
        fx.push(Effect::CancelTimer(TimerId::Protect(sid.to_owned(), gn)));
        self.protected.insert(sid.to_owned());
        let next = if self.selected_uid == Some(uid) && self.greeter_target().is_none() {
            SessionState::LockedGreeterForeground
        } else {
            SessionState::LockedBackground
        };
        self.set_session_state(sid, next, fx);
        self.set_locked_hint(sid, true, fx);

        let mut ready = false;
        if let Some(op) = self.op.as_mut() {
            op.waiting_protect.remove(&(sid.to_owned(), gn));
            ready = op.waiting_protect.is_empty() && op.tstate == TransitionState::WaitingForFrames;
        }
        let kind = self.op.as_ref().map(|o| (o.kind, o.action));
        if ready {
            match kind {
                Some((OpKind::Lock | OpKind::SwitchUser, _)) => self.lock_scene_ready(fx),
                Some((OpKind::Power, Some(PowerAction::Suspend))) => self.suspend_scene_ready(fx),
                _ => {}
            }
        }
        if let Some(set) = self.sleep_prep.as_mut() {
            set.remove(&(sid.to_owned(), gn));
            self.maybe_sleep_ready(fx);
        }
    }

    pub(super) fn set_locked_hint(&mut self, sid: &str, locked: bool, fx: &mut Fx) {
        let Some(s) = self.sessions.get(sid) else { return };
        if s.logind.is_none() {
            return;
        }
        let (generation, session) = (s.generation, sid.to_owned());
        let req_id = self.helper_req();
        fx.push(Effect::Helper(HelperRequest::SetLockedHint { req_id, session, generation, locked }));
    }

    pub(super) fn on_protect_timeout(&mut self, sid: &str, gn: Generation, fx: &mut Fx) {
        if !self.session_gen(sid, gn).is_some_and(|s| s.state == SessionState::LockRequested) {
            return;
        }
        // Fail closed: the desktop is hidden at once and protection is asked for again.
        Self::log(fx, LogLevel::Error, Some(sid), "the compositor did not acknowledge Protect in time");
        fx.push(Effect::Scene(SceneCommand::ShowGreeterOnly));
        fx.push(Effect::Scene(SceneCommand::Protect { session: sid.to_owned(), generation: gn }));
        fx.push(Effect::SetTimer { id: TimerId::Protect(sid.to_owned(), gn), after: self.policy.protect_timeout });
        if self.op.as_ref().is_some_and(|o| o.waiting_protect.contains(&(sid.to_owned(), gn))) {
            self.op_fail(ErrorCode::Timeout, fx);
        }
    }

    fn lock_scene_ready(&mut self, fx: &mut Fx) {
        let Some(op) = self.op.clone() else { return };
        // Input goes to the greeter before any pixel of it is shown.
        self.input_owner = InputOwner::Greeter;
        fx.push(Effect::Scene(SceneCommand::SetInputOwner(InputOwner::Greeter)));
        let source = op.session.clone();
        self.begin_transition(op.transition, source, None, None, fx);
        if let Some(op) = self.op.as_mut() {
            op.locked_scene_ready = true;
        }
    }

    // ----- starting compositor transitions -----

    /// Send `BeginTransition` for the current operation and arm its watchdog.
    pub(super) fn begin_transition(
        &mut self,
        kind: TransitionKind,
        source: Option<(SessionId, Generation)>,
        target: Option<(SessionId, Generation)>,
        power: Option<PowerAction>,
        fx: &mut Fx,
    ) {
        let Some(op) = &self.op else { return };
        let id = op.id;
        let duration_ms = self.policy.duration_ms(kind);
        let uid = target.as_ref().or(source.as_ref()).and_then(|(s, _)| self.sessions.get(s)).map(|s| s.uid);
        let user_reduces = uid
            .and_then(|u| self.users.get(&u))
            .is_some_and(|u| u.profile.appearance.reduce_motion);
        let reduce_motion = self.policy.reduce_motion || user_reduces;
        fx.push(Effect::Scene(SceneCommand::BeginTransition {
            operation: id,
            kind,
            source,
            target,
            duration_ms,
            reduce_motion,
            power,
        }));
        fx.push(Effect::CancelTimer(TimerId::Frames(id)));
        fx.push(Effect::SetTimer {
            id: TimerId::Transition(id),
            after: Duration::from_millis(duration_ms as u64) + self.policy.transition_grace,
        });
        if let Some(op) = self.op.as_mut() {
            op.began = true;
            op.transition = kind;
        }
        self.op_set(None, Some(TransitionState::Prepared), fx);
    }

    pub(super) fn on_transition_timeout(&mut self, id: OperationId, fx: &mut Fx) {
        let Some(op) = &self.op else { return };
        if op.id != id {
            return;
        }
        if op.committed {
            // The point of no return passed; only the compositor's final report is missing.
            if op.kind != OpKind::Power {
                Self::log(fx, LogLevel::Warn, None, "the compositor never reported completion; finishing");
                self.op_complete(fx);
            }
            return;
        }
        Self::log(fx, LogLevel::Error, None, "transition timed out; falling back to the greeter");
        if self.input_owner != InputOwner::Greeter && self.sessions.values().all(|s| s.state != SessionState::Foreground) {
            self.input_owner = InputOwner::Greeter;
            fx.push(Effect::Scene(SceneCommand::SetInputOwner(InputOwner::Greeter)));
        }
        self.op_abort(ErrorCode::Timeout, fx);
    }

    // ----- reveal: new login, unlock, resume -----

    pub(super) fn on_auth_success(&mut self, plan: Plan, uid: u32, ticket: Option<u64>, fx: &mut Fx) {
        match plan {
            Plan::New { generation } => {
                let sid = self.create_session(uid, generation, ticket.unwrap_or(0), fx);
                if let Some(op) = self.op.as_mut() {
                    op.session = Some((sid, generation));
                    op.new_login = true;
                    op.transition = TransitionKind::NewLoginFade;
                }
                self.op_set(Some(OperationPhase::Preparing), Some(TransitionState::WaitingForFrames), fx);
            }
            Plan::Existing { session, generation } => {
                let first_login = self.sessions.get(&session).is_some_and(|s| s.state == SessionState::ReadyBackground);
                if let Some(op) = self.op.as_mut() {
                    op.session = Some((session, generation));
                    op.new_login = first_login;
                    op.transition = if first_login { TransitionKind::NewLoginFade } else { TransitionKind::UnlockSlideUp };
                }
                self.op_set(Some(OperationPhase::Preparing), Some(TransitionState::WaitingForFrames), fx);
                self.try_reveal(fx);
            }
        }
    }

    /// Begin the reveal transition if, and only if, the gate is satisfied: the right session
    /// and generation, revealable state, the greeter present underneath or above, and
    /// fresh presented frames on every current output.
    pub(super) fn try_reveal(&mut self, fx: &mut Fx) {
        let Some(op) = &self.op else { return };
        if op.kind != OpKind::Reveal || op.tstate != TransitionState::WaitingForFrames || op.began {
            return;
        }
        let Some((sid, gn)) = op.session.clone() else { return };
        let (id, new_login, frames_armed) = (op.id, op.new_login, op.frames_armed);
        let Some(s) = self.sessions.get(&sid).filter(|s| s.generation == gn) else {
            self.op_fail(ErrorCode::StaleGeneration, fx);
            return;
        };
        if !matches!(
            s.state,
            SessionState::ReadyBackground | SessionState::LockedBackground | SessionState::LockedGreeterForeground
        ) {
            // Still launching: the stage timers bound the wait.
            return;
        }
        let epoch_ok = !(new_login && s.launch.configured_epoch.is_some_and(|e| e != self.outputs_epoch));
        let ready = self.greeter.wayland && self.greeter_authoritative() && self.surfaces_ready(s);
        if !epoch_ok {
            self.fail_launch_ready(&sid, gn, ErrorCode::OutputNotReady, "the displays changed before the reveal", fx);
            return;
        }
        if !ready {
            if !frames_armed {
                fx.push(Effect::SetTimer { id: TimerId::Frames(id), after: self.policy.frames_timeout });
                if let Some(op) = self.op.as_mut() {
                    op.frames_armed = true;
                }
            }
            return;
        }
        let kind = if new_login { TransitionKind::NewLoginFade } else { TransitionKind::UnlockSlideUp };
        self.begin_transition(kind, None, Some((sid, gn)), None, fx);
    }

    /// A session that was ready can no longer be revealed: close it and fail the operation.
    fn fail_launch_ready(&mut self, sid: &str, _gen: Generation, code: ErrorCode, message: &str, fx: &mut Fx) {
        let msg = format!("reveal refused: {message}");
        Self::log(fx, LogLevel::Error, Some(sid), msg);
        self.begin_cleanup(sid, SessionState::Failed, fx);
        self.op_fail(code, fx);
    }

    pub(super) fn on_frames_timeout(&mut self, id: OperationId, fx: &mut Fx) {
        let Some(op) = &self.op else { return };
        if op.id != id || op.began || op.tstate != TransitionState::WaitingForFrames {
            return;
        }
        let code = if self.greeter.wayland { ErrorCode::OutputNotReady } else { ErrorCode::Timeout };
        Self::log(fx, LogLevel::Error, op.session.as_ref().map(|s| s.0.as_str()), "reveal gate not satisfied in time");
        self.op_fail(code, fx);
    }

    // ----- compositor transition events -----

    pub(super) fn on_transition_phase(&mut self, id: OperationId, phase: TransitionPhaseEvent, fx: &mut Fx) {
        let Some(op) = &self.op else {
            Self::log(fx, LogLevel::Warn, None, "transition event with no operation ignored");
            return;
        };
        if op.id != id {
            Self::log(fx, LogLevel::Warn, None, "transition event for another operation ignored");
            return;
        }
        if !op.began {
            // A compositor cannot commit what bsm never asked it to run.
            Self::log(fx, LogLevel::Warn, None, "transition event before BeginTransition ignored");
            return;
        }
        match phase {
            TransitionPhaseEvent::Prepared => {}
            TransitionPhaseEvent::Animating => {
                if !op.committed {
                    self.op_set(Some(OperationPhase::Animating), Some(TransitionState::Animating), fx);
                }
            }
            TransitionPhaseEvent::Committed => self.commit_op(fx),
            TransitionPhaseEvent::Completed => {
                if !self.op.as_ref().is_some_and(|o| o.committed) {
                    self.commit_op(fx);
                }
                self.complete_transition(fx);
            }
            TransitionPhaseEvent::Failed => self.transition_failed(ErrorCode::TransitionInterrupted, fx),
            TransitionPhaseEvent::Cancelled => {
                if self.op.as_ref().is_some_and(|o| o.cancel_requested) {
                    // The scene is restored: only now does input go back to where it was.
                    self.rollback_power_scene(fx);
                    match self.op.as_ref().and_then(|o| o.pending_fail) {
                        Some(code) => self.op_fail(code, fx),
                        None => self.op_cancelled(fx),
                    }
                } else {
                    self.transition_failed(ErrorCode::TransitionInterrupted, fx);
                }
            }
        }
    }

    /// The compositor aborted and restored a safe scene. Nothing was revealed.
    fn transition_failed(&mut self, code: ErrorCode, fx: &mut Fx) {
        let Some(op) = self.op.clone() else { return };
        match op.kind {
            OpKind::Lock | OpKind::SwitchUser => {
                // The session is protected; only the greeter animation failed.
                fx.push(Effect::Scene(SceneCommand::ShowGreeterOnly));
            }
            OpKind::Power => self.rollback_power_scene(fx),
            OpKind::Reveal | OpKind::Logout => {
                if self.input_owner != InputOwner::Greeter
                    && self.sessions.values().all(|s| s.state != SessionState::Foreground)
                {
                    self.input_owner = InputOwner::Greeter;
                    fx.push(Effect::Scene(SceneCommand::SetInputOwner(InputOwner::Greeter)));
                }
            }
        }
        self.op_fail(code, fx);
    }

    fn commit_op(&mut self, fx: &mut Fx) {
        let Some(op) = self.op.clone() else { return };
        if op.committed {
            return;
        }
        if let Some(o) = self.op.as_mut() {
            o.committed = true;
        }
        match op.kind {
            OpKind::Reveal => {
                let Some((sid, gn)) = op.session.clone() else { return };
                let valid = self.sessions.get(&sid).is_some_and(|s| {
                    s.generation == gn
                        && matches!(
                            s.state,
                            SessionState::ReadyBackground | SessionState::LockedBackground | SessionState::LockedGreeterForeground
                        )
                });
                if !valid {
                    // Source of the commit vanished: never reveal; fall back.
                    if let Some(o) = self.op.as_mut() {
                        o.committed = false;
                    }
                    self.op_abort(ErrorCode::TransitionInterrupted, fx);
                    return;
                }
                let uid = self.sessions[&sid].uid;
                self.fg_seq += 1;
                let seq = self.fg_seq;
                self.protected.remove(&sid);
                self.set_session_state(&sid, SessionState::Foreground, fx);
                if let Some(s) = self.sessions.get_mut(&sid) {
                    s.fg_seq = seq;
                    s.ticket = None;
                }
                self.foreground = Some(sid.clone());
                self.selected_uid = Some(uid);
                self.last_used_uid = Some(uid);
                self.switching_user = false;
                self.retained_failure = None;
                self.input_owner = InputOwner::Session(sid.clone());
                fx.push(Effect::Scene(SceneCommand::SetInputOwner(InputOwner::Session(sid.clone()))));
                self.set_locked_hint(&sid, false, fx);
                self.op_set(Some(OperationPhase::Committed), Some(TransitionState::Committed), fx);
            }
            OpKind::Lock | OpKind::SwitchUser => {
                self.op_set(Some(OperationPhase::Committed), Some(TransitionState::Committed), fx);
            }
            OpKind::Logout => {
                let Some((sid, _)) = op.session.clone() else { return };
                let uid = self.sessions.get(&sid).map(|s| s.uid);
                self.input_owner = InputOwner::Greeter;
                fx.push(Effect::Scene(SceneCommand::SetInputOwner(InputOwner::Greeter)));
                self.begin_cleanup(&sid, SessionState::Stopping, fx);
                if let Some(u) = uid {
                    self.selected_uid = Some(u);
                }
                self.op_set(Some(OperationPhase::Committed), Some(TransitionState::Committed), fx);
            }
            OpKind::Power => self.commit_power(fx),
        }
    }

    fn complete_transition(&mut self, fx: &mut Fx) {
        let Some(op) = &self.op else { return };
        match op.kind {
            // Reboot and power off and suspend finish when logind answers.
            OpKind::Power => {}
            _ => self.op_complete(fx),
        }
    }

    // ----- logout -----

    pub(super) fn request_logout(&mut self, requester: Requester, sid: &str, fx: &mut Fx) -> Result<OperationId, BsmError> {
        let (gn, state, uid) = match self.sessions.get(sid) {
            Some(s) if s.state.is_running() => (s.generation, s.state, s.uid),
            _ => return Err(BsmError::new(ErrorCode::NoSession, "no such session")),
        };
        self.interrupt_current(fx)?;
        let id = self.new_operation(OpKind::Logout, requester, Some((sid.to_owned(), gn)), fx);
        if state == SessionState::Foreground {
            self.op_set(Some(OperationPhase::Preparing), Some(TransitionState::WaitingForFrames), fx);
            self.begin_transition(TransitionKind::LogoutFade, Some((sid.to_owned(), gn)), None, None, fx);
            return Ok(id);
        }
        // Not on screen: nothing to animate. The session is closed and the operation ends
        // when its processes are gone.
        let was_target = state == SessionState::LockedGreeterForeground;
        let _ = (was_target, uid);
        if let Some(op) = self.op.as_mut() {
            op.committed = true;
            op.await_exit = Some((sid.to_owned(), gn));
        }
        self.op_set(Some(OperationPhase::Committed), Some(TransitionState::Prepared), fx);
        self.op_set(None, Some(TransitionState::Committed), fx);
        if self.sessions.get(sid).is_some_and(|s| s.state.is_running()) {
            self.begin_cleanup(sid, SessionState::Stopping, fx);
        }
        if self.foreground.as_deref() == Some(sid) {
            self.foreground = None;
        }
        Ok(id)
    }
}
