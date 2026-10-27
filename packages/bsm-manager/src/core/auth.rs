//! The greeter control protocol and authentication challenges.
//!
//! A challenge binds one PAM conversation to: the account (`uid`), the intent, the
//! launch generation or the existing session and generation it may act on, the
//! operation it belongs to and a deadline. Success is single use: the challenge is
//! taken out of the manager before anything is done with the result, so a repeated
//! or late `AuthDone` finds nothing. Success never reveals a desktop by itself; it
//! only lets the operation continue to its own gated transition.

use bsm_proto::control::{ClientMessage, PromptKind, ServerMessage};
use bsm_proto::error::ErrorCode;
use bsm_proto::helper::{HelperRequest, PamPromptKind};
use bsm_proto::state::{AuthIntent, Generation, SessionId, SessionState};

use super::ops::{OpKind, Requester};
use super::types::*;
use super::{Fx, Manager};

const MAX_RESPONSE_BYTES: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Plan {
    /// No session exists: launch a new one with this generation on success.
    New { generation: Generation },
    /// A session exists and is resumed (or unlocked) on success.
    Existing { session: SessionId, generation: Generation },
}

#[derive(Debug, Clone)]
pub(super) struct Challenge {
    pub id: u64,
    /// The greeter's `request_id`, echoed in the first prompt.
    pub request_id: u64,
    pub helper_req: u64,
    pub uid: u32,
    #[allow(dead_code)]
    pub intent: AuthIntent,
    pub op: u64,
    pub plan: Plan,
    pub first_prompt_sent: bool,
    pub awaiting_response: bool,
}

impl Manager {
    pub(super) fn greeter_error(fx: &mut Fx, request_id: Option<u64>, code: ErrorCode, message: &str) {
        fx.push(Effect::Greeter(ServerMessage::Error { request_id, code, message: message.to_owned() }));
    }

    // ----- connection management -----

    pub(super) fn on_greeter_peer(&mut self, conn: u64, uid: u32, pid: u32, fx: &mut Fx) {
        let ok = self.policy.greeter_uid == Some(uid)
            && self.greeter.pid == Some(pid)
            && self.greeter.generation.is_some()
            && self.greeter.control.is_none();
        if !ok {
            let msg = format!("greeter control connection refused (uid {uid}, pid {pid})");
            Self::log(fx, LogLevel::Warn, None, msg);
        } else {
            self.greeter.control = Some(conn);
            self.greeter.hello = false;
        }
        fx.push(Effect::ConnVerdict(ConnVerdict { conn, accept: ok }));
    }

    pub(super) fn on_greeter_disconnected(&mut self, conn: u64, fx: &mut Fx) {
        if self.greeter.control != Some(conn) {
            return;
        }
        self.greeter.control = None;
        self.greeter.hello = false;
        // Nobody is left to answer the prompts; the conversation cannot continue.
        if self.challenge.is_some() {
            self.fail_challenge(ErrorCode::Cancelled, "the greeter disconnected", fx);
        }
    }

    pub(super) fn on_greeter_message(&mut self, conn: u64, msg: ClientMessage, fx: &mut Fx) {
        if self.greeter.control != Some(conn) {
            return;
        }
        if let ClientMessage::Hello { version } = msg {
            if version != bsm_proto::PROTOCOL_VERSION {
                Self::greeter_error(fx, None, ErrorCode::InvalidArgument, "unsupported protocol version");
                fx.push(Effect::DropGreeter);
                self.greeter.control = None;
                return;
            }
            self.greeter.hello = true;
            // A full snapshot right after hello, whether or not anything changed.
            let mut snap = self.build_snapshot();
            snap.revision = self.revision;
            fx.push(Effect::Greeter(ServerMessage::Snapshot(snap)));
            if let Some(p) = self.progress_view() {
                fx.push(Effect::Greeter(ServerMessage::Progress(p)));
            }
            return;
        }
        if !self.greeter.hello {
            Self::greeter_error(fx, None, ErrorCode::Unauthorised, "hello is required first");
            return;
        }
        match msg {
            ClientMessage::Hello { .. } => {}
            ClientMessage::SelectUser { uid } => self.greeter_select_user(uid, fx),
            ClientMessage::BeginAuth { request_id, uid, intent } => self.greeter_begin_auth(request_id, uid, intent, fx),
            ClientMessage::AuthResponse { challenge_id, response } => self.greeter_auth_response(challenge_id, response, fx),
            ClientMessage::CancelAuth { challenge_id } => {
                if self.challenge.as_ref().is_some_and(|c| c.id == challenge_id) {
                    self.fail_challenge(ErrorCode::Cancelled, "cancelled", fx);
                } else {
                    Self::greeter_error(fx, None, ErrorCode::StaleGeneration, "no such challenge");
                }
            }
            ClientMessage::Power { request_id, action } => {
                if !self.greeter_authoritative() || self.policy.greeter_uid.is_none() {
                    Self::greeter_error(fx, Some(request_id), ErrorCode::Unauthorised, "the greeter is not active");
                } else if !self.policy.greeter_power.allows(action) {
                    Self::greeter_error(fx, Some(request_id), ErrorCode::Unauthorised, "policy does not allow this action here");
                } else if let Err(e) = self.request_power(Requester::Greeter, action, None, fx) {
                    Self::greeter_error(fx, Some(request_id), e.code, &e.message);
                }
            }
            ClientMessage::CancelOperation { operation_id } => {
                if let Err(e) = self.cancel_operation(&Requester::Greeter, operation_id, fx) {
                    Self::greeter_error(fx, None, e.code, &e.message);
                }
            }
        }
    }

    // ----- selection -----

    pub(super) fn greeter_select_user(&mut self, uid: u32, fx: &mut Fx) {
        if let Err(e) = self.select_user(uid, fx) {
            Self::greeter_error(fx, None, e.code, &e.message);
        }
    }

    /// Show another account's identity. Changes nothing but what the greeter shows (and,
    /// for a session whose unlock form was up, hides that form), never reveals a desktop.
    pub(super) fn select_user(&mut self, uid: u32, fx: &mut Fx) -> Result<(), bsm_proto::error::BsmError> {
        use bsm_proto::error::BsmError;
        if !self.greeter_authoritative() {
            return Err(BsmError::new(ErrorCode::Unauthorised, "the greeter is not active"));
        }
        if !self.user_known(uid) {
            return Err(BsmError::new(ErrorCode::InvalidAccount, "no such account"));
        }
        if self.selected_uid == Some(uid) {
            return Ok(());
        }
        match &self.op {
            Some(op) if op.tstate == bsm_proto::state::TransitionState::WaitingForAuth => {
                // A mid-flight change of the selected account cancels the challenge.
                self.fail_challenge(ErrorCode::Cancelled, "another account was selected", fx);
            }
            Some(_) => return Err(BsmError::new(ErrorCode::Busy, "an operation is in progress")),
            None => {}
        }
        self.retained_failure = None;
        self.selected_uid = Some(uid);
        if let Some(t) = self.greeter_target().map(|s| (s.id.clone(), s.uid)) {
            if t.1 != uid {
                // The unlock form of the other account goes away; its session stays protected.
                self.set_session_state(&t.0, bsm_proto::state::SessionState::LockedBackground, fx);
            }
        }
        Ok(())
    }

    // ----- authentication -----

    fn failure_cooldown_active(&self, uid: u32) -> bool {
        self.failures
            .get(&uid)
            .is_some_and(|(n, at)| *n >= self.policy.max_failures && self.now().duration_since(*at) < self.policy.failure_cooldown)
    }

    pub(super) fn greeter_begin_auth(&mut self, request_id: u64, uid: u32, intent: AuthIntent, fx: &mut Fx) {
        use bsm_proto::error::BsmError;
        let fail = |fx: &mut Fx, e: BsmError| Self::greeter_error(fx, Some(request_id), e.code, &e.message);
        if !self.greeter_authoritative() {
            return fail(fx, BsmError::new(ErrorCode::Unauthorised, "the greeter is not active"));
        }
        if !self.helper_up {
            return fail(fx, BsmError::new(ErrorCode::HelperFailure, "the authentication helper is not available"));
        }
        if !self.user_known(uid) {
            return fail(fx, BsmError::new(ErrorCode::InvalidAccount, "no such account"));
        }
        if self.failure_cooldown_active(uid) {
            return fail(fx, BsmError::new(ErrorCode::Busy, "too many failed attempts; wait before trying again"));
        }
        // The greeter must ask for the account it shows. A begin_auth for another account was
        // composed before a newer selection (for example `bsmctl select-user`) reached the
        // greeter: it is stale, and it must not undo that selection.
        if self.selected_uid != Some(uid) {
            return fail(fx, BsmError::new(ErrorCode::StaleGeneration, "the selected account changed; select it again"));
        }
        // A new request supersedes a conversation still waiting for answers.
        if self.op.as_ref().is_some_and(|o| o.tstate == bsm_proto::state::TransitionState::WaitingForAuth) {
            self.fail_challenge(ErrorCode::Cancelled, "superseded", fx);
        }
        if self.op.is_some() {
            return fail(fx, BsmError::new(ErrorCode::Busy, "an operation is in progress"));
        }

        // The PAM service follows the intent, as bsm-helper requires: Unlock uses the unlock
        // service, Login and Switch the login service, whatever the manager then decides to do.
        let service = match intent {
            AuthIntent::Unlock => self.policy.unlock_service.clone(),
            AuthIntent::Login | AuthIntent::Switch => self.policy.login_service.clone(),
        };
        let (plan, generation) = match intent {
            AuthIntent::Unlock => match self.greeter_target() {
                Some(s) if s.uid == uid => (Plan::Existing { session: s.id.clone(), generation: s.generation }, s.generation),
                _ => return fail(fx, BsmError::new(ErrorCode::NoSession, "no locked session for this account")),
            },
            AuthIntent::Login | AuthIntent::Switch => match self.running_session(uid) {
                None => {
                    let g = self.alloc_generation();
                    (Plan::New { generation: g }, g)
                }
                Some(s) => match s.state {
                    SessionState::LockedBackground | SessionState::LockedGreeterForeground | SessionState::ReadyBackground => {
                        (Plan::Existing { session: s.id.clone(), generation: s.generation }, s.generation)
                    }
                    _ => return fail(fx, BsmError::new(ErrorCode::Busy, "this account's session is not ready")),
                },
            },
        };
        if let Err(e) = self.select_user(uid, fx) {
            return fail(fx, e);
        }
        self.retained_failure = None;
        let subject = match &plan {
            Plan::Existing { session, generation } => Some((session.clone(), *generation)),
            Plan::New { .. } => None,
        };
        let op_id = self.new_operation(OpKind::Reveal, Requester::Greeter, subject, fx);
        let challenge_id = self.next_challenge;
        self.next_challenge += 1;
        let helper_req = self.helper_req();
        let user = self.users.get(&uid).map(|u| u.profile.user_name.clone()).unwrap_or_default();
        self.challenge = Some(Challenge {
            id: challenge_id,
            request_id,
            helper_req,
            uid,
            intent,
            op: op_id,
            plan,
            first_prompt_sent: false,
            awaiting_response: false,
        });
        self.op_authorising(fx);
        fx.push(Effect::Helper(HelperRequest::AuthStart { req_id: helper_req, user, uid, intent, generation, service }));
        fx.push(Effect::SetTimer { id: TimerId::Auth(challenge_id), after: self.policy.auth_timeout });
    }

    fn greeter_auth_response(&mut self, challenge_id: u64, response: String, fx: &mut Fx) {
        let Some(c) = self.challenge.as_mut().filter(|c| c.id == challenge_id) else {
            Self::greeter_error(fx, None, ErrorCode::StaleGeneration, "no such challenge");
            return;
        };
        if !c.awaiting_response {
            Self::greeter_error(fx, None, ErrorCode::Busy, "no answer is expected now");
            return;
        }
        if response.len() > MAX_RESPONSE_BYTES {
            Self::greeter_error(fx, None, ErrorCode::InvalidArgument, "answer too long");
            return;
        }
        c.awaiting_response = false;
        let req_id = c.helper_req;
        fx.push(Effect::Helper(HelperRequest::AuthResponse { req_id, response }));
    }

    pub(super) fn on_auth_prompt(&mut self, req_id: u64, kind: PamPromptKind, text: String, fx: &mut Fx) {
        let Some(c) = self.challenge.as_mut().filter(|c| c.helper_req == req_id) else {
            Self::log(fx, LogLevel::Warn, None, "stale PAM prompt ignored");
            return;
        };
        let kind = match kind {
            PamPromptKind::EchoOff => PromptKind::Secret,
            PamPromptKind::EchoOn => PromptKind::Visible,
            PamPromptKind::Info => PromptKind::Info,
            PamPromptKind::Error => PromptKind::Error,
        };
        c.awaiting_response = matches!(kind, PromptKind::Secret | PromptKind::Visible);
        let request_id = if c.first_prompt_sent { None } else { Some(c.request_id) };
        c.first_prompt_sent = true;
        let challenge_id = c.id;
        fx.push(Effect::Greeter(ServerMessage::AuthPrompt { request_id, challenge_id, kind, text }));
    }

    pub(super) fn on_auth_done(
        &mut self,
        req_id: u64,
        ok: bool,
        code: Option<ErrorCode>,
        message: String,
        ticket: Option<u64>,
        fx: &mut Fx,
    ) {
        if !self.challenge.as_ref().is_some_and(|c| c.helper_req == req_id) {
            Self::log(fx, LogLevel::Warn, None, "stale or duplicate AuthDone ignored");
            return;
        }
        // Single use: the challenge is gone before anything is decided.
        let c = self.challenge.take().expect("checked above");
        fx.push(Effect::CancelTimer(TimerId::Auth(c.id)));
        let result = |fx: &mut Fx, ok: bool, code: Option<ErrorCode>, message: &str| {
            fx.push(Effect::Greeter(ServerMessage::AuthResult { challenge_id: c.id, ok, code, message: message.to_owned() }));
        };
        if !ok {
            let n = self.failures.get(&c.uid).map_or(0, |f| f.0);
            if code != Some(ErrorCode::Cancelled) {
                self.failures.insert(c.uid, (n + 1, self.now()));
            }
            let code = code.unwrap_or(ErrorCode::PamFailure);
            result(fx, false, Some(code), &message);
            self.op_end_failed(c.op, code, fx);
            return;
        }
        self.failures.remove(&c.uid);
        // Validate that the world is still the one the challenge was bound to.
        let still_ok = self.selected_uid == Some(c.uid)
            && self.op.as_ref().is_some_and(|o| o.id == c.op)
            && self.greeter.control.is_some()
            && match &c.plan {
                Plan::New { .. } => ticket.is_some() && self.running_session(c.uid).is_none(),
                Plan::Existing { session, generation } => self.sessions.get(session).is_some_and(|s| {
                    s.generation == *generation
                        && matches!(
                            s.state,
                            SessionState::LockedBackground | SessionState::LockedGreeterForeground | SessionState::ReadyBackground
                        )
                }),
            };
        if !still_ok {
            result(fx, false, Some(ErrorCode::StaleGeneration), "the request is no longer valid");
            self.op_end_failed(c.op, ErrorCode::StaleGeneration, fx);
            return;
        }
        result(fx, true, None, "");
        self.on_auth_success(c.plan, c.uid, ticket, fx);
    }

    pub(super) fn on_auth_timeout(&mut self, id: u64, fx: &mut Fx) {
        if self.challenge.as_ref().is_some_and(|c| c.id == id) {
            self.fail_challenge(ErrorCode::Timeout, "the authentication timed out", fx);
        }
    }

    /// End the conversation in flight without success: tell the helper, tell the greeter,
    /// end the operation. Safe to call with no challenge (does nothing).
    pub(super) fn fail_challenge(&mut self, code: ErrorCode, message: &str, fx: &mut Fx) {
        let Some(c) = self.challenge.take() else { return };
        fx.push(Effect::CancelTimer(TimerId::Auth(c.id)));
        fx.push(Effect::Helper(HelperRequest::AuthAbort { req_id: c.helper_req }));
        fx.push(Effect::Greeter(ServerMessage::AuthResult {
            challenge_id: c.id,
            ok: false,
            code: Some(code),
            message: message.to_owned(),
        }));
        self.op_end_failed(c.op, code, fx);
    }
}
