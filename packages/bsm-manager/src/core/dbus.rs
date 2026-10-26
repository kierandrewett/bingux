//! D-Bus method dispatch and per-caller authorisation.
//!
//! The runtime resolves the caller's uid, pid and cgroup from the bus and passes them
//! in `Caller`. The manager decides who the caller is: root, the greeter account, a
//! hosted session (cgroup inside that session's scope **and** the same uid) or an
//! ordinary local user. A caller never names its own role, uid or session. Every
//! method below states which principals may call it; anything else is `Unauthorised`.
//!
//! | method                 | root | greeter | own session | other local user |
//! | ---------------------- | ---- | ------- | ----------- | ---------------- |
//! | Get* (read only)       | yes  | yes     | yes         | yes              |
//! | SelectGreeterUser      | yes  | yes     | no          | no               |
//! | RequestLock            | any  | no      | own         | no               |
//! | RequestSwitchUser      | fg   | no      | own         | no               |
//! | RequestLogout          | any  | no      | own         | no               |
//! | RequestSuspend/Reboot/PowerOff | yes | no (control socket) | polkit | no   |
//! | CancelOperation        | any  | its own | its own     | no               |
//! | PublishAppearance      | no   | no      | own uid     | own uid          |

use bsm_proto::error::{BsmError, ErrorCode};
use bsm_proto::state::{OperationId, PowerAction, SessionId};

use super::ops::{OpKind, Requester};
use super::types::*;
use super::{Fx, Manager};

fn deny(why: &str) -> BsmError {
    BsmError::new(ErrorCode::Unauthorised, why)
}

fn valid_accent(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

impl Manager {
    /// Who is calling. Pure function of the caller's resolved identity and the registry.
    pub fn principal(&self, caller: &Caller) -> Principal {
        if caller.uid == 0 {
            return Principal::Root;
        }
        if self.policy.greeter_uid == Some(caller.uid) {
            return Principal::Greeter;
        }
        if let Some(cg) = &caller.cgroup {
            for s in self.sessions.values().filter(|s| s.state.is_running()) {
                let in_scope = s.scope.as_deref().is_some_and(|sc| cgroup_in_scope(cg, sc));
                if in_scope && s.uid == caller.uid {
                    return Principal::Session(s.id.clone());
                }
            }
        }
        Principal::User(caller.uid)
    }

    pub(super) fn on_dbus(&mut self, call: CallId, caller: Caller, method: Method, fx: &mut Fx) {
        let name = method.name();
        let who = self.principal(&caller);
        let result = self.dbus_call(call, &caller, &who, method, fx);
        if let Some(r) = result {
            if let Err(e) = &r {
                let msg = format!("{name} from uid {} refused: {}", caller.uid, e.code.as_str());
                Self::log(fx, LogLevel::Info, None, msg);
            }
            fx.push(Effect::DbusReply { call, result: r });
        }
    }

    /// `None` means the reply is sent later (appearance publication).
    fn dbus_call(
        &mut self,
        call: CallId,
        caller: &Caller,
        who: &Principal,
        method: Method,
        fx: &mut Fx,
    ) -> Option<Result<Reply, BsmError>> {
        Some(match method {
            Method::GetUsers => Ok(Reply::Users(self.user_infos())),
            Method::GetSessions => Ok(Reply::Sessions(self.session_infos())),
            Method::GetSelectedUser => Ok(Reply::Selected(self.selected_uid)),
            Method::GetForegroundSession => Ok(Reply::Foreground(
                self.foreground.as_ref().and_then(|f| self.sessions.get(f)).map(|s| (s.id.clone(), s.generation)),
            )),
            Method::GetStatus => Ok(Reply::Status(StatusInfo {
                seat: self.seat,
                selected_uid: self.selected_uid,
                last_used_uid: self.last_used_uid,
                foreground: self.foreground.clone(),
                revision: self.revision,
                operation: self.operation(),
                greeter_connected: self.greeter_control_connected(),
                helper_connected: self.helper_up,
                outputs: self.outputs.len(),
            })),
            Method::GetOutputs => Ok(Reply::Outputs(self.outputs.clone())),
            Method::GetTransitions => {
                let mut v: Vec<OperationInfo> = self.op.iter().map(|o| o.info()).collect();
                v.extend(self.history.iter().rev().cloned());
                Ok(Reply::Transitions(v))
            }
            Method::SelectGreeterUser { uid } => match who {
                Principal::Root | Principal::Greeter => self.select_user(uid, fx).map(|_| Reply::Done),
                _ => Err(deny("only root or the greeter may select the greeter's account")),
            },
            Method::RequestLock { session } => self.own_session_action(who, session, true).and_then(|(req, sid)| {
                self.request_lock(req, &sid, OpKind::Lock, fx).map(Reply::Operation)
            }),
            Method::RequestSwitchUser => self.own_session_action(who, None, true).and_then(|(req, sid)| {
                self.request_lock(req, &sid, OpKind::SwitchUser, fx).map(Reply::Operation)
            }),
            Method::RequestLogout { session } => self.own_session_action(who, session, false).and_then(|(req, sid)| {
                self.request_logout(req, &sid, fx).map(Reply::Operation)
            }),
            Method::RequestSuspend => self.dbus_power(who, caller, PowerAction::Suspend, fx),
            Method::RequestReboot => self.dbus_power(who, caller, PowerAction::Reboot, fx),
            Method::RequestPowerOff => self.dbus_power(who, caller, PowerAction::PowerOff, fx),
            Method::CancelOperation { operation } => {
                let req = match who {
                    Principal::Root => Requester::Root,
                    Principal::Greeter => Requester::Greeter,
                    Principal::Session(s) => Requester::Session(s.clone()),
                    Principal::User(_) => return Some(Err(deny("not your operation"))),
                };
                let id: OperationId = operation;
                self.cancel_operation(&req, id, fx).map(|_| Reply::Done)
            }
            Method::PublishAppearance(req) => return self.dbus_publish(call, who, req, fx),
        })
    }

    /// Resolve the session an own-session action targets and the requester identity.
    /// `lock_like` selects the default target for root: the foreground session.
    fn own_session_action(
        &self,
        who: &Principal,
        target: Option<SessionId>,
        lock_like: bool,
    ) -> Result<(Requester, SessionId), BsmError> {
        match who {
            Principal::Session(own) => {
                let t = target.filter(|t| !t.is_empty()).unwrap_or_else(|| own.clone());
                if &t != own {
                    return Err(deny("a session may act only on itself"));
                }
                Ok((Requester::Session(own.clone()), t))
            }
            Principal::Root => {
                let t = target.filter(|t| !t.is_empty()).or_else(|| self.foreground.clone());
                let _ = lock_like;
                match t {
                    Some(t) => Ok((Requester::Root, t)),
                    None => Err(BsmError::new(ErrorCode::NoSession, "no session")),
                }
            }
            _ => Err(deny("only a hosted session or root may do this")),
        }
    }

    fn dbus_power(
        &mut self,
        who: &Principal,
        caller: &Caller,
        action: PowerAction,
        fx: &mut Fx,
    ) -> Result<Reply, BsmError> {
        let (req, c) = match who {
            Principal::Root => (Requester::Root, None),
            Principal::Session(s) => (Requester::Session(s.clone()), Some(caller.clone())),
            Principal::Greeter => return Err(deny("the greeter uses its control socket for power actions")),
            Principal::User(_) => return Err(deny("not a hosted session")),
        };
        self.request_power(req, action, c, fx).map(Reply::Operation)
    }

    fn dbus_publish(
        &mut self,
        call: CallId,
        who: &Principal,
        req: PublishRequest,
        fx: &mut Fx,
    ) -> Option<Result<Reply, BsmError>> {
        let uid = match who {
            Principal::Session(s) => self.sessions.get(s).map(|s| s.uid)?,
            Principal::User(u) => *u,
            _ => return Some(Err(deny("only an account's own shell may publish its appearance"))),
        };
        if !self.user_known(uid) {
            return Some(Err(BsmError::new(ErrorCode::InvalidAccount, "not an eligible account")));
        }
        if !valid_accent(&req.appearance.accent) {
            return Some(Err(BsmError::new(ErrorCode::InvalidArgument, "accent must be #rrggbb")));
        }
        if self.publishing.contains(&uid)
            || self.last_publish.get(&uid).is_some_and(|t| self.now().duration_since(*t) < self.policy.publish_interval)
        {
            return Some(Err(BsmError::new(ErrorCode::Busy, "appearance is published too often")));
        }
        self.publishing.insert(uid);
        self.last_publish.insert(uid, self.now());
        fx.push(Effect::PublishAppearance { call: Some(call), uid, request: req });
        None
    }

    pub(super) fn on_appearance_published(
        &mut self,
        call: Option<CallId>,
        uid: u32,
        result: Result<PublishedView, BsmError>,
        fx: &mut Fx,
    ) {
        self.publishing.remove(&uid);
        let reply = match result {
            Ok(view) => {
                if let Some(u) = self.users.get_mut(&uid) {
                    let p = &mut u.profile;
                    p.appearance = view.appearance;
                    p.avatar_path = view.avatar_path;
                    p.avatar_rev = view.avatar_rev;
                    p.wallpaper_path = view.wallpaper_path;
                    p.wallpaper_blur_path = view.wallpaper_blur_path;
                    p.wallpaper_rev = view.wallpaper_rev;
                    p.show_personal_images = view.show_personal_images;
                }
                let rev = self.appearance_rev.entry(uid).or_insert(0);
                *rev += 1;
                fx.push(Effect::Dbus(DbusSignal::AppearanceChanged { uid, revision: *rev }));
                Ok(Reply::Done)
            }
            Err(e) => Err(e),
        };
        if let Some(call) = call {
            fx.push(Effect::DbusReply { call, result: reply });
        }
    }

    fn user_infos(&self) -> Vec<UserInfo> {
        self.users
            .values()
            .map(|u| {
                let (availability, session) = self.availability_of(u.profile.uid);
                UserInfo {
                    uid: u.profile.uid,
                    user_name: u.profile.user_name.clone(),
                    display_name: u.profile.display_name.clone(),
                    availability,
                    session,
                    avatar_rev: u.profile.avatar_rev,
                    wallpaper_rev: u.profile.wallpaper_rev,
                }
            })
            .collect()
    }

    /// Every session as `GetSessions` reports it.
    pub fn session_infos(&self) -> Vec<SessionInfo> {
        use bsm_proto::state::SessionState as S;
        self.sessions
            .values()
            .filter(|s| s.state != S::Terminated)
            .map(|s| SessionInfo {
                id: s.id.clone(),
                generation: s.generation,
                uid: s.uid,
                state: s.state,
                availability: self.session_summaries().iter().find(|x| x.id == s.id).map(|x| x.availability).unwrap_or(
                    bsm_proto::state::SessionAvailability::Unavailable,
                ),
                foreground: self.foreground.as_deref() == Some(s.id.as_str()),
                locked: s.state.is_protected(),
                progress: s.progress.as_ref().filter(|_| s.launching()).map_or(100, |p| p.fraction()),
            })
            .collect()
    }
}
