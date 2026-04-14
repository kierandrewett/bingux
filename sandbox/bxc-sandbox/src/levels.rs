use serde::{Deserialize, Serialize};

/// The four sandbox levels, from completely unconfined to fully locked down.
///
/// Each level determines what namespaces are created and whether seccomp
/// filtering is applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SandboxLevel {
    /// System-critical packages — no sandbox at all.
    /// Used for init, kernel modules, and similar low-level components.
    None,

    /// Mount namespace only, no seccomp filtering.
    /// The process sees an isolated filesystem but all syscalls are allowed.
    /// Suitable for toolchain / build-time packages.
    Minimal,

    /// Full sandbox: mount namespace + seccomp USER_NOTIF for sensitive
    /// syscalls. The permission daemon prompts the user on first access.
    Standard,

    /// Full sandbox plus PID and network namespace isolation.
    /// Additional syscalls are notified (clone, fork, accept).
    /// For untrusted or internet-facing packages.
    Strict,
}

impl SandboxLevel {
    /// Returns `true` if this level applies seccomp filtering.
    pub fn has_seccomp(&self) -> bool {
        matches!(self, SandboxLevel::Standard | SandboxLevel::Strict)
    }

    /// Returns `true` if this level creates a mount namespace.
    pub fn has_mount_ns(&self) -> bool {
        !matches!(self, SandboxLevel::None)
    }

    /// Returns `true` if this level creates PID and network namespaces.
    pub fn has_pid_net_ns(&self) -> bool {
        matches!(self, SandboxLevel::Strict)
    }
}
