//! Manager commands. Every command that names a session carries its generation and is
//! dropped when that is not the registered one.

use bsm_proto::scene::SceneCommand;

use crate::state::State;

impl State {
    pub(crate) fn handle_command(&mut self, cmd: SceneCommand) {
        match cmd {
            SceneCommand::RegisterSession { session, generation, uid, scope, host_pid, host_exe } => {
                self.register_session(session, generation, uid, scope, host_pid, host_exe)
            }
            SceneCommand::BindHost { session, generation, scope, host_pid } => {
                self.bind_host(&session, generation, scope, host_pid)
            }
            SceneCommand::UnregisterSession { session, generation } => {
                self.unregister_session(&session, generation)
            }
            SceneCommand::RegisterGreeter { generation, uid, pid } => {
                self.register_greeter(generation, uid, pid)
            }
            SceneCommand::UnregisterGreeter { generation } => self.unregister_greeter(generation),
            SceneCommand::Protect { session, generation } => {
                let known = self.sessions.get(&session);
                if known.is_some_and(|r| r.generation != generation) {
                    tracing::warn!("protect for stale generation of {session} dropped");
                } else {
                    // An unknown session has no pixels; protecting it is trivially true and
                    // still answered after a frame.
                    let key = (session.clone(), generation);
                    self.engine.protect(&key);
                    self.revoke_input_of(&session);
                }
            }
            SceneCommand::SetInputOwner(o) => self.request_owner(o),
            SceneCommand::ShowGreeterOnly => self.engine.show_greeter_only(),
            SceneCommand::BeginTransition {
                operation,
                kind,
                source,
                target,
                duration_ms,
                reduce_motion,
                power,
            } => {
                let probe = crate::state::ProbeView {
                    greeter_connected: self.greeter.connected.is_some(),
                    outputs: &self.outputs,
                    sessions: &self.sessions,
                };
                self.engine
                    .begin(&probe, operation, kind, source, target, duration_ms, reduce_motion, power);
            }
            SceneCommand::CancelTransition { operation } => self.engine.cancel(operation),
            SceneCommand::SetCoverColor(c) => self.cover = c,
            SceneCommand::SetDpms { on } => {
                self.dpms_on = on;
                if let Some(g) = self.gfx.as_mut() {
                    g.set_dpms(on);
                }
            }
        }
        for e in self.engine.drain_events() {
            self.emit(e);
        }
        self.needs_redraw = true;
    }
}
