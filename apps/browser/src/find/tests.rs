//! What a search of the page asks of the engine, and what it shows of the answer.

use bingux_ui::web_view::test_double::{Call, TestDoubleBackend};
use bingux_ui::web_view::{BrowserEvent, Capabilities, FailureCause, TabEvent};

use super::*;
use crate::browser::testing::{browser, go};
use crate::browser::{Browser, Options, UiRequest};
use crate::commands::Command;
use crate::tabs::Place;

#[test]
fn find_is_not_found_only_after_a_search_with_no_matches() {
    let mut find = Find::default();
    assert!(!find.not_found());
    find.query.text = "x".to_string();
    find.bar = Bar::Open(Progress::Pending);
    assert!(!find.not_found(), "no answer yet");
    find.on_result(0, 0);
    assert!(find.not_found());
    find.on_result(1, 4);
    assert!(!find.not_found());
}

#[test]
fn find_asks_the_engine_and_shows_the_count() {
    let (mut browser, double) = browser();
    let key = go(&mut browser, "example.com");
    browser.open_find(key);
    assert_eq!(browser.take_ui(), [UiRequest::OpenFind]);
    browser.find(key, "example", true, true);
    browser.poll();
    assert_eq!(browser.tabs.get(key).unwrap().find.result(), Some((1, 3)));
    browser.run(Command::FindNext);
    browser.poll();
    assert_eq!(browser.tabs.get(key).unwrap().find.result(), Some((2, 3)));
    browser.run(Command::FindPrevious);
    browser.poll();
    assert_eq!(browser.tabs.get(key).unwrap().find.result(), Some((1, 3)));
    browser.find(key, "ab", true, true);
    browser.poll();
    assert!(browser.tabs.get(key).unwrap().find.not_found());
    browser.close_find(key);
    assert!(!browser.tabs.get(key).unwrap().find.is_open());
    assert!(
        double
            .calls()
            .iter()
            .any(|call| matches!(call, Call::FindStop(_)))
    );
}

#[test]
fn find_says_so_when_the_engine_cannot() {
    crate::locale::register();
    let double = TestDoubleBackend::with_capabilities(Capabilities {
        find: false,
        ..Capabilities::all()
    });
    let mut browser = Browser::new(double, Options::default());
    let key = browser.open_tab("example.com", Place::End, true).unwrap();
    browser.open_find(key);
    assert!(browser.take_ui().is_empty());
    assert_eq!(browser.take_notices().len(), 1);
}

#[test]
fn what_the_engine_took_of_find_is_read_again_when_the_bar_opens() {
    crate::locale::register();
    let double = TestDoubleBackend::with_capabilities(Capabilities {
        find: false,
        ..Capabilities::all()
    });
    let mut browser = Browser::new(double.clone(), Options::default());
    let key = browser.open_tab("example.com", Place::End, true).unwrap();
    // The engine took the call at the first session, after the window read
    // what it could do.
    double.set_capabilities(Capabilities::all());
    browser.open_find(key);
    assert!(browser.engine.capabilities().find);
    assert!(
        browser.take_notices().is_empty(),
        "no word of find being absent"
    );
    assert!(
        browser
            .take_ui()
            .iter()
            .any(|ui| matches!(ui, UiRequest::OpenFind))
    );
}

#[test]
fn an_edited_query_does_not_show_the_old_count() {
    let (mut browser, _double) = browser();
    let key = go(&mut browser, "example.com");
    browser.open_find(key);
    browser.find(key, "ab", true, true);
    browser.poll();
    assert!(browser.tabs.get(key).unwrap().find.not_found());
    // The person edits the text. The engine has not counted the new query yet.
    browser.find(key, "a", true, true);
    let find = &browser.tabs.get(key).unwrap().find;
    assert!(
        !find.not_found(),
        "the old answer is not the answer to this text"
    );
    assert_eq!(find.result(), None);
}

#[test]
fn a_find_bar_closes_when_its_tab_has_no_session() {
    let (mut browser, double) = browser();
    let key = go(&mut browser, "example.com");
    browser.open_find(key);
    double.fail_engine(FailureCause::Crashed, "gone", 1, true);
    browser.poll();
    assert!(browser.tabs.get(key).unwrap().session.is_none());
    browser.close_find(key);
    assert!(
        !browser.tabs.get(key).unwrap().find.is_open(),
        "closing a bar needs no engine"
    );
}

#[test]
fn a_count_that_arrives_after_the_bar_closed_is_ignored() {
    let (mut browser, double) = browser();
    let key = go(&mut browser, "example.com");
    browser.open_find(key);
    browser.find(key, "example", true, true);
    browser.poll();
    let session = browser.session_of(key).unwrap();
    browser.close_find(key);
    double.emit(BrowserEvent::tab(
        session,
        TabEvent::FindResult {
            current: 2,
            total: 3,
        },
    ));
    browser.poll();
    assert_eq!(
        browser.tabs.get(key).unwrap().find.result(),
        None,
        "a count of a bar that is not there"
    );
}
