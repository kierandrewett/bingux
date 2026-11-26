//! The questions a page asks: queued in order, answered once, and dropped when
//! nothing can hear the answer.

use bingux_ui::web_view::requests::{DialogKind, DialogRequest, RequestKind};
use bingux_ui::web_view::test_double::Call;
use bingux_ui::web_view::{GoneReason, Response, TabEvent, TabId};

use super::*;
use crate::browser::testing::{browser, go};

#[test]
fn dialogs_queue_in_order_and_each_is_answered_once() {
    let (mut browser, double) = browser();
    let key = go(&mut browser, "dialog.example.net");
    let session = browser.session_of(key).unwrap();
    double.ask(
        session,
        RequestKind::Dialog(DialogRequest {
            origin: "https://dialog.example.net".to_string(),
            kind: DialogKind::Alert,
            message: "Second".to_string(),
        }),
    );
    browser.poll();
    assert_eq!(browser.tabs.get(key).unwrap().prompts.len(), 2);
    assert!(browser.tabs.get(key).unwrap().needs_attention());
    double.clear_calls();
    let first = browser.prompt(key).unwrap().id();
    browser.answer_dialog(key, first, true, None);
    let Some(Prompt::Dialog(second_id, second)) = browser.prompt(key).cloned() else {
        panic!("the second dialog");
    };
    assert_eq!(second.message, "Second");
    browser.answer_dialog(key, second_id, false, Some("typed".to_string()));
    assert!(browser.prompt(key).is_none());
    browser.answer_dialog(key, second_id, true, None);
    let answers = double
        .calls()
        .iter()
        .filter(|call| matches!(call, Call::Respond(..)))
        .count();
    assert_eq!(answers, 2, "a third answer has no question");
}

#[test]
fn an_answer_for_a_question_the_engine_closed_answers_nothing_else() {
    let (mut browser, double) = browser();
    let key = go(&mut browser, "dialog.example.net");
    let session = browser.session_of(key).unwrap();
    let closed = browser.prompt(key).unwrap().id();
    let next = double.ask(
        session,
        RequestKind::Dialog(DialogRequest {
            origin: "https://dialog.example.net".to_string(),
            kind: DialogKind::Alert,
            message: "Next".to_string(),
        }),
    );
    browser.poll();
    // The engine ends the first question; the dialog for it is still on screen.
    double.close_request(session, closed);
    browser.poll();
    double.clear_calls();
    browser.answer_dialog(key, closed, true, None);
    assert!(
        !double
            .calls()
            .iter()
            .any(|call| matches!(call, Call::Respond(..))),
        "the stale dialog's answer is for no question"
    );
    assert_eq!(
        browser.prompt(key).map(|prompt| prompt.id()),
        Some(next),
        "the next question still waits for its own answer"
    );
}

#[test]
fn a_question_is_answered_by_its_own_kind_only() {
    let (mut browser, double) = browser();
    let key = go(&mut browser, "dialog.example.net");
    let asked = browser.prompt(key).unwrap().id();
    double.clear_calls();
    browser.answer_auth(key, asked, None);
    assert!(
        double.calls().is_empty(),
        "a sign-in answer does not take a dialog off the queue"
    );
    assert!(browser.prompt(key).is_some());
}

#[test]
fn a_tab_that_crashes_drops_the_questions_it_can_no_longer_hear() {
    let (mut browser, double) = browser();
    let key = go(&mut browser, "dialog.example.net");
    assert!(browser.prompt(key).is_some());
    let session = browser.session_of(key).unwrap();
    double.emit(bingux_ui::web_view::BrowserEvent::tab(
        session,
        TabEvent::ProcessGone(GoneReason::Killed),
    ));
    browser.poll();
    assert!(browser.prompt(key).is_none());
}

#[test]
fn a_request_for_a_tab_that_is_gone_is_refused() {
    let (mut browser, double) = browser();
    let stale = TabId(999);
    double.ask(
        stale,
        RequestKind::Dialog(DialogRequest {
            origin: "https://x.example".to_string(),
            kind: DialogKind::Confirm,
            message: "Hello?".to_string(),
        }),
    );
    double.clear_calls();
    browser.poll();
    // The double has no such tab, so the refusal is recorded as a failed call.
    assert!(double.calls().iter().any(|call| matches!(call, Call::Respond(t, _, Response::Dialog { accept: false, .. }) if *t == stale)));
}

#[test]
fn the_engine_ending_a_question_takes_it_off_the_screen() {
    let (mut browser, double) = browser();
    let key = go(&mut browser, "dialog.example.net");
    let session = browser.session_of(key).unwrap();
    let asked = browser.prompt(key).unwrap().id();
    double.close_request(session, asked);
    browser.poll();
    assert!(
        browser.prompt(key).is_none(),
        "its deadline passed: nothing is left to answer"
    );
    // A second question, then a certificate that was waiting.
    let other = double.ask(
        session,
        RequestKind::Dialog(DialogRequest {
            origin: "https://x.example".to_string(),
            kind: DialogKind::Alert,
            message: "Hi".to_string(),
        }),
    );
    browser.poll();
    assert_eq!(browser.prompt(key).map(|prompt| prompt.id()), Some(other));
    double.close_request(session, other);
    browser.poll();
    assert!(browser.prompt(key).is_none());
}

/// The dialogs of the questions, as the window shows them, over the test
/// double.
///
/// Failure this guards against: the engine ended a question itself (its
/// deadline passed, the page went on) and the dialog for it stayed on the
/// screen. Its buttons then answered whatever question was first in the queue,
/// which was a different one.
#[cfg(feature = "snapshot")]
mod shown {
    use crate::snapshot::{self, ctx, wait};

    #[test]
    fn a_dialog_the_engine_closed_is_taken_off_the_screen() {
        let (mut shot, rig) = snapshot::window(&["dialog.example.net"]);
        wait(&mut shot, 600);
        assert!(ctx().overlays.dialog_open(), "the page's dialog is up");
        let window = ctx();
        let (session, asked) = {
            let browser = window.browser.peek();
            let key = browser.tabs.active_key();
            (
                browser.session_of(key).expect("a session"),
                browser.prompt(key).expect("a question").id(),
            )
        };
        rig.double.close_request(session, asked);
        wait(&mut shot, 600);
        assert!(
            !ctx().overlays.dialog_open(),
            "nothing is left to answer, so nothing is left to press"
        );
    }
}
