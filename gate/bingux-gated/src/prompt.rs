//! Prompt protocol between bingux-gated and the user-facing prompt UI.
//!
//! When the permission database has no entry for a requested resource
//! the daemon sends a [`PromptRequest`] to a [`PromptBackend`] and
//! waits for a [`PromptResponse`].  The backend may be a D-Bus call to
//! a graphical dialog, or the [`TtyPrompter`] fallback that uses
//! stderr / stdin.

use std::path::PathBuf;

use crate::error::{GatedError, Result};

// ── Request / response ────────────────────────────────────────────

/// A prompt sent to the user asking whether a package should be allowed
/// to access a resource.
#[derive(Debug, Clone)]
pub struct PromptRequest {
    /// Unique ID for correlating request ↔ response.
    pub id: u64,
    /// Human-readable package name (e.g. `"firefox"`).
    pub package_name: String,
    /// Optional path to the package icon for the GUI dialog.
    pub package_icon: Option<PathBuf>,
    /// Category of the resource: `"file"`, `"network"`, `"device"`, etc.
    pub resource_type: String,
    /// Human-readable detail: `"~/Downloads/file.pdf"`, `"tcp:443"`, etc.
    pub resource_detail: String,
    /// If `true` the prompt must **not** offer an "Always Allow" option
    /// (e.g. ptrace, mounting, accessing private keys).
    pub is_dangerous: bool,
}

/// The user's response to a permission prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptResponse {
    /// Deny the request (this time, and do not remember).
    Deny,
    /// Allow the request once but do not persist the grant.
    AllowOnce,
    /// Allow and persist to the permission database.
    AlwaysAllow,
}

// ── Backend trait ─────────────────────────────────────────────────

/// Trait for the component that presents a prompt to the user and
/// collects their response.  Implementations must be `Send` so the
/// daemon can hold one in an `Arc` or `Box<dyn PromptBackend>`.
pub trait PromptBackend: Send {
    fn prompt(&self, request: PromptRequest) -> Result<PromptResponse>;
}

// ── TTY fallback ──────────────────────────────────────────────────

/// A minimal prompter that writes to stderr and reads from stdin.
///
/// Intended as a last-resort fallback when no graphical prompt
/// daemon is running (e.g. during headless testing or SSH sessions).
pub struct TtyPrompter;

impl PromptBackend for TtyPrompter {
    fn prompt(&self, request: PromptRequest) -> Result<PromptResponse> {
        use std::io::{BufRead, Write};

        let action = if request.resource_type == "file" {
            format!("access {}", request.resource_detail)
        } else {
            format!("{} {}", request.resource_type, request.resource_detail)
        };

        let stderr = std::io::stderr();
        let mut out = stderr.lock();

        writeln!(out)?;
        writeln!(out, "{} wants to {action}", request.package_name)?;

        if request.is_dangerous {
            write!(out, "[D]eny  [A]llow once: ")?;
        } else {
            write!(out, "[D]eny  [A]llow once  [P]ermanently allow: ")?;
        }
        out.flush()?;

        let stdin = std::io::stdin();
        let mut line = String::new();
        stdin.lock().read_line(&mut line).map_err(|e| {
            GatedError::PromptFailed(format!("failed to read stdin: {e}"))
        })?;

        let choice = line.trim().to_lowercase();
        match choice.as_str() {
            "d" | "deny" => Ok(PromptResponse::Deny),
            "a" | "allow" => Ok(PromptResponse::AllowOnce),
            "p" | "permanently" | "always" if !request.is_dangerous => {
                Ok(PromptResponse::AlwaysAllow)
            }
            _ => {
                // Default to deny on unrecognised input.
                writeln!(out, "Unrecognised input, defaulting to Deny.")?;
                Ok(PromptResponse::Deny)
            }
        }
    }
}

// ── Mock prompter for testing ─────────────────────────────────────

/// A prompter that returns a pre-configured response, for use in tests.
pub struct MockPrompter {
    pub response: PromptResponse,
}

impl MockPrompter {
    pub fn new(response: PromptResponse) -> Self {
        Self { response }
    }
}

impl PromptBackend for MockPrompter {
    fn prompt(&self, _request: PromptRequest) -> Result<PromptResponse> {
        Ok(self.response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_request(dangerous: bool) -> PromptRequest {
        PromptRequest {
            id: 1,
            package_name: "firefox".to_string(),
            package_icon: None,
            resource_type: "file".to_string(),
            resource_detail: "~/Downloads/file.pdf".to_string(),
            is_dangerous: dangerous,
        }
    }

    #[test]
    fn mock_prompter_returns_configured_response() {
        let p = MockPrompter::new(PromptResponse::AllowOnce);
        let resp = p.prompt(sample_request(false)).unwrap();
        assert_eq!(resp, PromptResponse::AllowOnce);
    }

    #[test]
    fn mock_prompter_deny() {
        let p = MockPrompter::new(PromptResponse::Deny);
        let resp = p.prompt(sample_request(false)).unwrap();
        assert_eq!(resp, PromptResponse::Deny);
    }

    #[test]
    fn mock_prompter_always_allow() {
        let p = MockPrompter::new(PromptResponse::AlwaysAllow);
        let resp = p.prompt(sample_request(false)).unwrap();
        assert_eq!(resp, PromptResponse::AlwaysAllow);
    }

    #[test]
    fn prompt_request_dangerous_flag() {
        let req = sample_request(true);
        assert!(req.is_dangerous);
        let req = sample_request(false);
        assert!(!req.is_dangerous);
    }
}
