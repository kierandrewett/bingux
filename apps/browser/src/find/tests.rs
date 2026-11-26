//! What a search of the page asks of the engine, and what it shows of the answer.

use bingux_ui::web_view::Capabilities;
use bingux_ui::web_view::test_double::{Call, TestDoubleBackend};

use super::*;
use crate::browser::testing::{browser, go};
use crate::browser::{Browser, Options, UiRequest};
use crate::commands::Command;
use crate::tabs::Place;

#[test]
fn find_is_not_found_only_after_a_search_with_no_matches() {
    let mut find = Find::default();
    assert!(!find.not_found());
    find.text = "x".to_string();
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
