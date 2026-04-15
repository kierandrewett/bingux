use std::path::{Path, PathBuf};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use crate::error::{Error, Result};

/// Client for the QEMU Machine Protocol (QMP).
///
/// QMP is a JSON-based protocol for controlling QEMU instances. It runs over
/// a Unix domain socket and uses a simple request/response pattern after an
/// initial capabilities negotiation.
pub struct QmpClient {
    socket_path: PathBuf,
}

impl QmpClient {
    /// Connect to a QMP socket and perform capabilities negotiation.
    pub async fn connect(socket_path: &Path) -> Result<Self> {
        // Verify we can connect and negotiate
        let stream = UnixStream::connect(socket_path).await.map_err(|e| {
            Error::QmpConnectionFailed {
                path: socket_path.to_path_buf(),
                source: e,
            }
        })?;

        let mut reader = BufReader::new(stream);

        // Read the greeting
        let mut greeting = String::new();
        reader.read_line(&mut greeting).await.map_err(|e| {
            Error::QmpConnectionFailed {
                path: socket_path.to_path_buf(),
                source: e,
            }
        })?;
        tracing::debug!(greeting = %greeting.trim(), "QMP greeting received");

        // Send qmp_capabilities to exit negotiation mode
        let stream = reader.into_inner();
        let (read_half, mut write_half) = stream.into_split();
        let negotiate = serde_json::json!({"execute": "qmp_capabilities"});
        let mut msg = serde_json::to_vec(&negotiate)?;
        msg.push(b'\n');
        write_half.write_all(&msg).await?;

        // Read the response
        let mut reader = BufReader::new(read_half);
        let mut response = String::new();
        reader.read_line(&mut response).await?;
        tracing::debug!(response = %response.trim(), "QMP capabilities negotiated");

        Ok(Self {
            socket_path: socket_path.to_path_buf(),
        })
    }

    /// Execute a QMP command and return the result.
    ///
    /// Each call opens a fresh connection, negotiates capabilities, then sends
    /// the command. This avoids dealing with interleaved async events on a
    /// persistent connection.
    pub async fn execute(
        &self,
        command: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let stream =
            UnixStream::connect(&self.socket_path)
                .await
                .map_err(|e| Error::QmpConnectionFailed {
                    path: self.socket_path.clone(),
                    source: e,
                })?;

        let (read_half, mut write_half) = stream.into_split();
        let mut reader = BufReader::new(read_half);

        // Read greeting
        let mut line = String::new();
        reader.read_line(&mut line).await?;

        // Negotiate capabilities
        let negotiate = serde_json::json!({"execute": "qmp_capabilities"});
        let mut msg = serde_json::to_vec(&negotiate)?;
        msg.push(b'\n');
        write_half.write_all(&msg).await?;

        line.clear();
        reader.read_line(&mut line).await?;

        // Send the actual command
        let request = if args.is_null() || args == serde_json::json!({}) {
            serde_json::json!({"execute": command})
        } else {
            serde_json::json!({"execute": command, "arguments": args})
        };

        tracing::debug!(command = %command, "sending QMP command");
        let mut msg = serde_json::to_vec(&request)?;
        msg.push(b'\n');
        write_half.write_all(&msg).await?;

        // Read response, skipping any async event lines
        loop {
            line.clear();
            reader.read_line(&mut line).await?;
            let parsed: serde_json::Value = serde_json::from_str(line.trim())?;

            if parsed.get("return").is_some() {
                return Ok(parsed["return"].clone());
            }
            if let Some(err) = parsed.get("error") {
                return Err(Error::QmpCommandFailed {
                    command: command.to_string(),
                    detail: err.to_string(),
                });
            }
            // Skip event messages and keep reading
            if parsed.get("event").is_some() {
                continue;
            }
            return Err(Error::QmpProtocol(format!(
                "unexpected QMP response: {}",
                line.trim()
            )));
        }
    }

    /// Capture a screenshot to a PPM file via the `screendump` command.
    /// If `device` is provided, captures from that specific display device (e.g. "gpu1").
    pub async fn screendump(&self, output_path: &Path, device: Option<&str>) -> Result<()> {
        let mut args = serde_json::json!({"filename": output_path.to_str().unwrap()});
        if let Some(dev) = device {
            args["device"] = serde_json::Value::String(dev.to_string());
        }
        self.execute("screendump", args).await?;
        Ok(())
    }

    /// Send key press events.
    ///
    /// Keys use QMP key names, e.g. `["ctrl", "alt", "delete"]` or `["a"]`.
    pub async fn send_key(&self, keys: &[&str]) -> Result<()> {
        let key_list: Vec<serde_json::Value> = keys
            .iter()
            .map(|k| serde_json::json!({"type": "qcode", "data": k}))
            .collect();

        self.execute("send-key", serde_json::json!({"keys": key_list}))
            .await?;
        Ok(())
    }

    /// Move the mouse to absolute coordinates.
    pub async fn send_mouse_move(&self, x: i32, y: i32) -> Result<()> {
        self.execute(
            "input-send-event",
            serde_json::json!({
                "events": [
                    {"type": "abs", "data": {"axis": "x", "value": x}},
                    {"type": "abs", "data": {"axis": "y", "value": y}},
                ]
            }),
        )
        .await?;
        Ok(())
    }

    /// Click a mouse button. Button is "left", "right", or "middle".
    pub async fn send_mouse_click(&self, button: &str) -> Result<()> {
        let btn_val = match button {
            "left" => 0,
            "middle" => 1,
            "right" => 2,
            other => {
                return Err(Error::InvalidArguments(format!(
                    "unknown mouse button: {other}"
                )))
            }
        };

        // Press
        self.execute(
            "input-send-event",
            serde_json::json!({
                "events": [
                    {"type": "btn", "data": {"down": true, "button": btn_val}},
                ]
            }),
        )
        .await?;

        // Release
        self.execute(
            "input-send-event",
            serde_json::json!({
                "events": [
                    {"type": "btn", "data": {"down": false, "button": btn_val}},
                ]
            }),
        )
        .await?;

        Ok(())
    }

    /// Save a VM snapshot with the given name.
    pub async fn savevm(&self, name: &str) -> Result<()> {
        self.execute(
            "human-monitor-command",
            serde_json::json!({"command-line": format!("savevm {name}")}),
        )
        .await?;
        Ok(())
    }

    /// Load a VM snapshot by name.
    pub async fn loadvm(&self, name: &str) -> Result<()> {
        self.execute(
            "human-monitor-command",
            serde_json::json!({"command-line": format!("loadvm {name}")}),
        )
        .await?;
        Ok(())
    }
}
