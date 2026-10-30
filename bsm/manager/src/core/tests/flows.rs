//! End to end flows through the pure core: login, lock, unlock, switch, logout.

use bsm_proto::control::{ClientMessage, GreeterMode, ServerMessage};
use bsm_proto::error::ErrorCode;
use bsm_proto::helper::HelperRequest;
use bsm_proto::progress::Stage;
use bsm_proto::scene::{HostReport, SceneCommand, SceneEvent, TransitionPhaseEvent};
use bsm_proto::state::{
    AuthIntent, InputOwner, OperationPhase, SeatState, SessionAvailability, SessionState, TransitionKind,
};

use super::*;
use crate::core::*;

fn snapshot_of(sim: &Sim) -> bsm_proto::control::Snapshot {
    sim.m.snapshot()
}

#[test]
fn boot_reaches_the_greeter_with_the_last_user_selected() {
    let mut sim = Sim::new();
    sim.feed(Input::Start { persisted: Persisted { version: 1, last_used_uid: Some(BOB), sessions: vec![] } });
    assert_eq!(sim.m.seat(), SeatState::Booting);
    sim.feed(Input::Accounts(accounts()));
    sim.feed(Input::HelperConnected);
    let mark = sim.mark();
    sim.feed(Input::Scene(SceneEvent::Ready { outputs: sim.outputs.clone() }));
    // The compositor shows the greeter scene only, and the greeter is asked for.
    assert!(sim.scene_cmds(mark).contains(&SceneCommand::ShowGreeterOnly));
    assert!(sim.helper_reqs(mark).iter().any(|r| matches!(r, HelperRequest::SpawnGreeter { .. })));
    assert!(sim.scene_cmds(mark).iter().any(|c| matches!(c, SceneCommand::RegisterGreeter { uid: GREETER_UID, pid: GREETER_PID, .. })));
    assert_eq!(sim.m.selected_uid(), Some(BOB));
    assert_eq!(sim.m.last_used_uid(), Some(BOB));
    assert!(sim.m.check_invariants().is_ok());
}

#[test]
fn the_snapshot_follows_hello_and_lists_real_availability() {
    let mut sim = Sim::new();
    sim.boot();
    let snap = snapshot_of(&sim);
    assert_eq!(sim.m.seat(), SeatState::Greeter);
    assert_eq!(snap.mode, GreeterMode::Login);
    assert_eq!(snap.users.len(), 3);
    assert!(snap.users.iter().all(|u| u.availability == SessionAvailability::NotSignedIn && u.session.is_none()));
    // A full snapshot right after hello.
    assert!(sim.greeter_msgs(0).iter().any(|m| matches!(m, ServerMessage::Snapshot(_))));
}

#[test]
fn selecting_a_user_changes_the_identity_only() {
    let mut sim = Sim::new();
    sim.boot();
    let mark = sim.mark();
    sim.select(BOB);
    assert_eq!(sim.m.selected_uid(), Some(BOB));
    // No scene command of any kind, no input change, no helper traffic.
    assert!(sim.scene_cmds(mark).is_empty(), "{:?}", sim.scene_cmds(mark));
    assert!(sim.helper_reqs(mark).is_empty());
    assert_eq!(sim.input_owner(), InputOwner::Greeter);
    assert!(sim.m.operation().is_none());
    assert!(sim.greeter_msgs(mark).iter().any(|m| matches!(m, ServerMessage::Snapshot(s) if s.selected_uid == Some(BOB))));
}

#[test]
fn new_login_shows_real_progress_and_fades_the_desktop_over_the_greeter() {
    let mut sim = Sim::new();
    sim.boot();
    sim.hold();
    sim.select(ALICE);
    sim.authenticate(ALICE, AuthIntent::Login);
    let sid = sim.sid_of(ALICE);
    // Auth accepted launches a session; nothing is on screen and input stays with the greeter.
    assert_eq!(sim.m.seat(), SeatState::StartingSession);
    let progress = |sim: &Sim| sim.m.session(&sid).unwrap().progress.clone().unwrap();
    assert!(progress(&sim).has(Stage::AuthAccepted));
    assert!(sim.begin_transitions(0).is_empty());
    sim.bridge_to_gnoblin_ready(&sid);
    assert!(progress(&sim).has(Stage::GnoblinReady));
    assert_eq!(sim.state(&sid), SessionState::BinguxStarting);
    sim.bridge_outputs(&sid);
    assert_eq!(sim.state(&sid), SessionState::OutputReady);
    assert!(sim.begin_transitions(0).is_empty(), "no reveal before the desktop is presented");
    sim.bridge_bingux(&sid);
    assert!(progress(&sim).has(Stage::SurfacesMapped));
    assert!(!progress(&sim).has(Stage::DesktopReady));
    let mark = sim.mark();
    sim.bridge_present(&sid);
    assert_eq!(sim.state(&sid), SessionState::ReadyBackground);
    let begins = sim.begin_transitions(mark);
    assert_eq!(begins.len(), 1);
    let SceneCommand::BeginTransition { operation, kind, source, target, .. } = begins[0].clone() else { unreachable!() };
    assert_eq!(kind, TransitionKind::NewLoginFade);
    assert_eq!(source, None);
    assert_eq!(target, Some((sid.clone(), sim.gen_of(&sid))));
    // Input has not moved yet: the keystrokes that authenticated stay with the greeter.
    assert_eq!(sim.input_owner(), InputOwner::Greeter);
    assert!(sim.set_input_owner_cmds(mark).is_empty());
    assert_eq!(sim.m.seat(), SeatState::RevealingSession);
    sim.phase(operation, TransitionPhaseEvent::Prepared);
    sim.phase(operation, TransitionPhaseEvent::Animating);
    assert_eq!(sim.state(&sid), SessionState::ReadyBackground);
    assert_eq!(sim.input_owner(), InputOwner::Greeter);
    sim.phase(operation, TransitionPhaseEvent::Committed);
    assert_eq!(sim.state(&sid), SessionState::Foreground);
    assert_eq!(sim.input_owner(), InputOwner::Session(sid.clone()));
    assert_eq!(sim.m.foreground(), Some(&sid));
    sim.phase(operation, TransitionPhaseEvent::Completed);
    assert!(sim.m.operation().is_none());
    assert_eq!(sim.m.seat(), SeatState::UserForeground);
    assert_eq!(sim.m.last_used_uid(), Some(ALICE));
    let hist: Vec<_> = sim.m.history().cloned().collect();
    assert_eq!(hist.last().unwrap().phase, OperationPhase::Completed);
    // The last user was persisted.
    assert!(sim.all.iter().any(|e| matches!(e, Effect::Persist(p) if p.last_used_uid == Some(ALICE))));
}

#[test]
fn progress_bar_values_come_only_from_events() {
    let mut sim = Sim::new();
    sim.boot();
    sim.hold();
    sim.select(ALICE);
    sim.authenticate(ALICE, AuthIntent::Login);
    let sid = sim.sid_of(ALICE);
    let fraction = |sim: &Sim| sim.m.session(&sid).unwrap().progress.as_ref().unwrap().fraction();
    let f0 = fraction(&sim);
    // Time passing changes nothing on the bar (until the stage timeout, which fails).
    sim.advance(std::time::Duration::from_secs(30));
    assert_eq!(fraction(&sim), f0);
    let mut last = f0;
    sim.bridge_to_gnoblin_ready(&sid);
    assert!(fraction(&sim) > last);
    last = fraction(&sim);
    // Output n of m: half of the stage weight after one of two outputs.
    let o = sim.outputs[0].name.clone();
    sim.acked.insert((sid.clone(), o.clone()), true);
    sim.evidence(&sid);
    sim.report(&sid, HostReport::OutputConfigured { output: o });
    let p = sim.m.session(&sid).unwrap().progress.clone().unwrap();
    assert_eq!((p.counted_stage, p.done, p.total), (Some(Stage::OutputsConfigured), 1, 2));
    assert_eq!(p.fraction(), last + Stage::OutputsConfigured.weight() / 2);
    assert!(p.is_indeterminate() == false);
    sim.bridge_outputs(&sid);
    sim.bridge_bingux(&sid);
    sim.bridge_present(&sid);
    assert_eq!(sim.m.session(&sid).unwrap().progress.as_ref().unwrap().fraction(), 100);
}

#[test]
fn auth_success_alone_reveals_nothing() {
    let mut sim = Sim::new();
    sim.boot();
    sim.hold();
    let mark = sim.mark();
    sim.select(ALICE);
    sim.authenticate(ALICE, AuthIntent::Login);
    // PAM said yes. No desktop, no input for any session, no transition.
    assert!(sim.begin_transitions(mark).is_empty());
    assert!(sim.set_input_owner_cmds(mark).iter().all(|o| !matches!(o, InputOwner::Session(_))));
    let sid = sim.sid_of(ALICE);
    assert_ne!(sim.state(&sid), SessionState::Foreground);
    assert!(sim.m.foreground().is_none());
    assert!(sim.greeter_msgs(mark).iter().any(|m| matches!(m, ServerMessage::AuthResult { ok: true, .. })));
}

#[test]
fn lock_protects_first_then_shows_the_greeter_in_unlock_mode() {
    let mut sim = Sim::new();
    sim.boot();
    let sid = sim.login(ALICE);
    assert_eq!(sim.state(&sid), SessionState::Foreground);
    sim.comp.withhold_protect = true;
    let mark = sim.mark();
    let r = sim.call(Sim::session_caller(&sim, &sid), Method::RequestLock { session: None }).unwrap();
    let Reply::Operation(op) = r else { panic!("operation id expected") };
    // Input is revoked and Protect is sent, but the greeter is not shown until it is acknowledged.
    assert_eq!(sim.state(&sid), SessionState::LockRequested);
    assert_eq!(sim.m.seat(), SeatState::SecuringLock);
    let cmds = sim.scene_cmds(mark);
    assert!(cmds.iter().any(|c| matches!(c, SceneCommand::Protect { session, .. } if session == &sid)));
    assert!(sim.begin_transitions(mark).is_empty(), "no greeter transition before Protected");
    assert!(!sim.m.protected().contains(&sid));
    assert_eq!(sim.input_owner(), InputOwner::None);
    // Acknowledge.
    sim.comp.withhold_protect = false;
    let gn = sim.gen_of(&sid);
    sim.feed(Input::Scene(SceneEvent::Protected { session: sid.clone(), generation: gn }));
    assert_eq!(sim.state(&sid), SessionState::LockedGreeterForeground);
    assert!(sim.m.protected().contains(&sid));
    assert_eq!(sim.input_owner(), InputOwner::Greeter);
    let begins = sim.begin_transitions(mark);
    assert_eq!(begins.len(), 1);
    assert!(matches!(&begins[0], SceneCommand::BeginTransition { kind: TransitionKind::Lock, operation, target: None, .. } if *operation == op));
    assert_eq!(sim.m.seat(), SeatState::LockedGreeter);
    assert_eq!(snapshot_of(&sim).mode, GreeterMode::Unlock { session: sid.clone(), uid: ALICE });
    assert_eq!(sim.m.selected_uid(), Some(ALICE));
    // logind's LockedHint is set through the helper.
    assert!(sim.helper_reqs(mark).iter().any(|r| matches!(r, HelperRequest::SetLockedHint { locked: true, .. })));
}

#[test]
fn unlock_resumes_the_same_session_with_the_slide_up_and_no_relaunch() {
    let mut sim = Sim::new();
    sim.boot();
    let sid = sim.login(ALICE);
    let gn = sim.gen_of(&sid);
    sim.call(Sim::session_caller(&sim, &sid), Method::RequestLock { session: None }).unwrap();
    assert_eq!(sim.state(&sid), SessionState::LockedGreeterForeground);
    sim.hold();
    let mark = sim.mark();
    sim.authenticate(ALICE, AuthIntent::Unlock);
    // PAM accepted: the gate checks the existing session's frames and begins the slide.
    let cmds = sim.scene_cmds(mark);
    assert!(!cmds.iter().any(|c| matches!(c, SceneCommand::RegisterSession { .. })), "no relaunch");
    assert!(!sim.helper_reqs(mark).iter().any(|r| matches!(r, HelperRequest::OpenSession { .. })));
    let begins = sim.begin_transitions(mark);
    assert_eq!(begins.len(), 1);
    let SceneCommand::BeginTransition { operation, kind, target, .. } = begins[0].clone() else { unreachable!() };
    assert_eq!(kind, TransitionKind::UnlockSlideUp);
    assert_eq!(target, Some((sid.clone(), gn)));
    assert_eq!(sim.input_owner(), InputOwner::Greeter);
    assert_eq!(sim.state(&sid), SessionState::LockedGreeterForeground);
    sim.phase(operation, TransitionPhaseEvent::Animating);
    assert_eq!(sim.input_owner(), InputOwner::Greeter, "input moves at Committed only");
    sim.phase(operation, TransitionPhaseEvent::Committed);
    assert_eq!(sim.state(&sid), SessionState::Foreground);
    assert!(!sim.m.protected().contains(&sid));
    assert_eq!(sim.input_owner(), InputOwner::Session(sid.clone()));
    sim.phase(operation, TransitionPhaseEvent::Completed);
    assert_eq!(sim.m.seat(), SeatState::UserForeground);
    assert_eq!(sim.gen_of(&sid), gn, "same generation: the same Gnoblin");
    assert!(sim.helper_reqs(mark).iter().any(|r| matches!(r, HelperRequest::SetLockedHint { locked: false, .. })));
}

#[test]
fn switching_a_to_b_and_back_resumes_a_without_a_relaunch_and_never_crossfades() {
    let mut sim = Sim::new();
    sim.boot();
    let a = sim.login(ALICE);
    let gen_a = sim.gen_of(&a);
    let mark = sim.mark();
    // A asks to switch user: A is protected and the greeter takes over.
    sim.call(Sim::session_caller(&sim, &a), Method::RequestSwitchUser).unwrap();
    assert_eq!(sim.state(&a), SessionState::LockedGreeterForeground);
    // Picking B hides A's unlock form, A stays protected and running.
    sim.select(BOB);
    assert_eq!(sim.state(&a), SessionState::LockedBackground);
    assert_eq!(snapshot_of(&sim).mode, GreeterMode::Login);
    assert!(sim.m.protected().contains(&a));
    // B logs in for the first time.
    sim.authenticate(BOB, AuthIntent::Switch);
    let b = sim.sid_of(BOB);
    sim.bridge_full(&b);
    assert_eq!(sim.state(&b), SessionState::Foreground);
    assert_eq!(sim.state(&a), SessionState::LockedBackground);
    assert_eq!(sim.input_owner(), InputOwner::Session(b.clone()));
    // Back to A: the existing session resumes.
    sim.call(Sim::session_caller(&sim, &b), Method::RequestSwitchUser).unwrap();
    sim.select(ALICE);
    let mark2 = sim.mark();
    sim.authenticate(ALICE, AuthIntent::Switch);
    assert_eq!(sim.state(&a), SessionState::Foreground);
    assert_eq!(sim.gen_of(&a), gen_a);
    assert_eq!(sim.state(&b), SessionState::LockedBackground);
    assert_eq!(sim.input_owner(), InputOwner::Session(a.clone()));
    let begins = sim.begin_transitions(mark2);
    assert_eq!(begins.len(), 1);
    assert!(matches!(&begins[0], SceneCommand::BeginTransition { kind: TransitionKind::UnlockSlideUp, .. }));
    assert!(!sim.helper_reqs(mark2).iter().any(|r| matches!(r, HelperRequest::OpenSession { .. } | HelperRequest::CloseSession { .. })));
    // No transition in the whole run has both a source and a target desktop.
    for c in sim.scene_cmds(mark) {
        if let SceneCommand::BeginTransition { source, target, .. } = c {
            assert!(!(source.is_some() && target.is_some()), "two desktops in one transition");
        }
    }
    assert!(sim.m.check_invariants().is_ok());
}

#[test]
fn logout_fades_only_that_session_and_leaves_the_others_alive() {
    let mut sim = Sim::new();
    sim.boot();
    let a = sim.login(ALICE);
    sim.call(Sim::session_caller(&sim, &a), Method::RequestSwitchUser).unwrap();
    sim.select(BOB);
    sim.authenticate(BOB, AuthIntent::Switch);
    let b = sim.sid_of(BOB);
    sim.bridge_full(&b);
    sim.hold();
    let mark = sim.mark();
    let r = sim.call(Sim::session_caller(&sim, &b), Method::RequestLogout { session: None }).unwrap();
    let Reply::Operation(op) = r else { panic!() };
    let begins = sim.begin_transitions(mark);
    assert_eq!(begins.len(), 1);
    assert!(matches!(&begins[0], SceneCommand::BeginTransition { kind: TransitionKind::LogoutFade, source: Some((s, _)), target: None, .. } if s == &b));
    // Nothing is torn down while the fade runs.
    assert!(sim.helper_reqs(mark).iter().all(|r| !matches!(r, HelperRequest::CloseSession { .. })));
    assert_eq!(sim.state(&b), SessionState::Foreground);
    sim.phase(op, TransitionPhaseEvent::Animating);
    sim.phase(op, TransitionPhaseEvent::Committed);
    // The greeter owns input first, then only B is closed.
    assert_eq!(sim.input_owner(), InputOwner::Greeter);
    let closes: Vec<_> = sim
        .helper_reqs(mark)
        .into_iter()
        .filter_map(|r| match r {
            HelperRequest::CloseSession { session, .. } => Some(session),
            _ => None,
        })
        .collect();
    assert_eq!(closes, vec![b.clone()]);
    sim.phase(op, TransitionPhaseEvent::Completed);
    // The helper reported the exit: B is gone from the registry, unregistered from the compositor.
    assert!(sim.m.session(&b).is_none());
    assert!(sim.scene_cmds(mark).iter().any(|c| matches!(c, SceneCommand::UnregisterSession { session, .. } if session == &b)));
    assert_eq!(sim.state(&a), SessionState::LockedBackground);
    assert!(sim.m.protected().contains(&a));
    assert_eq!(sim.m.seat(), SeatState::Greeter);
    assert!(sim.m.foreground().is_none());
    assert!(sim.m.check_invariants().is_ok());
}

#[test]
fn a_session_crash_ends_only_that_session_and_returns_to_the_greeter() {
    let mut sim = Sim::new();
    sim.boot();
    let a = sim.login(ALICE);
    sim.call(Sim::session_caller(&sim, &a), Method::RequestSwitchUser).unwrap();
    sim.select(BOB);
    sim.authenticate(BOB, AuthIntent::Switch);
    let b = sim.sid_of(BOB);
    sim.bridge_full(&b);
    let mark = sim.mark();
    // Gnoblin of B dies.
    let gn = sim.gen_of(&b);
    sim.feed(Input::Helper(bsm_proto::helper::HelperEvent::SessionExited { session: b.clone(), generation: gn, status: -11 }));
    assert!(sim.m.session(&b).is_none());
    assert_eq!(sim.state(&a), SessionState::LockedBackground);
    assert!(sim.m.protected().contains(&a));
    assert!(sim.scene_cmds(mark).contains(&SceneCommand::ShowGreeterOnly));
    assert_eq!(sim.input_owner(), InputOwner::Greeter);
    assert_eq!(sim.m.seat(), SeatState::Greeter);
    assert!(sim.greeter_msgs(mark).iter().any(|m| matches!(m, ServerMessage::Error { .. })));
    assert!(sim.m.check_invariants().is_ok());
}

#[test]
fn a_failed_launch_ends_in_a_visible_failure_with_retry_and_never_waits_forever() {
    let mut sim = Sim::new();
    sim.boot();
    sim.hold();
    sim.select(ALICE);
    sim.authenticate(ALICE, AuthIntent::Login);
    let sid = sim.sid_of(ALICE);
    sim.bridge_to_gnoblin_ready(&sid);
    // Outputs never get configured: the stage times out.
    let mark = sim.mark();
    sim.advance(std::time::Duration::from_secs(121));
    let failed = sim.m.retained_failure().expect("failure is kept for the greeter").clone();
    assert_eq!(failed.failure.as_ref().unwrap().stage, Stage::OutputsConfigured);
    assert_eq!(failed.failure.as_ref().unwrap().code, ErrorCode::Timeout);
    assert!(sim.helper_reqs(mark).iter().any(|r| matches!(r, HelperRequest::CloseSession { .. })));
    assert!(sim.begin_transitions(mark).is_empty());
    assert!(snapshot_of(&sim).progress.unwrap().failure.is_some());
    assert!(sim.m.operation().is_none());
    assert!(sim.greeter_msgs(mark).iter().any(|m| matches!(m, ServerMessage::Operation { phase: OperationPhase::Failed, reason: Some(ErrorCode::Timeout), .. })));
    assert_eq!(sim.m.seat(), SeatState::Greeter);
    assert!(sim.m.session(&sid).is_none(), "cleaned up after the exit");
    // Retry: a fresh launch with a new generation.
    sim.comp.hold_transitions = false;
    let sid2 = {
        sim.authenticate(ALICE, AuthIntent::Login);
        sim.sid_of(ALICE)
    };
    assert_ne!(sid2, sid);
    assert!(sim.m.retained_failure().is_none());
    sim.bridge_full(&sid2);
    assert_eq!(sim.state(&sid2), SessionState::Foreground);
}

#[test]
fn back_dismisses_a_failed_launch() {
    let mut sim = Sim::new();
    sim.boot();
    sim.hold();
    sim.select(ALICE);
    sim.authenticate(ALICE, AuthIntent::Login);
    let sid = sim.sid_of(ALICE);
    let gn = sim.gen_of(&sid);
    sim.feed(Input::Helper(bsm_proto::helper::HelperEvent::SessionFailed {
        req_id: 0,
        session: sid.clone(),
        generation: gn,
        code: ErrorCode::LogindFailure,
        message: "x".into(),
    }));
    // A SessionFailed with a request id that is not the open request is stale.
    assert!(sim.m.retained_failure().is_none());
    let open_req = sim
        .all
        .iter()
        .find_map(|e| match e {
            Effect::Helper(HelperRequest::OpenSession { req_id, .. }) => Some(*req_id),
            _ => None,
        })
        .unwrap();
    sim.feed(Input::Helper(bsm_proto::helper::HelperEvent::SessionFailed {
        req_id: open_req,
        session: sid.clone(),
        generation: gn,
        code: ErrorCode::LogindFailure,
        message: "scope failed".into(),
    }));
    assert!(sim.m.retained_failure().is_some());
    sim.greeter(ClientMessage::CancelOperation { operation_id: 0 });
    assert!(sim.m.retained_failure().is_none());
}

#[test]
fn cancelling_a_launch_closes_that_session_only() {
    let mut sim = Sim::new();
    sim.boot();
    sim.hold();
    sim.select(ALICE);
    sim.authenticate(ALICE, AuthIntent::Login);
    let sid = sim.sid_of(ALICE);
    let op = sim.current_op().id;
    let mark = sim.mark();
    sim.greeter(ClientMessage::CancelOperation { operation_id: op });
    assert!(sim.m.operation().is_none());
    assert!(sim.helper_reqs(mark).iter().any(|r| matches!(r, HelperRequest::CloseSession { session, .. } if session == &sid)));
    assert!(sim.m.session(&sid).is_none());
    assert_eq!(sim.m.history().last().unwrap().phase, OperationPhase::Cancelled);
}

#[test]
fn reduced_motion_still_runs_the_same_gates() {
    let mut sim = Sim::with_policy(Policy { greeter_uid: Some(GREETER_UID), reduce_motion: true, ..Policy::default() });
    sim.boot();
    sim.hold();
    sim.select(ALICE);
    sim.authenticate(ALICE, AuthIntent::Login);
    let sid = sim.sid_of(ALICE);
    let mark = sim.mark();
    sim.bridge_to_gnoblin_ready(&sid);
    sim.bridge_outputs(&sid);
    assert!(sim.begin_transitions(mark).is_empty());
    sim.bridge_bingux(&sid);
    assert!(sim.begin_transitions(mark).is_empty());
    sim.bridge_present(&sid);
    let begins = sim.begin_transitions(mark);
    assert!(matches!(&begins[0], SceneCommand::BeginTransition { reduce_motion: true, .. }));
}

#[test]
fn the_greeter_owns_input_from_the_moment_it_connects_and_after_every_reconnect() {
    let mut sim = Sim::new();
    sim.feed(Input::Start { persisted: Persisted::default() });
    sim.feed(Input::Accounts(accounts()));
    sim.feed(Input::HelperConnected);
    sim.feed(Input::Scene(SceneEvent::Ready { outputs: sim.outputs.clone() }));
    assert_eq!(sim.input_owner(), InputOwner::None, "no greeter yet, nobody types");
    let mark = sim.mark();
    let g = sim.m.greeter.generation.unwrap();
    sim.feed(Input::Scene(SceneEvent::GreeterConnected { generation: g, pid: GREETER_PID }));
    assert_eq!(sim.set_input_owner_cmds(mark), vec![InputOwner::Greeter]);
    assert_eq!(sim.input_owner(), InputOwner::Greeter);
    // The greeter dies: the compositor drops the owner with it; the new one gets it again.
    sim.feed(Input::Scene(SceneEvent::GreeterGone { generation: g }));
    assert_eq!(sim.input_owner(), InputOwner::None);
    sim.advance(std::time::Duration::from_secs(2));
    let g2 = sim.m.greeter.generation.unwrap();
    let mark = sim.mark();
    sim.feed(Input::Scene(SceneEvent::GreeterConnected { generation: g2, pid: GREETER_PID }));
    assert_eq!(sim.set_input_owner_cmds(mark), vec![InputOwner::Greeter]);
    // Also when the greeter process exits (helper report) while locked.
    sim.conn += 1;
    sim.feed(Input::GreeterPeer { conn: sim.conn, uid: GREETER_UID, pid: GREETER_PID });
    sim.greeter(ClientMessage::Hello { version: bsm_proto::PROTOCOL_VERSION });
    let sid = sim.login(ALICE);
    sim.call(Sim::session_caller(&sim, &sid), Method::RequestLock { session: None }).unwrap();
    assert_eq!(sim.input_owner(), InputOwner::Greeter);
    sim.feed(Input::Helper(bsm_proto::helper::HelperEvent::GreeterExited { generation: g2, status: 1 }));
    assert_eq!(sim.input_owner(), InputOwner::None);
    sim.advance(std::time::Duration::from_secs(2));
    let g3 = sim.m.greeter.generation.unwrap();
    let mark = sim.mark();
    sim.feed(Input::Scene(SceneEvent::GreeterConnected { generation: g3, pid: GREETER_PID }));
    assert_eq!(sim.set_input_owner_cmds(mark), vec![InputOwner::Greeter]);
    assert!(sim.m.check_invariants().is_ok());
}
