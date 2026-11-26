//! What the pages say that stand where a page could not be shown. The engine
//! reports a failed load as an `nsresult` and its name; the page the person
//! sees is the browser's, in the browser's words.

mod pages;

use bingux_ui::icons::Icon;
use bingux_ui::web_view::requests::{CertError, CertProblem};
use bingux_ui::web_view::{GoneReason, LoadError};

pub use pages::{cert_error, crashed, failed};

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

/// The page for a failed load of `host`.
pub fn for_load_error(error: &LoadError, host: &str) -> ErrorPage {
    let (icon, title, text, retry) = match error.name.as_str() {
        "NS_ERROR_UNKNOWN_HOST" => (
            Icon::NetworkError,
            "Can\u{2019}t find that site".to_string(),
            format!(
                "The address {host} could not be found. Check it for typing mistakes, and that you are connected."
            ),
            true,
        ),
        "NS_ERROR_CONNECTION_REFUSED" => (
            Icon::NetworkError,
            "The site refused the connection".to_string(),
            format!("{host} is there, but is not accepting connections. It may be down."),
            true,
        ),
        "NS_ERROR_NET_TIMEOUT" => (
            Icon::NetworkError,
            "The site took too long".to_string(),
            format!("{host} did not answer in time."),
            true,
        ),
        "NS_ERROR_OFFLINE" | "NS_ERROR_NET_RESET" | "NS_ERROR_NET_INTERRUPT" => (
            Icon::NetworkOffline,
            "You are offline".to_string(),
            "The connection was lost. Check your network and try again.".to_string(),
            true,
        ),
        "NS_ERROR_PROXY_CONNECTION_REFUSED" => (
            Icon::NetworkError,
            "The proxy refused the connection".to_string(),
            "Check the proxy settings of the system.".to_string(),
            true,
        ),
        "NS_ERROR_MALFORMED_URI" | "NS_ERROR_INVALID_CONTENT_ENCODING" => (
            Icon::DialogWarning,
            "That address is not valid".to_string(),
            "Check it for typing mistakes.".to_string(),
            false,
        ),
        "NS_ERROR_FILE_NOT_FOUND" | "NS_ERROR_FILE_ACCESS_DENIED" => (
            Icon::DialogWarning,
            "The file can\u{2019}t be opened".to_string(),
            "It may have been moved, or this browser may not be allowed to read it.".to_string(),
            false,
        ),
        _ => (
            Icon::DialogWarning,
            "The page can\u{2019}t be shown".to_string(),
            format!("Loading {host} failed."),
            true,
        ),
    };
    ErrorPage {
        icon,
        title,
        text,
        code: error.name.clone(),
        retry,
    }
}

/// What is wrong with a certificate, in a sentence.
pub fn cert_problem_text(error: &CertError) -> String {
    let host = &error.host;
    match error.problem {
        CertProblem::Expired => match &error.valid_until {
            Some(date) => format!("The certificate of {host} expired on {date}."),
            None => format!("The certificate of {host} has expired."),
        },
        CertProblem::NotYetValid => format!("The certificate of {host} is not valid yet."),
        CertProblem::WrongHost => format!("The certificate is for another site, not {host}."),
        CertProblem::SelfSigned => format!("{host} uses a certificate it signed itself."),
        CertProblem::UnknownIssuer => {
            format!("The certificate of {host} was issued by someone this browser does not trust.")
        }
        CertProblem::Revoked => {
            format!("The certificate of {host} was revoked by the one who issued it.")
        }
        CertProblem::Other => {
            format!("The certificate of {host} does not meet this browser\u{2019}s requirements.")
        }
    }
}

/// The text that tells a person who may proceed what it risks.
pub const CERT_RISK: &str = "Someone on the network could read or change what you send and receive. Go ahead only if you know why the certificate is wrong.";

/// The page for a content process that went away.
pub fn for_process_gone(reason: GoneReason) -> ErrorPage {
    match reason {
        GoneReason::Crashed => ErrorPage {
            icon: Icon::DialogError,
            title: "This tab crashed".to_string(),
            text: "The page stopped working and was closed. Reload it to try again. Your other tabs are not affected.".to_string(),
            code: "process crashed".to_string(),
            retry: true,
        },
        GoneReason::Killed => ErrorPage {
            icon: Icon::DialogError,
            title: "This tab was stopped".to_string(),
            text: "The system ended the process that showed the page, often because it ran out of memory. Reload it to try again.".to_string(),
            code: "process killed".to_string(),
            retry: true,
        },
        GoneReason::CleanExit => ErrorPage {
            icon: Icon::DialogInformation,
            title: "This tab was closed".to_string(),
            text: "The process that showed the page has ended. Reload it to show it again.".to_string(),
            code: "process exited".to_string(),
            retry: true,
        },
    }
}

#[cfg(test)]
mod tests;
