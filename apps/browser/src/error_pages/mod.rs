//! What the pages say that stand where a page could not be shown. The engine
//! reports a failed load as an `nsresult` and its name; the page the person
//! sees is the browser's, in the browser's words.

mod pages;

use bingux_ui::icons::Icon;
use bingux_ui::prelude::tr;
use bingux_ui::web_view::requests::{CertError, CertProblem};
use bingux_ui::web_view::{GoneReason, LoadError};

pub use pages::{CertErrorPage, CrashedPage, LoadFailedPage};

/// A page that replaces the web page because the load did not make one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ErrorPage {
    pub icon: Icon,
    pub title: String,
    pub text: String,
    /// The engine's name for it, for the person who searches for it.
    pub code: String,
    /// "Try again" makes sense: the problem may go away by itself.
    pub retry: bool,
}

impl ErrorPage {
    /// The page for a failed load of `host`.
    pub fn for_load(error: &LoadError, host: &str) -> Self {
        let (icon, title, text, retry) = match error.name.as_str() {
            "NS_ERROR_UNKNOWN_HOST" => (
                Icon::NetworkError,
                tr!("browser-error-unknown-host-title"),
                tr!("browser-error-unknown-host-description", host = host),
                true,
            ),
            "NS_ERROR_CONNECTION_REFUSED" => (
                Icon::NetworkError,
                tr!("browser-error-refused-title"),
                tr!("browser-error-refused-description", host = host),
                true,
            ),
            "NS_ERROR_NET_TIMEOUT" => (
                Icon::NetworkError,
                tr!("browser-error-timeout-title"),
                tr!("browser-error-timeout-description", host = host),
                true,
            ),
            "NS_ERROR_OFFLINE" | "NS_ERROR_NET_RESET" | "NS_ERROR_NET_INTERRUPT" => (
                Icon::NetworkOffline,
                tr!("browser-error-offline-title"),
                tr!("browser-error-offline-description"),
                true,
            ),
            "NS_ERROR_PROXY_CONNECTION_REFUSED" => (
                Icon::NetworkError,
                tr!("browser-error-proxy-title"),
                tr!("browser-error-proxy-description"),
                true,
            ),
            "NS_ERROR_MALFORMED_URI" | "NS_ERROR_INVALID_CONTENT_ENCODING" => (
                Icon::DialogWarning,
                tr!("browser-error-malformed-title"),
                tr!("browser-error-malformed-description"),
                false,
            ),
            "NS_ERROR_FILE_NOT_FOUND" | "NS_ERROR_FILE_ACCESS_DENIED" => (
                Icon::DialogWarning,
                tr!("browser-error-file-title"),
                tr!("browser-error-file-description"),
                false,
            ),
            _ => (
                Icon::DialogWarning,
                tr!("browser-error-other-title"),
                tr!("browser-error-other-description", host = host),
                true,
            ),
        };
        Self {
            icon,
            title,
            text,
            code: error.name.clone(),
            retry,
        }
    }

    /// The page for a content process that went away.
    pub fn for_crash(reason: GoneReason) -> Self {
        match reason {
            GoneReason::Crashed => Self {
                icon: Icon::DialogError,
                title: tr!("browser-error-crashed-title"),
                text: tr!("browser-error-crashed-description"),
                code: "process crashed".to_string(),
                retry: true,
            },
            GoneReason::Killed => Self {
                icon: Icon::DialogError,
                title: tr!("browser-error-killed-title"),
                text: tr!("browser-error-killed-description"),
                code: "process killed".to_string(),
                retry: true,
            },
            GoneReason::CleanExit => Self {
                icon: Icon::DialogInformation,
                title: tr!("browser-error-exited-title"),
                text: tr!("browser-error-exited-description"),
                code: "process exited".to_string(),
                retry: true,
            },
        }
    }
}

/// A certificate problem, in words.
pub struct CertSentence<'a>(pub &'a CertError);

impl CertSentence<'_> {
    /// What is wrong with a certificate, in a sentence.
    pub fn text(&self) -> String {
        let error = self.0;
        let host = &error.host;
        match error.problem {
            CertProblem::Expired => match &error.valid_until {
                Some(date) => tr!(
                    "browser-error-cert-expired-on",
                    host = host.as_str(),
                    date = date.as_str()
                ),
                None => tr!("browser-error-cert-expired", host = host.as_str()),
            },
            CertProblem::NotYetValid => tr!("browser-error-cert-not-yet", host = host.as_str()),
            CertProblem::WrongHost => tr!("browser-error-cert-wrong-host", host = host.as_str()),
            CertProblem::SelfSigned => tr!("browser-error-cert-self-signed", host = host.as_str()),
            CertProblem::UnknownIssuer => {
                tr!("browser-error-cert-unknown-issuer", host = host.as_str())
            }
            CertProblem::Revoked => {
                tr!("browser-error-cert-revoked", host = host.as_str())
            }
            CertProblem::Other => {
                tr!("browser-error-cert-other", host = host.as_str())
            }
        }
    }
}

#[cfg(test)]
mod tests;
