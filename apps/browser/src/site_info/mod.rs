//! How the connection of the page on show is described: the icon beside the
//! address and the words in the page info.

use bingux_ui::icons::Icon;
use bingux_ui::prelude::tr;
use bingux_ui::web_view::{Security, SecurityState};

mod button;
mod popover;

pub use button::ConnectionButton;
pub use popover::{PopoverState, SiteInfoPopover};

use crate::tabs::{Tab, TabKind};

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

impl Summary {
    /// Describes `security` for the page at `url`.
    pub fn of(security: &Security, url: &str) -> Self {
        let blank = url.is_empty() || url == "about:blank";
        // A file is local whatever the engine calls its connection.
        let state = if url.starts_with("file://") {
            SecurityState::Local
        } else {
            security.state
        };
        match state {
            _ if blank => Self {
                icon: Icon::WebBrowser,
                tone: Tone::Neutral,
                headline: tr!("browser-site-info-blank-title"),
                detail: tr!("browser-site-info-blank-description"),
            },
            SecurityState::Internal => Self {
                icon: Icon::WebBrowser,
                tone: Tone::Neutral,
                headline: tr!("browser-site-info-internal-title"),
                detail: tr!("browser-site-info-internal-description"),
            },
            SecurityState::Local => Self {
                icon: Icon::Computer,
                tone: Tone::Neutral,
                headline: tr!("browser-site-info-local-title"),
                detail: tr!("browser-site-info-local-description"),
            },
            SecurityState::Secure if security.mixed_passive => Self {
                icon: Icon::DialogWarning,
                tone: Tone::Warning,
                headline: tr!("browser-site-info-mixed-title"),
                detail: tr!("browser-site-info-mixed-description"),
            },
            SecurityState::Secure => Self {
                icon: Icon::NetworkWirelessEncrypted,
                tone: Tone::Good,
                headline: tr!("browser-site-info-secure-title"),
                detail: if security.extended_validation {
                    tr!("browser-site-info-verified-description")
                } else {
                    tr!("browser-site-info-secure-description")
                },
            },
            SecurityState::Broken => Self {
                icon: Icon::DialogWarning,
                tone: Tone::Danger,
                headline: tr!("browser-site-info-insecure-title"),
                detail: if security.mixed_active {
                    tr!("browser-site-info-active-mixed-description")
                } else {
                    tr!("browser-site-info-certificate-description")
                },
            },
            SecurityState::Insecure if url.starts_with("https://") => Self {
                icon: Icon::WebBrowser,
                tone: Tone::Neutral,
                headline: tr!("browser-site-info-connecting-title"),
                detail: tr!("browser-site-info-connecting-description"),
            },
            SecurityState::Insecure => Self {
                icon: Icon::DialogWarning,
                tone: Tone::Warning,
                headline: tr!("browser-site-info-insecure-title"),
                detail: tr!("browser-site-info-insecure-description"),
            },
        }
    }

    /// Describes the connection of `tab`, or of no tab.
    pub fn of_tab(tab: Option<&Tab>) -> Self {
        match tab {
            None => Self::of(&Security::default(), ""),
            Some(tab) => match tab.kind {
                TabKind::Settings => Self::of(
                    &Security {
                        state: SecurityState::Internal,
                        ..Security::default()
                    },
                    "bingux:settings",
                ),
                TabKind::Web => Self::of(&tab.security, &tab.url),
            },
        }
    }
}

#[cfg(test)]
mod tests;
