//! Answer-only CLI adapters. Prompts use stdin; no tools or project context are exposed.
use crate::{config::AiConfig, protocol::MAX_CHAT_RESPONSE_BYTES};
use anyhow::{Result, bail};
use serde_json::Value;
use std::{
    io::{BufRead, BufReader, Read, Write},
    os::unix::process::CommandExt,
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

const SYSTEM: &str = "You are the Bingux desktop search assistant. Answer the user's question directly and concisely. Do not use tools. The input is a JSON conversation, with role and content fields. Respond to the last user message. Use Markdown when helpful.";

pub fn complete(
    config: &AiConfig,
    input: &str,
    update: &mut impl FnMut(&str) -> bool,
) -> Result<String> {
    let harness = config.harness.as_deref().unwrap_or("pi");
    let executable = config.executable.clone().unwrap_or_else(|| harness.into());
    let mut command = Command::new(executable);
    match harness {
        "pi" => {
            command.args([
                "--print",
                "--mode",
                "json",
                "--no-session",
                "--no-tools",
                "--no-extensions",
                "--no-skills",
                "--no-prompt-templates",
                "--no-context-files",
                "--offline",
                "--thinking",
                "off",
                "--system-prompt",
                SYSTEM,
            ]);
        }
        "claude" => {
            command.args([
                "--print",
                "--output-format",
                "stream-json",
                "--verbose",
                "--include-partial-messages",
                "--no-session-persistence",
                "--tools",
                "",
                "--strict-mcp-config",
                "--mcp-config",
                "{\"mcpServers\":{}}",
                "--setting-sources",
                "",
                "--settings",
                "{\"disableAllHooks\":true}",
                "--system-prompt",
                SYSTEM,
            ]);
        }
        _ => bail!("Unsupported AI harness"),
    }
    if !config.model.is_empty() {
        command.args(["--model", &config.model]);
    }
    // A neutral cwd avoids project instructions. Keep the harness's existing login environment.
    command
        .current_dir("/")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0);
    let mut child = command.spawn()?;
    let pid = child.id();
    let result = (|| {
        let mut stdin = child.stdin.take().unwrap();
        let input = input.as_bytes().to_vec();
        thread::spawn(move || {
            let _ = stdin.write_all(&input);
        });
        let stdout = child.stdout.take().unwrap();
        let (tx, rx) = mpsc::sync_channel(16);
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = Vec::new();
                match reader
                    .by_ref()
                    .take(256 * 1024 + 1)
                    .read_until(b'\n', &mut line)
                {
                    Ok(0) => break,
                    Ok(_) if line.len() <= 256 * 1024 => {
                        if tx.send(line).is_err() {
                            break;
                        }
                    }
                    _ => break,
                }
            }
        });
        let started = Instant::now();
        let mut last_update = Instant::now() - Duration::from_secs(1);
        let mut message = String::new();
        let mut sent = String::new();
        let mut capped = false;
        loop {
            // Empty callbacks check cancellation even while the model is thinking.
            if !update("") {
                bail!("AI request cancelled");
            }
            if started.elapsed() > Duration::from_secs(60) {
                bail!("AI request timed out");
            }
            match rx.recv_timeout(Duration::from_millis(40)) {
                Ok(line) => {
                    if let Ok(event) = serde_json::from_slice::<Value>(&line) {
                        if event.get("is_error").and_then(Value::as_bool) == Some(true) {
                            bail!("AI harness failed");
                        }
                        if let Some(delta) = text_delta(harness, &event) {
                            message.push_str(&delta);
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => (),
            }
            message.retain(|c| !c.is_control() || c == '\n' || c == '\t');
            if message.len() > MAX_CHAT_RESPONSE_BYTES {
                let mut end = MAX_CHAT_RESPONSE_BYTES;
                while !message.is_char_boundary(end) {
                    end -= 1;
                }
                message.truncate(end);
                capped = true;
                break;
            }
            if message != sent
                && !message.trim().is_empty()
                && last_update.elapsed() >= Duration::from_millis(40)
            {
                if !update(&message) {
                    bail!("AI request cancelled");
                }
                sent.clone_from(&message);
                last_update = Instant::now();
            }
        }
        if message.trim().is_empty() {
            bail!("AI harness returned no answer");
        }
        if !capped {
            loop {
                if let Some(status) = child.try_wait()? {
                    if !status.success() {
                        bail!("AI harness failed");
                    }
                    break;
                }
                if !update("") {
                    bail!("AI request cancelled");
                }
                if started.elapsed() > Duration::from_secs(60) {
                    bail!("AI request timed out");
                }
                thread::sleep(Duration::from_millis(20));
            }
        }
        Ok(message)
    })();
    // Also stop descendants on cancellation, timeout, output limit or normal completion.
    unsafe {
        libc::kill(-(pid as i32), libc::SIGKILL);
    }
    let _ = child.wait();
    result
}

fn text_delta<'a>(harness: &str, event: &'a Value) -> Option<&'a str> {
    if harness == "pi" && event["type"] == "message_update" {
        let delta = &event["assistantMessageEvent"];
        if delta["type"] == "text_delta" {
            return delta["delta"].as_str();
        }
    }
    if harness == "claude" && event["type"] == "stream_event" {
        let delta = &event["event"]["delta"];
        if delta["type"] == "text_delta" {
            return delta["text"].as_str();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adapters_only_stream_answer_text() {
        assert_eq!(
            text_delta(
                "pi",
                &serde_json::json!({"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":"Hello"}})
            ),
            Some("Hello")
        );
        assert_eq!(
            text_delta(
                "pi",
                &serde_json::json!({"type":"message_update","assistantMessageEvent":{"type":"thinking_delta","delta":"private"}})
            ),
            None
        );
        assert_eq!(
            text_delta(
                "claude",
                &serde_json::json!({"type":"stream_event","event":{"delta":{"type":"text_delta","text":"Hello"}}})
            ),
            Some("Hello")
        );
    }
}
