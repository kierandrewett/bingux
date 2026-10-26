//! Suspend, reboot and power off, and sleep preparation.
//!
//! Flow of a power operation:
//!
//! ```text
//! Requested -> Authorising   polkit (session callers) and the inhibitor query, both answered
//!           -> Preparing     suspend: protect every visible session and wait for the acks
//!           -> Animating     compositor-owned fade on every output
//!           -> Committed     the cover is up; logind is called now, not before
//!           -> Completed     logind accepted (the cover stays for reboot and power off)
//!           -> Failed        denial or logind failure: the authorised scene is restored
//! ```

use std::collections::BTreeSet;

use bsm_proto::error::{BsmError, ErrorCode};
use bsm_proto::scene::SceneCommand;
use bsm_proto::state::{
    Generation, InputOwner, OperationId, OperationPhase, PowerAction, SessionId, SessionState, TransitionKind,
    TransitionState,
};

use super::ops::{OpKind, Requester};
use super::types::*;
use super::{Fx, Manager};

/// Pseudo operation id for the timer that bounds sleep preparation started by logind.
const SLEEP_TIMER_OP: OperationId = 0;

impl Manager {
    /// True once the machine is on its way down and the cover must stay.
    pub(super) fn power_committed(&self) -> bool {
        self.shutting_down
            || self.op.as_ref().is_some_and(|o| {
                o.kind == OpKind::Power && o.committed && o.action != Some(PowerAction::Suspend)
            })
    }

    fn action_id(&self, action: PowerAction, caller: Option<&Caller>) -> String {
        let base = match action {
            PowerAction::Suspend => "suspend",
            PowerAction::Reboot => "reboot",
            PowerAction::PowerOff => "power-off",
        };
        // Other users' sessions are affected when anybody else has a running session.
        let own = caller.map(|c| c.uid);
        let multiple = self.sessions.values().any(|s| s.state.is_running() && Some(s.uid) != own);
        if multiple {
            format!("org.freedesktop.login1.{base}-multiple-sessions")
        } else {
            format!("org.freedesktop.login1.{base}")
        }
    }

    pub(super) fn request_power(
        &mut self,
        requester: Requester,
        action: PowerAction,
        caller: Option<Caller>,
        fx: &mut Fx,
    ) -> Result<OperationId, BsmError> {
        if self.power_committed() {
            return Err(BsmError::new(ErrorCode::Busy, "the system is already shutting down"));
        }
        let needs_polkit = matches!(requester, Requester::Session(_));
        if needs_polkit && caller.is_none() {
            return Err(BsmError::new(ErrorCode::Unauthorised, "unknown caller"));
        }
        self.interrupt_current(fx)?;
        let id = self.new_operation(OpKind::Power, requester, None, fx);
        if let Some(op) = self.op.as_mut() {
            op.action = Some(action);
            op.transition = if action == PowerAction::Suspend { TransitionKind::Suspend } else { TransitionKind::PowerFade };
            op.polkit = if needs_polkit { None } else { Some(true) };
        }
        self.op_set(Some(OperationPhase::Authorising), Some(TransitionState::WaitingForAuth), fx);
        if needs_polkit {
            let c = caller.expect("checked above");
            let action_id = self.action_id(action, Some(&c));
            fx.push(Effect::Polkit { operation: id, action_id, caller: c });
        }
        fx.push(Effect::QueryInhibitors { operation: id, action });
        fx.push(Effect::SetTimer { id: TimerId::Checks(id), after: self.policy.check_timeout });
        Ok(id)
    }

    pub(super) fn cancel_power_checks(&mut self, fx: &mut Fx) {
        if let Some(op) = &self.op {
            fx.push(Effect::CancelTimer(TimerId::Checks(op.id)));
        }
    }

    pub(super) fn on_polkit(&mut self, id: OperationId, authorised: bool, fx: &mut Fx) {
        let Some(op) = self.op.as_mut().filter(|o| o.id == id && o.kind == OpKind::Power && o.polkit.is_none()) else {
            return;
        };
        op.polkit = Some(authorised);
        if !authorised {
            Self::log(fx, LogLevel::Warn, None, "power action denied by polkit");
            self.op_fail(ErrorCode::Unauthorised, fx);
            return;
        }
        self.power_checks_done(fx);
    }

    pub(super) fn on_inhibitors(&mut self, id: OperationId, result: Result<Vec<Inhibitor>, BsmError>, fx: &mut Fx) {
        let Some(op) = self.op.as_ref().filter(|o| o.id == id && o.kind == OpKind::Power && o.inhibitors.is_none()) else {
            return;
        };
        let action = op.action.unwrap_or(PowerAction::PowerOff);
        match result {
            Err(e) => {
                Self::log(fx, LogLevel::Error, None, format!("inhibitor query failed: {}", e.message));
                self.op_fail(ErrorCode::LogindFailure, fx);
            }
            Ok(list) => {
                let what = if action == PowerAction::Suspend { "sleep" } else { "shutdown" };
                let blockers: Vec<Inhibitor> =
                    list.into_iter().filter(|i| i.mode == "block" && i.what.split(':').any(|w| w == what)).collect();
                if !blockers.is_empty() {
                    let who: Vec<&str> = blockers.iter().map(|b| b.who.as_str()).collect();
                    let message = format!("Blocked by: {}", who.join(", "));
                    if matches!(self.op.as_ref().map(|o| &o.requester), Some(Requester::Greeter)) {
                        Self::greeter_error(fx, None, ErrorCode::Inhibited, &message);
                    }
                    Self::log(fx, LogLevel::Warn, None, message);
                    self.op_fail(ErrorCode::Inhibited, fx);
                    return;
                }
                if let Some(op) = self.op.as_mut() {
                    op.inhibitors = Some(Vec::new());
                }
                self.power_checks_done(fx);
            }
        }
    }

    pub(super) fn on_checks_timeout(&mut self, id: OperationId, fx: &mut Fx) {
        if self.op.as_ref().is_some_and(|o| o.id == id && o.kind == OpKind::Power && o.tstate == TransitionState::WaitingForAuth) {
            Self::log(fx, LogLevel::Error, None, "polkit or the inhibitor query did not answer in time");
            self.op_fail(ErrorCode::Timeout, fx);
        }
    }

    fn power_checks_done(&mut self, fx: &mut Fx) {
        let Some(op) = &self.op else { return };
        if op.polkit != Some(true) || op.inhibitors.is_none() {
            return;
        }
        let (id, action) = (op.id, op.action.unwrap_or(PowerAction::PowerOff));
        fx.push(Effect::CancelTimer(TimerId::Checks(id)));
        self.op_set(Some(OperationPhase::Preparing), Some(TransitionState::WaitingForFrames), fx);
        match action {
            PowerAction::Suspend => {
                let waiting = self.protect_visible_sessions(fx);
                if let Some(op) = self.op.as_mut() {
                    op.waiting_protect = waiting.clone();
                }
                if waiting.is_empty() {
                    self.suspend_scene_ready(fx);
                }
            }
            PowerAction::Reboot | PowerAction::PowerOff => {
                let source = self
                    .sessions
                    .values()
                    .find(|s| s.state == SessionState::Foreground)
                    .map(|s| (s.id.clone(), s.generation));
                self.begin_transition(TransitionKind::PowerFade, source, None, Some(action), fx);
            }
        }
    }

    /// Protect every session that is on screen or about to be. Returns those whose
    /// acknowledgement is still awaited.
    fn protect_visible_sessions(&mut self, fx: &mut Fx) -> BTreeSet<(SessionId, Generation)> {
        let mut waiting = BTreeSet::new();
        let visible: Vec<(SessionId, Generation, u32, SessionState)> = self
            .sessions
            .values()
            .filter(|s| matches!(s.state, SessionState::Foreground | SessionState::LockRequested))
            .map(|s| (s.id.clone(), s.generation, s.uid, s.state))
            .collect();
        for (sid, gn, uid, state) in visible {
            if state == SessionState::Foreground {
                self.selected_uid = Some(uid);
                self.begin_protect(&sid, gn, fx);
            }
            waiting.insert((sid, gn));
        }
        waiting
    }

    pub(super) fn suspend_scene_ready(&mut self, fx: &mut Fx) {
        self.input_owner = InputOwner::Greeter;
        fx.push(Effect::Scene(SceneCommand::SetInputOwner(InputOwner::Greeter)));
        self.begin_transition(TransitionKind::Suspend, None, None, Some(PowerAction::Suspend), fx);
        if let Some(op) = self.op.as_mut() {
            op.locked_scene_ready = true;
        }
    }

    /// The compositor committed the cover or the lock scene: now, and only now, call logind.
    pub(super) fn commit_power(&mut self, fx: &mut Fx) {
        let Some(op) = self.op.clone() else { return };
        let action = op.action.unwrap_or(PowerAction::PowerOff);
        if action != PowerAction::Suspend {
            self.input_owner = InputOwner::None;
            fx.push(Effect::Scene(SceneCommand::SetInputOwner(InputOwner::None)));
        }
        fx.push(Effect::SystemPower { operation: op.id, action });
        fx.push(Effect::SetTimer { id: TimerId::Power(op.id), after: self.policy.power_timeout });
        self.op_set(Some(OperationPhase::Committed), Some(TransitionState::Committed), fx);
    }

    pub(super) fn on_power_result(&mut self, id: OperationId, result: Result<(), BsmError>, fx: &mut Fx) {
        let Some(op) = self.op.as_ref().filter(|o| o.id == id && o.kind == OpKind::Power && o.committed) else {
            return;
        };
        let action = op.action.unwrap_or(PowerAction::PowerOff);
        match result {
            Ok(()) => {
                // Accepted, which is not the same as done: the system stops on its own schedule,
                // and for reboot and power off the cover stays until it does.
                if action != PowerAction::Suspend {
                    self.shutting_down = true;
                }
                self.op_complete(fx);
            }
            Err(e) => {
                Self::log(fx, LogLevel::Error, None, format!("logind refused {action:?}: {}", e.message));
                let code = match e.code {
                    ErrorCode::Unauthorised | ErrorCode::Inhibited => e.code,
                    _ => ErrorCode::ShutdownRejected,
                };
                self.rollback_power_scene(fx);
                if action != PowerAction::Suspend {
                    fx.push(Effect::Scene(SceneCommand::CancelTransition { operation: id }));
                }
                self.op_fail(code, fx);
            }
        }
    }

    pub(super) fn on_power_timeout(&mut self, id: OperationId, fx: &mut Fx) {
        if id == SLEEP_TIMER_OP {
            if self.sleep_prep.is_some() {
                Self::log(fx, LogLevel::Error, None, "sleep preparation timed out; sessions were hidden and protection was requested");
                self.finish_sleep_prep(fx);
            }
            return;
        }
        self.on_power_result(id, Err(BsmError::new(ErrorCode::Timeout, "logind did not answer")), fx);
    }

    /// Give the physical input back to what it was before the cover was committed.
    pub(super) fn rollback_power_scene(&mut self, fx: &mut Fx) {
        let Some(op) = &self.op else { return };
        if op.kind != OpKind::Power || op.action == Some(PowerAction::Suspend) || !op.committed {
            return;
        }
        let prev = op.prev_input.clone();
        let restorable = match &prev {
            InputOwner::Session(s) => {
                self.sessions.get(s).is_some_and(|x| x.state == SessionState::Foreground) && !self.protected.contains(s)
            }
            _ => true,
        };
        let owner = if restorable { prev } else { InputOwner::Greeter };
        self.input_owner = owner.clone();
        fx.push(Effect::Scene(SceneCommand::SetInputOwner(owner)));
    }

    // ----- sleep preparation (logind PrepareForSleep) -----

    pub(super) fn on_prepare_for_sleep(&mut self, start: bool, fx: &mut Fx) {
        if start {
            if self.op.as_ref().is_some_and(|o| o.kind == OpKind::Power && o.action == Some(PowerAction::Suspend) && o.committed) {
                // Our own suspend: everything is protected and the lock scene is up.
                fx.push(Effect::SleepReady);
                return;
            }
            if self.sleep_prep.is_some() {
                return;
            }
            // Sleep cannot wait for a transition: stop it, hide everything, protect everything.
            if self.op.as_ref().is_some_and(|o| o.began && !o.committed) {
                self.op_abort(ErrorCode::TransitionInterrupted, fx);
            } else if self.op.as_ref().is_some_and(|o| !o.committed && !o.began) {
                let _ = self.interrupt_current(fx);
            }
            let waiting = self.protect_visible_sessions(fx);
            self.sleep_prep = Some(waiting);
            fx.push(Effect::SetTimer {
                id: TimerId::Power(SLEEP_TIMER_OP),
                after: self.policy.protect_timeout * 2,
            });
            self.maybe_sleep_ready(fx);
        } else {
            // Resume: nothing is trusted from before. Hide everything again, then show the
            // greeter; unlocking needs authentication as ever.
            fx.push(Effect::Scene(SceneCommand::SetDpms { on: true }));
            let protected: Vec<(SessionId, Generation)> =
                self.sessions.values().filter(|s| s.state.is_protected()).map(|s| (s.id.clone(), s.generation)).collect();
            for (session, generation) in protected {
                fx.push(Effect::Scene(SceneCommand::Protect { session, generation }));
            }
            if self.op.is_none() && self.greeter_authoritative() {
                fx.push(Effect::Scene(SceneCommand::ShowGreeterOnly));
                self.input_owner = InputOwner::Greeter;
                fx.push(Effect::Scene(SceneCommand::SetInputOwner(InputOwner::Greeter)));
            }
        }
    }

    pub(super) fn maybe_sleep_ready(&mut self, fx: &mut Fx) {
        if self.sleep_prep.as_ref().is_some_and(|w| w.is_empty()) {
            self.finish_sleep_prep(fx);
        }
    }

    fn finish_sleep_prep(&mut self, fx: &mut Fx) {
        self.sleep_prep = None;
        fx.push(Effect::CancelTimer(TimerId::Power(SLEEP_TIMER_OP)));
        if self.greeter_authoritative() {
            self.input_owner = InputOwner::Greeter;
            fx.push(Effect::Scene(SceneCommand::SetInputOwner(InputOwner::Greeter)));
        }
        fx.push(Effect::Scene(SceneCommand::ShowGreeterOnly));
        fx.push(Effect::SleepReady);
    }
}
