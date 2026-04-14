use serde::{Deserialize, Serialize};

/// All permission types that the sandbox can gate.
///
/// When a sandboxed process triggers a sensitive syscall, the seccomp-unotify
/// handler maps it to one of these categories before asking the permission
/// daemon whether to allow or deny the operation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PermissionCategory {
    // ── File access (decoded by path at runtime) ───────────────
    /// Generic file access — the actual path is resolved by bingux-gated
    /// at runtime from the seccomp notification's fd/path arguments.
    FileAccess,

    // ── Hardware ───────────────────────────────────────────────
    Gpu,
    Audio,
    Camera,
    Input,
    Usb,
    Bluetooth,

    // ── Network ───────────────────────────────────────────────
    NetOutbound,
    NetListen,
    NetPort(u16),

    // ── Display / desktop ─────────────────────────────────────
    Display,
    Notifications,
    Clipboard,
    Screenshot,

    // ── IPC ───────────────────────────────────────────────────
    DbusSession,
    DbusSystem,
    ProcessExec,
    ProcessPtrace,
    Keyring,

    // ── Dangerous (never "Always Allow") ──────────────────────
    Root,
    KernelModule,
    RawNet,
}

impl PermissionCategory {
    /// Returns `true` for categories that are too dangerous to receive
    /// permanent grants — the user must approve every single invocation.
    pub fn is_dangerous(&self) -> bool {
        matches!(
            self,
            PermissionCategory::Root
                | PermissionCategory::KernelModule
                | PermissionCategory::RawNet
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dangerous_categories() {
        assert!(PermissionCategory::Root.is_dangerous());
        assert!(PermissionCategory::KernelModule.is_dangerous());
        assert!(PermissionCategory::RawNet.is_dangerous());
    }

    #[test]
    fn safe_categories_not_dangerous() {
        let safe = [
            PermissionCategory::FileAccess,
            PermissionCategory::Gpu,
            PermissionCategory::Audio,
            PermissionCategory::Camera,
            PermissionCategory::Input,
            PermissionCategory::Usb,
            PermissionCategory::Bluetooth,
            PermissionCategory::NetOutbound,
            PermissionCategory::NetListen,
            PermissionCategory::NetPort(8080),
            PermissionCategory::Display,
            PermissionCategory::Notifications,
            PermissionCategory::Clipboard,
            PermissionCategory::Screenshot,
            PermissionCategory::DbusSession,
            PermissionCategory::DbusSystem,
            PermissionCategory::ProcessExec,
            PermissionCategory::ProcessPtrace,
            PermissionCategory::Keyring,
        ];
        for cat in &safe {
            assert!(!cat.is_dangerous(), "{cat:?} should not be dangerous");
        }
    }
}
