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
    browser.answer_dialog(key, true, None);
    let Some(Prompt::Dialog(_, second)) = browser.prompt(key) else {
        panic!("the second dialog");
    };
    assert_eq!(second.message, "Second");
    browser.answer_dialog(key, false, Some("typed".to_string()));
    assert!(browser.prompt(key).is_none());
    browser.answer_dialog(key, true, None);
    let answers = double
        .calls()
        .iter()
        .filter(|call| matches!(call, Call::Respond(..)))
        .count();
    assert_eq!(answers, 2, "a third answer has no question");
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
