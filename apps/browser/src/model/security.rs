//! How the connection of the page on show is described: the icon beside the
//! address and the words in the page info.

use bingux_geckoview::{Security, SecurityState};
use bingux_ui::icons::Icon;

/// How much the person should worry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    /// Nothing to say: a page of the browser, a local file.
    Neutral,
    Good,
    Warning,
    Danger,
}

/// What to show for a connection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Summary {
    pub icon: Icon,
    pub tone: Tone,
    /// The short name, also the button's accessible name.
    pub headline: String,
    pub detail: String,
}

/// Describes `security` for the page at `url`.
pub fn summarize(security: &Security, url: &str) -> Summary {
    let blank = url.is_empty() || url == "about:blank";
    // A file is local whatever the engine calls its connection.
    let state = if url.starts_with("file://") { SecurityState::Local } else { security.state };
    match state {
        _ if blank => Summary {
            icon: Icon::WebBrowser,
            tone: Tone::Neutral,
            headline: "No page".to_string(),
            detail: "There is nothing to describe yet.".to_string(),
        },
        SecurityState::Internal => Summary {
            icon: Icon::WebBrowser,
            tone: Tone::Neutral,
            headline: "A page of this browser".to_string(),
            detail: "This page is part of the browser and sends nothing to the network.".to_string(),
        },
        SecurityState::Local => Summary {
            icon: Icon::Computer,
            tone: Tone::Neutral,
            headline: "A file on this computer".to_string(),
            detail: "The page was read from your disk, not from the network.".to_string(),
        },
        SecurityState::Secure if security.mixed_passive => Summary {
            icon: Icon::DialogWarning,
            tone: Tone::Warning,
            headline: "Connection is secure, but some content is not".to_string(),
            detail: "Images or media on this page were loaded without encryption. Someone on the network could see or replace them.".to_string(),
        },
        SecurityState::Secure => Summary {
            icon: Icon::SecurityHigh,
            tone: Tone::Good,
            headline: "Connection is secure".to_string(),
            detail: if security.extended_validation {
                "What you send to this site is private, and the organisation behind it was verified."
            } else {
                "What you send to this site is private on its way there."
            }
            .to_string(),
        },
        SecurityState::Broken => Summary {
            icon: Icon::DialogWarning,
            tone: Tone::Danger,
            headline: "Connection is not secure".to_string(),
            detail: if security.mixed_active {
                "Scripts on this page were loaded without encryption. Someone on the network could change what the page does."
            } else {
                "You went on past a warning about this site\u{2019}s certificate. Do not enter passwords or card numbers here."
            }
            .to_string(),
        },
        SecurityState::Insecure if url.starts_with("https://") => Summary {
            icon: Icon::WebBrowser,
            tone: Tone::Neutral,
            headline: "Connecting".to_string(),
            detail: "The connection is being made; nothing is known about it yet.".to_string(),
        },
        SecurityState::Insecure => Summary {
            icon: Icon::SecurityLow,
            tone: Tone::Warning,
            headline: "Connection is not secure".to_string(),
            detail: "Anyone on the network could read this page and what you send to it. Do not enter passwords or card numbers here.".to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn security(state: SecurityState) -> Security {
        Security {
            state,
            ..Security::default()
        }
    }

    #[test]
    fn a_secure_page_is_good_and_an_http_page_is_a_warning() {
        let secure = summarize(&security(SecurityState::Secure), "https://example.com");
        assert_eq!((secure.tone, secure.icon), (Tone::Good, Icon::SecurityHigh));
        let plain = summarize(&security(SecurityState::Insecure), "http://example.com");
        assert_eq!((plain.tone, plain.icon), (Tone::Warning, Icon::SecurityLow));
    }

    #[test]
    fn mixed_content_changes_the_wording_and_the_tone() {
        let passive = Security {
            mixed_passive: true,
            ..security(SecurityState::Secure)
        };
        assert_eq!(summarize(&passive, "https://a.example").tone, Tone::Warning);
        let active = Security {
            mixed_active: true,
            ..security(SecurityState::Broken)
        };
        let summary = summarize(&active, "https://a.example");
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
            assert_eq!(summarize(&security(state), url).tone, Tone::Neutral, "{state:?} {url}");
        }
    }

    #[test]
    fn an_extended_validation_certificate_says_who_was_verified() {
        let ev = Security {
            extended_validation: true,
            ..security(SecurityState::Secure)
        };
        assert!(summarize(&ev, "https://bank.example").detail.contains("verified"));
    }
}
