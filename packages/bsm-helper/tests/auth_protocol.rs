//! The real helper binary, a real socket and real libpam (through a test
//! service directory): connection policy, the authentication conversation and
//! the ticket rules.

mod common;

use std::time::Duration;

use bsm_proto::error::ErrorCode;
use bsm_proto::helper::{HelperEvent, HelperRequest, PamPromptKind};
use bsm_proto::state::AuthIntent;
use common::*;

fn done(ev: &HelperEvent) -> bool {
    matches!(ev, HelperEvent::AuthDone { .. })
}

#[test]
fn ping_is_answered() {
    if skip_as_root() {
        return;
    }
    let env = Env::new();
    let _h = env.start();
    let c = env.connect();
    c.send(&HelperRequest::Ping { nonce: 77 });
    assert_eq!(c.recv(), HelperEvent::Pong { nonce: 77 });
}

#[test]
fn a_peer_with_another_uid_is_rejected() {
    if skip_as_root() {
        return;
    }
    let mut env = Env::new();
    env.allowed_uid = env.pam.uid + 1;
    let _h = env.start();
    let c = env.connect_raw();
    // The helper may close before the datagram is sent: both outcomes are a refusal.
    let _ = c.send_raw(&serde_json::to_vec(&HelperRequest::Ping { nonce: 1 }).unwrap());
    assert!(c.closed_within(Duration::from_secs(3)), "connection from the wrong uid must be closed");
}

#[test]
fn only_one_connection_at_a_time_and_a_restart_of_the_peer_is_accepted() {
    if skip_as_root() {
        return;
    }
    let env = Env::new();
    let _h = env.start();
    let first = env.connect();
    first.send(&HelperRequest::Ping { nonce: 1 });
    assert_eq!(first.recv(), HelperEvent::Pong { nonce: 1 });
    let second = env.connect_raw();
    assert!(second.closed_within(Duration::from_secs(3)));
    first.send(&HelperRequest::Ping { nonce: 2 });
    assert_eq!(first.recv(), HelperEvent::Pong { nonce: 2 });
    drop(first);
    // bsm restarting reconnects at once.
    let third = env.connect();
    third.send(&HelperRequest::Ping { nonce: 3 });
    assert_eq!(third.recv(), HelperEvent::Pong { nonce: 3 });
}

#[test]
fn garbage_is_ignored_and_oversized_datagrams_drop_the_connection() {
    if skip_as_root() {
        return;
    }
    let env = Env::new();
    let _h = env.start();
    let c = env.connect();
    c.send_raw(b"{not json").unwrap();
    c.send_raw(br#"{"type":"no_such_request"}"#).unwrap();
    c.send(&HelperRequest::Ping { nonce: 5 });
    assert_eq!(c.recv(), HelperEvent::Pong { nonce: 5 });
    c.send_raw(&vec![b' '; 64 * 1024 + 1]).unwrap();
    assert!(c.closed_within(Duration::from_secs(3)));
    // The helper itself is unharmed.
    let again = env.connect();
    again.send(&HelperRequest::Ping { nonce: 6 });
    assert_eq!(again.recv(), HelperEvent::Pong { nonce: 6 });
}

#[test]
fn successful_conversation_shows_info_then_hidden_prompt_and_returns_a_ticket() {
    if skip_as_root() {
        return;
    }
    let env = Env::new();
    let _h = env.start();
    let c = env.connect();
    c.auth_start(&env, 1, 100, AuthIntent::Login);
    let info = c.recv();
    assert!(
        matches!(&info, HelperEvent::AuthPrompt { req_id: 1, kind: PamPromptKind::Info, text } if text.contains("welcome banner")),
        "{info:?}"
    );
    let prompt = c.recv();
    assert!(matches!(prompt, HelperEvent::AuthPrompt { req_id: 1, kind: PamPromptKind::EchoOff, .. }), "{prompt:?}");
    c.send(&HelperRequest::AuthResponse { req_id: 1, response: PASSWORD.into() });
    match c.recv() {
        HelperEvent::AuthDone { req_id: 1, ok: true, code: None, ticket: Some(t), .. } => assert_ne!(t, 0),
        other => panic!("{other:?}"),
    }
}

#[test]
fn wrong_password_fails_and_the_secret_never_comes_back() {
    if skip_as_root() {
        return;
    }
    let env = Env::new();
    let _h = env.start();
    let c = env.connect();
    c.auth_start(&env, 1, 1, AuthIntent::Login);
    c.until(|e| matches!(e, HelperEvent::AuthPrompt { kind: PamPromptKind::EchoOff, .. }));
    c.send(&HelperRequest::AuthResponse { req_id: 1, response: "definitely-wrong-9".into() });
    let ev = c.until(done);
    match &ev {
        HelperEvent::AuthDone { ok: false, code: Some(ErrorCode::PamFailure), ticket: None, message, .. } => {
            assert!(!message.contains("definitely-wrong-9"));
        }
        other => panic!("{other:?}"),
    }
    assert!(!format!("{ev:?}").contains("definitely-wrong-9"));
}

#[test]
fn requests_that_could_not_be_legitimate_are_refused_without_a_pam_call() {
    if skip_as_root() {
        return;
    }
    let env = Env::new();
    let _h = env.start();
    let c = env.connect();
    let base = |service: &str, intent, user: &str, uid| HelperRequest::AuthStart {
        req_id: 1,
        user: user.to_owned(),
        uid,
        intent,
        generation: 1,
        service: service.to_owned(),
    };
    // A PAM service the helper was not configured for.
    c.send(&base("login", AuthIntent::Login, &env.pam.user, env.pam.uid));
    assert!(matches!(c.recv(), HelperEvent::AuthDone { ok: false, code: Some(ErrorCode::InvalidArgument), .. }));
    // The unlock stack cannot be used for a login, nor the login stack for an unlock.
    c.send(&base("bsm-test-password", AuthIntent::Login, &env.pam.user, env.pam.uid));
    assert!(matches!(c.recv(), HelperEvent::AuthDone { ok: false, code: Some(ErrorCode::InvalidArgument), .. }));
    c.send(&base("bsm-test-login", AuthIntent::Unlock, &env.pam.user, env.pam.uid));
    assert!(matches!(c.recv(), HelperEvent::AuthDone { ok: false, code: Some(ErrorCode::InvalidArgument), .. }));
    // A uid that does not belong to the named account.
    c.send(&base("bsm-test-login", AuthIntent::Login, &env.pam.user, env.pam.uid + 1));
    assert!(matches!(c.recv(), HelperEvent::AuthDone { ok: false, code: Some(ErrorCode::InvalidAccount), .. }));
    // An account that does not exist and root.
    c.send(&base("bsm-test-login", AuthIntent::Login, "no-such-account-bsm", 4242));
    assert!(matches!(c.recv(), HelperEvent::AuthDone { ok: false, code: Some(ErrorCode::InvalidAccount), .. }));
    c.send(&base("bsm-test-login", AuthIntent::Login, "root", 0));
    assert!(matches!(c.recv(), HelperEvent::AuthDone { ok: false, code: Some(ErrorCode::InvalidAccount), .. }));
    // Nothing above may have started a conversation.
    c.assert_silent(Duration::from_millis(300));
}

#[test]
fn responses_for_stale_requests_are_ignored_and_abort_ends_the_conversation() {
    if skip_as_root() {
        return;
    }
    let env = Env::new();
    let _h = env.start();
    let c = env.connect();
    // No conversation at all: a response goes nowhere.
    c.send(&HelperRequest::AuthResponse { req_id: 9, response: PASSWORD.into() });
    c.assert_silent(Duration::from_millis(200));

    c.auth_start(&env, 1, 1, AuthIntent::Login);
    c.until(|e| matches!(e, HelperEvent::AuthPrompt { kind: PamPromptKind::EchoOff, .. }));
    // A correct password under the wrong request id must not authenticate.
    c.send(&HelperRequest::AuthResponse { req_id: 2, response: PASSWORD.into() });
    c.assert_silent(Duration::from_millis(300));
    c.send(&HelperRequest::AuthAbort { req_id: 1 });
    assert!(matches!(c.recv(), HelperEvent::AuthDone { req_id: 1, ok: false, code: Some(ErrorCode::Cancelled), ticket: None, .. }));
    // A late answer to the aborted request is dropped.
    c.send(&HelperRequest::AuthResponse { req_id: 1, response: PASSWORD.into() });
    c.assert_silent(Duration::from_millis(300));
    // And the next conversation works.
    let ticket = c.login(&env, 2, 1, AuthIntent::Login);
    assert_ne!(ticket, 0);
}

#[test]
fn starting_another_conversation_supersedes_the_running_one() {
    if skip_as_root() {
        return;
    }
    let env = Env::new();
    let _h = env.start();
    let c = env.connect();
    c.auth_start(&env, 1, 1, AuthIntent::Login);
    c.until(|e| matches!(e, HelperEvent::AuthPrompt { req_id: 1, kind: PamPromptKind::EchoOff, .. }));
    c.auth_start(&env, 2, 1, AuthIntent::Unlock);
    let first = c.until(done);
    assert!(matches!(first, HelperEvent::AuthDone { req_id: 1, ok: false, code: Some(ErrorCode::Cancelled), .. }), "{first:?}");
    c.until(|e| matches!(e, HelperEvent::AuthPrompt { req_id: 2, kind: PamPromptKind::EchoOff, .. }));
    // The old request id can no longer answer anything.
    c.send(&HelperRequest::AuthResponse { req_id: 1, response: PASSWORD.into() });
    c.assert_silent(Duration::from_millis(300));
    c.send(&HelperRequest::AuthResponse { req_id: 2, response: PASSWORD.into() });
    assert!(matches!(c.until(done), HelperEvent::AuthDone { req_id: 2, ok: true, .. }));
}

#[test]
fn an_unanswered_prompt_times_out() {
    if skip_as_root() {
        return;
    }
    let mut env = Env::new();
    env.extra_config = String::new();
    let cfg = env.config().replace("ticket_ttl_secs = 30", "ticket_ttl_secs = 30\nprompt_timeout_secs = 1");
    let _h = env.start_with(&cfg);
    let c = env.connect();
    c.auth_start(&env, 1, 1, AuthIntent::Login);
    c.until(|e| matches!(e, HelperEvent::AuthPrompt { kind: PamPromptKind::EchoOff, .. }));
    let ev = c.until(done);
    assert!(matches!(ev, HelperEvent::AuthDone { ok: false, code: Some(ErrorCode::Timeout), .. }), "{ev:?}");
}

#[test]
fn the_whole_conversation_has_a_deadline_even_when_nothing_prompts() {
    if skip_as_root() {
        return;
    }
    let env = Env::new();
    let cfg = env
        .config()
        .replace("ticket_ttl_secs = 30", "ticket_ttl_secs = 30\nconversation_timeout_secs = 1\nprompt_timeout_secs = 60");
    let _h = env.start_with(&cfg);
    let c = env.connect();
    c.auth_start(&env, 1, 1, AuthIntent::Login);
    c.until(|e| matches!(e, HelperEvent::AuthPrompt { kind: PamPromptKind::EchoOff, .. }));
    let ev = c.until(done);
    assert!(matches!(ev, HelperEvent::AuthDone { ok: false, code: Some(ErrorCode::Timeout), .. }), "{ev:?}");
}

#[test]
fn tickets_are_single_use_and_bound_to_generation_and_intent() {
    if skip_as_root() {
        return;
    }
    let env = Env::new();
    let mut h = env.start();
    let c = env.connect();

    // Unknown ticket.
    c.open_session(&env, 1, 0xdead_beef, "s0000aaaa", 5);
    assert!(matches!(c.recv(), HelperEvent::SessionFailed { req_id: 1, code: ErrorCode::Unauthorised, .. }));

    // Wrong generation: refused and the ticket is burnt.
    let t = c.login(&env, 2, 5, AuthIntent::Login);
    c.open_session(&env, 3, t, "s0000aaaa", 6);
    assert!(matches!(c.recv(), HelperEvent::SessionFailed { req_id: 3, code: ErrorCode::StaleGeneration, .. }));
    c.open_session(&env, 4, t, "s0000aaaa", 5);
    assert!(matches!(c.recv(), HelperEvent::SessionFailed { req_id: 4, code: ErrorCode::Unauthorised, .. }));

    // An unlock ticket cannot open a session.
    let t = c.login(&env, 5, 7, AuthIntent::Unlock);
    c.open_session(&env, 6, t, "s0000bbbb", 7);
    assert!(matches!(c.recv(), HelperEvent::SessionFailed { req_id: 6, code: ErrorCode::Unauthorised, .. }));

    // A good ticket works once and only once.
    let t = c.login(&env, 7, 8, AuthIntent::Login);
    c.open_session(&env, 8, t, "s0000cccc", 8);
    match c.until(|e| matches!(e, HelperEvent::SessionOpened { .. } | HelperEvent::SessionFailed { .. })) {
        HelperEvent::SessionOpened { leader_pid, .. } => h.guard_session(leader_pid),
        other => panic!("{other:?}"),
    }
    c.open_session(&env, 9, t, "s0000dddd", 8);
    assert!(matches!(c.recv(), HelperEvent::SessionFailed { req_id: 9, code: ErrorCode::Unauthorised, .. }));
    c.send(&HelperRequest::CloseSession { session: "s0000cccc".into(), generation: 8, grace_ms: 300 });
    c.until(|e| matches!(e, HelperEvent::SessionExited { .. }));
}

#[test]
fn an_expired_ticket_is_refused() {
    if skip_as_root() {
        return;
    }
    let env = Env::new();
    let cfg = env.config().replace("ticket_ttl_secs = 30", "ticket_ttl_secs = 1");
    let _h = env.start_with(&cfg);
    let c = env.connect();
    let t = c.login(&env, 1, 3, AuthIntent::Login);
    std::thread::sleep(Duration::from_millis(1300));
    c.open_session(&env, 2, t, "s0000eeee", 3);
    assert!(matches!(c.recv(), HelperEvent::SessionFailed { req_id: 2, code: ErrorCode::Unauthorised, .. }));
}

#[test]
fn a_dropped_connection_discards_tickets() {
    if skip_as_root() {
        return;
    }
    let env = Env::new();
    let _h = env.start();
    let c = env.connect();
    let t = c.login(&env, 1, 3, AuthIntent::Login);
    drop(c);
    let c2 = env.connect();
    c2.open_session(&env, 2, t, "s0000ffff", 3);
    assert!(matches!(c2.recv(), HelperEvent::SessionFailed { req_id: 2, code: ErrorCode::Unauthorised, .. }));
}

#[test]
fn authentication_children_die_with_the_helper_and_with_the_connection() {
    if skip_as_root() {
        return;
    }
    let env = Env::new();
    let mut h = env.start();
    let c = env.connect();
    c.auth_start(&env, 1, 1, AuthIntent::Login);
    c.until(|e| matches!(e, HelperEvent::AuthPrompt { kind: PamPromptKind::EchoOff, .. }));
    let kids = h.children();
    assert_eq!(kids.len(), 1, "exactly one authentication child: {kids:?}");

    // Connection loss: the child goes.
    drop(c);
    assert!(wait_dead(kids[0], Duration::from_secs(5)), "child survived a dropped connection");

    // Helper crash: the child goes.
    let c = env.connect();
    c.auth_start(&env, 2, 1, AuthIntent::Login);
    c.until(|e| matches!(e, HelperEvent::AuthPrompt { kind: PamPromptKind::EchoOff, .. }));
    let kids = h.children();
    assert_eq!(kids.len(), 1, "{kids:?}");
    h.kill9();
    assert!(wait_dead(kids[0], Duration::from_secs(5)), "child survived the helper's death");
}

#[test]
fn rapid_restarts_do_not_leak_children_or_cross_conversations() {
    if skip_as_root() {
        return;
    }
    let env = Env::new();
    let h = env.start();
    let c = env.connect();
    for req in 1..=6u64 {
        c.auth_start(&env, req, req, AuthIntent::Login);
        if req % 2 == 0 && req != 6 {
            c.send(&HelperRequest::AuthAbort { req_id: req });
        }
    }
    // Only the last request survives; answer it.
    c.until(|e| matches!(e, HelperEvent::AuthPrompt { req_id: 6, kind: PamPromptKind::EchoOff, .. }));
    c.send(&HelperRequest::AuthResponse { req_id: 6, response: PASSWORD.into() });
    loop {
        match c.recv() {
            HelperEvent::AuthDone { req_id: 6, ok: true, .. } => {
                break;
            }
            HelperEvent::AuthDone { req_id, ok: true, .. } => panic!("superseded request {req_id} authenticated"),
            _ => {}
        }
    }
    std::thread::sleep(Duration::from_millis(1500));
    // Exactly one child is left: the authenticated login of request 20, parked
    // until its ticket is redeemed. Nothing else leaked.
    assert_eq!(h.children().len(), 1, "children: {:?}", h.children());
    drop(c);
    let kid = h.children();
    assert!(kid.iter().all(|p| wait_dead(*p, Duration::from_secs(5))), "parked child outlived the connection");
}

#[test]
fn an_authenticated_login_is_parked_until_its_ticket_ends_and_then_goes() {
    if skip_as_root() {
        return;
    }
    let env = Env::new();
    let cfg = env.config().replace("ticket_ttl_secs = 30", "ticket_ttl_secs = 1");
    let h = env.start_with(&cfg);
    let c = env.connect();
    let _t = c.login(&env, 1, 1, AuthIntent::Login);
    let parked = h.children();
    assert_eq!(parked.len(), 1, "the authenticated child must wait for its ticket: {parked:?}");
    assert!(alive(parked[0]));
    // The ticket expires after a second: the child, and the secret in its PAM handle, go with it.
    assert!(wait_dead(parked[0], Duration::from_secs(5)), "parked child outlived its ticket");
}

#[test]
fn an_unlock_authentication_leaves_nothing_behind() {
    if skip_as_root() {
        return;
    }
    let env = Env::new();
    let h = env.start();
    let c = env.connect();
    let t = c.login(&env, 1, 1, AuthIntent::Unlock);
    assert_ne!(t, 0);
    std::thread::sleep(Duration::from_millis(1500));
    assert!(h.children().is_empty(), "unlock children must exit: {:?}", h.children());
}

#[test]
fn a_switch_ticket_can_open_a_session_on_the_same_pam_handle() {
    if skip_as_root() {
        return;
    }
    let env = Env::new();
    let mut h = env.start();
    let c = env.connect();
    let t = c.login(&env, 1, 9, AuthIntent::Switch);
    let parked = h.children();
    assert_eq!(parked.len(), 1);
    c.open_session(&env, 2, t, "s0000b001", 9);
    match c.until(|e| matches!(e, HelperEvent::SessionOpened { .. } | HelperEvent::SessionFailed { .. })) {
        HelperEvent::SessionOpened { leader_pid, .. } => {
            h.guard_session(leader_pid);
            // The process that authenticated is the one that supervises the session:
            // one PAM handle from authenticate to close_session.
            assert_eq!(parent_of(leader_pid), Some(parked[0]));
        }
        other => panic!("{other:?}"),
    }
    c.send(&HelperRequest::CloseSession { session: "s0000b001".into(), generation: 9, grace_ms: 300 });
    c.until(|e| matches!(e, HelperEvent::SessionExited { .. }));
}

#[test]
fn an_unlock_ticket_cannot_open_a_session() {
    if skip_as_root() {
        return;
    }
    let env = Env::new();
    let _h = env.start();
    let c = env.connect();
    let t = c.login(&env, 1, 3, AuthIntent::Unlock);
    c.open_session(&env, 2, t, "s0000b002", 3);
    assert!(matches!(c.recv(), HelperEvent::SessionFailed { code: ErrorCode::Unauthorised, .. }));
    assert!(env.pam.session_log_lines().is_empty());
}

#[test]
fn locked_hint_without_a_logind_session_reports_a_clear_error() {
    if skip_as_root() {
        return;
    }
    let env = Env::new();
    let mut h = env.start();
    let c = env.connect();
    c.send(&HelperRequest::SetLockedHint { req_id: 1, session: "nope".into(), generation: 1, locked: true });
    assert!(matches!(c.recv(), HelperEvent::LockedHintResult { req_id: 1, ok: false, code: Some(ErrorCode::NoSession), .. }));

    let t = c.login(&env, 2, 4, AuthIntent::Login);
    c.open_session(&env, 3, t, "s0000abab", 4);
    match c.until(|e| matches!(e, HelperEvent::SessionOpened { .. } | HelperEvent::SessionFailed { .. })) {
        HelperEvent::SessionOpened { leader_pid, logind_session, .. } => {
            h.guard_session(leader_pid);
            assert!(logind_session.is_none(), "no pam_systemd in the test stack");
        }
        other => panic!("{other:?}"),
    }
    c.send(&HelperRequest::SetLockedHint { req_id: 4, session: "s0000abab".into(), generation: 4, locked: true });
    assert!(matches!(c.recv(), HelperEvent::LockedHintResult { req_id: 4, ok: false, code: Some(ErrorCode::NotSupported), .. }));
    c.send(&HelperRequest::CloseSession { session: "s0000abab".into(), generation: 4, grace_ms: 300 });
    c.until(|e| matches!(e, HelperEvent::SessionExited { .. }));
}

fn answer_wrong(c: &Client, env: &Env, req_id: u64, password: &str) {
    c.auth_start(env, req_id, 1, AuthIntent::Login);
    c.until(|e| matches!(e, HelperEvent::AuthPrompt { kind: PamPromptKind::EchoOff, .. }));
    c.send(&HelperRequest::AuthResponse { req_id, response: password.into() });
}

#[test]
fn superseding_an_answered_conversation_does_not_skip_the_failure_delay() {
    // Review F7: a guess whose verdict is thrown away by a new AuthStart still counts.
    if skip_as_root() {
        return;
    }
    let env = Env::new();
    let _h = env.start();
    let c = env.connect();
    // The stack takes two seconds to reject this guess, like pam_faildelay would.
    answer_wrong(&c, &env, 1, "slow-wrong");
    c.auth_start(&env, 2, 1, AuthIntent::Login);
    let first = c.until(done);
    assert!(matches!(first, HelperEvent::AuthDone { req_id: 1, ok: false, code: Some(ErrorCode::Cancelled), .. }), "{first:?}");
    match c.until(done) {
        HelperEvent::AuthDone { req_id: 2, ok: false, code: Some(ErrorCode::Busy), message, .. } => {
            assert!(message.contains("try again"), "{message}");
        }
        other => panic!("{other:?}"),
    }
    // After the delay the account is usable again.
    std::thread::sleep(Duration::from_millis(2300));
    assert_ne!(c.login(&env, 3, 1, AuthIntent::Login), 0);
}

#[test]
fn repeated_failures_back_off_per_account_and_success_resets() {
    if skip_as_root() {
        return;
    }
    let env = Env::new();
    let cfg = env.config().replace("ticket_ttl_secs = 30", "ticket_ttl_secs = 30\nfree_failures = 1");
    let _h = env.start_with(&cfg);
    let c = env.connect();
    // The first failure is free.
    answer_wrong(&c, &env, 1, "wrong-1");
    assert!(matches!(c.until(done), HelperEvent::AuthDone { req_id: 1, ok: false, code: Some(ErrorCode::PamFailure), .. }));
    answer_wrong(&c, &env, 2, "wrong-2");
    assert!(matches!(c.until(done), HelperEvent::AuthDone { req_id: 2, ok: false, .. }));
    // The second one starts the helper's own back-off.
    c.auth_start(&env, 3, 1, AuthIntent::Login);
    match c.until(done) {
        HelperEvent::AuthDone { req_id: 3, ok: false, code: Some(ErrorCode::Busy), .. } => {}
        other => panic!("{other:?}"),
    }
    std::thread::sleep(Duration::from_millis(2300));
    assert_ne!(c.login(&env, 4, 1, AuthIntent::Login), 0);
    // Success cleared the record: a new failure is free again.
    answer_wrong(&c, &env, 5, "wrong-3");
    assert!(matches!(c.until(done), HelperEvent::AuthDone { req_id: 5, ok: false, .. }));
    assert_ne!(c.login(&env, 6, 1, AuthIntent::Login), 0);
}

#[test]
fn too_many_children_still_ending_are_refused_with_busy() {
    // Review F7: the number of superseded root children is bounded.
    if skip_as_root() {
        return;
    }
    let env = Env::new();
    let _h = env.start();
    let c = env.connect();
    for req in 1..=40u64 {
        c.auth_start(&env, req, 1, AuthIntent::Login);
    }
    let mut busy = false;
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        match c.try_recv(Duration::from_millis(300)) {
            Some(HelperEvent::AuthDone { code: Some(ErrorCode::Busy), .. }) => {
                busy = true;
                break;
            }
            Some(_) => {}
            None => break,
        }
    }
    assert!(busy, "a flood of superseding AuthStarts must hit the bound");
}

#[test]
fn only_the_compositor_process_is_served_not_every_process_of_its_uid() {
    // Review F10: a squatter with the bsm uid (for example the image converter) must
    // neither hold the single connection slot nor issue requests.
    if skip_as_root() {
        return;
    }
    let me = std::env::current_exe().unwrap().display().to_string();
    let own_unit = std::fs::read_to_string("/proc/self/cgroup")
        .ok()
        .and_then(|t| bsm_helper::cgroup::parse_unified_path(&t))
        .and_then(|p| p.rsplit('/').next().map(str::to_owned))
        .unwrap_or_default();

    // Right executable (and unit when it has a name): served.
    let mut env = Env::new();
    env.peer_exe = me.clone();
    env.peer_cgroup = own_unit.clone();
    let _h = env.start();
    let c = env.connect();
    c.send(&HelperRequest::Ping { nonce: 1 });
    assert_eq!(c.recv(), HelperEvent::Pong { nonce: 1 });
    drop(c);

    // Same uid, wrong executable: closed without a byte read, and the slot stays free.
    let mut env2 = Env::new();
    env2.peer_exe = "/usr/bin/bsm".into();
    let _h2 = env2.start();
    let squatter = env2.connect_raw();
    let _ = squatter.send_raw(&serde_json::to_vec(&HelperRequest::Ping { nonce: 2 }).unwrap());
    assert!(squatter.closed_within(Duration::from_secs(3)), "a process that is not bsm must be refused");

    // Right executable, wrong unit.
    let mut env3 = Env::new();
    env3.peer_exe = me;
    env3.peer_cgroup = "bsm.service".into();
    if own_unit != "bsm.service" {
        let _h3 = env3.start();
        let wrong_unit = env3.connect_raw();
        let _ = wrong_unit.send_raw(&serde_json::to_vec(&HelperRequest::Ping { nonce: 3 }).unwrap());
        assert!(wrong_unit.closed_within(Duration::from_secs(3)), "a process outside bsm.service must be refused");
    }
}

fn record_config(env: &Env, extra: &str) -> String {
    env.config().replace("ticket_ttl_secs = 30", &format!("ticket_ttl_secs = 30\n{extra}"))
}

fn wait_for_records(env: &Env, at_least: usize, within: Duration) -> usize {
    let end = std::time::Instant::now() + within;
    while std::time::Instant::now() < end {
        if env.pam.failure_records() >= at_least {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    env.pam.failure_records()
}

#[test]
fn cancelled_and_timed_out_conversations_leave_no_failed_login_record() {
    // The lockout module (pam_faillock) records a failure when the stack fails. An abort,
    // a switch of account or an idle prompt is not a failed login and must not count.
    if skip_as_root() {
        return;
    }
    let mut env = Env::new();
    env.login_service = "bsm-test-record".into();
    let _h = env.start_with(&record_config(&env, "prompt_timeout_secs = 1"));
    let c = env.connect();
    // Positive control: a wrong password IS recorded.
    answer_wrong(&c, &env, 1, "wrong-password");
    assert!(matches!(c.until(done), HelperEvent::AuthDone { req_id: 1, ok: false, .. }));
    assert_eq!(wait_for_records(&env, 1, Duration::from_secs(5)), 1, "a wrong password must be recorded");

    // AuthAbort at the prompt.
    c.auth_start(&env, 2, 1, AuthIntent::Login);
    c.until(|e| matches!(e, HelperEvent::AuthPrompt { kind: PamPromptKind::EchoOff, .. }));
    c.send(&HelperRequest::AuthAbort { req_id: 2 });
    assert!(matches!(c.until(done), HelperEvent::AuthDone { req_id: 2, code: Some(ErrorCode::Cancelled), .. }));
    // Switching account: a new AuthStart supersedes the unanswered conversation.
    c.auth_start(&env, 3, 1, AuthIntent::Login);
    c.until(|e| matches!(e, HelperEvent::AuthPrompt { req_id: 3, kind: PamPromptKind::EchoOff, .. }));
    c.auth_start(&env, 4, 1, AuthIntent::Login);
    assert!(matches!(c.until(done), HelperEvent::AuthDone { req_id: 3, code: Some(ErrorCode::Cancelled), .. }));
    // An unanswered prompt times out (1 s here).
    c.until(|e| matches!(e, HelperEvent::AuthPrompt { req_id: 4, kind: PamPromptKind::EchoOff, .. }));
    assert!(matches!(c.until(done), HelperEvent::AuthDone { req_id: 4, code: Some(ErrorCode::Timeout), .. }));
    // bsm going away mid-conversation.
    c.auth_start(&env, 5, 1, AuthIntent::Login);
    c.until(|e| matches!(e, HelperEvent::AuthPrompt { req_id: 5, kind: PamPromptKind::EchoOff, .. }));
    drop(c);
    // Give every child time to finish; none may have added a record.
    std::thread::sleep(Duration::from_millis(1500));
    assert_eq!(env.pam.failure_records(), 1, "only the wrong password may be recorded");
}
