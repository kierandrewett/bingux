use crate::categories::PermissionCategory;

/// Maps a Linux syscall number to the permission category it triggers.
#[derive(Debug, Clone)]
pub struct SyscallMapping {
    /// The syscall number (x86_64 ABI).
    pub syscall_nr: i64,
    /// The permission category this syscall falls under.
    pub category: PermissionCategory,
    /// A human-readable description shown in prompts.
    pub description: &'static str,
}

// ── x86_64 syscall numbers ──────────────────────────────────────────

pub const SYS_READ: i64 = 0;
pub const SYS_WRITE: i64 = 1;
pub const SYS_CLOSE: i64 = 3;
pub const SYS_FSTAT: i64 = 5;
pub const SYS_LSEEK: i64 = 8;
pub const SYS_MMAP: i64 = 9;
pub const SYS_MPROTECT: i64 = 10;
pub const SYS_MUNMAP: i64 = 11;
pub const SYS_BRK: i64 = 12;
pub const SYS_RT_SIGACTION: i64 = 13;
pub const SYS_RT_SIGPROCMASK: i64 = 14;
pub const SYS_IOCTL: i64 = 16;
pub const SYS_PREAD64: i64 = 17;
pub const SYS_PWRITE64: i64 = 18;
pub const SYS_READV: i64 = 19;
pub const SYS_WRITEV: i64 = 20;
pub const SYS_ACCESS: i64 = 21;
pub const SYS_PIPE: i64 = 22;
pub const SYS_SELECT: i64 = 23;
pub const SYS_SCHED_YIELD: i64 = 24;
pub const SYS_MREMAP: i64 = 25;
pub const SYS_MSYNC: i64 = 26;
pub const SYS_MADVISE: i64 = 28;
pub const SYS_SHMGET: i64 = 29;
pub const SYS_SHMAT: i64 = 30;
pub const SYS_SHMCTL: i64 = 31;
pub const SYS_DUP: i64 = 32;
pub const SYS_DUP2: i64 = 33;
pub const SYS_NANOSLEEP: i64 = 35;
pub const SYS_GETPID: i64 = 39;
pub const SYS_CONNECT: i64 = 42;
pub const SYS_ACCEPT: i64 = 43;
pub const SYS_BIND: i64 = 49;
pub const SYS_LISTEN: i64 = 50;
pub const SYS_FORK: i64 = 57;
pub const SYS_VFORK: i64 = 58;
pub const SYS_EXECVE: i64 = 59;
pub const SYS_EXIT: i64 = 60;
pub const SYS_WAIT4: i64 = 61;
pub const SYS_KILL: i64 = 62;
pub const SYS_UNAME: i64 = 63;
pub const SYS_FCNTL: i64 = 72;
pub const SYS_FLOCK: i64 = 73;
pub const SYS_FSYNC: i64 = 74;
pub const SYS_FDATASYNC: i64 = 75;
pub const SYS_FTRUNCATE: i64 = 77;
pub const SYS_GETDENTS: i64 = 78;
pub const SYS_GETCWD: i64 = 79;
pub const SYS_CHDIR: i64 = 80;
pub const SYS_FCHDIR: i64 = 81;
pub const SYS_RENAME: i64 = 82;
pub const SYS_MKDIR: i64 = 83;
pub const SYS_RMDIR: i64 = 84;
pub const SYS_UNLINK: i64 = 87;
pub const SYS_READLINK: i64 = 89;
pub const SYS_CHMOD: i64 = 90;
pub const SYS_CHOWN: i64 = 92;
pub const SYS_PTRACE: i64 = 101;
pub const SYS_ARCH_PRCTL: i64 = 158;
pub const SYS_MOUNT: i64 = 165;
pub const SYS_UMOUNT2: i64 = 166;
pub const SYS_FUTEX: i64 = 202;
pub const SYS_EPOLL_CREATE: i64 = 213;
pub const SYS_GETDENTS64: i64 = 217;
pub const SYS_SET_TID_ADDRESS: i64 = 218;
pub const SYS_CLOCK_GETTIME: i64 = 228;
pub const SYS_CLOCK_GETRES: i64 = 229;
pub const SYS_CLOCK_NANOSLEEP: i64 = 230;
pub const SYS_EXIT_GROUP: i64 = 231;
pub const SYS_EPOLL_WAIT: i64 = 232;
pub const SYS_EPOLL_CTL: i64 = 233;
pub const SYS_OPENAT: i64 = 257;
pub const SYS_SET_ROBUST_LIST: i64 = 273;
pub const SYS_GET_ROBUST_LIST: i64 = 274;
pub const SYS_CLONE: i64 = 56;
pub const SYS_POLL: i64 = 7;
pub const SYS_PPOLL: i64 = 271;
pub const SYS_PSELECT6: i64 = 270;
pub const SYS_TIMERFD_CREATE: i64 = 283;
pub const SYS_TIMERFD_SETTIME: i64 = 286;
pub const SYS_TIMERFD_GETTIME: i64 = 287;
pub const SYS_EVENTFD2: i64 = 290;
pub const SYS_SIGNALFD4: i64 = 289;
pub const SYS_GETRANDOM: i64 = 318;
pub const SYS_MEMFD_CREATE: i64 = 319;
pub const SYS_STATX: i64 = 332;
pub const SYS_EXECVEAT: i64 = 322;
pub const SYS_PIPE2: i64 = 293;
pub const SYS_DUP3: i64 = 292;
pub const SYS_GETUID: i64 = 102;
pub const SYS_GETGID: i64 = 104;
pub const SYS_GETEUID: i64 = 107;
pub const SYS_GETEGID: i64 = 108;
pub const SYS_GETTID: i64 = 186;
pub const SYS_GETPPID: i64 = 110;
pub const SYS_NEWFSTATAT: i64 = 262;
pub const SYS_PRCTL: i64 = 157;
pub const SYS_CLOSE_RANGE: i64 = 436;
pub const SYS_RSEQ: i64 = 334;
pub const SYS_MLOCK: i64 = 149;
pub const SYS_MUNLOCK: i64 = 150;
pub const SYS_SIGALTSTACK: i64 = 131;
pub const SYS_RT_SIGRETURN: i64 = 15;
pub const SYS_SCHED_GETAFFINITY: i64 = 204;
pub const SYS_SYSINFO: i64 = 99;

/// The set of syscalls that map to permission categories when intercepted.
///
/// Note: `ioctl` is intentionally excluded here — at the seccomp layer it
/// always fires, but the permission daemon decodes the ioctl request
/// number and fd to decide the category (GPU, audio, camera, etc.).
pub fn sensitive_syscall_mappings() -> Vec<SyscallMapping> {
    vec![
        SyscallMapping {
            syscall_nr: SYS_OPENAT,
            category: PermissionCategory::FileAccess,
            description: "Open a file (path decoded at runtime by gated)",
        },
        SyscallMapping {
            syscall_nr: SYS_CONNECT,
            category: PermissionCategory::NetOutbound,
            description: "Initiate an outbound network connection",
        },
        SyscallMapping {
            syscall_nr: SYS_BIND,
            category: PermissionCategory::NetListen,
            description: "Bind to a network address for listening",
        },
        SyscallMapping {
            syscall_nr: SYS_LISTEN,
            category: PermissionCategory::NetListen,
            description: "Start listening for incoming connections",
        },
        SyscallMapping {
            syscall_nr: SYS_MOUNT,
            category: PermissionCategory::Root,
            description: "Mount a filesystem",
        },
        SyscallMapping {
            syscall_nr: SYS_UMOUNT2,
            category: PermissionCategory::Root,
            description: "Unmount a filesystem",
        },
        SyscallMapping {
            syscall_nr: SYS_PTRACE,
            category: PermissionCategory::ProcessPtrace,
            description: "Attach to or trace another process",
        },
        SyscallMapping {
            syscall_nr: SYS_EXECVE,
            category: PermissionCategory::ProcessExec,
            description: "Execute a new program",
        },
        SyscallMapping {
            syscall_nr: SYS_EXECVEAT,
            category: PermissionCategory::ProcessExec,
            description: "Execute a new program (fd-relative)",
        },
    ]
}

/// Look up the permission category for a syscall number.
///
/// Returns `None` for syscalls that are not sensitive (i.e. always allowed
/// or always denied at the BPF level without prompting).
pub fn category_for_syscall(nr: i64) -> Option<&'static PermissionCategory> {
    // Use a simple match instead of iterating — this is called on every
    // seccomp notification so it should be fast.
    match nr {
        SYS_OPENAT => Some(&PermissionCategory::FileAccess),
        SYS_CONNECT => Some(&PermissionCategory::NetOutbound),
        SYS_BIND | SYS_LISTEN => Some(&PermissionCategory::NetListen),
        SYS_MOUNT | SYS_UMOUNT2 => Some(&PermissionCategory::Root),
        SYS_PTRACE => Some(&PermissionCategory::ProcessPtrace),
        SYS_EXECVE | SYS_EXECVEAT => Some(&PermissionCategory::ProcessExec),
        _ => None,
    }
}

/// All syscall numbers considered safe for unprivileged sandboxed processes.
///
/// These are always allowed in Standard and Strict profiles — they cover
/// basic I/O on already-opened fds, memory management, signals, timing,
/// and similar operations that cannot escape the sandbox.
pub fn safe_syscall_list() -> Vec<i64> {
    vec![
        SYS_READ,
        SYS_WRITE,
        SYS_CLOSE,
        SYS_FSTAT,
        SYS_LSEEK,
        SYS_MMAP,
        SYS_MPROTECT,
        SYS_MUNMAP,
        SYS_BRK,
        SYS_RT_SIGACTION,
        SYS_RT_SIGPROCMASK,
        SYS_RT_SIGRETURN,
        SYS_PREAD64,
        SYS_PWRITE64,
        SYS_READV,
        SYS_WRITEV,
        SYS_ACCESS,
        SYS_PIPE,
        SYS_PIPE2,
        SYS_SELECT,
        SYS_SCHED_YIELD,
        SYS_MREMAP,
        SYS_MSYNC,
        SYS_MADVISE,
        SYS_SHMGET,
        SYS_SHMAT,
        SYS_SHMCTL,
        SYS_DUP,
        SYS_DUP2,
        SYS_DUP3,
        SYS_NANOSLEEP,
        SYS_GETPID,
        SYS_FORK,
        SYS_VFORK,
        SYS_CLONE,
        SYS_WAIT4,
        SYS_KILL,
        SYS_UNAME,
        SYS_FCNTL,
        SYS_FLOCK,
        SYS_FSYNC,
        SYS_FDATASYNC,
        SYS_FTRUNCATE,
        SYS_GETDENTS,
        SYS_GETDENTS64,
        SYS_GETCWD,
        SYS_CHDIR,
        SYS_FCHDIR,
        SYS_RENAME,
        SYS_MKDIR,
        SYS_RMDIR,
        SYS_UNLINK,
        SYS_READLINK,
        SYS_CHMOD,
        SYS_CHOWN,
        SYS_CLOCK_GETTIME,
        SYS_CLOCK_GETRES,
        SYS_CLOCK_NANOSLEEP,
        SYS_EXIT,
        SYS_EXIT_GROUP,
        SYS_ARCH_PRCTL,
        SYS_FUTEX,
        SYS_SET_TID_ADDRESS,
        SYS_SET_ROBUST_LIST,
        SYS_GET_ROBUST_LIST,
        SYS_EPOLL_CREATE,
        SYS_EPOLL_CTL,
        SYS_EPOLL_WAIT,
        SYS_POLL,
        SYS_PPOLL,
        SYS_PSELECT6,
        SYS_TIMERFD_CREATE,
        SYS_TIMERFD_SETTIME,
        SYS_TIMERFD_GETTIME,
        SYS_EVENTFD2,
        SYS_SIGNALFD4,
        SYS_GETRANDOM,
        SYS_MEMFD_CREATE,
        SYS_STATX,
        SYS_NEWFSTATAT,
        SYS_GETUID,
        SYS_GETGID,
        SYS_GETEUID,
        SYS_GETEGID,
        SYS_GETTID,
        SYS_GETPPID,
        SYS_PRCTL,
        SYS_CLOSE_RANGE,
        SYS_RSEQ,
        SYS_MLOCK,
        SYS_MUNLOCK,
        SYS_SIGALTSTACK,
        SYS_SCHED_GETAFFINITY,
        SYS_SYSINFO,
    ]
}

/// Syscall numbers that trigger USER_NOTIF in the Standard profile.
pub fn standard_notify_list() -> Vec<i64> {
    vec![
        SYS_OPENAT,
        SYS_CONNECT,
        SYS_BIND,
        SYS_LISTEN,
        SYS_IOCTL,
        SYS_EXECVE,
        SYS_EXECVEAT,
        SYS_PTRACE,
    ]
}

/// Additional syscalls notified in Strict mode on top of Standard.
pub fn strict_extra_notify_list() -> Vec<i64> {
    vec![
        SYS_ACCEPT,
        SYS_CLONE,
        SYS_FORK,
        SYS_VFORK,
    ]
}

/// Syscall numbers always denied (even in Standard mode).
pub fn standard_deny_list() -> Vec<i64> {
    vec![SYS_MOUNT, SYS_UMOUNT2]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitive_mappings_cover_expected_syscalls() {
        let mappings = sensitive_syscall_mappings();
        let nrs: Vec<i64> = mappings.iter().map(|m| m.syscall_nr).collect();
        assert!(nrs.contains(&SYS_OPENAT));
        assert!(nrs.contains(&SYS_CONNECT));
        assert!(nrs.contains(&SYS_BIND));
        assert!(nrs.contains(&SYS_LISTEN));
        assert!(nrs.contains(&SYS_MOUNT));
        assert!(nrs.contains(&SYS_UMOUNT2));
        assert!(nrs.contains(&SYS_PTRACE));
        assert!(nrs.contains(&SYS_EXECVE));
        assert!(nrs.contains(&SYS_EXECVEAT));
    }

    #[test]
    fn category_lookup_works() {
        assert_eq!(
            category_for_syscall(SYS_OPENAT),
            Some(&PermissionCategory::FileAccess)
        );
        assert_eq!(
            category_for_syscall(SYS_CONNECT),
            Some(&PermissionCategory::NetOutbound)
        );
        assert_eq!(
            category_for_syscall(SYS_MOUNT),
            Some(&PermissionCategory::Root)
        );
        assert_eq!(category_for_syscall(SYS_READ), None);
    }

    #[test]
    fn safe_list_does_not_overlap_with_notify_list() {
        let safe = safe_syscall_list();
        let notify = standard_notify_list();
        for nr in &notify {
            // clone and ioctl may appear in both lists in different contexts,
            // but the notify list takes precedence at the BPF level.
            if *nr == SYS_IOCTL || *nr == SYS_CLONE {
                continue;
            }
            assert!(
                !safe.contains(nr),
                "syscall {nr} should not be in both safe and notify lists"
            );
        }
    }
}
