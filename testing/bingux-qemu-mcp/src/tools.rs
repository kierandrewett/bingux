use std::path::PathBuf;

use serde_json::{json, Value};

use crate::config::LaunchConfig;
use crate::error::{Error, Result};
use crate::mcp::ServerState;
use crate::qemu::QemuInstance;
use crate::qmp::QmpClient;
use crate::screenshot;
use crate::serial::SerialReader;

/// An MCP tool definition.
pub struct ToolDefinition {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
}

/// Return all available MCP tool definitions.
pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "bingux_qemu_boot",
            description: "Boot a Bingux QEMU virtual machine from a disk image.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "image": {
                        "type": "string",
                        "description": "Path to the disk image (qcow2 or raw)"
                    },
                    "memory": {
                        "type": "string",
                        "description": "Memory allocation (e.g. '2G', '4096M')",
                        "default": "2G"
                    },
                    "cpus": {
                        "type": "integer",
                        "description": "Number of virtual CPUs",
                        "default": 2
                    },
                    "kvm": {
                        "type": "boolean",
                        "description": "Enable KVM hardware acceleration",
                        "default": true
                    },
                    "serial_only": {
                        "type": "boolean",
                        "description": "Run headless with serial-only console",
                        "default": false
                    },
                    "kernel": {
                        "type": "string",
                        "description": "Path to kernel image for direct boot (bypasses disk)"
                    },
                    "initrd": {
                        "type": "string",
                        "description": "Path to initrd/initramfs image"
                    },
                    "append": {
                        "type": "string",
                        "description": "Kernel command-line arguments"
                    },
                    "virtio_gpu": {
                        "type": "boolean",
                        "description": "Enable virtio-GPU for graphical compositor",
                        "default": false
                    },
                    "vga": {
                        "type": "boolean",
                        "description": "Enable VGA (std) alongside virtio-GPU for VT support",
                        "default": false
                    },
                    "extra_args": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Extra QEMU arguments"
                    }
                },
                "required": []
            }),
        },
        ToolDefinition {
            name: "bingux_qemu_screenshot",
            description: "Capture a screenshot of the VM's framebuffer. Returns a base64-encoded PNG image.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "vm_id": {
                        "type": "string",
                        "description": "VM identifier (from bingux_qemu_boot). If omitted, uses the most recent VM."
                    },
                    "device": {
                        "type": "string",
                        "description": "GPU device to capture from (e.g. 'gpu1' for virtio-gpu). If omitted, captures the default display."
                    }
                }
            }),
        },
        ToolDefinition {
            name: "bingux_qemu_serial_read",
            description: "Read lines from the VM's serial console. Can optionally wait for a pattern or filter buffered output.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "vm_id": {
                        "type": "string",
                        "description": "VM identifier. If omitted, uses the most recent VM."
                    },
                    "count": {
                        "type": "integer",
                        "description": "Maximum number of lines to read"
                    },
                    "wait_for": {
                        "type": "string",
                        "description": "Wait until a line matching this substring appears"
                    },
                    "timeout": {
                        "type": "integer",
                        "description": "Timeout in seconds when using wait_for (default: 30)",
                        "default": 30
                    },
                    "filter": {
                        "type": "string",
                        "description": "Filter buffered lines containing this substring"
                    }
                }
            }),
        },
        ToolDefinition {
            name: "bingux_qemu_type",
            description: "Send keystrokes to the VM. Supports individual keys or key combinations.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "vm_id": {
                        "type": "string",
                        "description": "VM identifier"
                    },
                    "keys": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Keys to press, e.g. ['ctrl', 'alt', 'delete'] or ['a', 'b', 'c']. Uses QMP key names."
                    },
                    "text": {
                        "type": "string",
                        "description": "Text string to type character by character (alternative to keys)"
                    }
                }
            }),
        },
        ToolDefinition {
            name: "bingux_qemu_mouse",
            description: "Move or click the mouse in the VM.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "vm_id": {
                        "type": "string",
                        "description": "VM identifier"
                    },
                    "x": {
                        "type": "integer",
                        "description": "Absolute X coordinate"
                    },
                    "y": {
                        "type": "integer",
                        "description": "Absolute Y coordinate"
                    },
                    "click": {
                        "type": "string",
                        "enum": ["left", "right", "middle"],
                        "description": "Mouse button to click after moving"
                    }
                }
            }),
        },
        ToolDefinition {
            name: "bingux_qemu_shell",
            description: "Execute a shell command on the VM via the serial console. Sends the command and reads output until the next prompt.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "vm_id": {
                        "type": "string",
                        "description": "VM identifier"
                    },
                    "command": {
                        "type": "string",
                        "description": "Shell command to execute"
                    },
                    "timeout": {
                        "type": "integer",
                        "description": "Timeout in seconds (default: 30)",
                        "default": 30
                    }
                },
                "required": ["command"]
            }),
        },
        ToolDefinition {
            name: "bingux_qemu_snapshot",
            description: "Save or restore a VM snapshot.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "vm_id": {
                        "type": "string",
                        "description": "VM identifier"
                    },
                    "action": {
                        "type": "string",
                        "enum": ["save", "load"],
                        "description": "Whether to save or load a snapshot"
                    },
                    "name": {
                        "type": "string",
                        "description": "Snapshot name"
                    }
                },
                "required": ["action", "name"]
            }),
        },
        ToolDefinition {
            name: "bingux_qemu_stop",
            description: "Stop and clean up a running VM.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "vm_id": {
                        "type": "string",
                        "description": "VM identifier"
                    },
                    "force": {
                        "type": "boolean",
                        "description": "Force-kill the VM immediately",
                        "default": false
                    }
                }
            }),
        },
    ]
}

/// Resolve a VM ID from args, defaulting to the most recently launched VM.
fn resolve_vm_id(args: &Value, state: &ServerState) -> Result<String> {
    if let Some(id) = args.get("vm_id").and_then(|v| v.as_str()) {
        Ok(id.to_string())
    } else if let Some(id) = state.last_vm_id.as_ref() {
        Ok(id.clone())
    } else {
        Err(Error::InvalidArguments(
            "no vm_id specified and no VM is running".into(),
        ))
    }
}

/// Get a QMP client for the given VM.
async fn get_qmp(state: &ServerState, vm_id: &str) -> Result<QmpClient> {
    let instance = state
        .instances
        .get(vm_id)
        .ok_or_else(|| Error::VmNotFound(vm_id.to_string()))?;
    QmpClient::connect(&instance.qmp_socket).await
}

/// Handle an MCP tool call by name.
pub async fn handle_tool_call(
    name: &str,
    args: Value,
    state: &mut ServerState,
) -> Result<Value> {
    match name {
        "bingux_qemu_boot" => handle_boot(args, state).await,
        "bingux_qemu_screenshot" => handle_screenshot(args, state).await,
        "bingux_qemu_serial_read" => handle_serial_read(args, state).await,
        "bingux_qemu_type" => handle_type(args, state).await,
        "bingux_qemu_mouse" => handle_mouse(args, state).await,
        "bingux_qemu_shell" => handle_shell(args, state).await,
        "bingux_qemu_snapshot" => handle_snapshot(args, state).await,
        "bingux_qemu_stop" => handle_stop(args, state).await,
        _ => Err(Error::InvalidArguments(format!("unknown tool: {name}"))),
    }
}

async fn handle_boot(args: Value, state: &mut ServerState) -> Result<Value> {
    let image = args
        .get("image")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let config = LaunchConfig {
        image: image.into(),
        memory: args
            .get("memory")
            .and_then(|v| v.as_str())
            .unwrap_or("2G")
            .to_string(),
        cpus: args
            .get("cpus")
            .and_then(|v| v.as_u64())
            .unwrap_or(2) as u32,
        kvm: args.get("kvm").and_then(|v| v.as_bool()).unwrap_or(true),
        serial_only: args
            .get("serial_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        kernel: args.get("kernel").and_then(|v| v.as_str()).map(PathBuf::from),
        initrd: args.get("initrd").and_then(|v| v.as_str()).map(PathBuf::from),
        append: args.get("append").and_then(|v| v.as_str()).map(String::from),
        virtio_gpu: args.get("virtio_gpu").and_then(|v| v.as_bool()).unwrap_or(false),
        vga: args.get("vga").and_then(|v| v.as_bool()).unwrap_or(false),
        extra_args: args.get("extra_args")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default(),
    };

    let instance = QemuInstance::launch(config).await?;
    let vm_id = instance.vm_id.clone();

    // Wait a moment for QEMU to start and create sockets
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    // Connect serial reader
    let serial = SerialReader::connect(&instance.serial_socket).await?;

    state.last_vm_id = Some(vm_id.clone());
    state.instances.insert(vm_id.clone(), instance);
    state.serial_readers.insert(vm_id.clone(), serial);

    Ok(json!({
        "vm_id": vm_id,
        "status": "running",
        "message": "VM launched successfully"
    }))
}

async fn handle_screenshot(args: Value, state: &mut ServerState) -> Result<Value> {
    let vm_id = resolve_vm_id(&args, state)?;
    let qmp = get_qmp(state, &vm_id).await?;
    let device = args.get("device").and_then(|v| v.as_str());
    let result = screenshot::capture_screenshot_device(&qmp, device).await?;

    Ok(json!({
        "vm_id": vm_id,
        "width": result.width,
        "height": result.height,
        "image": {
            "type": "image",
            "data": result.png_base64,
            "mimeType": "image/png"
        }
    }))
}

async fn handle_serial_read(args: Value, state: &mut ServerState) -> Result<Value> {
    let vm_id = resolve_vm_id(&args, state)?;

    let reader = state
        .serial_readers
        .get_mut(&vm_id)
        .ok_or_else(|| Error::VmNotFound(vm_id.clone()))?;

    // Filter mode: search buffered lines
    if let Some(filter) = args.get("filter").and_then(|v| v.as_str()) {
        let lines = reader.filter(filter);
        return Ok(json!({
            "vm_id": vm_id,
            "lines": lines,
            "mode": "filter"
        }));
    }

    // Wait-for mode: block until pattern appears
    if let Some(pattern) = args.get("wait_for").and_then(|v| v.as_str()) {
        let timeout = args
            .get("timeout")
            .and_then(|v| v.as_u64())
            .unwrap_or(30);
        let lines = reader.read_until(pattern, timeout).await?;
        return Ok(json!({
            "vm_id": vm_id,
            "lines": lines,
            "mode": "wait_for"
        }));
    }

    // Default: read available lines
    let count = args.get("count").and_then(|v| v.as_u64()).map(|n| n as usize);
    let lines = reader.read_lines(count).await?;

    Ok(json!({
        "vm_id": vm_id,
        "lines": lines,
        "mode": "read"
    }))
}

async fn handle_type(args: Value, state: &mut ServerState) -> Result<Value> {
    let vm_id = resolve_vm_id(&args, state)?;
    let qmp = get_qmp(state, &vm_id).await?;

    if let Some(keys) = args.get("keys").and_then(|v| v.as_array()) {
        let key_strs: Vec<&str> = keys
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        qmp.send_key(&key_strs).await?;
        return Ok(json!({
            "vm_id": vm_id,
            "sent": "keys",
            "keys": keys
        }));
    }

    if let Some(text) = args.get("text").and_then(|v| v.as_str()) {
        // Type each character as an individual keypress
        for ch in text.chars() {
            let key_name = char_to_qcode(ch);
            qmp.send_key(&[key_name]).await?;
            // Small delay between keystrokes
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        return Ok(json!({
            "vm_id": vm_id,
            "sent": "text",
            "length": text.len()
        }));
    }

    Err(Error::InvalidArguments(
        "either 'keys' or 'text' must be provided".into(),
    ))
}

async fn handle_mouse(args: Value, state: &mut ServerState) -> Result<Value> {
    let vm_id = resolve_vm_id(&args, state)?;
    let qmp = get_qmp(state, &vm_id).await?;

    let x = args.get("x").and_then(|v| v.as_i64());
    let y = args.get("y").and_then(|v| v.as_i64());

    if let (Some(x), Some(y)) = (x, y) {
        qmp.send_mouse_move(x as i32, y as i32).await?;
    }

    if let Some(click) = args.get("click").and_then(|v| v.as_str()) {
        qmp.send_mouse_click(click).await?;
    }

    Ok(json!({
        "vm_id": vm_id,
        "action": "mouse",
        "x": x,
        "y": y,
        "click": args.get("click")
    }))
}

async fn handle_shell(args: Value, state: &mut ServerState) -> Result<Value> {
    let vm_id = resolve_vm_id(&args, state)?;
    let command = args
        .get("command")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::InvalidArguments("missing required field: command".into()))?;
    let timeout = args
        .get("timeout")
        .and_then(|v| v.as_u64())
        .unwrap_or(30);

    let reader = state
        .serial_readers
        .get_mut(&vm_id)
        .ok_or_else(|| Error::VmNotFound(vm_id.clone()))?;

    // Use a sentinel to detect command completion
    let sentinel = format!("__BINGUX_DONE_{}__", std::process::id());
    let full_command = format!("{command}; echo {sentinel}");

    reader.write_command(&full_command).await?;
    let lines = reader.read_until(&sentinel, timeout).await?;

    // Remove the sentinel line and the echoed command from output
    let output: Vec<String> = lines
        .into_iter()
        .filter(|l| !l.contains(&sentinel) && !l.contains(&full_command))
        .collect();

    Ok(json!({
        "vm_id": vm_id,
        "command": command,
        "output": output
    }))
}

async fn handle_snapshot(args: Value, state: &mut ServerState) -> Result<Value> {
    let vm_id = resolve_vm_id(&args, state)?;
    let action = args
        .get("action")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::InvalidArguments("missing required field: action".into()))?;
    let name = args
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::InvalidArguments("missing required field: name".into()))?;

    let qmp = get_qmp(state, &vm_id).await?;

    match action {
        "save" => {
            qmp.savevm(name).await?;
            Ok(json!({
                "vm_id": vm_id,
                "action": "save",
                "name": name,
                "status": "saved"
            }))
        }
        "load" => {
            qmp.loadvm(name).await?;
            Ok(json!({
                "vm_id": vm_id,
                "action": "load",
                "name": name,
                "status": "loaded"
            }))
        }
        _ => Err(Error::InvalidArguments(format!(
            "action must be 'save' or 'load', got: {action}"
        ))),
    }
}

async fn handle_stop(args: Value, state: &mut ServerState) -> Result<Value> {
    let vm_id = resolve_vm_id(&args, state)?;
    let force = args.get("force").and_then(|v| v.as_bool()).unwrap_or(false);

    // Try to send QMP quit first for graceful shutdown
    if !force {
        if let Ok(qmp) = get_qmp(state, &vm_id).await {
            let _ = qmp.execute("quit", json!({})).await;
        }
    }

    // Remove and stop the instance
    if let Some(mut instance) = state.instances.remove(&vm_id) {
        instance.stop(force).await?;
    }
    state.serial_readers.remove(&vm_id);

    if state.last_vm_id.as_deref() == Some(&vm_id) {
        state.last_vm_id = None;
    }

    Ok(json!({
        "vm_id": vm_id,
        "status": "stopped"
    }))
}

/// Map a character to its QMP qcode name.
fn char_to_qcode(ch: char) -> &'static str {
    match ch {
        'a' => "a",
        'b' => "b",
        'c' => "c",
        'd' => "d",
        'e' => "e",
        'f' => "f",
        'g' => "g",
        'h' => "h",
        'i' => "i",
        'j' => "j",
        'k' => "k",
        'l' => "l",
        'm' => "m",
        'n' => "n",
        'o' => "o",
        'p' => "p",
        'q' => "q",
        'r' => "r",
        's' => "s",
        't' => "t",
        'u' => "u",
        'v' => "v",
        'w' => "w",
        'x' => "x",
        'y' => "y",
        'z' => "z",
        '0' => "0",
        '1' => "1",
        '2' => "2",
        '3' => "3",
        '4' => "4",
        '5' => "5",
        '6' => "6",
        '7' => "7",
        '8' => "8",
        '9' => "9",
        ' ' => "spc",
        '\n' => "ret",
        '\t' => "tab",
        '-' => "minus",
        '=' => "equal",
        '[' => "bracket_left",
        ']' => "bracket_right",
        '\\' => "backslash",
        ';' => "semicolon",
        '\'' => "apostrophe",
        ',' => "comma",
        '.' => "dot",
        '/' => "slash",
        '`' => "grave_accent",
        _ => "spc", // fallback for unmapped characters
    }
}
