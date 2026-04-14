//! Syscall decoder — maps raw seccomp notification events into
//! high-level [`PermissionRequest`] values that the daemon can match
//! against the permission database.
//!
//! In production the path / address arguments live in the *target*
//! process's address space and must be read via `/proc/<pid>/mem`.
//! The helpers here are stubbed (`read_string_from_process`) to keep
//! the crate buildable without root or real seccomp.

use crate::error::{GatedError, Result};

// ── Syscall numbers (x86_64) ─────────────────────────────────────

pub const SYS_OPENAT: i64 = 257;
pub const SYS_CONNECT: i64 = 42;
pub const SYS_BIND: i64 = 49;
pub const SYS_LISTEN: i64 = 50;
pub const SYS_EXECVE: i64 = 59;
pub const SYS_EXECVEAT: i64 = 322;
pub const SYS_PTRACE: i64 = 101;
pub const SYS_MOUNT: i64 = 165;

// ── Raw event from seccomp notification ──────────────────────────

/// A raw syscall event as received from a seccomp user notification.
#[derive(Debug, Clone)]
pub struct SyscallEvent {
    pub pid: u32,
    pub syscall_nr: i64,
    pub args: [u64; 6],
}

// ── High-level permission requests ───────────────────────────────

bitflags::bitflags! {
    /// Bitflag describing the kind of file access requested.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct FileAccessFlags: u32 {
        const READ  = 0b001;
        const WRITE = 0b010;
        const LIST  = 0b100;
    }
}

/// A decoded permission request that the daemon can evaluate against
/// the permission database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionRequest {
    FileAccess {
        path: String,
        flags: FileAccessFlags,
    },
    NetworkConnect {
        addr: String,
        port: u16,
    },
    NetworkBind {
        addr: String,
        port: u16,
    },
    NetworkListen {
        fd: i32,
    },
    DeviceAccess {
        device: String,
        category: String,
    },
    ProcessExec {
        path: String,
    },
    ProcessPtrace {
        target_pid: u32,
    },
    Mount {
        source: String,
        target: String,
    },
}

// ── Process memory reading (stubbed) ─────────────────────────────

/// Read a NUL-terminated string from `pid`'s address space at `addr`.
///
/// On a real system this opens `/proc/<pid>/mem`, seeks to `addr`, and
/// reads until NUL.  In tests and on non-Linux platforms we return a
/// placeholder so the crate compiles without special privileges.
#[cfg(any(test, not(target_os = "linux")))]
fn read_string_from_process(_pid: u32, addr: u64) -> Result<String> {
    Ok(format!("<mem@0x{addr:x}>"))
}

#[cfg(all(not(test), target_os = "linux"))]
fn read_string_from_process(pid: u32, addr: u64) -> Result<String> {
    use std::fs::File;
    use std::io::{Read, Seek, SeekFrom};

    if addr == 0 {
        return Ok(String::new());
    }

    let mem_path = format!("/proc/{pid}/mem");
    let mut file = File::open(&mem_path).map_err(|e| GatedError::ProcessMemoryRead {
        pid,
        message: format!("cannot open {mem_path}: {e}"),
    })?;

    file.seek(SeekFrom::Start(addr)).map_err(|e| GatedError::ProcessMemoryRead {
        pid,
        message: format!("seek to 0x{addr:x}: {e}"),
    })?;

    let mut buf = Vec::with_capacity(4096);
    let mut byte = [0u8; 1];
    for _ in 0..4096 {
        match file.read_exact(&mut byte) {
            Ok(()) if byte[0] == 0 => break,
            Ok(()) => buf.push(byte[0]),
            Err(e) => {
                return Err(GatedError::ProcessMemoryRead {
                    pid,
                    message: format!("read at 0x{addr:x}: {e}"),
                })
            }
        }
    }

    String::from_utf8(buf).map_err(|e| GatedError::ProcessMemoryRead {
        pid,
        message: format!("invalid UTF-8: {e}"),
    })
}

/// Read a `sockaddr_in` / `sockaddr_in6` from process memory and
/// return `(addr_string, port)`.  Stubbed for now.
fn read_sockaddr_from_process(_pid: u32, addr: u64) -> Result<(String, u16)> {
    // In a real implementation we would read the sockaddr struct and
    // decode the family, address, and port fields.
    Ok((format!("<sockaddr@0x{addr:x}>"), 0))
}

// ── Decoder ──────────────────────────────────────────────────────

/// Decode a raw [`SyscallEvent`] into a [`PermissionRequest`].
///
/// # Mapping
///
/// | Syscall NR | Name        | → PermissionRequest variant |
/// |------------|-------------|-----------------------------|
/// | 257        | `openat`    | `FileAccess`                |
/// | 42         | `connect`   | `NetworkConnect`            |
/// | 49         | `bind`      | `NetworkBind`               |
/// | 50         | `listen`    | `NetworkListen`             |
/// | 59         | `execve`    | `ProcessExec`               |
/// | 322        | `execveat`  | `ProcessExec`               |
/// | 101        | `ptrace`    | `ProcessPtrace`             |
/// | 165        | `mount`     | `Mount`                     |
pub fn decode_syscall(event: &SyscallEvent) -> Result<PermissionRequest> {
    match event.syscall_nr {
        SYS_OPENAT => decode_openat(event),
        SYS_CONNECT => decode_connect(event),
        SYS_BIND => decode_bind(event),
        SYS_LISTEN => decode_listen(event),
        SYS_EXECVE => decode_execve(event),
        SYS_EXECVEAT => decode_execveat(event),
        SYS_PTRACE => decode_ptrace(event),
        SYS_MOUNT => decode_mount(event),
        nr => Err(GatedError::UnknownSyscall(nr)),
    }
}

// openat(dirfd, pathname, flags, mode)
fn decode_openat(event: &SyscallEvent) -> Result<PermissionRequest> {
    let path = read_string_from_process(event.pid, event.args[1])?;
    let raw_flags = event.args[2] as u32;

    // O_RDONLY=0, O_WRONLY=1, O_RDWR=2, O_DIRECTORY=0o200000
    let mut flags = FileAccessFlags::empty();
    let access_mode = raw_flags & 0o3;
    if access_mode == 0 || access_mode == 2 {
        flags |= FileAccessFlags::READ;
    }
    if access_mode == 1 || access_mode == 2 {
        flags |= FileAccessFlags::WRITE;
    }
    if raw_flags & 0o200000 != 0 {
        flags |= FileAccessFlags::LIST;
    }

    Ok(PermissionRequest::FileAccess { path, flags })
}

// connect(sockfd, addr, addrlen)
fn decode_connect(event: &SyscallEvent) -> Result<PermissionRequest> {
    let (addr, port) = read_sockaddr_from_process(event.pid, event.args[1])?;
    Ok(PermissionRequest::NetworkConnect { addr, port })
}

// bind(sockfd, addr, addrlen)
fn decode_bind(event: &SyscallEvent) -> Result<PermissionRequest> {
    let (addr, port) = read_sockaddr_from_process(event.pid, event.args[1])?;
    Ok(PermissionRequest::NetworkBind { addr, port })
}

// listen(sockfd, backlog)
fn decode_listen(event: &SyscallEvent) -> Result<PermissionRequest> {
    Ok(PermissionRequest::NetworkListen {
        fd: event.args[0] as i32,
    })
}

// execve(pathname, argv, envp)
fn decode_execve(event: &SyscallEvent) -> Result<PermissionRequest> {
    let path = read_string_from_process(event.pid, event.args[0])?;
    Ok(PermissionRequest::ProcessExec { path })
}

// execveat(dirfd, pathname, argv, envp, flags)
fn decode_execveat(event: &SyscallEvent) -> Result<PermissionRequest> {
    let path = read_string_from_process(event.pid, event.args[1])?;
    Ok(PermissionRequest::ProcessExec { path })
}

// ptrace(request, pid, addr, data)
fn decode_ptrace(event: &SyscallEvent) -> Result<PermissionRequest> {
    Ok(PermissionRequest::ProcessPtrace {
        target_pid: event.args[1] as u32,
    })
}

// mount(source, target, filesystemtype, mountflags, data)
fn decode_mount(event: &SyscallEvent) -> Result<PermissionRequest> {
    let source = read_string_from_process(event.pid, event.args[0])?;
    let target = read_string_from_process(event.pid, event.args[1])?;
    Ok(PermissionRequest::Mount { source, target })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_openat_read() {
        let event = SyscallEvent {
            pid: 1000,
            syscall_nr: SYS_OPENAT,
            args: [0, 0x1000, 0, 0, 0, 0], // O_RDONLY = 0
        };
        let req = decode_syscall(&event).unwrap();
        match req {
            PermissionRequest::FileAccess { flags, .. } => {
                assert!(flags.contains(FileAccessFlags::READ));
                assert!(!flags.contains(FileAccessFlags::WRITE));
            }
            other => panic!("expected FileAccess, got {other:?}"),
        }
    }

    #[test]
    fn decode_openat_write() {
        let event = SyscallEvent {
            pid: 1000,
            syscall_nr: SYS_OPENAT,
            args: [0, 0x1000, 1, 0, 0, 0], // O_WRONLY = 1
        };
        let req = decode_syscall(&event).unwrap();
        match req {
            PermissionRequest::FileAccess { flags, .. } => {
                assert!(!flags.contains(FileAccessFlags::READ));
                assert!(flags.contains(FileAccessFlags::WRITE));
            }
            other => panic!("expected FileAccess, got {other:?}"),
        }
    }

    #[test]
    fn decode_openat_rdwr() {
        let event = SyscallEvent {
            pid: 1000,
            syscall_nr: SYS_OPENAT,
            args: [0, 0x1000, 2, 0, 0, 0], // O_RDWR = 2
        };
        let req = decode_syscall(&event).unwrap();
        match req {
            PermissionRequest::FileAccess { flags, .. } => {
                assert!(flags.contains(FileAccessFlags::READ));
                assert!(flags.contains(FileAccessFlags::WRITE));
            }
            other => panic!("expected FileAccess, got {other:?}"),
        }
    }

    #[test]
    fn decode_openat_directory() {
        let event = SyscallEvent {
            pid: 1000,
            syscall_nr: SYS_OPENAT,
            args: [0, 0x1000, 0o200000, 0, 0, 0], // O_DIRECTORY | O_RDONLY
        };
        let req = decode_syscall(&event).unwrap();
        match req {
            PermissionRequest::FileAccess { flags, .. } => {
                assert!(flags.contains(FileAccessFlags::LIST));
                assert!(flags.contains(FileAccessFlags::READ));
            }
            other => panic!("expected FileAccess, got {other:?}"),
        }
    }

    #[test]
    fn decode_connect() {
        let event = SyscallEvent {
            pid: 1000,
            syscall_nr: SYS_CONNECT,
            args: [3, 0x2000, 16, 0, 0, 0],
        };
        let req = decode_syscall(&event).unwrap();
        assert!(matches!(req, PermissionRequest::NetworkConnect { .. }));
    }

    #[test]
    fn decode_bind() {
        let event = SyscallEvent {
            pid: 1000,
            syscall_nr: SYS_BIND,
            args: [3, 0x2000, 16, 0, 0, 0],
        };
        let req = decode_syscall(&event).unwrap();
        assert!(matches!(req, PermissionRequest::NetworkBind { .. }));
    }

    #[test]
    fn decode_listen() {
        let event = SyscallEvent {
            pid: 1000,
            syscall_nr: SYS_LISTEN,
            args: [5, 128, 0, 0, 0, 0],
        };
        let req = decode_syscall(&event).unwrap();
        assert!(matches!(req, PermissionRequest::NetworkListen { fd: 5 }));
    }

    #[test]
    fn decode_execve() {
        let event = SyscallEvent {
            pid: 1000,
            syscall_nr: SYS_EXECVE,
            args: [0x3000, 0, 0, 0, 0, 0],
        };
        let req = decode_syscall(&event).unwrap();
        assert!(matches!(req, PermissionRequest::ProcessExec { .. }));
    }

    #[test]
    fn decode_execveat() {
        let event = SyscallEvent {
            pid: 1000,
            syscall_nr: SYS_EXECVEAT,
            args: [0, 0x3000, 0, 0, 0, 0],
        };
        let req = decode_syscall(&event).unwrap();
        assert!(matches!(req, PermissionRequest::ProcessExec { .. }));
    }

    #[test]
    fn decode_ptrace() {
        let event = SyscallEvent {
            pid: 1000,
            syscall_nr: SYS_PTRACE,
            args: [0, 2000, 0, 0, 0, 0],
        };
        let req = decode_syscall(&event).unwrap();
        assert!(matches!(
            req,
            PermissionRequest::ProcessPtrace { target_pid: 2000 }
        ));
    }

    #[test]
    fn decode_mount() {
        let event = SyscallEvent {
            pid: 1000,
            syscall_nr: SYS_MOUNT,
            args: [0x4000, 0x5000, 0, 0, 0, 0],
        };
        let req = decode_syscall(&event).unwrap();
        assert!(matches!(req, PermissionRequest::Mount { .. }));
    }

    #[test]
    fn unknown_syscall() {
        let event = SyscallEvent {
            pid: 1000,
            syscall_nr: 999,
            args: [0; 6],
        };
        assert!(decode_syscall(&event).is_err());
    }
}
