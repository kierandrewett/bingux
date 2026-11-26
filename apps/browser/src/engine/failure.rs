//! What the page says when the engine itself is gone.

use bingux_ui::icons::Icon;
use bingux_ui::prelude::tr;
use bingux_ui::web_view::FailureCause;

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

impl EnginePage {
    /// How the engine ended: its exit status, or the signal that ended it.
    fn exit_text(code: u32) -> String {
        match code {
            0 => String::new(),
            129..=192 => tr!("browser-engine-signal-detail", signal = code - 128),
            code => tr!("browser-engine-exit-detail", code = code),
        }
    }

    /// The words of the page for an engine that is gone. `message` is the
    /// engine's or the library's own account (it names a folder and a rule when
    /// the profile is not safe); `restartable` says a new engine may be started.
    pub fn of(cause: FailureCause, message: &str, code: u32, restartable: bool) -> Self {
        // The library says "invalid configuration `profile_dir`: profile_dir: ..." for a refused
        // folder: the person needs the folder and the rule, once.
        let message = message.trim();
        let message = message
            .strip_prefix("invalid configuration")
            .and_then(|rest| rest.split_once("`: ").map(|(_, rest)| rest))
            .unwrap_or(message);
        let message = message
            .strip_prefix("profile_dir: ")
            .unwrap_or(message)
            .trim_end_matches('.');
        let restart = if restartable { "yes" } else { "no" };
        let (icon, title, text) = match cause {
            FailureCause::Crashed => (
                Icon::DialogError,
                tr!("browser-engine-crashed-title"),
                tr!("browser-engine-crashed-description", restart = restart),
            ),
            FailureCause::ProtocolViolation => (
                Icon::DialogError,
                tr!("browser-engine-crashed-title"),
                tr!(
                    "browser-engine-violation-description",
                    message = message,
                    restart = restart
                ),
            ),
            FailureCause::ProfileLocked => (
                Icon::DialogWarning,
                tr!("browser-engine-locked-title"),
                tr!("browser-engine-locked-description"),
            ),
            FailureCause::SandboxFloor => (
                Icon::DialogWarning,
                tr!("browser-engine-sandbox-title"),
                tr!("browser-engine-sandbox-description", message = message),
            ),
            FailureCause::ProfileUnsafe => (
                Icon::DialogWarning,
                tr!("browser-engine-unsafe-title"),
                tr!("browser-engine-unsafe-description", message = message),
            ),
            FailureCause::StartFailed => (
                Icon::DialogError,
                tr!("browser-engine-start-failed-title"),
                tr!("browser-engine-start-failed-description", message = message),
            ),
        };
        Self {
            icon,
            title,
            text,
            detail: Self::exit_text(code),
            restart: restartable,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_crash_says_the_tabs_are_kept_when_a_restart_is_safe_and_how_it_ended() {
        crate::locale::register();
        let page = EnginePage::of(
            FailureCause::Crashed,
            "the engine process was killed",
            137,
            true,
        );
        assert!(page.restart);
        assert!(page.text.contains("tabs are kept"));
        assert_eq!(page.detail, "Ended by signal 9");
        let closed = EnginePage::of(FailureCause::Crashed, "gone", 1, false);
        assert!(!closed.restart && closed.text.contains("Close this window"));
        assert_eq!(closed.detail, "Exit status 1");
        assert_eq!(
            EnginePage::of(FailureCause::StartFailed, "x", 0, true).detail,
            "",
            "an engine that never ran has no exit status"
        );
    }

    #[test]
    fn an_unsafe_profile_names_the_folder_and_the_rule_the_engine_gave() {
        crate::locale::register();
        let page = EnginePage::of(
            FailureCause::ProfileUnsafe,
            "profile_dir: /home/u/.local/share/bingux-browser/profile is writable by group or others.",
            0,
            true,
        );
        assert_eq!(page.title, "The profile folder is not safe");
        assert!(page.text.contains(
            "/home/u/.local/share/bingux-browser/profile is writable by group or others"
        ));
        assert!(
            !page.text.contains(".."),
            "one full stop, not two: {}",
            page.text
        );
        assert!(page.restart, "the person fixes the folder and tries again");
    }

    #[test]
    fn the_librarys_own_prefix_is_not_said_twice() {
        crate::locale::register();
        let page = EnginePage::of(
            FailureCause::ProfileUnsafe,
            "invalid configuration `profile_dir`: profile_dir: /p is writable by group or others (mode 0777); another user could replace its contents.",
            0,
            true,
        );
        assert!(
            !page.text.contains("invalid configuration") && !page.text.contains("profile_dir"),
            "{}",
            page.text
        );
        assert!(
            page.text
                .contains("/p is writable by group or others (mode 0777)")
        );
    }

    #[test]
    fn every_cause_has_a_page_with_a_title_and_words() {
        crate::locale::register();
        for cause in [
            FailureCause::Crashed,
            FailureCause::ProtocolViolation,
            FailureCause::ProfileLocked,
            FailureCause::SandboxFloor,
            FailureCause::ProfileUnsafe,
            FailureCause::StartFailed,
        ] {
            let page = EnginePage::of(cause, "a message.", 0, false);
            assert!(!page.title.is_empty() && !page.text.is_empty(), "{cause:?}");
        }
        assert!(
            EnginePage::of(
                FailureCause::ProtocolViolation,
                "sent a frame it did not own.",
                0,
                true
            )
            .text
            .contains("sent a frame it did not own")
        );
    }
}
