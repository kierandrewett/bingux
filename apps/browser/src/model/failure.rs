//! What the error pages say. The engine reports a failed load as an `nsresult`
//! and its name; the page the person sees is the browser's, in the browser's
//! words.

use bingux_geckoview::requests::{CertError, CertProblem};
use bingux_geckoview::{FailureCause, GoneReason, LoadError};
use bingux_ui::icons::Icon;

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
            format!("The address {host} could not be found. Check it for typing mistakes, and that you are connected."),
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
        CertProblem::UnknownIssuer => format!("The certificate of {host} was issued by someone this browser does not trust."),
        CertProblem::Revoked => format!("The certificate of {host} was revoked by the one who issued it."),
        CertProblem::Other => format!("The certificate of {host} does not meet this browser\u{2019}s requirements."),
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

/// The page that stands in the whole window when the engine is gone: nothing
/// in the window can work without it, and what to do depends on why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnginePage {
    pub icon: Icon,
    pub title: String,
    pub text: String,
    /// How it ended, for the person who searches for it; empty when there is nothing to say.
    pub detail: String,
    /// "Start again" is offered.
    pub restart: bool,
}

/// How the engine ended: its exit status, or the signal that ended it.
fn exit_text(code: u32) -> String {
    match code {
        0 => String::new(),
        129..=192 => format!("Ended by signal {}", code - 128),
        code => format!("Exit status {code}"),
    }
}

/// The words of the page for an engine that is gone. `message` is the
/// engine's or the library's own account (it names a folder and a rule when
/// the profile is not safe); `restartable` says a new engine may be started.
pub fn for_engine_failure(cause: FailureCause, message: &str, code: u32, restartable: bool) -> EnginePage {
    // The library says "invalid configuration `profile_dir`: profile_dir: ..." for a refused
    // folder: the person needs the folder and the rule, once.
    let message = message.trim();
    let message = message
        .strip_prefix("invalid configuration")
        .and_then(|rest| rest.split_once("`: ").map(|(_, rest)| rest))
        .unwrap_or(message);
    let message = message.strip_prefix("profile_dir: ").unwrap_or(message).trim_end_matches('.');
    let kept = if restartable {
        " Your tabs are kept: start again to open them where they were."
    } else {
        " Close this window and open the browser again."
    };
    let (icon, title, text) = match cause {
        FailureCause::Crashed => (
            Icon::DialogError,
            "Web pages stopped working".to_string(),
            format!("The part of the browser that shows web pages stopped unexpectedly.{kept}"),
        ),
        FailureCause::ProtocolViolation => (
            Icon::DialogError,
            "Web pages stopped working".to_string(),
            format!("The part that shows web pages did something it must not, so the browser ended it: {message}.{kept}"),
        ),
        FailureCause::ProfileLocked => (
            Icon::DialogWarning,
            "This profile is in use".to_string(),
            "Another copy of the browser is using the same profile. Close it, then try again.".to_string(),
        ),
        FailureCause::SandboxFloor => (
            Icon::SecurityLow,
            "The browser could not start safely".to_string(),
            format!("It could not set up the protection that keeps web pages away from your files, so it did not run: {message}."),
        ),
        FailureCause::ProfileUnsafe => (
            Icon::SecurityLow,
            "The profile folder is not safe".to_string(),
            format!("The browser did not start because it does not trust its profile folder: {message}. Make the folder yours alone (no access for anyone else, no links out of it), then try again."),
        ),
        FailureCause::StartFailed => (
            Icon::DialogError,
            "The browser could not start".to_string(),
            format!("{message}. Check how the browser was installed, then try again."),
        ),
    };
    EnginePage {
        icon,
        title,
        text,
        detail: exit_text(code),
        restart: restartable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            assert!(cert_problem_text(&cert).contains("expired.example.net"), "{problem:?}");
        }
        let cert = CertError::for_test("expired.example.net");
        assert!(cert_problem_text(&cert).contains("12 March 2025"), "an expired certificate says when");
    }

    #[test]
    fn every_way_a_process_goes_away_has_a_page_that_offers_a_reload() {
        for reason in [GoneReason::Crashed, GoneReason::Killed, GoneReason::CleanExit] {
            let page = for_process_gone(reason);
            assert!(page.retry);
            assert!(!page.text.is_empty());
        }
    }

    #[test]
    fn a_crash_says_the_tabs_are_kept_when_a_restart_is_safe_and_how_it_ended() {
        let page = for_engine_failure(FailureCause::Crashed, "the engine process was killed", 137, true);
        assert!(page.restart);
        assert!(page.text.contains("tabs are kept"));
        assert_eq!(page.detail, "Ended by signal 9");
        let closed = for_engine_failure(FailureCause::Crashed, "gone", 1, false);
        assert!(!closed.restart && closed.text.contains("Close this window"));
        assert_eq!(closed.detail, "Exit status 1");
        assert_eq!(for_engine_failure(FailureCause::StartFailed, "x", 0, true).detail, "", "an engine that never ran has no exit status");
    }

    #[test]
    fn an_unsafe_profile_names_the_folder_and_the_rule_the_engine_gave() {
        let page = for_engine_failure(
            FailureCause::ProfileUnsafe,
            "profile_dir: /home/u/.local/share/bingux-browser/profile is writable by group or others.",
            0,
            true,
        );
        assert_eq!(page.title, "The profile folder is not safe");
        assert!(page.text.contains("/home/u/.local/share/bingux-browser/profile is writable by group or others"));
        assert!(!page.text.contains(".."), "one full stop, not two: {}", page.text);
        assert!(page.restart, "the person fixes the folder and tries again");
    }

    #[test]
    fn the_librarys_own_prefix_is_not_said_twice() {
        let page = for_engine_failure(
            FailureCause::ProfileUnsafe,
            "invalid configuration `profile_dir`: profile_dir: /p is writable by group or others (mode 0777); another user could replace its contents.",
            0,
            true,
        );
        assert!(!page.text.contains("invalid configuration") && !page.text.contains("profile_dir"), "{}", page.text);
        assert!(page.text.contains("/p is writable by group or others (mode 0777)"));
    }

    #[test]
    fn every_cause_has_a_page_with_a_title_and_words() {
        for cause in [
            FailureCause::Crashed,
            FailureCause::ProtocolViolation,
            FailureCause::ProfileLocked,
            FailureCause::SandboxFloor,
            FailureCause::ProfileUnsafe,
            FailureCause::StartFailed,
        ] {
            let page = for_engine_failure(cause, "a message.", 0, false);
            assert!(!page.title.is_empty() && !page.text.is_empty(), "{cause:?}");
        }
        assert!(for_engine_failure(FailureCause::ProtocolViolation, "sent a frame it did not own.", 0, true).text.contains("sent a frame it did not own"));
    }
}
