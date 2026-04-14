use std::collections::HashMap;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{debug, error, info};

use crate::error::{Error, Result};
use crate::qemu::QemuInstance;
use crate::serial::SerialReader;
use crate::tools;

/// State shared across tool invocations.
pub struct ServerState {
    /// Running VM instances, keyed by vm_id.
    pub instances: HashMap<String, QemuInstance>,
    /// Serial console readers, keyed by vm_id.
    pub serial_readers: HashMap<String, SerialReader>,
    /// ID of the most recently launched VM (for convenience when only one is
    /// running).
    pub last_vm_id: Option<String>,
}

/// MCP protocol server using JSON-RPC over stdio.
pub struct McpServer {
    state: ServerState,
}

impl McpServer {
    pub fn new() -> Self {
        Self {
            state: ServerState {
                instances: HashMap::new(),
                serial_readers: HashMap::new(),
                last_vm_id: None,
            },
        }
    }

    /// Run the MCP server, reading JSON-RPC messages from stdin and writing
    /// responses to stdout.
    pub async fn run(&mut self) -> Result<()> {
        let stdin = tokio::io::stdin();
        let mut stdout = tokio::io::stdout();
        let mut reader = BufReader::new(stdin);

        info!("MCP server ready, reading from stdin");

        loop {
            let mut line = String::new();
            match reader.read_line(&mut line).await {
                Ok(0) => {
                    info!("stdin closed, shutting down");
                    self.shutdown().await;
                    break;
                }
                Ok(_) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }

                    debug!(message = %trimmed, "received message");

                    let msg: Value = match serde_json::from_str(trimmed) {
                        Ok(v) => v,
                        Err(e) => {
                            let err_response = json!({
                                "jsonrpc": "2.0",
                                "id": null,
                                "error": {
                                    "code": -32700,
                                    "message": format!("Parse error: {e}")
                                }
                            });
                            let mut out = serde_json::to_vec(&err_response)?;
                            out.push(b'\n');
                            stdout.write_all(&out).await?;
                            stdout.flush().await?;
                            continue;
                        }
                    };

                    let response = self.handle_message(msg).await;

                    match response {
                        Ok(Some(resp)) => {
                            let mut out = serde_json::to_vec(&resp)?;
                            out.push(b'\n');
                            stdout.write_all(&out).await?;
                            stdout.flush().await?;
                        }
                        Ok(None) => {
                            // Notification — no response needed
                        }
                        Err(e) => {
                            error!(error = %e, "handler error");
                        }
                    }
                }
                Err(e) => {
                    error!(error = %e, "stdin read error");
                    break;
                }
            }
        }

        Ok(())
    }

    /// Handle a single JSON-RPC message and return the response (if any).
    async fn handle_message(&mut self, msg: Value) -> Result<Option<Value>> {
        let id = msg.get("id").cloned();
        let method = msg
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        match method {
            "initialize" => {
                let result = json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "tools": {}
                    },
                    "serverInfo": {
                        "name": "bingux-qemu-mcp",
                        "version": "0.1.0"
                    }
                });
                Ok(Some(jsonrpc_response(id, result)))
            }

            "notifications/initialized" => {
                // Client acknowledgment — no response needed
                Ok(None)
            }

            "tools/list" => {
                let defs = tools::tool_definitions();
                let tool_list: Vec<Value> = defs
                    .iter()
                    .map(|t| {
                        json!({
                            "name": t.name,
                            "description": t.description,
                            "inputSchema": t.input_schema
                        })
                    })
                    .collect();

                Ok(Some(jsonrpc_response(id, json!({"tools": tool_list}))))
            }

            "tools/call" => {
                let params = msg.get("params").cloned().unwrap_or(json!({}));
                let tool_name = params
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let tool_args = params
                    .get("arguments")
                    .cloned()
                    .unwrap_or(json!({}));

                info!(tool = %tool_name, "executing tool");

                match tools::handle_tool_call(tool_name, tool_args, &mut self.state).await {
                    Ok(result) => {
                        let content = json!({
                            "content": [
                                {
                                    "type": "text",
                                    "text": serde_json::to_string_pretty(&result)
                                        .unwrap_or_else(|_| result.to_string())
                                }
                            ]
                        });
                        Ok(Some(jsonrpc_response(id, content)))
                    }
                    Err(e) => {
                        let content = json!({
                            "content": [
                                {
                                    "type": "text",
                                    "text": format!("Error: {e}")
                                }
                            ],
                            "isError": true
                        });
                        Ok(Some(jsonrpc_response(id, content)))
                    }
                }
            }

            "" => {
                // Missing method
                Err(Error::McpProtocol("missing method field".into()))
            }

            other => {
                let err = json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": -32601,
                        "message": format!("Method not found: {other}")
                    }
                });
                Ok(Some(err))
            }
        }
    }

    /// Shut down all running VMs on exit.
    async fn shutdown(&mut self) {
        let vm_ids: Vec<String> = self.state.instances.keys().cloned().collect();
        for vm_id in vm_ids {
            if let Some(mut instance) = self.state.instances.remove(&vm_id) {
                info!(vm_id = %vm_id, "shutting down VM on exit");
                let _ = instance.stop(true).await;
            }
        }
    }
}

/// Construct a JSON-RPC 2.0 success response.
fn jsonrpc_response(id: Option<Value>, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    })
}
