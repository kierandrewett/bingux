//! What was typed in the address bar, and the search engine it is sent to.

use bingux_ui::web_view::test_double::Call;

use super::*;
use crate::browser::testing::{browser, go};
use crate::model::tab::TabKind;

fn address(text: &str) -> String {
    match classify(text) {
        Typed::Address(url) => url,
        other => panic!("{text:?} should be an address, was {other:?}"),
    }
}

fn search(text: &str) -> String {
    match classify(text) {
        Typed::Search(words) => words,
        other => panic!("{text:?} should be a search, was {other:?}"),
    }
}

#[test]
fn a_name_with_a_dot_is_an_address_over_https() {
    assert_eq!(address("example.com"), "https://example.com");
    assert_eq!(
        address("docs.example.org/a/b?c=d#e"),
        "https://docs.example.org/a/b?c=d#e"
    );
    assert_eq!(address("  example.com  "), "https://example.com");
    assert_eq!(address("bücher.example"), "https://bücher.example");
}

#[test]
fn this_machine_and_numbers_are_plain_http() {
    assert_eq!(address("localhost"), "http://localhost");
    assert_eq!(address("localhost:8080/x"), "http://localhost:8080/x");
    assert_eq!(address("192.168.1.1"), "http://192.168.1.1");
    assert_eq!(address("192.168.1.1:3000"), "http://192.168.1.1:3000");
    assert_eq!(address("[::1]:8080"), "http://[::1]:8080");
}

#[test]
fn a_word_with_a_port_is_a_host() {
    assert_eq!(address("router:8080"), "https://router:8080");
}

#[test]
fn a_scheme_is_kept_as_typed() {
    assert_eq!(address("http://example.com"), "http://example.com");
    assert_eq!(address("file:///etc/hostname"), "file:///etc/hostname");
    assert_eq!(address("about:blank"), "about:blank");
    assert_eq!(
        address("view-source:https://example.com"),
        "view-source:https://example.com"
    );
}

#[test]
fn a_path_is_a_file() {
    assert_eq!(address("/home/me/notes.html"), "file:///home/me/notes.html");
}

#[test]
fn words_and_numbers_are_searches() {
    assert_eq!(search("rust lifetimes"), "rust lifetimes");
    assert_eq!(search("weather"), "weather");
    assert_eq!(search("3.14"), "3.14");
    assert_eq!(search("what is example.com"), "what is example.com");
    assert_eq!(search("notes."), "notes.");
    assert_eq!(search("a..b"), "a..b");
}

#[test]
fn the_address_bar_never_runs_script() {
    assert_eq!(search("javascript:alert(1)"), "javascript:alert(1)");
    assert_eq!(search("JavaScript:void(0)"), "JavaScript:void(0)");
}

#[test]
fn nothing_is_nothing() {
    assert_eq!(classify(""), Typed::Nothing);
    assert_eq!(classify("   "), Typed::Nothing);
    assert_eq!(resolve("  ", &ENGINES[0]), None);
}

#[test]
fn a_search_is_encoded_for_the_engine() {
    assert_eq!(
        resolve("rust & c++ \u{e9}", &ENGINES[0]),
        Some("https://duckduckgo.com/?q=rust+%26+c%2B%2B+%C3%A9".to_string())
    );
    assert_eq!(
        resolve("rust", &ENGINES[2]),
        Some("https://en.wikipedia.org/w/index.php?search=rust".to_string())
    );
}

#[test]
fn the_bar_shows_the_address_without_the_noise() {
    assert_eq!(display("https://example.com/"), "example.com");
    assert_eq!(display("https://example.com/a/"), "example.com/a/");
    assert_eq!(
        display("http://example.com/"),
        "http://example.com",
        "plain http stays visible"
    );
    assert_eq!(display("about:blank"), "");
    assert_eq!(display(""), "");
    assert_eq!(display("bingux:newtab"), "");
}

#[test]
fn host_and_origin_are_read_from_an_address() {
    assert_eq!(
        host("https://User:pw@Docs.Example.org:8443/x?y#z"),
        "docs.example.org"
    );
    assert_eq!(host("http://[::1]:80/"), "::1");
    assert_eq!(
        origin("HTTPS://Example.com:8443/a/b?c"),
        "https://example.com:8443"
    );
    assert!(is_plain_http("HTTP://x"));
    assert!(!is_plain_http("https://x"));
    assert_eq!(initial("https://www.example.com"), Some('E'));
    assert_eq!(initial("about:blank"), None);
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
fn the_pages_of_the_browser_are_reached_by_their_names() {
    let (mut browser, _) = browser();
    let before = browser.tabs.len();
    browser.navigate(browser.tabs.active_key(), "bingux:settings");
    assert_eq!(browser.tabs.len(), before + 1);
    assert_eq!(browser.tabs.active().unwrap().kind, TabKind::Settings);
}
