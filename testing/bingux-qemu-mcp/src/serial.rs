use std::path::{Path, PathBuf};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use crate::error::{Error, Result};

/// Reader for the QEMU serial console via a Unix socket.
///
/// Maintains a buffer of lines already read, and can wait for new output
/// matching a pattern.
pub struct SerialReader {
    socket_path: PathBuf,
    reader: BufReader<tokio::net::unix::OwnedReadHalf>,
    writer: tokio::net::unix::OwnedWriteHalf,
    buffer: Vec<String>,
}

impl SerialReader {
    /// Connect to the serial console socket.
    pub async fn connect(socket_path: &Path) -> Result<Self> {
        let stream =
            UnixStream::connect(socket_path)
                .await
                .map_err(|e| Error::SerialConnectionFailed {
                    path: socket_path.to_path_buf(),
                    source: e,
                })?;

        let (read_half, write_half) = stream.into_split();
        let reader = BufReader::new(read_half);

        Ok(Self {
            socket_path: socket_path.to_path_buf(),
            reader,
            writer: write_half,
            buffer: Vec::new(),
        })
    }

    /// Read available lines from the serial console.
    ///
    /// If `count` is specified, reads up to that many new lines (with a short
    /// timeout between lines). If `None`, drains all currently-available lines.
    pub async fn read_lines(&mut self, count: Option<usize>) -> Result<Vec<String>> {
        let max = count.unwrap_or(1000);
        let mut new_lines = Vec::new();

        for _ in 0..max {
            let mut line = String::new();
            match tokio::time::timeout(
                std::time::Duration::from_millis(500),
                self.reader.read_line(&mut line),
            )
            .await
            {
                Ok(Ok(0)) => break, // EOF
                Ok(Ok(_)) => {
                    let trimmed = line.trim_end().to_string();
                    self.buffer.push(trimmed.clone());
                    new_lines.push(trimmed);
                }
                Ok(Err(e)) => return Err(Error::Io(e)),
                Err(_) => break, // Timeout — no more data right now
            }
        }

        Ok(new_lines)
    }

    /// Read lines until one matches the given pattern, or timeout.
    pub async fn read_until(&mut self, pattern: &str, timeout_secs: u64) -> Result<Vec<String>> {
        let deadline = tokio::time::Instant::now()
            + std::time::Duration::from_secs(timeout_secs);

        let mut collected = Vec::new();

        loop {
            if tokio::time::Instant::now() >= deadline {
                return Err(Error::SerialTimeout {
                    pattern: pattern.to_string(),
                    timeout_secs,
                });
            }

            let mut line = String::new();
            let remaining = deadline - tokio::time::Instant::now();
            match tokio::time::timeout(remaining, self.reader.read_line(&mut line)).await {
                Ok(Ok(0)) => {
                    return Err(Error::SerialTimeout {
                        pattern: pattern.to_string(),
                        timeout_secs,
                    });
                }
                Ok(Ok(_)) => {
                    let trimmed = line.trim_end().to_string();
                    self.buffer.push(trimmed.clone());
                    collected.push(trimmed.clone());
                    if trimmed.contains(pattern) {
                        return Ok(collected);
                    }
                }
                Ok(Err(e)) => return Err(Error::Io(e)),
                Err(_) => {
                    return Err(Error::SerialTimeout {
                        pattern: pattern.to_string(),
                        timeout_secs,
                    });
                }
            }
        }
    }

    /// Filter the accumulated buffer for lines containing the pattern.
    pub fn filter(&self, pattern: &str) -> Vec<String> {
        self.buffer
            .iter()
            .filter(|line| line.contains(pattern))
            .cloned()
            .collect()
    }

    /// Write a command to the serial console (followed by a newline).
    pub async fn write_command(&mut self, command: &str) -> Result<()> {
        let msg = format!("{command}\n");
        self.writer
            .write_all(msg.as_bytes())
            .await
            .map_err(|e| Error::Io(e))?;
        self.writer.flush().await.map_err(|e| Error::Io(e))?;
        Ok(())
    }
}
