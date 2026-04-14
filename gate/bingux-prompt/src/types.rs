use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A request from bingux-gated to prompt the user for a permission decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptRequest {
    pub id: u64,
    pub package_name: String,
    pub package_icon: Option<PathBuf>,
    pub resource_type: ResourceType,
    pub resource_detail: String,
    pub is_dangerous: bool,
    pub timestamp: u64,
}

/// The kind of resource a sandboxed application is requesting access to.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ResourceType {
    FileRead,
    FileWrite,
    FileList,
    NetworkOutbound,
    NetworkListen,
    DeviceGpu,
    DeviceAudio,
    DeviceCamera,
    DeviceInput,
    Display,
    Clipboard,
    Notifications,
    ProcessExec,
    ProcessPtrace,
    DbusSession,
    DbusSystem,
    Mount,
}

/// The user's response to a permission prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PromptResponse {
    Deny,
    AllowOnce,
    AlwaysAllow,
}

impl ResourceType {
    /// Human-readable description for the prompt dialog.
    pub fn description(&self) -> &'static str {
        match self {
            Self::FileRead => "Read files",
            Self::FileWrite => "Write files",
            Self::FileList => "List directory contents",
            Self::NetworkOutbound => "Make outbound network connections",
            Self::NetworkListen => "Listen for incoming network connections",
            Self::DeviceGpu => "Access the GPU",
            Self::DeviceAudio => "Access audio devices",
            Self::DeviceCamera => "Access the camera",
            Self::DeviceInput => "Access input devices",
            Self::Display => "Access the display server",
            Self::Clipboard => "Access the clipboard",
            Self::Notifications => "Send desktop notifications",
            Self::ProcessExec => "Execute other programs",
            Self::ProcessPtrace => "Trace or debug other processes",
            Self::DbusSession => "Access the session D-Bus",
            Self::DbusSystem => "Access the system D-Bus",
            Self::Mount => "Mount filesystems",
        }
    }

    /// Freedesktop icon name for this resource type.
    pub fn icon_name(&self) -> &'static str {
        match self {
            Self::FileRead => "document-open",
            Self::FileWrite => "document-save",
            Self::FileList => "folder-open",
            Self::NetworkOutbound => "network-transmit",
            Self::NetworkListen => "network-receive",
            Self::DeviceGpu => "video-display",
            Self::DeviceAudio => "audio-card",
            Self::DeviceCamera => "camera-video",
            Self::DeviceInput => "input-keyboard",
            Self::Display => "video-display",
            Self::Clipboard => "edit-paste",
            Self::Notifications => "dialog-information",
            Self::ProcessExec => "system-run",
            Self::ProcessPtrace => "utilities-system-monitor",
            Self::DbusSession => "network-workgroup",
            Self::DbusSystem => "network-server",
            Self::Mount => "drive-harddisk",
        }
    }
}

impl PromptRequest {
    /// Format the prompt message for display.
    pub fn format_message(&self) -> String {
        format!(
            "{} wants to: {} ({})",
            self.package_name,
            self.resource_type.description(),
            self.resource_detail,
        )
    }

    /// Available responses — dangerous requests cannot be permanently allowed.
    pub fn available_responses(&self) -> Vec<PromptResponse> {
        if self.is_dangerous {
            vec![PromptResponse::Deny, PromptResponse::AllowOnce]
        } else {
            vec![
                PromptResponse::Deny,
                PromptResponse::AllowOnce,
                PromptResponse::AlwaysAllow,
            ]
        }
    }
}
