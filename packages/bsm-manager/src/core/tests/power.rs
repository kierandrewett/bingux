//! Power, suspend and sleep preparation.

use std::time::Duration;

use bsm_proto::control::ClientMessage;
use bsm_proto::error::{BsmError, ErrorCode};
use bsm_proto::scene::{SceneCommand, TransitionPhaseEvent};
use bsm_proto::state::{InputOwner, OperationPhase, PowerAction, SeatState, SessionState, TransitionKind};

use super::*;
use crate::core::*;

fn polkit_action(sim: &Sim, mark: usize) -> Option<String> {
    sim.since(mark).iter().find_map(|e| match e {
        Effect::Polkit { action_id, .. } => Some(action_id.clone()),
        _ => None,
    })
}

fn system_power(sim: &Sim, mark: usize) -> Vec<PowerAction> {
    sim.since(mark)
        .iter()
        .filter_map(|e| match e {
            Effect::SystemPower { action, .. } => Some(*action),
            _ => None,
        })
        .collect()
}

fn allow(sim: &mut Sim, op: u64) {
    sim.feed(Input::PolkitResult { operation: op, authorised: true });
    sim.feed(Input::InhibitorsResult { operation: op, result: Ok(vec![]) });
}

#[test]
fn a_session_reboot_needs_polkit_and_the_inhibitor_check_before_any_animation() {
    let mut sim = Sim::new();
    sim.boot();
    let a = sim.login(ALICE);
    sim.hold();
    let mark = sim.mark();
    let Reply::Operation(op) = sim.call(sim.session_caller(&a), Method::RequestReboot).unwrap() else { panic!() };
    assert_eq!(polkit_action(&sim, mark).as_deref(), Some("org.freedesktop.login1.reboot"));
    assert!(sim.begin_transitions(mark).is_empty(), "no fade before authorisation");
    assert_eq!(sim.current_op().phase, OperationPhase::Authorising);
    // One answer is not enough.
    sim.feed(Input::PolkitResult { operation: op, authorised: true });
    assert!(sim.begin_transitions(mark).is_empty());
    sim.feed(Input::InhibitorsResult { operation: op, result: Ok(vec![]) });
    let begins = sim.begin_transitions(mark);
    assert_eq!(begins.len(), 1);
    assert!(matches!(&begins[0], SceneCommand::BeginTransition { kind: TransitionKind::PowerFade, power: Some(PowerAction::Reboot), .. }));
    // logind is called only once the cover is committed.
    assert!(system_power(&sim, mark).is_empty());
    sim.phase(op, TransitionPhaseEvent::Animating);
    assert!(system_power(&sim, mark).is_empty());
    sim.phase(op, TransitionPhaseEvent::Committed);
    assert_eq!(system_power(&sim, mark), vec![PowerAction::Reboot]);
    assert_eq!(sim.input_owner(), InputOwner::None, "nobody types into a dying system");
    sim.phase(op, TransitionPhaseEvent::Completed);
    assert!(sim.m.operation().is_some(), "the operation ends when logind answers, not when the fade does");
    sim.feed(Input::PowerResult { operation: op, result: Ok(()) });
    assert!(sim.m.operation().is_none());
    assert_eq!(sim.m.seat(), SeatState::ShuttingDown);
    assert_eq!(sim.m.history().last().unwrap().phase, OperationPhase::Completed);
    // The cover stays: another request is refused.
    let e = sim.call(Sim::root(), Method::RequestPowerOff).unwrap_err();
    assert_eq!(e.code, ErrorCode::Busy);
}

#[test]
fn polkit_denial_stops_everything() {
    let mut sim = Sim::new();
    sim.boot();
    let a = sim.login(ALICE);
    let mark = sim.mark();
    let Reply::Operation(op) = sim.call(sim.session_caller(&a), Method::RequestPowerOff).unwrap() else { panic!() };
    sim.feed(Input::PolkitResult { operation: op, authorised: false });
    assert!(sim.m.operation().is_none());
    assert_eq!(sim.m.history().last().unwrap().reason, Some(ErrorCode::Unauthorised));
    assert!(sim.begin_transitions(mark).is_empty());
    assert!(system_power(&sim, mark).is_empty());
    // A late "allowed" for the dead operation changes nothing.
    sim.feed(Input::InhibitorsResult { operation: op, result: Ok(vec![]) });
    assert!(sim.begin_transitions(mark).is_empty());
    assert_eq!(sim.input_owner(), InputOwner::Session(a));
}

#[test]
fn the_multiple_sessions_action_is_used_when_others_are_signed_in() {
    let mut sim = Sim::new();
    sim.boot();
    let a = sim.login(ALICE);
    sim.call(sim.session_caller(&a), Method::RequestSwitchUser).unwrap();
    sim.select(BOB);
    sim.authenticate(BOB, AuthIntent::Switch);
    let b = sim.sid_of(BOB);
    sim.bridge_full(&b);
    let mark = sim.mark();
    sim.call(sim.session_caller(&b), Method::RequestPowerOff).unwrap();
    assert_eq!(polkit_action(&sim, mark).as_deref(), Some("org.freedesktop.login1.power-off-multiple-sessions"));
}

use bsm_proto::state::AuthIntent;

#[test]
fn a_blocking_inhibitor_fails_the_request_and_names_who() {
    let mut sim = Sim::new();
    sim.boot();
    let mark = sim.mark();
    sim.greeter(ClientMessage::Power { request_id: 1, action: PowerAction::Reboot });
    let op = sim.current_op().id;
    let block = Inhibitor { what: "shutdown:sleep".into(), who: "Backup".into(), why: "copying".into(), mode: "block".into(), uid: 1000 };
    let delay = Inhibitor { what: "shutdown".into(), who: "Other".into(), why: "x".into(), mode: "delay".into(), uid: 1000 };
    sim.feed(Input::InhibitorsResult { operation: op, result: Ok(vec![delay, block]) });
    assert!(sim.m.operation().is_none());
    assert_eq!(sim.m.history().last().unwrap().reason, Some(ErrorCode::Inhibited));
    let msgs = sim.greeter_msgs(mark);
    assert!(msgs.iter().any(|m| matches!(m, bsm_proto::control::ServerMessage::Error { code: ErrorCode::Inhibited, message, .. } if message.contains("Backup") && !message.contains("Other"))));
    assert!(sim.begin_transitions(mark).is_empty());
}

#[test]
fn delay_inhibitors_do_not_block() {
    let mut sim = Sim::new();
    sim.boot();
    sim.hold();
    sim.greeter(ClientMessage::Power { request_id: 1, action: PowerAction::PowerOff });
    let op = sim.current_op().id;
    let delay = Inhibitor { what: "shutdown".into(), who: "Other".into(), why: "x".into(), mode: "delay".into(), uid: 1000 };
    sim.feed(Input::InhibitorsResult { operation: op, result: Ok(vec![delay]) });
    assert_eq!(sim.current_op().kind, TransitionKind::PowerFade);
    assert_eq!(sim.current_op().phase, OperationPhase::Preparing);
}

#[test]
fn logind_refusal_restores_the_desktop_scene_and_input() {
    let mut sim = Sim::new();
    sim.boot();
    let a = sim.login(ALICE);
    sim.hold();
    let Reply::Operation(op) = sim.call(sim.session_caller(&a), Method::RequestReboot).unwrap() else { panic!() };
    allow(&mut sim, op);
    sim.phase(op, TransitionPhaseEvent::Animating);
    sim.phase(op, TransitionPhaseEvent::Committed);
    sim.phase(op, TransitionPhaseEvent::Completed);
    assert_eq!(sim.input_owner(), InputOwner::None);
    let mark = sim.mark();
    sim.feed(Input::PowerResult { operation: op, result: Err(BsmError::new(ErrorCode::LogindFailure, "denied")) });
    // The compositor is asked to take the cover away; input stays revoked until it says the
    // scene is back (a SetInputOwner before that would be refused and lost).
    assert!(sim.scene_cmds(mark).iter().any(|c| matches!(c, SceneCommand::CancelTransition { operation } if *operation == op)));
    assert_eq!(sim.input_owner(), InputOwner::None);
    assert!(sim.m.operation().is_some());
    assert!(sim.set_input_owner_cmds(mark).is_empty());
    sim.phase(op, TransitionPhaseEvent::Cancelled);
    assert!(sim.m.operation().is_none());
    assert_eq!(sim.m.history().last().unwrap().reason, Some(ErrorCode::ShutdownRejected));
    assert_eq!(sim.input_owner(), InputOwner::Session(a.clone()));
    assert_eq!(sim.m.seat(), SeatState::UserForeground);
    assert_ne!(sim.m.seat(), SeatState::ShuttingDown);
    // And a new request is possible.
    assert!(sim.call(Sim::root(), Method::RequestPowerOff).is_ok());
}

#[test]
fn logind_silence_ends_in_a_failure_not_a_black_screen() {
    let mut sim = Sim::new();
    sim.boot();
    sim.hold();
    sim.greeter(ClientMessage::Power { request_id: 1, action: PowerAction::PowerOff });
    let op = sim.current_op().id;
    allow(&mut sim, op);
    sim.phase(op, TransitionPhaseEvent::Committed);
    let mark = sim.mark();
    sim.advance(Duration::from_secs(31));
    assert!(sim.scene_cmds(mark).iter().any(|c| matches!(c, SceneCommand::CancelTransition { .. })));
    // Even a compositor that never confirms the restore cannot leave the seat shutting down forever.
    sim.advance(Duration::from_secs(6));
    assert!(sim.m.operation().is_none());
    assert_ne!(sim.m.seat(), SeatState::ShuttingDown);
}

#[test]
fn power_checks_that_never_answer_time_out() {
    let mut sim = Sim::new();
    sim.boot();
    let a = sim.login(ALICE);
    sim.call(sim.session_caller(&a), Method::RequestReboot).unwrap();
    sim.advance(Duration::from_secs(16));
    assert!(sim.m.operation().is_none());
    assert_eq!(sim.m.history().last().unwrap().reason, Some(ErrorCode::Timeout));
    assert_eq!(sim.input_owner(), InputOwner::Session(a));
}

#[test]
fn the_greeter_may_only_do_what_policy_allows_and_only_while_it_is_active() {
    let mut sim = Sim::with_policy(Policy {
        greeter_uid: Some(GREETER_UID),
        greeter_power: GreeterPower { suspend: false, reboot: true, poweroff: true },
        ..Policy::default()
    });
    sim.boot();
    let mark = sim.mark();
    sim.greeter(ClientMessage::Power { request_id: 4, action: PowerAction::Suspend });
    assert!(sim.m.operation().is_none());
    assert!(sim.greeter_msgs(mark).iter().any(|m| matches!(m, bsm_proto::control::ServerMessage::Error { request_id: Some(4), code: ErrorCode::Unauthorised, .. })));
    // With a desktop in front the greeter has no standing at all.
    let _a = sim.login(ALICE);
    let mark = sim.mark();
    sim.greeter(ClientMessage::Power { request_id: 5, action: PowerAction::Reboot });
    assert!(sim.m.operation().is_none());
    assert!(sim.since(mark).iter().all(|e| !matches!(e, Effect::QueryInhibitors { .. })));
}

#[test]
fn root_needs_no_polkit_but_still_the_inhibitor_check() {
    let mut sim = Sim::new();
    sim.boot();
    let mark = sim.mark();
    let Reply::Operation(_) = sim.call(Sim::root(), Method::RequestReboot).unwrap() else { panic!() };
    assert!(polkit_action(&sim, mark).is_none());
    assert!(sim.since(mark).iter().any(|e| matches!(e, Effect::QueryInhibitors { .. })));
}

#[test]
fn suspend_protects_first_then_locks_then_sleeps_and_resume_lands_on_the_greeter() {
    let mut sim = Sim::new();
    sim.boot();
    let a = sim.login(ALICE);
    sim.comp.withhold_protect = true;
    let mark = sim.mark();
    let Reply::Operation(op) = sim.call(sim.session_caller(&a), Method::RequestSuspend).unwrap() else { panic!() };
    allow(&mut sim, op);
    // Protect is sent, but nothing else happens until it is acknowledged.
    assert!(sim.scene_cmds(mark).iter().any(|c| matches!(c, SceneCommand::Protect { session, .. } if session == &a)));
    assert!(sim.begin_transitions(mark).is_empty());
    assert!(system_power(&sim, mark).is_empty());
    assert_eq!(sim.input_owner(), InputOwner::None);
    sim.comp.withhold_protect = false;
    sim.hold();
    let gn = sim.gen_of(&a);
    sim.feed(Input::Scene(bsm_proto::scene::SceneEvent::Protected { session: a.clone(), generation: gn }));
    assert_eq!(sim.state(&a), SessionState::LockedGreeterForeground);
    assert_eq!(sim.begin_transitions(mark).len(), 1);
    assert!(system_power(&sim, mark).is_empty());
    sim.phase(op, TransitionPhaseEvent::Committed);
    assert_eq!(system_power(&sim, mark), vec![PowerAction::Suspend]);
    // logind announces the sleep: nothing more to protect, the delay inhibitor is released at once.
    sim.feed(Input::PrepareForSleep { start: true });
    assert!(sim.since(mark).iter().any(|e| matches!(e, Effect::SleepReady)));
    sim.feed(Input::PowerResult { operation: op, result: Ok(()) });
    assert_eq!(sim.m.history().last().unwrap().phase, OperationPhase::Completed);
    // Resume: everything protected stays protected, the greeter is what shows.
    let mark = sim.mark();
    sim.feed(Input::PrepareForSleep { start: false });
    let cmds = sim.scene_cmds(mark);
    assert!(cmds.contains(&SceneCommand::SetDpms { on: true }));
    assert!(cmds.iter().any(|c| matches!(c, SceneCommand::Protect { session, .. } if session == &a)));
    assert!(cmds.contains(&SceneCommand::ShowGreeterOnly));
    assert_eq!(sim.input_owner(), InputOwner::Greeter);
    assert_eq!(sim.state(&a), SessionState::LockedGreeterForeground);
}

#[test]
fn a_failed_suspend_leaves_the_session_locked() {
    let mut sim = Sim::new();
    sim.boot();
    let a = sim.login(ALICE);
    let Reply::Operation(op) = sim.call(sim.session_caller(&a), Method::RequestSuspend).unwrap() else { panic!() };
    allow(&mut sim, op);
    assert!(sim.m.operation().is_some_and(|o| o.phase == OperationPhase::Committed || o.phase == OperationPhase::Completed));
    sim.feed(Input::PowerResult { operation: op, result: Err(BsmError::new(ErrorCode::LogindFailure, "no")) });
    assert_eq!(sim.state(&a), SessionState::LockedGreeterForeground);
    assert!(sim.m.protected().contains(&a));
    assert_ne!(sim.input_owner(), InputOwner::Session(a));
}

#[test]
fn sleep_started_by_logind_protects_a_visible_desktop_before_releasing_the_inhibitor() {
    let mut sim = Sim::new();
    sim.boot();
    let a = sim.login(ALICE);
    sim.comp.withhold_protect = true;
    let mark = sim.mark();
    sim.feed(Input::PrepareForSleep { start: true });
    assert!(sim.scene_cmds(mark).iter().any(|c| matches!(c, SceneCommand::Protect { .. })));
    assert!(sim.since(mark).iter().all(|e| !matches!(e, Effect::SleepReady)), "sleep waits for the acknowledgement");
    assert_eq!(sim.input_owner(), InputOwner::None);
    let gn = sim.gen_of(&a);
    sim.feed(Input::Scene(bsm_proto::scene::SceneEvent::Protected { session: a.clone(), generation: gn }));
    assert!(sim.since(mark).iter().any(|e| matches!(e, Effect::SleepReady)));
    assert_eq!(sim.state(&a), SessionState::LockedGreeterForeground);
    assert_eq!(sim.input_owner(), InputOwner::Greeter);
}

#[test]
fn sleep_preparation_cannot_block_sleep_forever() {
    let mut sim = Sim::new();
    sim.boot();
    let _a = sim.login(ALICE);
    sim.comp.withhold_protect = true;
    let mark = sim.mark();
    sim.feed(Input::PrepareForSleep { start: true });
    sim.advance(Duration::from_secs(7));
    assert!(sim.since(mark).iter().any(|e| matches!(e, Effect::SleepReady)));
    assert!(sim.scene_cmds(mark).contains(&SceneCommand::ShowGreeterOnly));
}
