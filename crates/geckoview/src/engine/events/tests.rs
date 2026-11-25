//! The conversion of the engine's page and lifecycle events, from hand-made
//! values.

use super::*;

fn state(state: vv::LoadState, progress: u32, nsresult: u32, name: &str) -> vv::Load {
    vv::Load {
        state,
        progress,
        can_back: false,
        can_forward: false,
        nsresult,
        http_status: 0,
        flags: 0,
        error_name: name.to_string(),
    }
}

#[test]
fn a_load_reports_its_start_its_progress_and_its_end() {
    assert_eq!(
        load(&state(vv::LoadState::Started, 0, 0, "")),
        TabEvent::Load { started: true, loading: true, progress: 0.0, error: None }
    );
    assert_eq!(
        load(&state(vv::LoadState::Progress, 450, 0, "")),
        TabEvent::Load { started: false, loading: true, progress: 0.45, error: None }
    );
    assert_eq!(
        load(&state(vv::LoadState::Committed, 2000, 0, "")),
        TabEvent::Load { started: false, loading: true, progress: 1.0, error: None },
        "a progress past the end is the end"
    );
    assert_eq!(
        load(&state(vv::LoadState::Stopped, 1000, 0, "")),
        TabEvent::Load { started: false, loading: false, progress: 1.0, error: None }
    );
}

#[test]
fn a_load_that_failed_names_what_failed() {
    let TabEvent::Load { loading, error, .. } = load(&state(vv::LoadState::Stopped, 0, 0x804B_001E, "NS_ERROR_UNKNOWN_HOST")) else {
        panic!("a load")
    };
    assert!(!loading);
    assert_eq!(error, Some(LoadError { code: 0x804B_001E, name: "NS_ERROR_UNKNOWN_HOST".to_string() }));
    // The error of a load that is still going is not the end of it.
    let TabEvent::Load { error, .. } = load(&state(vv::LoadState::Progress, 10, 0x804B_001E, "NS_ERROR_UNKNOWN_HOST")) else { panic!("a load") };
    assert_eq!(error, None);
}

#[test]
fn a_load_the_person_stopped_is_not_a_page_that_failed() {
    let TabEvent::Load { error, loading, .. } = load(&state(vv::LoadState::Stopped, 300, NS_BINDING_ABORTED, "NS_BINDING_ABORTED")) else {
        panic!("a load")
    };
    assert_eq!((loading, error), (false, None));
}

fn failure(status: vv::Status, exit_code: u32, flags: vv::EngineFailedFlags) -> vv::EngineFailure {
    vv::EngineFailure {
        status: Some(status),
        exit_code,
        flags,
        message: "the engine stopped".to_string(),
    }
}

#[test]
fn an_engine_that_died_is_restartable_when_it_says_so() {
    let died = failed_event(&failure(vv::Status::EngineFailed, 137, vv::EngineFailedFlags::RESTARTABLE));
    assert_eq!(
        died,
        EngineEvent::Failed { message: "the engine stopped".to_string(), code: 137, cause: FailureCause::Crashed, restartable: true }
    );
    // `kill -9` sets no flag: the engine could not say anything. Trying again is harmless.
    let EngineEvent::Failed { restartable, .. } = failed_event(&failure(vv::Status::EngineFailed, 137, vv::EngineFailedFlags::empty())) else {
        panic!("a failure")
    };
    assert!(restartable, "a crash is offered a restart even when the engine could not say it is safe");
    let EngineEvent::Failed { restartable, .. } = failed_event(&failure(vv::Status::Protocol, 0, vv::EngineFailedFlags::empty())) else {
        panic!("a failure")
    };
    assert!(!restartable, "a protocol violation is not, unless the engine says so");
    let EngineEvent::Failed { restartable, .. } = failed_event(&failure(vv::Status::EngineFailed, 1, vv::EngineFailedFlags::SANDBOX_FLOOR)) else {
        panic!("a failure")
    };
    assert!(!restartable, "the same engine would fail the same floor again");
}

#[test]
fn a_protocol_violation_is_not_a_crash() {
    for status in [vv::Status::Protocol, vv::Status::Limit, vv::Status::OutOfMemory] {
        assert_eq!(failure_cause(&failure(status, 0, vv::EngineFailedFlags::empty())), FailureCause::ProtocolViolation, "{status:?}");
    }
    let EngineEvent::Failed { cause, .. } = failed_event(&failure(vv::Status::Protocol, 0, vv::EngineFailedFlags::RESTARTABLE)) else {
        panic!("a failure")
    };
    assert_eq!(cause, FailureCause::ProtocolViolation);
}

#[test]
fn a_locked_profile_and_a_sandbox_that_was_not_met_are_told_apart() {
    assert_eq!(
        failure_cause(&failure(vv::Status::EngineFailed, 1, vv::EngineFailedFlags::SANDBOX_FLOOR)),
        FailureCause::SandboxFloor
    );
    let locked = failed_event(&failure(vv::Status::EngineFailed, 1, vv::EngineFailedFlags::PROFILE_LOCKED));
    assert_eq!(
        locked,
        EngineEvent::Failed { message: "the engine stopped".to_string(), code: 1, cause: FailureCause::ProfileLocked, restartable: true },
        "trying again after the other window quit is harmless"
    );
}

#[test]
fn an_exit_by_signal_keeps_its_number() {
    let killed = failure(vv::Status::EngineFailed, 128 + 9, vv::EngineFailedFlags::empty());
    assert_eq!(killed.signal(), Some(9));
    let EngineEvent::Failed { code, .. } = failed_event(&killed) else { panic!("a failure") };
    assert_eq!(code, 137);
}
