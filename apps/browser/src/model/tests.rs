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
use bingux_ui::web_view::requests::{
    ContextTarget, DialogKind, DialogRequest, DownloadInfo, DownloadState, PermissionKind,
    PermissionRequest, RequestKind,
};
use bingux_ui::web_view::test_double::{Call, TestDoubleBackend};
use bingux_ui::web_view::{
    BrowserBackend, BrowserEvent, Capabilities, DownloadId, GoneReason, Response, TabEvent, TabId,
};

use super::shortcuts::Command;
use super::tab::{Page, TabKind};
use super::tabs::Place;
use crate::browser::testing::{browser, go};
use crate::browser::{Browser, Options, UiRequest};

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
fn words_are_searched_for_with_the_engine_the_person_chose() {
    let (mut browser, double) = browser();
    go(&mut browser, "rust lifetimes");
    assert!(double.calls().iter().any(
        |call| matches!(call, Call::Load(_, url) if url == "https://duckduckgo.com/?q=rust+lifetimes")
    ));
    browser.set_search_engine(2);
    go(&mut browser, "rust");
    assert!(double.calls().iter().any(
        |call| matches!(call, Call::Load(_, url) if url == "https://en.wikipedia.org/w/index.php?search=rust")
    ));
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
fn a_private_window_has_no_history_and_its_tabs_are_private() {
    // History itself is the profile store's: `profile/history_tests.rs`.
    let double = TestDoubleBackend::new();
    let mut private = Browser::new(double, Options { private: true });
    private.open_tab("example.com", Place::End, true);
    private.poll();
    assert!(!private.has_history());
    assert_eq!(private.history_len(), 0);
    assert!(private.tabs.active().unwrap().private);
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
fn the_pages_of_the_browser_are_reached_by_their_names() {
    let (mut browser, _) = browser();
    let before = browser.tabs.len();
    browser.navigate(browser.tabs.active_key(), "bingux:settings");
    assert_eq!(browser.tabs.len(), before + 1);
    assert_eq!(browser.tabs.active().unwrap().kind, TabKind::Settings);
}

#[test]
fn a_crashed_tab_shows_the_crash_page_and_a_reload_loads_it_again() {
    let (mut browser, double) = browser();
    let key = go(&mut browser, "crash.example.net");
    let tab = browser.tabs.get(key).unwrap();
    assert_eq!(tab.gone, Some(GoneReason::Crashed));
    assert_eq!(tab.page(), Page::Crashed(GoneReason::Crashed));
    double.clear_calls();
    browser.reload(key, false);
    browser.poll();
    assert!(
        double
            .calls()
            .iter()
            .any(|call| matches!(call, Call::Load(_, url) if url == "https://crash.example.net")),
        "a crashed tab has no page to reload: a new load makes one"
    );
}

#[test]
fn a_load_that_fails_shows_its_page_until_the_next_load() {
    let (mut browser, _) = browser();
    let key = go(&mut browser, "missing.example.net");
    assert!(
        matches!(browser.tabs.get(key).unwrap().page(), Page::Failed(error) if error.name == "NS_ERROR_UNKNOWN_HOST")
    );
    go(&mut browser, "example.com");
    assert_eq!(browser.tabs.get(key).unwrap().page(), Page::Web);
}

#[test]
fn a_certificate_error_can_be_refused_or_proceeded_past() {
    let (mut browser, double) = browser();
    let key = go(&mut browser, "expired.example.net");
    assert!(matches!(
        browser.tabs.get(key).unwrap().page(),
        Page::CertError(_)
    ));
    browser.cert_proceed(key);
    browser.poll();
    let tab = browser.tabs.get(key).unwrap();
    assert_eq!(tab.page(), Page::Web);
    assert_eq!(
        tab.security.state,
        bingux_ui::web_view::SecurityState::Broken,
        "the page loads but is marked not secure"
    );
    assert!(double.calls().iter().any(|call| matches!(
        call,
        Call::Respond(_, _, Response::CertError { proceed: true })
    )));

    let key = go(&mut browser, "selfsigned.example.net");
    browser.cert_go_back(key);
    assert!(browser.tabs.get(key).unwrap().cert.is_none());
    assert!(double.calls().iter().any(|call| matches!(
        call,
        Call::Respond(_, _, Response::CertError { proceed: false })
    )));
}

#[test]
fn a_certificate_that_cannot_be_overridden_cannot_be_proceeded_past() {
    let (mut browser, double) = browser();
    let key = go(&mut browser, "pinned.example.net");
    double.clear_calls();
    browser.cert_proceed(key);
    assert!(browser.tabs.get(key).unwrap().cert.is_some());
    assert!(double.calls().is_empty(), "nothing was sent to the engine");
}

#[test]
fn a_permission_prompt_waits_and_a_remembered_answer_is_reused() {
    let (mut browser, double) = browser();
    let key = go(&mut browser, "perm.example.net");
    let Some(super::tab::Prompt::Permission(_, request)) = browser.prompt(key).cloned() else {
        panic!("a permission prompt");
    };
    assert_eq!(request.kinds, [PermissionKind::Geolocation]);
    double.clear_calls();
    browser.answer_permission(key, true, true);
    assert!(browser.prompt(key).is_none());
    assert!(double.calls().iter().any(|call| matches!(
        call,
        Call::Respond(
            _,
            _,
            Response::Permission {
                allow: true,
                remember: true
            }
        )
    )));
    // The same site asks again: it is answered without a prompt.
    double.clear_calls();
    let session = browser.session_of(key).unwrap();
    double.ask(session, RequestKind::Permission(request));
    browser.poll();
    assert!(
        browser.prompt(key).is_none(),
        "no prompt for a decided site"
    );
    assert!(double.calls().iter().any(|call| matches!(
        call,
        Call::Respond(_, _, Response::Permission { allow: true, .. })
    )));
}

#[test]
fn a_refused_permission_that_is_not_remembered_asks_again() {
    let (mut browser, double) = browser();
    let key = go(&mut browser, "perm.example.net");
    browser.answer_permission(key, false, false);
    let session = browser.session_of(key).unwrap();
    double.ask(
        session,
        RequestKind::Permission(PermissionRequest {
            origin: "https://perm.example.net".to_string(),
            kinds: vec![PermissionKind::Geolocation],
        }),
    );
    browser.poll();
    assert!(browser.prompt(key).is_some());
}

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
    let Some(super::tab::Prompt::Dialog(_, second)) = browser.prompt(key) else {
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
fn find_asks_the_engine_and_shows_the_count() {
    let (mut browser, double) = browser();
    let key = go(&mut browser, "example.com");
    browser.open_find(key);
    assert_eq!(browser.take_ui(), [UiRequest::OpenFind]);
    browser.find(key, "example", true, true);
    browser.poll();
    assert_eq!(browser.tabs.get(key).unwrap().find.result, Some((1, 3)));
    browser.run(Command::FindNext);
    browser.poll();
    assert_eq!(browser.tabs.get(key).unwrap().find.result, Some((2, 3)));
    browser.run(Command::FindPrevious);
    browser.poll();
    assert_eq!(browser.tabs.get(key).unwrap().find.result, Some((1, 3)));
    browser.find(key, "ab", true, true);
    browser.poll();
    assert!(browser.tabs.get(key).unwrap().find.not_found());
    browser.close_find(key);
    assert!(!browser.tabs.get(key).unwrap().find.open);
    assert!(
        double
            .calls()
            .iter()
            .any(|call| matches!(call, Call::FindStop(_)))
    );
}

#[test]
fn find_says_so_when_the_engine_cannot() {
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
fn downloads_are_listed_with_a_notice_and_can_be_controlled() {
    let (mut browser, double) = browser();
    go(&mut browser, "files.example.net");
    assert_eq!(browser.downloads.list().len(), 1);
    let notices = browser.take_notices();
    assert!(
        notices
            .iter()
            .any(|notice| notice.text == "Downloading report-2026.pdf" && notice.action.is_some())
    );
    double.advance_downloads(10_000_000);
    browser.poll();
    let notices = browser.take_notices();
    assert!(
        notices
            .iter()
            .any(|notice| notice.text == "report-2026.pdf was downloaded")
    );
    assert_eq!(browser.downloads.active(), 0);
}

fn download(id: u64, private: bool) -> DownloadInfo {
    DownloadInfo {
        id: DownloadId(id),
        url: format!("https://example.com/{id}"),
        filename: format!("file-{id}.bin"),
        path: None,
        mime: "application/octet-stream".to_string(),
        state: DownloadState::Running,
        received: 0,
        total: Some(100),
        speed: 0,
        container: 0,
        private,
    }
}

#[test]
fn a_private_download_is_not_in_a_normal_windows_list() {
    let (mut browser, _double) = browser();
    browser.apply(BrowserEvent::Download(download(1, true)));
    browser.apply(BrowserEvent::Download(download(2, false)));
    let ids: Vec<DownloadId> = browser.downloads.list().iter().map(|d| d.id).collect();
    assert_eq!(ids, [DownloadId(2)]);
    assert!(
        browser
            .take_notices()
            .iter()
            .all(|notice| !notice.text.contains("file-1")),
        "nor does it make a toast"
    );
    assert_eq!(browser.downloads.unseen(), 1);
}

#[test]
fn a_private_download_is_in_a_private_windows_list_and_a_normal_one_is_unchanged() {
    let double = TestDoubleBackend::new();
    let mut browser = Browser::new(double, Options { private: true });
    browser.apply(BrowserEvent::Download(download(1, true)));
    browser.apply(BrowserEvent::Download(download(2, false)));
    let list = browser.downloads.list();
    assert_eq!(list.len(), 2);
    assert!(list[1].private && !list[0].private, "each keeps its flag");
    assert!(
        browser
            .take_notices()
            .iter()
            .any(|notice| notice.text == "Downloading file-1.bin")
    );
}

#[test]
fn a_normal_download_is_unchanged_in_a_normal_window() {
    let (mut browser, _double) = browser();
    browser.apply(BrowserEvent::Download(download(5, false)));
    let list = browser.downloads.list();
    assert_eq!(list.len(), 1);
    assert_eq!(
        (list[0].id, list[0].private, list[0].container),
        (DownloadId(5), false, 0)
    );
}

#[test]
fn clearing_the_list_has_the_engine_forget_what_is_over_and_nothing_else() {
    let (mut browser, double) = browser();
    go(&mut browser, "files.example.net");
    browser.apply(BrowserEvent::Download(download(8, false)));
    double.advance_downloads(10_000_000);
    browser.poll();
    // The page's download is over; the two the test added still go.
    let mut running = download(9, false);
    running.total = None;
    browser.apply(BrowserEvent::Download(running));
    double.clear_calls();
    let over: Vec<DownloadId> = browser
        .downloads
        .list()
        .iter()
        .filter(|d| !matches!(d.state, DownloadState::Running | DownloadState::Paused))
        .map(|d| d.id)
        .collect();
    assert!(!over.is_empty());
    browser.clear_finished_downloads();
    let removed: Vec<DownloadId> = double
        .calls()
        .iter()
        .filter_map(|call| match call {
            Call::RemoveDownload(id) => Some(*id),
            _ => None,
        })
        .collect();
    assert_eq!(removed.len(), over.len());
    for id in &over {
        assert!(removed.contains(id), "{id:?} is forgotten");
    }
    assert!(
        !removed.contains(&DownloadId(8)) && !removed.contains(&DownloadId(9)),
        "a running one is not"
    );
    let left: Vec<DownloadId> = browser.downloads.list().iter().map(|d| d.id).collect();
    assert_eq!(left, [DownloadId(9), DownloadId(8)]);
}

#[test]
fn what_the_engine_took_of_find_is_read_again_when_the_bar_opens() {
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
fn print_and_unsupported_things_say_so() {
    let (mut browser, _) = browser();
    browser.run(Command::Print);
    let notices = browser.take_notices();
    assert_eq!(notices.len(), 1);
    assert!(notices[0].error);
}
