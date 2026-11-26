//! How the connection of the page on show is described: the icon beside the
//! address and the words in the page info.

use bingux_ui::icons::Icon;
use bingux_ui::web_view::{Security, SecurityState};

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
    let state = if url.starts_with("file://") {
        SecurityState::Local
    } else {
        security.state
    };
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
            icon: Icon::NetworkWirelessEncrypted,
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
            icon: Icon::DialogWarning,
            tone: Tone::Warning,
            headline: "Connection is not secure".to_string(),
            detail: "Anyone on the network could read this page and what you send to it. Do not enter passwords or card numbers here.".to_string(),
        },
    }
}

#[cfg(test)]
mod tests;
