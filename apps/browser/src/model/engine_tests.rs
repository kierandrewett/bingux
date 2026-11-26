//! The engine's life as the window sees it: it fails and is started again,
//! a profile that is not safe stops it from starting, pages ask for windows,
//! and the controls follow what the engine says it can do. Over the in-memory
//! test double.

use std::sync::Arc;

use bingux_ui::web_view::requests::{DialogKind, DialogRequest, RequestKind};
use bingux_ui::web_view::test_double::{Call, TestDoubleBackend};
use bingux_ui::web_view::{
    BrowserBackend, BrowserEvent, Capabilities, Disposition, EngineEvent, FailureCause, NewWindow,
};

use super::shortcuts::Command;
use super::tabs::Place;
use crate::browser::{Browser, Options};
use crate::engine;

fn browser() -> (Browser, Arc<TestDoubleBackend>) {
    let double = TestDoubleBackend::new();
    let mut browser = Browser::new(double.clone(), Options::default());
    browser.open_tab("", Place::End, true);
    browser.poll();
    double.clear_calls();
    (browser, double)
}

fn go(browser: &mut Browser, url: &str) -> u64 {
    let key = browser.tabs.active_key();
    browser.navigate(key, url);
    browser.poll();
    key
}

fn failed(browser: &Browser) -> Option<(FailureCause, bool, u32)> {
    match &browser.engine {
        engine::Status::Failed {
            cause,
            restartable,
            code,
            ..
        } => Some((*cause, *restartable, *code)),
        _ => None,
    }
}

#[test]
fn an_engine_failure_is_a_state_with_its_cause_and_not_a_panic() {
    let (mut browser, double) = browser();
    double.fail_engine(
        FailureCause::Crashed,
        "the engine process was killed",
        137,
        true,
    );
    browser.poll();
    assert_eq!(failed(&browser), Some((FailureCause::Crashed, true, 137)));
    let engine::Status::Failed { message, .. } = &browser.engine else {
        panic!("failed")
    };
    assert_eq!(message, "the engine process was killed");
}

#[test]
fn the_tabs_stay_when_the_engine_is_gone_so_the_window_does_not_close_with_it() {
    let (mut browser, double) = browser();
    let key = go(&mut browser, "example.com");
    browser.open_tab("news.bingux.dev", Place::End, false);
    browser.poll();
    double.fail_engine(FailureCause::Crashed, "gone", 1, true);
    browser.poll();
    assert_eq!(browser.tabs.len(), 2, "both pages are still there");
    let tab = browser.tabs.get(key).unwrap();
    assert_eq!(tab.session, None, "the session is gone with the engine");
    assert!(!tab.load.loading);
    assert_eq!(
        tab.url, "https://example.com",
        "and the address is what a restart opens again"
    );
}

#[test]
fn questions_waiting_on_an_engine_that_died_are_dropped() {
    let (mut browser, double) = browser();
    let key = go(&mut browser, "dialog.example.net");
    assert!(browser.prompt(key).is_some());
    double.fail_engine(FailureCause::Crashed, "gone", 1, true);
    browser.poll();
    assert!(browser.prompt(key).is_none());
}

#[test]
fn a_restart_opens_every_tab_again_where_it_was_and_the_engine_is_ready_again() {
    let (mut browser, double) = browser();
    let first = go(&mut browser, "example.com");
    browser.navigate(first, "news.bingux.dev");
    browser.poll();
    let second = browser
        .open_tab("video.example.net", Place::End, false)
        .unwrap();
    browser.poll();
    let before: Vec<_> = browser
        .tabs
        .all()
        .iter()
        .map(|tab| (tab.key, tab.session))
        .collect();
    double.fail_engine(FailureCause::Crashed, "gone", 137, true);
    browser.poll();
    double.clear_calls();

    browser.restart_engine();
    assert_eq!(browser.engine, engine::Status::Starting);
    let calls = double.calls();
    assert!(matches!(calls[0], Call::Restart), "the engine first");
    let restored: Vec<_> = calls
        .iter()
        .filter_map(|call| match call {
            Call::CreateTab(config) => Some(config.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(restored.len(), 2, "every web tab, in order");
    assert!(
        restored.iter().all(|config| config.restore_from.is_some()),
        "each takes up what its old session had saved"
    );
    let old: Vec<_> = before.iter().filter_map(|(_, session)| *session).collect();
    assert_eq!(
        restored
            .iter()
            .filter_map(|config| config.restore_from)
            .collect::<Vec<_>>(),
        old
    );
    assert_eq!(
        restored.iter().filter(|config| config.hidden).count(),
        1,
        "only the tab on show is made visible"
    );

    browser.poll();
    assert_eq!(browser.engine, engine::Status::Ready);
    for (key, was) in before {
        let tab = browser.tabs.get(key).unwrap();
        assert!(
            tab.session.is_some() && tab.session != was,
            "tab {key} has a session of the new engine"
        );
    }
    let tab = browser.tabs.get(first).unwrap();
    assert_eq!(tab.url, "https://news.bingux.dev", "the page it was on");
    assert_eq!(tab.history.len(), 2, "and the history behind it");
    assert!(browser.tabs.get(second).unwrap().session.is_some());
}

#[test]
fn a_restart_is_offered_only_when_the_engine_says_it_is_safe() {
    let (mut browser, double) = browser();
    double.fail_engine(
        FailureCause::SandboxFloor,
        "the sandbox level was not reached",
        1,
        false,
    );
    browser.poll();
    double.clear_calls();
    browser.restart_engine();
    assert!(
        double.calls().is_empty(),
        "no restart was asked of the engine"
    );
    assert!(failed(&browser).is_some());
    // Nor while it runs.
    let (mut running, running_double) = self::browser();
    running.restart_engine();
    assert!(running_double.calls().is_empty());
}

#[test]
fn an_engine_that_does_not_start_is_a_window_that_says_why_and_tries_again_when_told() {
    let double = TestDoubleBackend::failed_start(
        FailureCause::ProfileUnsafe,
        "profile_dir: /p is writable by group or others.",
    );
    let mut browser = Browser::new(double.clone(), Options::default());
    browser.poll();
    assert_eq!(
        failed(&browser),
        Some((FailureCause::ProfileUnsafe, true, 0))
    );
    // The window opens on the page, with no tab asked of an engine that is not there.
    browser.open_first(&["example.com".to_string()]);
    assert!(browser.tabs.is_empty());
    assert!(
        browser.take_notices().is_empty(),
        "the page says why; no toast says it again"
    );
    // The person fixed the folder.
    double.fix_start();
    browser.restart_engine();
    browser.poll();
    assert_eq!(browser.engine, engine::Status::Ready);
    assert_eq!(
        browser.tabs.len(),
        1,
        "what the window was to start on opens now"
    );
    assert_eq!(
        browser.active().map(|tab| tab.url.as_str()),
        Some("https://example.com")
    );
}

#[test]
fn a_restart_that_does_not_work_keeps_saying_why_with_the_new_words() {
    let double = TestDoubleBackend::failed_start(
        FailureCause::ProfileUnsafe,
        "profile_dir: /p is writable by group or others.",
    );
    let mut browser = Browser::new(double.clone(), Options::default());
    browser.poll();
    browser.restart_engine();
    browser.poll();
    assert_eq!(
        failed(&browser).map(|(cause, ..)| cause),
        Some(FailureCause::ProfileUnsafe)
    );
    let notices = browser.take_notices();
    assert!(
        notices.iter().any(|notice| notice.error),
        "the person is told it did not work"
    );
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

fn popup(double: &TestDoubleBackend, browser: &mut Browser, window: NewWindow) -> u64 {
    let key = go(browser, "example.com");
    let session = browser.session_of(key).unwrap();
    double.open_window_with(session, "https://news.bingux.dev/popup", window);
    browser.poll();
    key
}

#[test]
fn a_window_a_page_opens_as_it_loads_with_no_action_of_the_person_is_blocked() {
    let (mut browser, double) = browser();
    popup(
        &double,
        &mut browser,
        NewWindow {
            unrequested: true,
            origin: "https://ads.example".to_string(),
            ..NewWindow::default()
        },
    );
    assert_eq!(browser.tabs.len(), 1, "no tab for it");
    let calls = double.calls();
    assert!(!calls.iter().any(|call| matches!(call, Call::AcceptTab(_))));
    assert!(
        calls.iter().any(|call| matches!(call, Call::CloseTab(_))),
        "the engine is told it was refused"
    );
    let notices = browser.take_notices();
    assert!(
        notices
            .iter()
            .any(|notice| notice.text == "Blocked a pop-up from https://ads.example"),
        "{notices:?}"
    );
}

#[test]
fn a_window_the_person_asked_for_is_never_blocked_even_when_it_came_from_a_script() {
    let (mut browser, double) = browser();
    popup(
        &double,
        &mut browser,
        NewWindow {
            unrequested: true,
            user_gesture: true,
            ..NewWindow::default()
        },
    );
    assert_eq!(browser.tabs.len(), 2, "the click said so");
}

#[test]
fn a_window_that_says_nothing_of_who_asked_is_opened_because_not_knowing_is_not_a_verdict() {
    // What the engine sends today (CR-8): no gesture, no origin, the disposition "window".
    let (mut browser, double) = browser();
    popup(
        &double,
        &mut browser,
        NewWindow {
            disposition: Disposition::Window,
            ..NewWindow::default()
        },
    );
    assert_eq!(browser.tabs.len(), 2);
    assert!(
        double
            .calls()
            .iter()
            .any(|call| matches!(call, Call::AcceptTab(_)))
    );
    assert_eq!(
        browser.tabs.active().unwrap().url,
        "https://news.bingux.dev/popup",
        "in front, as a window is"
    );
}

#[test]
fn a_window_asked_for_in_the_background_opens_behind_the_page() {
    let (mut browser, double) = browser();
    let opener = popup(
        &double,
        &mut browser,
        NewWindow {
            disposition: Disposition::BackgroundTab,
            ..NewWindow::default()
        },
    );
    assert_eq!(browser.tabs.len(), 2);
    assert_eq!(browser.tabs.active_key(), opener, "the page stays on show");
    assert!(
        double
            .calls()
            .iter()
            .any(|call| matches!(call, Call::AcceptTab(_)))
    );
}

#[test]
fn what_the_engine_cannot_do_is_disabled_and_the_command_says_so() {
    let double = TestDoubleBackend::with_capabilities(Capabilities {
        favicons: true,
        ..Capabilities::default()
    });
    let mut browser = Browser::new(double.clone(), Options::default());
    browser.open_tab("example.com", Place::End, true);
    browser.poll();
    assert!(
        !browser.capabilities().find
            && !browser.capabilities().downloads
            && !browser.capabilities().context_menus
    );
    browser.run(Command::Find);
    let notices = browser.take_notices();
    assert_eq!(notices.len(), 1, "find in page says it is not there");
    assert!(
        notices[0].text.contains("Find in page is not available"),
        "{notices:?}"
    );
    assert!(browser.capabilities().favicons, "and what it can do is on");
}

#[test]
fn the_controls_follow_the_engine_after_a_restart() {
    let (mut browser, double) = browser();
    assert!(browser.capabilities().find);
    double.fail_engine(FailureCause::Crashed, "gone", 1, true);
    browser.poll();
    browser.restart_engine();
    browser.poll();
    assert_eq!(
        browser.capabilities(),
        double.capabilities(),
        "read again from the new engine"
    );
    assert_eq!(browser.engine, engine::Status::Ready);
}

#[test]
fn the_latest_words_of_a_failure_are_the_ones_the_window_keeps() {
    // The backend says the failure again, with new words, when a restart does not work.
    let (mut browser, double) = browser();
    double.fail_engine(FailureCause::Crashed, "first", 1, true);
    double.emit(BrowserEvent::Engine(EngineEvent::Failed {
        message: "second".to_string(),
        code: 2,
        cause: FailureCause::StartFailed,
        restartable: true,
    }));
    browser.poll();
    let engine::Status::Failed {
        message,
        code,
        cause,
        ..
    } = &browser.engine
    else {
        panic!("failed")
    };
    assert_eq!(
        (message.as_str(), *code, *cause),
        ("second", 2, FailureCause::StartFailed)
    );
    assert_eq!(browser.tabs.len(), 1, "the tab was kept once");
}
