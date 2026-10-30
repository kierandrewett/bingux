//! Peer credentials and the checks that decide whether a connecting process may take
//! a role. Everything here is pure or reads only `/proc`, so it is tested with the
//! test process's own credentials against deliberately wrong expectations.

use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::path::Path;

/// Who is on the other end of a connected socket, as the kernel reports it.
#[derive(Debug)]
pub struct Peer {
    pub uid: u32,
    pub pid: u32,
    /// `SO_PEERPIDFD` (Linux 6.5+): keeps the process identity stable between the
    /// connect and our `/proc` reads. `None` on older kernels.
    pub pidfd: Option<OwnedFd>,
}

const SO_PEERPIDFD: libc::c_int = 77;

pub fn peer_of(stream: &UnixStream) -> io::Result<Peer> {
    let mut cred = libc::ucred { pid: 0, uid: 0, gid: 0 };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: getsockopt writes at most `len` bytes into the ucred we own.
    let r = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut cred as *mut libc::ucred).cast(),
            &mut len,
        )
    };
    if r != 0 {
        return Err(io::Error::last_os_error());
    }
    let mut pid = cred.pid as u32;
    let pidfd = peer_pidfd(stream);
    if let Some(fd) = &pidfd {
        // The pidfd is authoritative: SO_PEERCRED's pid can name a recycled number.
        if let Some(p) = pid_of_pidfd(fd) {
            pid = p;
        }
    }
    Ok(Peer { uid: cred.uid, pid, pidfd })
}

fn peer_pidfd(stream: &UnixStream) -> Option<OwnedFd> {
    let mut fd: libc::c_int = -1;
    let mut len = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
    // SAFETY: plain getsockopt into a c_int of the stated size.
    let r = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            SO_PEERPIDFD,
            (&mut fd as *mut libc::c_int).cast(),
            &mut len,
        )
    };
    if r == 0 && fd >= 0 {
        // SAFETY: the kernel returned a fresh descriptor that we now own.
        Some(unsafe { OwnedFd::from_raw_fd(fd) })
    } else {
        None
    }
}

fn pid_of_pidfd(fd: &OwnedFd) -> Option<u32> {
    let text = std::fs::read_to_string(format!("/proc/self/fdinfo/{}", fd.as_raw_fd())).ok()?;
    text.lines().find_map(|l| l.strip_prefix("Pid:")).and_then(|v| v.trim().parse().ok())
}

impl Peer {
    /// False once the peer process has exited (its pidfd becomes readable). Without a
    /// pidfd this cannot tell and returns true.
    pub fn still_alive(&self) -> bool {
        let Some(fd) = &self.pidfd else { return true };
        let mut p = libc::pollfd { fd: fd.as_raw_fd(), events: libc::POLLIN, revents: 0 };
        // SAFETY: one valid pollfd, zero timeout.
        let n = unsafe { libc::poll(&mut p, 1, 0) };
        n == 0
    }
}

/// Does the unified-hierarchy line(s) of `/proc/<pid>/cgroup` put the process in
/// `scope`? A scope that starts with `/` is a cgroup path prefix; otherwise it is a
/// path component (for example `app-bsm-host-s1234.scope`).
pub fn cgroup_matches(cgroup_file: &str, scope: &str) -> bool {
    if scope.is_empty() {
        return false;
    }
    cgroup_file.lines().any(|line| {
        let path = line.rsplit(':').next().unwrap_or("");
        if scope.starts_with('/') {
            path == scope || path.starts_with(&format!("{}/", scope.trim_end_matches('/')))
        } else {
            path.split('/').any(|c| c == scope)
        }
    })
}

#[derive(Debug, PartialEq, Eq)]
pub enum ExeCheck {
    Match,
    Mismatch(String),
    /// Unreadable (another uid's `exe` needs CAP_SYS_PTRACE, which bsm does not have).
    Unreadable,
}

/// Best-effort `/proc/<pid>/exe` check. A mismatch is a rejection; unreadable is not.
pub fn exe_check(pid: u32, expected: &str) -> ExeCheck {
    match std::fs::read_link(format!("/proc/{pid}/exe")) {
        Ok(p) => {
            // The kernel appends " (deleted)" for replaced binaries: that is a mismatch too.
            if Path::new(expected) == p.as_path() {
                ExeCheck::Match
            } else {
                ExeCheck::Mismatch(p.display().to_string())
            }
        }
        Err(_) => ExeCheck::Unreadable,
    }
}

/// What a connection must look like. `scope` empty or `pid` zero means "not bound yet".
#[derive(Debug, Clone)]
pub struct Expectation {
    pub uid: u32,
    pub pid: u32,
    pub scope: String,
    pub exe: String,
}

/// A peer whose process cannot be pinned (no `SO_PEERPIDFD`, Linux before 6.5) is refused:
/// a pid number alone can be recycled between the connect and our `/proc` reads (review F8).
fn require_pinned(peer: &Peer) -> Result<(), String> {
    if peer.pidfd.is_none() {
        return Err("peer process cannot be pinned (no SO_PEERPIDFD); refusing".into());
    }
    Ok(())
}

/// Check uid only (done at accept time, before the binding arrives).
pub fn check_uid(peer: &Peer, uid: u32) -> Result<(), String> {
    if peer.uid == uid {
        Ok(())
    } else {
        Err(format!("peer uid {} is not the expected uid {uid}", peer.uid))
    }
}

/// Full check of a hosted-session bridge: uid, pid, cgroup scope, and best effort exe.
/// `cgroup_of` is `/proc/<pid>/cgroup` unless a test injects another source.
pub fn verify_host(peer: &Peer, want: &Expectation) -> Result<(), String> {
    check_uid(peer, want.uid)?;
    require_pinned(peer)?;
    if want.pid == 0 || peer.pid != want.pid {
        return Err(format!("peer pid {} is not the expected pid {}", peer.pid, want.pid));
    }
    let cg = std::fs::read_to_string(format!("/proc/{}/cgroup", peer.pid))
        .map_err(|e| format!("cannot read cgroup of pid {}: {e}", peer.pid))?;
    if !cgroup_matches(&cg, &want.scope) {
        return Err(format!("pid {} is not in scope {}", peer.pid, want.scope));
    }
    if !want.exe.is_empty() {
        if let ExeCheck::Mismatch(actual) = exe_check(peer.pid, &want.exe) {
            return Err(format!("exe {actual} is not {}", want.exe));
        }
    }
    if !peer.still_alive() {
        return Err("peer exited during verification".into());
    }
    Ok(())
}

/// Greeter: uid and the pid bsm-helper launched. Best effort exe is not checked (the
/// greeter is a Quickshell process whose path the helper does not report).
pub fn verify_greeter(peer: &Peer, uid: u32, pid: u32) -> Result<(), String> {
    check_uid(peer, uid)?;
    require_pinned(peer)?;
    if peer.pid != pid {
        return Err(format!("peer pid {} is not the greeter pid {pid}", peer.pid));
    }
    if !peer.still_alive() {
        return Err("peer exited during verification".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn me() -> Peer {
        let (a, _b) = UnixStream::pair().unwrap();
        peer_of(&a).unwrap()
    }

    #[test]
    fn socketpair_peer_is_this_process() {
        let p = me();
        assert_eq!(p.pid, std::process::id());
        assert_eq!(p.uid, rustix::process::getuid().as_raw());
        assert!(p.still_alive());
    }

    #[test]
    fn cgroup_scope_matching() {
        let f = "0::/user.slice/user-1000.slice/user@1000.service/app.slice/bsm-host-s1.scope\n";
        assert!(cgroup_matches(f, "bsm-host-s1.scope"));
        assert!(cgroup_matches(f, "/user.slice/user-1000.slice"));
        assert!(!cgroup_matches(f, "bsm-host-s2.scope"));
        assert!(!cgroup_matches(f, "host-s1.scope"), "components match whole names only");
        assert!(!cgroup_matches(f, "/user.slice/user-100"), "prefix must end at a component");
        assert!(!cgroup_matches(f, ""));
    }

    #[test]
    fn wrong_uid_pid_scope_and_exe_are_rejected() {
        let p = me();
        let my_cg = std::fs::read_to_string("/proc/self/cgroup").unwrap();
        let comp = my_cg.lines().next().unwrap().rsplit(':').next().unwrap().to_string();
        let exe = std::fs::read_link("/proc/self/exe").unwrap().display().to_string();
        let good = Expectation {
            uid: p.uid,
            pid: p.pid,
            scope: if comp == "/" { "/".into() } else { comp.clone() },
            exe: exe.clone(),
        };
        if comp != "/" {
            assert_eq!(verify_host(&p, &good), Ok(()));
        }
        let mut bad = good.clone();
        bad.uid = p.uid + 1;
        assert!(verify_host(&p, &bad).unwrap_err().contains("uid"));
        let mut bad = good.clone();
        bad.pid = p.pid + 1;
        assert!(verify_host(&p, &bad).unwrap_err().contains("pid"));
        let mut bad = good.clone();
        bad.pid = 0;
        assert!(verify_host(&p, &bad).is_err(), "an unbound expectation never verifies");
        let mut bad = good.clone();
        bad.scope = "definitely-not-my-scope.scope".into();
        assert!(verify_host(&p, &bad).unwrap_err().contains("scope"));
        let mut bad = good;
        bad.exe = "/usr/bin/true-not-me".into();
        if let ExeCheck::Unreadable = exe_check(p.pid, &exe) {
        } else {
            assert!(verify_host(&p, &bad).unwrap_err().contains("exe"));
        }
    }

    #[test]
    fn a_peer_that_cannot_be_pinned_is_refused() {
        let mut p = me();
        p.pidfd = None;
        let want = Expectation { uid: p.uid, pid: p.pid, scope: "/".into(), exe: String::new() };
        assert!(verify_host(&p, &want).unwrap_err().contains("pinned"));
        assert!(verify_greeter(&p, p.uid, p.pid).unwrap_err().contains("pinned"));
    }

    #[test]
    fn greeter_checks() {
        let p = me();
        assert_eq!(verify_greeter(&p, p.uid, p.pid), Ok(()));
        assert!(verify_greeter(&p, p.uid + 1, p.pid).is_err());
        assert!(verify_greeter(&p, p.uid, p.pid + 1).is_err());
    }
}
