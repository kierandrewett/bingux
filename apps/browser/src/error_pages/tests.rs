//! What each failure says, and what a crashed or untrusted tab does.

use bingux_ui::web_view::requests::{CertError, CertProblem};
use bingux_ui::web_view::test_double::Call;
use bingux_ui::web_view::{GoneReason, LoadError, Response};

use super::*;
use crate::browser::testing::{browser, go};
use crate::model::tab::Page;

fn error(name: &str) -> LoadError {
    LoadError {
        code: 0,
        name: name.to_string(),
    }
}

#[test]
fn an_unknown_host_names_the_host_and_offers_a_retry() {
    let page = for_load_error(&error("NS_ERROR_UNKNOWN_HOST"), "missing.example");
    assert!(page.text.contains("missing.example"));
    assert!(page.retry);
    assert_eq!(page.code, "NS_ERROR_UNKNOWN_HOST");
}

#[test]
fn an_error_the_browser_does_not_know_still_has_a_page() {
    let page = for_load_error(&error("NS_ERROR_SOMETHING_NEW"), "a.example");
    assert_eq!(page.title, "The page can\u{2019}t be shown");
    assert!(page.text.contains("a.example"));
    assert_eq!(page.code, "NS_ERROR_SOMETHING_NEW");
}

#[test]
fn a_bad_address_is_not_worth_retrying() {
    assert!(!for_load_error(&error("NS_ERROR_MALFORMED_URI"), "").retry);
}

#[test]
fn every_certificate_problem_has_a_sentence_that_names_the_site() {
    for problem in [
        CertProblem::UnknownIssuer,
        CertProblem::SelfSigned,
        CertProblem::Expired,
        CertProblem::NotYetValid,
        CertProblem::WrongHost,
        CertProblem::Revoked,
        CertProblem::Other,
    ] {
        let mut cert = CertError::for_test("expired.example.net");
        cert.problem = problem;
        assert!(
            cert_problem_text(&cert).contains("expired.example.net"),
            "{problem:?}"
        );
    }
    let cert = CertError::for_test("expired.example.net");
    assert!(
        cert_problem_text(&cert).contains("12 March 2025"),
        "an expired certificate says when"
    );
}

#[test]
fn every_way_a_process_goes_away_has_a_page_that_offers_a_reload() {
    for reason in [
        GoneReason::Crashed,
        GoneReason::Killed,
        GoneReason::CleanExit,
    ] {
        let page = for_process_gone(reason);
        assert!(page.retry);
        assert!(!page.text.is_empty());
    }
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
