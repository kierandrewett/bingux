//! Runs one screenshot through the `binguxctl capture` request API.

use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::binguxctl::{CaptureState, Ctl};

pub const POLL_INTERVAL: Duration = Duration::from_millis(200);
/// Failed polls in a row before the capture is abandoned.
pub const MAX_CONSECUTIVE_FAILURES: u32 = 25;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The capture was saved. The value is a `file://` URI.
    Saved(String),
    /// The user cancelled, or the portal client asked for it.
    Cancelled,
    /// The capture could not be completed.
    Failed(String),
}

/// Starts a capture for `capture_id` and waits until it finishes.
///
/// `alive` must report whether the requesting frontend still owns its bus
/// name. If it stops, the capture is cancelled. `cancel` is set when the
/// portal client closes its request.
pub async fn run<F, Fut>(
    ctl: &Ctl,
    capture_id: &str,
    interactive: bool,
    cancel: &AtomicBool,
    alive: F,
) -> Outcome
where
    F: Fn() -> Fut,
    Fut: Future<Output = bool>,
{
    if let Err(message) = ctl.request(capture_id, interactive).await {
        return Outcome::Failed(message);
    }
    let mut failures = 0;
    loop {
        async_io::Timer::after(POLL_INTERVAL).await;
        if cancel.load(Ordering::SeqCst) {
            let _ = ctl.cancel(capture_id).await;
            return Outcome::Cancelled;
        }
        if !alive().await {
            let _ = ctl.cancel(capture_id).await;
            return Outcome::Failed("the portal frontend went away".to_owned());
        }
        match ctl.result(capture_id).await {
            Ok(result) => {
                failures = 0;
                match result.state {
                    CaptureState::Pending => {}
                    CaptureState::Saved => {
                        return match file_uri(&result.path) {
                            Some(uri) => Outcome::Saved(uri),
                            None => {
                                Outcome::Failed("the saved capture has no absolute path".to_owned())
                            }
                        };
                    }
                    CaptureState::Cancelled => return Outcome::Cancelled,
                    CaptureState::Error => {
                        let message = if result.message.is_empty() {
                            "the capture failed".to_owned()
                        } else {
                            result.message
                        };
                        return Outcome::Failed(message);
                    }
                    CaptureState::Unknown => {
                        return Outcome::Failed("the capture request is unknown".to_owned());
                    }
                }
            }
            Err(message) => {
                failures += 1;
                if failures >= MAX_CONSECUTIVE_FAILURES {
                    let _ = ctl.cancel(capture_id).await;
                    return Outcome::Failed(message);
                }
            }
        }
    }
}

/// Converts an absolute path to a percent-encoded `file://` URI.
pub fn file_uri(path: &str) -> Option<String> {
    if !path.starts_with('/') {
        return None;
    }
    let mut uri = String::from("file://");
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                uri.push(byte as char)
            }
            _ => uri.push_str(&format!("%{byte:02X}")),
        }
    }
    Some(uri)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bingux-portal-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Writes a fake `binguxctl` whose `result` reply is `results`, one per poll.
    /// The last entry repeats.
    fn fake_ctl(name: &str, request_reply: &str, results: &[&str]) -> Ctl {
        let dir = temp_dir(name);
        let count = dir.join("count");
        let mut script = String::from("#!/bin/sh\ncase \"$2\" in\n");
        script.push_str(&format!("  request) echo '{request_reply}' ;;\n"));
        script.push_str("  cancel-request) echo '{\"ok\":true}' ;;\n");
        script.push_str("  result)\n");
        script.push_str(&format!(
            "    n=$(cat '{}' 2>/dev/null || echo 0)\n",
            count.display()
        ));
        script.push_str(&format!("    echo $((n+1)) > '{}'\n", count.display()));
        script.push_str("    case $((n+1)) in\n");
        for (index, reply) in results.iter().enumerate() {
            script.push_str(&format!("      {}) echo '{reply}' ;;\n", index + 1));
        }
        script.push_str(&format!("      *) echo '{}' ;;\n", results.last().unwrap()));
        script.push_str("    esac ;;\nesac\n");
        let path = dir.join("binguxctl");
        std::fs::write(&path, script).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        Ctl::new(path)
    }

    async fn alive() -> bool {
        true
    }

    #[test]
    fn file_uri_encodes_the_path() {
        assert_eq!(
            file_uri("/home/u/Pictures/Screenshot 1.png").as_deref(),
            Some("file:///home/u/Pictures/Screenshot%201.png")
        );
        assert_eq!(file_uri("relative.png"), None);
        assert_eq!(
            file_uri("/a/ü.png").as_deref(),
            Some("file:///a/%C3%BC.png")
        );
    }

    #[test]
    fn saved_capture_returns_its_uri_after_pending_polls() {
        let ctl = fake_ctl(
            "saved",
            r#"{"ok":true}"#,
            &[
                r#"{"id":"x","state":"pending"}"#,
                r#"{"id":"x","state":"pending"}"#,
                r#"{"id":"x","state":"saved","path":"/tmp/shot.png"}"#,
            ],
        );
        let cancel = AtomicBool::new(false);
        let outcome = async_io::block_on(run(&ctl, "x", false, &cancel, alive));
        assert_eq!(outcome, Outcome::Saved("file:///tmp/shot.png".to_owned()));
    }

    #[test]
    fn rejected_request_fails_at_once() {
        let ctl = fake_ctl(
            "busy",
            r#"{"ok":false,"error":"busy"}"#,
            &[r#"{"id":"x","state":"pending"}"#],
        );
        let cancel = AtomicBool::new(false);
        let outcome = async_io::block_on(run(&ctl, "x", true, &cancel, alive));
        assert_eq!(outcome, Outcome::Failed("busy".to_owned()));
    }

    #[test]
    fn user_cancel_reports_cancelled() {
        let ctl = fake_ctl(
            "cancelled",
            r#"{"ok":true}"#,
            &[r#"{"id":"x","state":"cancelled"}"#],
        );
        let cancel = AtomicBool::new(false);
        let outcome = async_io::block_on(run(&ctl, "x", false, &cancel, alive));
        assert_eq!(outcome, Outcome::Cancelled);
    }

    #[test]
    fn client_close_cancels_the_capture() {
        let ctl = fake_ctl(
            "close",
            r#"{"ok":true}"#,
            &[r#"{"id":"x","state":"pending"}"#],
        );
        let cancel = AtomicBool::new(true);
        let outcome = async_io::block_on(run(&ctl, "x", false, &cancel, alive));
        assert_eq!(outcome, Outcome::Cancelled);
    }

    #[test]
    fn vanished_frontend_fails_the_capture() {
        let ctl = fake_ctl(
            "gone",
            r#"{"ok":true}"#,
            &[r#"{"id":"x","state":"pending"}"#],
        );
        let cancel = AtomicBool::new(false);
        let outcome = async_io::block_on(run(&ctl, "x", false, &cancel, || async { false }));
        assert!(matches!(outcome, Outcome::Failed(_)));
    }

    #[test]
    fn error_state_reports_the_shell_message() {
        let ctl = fake_ctl(
            "error",
            r#"{"ok":true}"#,
            &[r#"{"id":"x","state":"error","message":"no output"}"#],
        );
        let cancel = AtomicBool::new(false);
        let outcome = async_io::block_on(run(&ctl, "x", false, &cancel, alive));
        assert_eq!(outcome, Outcome::Failed("no output".to_owned()));
    }
}
