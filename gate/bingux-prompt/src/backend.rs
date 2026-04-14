use std::io::{self, Read, Write};
use std::time::Duration;

use crate::types::{PromptRequest, PromptResponse};

/// Errors from prompt backends.
#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    #[error("prompt was dismissed")]
    Dismissed,
    #[error("prompt timed out")]
    Timeout,
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
}

pub type Result<T> = std::result::Result<T, BackendError>;

/// A backend that can present a permission prompt and collect the user's response.
pub trait PromptBackend: Send + Sync {
    fn show_prompt(&self, request: &PromptRequest) -> Result<PromptResponse>;
    fn dismiss(&self, prompt_id: u64) -> Result<()>;
}

// ---------------------------------------------------------------------------
// AutoDenyBackend
// ---------------------------------------------------------------------------

/// Always denies — suitable for headless / non-interactive systems.
pub struct AutoDenyBackend;

impl PromptBackend for AutoDenyBackend {
    fn show_prompt(&self, _request: &PromptRequest) -> Result<PromptResponse> {
        tracing::info!("auto-deny backend: denying prompt");
        Ok(PromptResponse::Deny)
    }

    fn dismiss(&self, _prompt_id: u64) -> Result<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// AutoAllowBackend
// ---------------------------------------------------------------------------

/// Always allows once — useful for testing.
pub struct AutoAllowBackend;

impl PromptBackend for AutoAllowBackend {
    fn show_prompt(&self, _request: &PromptRequest) -> Result<PromptResponse> {
        tracing::info!("auto-allow backend: allowing prompt");
        Ok(PromptResponse::AllowOnce)
    }

    fn dismiss(&self, _prompt_id: u64) -> Result<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// TtyBackend
// ---------------------------------------------------------------------------

/// Interactive TTY backend that prints to stderr and reads from stdin.
pub struct TtyBackend {
    timeout_secs: u64,
}

impl TtyBackend {
    pub fn new(timeout_secs: u64) -> Self {
        Self { timeout_secs }
    }

    fn read_char_with_timeout(&self) -> Result<Option<u8>> {
        // Use a polling approach: check stdin readability with a timeout.
        // On Linux we can use poll(2) via nix, but to keep deps minimal we
        // do a simple blocking read in a thread with a timeout.
        let timeout = Duration::from_secs(self.timeout_secs);
        let (tx, rx) = std::sync::mpsc::channel();

        std::thread::spawn(move || {
            let mut buf = [0u8; 1];
            let result = io::stdin().lock().read(&mut buf);
            let _ = tx.send(result.map(|n| if n > 0 { Some(buf[0]) } else { None }));
        });

        match rx.recv_timeout(timeout) {
            Ok(Ok(ch)) => Ok(ch),
            Ok(Err(e)) => Err(BackendError::Io(e)),
            Err(_) => Err(BackendError::Timeout),
        }
    }
}

impl PromptBackend for TtyBackend {
    fn show_prompt(&self, request: &PromptRequest) -> Result<PromptResponse> {
        let mut stderr = io::stderr().lock();

        writeln!(stderr)?;
        writeln!(stderr, "=== Bingux Permission Prompt ===")?;
        writeln!(stderr, "{}", request.format_message())?;
        writeln!(stderr)?;

        if request.is_dangerous {
            writeln!(stderr, "  [d] Deny")?;
            writeln!(stderr, "  [a] Allow once")?;
        } else {
            writeln!(stderr, "  [d] Deny")?;
            writeln!(stderr, "  [a] Allow once")?;
            writeln!(stderr, "  [p] Always allow")?;
        }

        write!(
            stderr,
            "\nChoice (timeout {}s, default deny): ",
            self.timeout_secs
        )?;
        stderr.flush()?;

        match self.read_char_with_timeout()? {
            Some(b'a' | b'A') => Ok(PromptResponse::AllowOnce),
            Some(b'p' | b'P') if !request.is_dangerous => Ok(PromptResponse::AlwaysAllow),
            _ => Ok(PromptResponse::Deny),
        }
    }

    fn dismiss(&self, _prompt_id: u64) -> Result<()> {
        let mut stderr = io::stderr().lock();
        writeln!(stderr, "[prompt dismissed]")?;
        Ok(())
    }
}
