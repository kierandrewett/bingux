//! Security invariants of docs/bsm/architecture.md as tests.

use std::time::Duration;

use bsm_proto::control::{ClientMessage, ServerMessage};
use bsm_proto::error::ErrorCode;
use bsm_proto::helper::{HelperEvent, HelperRequest, PamPromptKind};
use bsm_proto::progress::Stage;
use bsm_proto::scene::{HostReport, SceneCommand, SceneEvent, TransitionPhaseEvent};
use bsm_proto::state::{AuthIntent, InputOwner, SeatState, SessionState, TransitionKind};

use super::*;
use crate::core::*;

fn no_session_input(owners: &[InputOwner]) -> bool {
    owners.iter().all(|o| !matches!(o, InputOwner::Session(_)))
}

fn two_users_locked() -> (Sim, String, String) {
    let mut sim = Sim::new();
    sim.boot();
    let a = sim.login(ALICE);
    sim.call(Sim::session_caller(&sim, &a), Method::RequestSwitchUser).unwrap();
    sim.select(BOB);
    sim.authenticate(BOB, AuthIntent::Switch);
    let b = sim.sid_of(BOB);
    sim.bridge_full(&b);
    sim.call(Sim::session_caller(&sim, &b), Method::RequestLock { session: None }).unwrap();
    (sim, a, b)
}

// ----- the greeter connection -----

#[test]
fn only_the_greeter_account_and_the_process_bsm_started_may_attach() {
    let mut sim = Sim::new();
    sim.feed(Input::Start { persisted: Persisted::default() });
    sim.feed(Input::Accounts(accounts()));
    sim.feed(Input::HelperConnected);
    sim.feed(Input::Scene(SceneEvent::Ready { outputs: sim.outputs.clone() }));
    let verdict = |sim: &Sim| {
        sim.all
            .iter()
            .rev()
            .find_map(|e| match e {
                Effect::ConnVerdict(v) => Some(v.clone()),
                _ => None,
            })
            .unwrap()
    };
    sim.feed(Input::GreeterPeer { conn: 1, uid: ALICE, pid: GREETER_PID });
    assert!(!verdict(&sim).accept, "another account");
    sim.feed(Input::GreeterPeer { conn: 2, uid: GREETER_UID, pid: GREETER_PID + 1 });
    assert!(!verdict(&sim).accept, "right account, wrong process");
    sim.feed(Input::GreeterPeer { conn: 3, uid: 0, pid: GREETER_PID });
    assert!(!verdict(&sim).accept, "root is not the greeter either");
    sim.feed(Input::GreeterPeer { conn: 4, uid: GREETER_UID, pid: GREETER_PID });
    assert!(verdict(&sim).accept);
    sim.feed(Input::GreeterPeer { conn: 5, uid: GREETER_UID, pid: GREETER_PID });
    assert!(!verdict(&sim).accept, "one connection only; a second needs the first to close");
    // Messages from a connection that was never accepted are dropped.
    let mark = sim.mark();
    sim.feed(Input::Greeter { conn: 5, message: ClientMessage::Hello { version: 1 } });
    assert!(sim.greeter_msgs(mark).is_empty());
}

#[test]
fn nothing_is_accepted_before_hello() {
    let mut sim = Sim::new();
    sim.feed(Input::Start { persisted: Persisted::default() });
    sim.feed(Input::Accounts(accounts()));
    sim.feed(Input::HelperConnected);
    sim.feed(Input::Scene(SceneEvent::Ready { outputs: sim.outputs.clone() }));
    sim.conn = 1;
    sim.feed(Input::GreeterPeer { conn: 1, uid: GREETER_UID, pid: GREETER_PID });
    sim.select(BOB);
    assert_ne!(sim.m.selected_uid(), Some(BOB));
    assert!(sim.greeter_msgs(0).iter().any(|m| matches!(m, ServerMessage::Error { code: ErrorCode::Unauthorised, .. })));
}

#[test]
fn an_unsupported_protocol_version_is_refused() {
    let mut sim = Sim::new();
    sim.feed(Input::Start { persisted: Persisted::default() });
    sim.feed(Input::Accounts(accounts()));
    sim.feed(Input::HelperConnected);
    sim.feed(Input::Scene(SceneEvent::Ready { outputs: sim.outputs.clone() }));
    sim.conn = 1;
    sim.feed(Input::GreeterPeer { conn: 1, uid: GREETER_UID, pid: GREETER_PID });
    sim.greeter(ClientMessage::Hello { version: 999 });
    assert!(sim.all.iter().any(|e| matches!(e, Effect::DropGreeter)));
}

// ----- authentication challenges -----

#[test]
fn changing_the_selected_user_mid_auth_cancels_the_challenge_and_a_late_success_is_ignored() {
    let mut sim = Sim::new();
    sim.boot();
    sim.select(ALICE);
    sim.greeter(ClientMessage::BeginAuth { request_id: 1, uid: ALICE, intent: AuthIntent::Login });
    let helper_req = sim.last_auth_helper_req;
    let cid = sim.last_challenge_id();
    let mark = sim.mark();
    sim.select(BOB);
    assert!(sim.helper_reqs(mark).iter().any(|r| matches!(r, HelperRequest::AuthAbort { req_id } if *req_id == helper_req)));
    assert!(sim.greeter_msgs(mark).iter().any(|m| matches!(m, ServerMessage::AuthResult { challenge_id, ok: false, code: Some(ErrorCode::Cancelled), .. } if *challenge_id == cid)));
    assert!(sim.m.operation().is_none());
    // The helper's answer for the cancelled conversation arrives anyway: it authorises nothing.
    let mark = sim.mark();
    sim.feed(Input::Helper(HelperEvent::AuthDone { req_id: helper_req, ok: true, code: None, message: String::new(), ticket: Some(1) }));
    assert!(sim.m.sessions().next().is_none(), "no session for the account that was deselected");
    assert!(sim.scene_cmds(mark).is_empty());
    // And an answer for the old challenge id does not reach the helper.
    sim.greeter(ClientMessage::AuthResponse { challenge_id: cid, response: "x".into() });
    assert!(sim.helper_reqs(mark).iter().all(|r| !matches!(r, HelperRequest::AuthResponse { .. })));
}

#[test]
fn auth_success_is_single_use() {
    let mut sim = Sim::new();
    sim.boot();
    sim.select(ALICE);
    sim.authenticate(ALICE, AuthIntent::Login);
    assert_eq!(sim.m.sessions().count(), 1);
    let req = sim.last_auth_helper_req;
    let mark = sim.mark();
    // The same success replayed: nothing happens.
    sim.feed(Input::Helper(HelperEvent::AuthDone { req_id: req, ok: true, code: None, message: String::new(), ticket: Some(5) }));
    assert_eq!(sim.m.sessions().count(), 1);
    assert!(sim.scene_cmds(mark).is_empty());
    assert!(sim.helper_reqs(mark).is_empty());
}

#[test]
fn a_failed_password_creates_nothing_and_leads_to_a_cooldown() {
    let mut sim = Sim::new();
    sim.boot();
    for n in 0..5 {
        sim.greeter(ClientMessage::BeginAuth { request_id: n, uid: ALICE, intent: AuthIntent::Login });
        let cid = sim.last_challenge_id();
        sim.greeter(ClientMessage::AuthResponse { challenge_id: cid, response: "wrong".into() });
        assert!(sim.m.sessions().next().is_none());
        assert!(sim.m.operation().is_none());
    }
    // Too many failures: refused outright, before PAM, until the cooldown ends.
    let mark = sim.mark();
    sim.greeter(ClientMessage::BeginAuth { request_id: 9, uid: ALICE, intent: AuthIntent::Login });
    assert!(sim.helper_reqs(mark).is_empty());
    assert!(sim.greeter_msgs(mark).iter().any(|m| matches!(m, ServerMessage::Error { code: ErrorCode::Busy, request_id: Some(9), .. })));
    // Another account is not affected.
    sim.select(BOB);
    sim.greeter(ClientMessage::BeginAuth { request_id: 10, uid: BOB, intent: AuthIntent::Login });
    assert!(sim.m.operation().is_some());
    sim.greeter(ClientMessage::CancelAuth { challenge_id: sim.last_challenge_id() });
    sim.advance(Duration::from_secs(31));
    sim.select(ALICE);
    sim.greeter(ClientMessage::BeginAuth { request_id: 11, uid: ALICE, intent: AuthIntent::Login });
    assert!(sim.m.operation().is_some());
}

#[test]
fn unlock_is_bound_to_the_locked_account() {
    let (mut sim, a, b) = two_users_locked();
    // B is the lock owner. An unlock for A is refused: A's session is not the greeter target.
    // Without selecting A first the request is stale, not an unlock of A.
    let mark = sim.mark();
    sim.greeter(ClientMessage::BeginAuth { request_id: 1, uid: ALICE, intent: AuthIntent::Unlock });
    assert!(sim.helper_reqs(mark).iter().all(|r| !matches!(r, HelperRequest::AuthStart { .. })));
    assert_eq!(sim.state(&b), SessionState::LockedGreeterForeground);
    // Selecting A hides B's unlock form; A is then no lock owner either, so unlock has no target.
    sim.select(ALICE);
    let mark = sim.mark();
    sim.greeter(ClientMessage::BeginAuth { request_id: 2, uid: ALICE, intent: AuthIntent::Unlock });
    assert!(sim.helper_reqs(mark).iter().all(|r| !matches!(r, HelperRequest::AuthStart { .. })));
    assert!(sim.greeter_msgs(mark).iter().any(|m| matches!(m, ServerMessage::Error { code: ErrorCode::NoSession, .. })));
    assert_eq!(sim.state(&a), SessionState::LockedBackground);
    assert_eq!(sim.state(&b), SessionState::LockedBackground);
}

#[test]
fn a_prompt_or_result_for_an_old_conversation_changes_nothing() {
    let mut sim = Sim::new();
    sim.boot();
    sim.select(ALICE);
    sim.greeter(ClientMessage::BeginAuth { request_id: 1, uid: ALICE, intent: AuthIntent::Login });
    let mark = sim.mark();
    sim.feed(Input::Helper(HelperEvent::AuthPrompt { req_id: 999, kind: PamPromptKind::EchoOff, text: "evil".into() }));
    sim.feed(Input::Helper(HelperEvent::AuthDone { req_id: 999, ok: true, code: None, message: String::new(), ticket: Some(1) }));
    assert!(sim.greeter_msgs(mark).iter().all(|m| !matches!(m, ServerMessage::AuthPrompt { .. } | ServerMessage::AuthResult { .. })));
    assert!(sim.m.sessions().next().is_none());
}

#[test]
fn helper_loss_mid_auth_ends_the_challenge_visibly() {
    let mut sim = Sim::new();
    sim.boot();
    sim.select(ALICE);
    sim.greeter(ClientMessage::BeginAuth { request_id: 1, uid: ALICE, intent: AuthIntent::Login });
    let mark = sim.mark();
    sim.feed(Input::HelperLost);
    assert!(sim.greeter_msgs(mark).iter().any(|m| matches!(m, ServerMessage::AuthResult { ok: false, code: Some(ErrorCode::HelperFailure), .. })));
    assert!(sim.m.operation().is_none());
    // While the helper is down, authentication is refused with a stable error.
    let mark = sim.mark();
    sim.greeter(ClientMessage::BeginAuth { request_id: 2, uid: ALICE, intent: AuthIntent::Login });
    assert!(sim.greeter_msgs(mark).iter().any(|m| matches!(m, ServerMessage::Error { code: ErrorCode::HelperFailure, .. })));
}

#[test]
fn the_challenge_expires() {
    let mut sim = Sim::new();
    sim.boot();
    sim.select(ALICE);
    sim.greeter(ClientMessage::BeginAuth { request_id: 1, uid: ALICE, intent: AuthIntent::Login });
    let mark = sim.mark();
    sim.advance(Duration::from_secs(121));
    assert!(sim.helper_reqs(mark).iter().any(|r| matches!(r, HelperRequest::AuthAbort { .. })));
    assert!(sim.greeter_msgs(mark).iter().any(|m| matches!(m, ServerMessage::AuthResult { ok: false, code: Some(ErrorCode::Timeout), .. })));
    assert!(sim.m.operation().is_none());
}

#[test]
fn secrets_never_appear_in_effects_logs_or_debug_output() {
    let mut sim = Sim::new();
    sim.boot();
    sim.select(ALICE);
    sim.authenticate(ALICE, AuthIntent::Login);
    for e in &sim.all {
        let s = format!("{e:?}");
        assert!(!s.contains(PASSWORD), "{s}");
        if let Effect::Log(l) = e {
            assert!(!l.message.contains(PASSWORD));
        }
    }
}

// ----- stale and out of order callbacks -----

#[test]
fn stale_generation_callbacks_change_nothing() {
    let mut sim = Sim::new();
    sim.boot();
    sim.hold();
    sim.select(ALICE);
    sim.authenticate(ALICE, AuthIntent::Login);
    let sid = sim.sid_of(ALICE);
    let gn = sim.gen_of(&sid);
    let stale = gn.wrapping_add(1);
    let before = sim.m.session(&sid).unwrap().progress.clone();
    let mark = sim.mark();
    sim.feed(Input::Scene(SceneEvent::SessionConnected { session: sid.clone(), generation: stale, pid: 1 }));
    for r in [
        HostReport::GnoblinReady,
        HostReport::BinguxLaunched,
        HostReport::DesktopReady,
        HostReport::OutputConfigured { output: "DP-1".into() },
    ] {
        sim.feed(Input::Scene(SceneEvent::SessionReport { session: sid.clone(), generation: stale, report: r }));
    }
    sim.feed(Input::Scene(SceneEvent::SessionSurfaces { session: sid.clone(), generation: stale, surfaces: vec![] }));
    sim.feed(Input::Scene(SceneEvent::Protected { session: sid.clone(), generation: stale }));
    sim.feed(Input::Scene(SceneEvent::SessionGone { session: sid.clone(), generation: stale }));
    sim.feed(Input::Helper(HelperEvent::SessionExited { session: sid.clone(), generation: stale, status: 1 }));
    sim.feed(Input::Helper(HelperEvent::SessionOpened {
        req_id: 1,
        session: sid.clone(),
        generation: stale,
        host_pid: 1,
        leader_pid: 2,
        scope: "x.scope".into(),
        logind_session: None,
    }));
    assert_eq!(sim.m.session(&sid).unwrap().progress, before);
    assert_eq!(sim.state(&sid), SessionState::GnoblinStarting);
    assert!(sim.scene_cmds(mark).is_empty());
    assert!(sim.m.session(&sid).is_some());
    // Unknown transition events are ignored too.
    sim.phase(424242, TransitionPhaseEvent::Committed);
    assert!(sim.m.check_invariants().is_ok());
}

#[test]
fn out_of_order_readiness_cannot_advance_the_bar_or_reveal() {
    let mut sim = Sim::new();
    sim.boot();
    sim.hold();
    sim.select(ALICE);
    sim.authenticate(ALICE, AuthIntent::Login);
    let sid = sim.sid_of(ALICE);
    let mark = sim.mark();
    // Reports before the bridge even connected, and a DesktopReady with nothing behind it.
    sim.report(&sid, HostReport::GnoblinReady);
    sim.report(&sid, HostReport::BinguxLaunched);
    sim.report(&sid, HostReport::DesktopReady);
    let p = sim.m.session(&sid).unwrap().progress.clone().unwrap();
    assert_eq!(p.reached, vec![Stage::AuthAccepted, Stage::PamSession]);
    assert!(sim.begin_transitions(mark).is_empty());
    // A DesktopReady before the surfaces stage is dropped and cannot later satisfy the gate.
    sim.bridge_to_gnoblin_ready(&sid);
    sim.bridge_outputs(&sid);
    sim.report(&sid, HostReport::DesktopReady);
    sim.bridge_bingux(&sid);
    for o in sim.outputs.clone() {
        *sim.frames.entry((sid.clone(), o.name)).or_insert(0) += 1;
    }
    sim.evidence(&sid);
    assert_ne!(sim.state(&sid), SessionState::ReadyBackground, "frames without a later DesktopReady are not evidence");
    assert!(sim.begin_transitions(mark).is_empty());
    // The proper report followed by a fresh frame finishes.
    sim.bridge_present(&sid);
    assert_eq!(sim.state(&sid), SessionState::ReadyBackground);
}

#[test]
fn every_required_output_needs_its_own_presented_frame() {
    let mut sim = Sim::new();
    sim.boot();
    sim.hold();
    sim.select(ALICE);
    sim.authenticate(ALICE, AuthIntent::Login);
    let sid = sim.sid_of(ALICE);
    sim.bridge_to_gnoblin_ready(&sid);
    sim.bridge_outputs(&sid);
    sim.bridge_bingux(&sid);
    sim.report(&sid, HostReport::DesktopReady);
    // A frame on DP-1 only.
    *sim.frames.entry((sid.clone(), "DP-1".into())).or_insert(0) += 1;
    sim.evidence(&sid);
    let p = sim.m.session(&sid).unwrap().progress.clone().unwrap();
    assert_eq!((p.counted_stage, p.done, p.total), (Some(Stage::DesktopReady), 1, 2));
    assert_eq!(sim.state(&sid), SessionState::OutputReady);
    assert!(sim.begin_transitions(0).is_empty());
    *sim.frames.entry((sid.clone(), "DP-2".into())).or_insert(0) += 1;
    sim.evidence(&sid);
    assert_eq!(sim.state(&sid), SessionState::ReadyBackground);
}

#[test]
fn a_surface_that_is_mapped_on_one_output_only_does_not_satisfy_the_stage() {
    let mut sim = Sim::new();
    sim.boot();
    sim.hold();
    sim.select(ALICE);
    sim.authenticate(ALICE, AuthIntent::Login);
    let sid = sim.sid_of(ALICE);
    sim.bridge_to_gnoblin_ready(&sid);
    sim.bridge_outputs(&sid);
    sim.report(&sid, HostReport::BinguxLaunched);
    sim.report(&sid, HostReport::SurfacesMapped { output: "DP-1".into(), mapped: 3, required: 3 });
    sim.report(&sid, HostReport::SurfacesMapped { output: "DP-2".into(), mapped: 2, required: 3 });
    let p = sim.m.session(&sid).unwrap().progress.clone().unwrap();
    assert!(!p.has(Stage::SurfacesMapped));
    assert_eq!((p.done, p.total), (1, 2));
    // An unknown output name is not counted.
    sim.report(&sid, HostReport::SurfacesMapped { output: "HDMI-9".into(), mapped: 3, required: 3 });
    let p = sim.m.session(&sid).unwrap().progress.clone().unwrap();
    assert!(!p.has(Stage::SurfacesMapped));
}

#[test]
fn outputs_changing_after_configuration_fail_the_launch_instead_of_revealing_stale_pixels() {
    let mut sim = Sim::new();
    sim.boot();
    sim.hold();
    sim.select(ALICE);
    sim.authenticate(ALICE, AuthIntent::Login);
    let sid = sim.sid_of(ALICE);
    sim.bridge_to_gnoblin_ready(&sid);
    sim.bridge_outputs(&sid);
    let mark = sim.mark();
    // A monitor is unplugged.
    sim.outputs.pop();
    sim.feed(Input::Scene(SceneEvent::OutputsChanged { outputs: sim.outputs.clone() }));
    let failed = sim.m.retained_failure().unwrap();
    assert_eq!(failed.failure.as_ref().unwrap().code, ErrorCode::OutputNotReady);
    assert!(sim.begin_transitions(mark).is_empty());
    assert!(sim.m.foreground().is_none());
}

#[test]
fn hotplug_before_configuration_only_changes_the_counts() {
    let mut sim = Sim::new();
    sim.boot();
    sim.hold();
    sim.select(ALICE);
    sim.authenticate(ALICE, AuthIntent::Login);
    let sid = sim.sid_of(ALICE);
    sim.bridge_to_gnoblin_ready(&sid);
    sim.outputs.pop();
    sim.feed(Input::Scene(SceneEvent::OutputsChanged { outputs: sim.outputs.clone() }));
    assert!(sim.m.retained_failure().is_none());
    sim.bridge_outputs(&sid);
    sim.bridge_bingux(&sid);
    sim.bridge_present(&sid);
    assert_eq!(sim.state(&sid), SessionState::ReadyBackground);
}

#[test]
fn a_rejected_connection_cannot_fail_or_advance_a_launch() {
    let mut sim = Sim::new();
    sim.boot();
    sim.hold();
    sim.select(ALICE);
    sim.authenticate(ALICE, AuthIntent::Login);
    let sid = sim.sid_of(ALICE);
    let gn = sim.gen_of(&sid);
    let before = sim.m.session(&sid).unwrap().progress.clone();
    sim.feed(Input::Scene(SceneEvent::SessionConnectionRejected { session: sid.clone(), generation: gn, reason: "uid".into() }));
    assert_eq!(sim.m.session(&sid).unwrap().progress, before);
    assert_eq!(sim.state(&sid), SessionState::GnoblinStarting);
}

// ----- greeter and session failures -----

#[test]
fn a_greeter_crash_while_locked_keeps_everything_protected() {
    let (mut sim, a, b) = two_users_locked();
    let mark = sim.mark();
    let ggen = sim.m.greeter.generation.unwrap();
    sim.feed(Input::Helper(HelperEvent::GreeterExited { generation: ggen, status: -6 }));
    // Nothing was released, nothing granted to a session.
    assert!(sim.m.protected().contains(&a) && sim.m.protected().contains(&b));
    assert_eq!(sim.state(&b), SessionState::LockedGreeterForeground);
    assert_eq!(sim.state(&a), SessionState::LockedBackground);
    assert!(no_session_input(&sim.set_input_owner_cmds(mark)));
    assert!(sim.begin_transitions(mark).is_empty());
    assert_eq!(sim.m.seat(), SeatState::Recovery);
    assert!(sim.all.iter().any(|e| matches!(e, Effect::DropGreeter)));
    // Authentication cannot start without a greeter, and a restart does not unlock anything.
    sim.advance(Duration::from_secs(2));
    assert!(sim.helper_reqs(mark).iter().any(|r| matches!(r, HelperRequest::SpawnGreeter { .. })));
    let ggen2 = sim.m.greeter.generation.unwrap();
    assert_ne!(ggen, ggen2);
    sim.feed(Input::Scene(SceneEvent::GreeterConnected { generation: ggen2, pid: GREETER_PID }));
    sim.conn += 1;
    sim.feed(Input::GreeterPeer { conn: sim.conn, uid: GREETER_UID, pid: GREETER_PID });
    sim.greeter(ClientMessage::Hello { version: bsm_proto::PROTOCOL_VERSION });
    assert_eq!(sim.state(&b), SessionState::LockedGreeterForeground);
    assert!(sim.m.protected().contains(&b));
    assert!(matches!(sim.m.snapshot().mode, bsm_proto::control::GreeterMode::Unlock { .. }));
    assert!(no_session_input(&sim.set_input_owner_cmds(mark)));
    assert!(sim.m.check_invariants().is_ok());
}

#[test]
fn the_greeter_connection_dropping_cancels_the_conversation_only() {
    let mut sim = Sim::new();
    sim.boot();
    sim.select(ALICE);
    sim.greeter(ClientMessage::BeginAuth { request_id: 1, uid: ALICE, intent: AuthIntent::Login });
    let mark = sim.mark();
    sim.feed(Input::GreeterDisconnected { conn: sim.conn });
    assert!(sim.m.operation().is_none());
    assert!(sim.helper_reqs(mark).iter().any(|r| matches!(r, HelperRequest::AuthAbort { .. })));
    assert!(!sim.m.greeter_control_connected());
}

#[test]
fn a_launch_waits_for_the_greeter_before_revealing() {
    let mut sim = Sim::new();
    sim.boot();
    sim.hold();
    sim.select(ALICE);
    sim.authenticate(ALICE, AuthIntent::Login);
    let sid = sim.sid_of(ALICE);
    let ggen = sim.m.greeter.generation.unwrap();
    sim.bridge_to_gnoblin_ready(&sid);
    sim.bridge_outputs(&sid);
    sim.bridge_bingux(&sid);
    // The greeter's Wayland connection drops right before the last frame.
    sim.feed(Input::Scene(SceneEvent::GreeterGone { generation: ggen }));
    let mark = sim.mark();
    sim.bridge_present(&sid);
    assert_eq!(sim.state(&sid), SessionState::ReadyBackground);
    assert!(sim.begin_transitions(mark).is_empty(), "no reveal without a greeter underneath");
    // It comes back; the reveal runs.
    sim.advance(Duration::from_secs(2));
    let g2 = sim.m.greeter.generation.unwrap();
    sim.feed(Input::Scene(SceneEvent::GreeterConnected { generation: g2, pid: GREETER_PID }));
    assert_eq!(sim.begin_transitions(mark).len(), 1);
}

#[test]
fn without_the_greeter_the_gate_times_out_visibly() {
    let mut sim = Sim::new();
    sim.boot();
    sim.hold();
    sim.select(ALICE);
    sim.authenticate(ALICE, AuthIntent::Login);
    let sid = sim.sid_of(ALICE);
    let ggen = sim.m.greeter.generation.unwrap();
    sim.bridge_to_gnoblin_ready(&sid);
    sim.bridge_outputs(&sid);
    sim.bridge_bingux(&sid);
    sim.feed(Input::Scene(SceneEvent::GreeterGone { generation: ggen }));
    sim.bridge_present(&sid);
    sim.advance(Duration::from_secs(11));
    assert!(sim.m.operation().is_none(), "the operation ended instead of waiting forever");
    assert_eq!(sim.m.history().last().unwrap().reason, Some(ErrorCode::Timeout));
}

#[test]
fn a_stuck_compositor_transition_falls_back_to_the_cover_and_greeter() {
    let mut sim = Sim::new();
    sim.boot();
    sim.hold();
    sim.select(ALICE);
    sim.authenticate(ALICE, AuthIntent::Login);
    let sid = sim.sid_of(ALICE);
    sim.bridge_full(&sid);
    assert_eq!(sim.m.seat(), SeatState::RevealingSession);
    let mark = sim.mark();
    sim.advance(Duration::from_secs(6));
    assert!(sim.m.operation().is_none());
    assert_eq!(sim.state(&sid), SessionState::ReadyBackground, "never revealed");
    assert!(sim.scene_cmds(mark).iter().any(|c| matches!(c, SceneCommand::CancelTransition { .. })));
    assert!(sim.scene_cmds(mark).contains(&SceneCommand::ShowGreeterOnly));
    assert!(no_session_input(&sim.set_input_owner_cmds(mark)));
    assert_eq!(sim.m.history().last().unwrap().reason, Some(ErrorCode::Timeout));
}

#[test]
fn a_failed_transition_reveals_nothing() {
    let mut sim = Sim::new();
    sim.boot();
    sim.hold();
    sim.select(ALICE);
    sim.authenticate(ALICE, AuthIntent::Login);
    let sid = sim.sid_of(ALICE);
    sim.bridge_full(&sid);
    let op = sim.current_op().id;
    let mark = sim.mark();
    sim.phase(op, TransitionPhaseEvent::Animating);
    sim.phase(op, TransitionPhaseEvent::Failed);
    assert_eq!(sim.state(&sid), SessionState::ReadyBackground);
    assert!(sim.m.foreground().is_none());
    assert!(no_session_input(&sim.set_input_owner_cmds(mark)));
    assert_eq!(sim.m.history().last().unwrap().reason, Some(ErrorCode::TransitionInterrupted));
    // The user can authenticate again and the same session (never relaunched) is revealed.
    sim.comp.hold_transitions = false;
    sim.authenticate(ALICE, AuthIntent::Login);
    assert_eq!(sim.state(&sid), SessionState::Foreground);
    assert_eq!(sim.m.sessions().count(), 1);
}

#[test]
fn the_source_vanishing_during_a_reveal_never_commits_it() {
    let mut sim = Sim::new();
    sim.boot();
    sim.hold();
    sim.select(ALICE);
    sim.authenticate(ALICE, AuthIntent::Login);
    let sid = sim.sid_of(ALICE);
    sim.bridge_full(&sid);
    let op = sim.current_op().id;
    let gn = sim.gen_of(&sid);
    let mark = sim.mark();
    sim.feed(Input::Scene(SceneEvent::SessionGone { session: sid.clone(), generation: gn }));
    assert!(sim.m.operation().is_none());
    assert!(sim.scene_cmds(mark).iter().any(|c| matches!(c, SceneCommand::CancelTransition { .. })));
    assert!(sim.scene_cmds(mark).contains(&SceneCommand::ShowGreeterOnly));
    // A late Committed from the compositor for the dead operation does nothing.
    sim.phase(op, TransitionPhaseEvent::Committed);
    assert!(no_session_input(&sim.set_input_owner_cmds(mark)));
    assert!(sim.m.foreground().is_none());
}

#[test]
fn protect_that_is_not_acknowledged_keeps_asking_and_hides_the_desktop() {
    let mut sim = Sim::new();
    sim.boot();
    let a = sim.login(ALICE);
    sim.comp.withhold_protect = true;
    sim.call(Sim::session_caller(&sim, &a), Method::RequestLock { session: None }).unwrap();
    let mark = sim.mark();
    sim.advance(Duration::from_secs(4));
    let cmds = sim.scene_cmds(mark);
    assert!(cmds.contains(&SceneCommand::ShowGreeterOnly));
    assert!(cmds.iter().any(|c| matches!(c, SceneCommand::Protect { .. })));
    assert_eq!(sim.state(&a), SessionState::LockRequested);
    assert!(!sim.m.protected().contains(&a));
    assert_eq!(sim.m.history().last().unwrap().reason, Some(ErrorCode::Timeout));
    // The late acknowledgement is accepted and the session ends up locked.
    let gn = sim.gen_of(&a);
    sim.feed(Input::Scene(SceneEvent::Protected { session: a.clone(), generation: gn }));
    assert!(sim.state(&a).is_protected());
    assert!(sim.m.check_invariants().is_ok());
}

#[test]
fn a_protected_ack_for_a_foreground_session_is_ignored() {
    let mut sim = Sim::new();
    sim.boot();
    let a = sim.login(ALICE);
    let gn = sim.gen_of(&a);
    sim.feed(Input::Scene(SceneEvent::Protected { session: a.clone(), generation: gn }));
    assert_eq!(sim.state(&a), SessionState::Foreground);
    assert!(!sim.m.protected().contains(&a));
    assert_eq!(sim.input_owner(), InputOwner::Session(a));
}

// ----- interruption -----

#[test]
fn a_running_lock_is_not_interrupted_and_a_launch_is_cancelled_by_power() {
    let mut sim = Sim::with_policy(Policy {
        greeter_uid: Some(GREETER_UID),
        greeter_power_with_sessions: GreeterPower { suspend: true, reboot: true, poweroff: true },
        ..Policy::default()
    });
    sim.boot();
    sim.hold();
    sim.select(ALICE);
    sim.authenticate(ALICE, AuthIntent::Login);
    let sid = sim.sid_of(ALICE);
    // Power off from the greeter while a session is still starting: the launch is cancelled.
    sim.greeter(ClientMessage::Power { request_id: 3, action: bsm_proto::state::PowerAction::PowerOff });
    assert!(sim.m.session(&sid).is_none(), "the half started session was closed");
    assert_eq!(sim.current_op().kind, TransitionKind::PowerFade);
}

// ----- recovery -----

#[test]
fn bsm_restart_brings_sessions_back_protected_never_foreground() {
    let mut sim = Sim::new();
    sim.feed(Input::Start { persisted: Persisted { version: 1, last_used_uid: Some(ALICE), sessions: vec![] } });
    sim.feed(Input::Accounts(accounts()));
    sim.feed(Input::HelperConnected);
    sim.feed(Input::Scene(SceneEvent::Ready { outputs: sim.outputs.clone() }));
    let rec = PersistedSession { id: "s0000beef".into(), generation: 55, uid: ALICE, user_name: "alice".into(), scope: "bsm-session-s0000beef.scope".into(), host_pid: 4242 };
    let dead = PersistedSession { id: "s0000dead".into(), generation: 56, uid: BOB, user_name: "bob".into(), scope: "x.scope".into(), host_pid: 1 };
    let mark = sim.mark();
    sim.feed(Input::Recovered(vec![RecoveredSession { session: rec.clone(), alive: true }, RecoveredSession { session: dead, alive: false }]));
    assert_eq!(sim.state("s0000beef"), SessionState::LockedBackground);
    assert!(sim.m.protected().contains("s0000beef"));
    assert!(sim.m.session("s0000dead").is_none());
    assert!(sim.m.foreground().is_none());
    assert!(no_session_input(&sim.set_input_owner_cmds(mark)));
    // The bridge must reconnect through a freshly bound socket before anything can be revealed.
    assert!(sim.scene_cmds(mark).iter().any(|c| matches!(c, SceneCommand::BindHost { scope, host_pid: 4242, .. } if scope == &rec.scope)));
    // The user authenticates; with no frames from a reconnected bridge the gate holds.
    let ggen = sim.m.greeter.generation.unwrap();
    sim.feed(Input::Scene(SceneEvent::GreeterConnected { generation: ggen, pid: GREETER_PID }));
    sim.conn += 1;
    sim.feed(Input::GreeterPeer { conn: sim.conn, uid: GREETER_UID, pid: GREETER_PID });
    sim.greeter(ClientMessage::Hello { version: bsm_proto::PROTOCOL_VERSION });
    sim.select(ALICE);
    sim.hold();
    let m2 = sim.mark();
    sim.authenticate(ALICE, AuthIntent::Switch);
    assert!(sim.begin_transitions(m2).is_empty());
    // The bridge reconnects and presents: the exact session resumes (UnlockSlideUp).
    sim.bridges.insert("s0000beef".into(), (55, 4242));
    sim.bridge_to_gnoblin_ready_only_connect("s0000beef");
    sim.bridge_outputs_recovered("s0000beef");
    let begins = sim.begin_transitions(m2);
    assert_eq!(begins.len(), 1);
    assert!(matches!(&begins[0], SceneCommand::BeginTransition { kind: TransitionKind::UnlockSlideUp, .. }));
}

#[test]
fn a_recovered_session_that_never_reconnects_is_closed() {
    let mut sim = Sim::new();
    sim.feed(Input::Start { persisted: Persisted::default() });
    sim.feed(Input::Accounts(accounts()));
    sim.feed(Input::HelperConnected);
    sim.feed(Input::Scene(SceneEvent::Ready { outputs: sim.outputs.clone() }));
    let rec = PersistedSession { id: "s0000beef".into(), generation: 55, uid: ALICE, user_name: "alice".into(), scope: "s.scope".into(), host_pid: 4242 };
    sim.feed(Input::Recovered(vec![RecoveredSession { session: rec, alive: true }]));
    let mark = sim.mark();
    sim.advance(Duration::from_secs(31));
    assert!(sim.helper_reqs(mark).iter().any(|r| matches!(r, HelperRequest::CloseSession { session, .. } if session == "s0000beef")));
    assert!(sim.m.session("s0000beef").is_none());
}

impl Sim {
    pub fn bridge_to_gnoblin_ready_only_connect(&mut self, sid: &str) {
        let (gn, pid) = self.bridges[sid];
        self.feed(Input::Scene(SceneEvent::SessionConnected { session: sid.into(), generation: gn, pid }));
    }

    pub fn bridge_outputs_recovered(&mut self, sid: &str) {
        for o in self.outputs.clone() {
            self.acked.insert((sid.to_owned(), o.name.clone()), true);
            *self.frames.entry((sid.to_owned(), o.name)).or_insert(0) += 1;
        }
        self.evidence(sid);
    }
}

// ----- D-Bus authorisation -----

#[test]
fn dbus_callers_are_classified_by_uid_and_cgroup_never_by_what_they_claim() {
    let mut sim = Sim::new();
    sim.boot();
    let a = sim.login(ALICE);
    let caller = sim.session_caller(&a);
    assert_eq!(sim.m.principal(&caller), Principal::Session(a.clone()));
    // Same uid, outside the scope: an ordinary user process (for example an ssh login).
    assert_eq!(sim.m.principal(&Sim::plain_user(ALICE)), Principal::User(ALICE));
    // In the scope but another uid: not the session.
    let mut spoof = caller.clone();
    spoof.uid = BOB;
    assert_eq!(sim.m.principal(&spoof), Principal::User(BOB));
    // A cgroup that merely contains the scope name as a substring is not in the scope.
    let mut sub = caller.clone();
    sub.cgroup = Some(format!("/x/{}-evil/app", sim.m.session(&a).unwrap().scope.clone().unwrap()));
    assert_eq!(sim.m.principal(&sub), Principal::User(ALICE));
    // No cgroup readable.
    let mut none = caller.clone();
    none.cgroup = None;
    assert_eq!(sim.m.principal(&none), Principal::User(ALICE));
    assert_eq!(sim.m.principal(&Sim::root()), Principal::Root);
    assert_eq!(sim.m.principal(&Caller { uid: GREETER_UID, pid: 777, bus_name: ":1.5".into(), cgroup: None }), Principal::Greeter);
}

#[test]
fn a_user_cannot_lock_log_out_or_cancel_another_users_session() {
    let (mut sim, a, b) = two_users_locked();
    let ca = sim.session_caller(&a);
    let err = |r: Result<Reply, bsm_proto::error::BsmError>| r.unwrap_err().code;
    assert_eq!(err(sim.call(ca.clone(), Method::RequestLogout { session: Some(b.clone()) })), ErrorCode::Unauthorised);
    assert_eq!(err(sim.call(ca.clone(), Method::RequestLock { session: Some(b.clone()) })), ErrorCode::Unauthorised);
    assert_eq!(err(sim.call(Sim::plain_user(BOB), Method::RequestLogout { session: Some(b.clone()) })), ErrorCode::Unauthorised);
    assert_eq!(err(sim.call(Sim::plain_user(BOB), Method::RequestReboot)), ErrorCode::Unauthorised);
    assert_eq!(err(sim.call(Sim::plain_user(BOB), Method::SelectGreeterUser { uid: ALICE })), ErrorCode::Unauthorised);
    assert_eq!(err(sim.call(ca.clone(), Method::SelectGreeterUser { uid: BOB })), ErrorCode::Unauthorised);
    assert!(sim.m.session(&b).is_some());
    assert!(sim.m.operation().is_none());
}

#[test]
fn a_locked_session_cannot_be_unlocked_or_revealed_through_dbus() {
    let (mut sim, _a, b) = two_users_locked();
    // There is simply no method that reveals or unlocks; the closest requests are refused or no-ops.
    let cb = sim.session_caller(&b);
    assert_eq!(sim.call(cb, Method::RequestLock { session: None }).unwrap_err().code, ErrorCode::AlreadyLocked);
    assert_eq!(sim.state(&b), SessionState::LockedGreeterForeground);
    assert!(sim.m.protected().contains(&b));
}

#[test]
fn the_greeter_account_cannot_drive_sessions_over_dbus() {
    let (mut sim, a, _b) = two_users_locked();
    let g = Caller { uid: GREETER_UID, pid: 777, bus_name: ":1.7".into(), cgroup: None };
    for m in [
        Method::RequestLogout { session: Some(a.clone()) },
        Method::RequestLock { session: Some(a.clone()) },
        Method::RequestPowerOff,
        Method::PublishAppearance(PublishRequest {
            appearance: bsm_proto::control::Appearance::default(),
            show_personal_images: true,
            wallpaper: None,
            avatar: None,
        }),
    ] {
        assert_eq!(sim.call(g.clone(), m).unwrap_err().code, ErrorCode::Unauthorised);
    }
    // But it may read, and select (which only changes appearance).
    assert!(sim.call(g.clone(), Method::GetUsers).is_ok());
    assert!(sim.call(g, Method::SelectGreeterUser { uid: CAROL }).is_ok());
}

#[test]
fn f10_reads_are_limited_to_the_callers_own_uid() {
    let (mut sim, a, b) = two_users_locked();
    // An unrelated account sees nothing of the seat.
    let carol = Sim::plain_user(CAROL);
    let Reply::Sessions(list) = sim.call(carol.clone(), Method::GetSessions).unwrap() else { panic!() };
    assert!(list.is_empty());
    let Reply::Foreground(f) = sim.call(carol.clone(), Method::GetForegroundSession).unwrap() else { panic!() };
    assert!(f.is_none());
    let Reply::Users(users) = sim.call(carol.clone(), Method::GetUsers).unwrap() else { panic!() };
    assert_eq!(users.iter().map(|u| u.uid).collect::<Vec<_>>(), vec![CAROL]);
    let Reply::Selected(sel) = sim.call(carol.clone(), Method::GetSelectedUser).unwrap() else { panic!() };
    assert_eq!(sel, None);
    let Reply::Status(st) = sim.call(carol.clone(), Method::GetStatus).unwrap() else { panic!() };
    assert!(st.foreground.is_none() && st.selected_uid.is_none() && st.last_used_uid.is_none());
    let Reply::Transitions(t) = sim.call(carol, Method::GetTransitions).unwrap() else { panic!() };
    assert!(t.is_empty());
    // A hosted session sees itself and no other account.
    let Reply::Sessions(list) = sim.call(sim.session_caller(&a), Method::GetSessions).unwrap() else { panic!() };
    assert_eq!(list.iter().map(|s| s.id.clone()).collect::<Vec<_>>(), vec![a.clone()]);
    let Reply::Users(users) = sim.call(sim.session_caller(&a), Method::GetUsers).unwrap() else { panic!() };
    assert_eq!(users.iter().map(|u| u.uid).collect::<Vec<_>>(), vec![ALICE]);
    // Root and the greeter see everything.
    let Reply::Sessions(list) = sim.call(Sim::root(), Method::GetSessions).unwrap() else { panic!() };
    assert_eq!(list.len(), 2);
    let g = Caller { uid: GREETER_UID, pid: 777, bus_name: ":1.7".into(), cgroup: None };
    let Reply::Users(users) = sim.call(g, Method::GetUsers).unwrap() else { panic!() };
    assert_eq!(users.len(), 3);
    let _ = b;
}

#[test]
fn appearance_publication_uses_the_bus_uid_and_validates() {
    let mut sim = Sim::new();
    sim.boot();
    let req = |accent: &str| PublishRequest {
        appearance: bsm_proto::control::Appearance { accent: accent.into(), ..Default::default() },
        show_personal_images: true,
        wallpaper: None,
        avatar: None,
    };
    // Bad accent: refused before any work is queued.
    let e = sim.call(Sim::plain_user(ALICE), Method::PublishAppearance(req("javascript:1"))).unwrap_err();
    assert_eq!(e.code, ErrorCode::InvalidArgument);
    // An uid that is not an eligible account.
    let e = sim.call(Sim::plain_user(31337), Method::PublishAppearance(req("#112233"))).unwrap_err();
    assert_eq!(e.code, ErrorCode::InvalidAccount);
    // Accepted: the work is for the caller's uid regardless of anything in the request.
    let id = 500;
    sim.feed(Input::Dbus { call: id, caller: Sim::plain_user(BOB), method: Method::PublishAppearance(req("#112233")) });
    let published: Vec<u32> = sim
        .all
        .iter()
        .filter_map(|e| match e {
            Effect::PublishAppearance { uid, .. } => Some(*uid),
            _ => None,
        })
        .collect();
    assert_eq!(published, vec![BOB]);
    assert!(!sim.replies.contains_key(&id), "reply is deferred until the publication is done");
    // Rate limited while one is in flight.
    let e = sim.call(Sim::plain_user(BOB), Method::PublishAppearance(req("#112244"))).unwrap_err();
    assert_eq!(e.code, ErrorCode::Busy);
    let view = PublishedView {
        appearance: bsm_proto::control::Appearance { accent: "#112233".into(), ..Default::default() },
        avatar_path: Some("/var/lib/bingux/greeter/1001/avatar.png".into()),
        avatar_rev: 3,
        wallpaper_path: None,
        wallpaper_blur_path: None,
        wallpaper_rev: 0,
        show_personal_images: true,
    };
    sim.feed(Input::AppearancePublished { call: Some(id), uid: BOB, result: Ok(view) });
    assert!(matches!(sim.replies.get(&id), Some(Ok(Reply::Done))));
    let snap = sim.m.snapshot();
    let bob = snap.users.iter().find(|u| u.uid == BOB).unwrap();
    assert_eq!(bob.appearance.accent, "#112233");
    assert_eq!(bob.avatar_rev, 3);
    assert!(sim.all.iter().any(|e| matches!(e, Effect::Dbus(DbusSignal::AppearanceChanged { uid: BOB, .. }))));
}

#[test]
fn only_the_requester_may_cancel_and_nothing_after_the_point_of_no_return() {
    let mut sim = Sim::new();
    sim.boot();
    let a = sim.login(ALICE);
    sim.hold();
    let ca = sim.session_caller(&a);
    let Reply::Operation(op) = sim.call(ca.clone(), Method::RequestLogout { session: None }).unwrap() else { panic!() };
    assert_eq!(sim.call(Sim::plain_user(BOB), Method::CancelOperation { operation: op }).unwrap_err().code, ErrorCode::Unauthorised);
    sim.phase(op, TransitionPhaseEvent::Animating);
    // Cancelling a running fade asks the compositor to restore the scene.
    let mark = sim.mark();
    assert!(sim.call(ca.clone(), Method::CancelOperation { operation: op }).is_ok());
    assert!(sim.scene_cmds(mark).iter().any(|c| matches!(c, SceneCommand::CancelTransition { operation } if *operation == op)));
    sim.phase(op, TransitionPhaseEvent::Cancelled);
    assert_eq!(sim.state(&a), SessionState::Foreground);
    assert_eq!(sim.m.history().last().unwrap().phase, bsm_proto::state::OperationPhase::Cancelled);
    // A second logout, past Committed, cannot be cancelled.
    let Reply::Operation(op2) = sim.call(ca.clone(), Method::RequestLogout { session: None }).unwrap() else { panic!() };
    sim.phase(op2, TransitionPhaseEvent::Committed);
    assert_eq!(sim.call(Sim::root(), Method::CancelOperation { operation: op2 }).unwrap_err().code, ErrorCode::Busy);
}

// ----- randomised invariants -----

#[test]
fn randomised_event_orders_never_break_the_invariants_or_reveal_early() {
    use bsm_proto::state::PowerAction;
    let seeds: u64 = std::env::var("BSM_FUZZ_SEEDS").ok().and_then(|v| v.parse().ok()).unwrap_or(60);
    for seed in 0..seeds {
        let mut rng = SplitMix(seed.wrapping_mul(7919) + 1);
        let mut sim = Sim::new();
        sim.boot();
        // Half of the runs let the fake compositor and helper answer, half leave everything to chance.
        if seed % 2 == 0 {
            sim.comp.hold_transitions = true;
            sim.comp.withhold_protect = seed % 4 == 0;
        }
        for _ in 0..250 {
            let pick = rng.next_u64() % 22;
            let uid = [ALICE, BOB, CAROL][(rng.next_u64() % 3) as usize];
            let sids: Vec<String> = sim.m.sessions().map(|s| s.id.clone()).collect();
            let sid = if sids.is_empty() { None } else { Some(sids[(rng.next_u64() as usize) % sids.len()].clone()) };
            let gn = sid.as_ref().map(|s| sim.gen_of(s));
            let op = sim.m.operation().map(|o| o.id);
            let before_owner = sim.input_owner();
            let mark = sim.mark();
            match pick {
                0 => sim.select(uid),
                1 => sim.greeter(ClientMessage::BeginAuth { request_id: 1, uid, intent: [AuthIntent::Login, AuthIntent::Unlock, AuthIntent::Switch][(rng.next_u64() % 3) as usize] }),
                2 => {
                    if let Some(c) = sim.all.iter().rev().find_map(|e| match e {
                        Effect::Greeter(ServerMessage::AuthPrompt { challenge_id, .. }) => Some(*challenge_id),
                        _ => None,
                    }) {
                        let response = if rng.next_u64() % 4 == 0 { "bad".to_owned() } else { PASSWORD.to_owned() };
                        sim.greeter(ClientMessage::AuthResponse { challenge_id: c, response });
                    }
                }
                3 => {
                    if let Some(s) = &sid {
                        if sim.bridges.contains_key(s) {
                            sim.bridge_full(s);
                        }
                    }
                }
                4 => {
                    if let (Some(s), true) = (&sid, true) {
                        if sim.bridges.contains_key(s) {
                            sim.bridge_to_gnoblin_ready(s);
                        }
                    }
                }
                5 => {
                    if let (Some(s), Some(g)) = (&sid, gn) {
                        sim.feed(Input::Scene(SceneEvent::Protected { session: s.clone(), generation: g }));
                    }
                }
                6 => {
                    if let Some(o) = op {
                        let p = [
                            TransitionPhaseEvent::Prepared,
                            TransitionPhaseEvent::Animating,
                            TransitionPhaseEvent::Committed,
                            TransitionPhaseEvent::Completed,
                            TransitionPhaseEvent::Failed,
                            TransitionPhaseEvent::Cancelled,
                        ][(rng.next_u64() % 6) as usize];
                        sim.phase(o, p);
                    }
                }
                7 => {
                    if let Some(s) = &sid {
                        let c = sim.session_caller(s);
                        let m = if rng.next_u64() % 2 == 0 { Method::RequestLock { session: None } } else { Method::RequestSwitchUser };
                        let _ = sim.call(c, m);
                    }
                }
                8 => {
                    if let Some(s) = &sid {
                        let c = sim.session_caller(s);
                        let _ = sim.call(c, Method::RequestLogout { session: None });
                    }
                }
                9 => {
                    if let (Some(s), Some(g)) = (&sid, gn) {
                        sim.feed(Input::Helper(HelperEvent::SessionExited { session: s.clone(), generation: g, status: 9 }));
                    }
                }
                10 => {
                    if let (Some(s), Some(g)) = (&sid, gn) {
                        sim.feed(Input::Scene(SceneEvent::SessionGone { session: s.clone(), generation: g }));
                    }
                }
                11 => sim.advance(Duration::from_secs(rng.next_u64() % 40)),
                12 => {
                    if let Some(gg) = sim.m.greeter.generation {
                        sim.feed(Input::Helper(HelperEvent::GreeterExited { generation: gg, status: 1 }));
                    }
                }
                13 => {
                    sim.outputs = if rng.next_u64() % 2 == 0 { vec![output("DP-1", 0)] } else { vec![output("DP-1", 0), output("DP-2", 1920)] };
                    sim.feed(Input::Scene(SceneEvent::OutputsChanged { outputs: sim.outputs.clone() }));
                }
                14 => {
                    let a = [PowerAction::Suspend, PowerAction::Reboot, PowerAction::PowerOff][(rng.next_u64() % 3) as usize];
                    sim.greeter(ClientMessage::Power { request_id: 5, action: a });
                }
                15 => {
                    if let Some(o) = op {
                        sim.feed(Input::PolkitResult { operation: o, authorised: rng.next_u64() % 3 != 0 });
                        sim.feed(Input::InhibitorsResult { operation: o, result: Ok(vec![]) });
                    }
                }
                16 => {
                    if let Some(o) = op {
                        let r = if rng.next_u64() % 2 == 0 { Ok(()) } else { Err(bsm_proto::error::BsmError::new(ErrorCode::ShutdownRejected, "no")) };
                        sim.feed(Input::PowerResult { operation: o, result: r });
                    }
                }
                17 => {
                    if let Some(o) = op {
                        sim.greeter(ClientMessage::CancelOperation { operation_id: o });
                    }
                }
                18 => sim.feed(Input::PrepareForSleep { start: rng.next_u64() % 2 == 0 }),
                19 => {
                    if let Some(s) = &sid {
                        if sim.bridges.contains_key(s) {
                            let o = sim.outputs[0].name.clone();
                            sim.report(s, HostReport::OutputConfigured { output: o });
                        }
                    }
                }
                20 => sim.feed(Input::HelperLost),
                _ => sim.feed(Input::HelperConnected),
            }
            assert!(sim.m.check_invariants().is_ok(), "seed {seed}: {:?}", sim.m.check_invariants());
            // Whatever happened, a session got physical input only after a reveal transition
            // that targeted it had begun.
            for o in sim.set_input_owner_cmds(mark) {
                if let InputOwner::Session(s) = o {
                    let began = sim.all.iter().any(|e| matches!(e, Effect::Scene(SceneCommand::BeginTransition { target: Some((t, _)), .. }) if *t == s));
                    assert!(began, "seed {seed}: input granted to {s} without a reveal transition");
                }
            }
            let _ = before_owner;
        }
    }
}

#[test]
fn a_stale_begin_auth_cannot_undo_a_newer_selection() {
    let mut sim = Sim::new();
    sim.boot();
    sim.select(ALICE);
    // `bsmctl select-user` moves the greeter to Bob while the greeter's begin_auth(Alice) is in flight.
    let g = Caller { uid: 0, pid: 1, bus_name: ":1.1".into(), cgroup: None };
    sim.call(g, Method::SelectGreeterUser { uid: BOB }).unwrap();
    assert_eq!(sim.m.selected_uid(), Some(BOB));
    let mark = sim.mark();
    sim.greeter(ClientMessage::BeginAuth { request_id: 9, uid: ALICE, intent: AuthIntent::Login });
    assert_eq!(sim.m.selected_uid(), Some(BOB), "the stale request did not re-select Alice");
    assert!(sim.helper_reqs(mark).is_empty());
    assert!(sim.m.operation().is_none());
    assert!(sim.greeter_msgs(mark).iter().any(|m| matches!(m, ServerMessage::Error { request_id: Some(9), code: ErrorCode::StaleGeneration, .. })));
    // The greeter, now showing Bob, authenticates Bob normally.
    sim.greeter(ClientMessage::BeginAuth { request_id: 10, uid: BOB, intent: AuthIntent::Login });
    assert!(sim.m.operation().is_some());
}

// ----- review findings (docs/bsm/review-manager.md) -----

/// F1: a hosted session cannot cancel another user's login by asking for power or logout.
#[test]
fn f1_a_session_cannot_preempt_a_login_in_flight() {
    let mut sim = Sim::new();
    sim.boot();
    let a = sim.login(ALICE);
    sim.call(Sim::session_caller(&sim, &a), Method::RequestSwitchUser).unwrap();
    // Bob is typing his password at the greeter.
    sim.select(BOB);
    sim.greeter(ClientMessage::BeginAuth { request_id: 1, uid: BOB, intent: AuthIntent::Switch });
    let cid = sim.last_challenge_id();
    let op = sim.current_op().id;
    let ca = sim.session_caller(&a);
    for m in [Method::RequestPowerOff, Method::RequestReboot, Method::RequestSuspend, Method::RequestLock { session: None }] {
        let e = sim.call(ca.clone(), m).unwrap_err();
        assert!(matches!(e.code, ErrorCode::Busy | ErrorCode::AlreadyLocked), "{e:?}");
        assert_eq!(sim.current_op().id, op, "Bob's operation is untouched");
    }
    // Her own hidden session may be closed (it takes no operation slot) and Bob's login goes on.
    assert!(sim.call(ca.clone(), Method::RequestLogout { session: None }).is_ok());
    assert_eq!(sim.current_op().id, op);
    // Bob's conversation still works.
    sim.greeter(ClientMessage::AuthResponse { challenge_id: cid, response: PASSWORD.into() });
    assert_eq!(sim.m.sessions().filter(|s| s.uid == BOB).count(), 1);
}

/// F1: a session's power request that is still waiting for polkit does not keep the greeter out.
#[test]
fn f1_a_pending_session_power_request_yields_to_the_greeter() {
    let mut sim = Sim::new();
    sim.boot();
    let a = sim.login(ALICE);
    sim.call(Sim::session_caller(&sim, &a), Method::RequestSwitchUser).unwrap();
    sim.select(BOB);
    sim.call(Sim::session_caller(&sim, &a), Method::RequestPowerOff).unwrap();
    assert!(sim.m.operation().is_some_and(|o| o.kind == TransitionKind::PowerFade));
    sim.greeter(ClientMessage::BeginAuth { request_id: 1, uid: BOB, intent: AuthIntent::Switch });
    assert!(sim.m.operation().is_some_and(|o| o.kind == TransitionKind::NewLoginFade));
    assert!(sim.helper_reqs(0).iter().any(|r| matches!(r, HelperRequest::AuthStart { uid: BOB, .. })));
    // The late polkit answer of the dead power operation changes nothing.
    let n = sim.mark();
    sim.feed(Input::PolkitResult { operation: 1, authorised: true });
    assert!(sim.begin_transitions(n).is_empty());
}

/// F2: a background logout does not make the foreground user's lock fail.
#[test]
fn f2_logging_out_a_hidden_session_does_not_block_the_foreground_lock() {
    let (mut sim, a, b) = two_users_locked();
    // B is the lock owner, A is hidden; bring B to the foreground, A stays hidden.
    sim.authenticate(BOB, AuthIntent::Unlock);
    assert_eq!(sim.state(&b), SessionState::Foreground);
    // A's process asks to log out its hidden session, and the helper is slow to report the exit.
    sim.helper.auto = false;
    let r = sim.call(Sim::session_caller(&sim, &a), Method::RequestLogout { session: None }).unwrap();
    let Reply::Operation(op) = r else { panic!() };
    assert!(sim.m.operation().is_none(), "no operation slot is held");
    assert_eq!(sim.state(&a), SessionState::Stopping);
    // Bob locks at once: works.
    sim.comp.withhold_protect = false;
    assert!(sim.call(Sim::session_caller(&sim, &b), Method::RequestLock { session: None }).is_ok());
    assert!(sim.state(&b).is_protected());
    // The exit arrives; the background operation finishes and is in the history.
    let ga = sim.gen_of(&a);
    sim.feed(Input::Helper(HelperEvent::SessionExited { session: a.clone(), generation: ga, status: 0 }));
    assert!(sim.m.session(&a).is_none());
    assert!(sim.all.iter().any(|e| matches!(e, Effect::Dbus(DbusSignal::OperationFinished { operation_id, success: true, .. }) if *operation_id == op)));
    assert_eq!(sim.m.history().last().unwrap().id, op);
}

/// F3: cancelling after the answer was submitted still counts as a guess.
#[test]
fn f3_cancel_after_submit_is_charged() {
    let mut sim = Sim::new();
    sim.boot();
    sim.helper.auto = false;
    for n in 0..5u64 {
        sim.greeter(ClientMessage::BeginAuth { request_id: n, uid: ALICE, intent: AuthIntent::Login });
        let req = sim.last_auth_helper_req_manual();
        sim.feed(Input::Helper(HelperEvent::AuthPrompt { req_id: req, kind: PamPromptKind::EchoOff, text: "Password:".into() }));
        let cid = sim.last_challenge_id();
        sim.greeter(ClientMessage::AuthResponse { challenge_id: cid, response: "guess".into() });
        // Cancel before the verdict arrives.
        sim.greeter(ClientMessage::CancelAuth { challenge_id: cid });
    }
    let mark = sim.mark();
    sim.greeter(ClientMessage::BeginAuth { request_id: 99, uid: ALICE, intent: AuthIntent::Login });
    assert!(sim.helper_reqs(mark).iter().all(|r| !matches!(r, HelperRequest::AuthStart { .. })));
    assert!(sim.greeter_msgs(mark).iter().any(|m| matches!(m, ServerMessage::Error { code: ErrorCode::Busy, .. })));
}

/// F3: a correct answer refunds the charge.
#[test]
fn f3_success_refunds() {
    let mut sim = Sim::new();
    sim.boot();
    for n in 0..4u64 {
        sim.greeter(ClientMessage::BeginAuth { request_id: n, uid: ALICE, intent: AuthIntent::Login });
        let cid = sim.last_challenge_id();
        sim.greeter(ClientMessage::AuthResponse { challenge_id: cid, response: "bad".into() });
    }
    sim.authenticate(ALICE, AuthIntent::Login);
    assert!(sim.m.sessions().any(|s| s.uid == ALICE));
    assert!(sim.m.failures.get(&ALICE).is_none());
}

/// F4: while somebody is signed in the greeter may not reboot or power off.
#[test]
fn f4_the_greeter_cannot_power_off_over_a_running_session() {
    let mut sim = Sim::new();
    sim.boot();
    let a = sim.login(ALICE);
    sim.call(Sim::session_caller(&sim, &a), Method::RequestLock { session: None }).unwrap();
    for action in [bsm_proto::state::PowerAction::PowerOff, bsm_proto::state::PowerAction::Reboot] {
        let mark = sim.mark();
        sim.greeter(ClientMessage::Power { request_id: 8, action });
        assert!(sim.m.operation().is_none());
        assert!(sim.greeter_msgs(mark).iter().any(|m| matches!(m, ServerMessage::Error { request_id: Some(8), code: ErrorCode::Unauthorised, .. })));
    }
    // Suspend is allowed: it protects and locks everything.
    sim.greeter(ClientMessage::Power { request_id: 9, action: bsm_proto::state::PowerAction::Suspend });
    assert!(sim.m.operation().is_some());
}

/// F17: a hung hidden session (no new frame) cannot be revealed; a healthy one can.
#[test]
fn f17_unlock_needs_a_frame_newer_than_the_hidden_one() {
    let mut sim = Sim::new();
    sim.boot();
    let a = sim.login(ALICE);
    sim.call(Sim::session_caller(&sim, &a), Method::RequestLock { session: None }).unwrap();
    sim.comp.auto_evidence = false; // Gnoblin is hung: it never commits again
    sim.hold();
    let mark = sim.mark();
    sim.authenticate(ALICE, AuthIntent::Unlock);
    assert!(sim.scene_cmds(mark).iter().any(|c| matches!(c, SceneCommand::RequestEvidence { session, .. } if session == &a)));
    assert!(sim.begin_transitions(mark).is_empty(), "the old frame is not evidence");
    // The gate gives up visibly instead of waiting for ever.
    sim.advance(Duration::from_secs(11));
    assert!(sim.m.operation().is_none());
    assert_eq!(sim.m.history().last().unwrap().reason, Some(ErrorCode::OutputNotReady));
    assert_eq!(sim.state(&a), SessionState::LockedGreeterForeground);
    // It recovers and commits: the next unlock works.
    sim.comp.auto_evidence = true;
    sim.comp.hold_transitions = false;
    sim.authenticate(ALICE, AuthIntent::Unlock);
    assert_eq!(sim.state(&a), SessionState::Foreground);
}
