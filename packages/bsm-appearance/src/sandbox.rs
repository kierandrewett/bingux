//! The converter's self-imposed sandbox.
//!
//! Applied by `bsm-imgconv` after it has opened everything it needs and before
//! it touches a single byte of the untrusted image. Layers, in order:
//!
//! 1. `PR_SET_DUMPABLE 0` and `RLIMIT_CORE 0`: no core dump of the image bytes.
//! 2. rlimits: `RLIMIT_AS`, `RLIMIT_CPU`, `RLIMIT_NOFILE`, `RLIMIT_FSIZE`, `RLIMIT_NPROC`.
//! 3. Refuse to run as root: the converter runs as the unprivileged `bsm` account, which
//!    holds no capabilities, so there is no privilege to drop.
//! 4. Every descriptor except stdio and the output files is closed.
//! 5. An empty network namespace (`CLONE_NEWNET`), if the process may create one.
//! 6. `PR_SET_NO_NEW_PRIVS`.
//! 7. A seccomp-bpf allow list (pure Rust, `seccompiler`) with `SECCOMP_RET_KILL_PROCESS`
//!    as the default. There is no `open`, `socket`, `connect`, `execve`, `fork`, `clone`,
//!    `ptrace`, or `unshare` in it; `write` is limited to stdout, stderr and the output
//!    descriptors; `mmap` and `mprotect` may not make memory executable.
//!
//! After layer 7 the process can only compute on memory it already has, read
//! descriptors it already has, and write to the descriptors it was given. The
//! gap that remains is the kernel's own attack surface reachable through that
//! small set of syscalls (memory management, futex, read, write).

use std::collections::BTreeMap;
use std::io;
use std::os::fd::RawFd;

use seccompiler::{
    BpfProgram, SeccompAction, SeccompCmpArgLen, SeccompCmpOp, SeccompCondition, SeccompFilter, SeccompRule,
};

use crate::convert::{ConvertLimits, SandboxReport};

fn os_error(what: &str) -> String {
    format!("{what}: {}", io::Error::last_os_error())
}

fn set_limit(resource: libc::__rlimit_resource_t, value: u64, name: &str) -> Result<(), String> {
    let limit = libc::rlimit { rlim_cur: value as libc::rlim_t, rlim_max: value as libc::rlim_t };
    // SAFETY: `limit` is a valid rlimit struct.
    if unsafe { libc::setrlimit(resource, &limit) } != 0 {
        return Err(os_error(&format!("setrlimit {name}")));
    }
    Ok(())
}

/// Layers 1 and 2. Safe to run as root or as an ordinary user: limits only ever go down.
pub fn apply_rlimits(limits: &ConvertLimits) -> Result<(), String> {
    // SAFETY: prctl with integer arguments.
    if unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0) } != 0 {
        return Err(os_error("prctl PR_SET_DUMPABLE"));
    }
    set_limit(libc::RLIMIT_CORE, 0, "CORE")?;
    set_limit(libc::RLIMIT_AS, limits.memory_bytes, "AS")?;
    set_limit(libc::RLIMIT_CPU, limits.cpu_seconds, "CPU")?;
    // Enough for stdio and the outputs; never for new descriptors.
    set_limit(libc::RLIMIT_NOFILE, 32, "NOFILE")?;
    set_limit(libc::RLIMIT_FSIZE, limits.max_output_bytes, "FSIZE")?;
    set_limit(libc::RLIMIT_NPROC, 0, "NPROC").or_else(|_| Ok::<(), String>(()))?;
    Ok(())
}

/// Layer 3: the converter never runs as root and never changes uid. bsm itself runs as an
/// unprivileged account with no capabilities and starts the converter as that account, so
/// there is nothing to drop; a root process is a packaging error and is refused rather than
/// silently made safe.
pub fn refuse_root() -> Result<(), String> {
    // SAFETY: geteuid/getuid cannot fail.
    let (euid, uid) = unsafe { (libc::geteuid(), libc::getuid()) };
    if euid == 0 || uid == 0 {
        return Err("bsm-imgconv refuses to decode an untrusted image as root; run it as the bsm account".into());
    }
    Ok(())
}

/// Layer 4: close every descriptor above stderr that is not in `keep`.
pub fn close_other_descriptors(keep: &[RawFd]) {
    let mut keep: Vec<RawFd> = keep.iter().copied().filter(|fd| *fd > 2).collect();
    keep.sort_unstable();
    keep.dedup();
    let mut start: u32 = 3;
    let mut ranges = Vec::new();
    for fd in keep {
        let fd = fd as u32;
        if fd > start {
            ranges.push((start, fd - 1));
        }
        start = fd + 1;
    }
    ranges.push((start, u32::MAX));
    for (first, last) in ranges {
        // SAFETY: close_range only closes descriptors of this process.
        let result = unsafe { libc::syscall(libc::SYS_close_range, first, last, 0u32) };
        if result != 0 && last != u32::MAX {
            for fd in first..=last {
                // SAFETY: closing an unused or already-closed number is harmless.
                unsafe { libc::close(fd as libc::c_int) };
            }
        } else if result != 0 {
            // Old kernel without close_range: close the plausible range one by one.
            for fd in first..1024 {
                // SAFETY: as above.
                unsafe { libc::close(fd as libc::c_int) };
            }
        }
    }
}

/// Layer 5. Returns whether an empty network namespace was created. The unprivileged `bsm`
/// account has no `CAP_SYS_ADMIN`, so this goes through a user namespace
/// (`CLONE_NEWUSER | CLONE_NEWNET`), which only works where the kernel allows unprivileged
/// user namespaces. It is defence in depth: the seccomp filter has no `socket` either.
pub fn enter_empty_network_namespace() -> bool {
    // SAFETY: unshare with flag constants; this process is single threaded.
    unsafe {
        libc::unshare(libc::CLONE_NEWNET) == 0 || libc::unshare(libc::CLONE_NEWUSER | libc::CLONE_NEWNET) == 0
    }
}

/// Layer 6.
pub fn set_no_new_privs() -> Result<(), String> {
    // SAFETY: prctl with integer arguments.
    if unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } != 0 {
        return Err(os_error("prctl PR_SET_NO_NEW_PRIVS"));
    }
    Ok(())
}

fn condition(index: u8, op: SeccompCmpOp, value: u64) -> SeccompCondition {
    SeccompCondition::new(index, SeccompCmpArgLen::Dword, op, value).expect("static seccomp condition is valid")
}

fn rule(conditions: Vec<SeccompCondition>) -> SeccompRule {
    SeccompRule::new(conditions).expect("static seccomp rule is valid")
}

/// Layer 7: the allow list. Exposed so tests can inspect it.
/// `pid` and `tid` are the process and thread the filter is installed in: the converter is
/// single threaded, so `tgkill` is pinned to exactly that pair.
pub fn build_filter(write_fds: &[RawFd], pid: libc::pid_t, tid: libc::pid_t) -> Result<BpfProgram, String> {
    let mut rules: BTreeMap<i64, Vec<SeccompRule>> = BTreeMap::new();
    // Plain allowed syscalls: memory management and what the Rust runtime needs to run
    // single-threaded and exit. None of them reaches the filesystem or the network.
    for syscall in [
        libc::SYS_read,
        libc::SYS_close,
        libc::SYS_munmap,
        libc::SYS_mremap,
        libc::SYS_brk,
        libc::SYS_madvise,
        libc::SYS_exit,
        libc::SYS_exit_group,
        libc::SYS_rt_sigreturn,
        libc::SYS_rt_sigprocmask,
        libc::SYS_sigaltstack,
        libc::SYS_getrandom,
        libc::SYS_clock_gettime,
        libc::SYS_sched_yield,
        libc::SYS_getpid,
        libc::SYS_gettid,
    ] {
        rules.insert(syscall, Vec::new());
    }
    // write(2) only to stdout, stderr and the output files.
    let mut writable: Vec<RawFd> = vec![1, 2];
    writable.extend_from_slice(write_fds);
    writable.sort_unstable();
    writable.dedup();
    rules.insert(
        libc::SYS_write,
        writable.iter().map(|fd| rule(vec![condition(0, SeccompCmpOp::Eq, *fd as u64)])).collect(),
    );
    // futex only with the process-private wait and wake operations (what std and glibc use);
    // shared futexes and the PI, requeue and robust-list operations are not needed.
    let futex_ops = [
        libc::FUTEX_WAIT | libc::FUTEX_PRIVATE_FLAG,
        libc::FUTEX_WAKE | libc::FUTEX_PRIVATE_FLAG,
        libc::FUTEX_WAIT_BITSET | libc::FUTEX_PRIVATE_FLAG,
        libc::FUTEX_WAKE_BITSET | libc::FUTEX_PRIVATE_FLAG,
        libc::FUTEX_WAIT_BITSET | libc::FUTEX_PRIVATE_FLAG | libc::FUTEX_CLOCK_REALTIME,
    ];
    rules.insert(
        libc::SYS_futex,
        futex_ops.iter().map(|op| rule(vec![condition(1, SeccompCmpOp::Eq, *op as u64)])).collect(),
    );
    // The only signalling allowed is tgkill to this very thread of this very process (the Rust
    // runtime's abort path). kill, tkill and tgkill at any other process or thread are killed.
    rules.insert(
        libc::SYS_tgkill,
        vec![rule(vec![condition(0, SeccompCmpOp::Eq, pid as u64), condition(1, SeccompCmpOp::Eq, tid as u64)])],
    );
    // Memory may be mapped and protected, never made executable.
    let not_executable = || rule(vec![condition(2, SeccompCmpOp::MaskedEq(libc::PROT_EXEC as u64), 0)]);
    rules.insert(libc::SYS_mmap, vec![not_executable()]);
    rules.insert(libc::SYS_mprotect, vec![not_executable()]);
    // Debug builds of the standard library check with fcntl(F_GETFD) that a descriptor is still
    // open when a `File` is dropped. Querying the flags of a descriptor is harmless.
    rules.insert(libc::SYS_fcntl, vec![rule(vec![condition(1, SeccompCmpOp::Eq, libc::F_GETFD as u64)])]);
    let filter = SeccompFilter::new(
        rules,
        SeccompAction::KillProcess,
        SeccompAction::Allow,
        std::env::consts::ARCH.try_into().map_err(|error| format!("seccomp architecture: {error:?}"))?,
    )
    .map_err(|error| format!("seccomp filter: {error}"))?;
    filter.try_into().map_err(|error: seccompiler::BackendError| format!("seccomp compile: {error}"))
}

/// Layer 7: install the filter on the calling (only) thread.
pub fn install_seccomp(write_fds: &[RawFd]) -> Result<(), String> {
    // SAFETY: getpid and gettid cannot fail.
    let (pid, tid) = unsafe { (libc::getpid(), libc::syscall(libc::SYS_gettid) as libc::pid_t) };
    let program = build_filter(write_fds, pid, tid)?;
    seccompiler::apply_filter(&program).map_err(|error| format!("seccomp install: {error}"))
}

/// Applies layers 5 to 7 and returns what was established. Layers 1 to 4 are applied
/// earlier by the caller because the image path is opened between them.
pub fn lock_down(write_fds: &[RawFd], require_seccomp: bool, mut report: SandboxReport) -> Result<SandboxReport, String> {
    report.network_namespace = enter_empty_network_namespace();
    set_no_new_privs()?;
    report.no_new_privs = true;
    match install_seccomp(write_fds) {
        Ok(()) => report.seccomp = true,
        Err(error) if require_seccomp => return Err(error),
        Err(error) => eprintln!("bsm-imgconv: seccomp unavailable, continuing with rlimits only: {error}"),
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_compiles_and_is_small() {
        let program = build_filter(&[7, 8], 100, 100).unwrap();
        assert!(!program.is_empty() && program.len() < 400, "{} instructions", program.len());
    }

    #[test]
    fn root_is_refused_and_ordinary_users_pass() {
        // SAFETY: geteuid cannot fail.
        let root = unsafe { libc::geteuid() } == 0;
        assert_eq!(refuse_root().is_err(), root);
    }
}
