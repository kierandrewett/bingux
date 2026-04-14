use crate::levels::SandboxLevel;
use crate::syscalls;

/// A declarative seccomp profile describing how syscalls should be handled.
///
/// This is a data structure only — actual BPF filter generation happens
/// in the shim (`bxc-shim`) when the sandbox process starts. The profile
/// tells the shim which syscalls to allow, which to notify on (via
/// `SECCOMP_RET_USER_NOTIF`), and which to deny outright.
#[derive(Debug, Clone)]
pub struct SeccompProfile {
    /// The sandbox level this profile was built for.
    pub level: SandboxLevel,
    /// Syscall numbers that are always allowed without notification.
    pub allow_list: Vec<i64>,
    /// Syscall numbers that trigger `SECCOMP_RET_USER_NOTIF` — the
    /// permission daemon decides whether to allow or deny.
    pub notify_list: Vec<i64>,
    /// Syscall numbers that are always denied (`SECCOMP_RET_KILL_PROCESS`
    /// or `SECCOMP_RET_ERRNO`).
    pub deny_list: Vec<i64>,
}

impl SeccompProfile {
    /// Build a seccomp profile for the given sandbox level.
    pub fn for_level(level: SandboxLevel) -> Self {
        match level {
            SandboxLevel::None | SandboxLevel::Minimal => Self {
                level,
                allow_list: Vec::new(),
                notify_list: Vec::new(),
                deny_list: Vec::new(),
            },
            SandboxLevel::Standard => Self {
                level,
                allow_list: syscalls::safe_syscall_list(),
                notify_list: syscalls::standard_notify_list(),
                deny_list: syscalls::standard_deny_list(),
            },
            SandboxLevel::Strict => {
                let mut notify = syscalls::standard_notify_list();
                notify.extend(syscalls::strict_extra_notify_list());

                // In strict mode, remove clone/fork/vfork from allow list
                // since they are in the notify list.
                let extra_notify = syscalls::strict_extra_notify_list();
                let mut allow = syscalls::safe_syscall_list();
                allow.retain(|nr| !extra_notify.contains(nr));

                Self {
                    level,
                    allow_list: allow,
                    notify_list: notify,
                    deny_list: syscalls::standard_deny_list(),
                }
            }
        }
    }

    /// Returns `true` if this profile is empty (no filtering at all).
    pub fn is_empty(&self) -> bool {
        self.allow_list.is_empty() && self.notify_list.is_empty() && self.deny_list.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syscalls::*;

    #[test]
    fn none_produces_empty_profile() {
        let p = SeccompProfile::for_level(SandboxLevel::None);
        assert!(p.is_empty());
    }

    #[test]
    fn minimal_produces_empty_profile() {
        let p = SeccompProfile::for_level(SandboxLevel::Minimal);
        assert!(p.is_empty());
    }

    #[test]
    fn standard_has_openat_in_notify() {
        let p = SeccompProfile::for_level(SandboxLevel::Standard);
        assert!(p.notify_list.contains(&SYS_OPENAT));
    }

    #[test]
    fn standard_has_read_in_allow() {
        let p = SeccompProfile::for_level(SandboxLevel::Standard);
        assert!(p.allow_list.contains(&SYS_READ));
    }

    #[test]
    fn standard_denies_mount() {
        let p = SeccompProfile::for_level(SandboxLevel::Standard);
        assert!(p.deny_list.contains(&SYS_MOUNT));
        assert!(p.deny_list.contains(&SYS_UMOUNT2));
    }

    #[test]
    fn strict_notifies_more_than_standard() {
        let standard = SeccompProfile::for_level(SandboxLevel::Standard);
        let strict = SeccompProfile::for_level(SandboxLevel::Strict);
        assert!(strict.notify_list.len() > standard.notify_list.len());
    }

    #[test]
    fn strict_removes_fork_from_allow() {
        let p = SeccompProfile::for_level(SandboxLevel::Strict);
        assert!(!p.allow_list.contains(&SYS_FORK));
        assert!(!p.allow_list.contains(&SYS_VFORK));
        assert!(!p.allow_list.contains(&SYS_CLONE));
        // But they are in the notify list
        assert!(p.notify_list.contains(&SYS_FORK));
        assert!(p.notify_list.contains(&SYS_VFORK));
        assert!(p.notify_list.contains(&SYS_CLONE));
    }

    #[test]
    fn standard_is_not_empty() {
        let p = SeccompProfile::for_level(SandboxLevel::Standard);
        assert!(!p.is_empty());
    }
}
