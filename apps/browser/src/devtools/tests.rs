//! The toolbox of a tab: opened and closed by one key, docked in the tab that
//! owns it, and gone when its target goes.
//!
//! Failures these guard against:
//!   1. F12 opening the toolbox as a tab of the strip, or in front of the page
//!      instead of under it in the same tab;
//!   2. F12 a second time opening another toolbox (the engine answers Busy)
//!      instead of closing the one that is open;
//!   3. F12 on a blank tab or Settings asking the engine for a toolbox;
//!   4. a toolbox that outlives its target in the model, so a closed page
//!      keeps a dead view docked;
//!   5. the splitter's position lost at the next toolbox;
//!   6. a toolbox that keeps drawing while its tab is in the background.

use bingux_ui::web_view::test_double::Call;
use bingux_ui::web_view::{BrowserEvent, TabEvent};

use crate::browser::testing::{browser, go};
use crate::commands::Command;
use crate::tabs::Place;

#[test]
fn f12_docks_a_toolbox_under_the_page_in_the_same_tab() {
    let (mut browser, double) = browser();
    let page = go(&mut browser, "https://example.com/");
    let tabs_before = browser.tabs.all().len();
    double.clear_calls();
    browser.run(Command::DevTools);
    assert!(
        double
            .calls()
            .iter()
            .any(|call| matches!(call, Call::OpenDevtools(_)))
    );
    assert_eq!(
        browser.tabs.all().len(),
        tabs_before,
        "a toolbox is not a tab"
    );
    assert_eq!(browser.tabs.active_key(), page, "the page stays in front");
    let docked = browser.tabs.get(page).and_then(|tab| tab.toolbox);
    assert!(docked.is_some(), "the toolbox belongs to the page's tab");
}

#[test]
fn f12_again_closes_the_toolbox_and_never_opens_a_second() {
    let (mut browser, double) = browser();
    let page = go(&mut browser, "https://example.com/");
    browser.run(Command::DevTools);
    let handle = browser
        .tabs
        .get(page)
        .and_then(|tab| tab.toolbox)
        .unwrap()
        .handle;
    double.clear_calls();
    browser.run(Command::DevTools);
    assert!(
        browser
            .tabs
            .get(page)
            .is_some_and(|tab| tab.toolbox.is_none()),
        "the second press toggles it off"
    );
    assert!(double.calls().contains(&Call::CloseDevtools(handle)));
    assert!(
        !double
            .calls()
            .iter()
            .any(|call| matches!(call, Call::OpenDevtools(_))),
        "nothing was opened"
    );
    browser.poll();
    assert!(
        browser
            .tabs
            .get(page)
            .is_some_and(|tab| tab.toolbox.is_none()),
        "the session's own end changes nothing"
    );
}

#[test]
fn a_tab_with_no_web_page_has_no_toolbox_to_open() {
    let (mut browser, double) = browser();
    browser.run(Command::DevTools);
    browser.run(Command::Settings);
    browser.run(Command::DevTools);
    assert!(
        !double
            .calls()
            .iter()
            .any(|call| matches!(call, Call::OpenDevtools(_))),
        "a blank tab and Settings have nothing to inspect"
    );
    assert!(
        browser.take_notices().is_empty(),
        "and nothing to say about it"
    );
}

#[test]
fn the_engine_ending_a_toolbox_drops_it_from_its_tab() {
    let (mut browser, double) = browser();
    let page = go(&mut browser, "https://example.com/");
    browser.run(Command::DevTools);
    let handle = browser
        .tabs
        .get(page)
        .and_then(|tab| tab.toolbox)
        .unwrap()
        .handle;
    // The person closed the toolbox from inside it, or the engine ended it.
    double.emit(BrowserEvent::tab(
        handle.session(),
        TabEvent::Closed { error: None },
    ));
    browser.poll();
    assert!(
        browser
            .tabs
            .get(page)
            .is_some_and(|tab| tab.toolbox.is_none())
    );
    assert_eq!(browser.tabs.active_key(), page, "the page is untouched");
}

#[test]
fn closing_the_tab_takes_its_toolbox_with_it() {
    let (mut browser, double) = browser();
    browser.open_tab("https://other.example/", Place::End, false);
    browser.poll();
    let page = go(&mut browser, "https://example.com/");
    browser.run(Command::DevTools);
    let handle = browser
        .tabs
        .get(page)
        .and_then(|tab| tab.toolbox)
        .unwrap()
        .handle;
    double.clear_calls();
    browser.close_tab(page);
    browser.poll();
    assert!(double.calls().contains(&Call::CloseDevtools(handle)));
    assert!(browser.tabs.get(page).is_none());
    assert!(
        browser.toolbox_owner(handle.session()).is_none(),
        "no tab owns a dead toolbox"
    );
}

#[test]
fn the_splitter_position_is_what_the_next_toolbox_opens_at() {
    let (mut browser, _double) = browser();
    let first = go(&mut browser, "https://example.com/");
    browser.run(Command::DevTools);
    assert_eq!(
        browser
            .tabs
            .get(first)
            .and_then(|tab| tab.toolbox)
            .map(|d| d.page_share),
        Some(super::DEFAULT_PAGE_SHARE)
    );
    browser.set_page_share(first, 0.4);
    let second = browser
        .open_tab("https://two.example/", Place::End, true)
        .unwrap();
    browser.poll();
    browser.run(Command::DevTools);
    assert_eq!(
        browser
            .tabs
            .get(second)
            .and_then(|tab| tab.toolbox)
            .map(|d| d.page_share),
        Some(0.4),
        "the second toolbox opens where the first was left"
    );
}

#[test]
fn the_toolbox_is_sized_by_its_view_and_hidden_with_its_tab() {
    let (mut browser, double) = browser();
    let page = go(&mut browser, "https://example.com/");
    browser.run(Command::DevTools);
    let handle = browser
        .tabs
        .get(page)
        .and_then(|tab| tab.toolbox)
        .unwrap()
        .handle;
    double.clear_calls();
    browser.resize_toolbox(page, 900, 300, 1.0);
    assert!(
        double
            .calls()
            .contains(&Call::SetSize(handle.session(), 900, 300, 1.0)),
        "the layout's size reaches the session"
    );
    double.clear_calls();
    browser.open_tab("https://other.example/", Place::End, true);
    browser.poll();
    assert!(
        double
            .calls()
            .contains(&Call::SetVisible(handle.session(), false, false)),
        "a toolbox in a background tab makes no frames"
    );
    browser.select(page);
    assert!(
        double
            .calls()
            .contains(&Call::SetVisible(handle.session(), true, true))
    );
}
