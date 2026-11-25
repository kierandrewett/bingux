//! The conversion of the engine's page and lifecycle events, from hand-made
//! values, and the delegate driven by the crate's mock engine: no engine is
//! needed to say that an event reaches the UI as the event it is, and that a
//! question waits for the person and is answered once.

use std::sync::Arc;

use vivarium::delegate::mock::{MockEngine, SentBody};
use vivarium::error::Status;
use vivarium::event::EventKind;
use vivarium::navigation::{NewWindowFlags, TargetWindow};
use vivarium::permissions::{PermissionItemFlags, PermissionState, PermissionType};
use vivarium::request::{
    Remember, RequestCloseReason, RequestKind as Kind, RequestPayload, Verdict,
};

use super::*;
use crate::web_view::requests::{PermissionKind, Response};

fn state(state: LoadState, progress: u32, nsresult: u32, name: &str) -> Load {
    Load {
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
        load(&state(LoadState::Started, 0, 0, "")),
        TabEvent::Load {
            started: true,
            loading: true,
            progress: 0.0,
            error: None
        }
    );
    assert_eq!(
        load(&state(LoadState::Progress, 450, 0, "")),
        TabEvent::Load {
            started: false,
            loading: true,
            progress: 0.45,
            error: None
        }
    );
    assert_eq!(
        load(&state(LoadState::Committed, 2000, 0, "")),
        TabEvent::Load {
            started: false,
            loading: true,
            progress: 1.0,
            error: None
        },
        "a progress past the end is the end"
    );
    assert_eq!(
        load(&state(LoadState::Stopped, 1000, 0, "")),
        TabEvent::Load {
            started: false,
            loading: false,
            progress: 1.0,
            error: None
        }
    );
}

#[test]
fn a_load_that_failed_names_what_failed() {
    let TabEvent::Load { loading, error, .. } = load(&state(
        LoadState::Stopped,
        0,
        0x804B_001E,
        "NS_ERROR_UNKNOWN_HOST",
    )) else {
        panic!("a load")
    };
    assert!(!loading);
    assert_eq!(
        error,
        Some(LoadError {
            code: 0x804B_001E,
            name: "NS_ERROR_UNKNOWN_HOST".to_string()
        })
    );
    // The error of a load that is still going is not the end of it.
    let TabEvent::Load { error, .. } = load(&state(
        LoadState::Progress,
        10,
        0x804B_001E,
        "NS_ERROR_UNKNOWN_HOST",
    )) else {
        panic!("a load")
    };
    assert_eq!(error, None);
}

#[test]
fn a_load_the_person_stopped_is_not_a_page_that_failed() {
    let TabEvent::Load { error, loading, .. } = load(&state(
        LoadState::Stopped,
        300,
        NS_BINDING_ABORTED,
        "NS_BINDING_ABORTED",
    )) else {
        panic!("a load")
    };
    assert_eq!((loading, error), (false, None));
}

fn failure(status: Status, exit_code: u32, flags: EngineFailedFlags) -> EngineFailure {
    EngineFailure {
        status: Some(status),
        exit_code,
        flags,
        message: "the engine stopped".to_string(),
    }
}

#[test]
fn an_engine_that_died_is_restartable_when_it_says_so() {
    let died = failed_event(&failure(
        Status::EngineFailed,
        137,
        EngineFailedFlags::RESTARTABLE,
    ));
    assert_eq!(
        died,
        EngineEvent::Failed {
            message: "the engine stopped".to_string(),
            code: 137,
            cause: FailureCause::Crashed,
            restartable: true
        }
    );
    // `kill -9` sets no flag: the engine could not say anything. Trying again is harmless.
    let EngineEvent::Failed { restartable, .. } = failed_event(&failure(
        Status::EngineFailed,
        137,
        EngineFailedFlags::empty(),
    )) else {
        panic!("a failure")
    };
    assert!(
        restartable,
        "a crash is offered a restart even when the engine could not say it is safe"
    );
    let EngineEvent::Failed { restartable, .. } =
        failed_event(&failure(Status::Protocol, 0, EngineFailedFlags::empty()))
    else {
        panic!("a failure")
    };
    assert!(
        !restartable,
        "a protocol violation is not, unless the engine says so"
    );
    let EngineEvent::Failed { restartable, .. } = failed_event(&failure(
        Status::EngineFailed,
        1,
        EngineFailedFlags::SANDBOX_FLOOR,
    )) else {
        panic!("a failure")
    };
    assert!(
        !restartable,
        "the same engine would fail the same floor again"
    );
}

#[test]
fn a_protocol_violation_is_not_a_crash() {
    for status in [Status::Protocol, Status::Limit, Status::OutOfMemory] {
        assert_eq!(
            failure_cause(&failure(status, 0, EngineFailedFlags::empty())),
            FailureCause::ProtocolViolation,
            "{status:?}"
        );
    }
    let EngineEvent::Failed { cause, .. } = failed_event(&failure(
        Status::Protocol,
        0,
        EngineFailedFlags::RESTARTABLE,
    )) else {
        panic!("a failure")
    };
    assert_eq!(cause, FailureCause::ProtocolViolation);
}

#[test]
fn a_locked_profile_and_a_sandbox_that_was_not_met_are_told_apart() {
    assert_eq!(
        failure_cause(&failure(
            Status::EngineFailed,
            1,
            EngineFailedFlags::SANDBOX_FLOOR
        )),
        FailureCause::SandboxFloor
    );
    let locked = failed_event(&failure(
        Status::EngineFailed,
        1,
        EngineFailedFlags::PROFILE_LOCKED,
    ));
    assert_eq!(
        locked,
        EngineEvent::Failed {
            message: "the engine stopped".to_string(),
            code: 1,
            cause: FailureCause::ProfileLocked,
            restartable: true
        },
        "trying again after the other window quit is harmless"
    );
}

#[test]
fn an_exit_by_signal_keeps_its_number() {
    let killed = failure(Status::EngineFailed, 128 + 9, EngineFailedFlags::empty());
    assert_eq!(killed.signal(), Some(9));
    let EngineEvent::Failed { code, .. } = failed_event(&killed) else {
        panic!("a failure")
    };
    assert_eq!(code, 137);
}

// ---- The delegate, driven by the crate's mock engine ------------------------

fn session(id: u32) -> SessionId {
    SessionId::from_raw(id).expect("a session id is not 0")
}

/// Dispatches what `mock` has queued into a fresh backend state, and returns
/// the state and what the UI was told.
fn dispatch(mock: &Arc<MockEngine>, shared: &Shared) -> (Vec<BrowserEvent>, bool) {
    let mut host = Host {
        shared,
        downloads: Path::new("/home/u/Downloads"),
        failed: false,
    };
    mock.dispatcher()
        .dispatch(&mut host)
        .expect("the mock dispatches");
    let mut out = Vec::new();
    shared.queue.drain_into(&mut out);
    (out, host.failed)
}

fn camera(mock: &MockEngine) -> vivarium::request::Request {
    let mut request = mock.request(
        Kind::Permission,
        RequestPayload::Permission(vivarium::permissions::PermissionRequest {
            items: vec![vivarium::permissions::PermissionItem {
                kind: PermissionType::Camera,
                flags: PermissionItemFlags::empty(),
                type_name: "camera".to_string(),
                detail: String::new(),
            }],
            stored_state: PermissionState::Default,
        }),
    );
    request.origin.serialized = "https://example.org".to_string();
    request
}

#[test]
fn page_events_reach_the_ui_as_events_of_their_tab() {
    let mock = MockEngine::new();
    mock.emit(None, EventKind::EngineReady);
    mock.emit(
        Some(session(3)),
        EventKind::Title("Example Domain".to_string()),
    );
    mock.emit(Some(session(3)), EventKind::StatusText(String::new()));
    let shared = Shared::new();
    let (events, failed) = dispatch(&mock, &shared);
    assert!(!failed);
    assert_eq!(
        events,
        [
            BrowserEvent::Engine(EngineEvent::Ready),
            BrowserEvent::tab(
                crate::web_view::TabId(3),
                TabEvent::Title("Example Domain".to_string())
            ),
            BrowserEvent::tab(crate::web_view::TabId(3), TabEvent::HoverLink(None)),
        ]
    );
}

#[test]
fn a_question_waits_for_the_person_and_their_answer_reaches_the_engine_once() {
    let mock = MockEngine::new();
    let request = camera(&mock);
    let engine_id = request.id;
    mock.emit(Some(session(3)), EventKind::Request(Box::new(request)));
    let shared = Shared::new();
    let (events, _) = dispatch(&mock, &shared);
    let [BrowserEvent::Request { tab, request }] = events.as_slice() else {
        panic!("one question, not {events:?}")
    };
    assert_eq!(*tab, crate::web_view::TabId(3));
    let crate::web_view::RequestKind::Permission(permission) = &request.kind else {
        panic!("a permission")
    };
    assert_eq!(
        (permission.origin.as_str(), permission.kinds.as_slice()),
        ("https://example.org", &[PermissionKind::Camera][..])
    );
    assert!(
        mock.sent().is_empty(),
        "nothing is answered before the person chose"
    );

    let asked = lock(&shared.state)
        .requests
        .remove(&request.id)
        .expect("the responder waits");
    requests::answer(
        asked.open,
        Response::Permission {
            allow: true,
            remember: true,
        },
    )
    .expect("the answer is sent");
    let sent = mock.sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(
        (sent[0].id, sent[0].verdict),
        (engine_id, Some(Verdict::Accept))
    );
    assert!(
        matches!(
            &sent[0].body,
            SentBody::Permission {
                remember: Remember::Session,
                ..
            }
        ),
        "kept while the window is open, as the settings page says: {:?}",
        sent[0].body
    );
}

#[test]
fn an_answer_to_another_question_is_refused_and_the_engine_gets_its_default() {
    let mock = MockEngine::new();
    mock.emit(
        Some(session(3)),
        EventKind::Request(Box::new(camera(&mock))),
    );
    let shared = Shared::new();
    let (events, _) = dispatch(&mock, &shared);
    let [BrowserEvent::Request { request, .. }] = events.as_slice() else {
        panic!("one question")
    };
    let asked = lock(&shared.state)
        .requests
        .remove(&request.id)
        .expect("the responder waits");
    assert!(matches!(
        requests::answer(asked.open, Response::CertError { proceed: true }),
        Err(requests::Unanswered::Mismatch)
    ));
    let sent = mock.sent();
    assert_eq!(
        sent.len(),
        1,
        "a certificate answer must not grant a permission; the default was sent instead"
    );
    assert_eq!(
        sent[0].verdict, None,
        "dismissed: the engine applies the default of the kind"
    );
}

#[test]
fn a_question_the_engine_ended_is_taken_back_from_the_ui() {
    let mock = MockEngine::new();
    let request = camera(&mock);
    let engine_id = request.id;
    mock.emit(Some(session(3)), EventKind::Request(Box::new(request)));
    let shared = Shared::new();
    let (events, _) = dispatch(&mock, &shared);
    let [BrowserEvent::Request { request, .. }] = events.as_slice() else {
        panic!("one question")
    };
    let ours = request.id;
    mock.close(engine_id);
    mock.emit(
        Some(session(3)),
        EventKind::RequestClosed(vivarium::request::RequestClosed {
            reason: RequestCloseReason::Timeout,
            request_id: engine_id,
            kind: Kind::Permission,
        }),
    );
    let (events, _) = dispatch(&mock, &shared);
    assert_eq!(
        events,
        [BrowserEvent::RequestClosed {
            tab: crate::web_view::TabId(3),
            request: ours
        }]
    );
    assert!(lock(&shared.state).requests.is_empty());
}

#[test]
fn a_new_window_waits_as_a_tab_to_take_and_one_not_taken_is_refused() {
    let mock = MockEngine::new();
    let request = mock.request(
        Kind::NewWindow,
        RequestPayload::NewWindow(vivarium::navigation::NewWindowRequest {
            target_window: TargetWindow::NewTabBackground,
            flags: NewWindowFlags::USER_GESTURE,
            new_session: Some(session(0x8000_0001)),
            position: (0, 0),
            size: (0, 0),
            url: "https://popup.example/".to_string(),
            target_name: "_blank".to_string(),
            referrer: String::new(),
        }),
    );
    mock.emit(Some(session(3)), EventKind::Request(Box::new(request)));
    let shared = Shared::new();
    let (events, _) = dispatch(&mock, &shared);
    let [
        BrowserEvent::Tab {
            tab,
            event: TabEvent::NewTab {
                new, url, window, ..
            },
        },
    ] = events.as_slice()
    else {
        panic!("a new tab, not {events:?}")
    };
    assert_eq!(
        (*tab, *new, url.as_str()),
        (
            crate::web_view::TabId(3),
            crate::web_view::TabId(0x8000_0001),
            "https://popup.example/"
        )
    );
    assert_eq!(
        window.disposition,
        crate::web_view::Disposition::BackgroundTab
    );
    assert!(window.user_gesture);
    let reply = lock(&shared.state)
        .windows
        .remove(new)
        .expect("the responder waits for the browser");
    reply
        .answer(vivarium::navigation::NewWindowReply::Refuse)
        .expect("refused");
    assert_eq!(mock.sent()[0].verdict, Some(Verdict::Refuse));
}

#[test]
fn a_failed_engine_lets_go_of_every_question_and_says_why() {
    let mock = MockEngine::new();
    mock.emit(
        Some(session(3)),
        EventKind::Request(Box::new(camera(&mock))),
    );
    let shared = Shared::new();
    dispatch(&mock, &shared);
    assert_eq!(lock(&shared.state).requests.len(), 1);
    mock.emit(
        None,
        EventKind::EngineFailed(failure(
            Status::EngineFailed,
            137,
            EngineFailedFlags::empty(),
        )),
    );
    let (events, failed) = dispatch(&mock, &shared);
    assert!(failed, "nothing more comes from it");
    assert!(lock(&shared.state).requests.is_empty());
    assert!(matches!(
        events.as_slice(),
        [BrowserEvent::Engine(EngineEvent::Failed {
            cause: FailureCause::Crashed,
            restartable: true,
            ..
        })]
    ));
}

// ---- Downloads, through the mock engine --------------------------------------
//
// Failures these guard against:
//   1. a download event reaches the list without the flag that says it is
//      private, so a normal window lists it;
//   2. a normal download is marked private;
//   3. a download of a state the browser has no word for (a removed one) is
//      listed as if it were running.
//
// The result of a search is an `OperationResult` and its live counts carry an
// `OperationId`, neither of which a host outside the crate can make, so those
// paths are tested through `requests::find_settled` and the real engine.

fn download_event(state: vivarium::downloads::DownloadState, flags: u32) -> EventKind {
    EventKind::Download(vivarium::downloads::DownloadEvent {
        state,
        download_id: vivarium::downloads::DownloadId::from_raw(11),
        flags,
        error_code: 0,
        received: 40,
        total: Some(100),
        speed_bps: 4,
        url: "https://example.com/a.zip".to_string(),
        target_path: "/home/u/Downloads/a.zip".to_string(),
        filename: "a.zip".to_string(),
        mime: "application/zip".to_string(),
        error: String::new(),
    })
}

#[test]
fn a_download_reaches_the_list_with_its_privacy() {
    use vivarium::downloads::DownloadState as State;
    let mock = MockEngine::new();
    mock.emit(
        None,
        download_event(State::Progress, vivarium_sys::VV_DOWNLOAD_IS_PRIVATE),
    );
    mock.emit(
        None,
        download_event(State::Progress, vivarium_sys::VV_DOWNLOAD_CAN_PAUSE),
    );
    let shared = Shared::new();
    let (events, _) = dispatch(&mock, &shared);
    let [
        BrowserEvent::Download(private),
        BrowserEvent::Download(normal),
    ] = events.as_slice()
    else {
        panic!("two downloads, not {events:?}")
    };
    assert!(private.private && !normal.private);
    assert_eq!(
        (private.id, private.state, private.received),
        (
            crate::web_view::DownloadId(11),
            crate::web_view::requests::DownloadState::Running,
            40
        )
    );
}

#[test]
fn a_download_the_engine_forgot_is_not_listed() {
    use vivarium::downloads::DownloadState as State;
    let mock = MockEngine::new();
    mock.emit(None, download_event(State::Removed, 0));
    let shared = Shared::new();
    let (events, _) = dispatch(&mock, &shared);
    assert!(events.is_empty(), "{events:?}");
}
