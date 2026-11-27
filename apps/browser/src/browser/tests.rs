//! The browser's rules, over the in-memory test double: what each command
//! asks of the engine and what each event does to the state.
//!
//! Failures the private-download tests guard against (a private download
//! must not leak into a list that outlives the private window; the browser
//! writes no downloads to disk, so what can leak is the in-memory list):
//!   1. a private download shown in a normal window's list or toast;
//!   2. a normal download wrongly dropped or marked private by the filter;
//!   3. a private window's own download missing from its list.
//!
//! Failures the find and download-control tests guard against:
//!   1. "Clear finished" empties the window's list but the engine keeps
//!      every download it was told of, so its own list grows for good;
//!   2. the same call stops a download that is still running;
//!   3. a download's pause, resume and cancel buttons drawn for an engine
//!      that has no such call, where they do nothing and say nothing;
//!   4. find learned from the first tab (the engine is asked with a session)
//!      but the window keeps reading "not available" from before it.

use bingux_ui::web_view::input::{EditCommand, Mods, PageInput};
use bingux_ui::web_view::requests::{ContextTarget, RequestKind};
use bingux_ui::web_view::test_double::{Call, TestDoubleBackend};
use bingux_ui::web_view::{Response, TabEvent};

use crate::browser::testing::{browser, go};
use crate::browser::{Browser, DuplicatesAsk, Options, UiRequest};
use crate::commands::Command;
use crate::tabs::Place;
use crate::tabs::{Page, TabKind};

#[test]
fn the_first_tab_is_blank_and_the_engine_is_ready_after_its_event() {
    let (browser, _) = browser();
    assert_eq!(browser.tabs.len(), 1);
    assert_eq!(browser.active().map(|tab| tab.page()), Some(Page::NewTab));
    assert_eq!(browser.engine.status, crate::engine::Status::Ready);
}

#[test]
fn typing_an_address_loads_it_and_the_events_fill_in_the_tab() {
    let (mut browser, double) = browser();
    let key = go(&mut browser, "example.com");
    let tab = browser.tabs.get(key).unwrap();
    assert_eq!(tab.url, "https://example.com");
    assert_eq!(tab.title, "Example Domain");
    assert!(!tab.load.loading);
    assert_eq!(
        tab.security.state,
        bingux_ui::web_view::SecurityState::Secure
    );
    assert_eq!(tab.page(), Page::Web);
    assert!(
        double
            .calls()
            .iter()
            .any(|call| matches!(call, Call::Load(_, url) if url == "https://example.com"))
    );
}

#[test]
fn back_and_forward_follow_what_the_engine_says_is_possible() {
    let (mut browser, _) = browser();
    let key = go(&mut browser, "example.com");
    assert!(!browser.tabs.get(key).unwrap().can_back);
    go(&mut browser, "news.bingux.dev");
    assert!(browser.tabs.get(key).unwrap().can_back);
    browser.back(key);
    browser.poll();
    let tab = browser.tabs.get(key).unwrap();
    assert_eq!(tab.url, "https://example.com");
    assert!(tab.can_forward && !tab.can_back);
    assert_eq!(tab.history.len(), 2);
}

#[test]
fn a_page_that_is_loading_shows_progress_and_stop_ends_it() {
    let double = TestDoubleBackend::manual();
    let mut browser = Browser::new(double.clone(), Options::default());
    let key = browser.open_tab("example.com", Place::End, true).unwrap();
    browser.poll();
    assert!(browser.tabs.get(key).unwrap().load.loading);
    assert_eq!(
        browser.tabs.get(key).unwrap().display_title(),
        "Loading\u{2026}"
    );
    browser.stop(key);
    assert!(!browser.tabs.get(key).unwrap().load.loading);
    assert!(
        double
            .calls()
            .iter()
            .any(|call| matches!(call, Call::Stop(_)))
    );
    browser.reload(key, false);
    double.finish_loads();
    browser.poll();
    assert_eq!(browser.tabs.get(key).unwrap().title, "Example Domain");
}

#[test]
fn a_new_appearance_reaches_the_engine_once_and_decides_the_scheme_of_new_tabs() {
    use bingux_ui::web_view::{PageEnvironment, PageScheme};
    let (mut browser, double) = browser();
    let sent = |double: &TestDoubleBackend| {
        double
            .calls()
            .iter()
            .filter(|call| matches!(call, Call::SetEnvironment(_)))
            .count()
    };
    let dark = PageEnvironment {
        scheme: PageScheme::Dark,
        ..PageEnvironment::default()
    };
    browser.set_environment(dark.clone());
    browser.set_environment(dark);
    assert_eq!(sent(&double), 1, "the same appearance twice is one call");
    browser.open_tab("example.com", Place::End, true);
    let made = double.calls().into_iter().filter_map(|call| match call {
        Call::CreateTab(config) => Some(config.dark),
        _ => None,
    });
    assert_eq!(made.last(), Some(true), "a tab made now is dark");
}

#[test]
fn closing_tabs_selects_by_the_rules_and_the_last_one_closes_the_window() {
    let (mut browser, double) = browser();
    let first = browser.tabs.active_key();
    let second = browser
        .open_tab("example.com", Place::AfterActive, true)
        .unwrap();
    browser.poll();
    assert_eq!(browser.tabs.active_key(), second);
    browser.close_tab(second);
    assert_eq!(browser.tabs.active_key(), first, "the page that opened it");
    assert!(
        double
            .calls()
            .iter()
            .any(|call| matches!(call, Call::CloseTab(_)))
    );
    browser.close_tab(first);
    assert_eq!(browser.take_ui(), [UiRequest::CloseWindow]);
    assert!(browser.tabs.is_empty());
}

#[test]
fn a_hidden_tab_is_shown_with_the_size_of_the_view_when_it_is_selected() {
    let (mut browser, double) = browser();
    let first = browser.tabs.active_key();
    browser.resize(800, 500, 2.0);
    let second = browser.open_tab("example.com", Place::End, false).unwrap();
    assert_eq!(
        browser.tabs.active_key(),
        first,
        "a background tab does not take the selection"
    );
    double.clear_calls();
    browser.select(second);
    let session = browser.session_of(second).unwrap();
    let old = browser.session_of(first).unwrap();
    let calls = double.calls();
    assert!(calls.contains(&Call::SetVisible(old, false, false)));
    assert!(calls.contains(&Call::SetSize(session, 800, 500, 2.0)));
    assert!(calls.contains(&Call::SetVisible(session, true, true)));
}

#[test]
fn reopening_a_closed_tab_brings_back_its_address_and_its_pin() {
    let (mut browser, _) = browser();
    let key = browser
        .open_tab("news.bingux.dev", Place::End, true)
        .unwrap();
    browser.poll();
    browser.set_pinned(key, true);
    browser.close_tab(key);
    browser.reopen_closed();
    browser.poll();
    let tab = browser.tabs.active().unwrap();
    assert_eq!(tab.url, "https://news.bingux.dev");
    assert!(tab.pinned);
    browser.closed.clear();
    browser.reopen_closed();
    assert_eq!(browser.take_notices().len(), 1, "nothing to reopen says so");
}

#[test]
fn settings_is_one_tab_that_is_found_again_and_becomes_a_page_when_navigated_away() {
    let (mut browser, double) = browser();
    let settings = browser.open_settings();
    assert_eq!(browser.open_settings(), settings, "there is only one");
    assert_eq!(browser.tabs.get(settings).unwrap().kind, TabKind::Settings);
    assert_eq!(browser.tabs.get(settings).unwrap().page(), Page::Settings);
    assert!(browser.tabs.get(settings).unwrap().session.is_none());
    browser.navigate(settings, "example.com");
    browser.poll();
    let tab = browser.tabs.get(settings).unwrap();
    assert_eq!(tab.kind, TabKind::Web);
    assert!(tab.session.is_some());
    assert!(
        double
            .calls()
            .iter()
            .any(|call| matches!(call, Call::CreateTab(_)))
    );
}

#[test]
fn the_context_menu_is_held_for_the_window_and_closing_it_tells_the_engine() {
    let (mut browser, double) = browser();
    let key = go(&mut browser, "example.com");
    let session = browser.session_of(key).unwrap();
    double.ask(
        session,
        RequestKind::ContextMenu(ContextTarget {
            link: Some(("https://example.com/x".to_string(), "x".to_string())),
            ..ContextTarget::default()
        }),
    );
    browser.poll();
    assert!(browser.menu.is_some());
    double.clear_calls();
    browser.dismiss_menu();
    assert!(browser.menu.is_none());
    assert!(
        double
            .calls()
            .iter()
            .any(|call| matches!(call, Call::Respond(_, _, Response::ContextMenu)))
    );
}

#[test]
fn a_window_opened_by_a_page_becomes_a_tab_next_to_its_opener() {
    let (mut browser, double) = browser();
    let opener = go(&mut browser, "example.com");
    let session = browser.session_of(opener).unwrap();
    double.open_window(session, "https://news.bingux.dev/popup");
    browser.poll();
    assert_eq!(browser.tabs.len(), 2);
    let popup = browser.tabs.active().unwrap();
    assert_eq!(popup.opener, Some(opener));
    assert_eq!(popup.url, "https://news.bingux.dev/popup");
    assert_eq!(browser.tabs.position(popup.key), Some(1));
    assert!(
        double
            .calls()
            .iter()
            .any(|call| matches!(call, Call::AcceptTab(_)))
    );
    // `window.close()` closes it and goes back to the page that opened it.
    let popup_key = popup.key;
    let popup_session = popup.session.unwrap();
    double.emit(bingux_ui::web_view::BrowserEvent::tab(
        popup_session,
        TabEvent::CloseRequested,
    ));
    browser.poll();
    assert!(browser.tabs.get(popup_key).is_none());
    assert_eq!(browser.tabs.active_key(), opener);
}

#[test]
fn page_input_reaches_the_tab_it_is_for() {
    let (mut browser, double) = browser();
    let key = go(&mut browser, "example.com");
    let session = browser.session_of(key).unwrap();
    double.clear_calls();
    browser.run(Command::Edit(EditCommand::Copy));
    browser.send(
        key,
        PageInput::ContextMenu {
            at: Some((1.0, 2.0)),
            modifiers: Mods::default(),
        },
    );
    let calls = double.calls();
    assert_eq!(
        calls[0],
        Call::Send(session, PageInput::Edit(EditCommand::Copy))
    );
    assert!(matches!(
        calls[1],
        Call::Send(_, PageInput::ContextMenu { .. })
    ));
}

#[test]
fn closing_other_tabs_keeps_the_pinned_ones() {
    let (mut browser, _) = browser();
    let a = browser.tabs.active_key();
    let b = browser.open_tab("example.com", Place::End, false).unwrap();
    let c = browser
        .open_tab("news.bingux.dev", Place::End, false)
        .unwrap();
    browser.set_pinned(b, true);
    browser.close_others(a);
    let keys: Vec<u64> = browser.tabs.all().iter().map(|tab| tab.key).collect();
    assert_eq!(keys, [b, a]);
    assert!(browser.tabs.get(c).is_none());
}

#[test]
fn muting_goes_to_the_engine_and_is_shown_once_it_agrees() {
    let (mut browser, double) = browser();
    let key = go(&mut browser, "video.example.net");
    assert!(browser.tabs.get(key).unwrap().audio.playing);
    browser.toggle_mute(key);
    assert!(browser.tabs.get(key).unwrap().audio.muted);
    assert!(
        double
            .calls()
            .iter()
            .any(|call| matches!(call, Call::SetMuted(_, true)))
    );
}

#[test]
fn search_all_tabs_asks_the_window_to_search_the_open_tabs() {
    let (mut browser, _) = browser();
    browser.run(Command::SearchTabs);
    assert_eq!(browser.take_ui(), [UiRequest::SearchTabs]);
}

/// A window with tabs `first`, `news`, then `extra` more of `example.com` after the
/// first one, which is the tab on show.
fn with_duplicates(extra: usize) -> (Browser, u64, u64, Vec<u64>) {
    let (mut browser, _) = browser();
    let first = browser.tabs.active_key();
    browser.navigate(first, "example.com");
    let news = browser
        .open_tab("news.bingux.dev", Place::End, false)
        .unwrap();
    let copies: Vec<u64> = (0..extra)
        .map(|_| browser.open_tab("example.com", Place::End, false).unwrap())
        .collect();
    browser.poll();
    (browser, first, news, copies)
}

fn keys(browser: &Browser) -> Vec<u64> {
    browser.tabs.all().iter().map(|tab| tab.key).collect()
}

#[test]
fn the_first_close_duplicate_tabs_of_a_profile_explains_itself_and_closes_nothing_until_agreed() {
    let (mut browser, first, news, copies) = with_duplicates(2);
    assert_eq!(browser.tabs.duplicates(), copies);
    assert!(!browser.settings.have_shown_close_all_duplicate_tabs_warning);
    browser.close_duplicate_tabs();
    assert_eq!(
        browser.take_ui(),
        [UiRequest::ConfirmCloseDuplicates {
            keys: copies.clone(),
            ask: DuplicatesAsk::FirstTime,
        }],
        "Firefox asks the first time, whatever the number of tabs"
    );
    assert_eq!(keys(&browser), [vec![first, news], copies.clone()].concat());
    assert!(
        browser.settings.have_shown_close_all_duplicate_tabs_warning,
        "the profile remembers it asked before the answer is known, as Firefox sets the pref first"
    );
    assert!(browser.take_notices().is_empty());
    // Close tabs: they go, the tab on show stays, and the hint says how many.
    browser.close_tabs_as_duplicates(copies);
    assert_eq!(
        keys(&browser),
        [first, news],
        "the first tab of each address stays"
    );
    assert_eq!(browser.tabs.active_key(), first);
    assert!(browser.tabs.duplicates().is_empty());
    let notices = browser.take_notices();
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].text, "Closed 2 tabs");
}

#[test]
fn a_cancelled_first_close_duplicate_tabs_is_not_asked_again() {
    let (mut browser, first, news, copies) = with_duplicates(2);
    browser.close_duplicate_tabs();
    browser.take_ui();
    // The person said Cancel: nothing closed, and the next time closes at once.
    assert_eq!(keys(&browser), [vec![first, news], copies].concat());
    browser.close_duplicate_tabs();
    assert!(browser.take_ui().is_empty());
    assert_eq!(keys(&browser), [first, news]);
    assert_eq!(browser.take_notices()[0].text, "Closed 2 tabs");
}

#[test]
fn one_duplicate_closes_without_asking_after_the_first_time_and_the_hint_is_singular() {
    let (mut browser, first, news, copies) = with_duplicates(1);
    browser.settings.have_shown_close_all_duplicate_tabs_warning = true;
    browser.close_duplicate_tabs();
    assert!(browser.take_ui().is_empty());
    assert_eq!(keys(&browser), [first, news]);
    assert_eq!(copies.len(), 1);
    assert_eq!(browser.take_notices()[0].text, "Closed 1 tab");
}

#[test]
fn more_duplicates_than_reopen_closed_tab_keeps_are_asked_about_by_their_number() {
    let (mut browser, _, _, copies) = with_duplicates(26);
    browser.settings.have_shown_close_all_duplicate_tabs_warning = true;
    browser.close_duplicate_tabs();
    assert_eq!(
        browser.take_ui(),
        [UiRequest::ConfirmCloseDuplicates {
            keys: copies,
            ask: DuplicatesAsk::Many(26),
        }]
    );
    assert_eq!(keys(&browser).len(), 28, "nothing closed before the answer");
    // Exactly as many as the undo list holds close without asking.
    let (mut browser, first, news, _) = with_duplicates(25);
    browser.settings.have_shown_close_all_duplicate_tabs_warning = true;
    browser.close_duplicate_tabs();
    assert!(browser.take_ui().is_empty());
    assert_eq!(keys(&browser), [first, news]);
    assert_eq!(browser.take_notices()[0].text, "Closed 25 tabs");
}
