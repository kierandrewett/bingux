//! How a connection is described.

use super::*;

fn security(state: SecurityState) -> Security {
    Security {
        state,
        ..Security::default()
    }
}

#[test]
fn a_secure_page_is_good_and_an_http_page_is_a_warning() {
    let secure = Summary::of(&security(SecurityState::Secure), "https://example.com");
    assert_eq!(
        (secure.tone, secure.icon),
        (Tone::Good, Icon::NetworkWirelessEncrypted)
    );
    let plain = Summary::of(&security(SecurityState::Insecure), "http://example.com");
    assert_eq!(
        (plain.tone, plain.icon),
        (Tone::Warning, Icon::DialogWarning)
    );
}

#[test]
fn mixed_content_changes_the_wording_and_the_tone() {
    let passive = Security {
        mixed_passive: true,
        ..security(SecurityState::Secure)
    };
    assert_eq!(
        Summary::of(&passive, "https://a.example").tone,
        Tone::Warning
    );
    let active = Security {
        mixed_active: true,
        ..security(SecurityState::Broken)
    };
    let summary = Summary::of(&active, "https://a.example");
    assert_eq!(summary.tone, Tone::Danger);
    assert!(summary.detail.contains("Scripts"));
}

#[test]
fn a_page_of_the_browser_a_file_and_a_blank_tab_say_nothing_alarming() {
    for (state, url) in [
        (SecurityState::Internal, "bingux:settings"),
        (SecurityState::Local, "file:///tmp/a.html"),
        (SecurityState::Insecure, ""),
        (SecurityState::Insecure, "about:blank"),
    ] {
        assert_eq!(
            Summary::of(&security(state), url).tone,
            Tone::Neutral,
            "{state:?} {url}"
        );
    }
}

#[test]
fn an_extended_validation_certificate_says_who_was_verified() {
    let ev = Security {
        extended_validation: true,
        ..security(SecurityState::Secure)
    };
    assert!(
        Summary::of(&ev, "https://bank.example")
            .detail
            .contains("verified")
    );
}
