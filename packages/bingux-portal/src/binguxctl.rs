//! A client for the `binguxctl capture` request API.
//!
//! `binguxctl` talks to the running shell. This module runs it with a timeout
//! and parses its JSON replies.

use std::ffi::OsString;
use std::process::Stdio;
use std::time::Duration;

use async_process::Command;
use futures_util::future::{Either, select};
use serde::Deserialize;

/// The environment variable that overrides the `binguxctl` program.
pub const CTL_ENV: &str = "BINGUXCTL";
const DEFAULT_PROGRAM: &str = "binguxctl";
const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CaptureState {
    Pending,
    Saved,
    Cancelled,
    Error,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CaptureResult {
    pub id: String,
    pub state: CaptureState,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub message: String,
}

#[derive(Deserialize)]
struct RequestReply {
    ok: bool,
    #[serde(default)]
    error: String,
}

#[derive(Debug, Clone)]
pub struct Ctl {
    program: OsString,
}

impl Ctl {
    pub fn new(program: impl Into<OsString>) -> Self {
        Ctl {
            program: program.into(),
        }
    }

    /// Uses `$BINGUXCTL` when it is set and non-empty, else `binguxctl` on PATH.
    pub fn from_env() -> Self {
        match std::env::var_os(CTL_ENV).filter(|v| !v.is_empty()) {
            Some(program) => Ctl::new(program),
            None => Ctl::new(DEFAULT_PROGRAM),
        }
    }

    /// Asks the shell to start a screenshot for `id`. `interactive` opens the
    /// selector. Otherwise the whole desktop is captured without UI.
    pub async fn request(&self, id: &str, interactive: bool) -> Result<(), String> {
        let args = request_args(id, interactive);
        let output = self.run(&args).await?;
        parse_request_reply(&output)
    }

    pub async fn result(&self, id: &str) -> Result<CaptureResult, String> {
        let output = self.run(&["capture", "result", id]).await?;
        parse_result(&output)
    }

    pub async fn cancel(&self, id: &str) -> Result<(), String> {
        let output = self.run(&["capture", "cancel-request", id]).await?;
        parse_request_reply(&output)
    }

    async fn run(&self, args: &[&str]) -> Result<String, String> {
        let child = Command::new(&self.program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .output();
        let timer = async_io::Timer::after(COMMAND_TIMEOUT);
        let output = match select(Box::pin(child), Box::pin(timer)).await {
            Either::Left((output, _)) => {
                output.map_err(|e| format!("cannot run binguxctl: {e}"))?
            }
            Either::Right(_) => return Err("binguxctl timed out".to_owned()),
        };
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        if stdout.trim().is_empty() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("binguxctl gave no reply: {}", stderr.trim()));
        }
        Ok(stdout)
    }
}

/// The arguments for `capture request`.
pub fn request_args(id: &str, interactive: bool) -> Vec<&str> {
    if interactive {
        vec![
            "capture",
            "request",
            id,
            "--interactive",
            "--mode",
            "screenshot",
        ]
    } else {
        vec!["capture", "request", id, "--desktop"]
    }
}

pub fn parse_request_reply(text: &str) -> Result<(), String> {
    let reply: RequestReply =
        serde_json::from_str(text.trim()).map_err(|e| format!("bad binguxctl reply: {e}"))?;
    if reply.ok {
        Ok(())
    } else if reply.error.is_empty() {
        Err("binguxctl refused the request".to_owned())
    } else {
        Err(reply.error)
    }
}

pub fn parse_result(text: &str) -> Result<CaptureResult, String> {
    serde_json::from_str(text.trim()).map_err(|e| format!("bad binguxctl result: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_arguments_match_the_shell_contract() {
        assert_eq!(
            request_args("1_2-abc", true),
            [
                "capture",
                "request",
                "1_2-abc",
                "--interactive",
                "--mode",
                "screenshot"
            ]
        );
        assert_eq!(
            request_args("1_2-abc", false),
            ["capture", "request", "1_2-abc", "--desktop"]
        );
    }

    #[test]
    fn request_reply_parses_ok_and_error() {
        assert_eq!(parse_request_reply(r#"{"ok":true}"#), Ok(()));
        assert_eq!(
            parse_request_reply(r#"{"ok":false,"error":"busy"}"#),
            Err("busy".to_owned())
        );
        assert!(parse_request_reply(r#"{"ok":false}"#).is_err());
        assert!(parse_request_reply("not json").is_err());
    }

    #[test]
    fn result_parses_every_state() {
        let saved =
            parse_result(r#"{"id":"a","state":"saved","path":"/x/y.png","message":""}"#).unwrap();
        assert_eq!(saved.state, CaptureState::Saved);
        assert_eq!(saved.path, "/x/y.png");
        for (text, state) in [
            ("pending", CaptureState::Pending),
            ("cancelled", CaptureState::Cancelled),
            ("error", CaptureState::Error),
            ("unknown", CaptureState::Unknown),
        ] {
            let json = format!(r#"{{"id":"a","state":"{text}"}}"#);
            assert_eq!(parse_result(&json).unwrap().state, state);
        }
        assert!(parse_result(r#"{"id":"a","state":"bogus"}"#).is_err());
    }
}
