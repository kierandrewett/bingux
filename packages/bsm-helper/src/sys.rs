//! Thin, audited wrappers over the libc calls the helper needs. Everything
//! that is `unsafe` outside the PAM binding lives here or in `pam.rs`.

use std::ffi::{CStr, CString, OsStr};
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub fn cvt(r: libc::c_int) -> io::Result<libc::c_int> {
    if r < 0 { Err(io::Error::last_os_error()) } else { Ok(r) }
}

/// A connected `SOCK_SEQPACKET` pair, close-on-exec.
pub fn seqpacket_pair() -> io::Result<(OwnedFd, OwnedFd)> {
    let mut fds = [0 as libc::c_int; 2];
    cvt(unsafe {
        libc::socketpair(libc::AF_UNIX, libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC, 0, fds.as_mut_ptr())
    })?;
    Ok(unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) })
}

pub fn set_nonblocking(fd: RawFd, on: bool) -> io::Result<()> {
    let flags = cvt(unsafe { libc::fcntl(fd, libc::F_GETFL) })?;
    let flags = if on { flags | libc::O_NONBLOCK } else { flags & !libc::O_NONBLOCK };
    cvt(unsafe { libc::fcntl(fd, libc::F_SETFL, flags) })?;
    Ok(())
}

fn sockaddr_un(path: &Path) -> io::Result<(libc::sockaddr_un, libc::socklen_t)> {
    let bytes = path.as_os_str().as_bytes();
    let mut addr: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    if bytes.is_empty() || bytes.len() >= addr.sun_path.len() || bytes.contains(&0) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "socket path empty, too long or contains NUL"));
    }
    addr.sun_family = libc::AF_UNIX as libc::sa_family_t;
    for (slot, b) in addr.sun_path.iter_mut().zip(bytes) {
        *slot = *b as libc::c_char;
    }
    let len = std::mem::size_of::<libc::sa_family_t>() + bytes.len() + 1;
    Ok((addr, len as libc::socklen_t))
}

/// Bind and listen on a `SOCK_SEQPACKET` socket at `path` (nonblocking, close-on-exec).
/// A stale socket file is replaced; the mode is applied before `listen` so there
/// is no window with a wider mode.
pub fn listen_seqpacket(path: &Path, mode: u32) -> io::Result<OwnedFd> {
    let (addr, len) = sockaddr_un(path)?;
    let fd = cvt(unsafe {
        libc::socket(libc::AF_UNIX, libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK, 0)
    })?;
    let fd = unsafe { OwnedFd::from_raw_fd(fd) };
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    // Create the socket node under a restrictive umask, then set the exact mode.
    let old = unsafe { libc::umask(0o177) };
    let bound = cvt(unsafe { libc::bind(fd.as_raw_fd(), &addr as *const _ as *const libc::sockaddr, len) });
    unsafe { libc::umask(old) };
    bound?;
    let c = path_cstr(path)?;
    cvt(unsafe { libc::chmod(c.as_ptr(), mode as libc::mode_t) })?;
    cvt(unsafe { libc::listen(fd.as_raw_fd(), 8) })?;
    Ok(fd)
}

pub fn connect_seqpacket(path: &Path) -> io::Result<OwnedFd> {
    let (addr, len) = sockaddr_un(path)?;
    let fd = cvt(unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC, 0) })?;
    let fd = unsafe { OwnedFd::from_raw_fd(fd) };
    cvt(unsafe { libc::connect(fd.as_raw_fd(), &addr as *const _ as *const libc::sockaddr, len) })?;
    Ok(fd)
}

/// Accept one connection from a nonblocking listener. `Ok(None)` when none is pending.
pub fn accept(listener: RawFd) -> io::Result<Option<OwnedFd>> {
    let fd = unsafe { libc::accept4(listener, std::ptr::null_mut(), std::ptr::null_mut(), libc::SOCK_CLOEXEC) };
    if fd < 0 {
        let e = io::Error::last_os_error();
        return match e.kind() {
            io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted => Ok(None),
            _ => Err(e),
        };
    }
    Ok(Some(unsafe { OwnedFd::from_raw_fd(fd) }))
}

/// Peer credentials of a connected unix socket: (pid, uid, gid).
pub fn peer_cred(fd: RawFd) -> io::Result<(u32, u32, u32)> {
    let mut cred: libc::ucred = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    cvt(unsafe {
        libc::getsockopt(fd, libc::SOL_SOCKET, libc::SO_PEERCRED, &mut cred as *mut _ as *mut libc::c_void, &mut len)
    })?;
    Ok((cred.pid as u32, cred.uid, cred.gid))
}

#[derive(Debug, PartialEq, Eq)]
pub enum Recv {
    /// One datagram of this many bytes is in the buffer.
    Data(usize),
    /// The peer closed the connection.
    Eof,
    WouldBlock,
    /// The datagram did not fit the buffer; its tail was discarded by the kernel.
    TooBig,
}

pub fn recv(fd: RawFd, buf: &mut [u8]) -> io::Result<Recv> {
    loop {
        // MSG_TRUNC makes the kernel return the real datagram length, so an
        // oversized message is detected rather than silently cut.
        let n = unsafe { libc::recv(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len(), libc::MSG_TRUNC) };
        if n < 0 {
            let e = io::Error::last_os_error();
            match e.kind() {
                io::ErrorKind::Interrupted => continue,
                io::ErrorKind::WouldBlock => return Ok(Recv::WouldBlock),
                _ if e.raw_os_error() == Some(libc::ECONNRESET) => return Ok(Recv::Eof),
                _ => return Err(e),
            }
        }
        let n = n as usize;
        return Ok(if n == 0 {
            Recv::Eof
        } else if n > buf.len() {
            Recv::TooBig
        } else {
            Recv::Data(n)
        });
    }
}

/// Send one datagram. `Ok(false)` when the socket buffer is full (nonblocking socket).
pub fn send(fd: RawFd, data: &[u8]) -> io::Result<bool> {
    loop {
        let n = unsafe { libc::send(fd, data.as_ptr() as *const libc::c_void, data.len(), libc::MSG_NOSIGNAL) };
        if n < 0 {
            let e = io::Error::last_os_error();
            match e.kind() {
                io::ErrorKind::Interrupted => continue,
                io::ErrorKind::WouldBlock => return Ok(false),
                _ => return Err(e),
            }
        }
        return Ok(true);
    }
}

/// Block until `fd` is readable or `timeout` passes. `Ok(false)` on timeout.
pub fn wait_readable(fd: RawFd, timeout: Duration) -> io::Result<bool> {
    let mut p = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
    let ms = timeout.as_millis().min(i32::MAX as u128) as libc::c_int;
    loop {
        let r = unsafe { libc::poll(&mut p, 1, ms) };
        if r < 0 {
            let e = io::Error::last_os_error();
            if e.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(e);
        }
        return Ok(r > 0);
    }
}

pub fn random_u64() -> io::Result<u64> {
    let mut buf = [0u8; 8];
    let mut off = 0;
    while off < buf.len() {
        let n = unsafe { libc::getrandom(buf[off..].as_mut_ptr() as *mut libc::c_void, buf.len() - off, 0) };
        if n < 0 {
            let e = io::Error::last_os_error();
            if e.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(e);
        }
        off += n as usize;
    }
    Ok(u64::from_ne_bytes(buf))
}

/// Open a directory without following a symlink in the last component.
pub fn open_dir_nofollow(path: &Path) -> io::Result<OwnedFd> {
    let c = path_cstr(path)?;
    let fd = cvt(unsafe { libc::open(c.as_ptr(), libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC) })?;
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

pub fn path_cstr(path: &Path) -> io::Result<CString> {
    CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains NUL"))
}

pub fn str_cstr(s: &str) -> io::Result<CString> {
    CString::new(s).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "string contains NUL"))
}

/// A passwd entry. Strings are owned so no pointer into libc's buffer escapes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Passwd {
    pub name: String,
    pub uid: u32,
    pub gid: u32,
    pub home: PathBuf,
    pub shell: String,
}

fn passwd_from(pw: &libc::passwd) -> Passwd {
    let s = |p: *const libc::c_char| unsafe {
        if p.is_null() { String::new() } else { CStr::from_ptr(p).to_string_lossy().into_owned() }
    };
    Passwd {
        name: s(pw.pw_name),
        uid: pw.pw_uid,
        gid: pw.pw_gid,
        home: PathBuf::from(OsStr::from_bytes(unsafe {
            if pw.pw_dir.is_null() { b"" } else { CStr::from_ptr(pw.pw_dir).to_bytes() }
        })),
        shell: s(pw.pw_shell),
    }
}

fn lookup(f: impl Fn(*mut libc::passwd, *mut libc::c_char, usize, *mut *mut libc::passwd) -> libc::c_int) -> io::Result<Option<Passwd>> {
    let mut size = 1024usize;
    loop {
        let mut buf = vec![0 as libc::c_char; size];
        let mut pw: libc::passwd = unsafe { std::mem::zeroed() };
        let mut out: *mut libc::passwd = std::ptr::null_mut();
        let r = f(&mut pw, buf.as_mut_ptr(), buf.len(), &mut out);
        if r == libc::ERANGE && size < 1 << 20 {
            size *= 4;
            continue;
        }
        if r != 0 {
            return Err(io::Error::from_raw_os_error(r));
        }
        if out.is_null() {
            return Ok(None);
        }
        return Ok(Some(passwd_from(&pw)));
    }
}

pub fn getpwnam(name: &str) -> io::Result<Option<Passwd>> {
    let c = str_cstr(name)?;
    lookup(|pw, buf, len, out| unsafe { libc::getpwnam_r(c.as_ptr(), pw, buf, len, out) })
}

pub fn getpwuid(uid: u32) -> io::Result<Option<Passwd>> {
    lookup(|pw, buf, len, out| unsafe { libc::getpwuid_r(uid, pw, buf, len, out) })
}

/// Fork. `Ok(0)` in the child. The helper is single threaded so the child may
/// allocate and call PAM.
///
/// # Safety
/// Caller must be single threaded or only call async-signal-safe functions in the child.
pub unsafe fn fork() -> io::Result<libc::pid_t> {
    cvt(unsafe { libc::fork() })
}

/// Close every descriptor except stdio and `keep`.
pub fn close_fds_except(keep: &[RawFd]) {
    let mut keep: Vec<RawFd> = keep.iter().copied().filter(|f| *f > 2).collect();
    keep.sort_unstable();
    let mut start: libc::c_uint = 3;
    let mut ranges = Vec::new();
    for k in keep {
        let k = k as libc::c_uint;
        if k > start {
            ranges.push((start, k - 1));
        }
        start = k + 1;
    }
    ranges.push((start, libc::c_uint::MAX));
    for (a, b) in ranges {
        let r = unsafe { libc::syscall(libc::SYS_close_range, a, b, 0) };
        if r != 0 {
            // Kernel without close_range: fall back to a bounded loop.
            let last = b.min(4096);
            for fd in a..=last {
                unsafe { libc::close(fd as RawFd) };
            }
        }
    }
}

/// Restore default signal dispositions and an empty mask in a forked child.
pub fn reset_signals() {
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        libc::pthread_sigmask(libc::SIG_SETMASK, &set, std::ptr::null_mut());
        for sig in [libc::SIGPIPE, libc::SIGCHLD, libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
            libc::signal(sig, libc::SIG_DFL);
        }
    }
}

/// Block `signals` for this thread and return a signalfd for them.
pub fn signal_fd(signals: &[libc::c_int]) -> io::Result<OwnedFd> {
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        for s in signals {
            libc::sigaddset(&mut set, *s);
        }
        cvt(libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut()))?;
        let fd = cvt(libc::signalfd(-1, &set, libc::SFD_CLOEXEC | libc::SFD_NONBLOCK))?;
        Ok(OwnedFd::from_raw_fd(fd))
    }
}

/// Drain a signalfd and return the signal numbers read.
pub fn read_signals(fd: RawFd) -> Vec<libc::c_int> {
    let mut out = Vec::new();
    loop {
        let mut info: libc::signalfd_siginfo = unsafe { std::mem::zeroed() };
        let n = unsafe {
            libc::read(fd, &mut info as *mut _ as *mut libc::c_void, std::mem::size_of::<libc::signalfd_siginfo>())
        };
        if n as usize != std::mem::size_of::<libc::signalfd_siginfo>() {
            break;
        }
        out.push(info.ssi_signo as libc::c_int);
    }
    out
}

/// Decode a wait status: exit code, or the negative signal number.
pub fn decode_status(status: libc::c_int) -> i32 {
    if libc::WIFEXITED(status) {
        libc::WEXITSTATUS(status)
    } else if libc::WIFSIGNALED(status) {
        -libc::WTERMSIG(status)
    } else {
        -1
    }
}

/// Reap every exited child. Returns `(pid, decoded status)` pairs.
pub fn reap_children() -> Vec<(libc::pid_t, i32)> {
    let mut out = Vec::new();
    loop {
        let mut status = 0;
        let pid = unsafe { libc::waitpid(-1, &mut status, libc::WNOHANG) };
        if pid <= 0 {
            break;
        }
        out.push((pid, decode_status(status)));
    }
    out
}

/// Wait for one specific child, without blocking forever: returns `None` on timeout.
pub fn wait_pid_timeout(pid: libc::pid_t, timeout: Duration) -> Option<i32> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let mut status = 0;
        let r = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
        if r == pid {
            return Some(decode_status(status));
        }
        if r < 0 {
            return Some(-1);
        }
        if std::time::Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Pids whose real uid is `uid`, from /proc.
fn pids_of_uid(uid: u32) -> Vec<libc::pid_t> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir("/proc") else { return out };
    for e in rd.flatten() {
        let Some(pid) = e.file_name().to_str().and_then(|n| n.parse::<libc::pid_t>().ok()) else { continue };
        let Ok(status) = std::fs::read_to_string(format!("/proc/{pid}/status")) else { continue };
        let real = status
            .lines()
            .find_map(|l| l.strip_prefix("Uid:"))
            .and_then(|l| l.split_whitespace().next())
            .and_then(|v| v.parse::<u32>().ok());
        if real == Some(uid) {
            out.push(pid);
        }
    }
    out
}

/// Make sure nothing of the dedicated service account `uid` is left (greeter
/// processes may `setsid` out of the process group). `leader` is a child of ours
/// that is reaped here. As an unprivileged process (tests) only `leader` is
/// waited for: the account is then the caller's own and must not be swept.
/// Returns false when something survived SIGKILL for `timeout`.
pub fn retire_user_processes(uid: u32, leader: Option<libc::pid_t>, timeout: Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    let sweep = unsafe { libc::geteuid() } == 0 && uid != 0;
    loop {
        if let Some(l) = leader {
            let mut st = 0;
            unsafe { libc::waitpid(l, &mut st, libc::WNOHANG) };
        }
        let mut alive = false;
        if sweep {
            for pid in pids_of_uid(uid) {
                // A zombie that we have not reaped yet does not count.
                let zombie = std::fs::read_to_string(format!("/proc/{pid}/stat"))
                    .ok()
                    .and_then(|s| s.rsplit_once(')').and_then(|(_, r)| r.split_whitespace().next().map(|c| c == "Z")))
                    .unwrap_or(false);
                if !zombie {
                    alive = true;
                    unsafe { libc::kill(pid, libc::SIGKILL) };
                }
            }
        } else if let Some(l) = leader {
            alive = unsafe { libc::kill(-l, 0) } == 0;
        }
        if !alive {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

pub fn kill(pid: libc::pid_t, sig: libc::c_int) {
    if pid > 0 {
        unsafe { libc::kill(pid, sig) };
    }
}

/// Signal a whole process group.
pub fn kill_group(pgid: libc::pid_t, sig: libc::c_int) {
    if pgid > 1 {
        unsafe { libc::kill(-pgid, sig) };
    }
}

pub fn die_with_parent(sig: libc::c_int) {
    unsafe { libc::prctl(libc::PR_SET_PDEATHSIG, sig as libc::c_ulong, 0, 0, 0) };
}

pub fn set_process_name(name: &str) {
    if let Ok(c) = CString::new(name) {
        unsafe { libc::prctl(libc::PR_SET_NAME, c.as_ptr() as libc::c_ulong, 0, 0, 0) };
    }
}

pub fn euid() -> u32 {
    unsafe { libc::geteuid() }
}

pub fn egid() -> u32 {
    unsafe { libc::getegid() }
}

/// Make this process the user: supplementary groups, gid, uid, in the order
/// that cannot leave a root saved id behind, then prove that root cannot be
/// regained. Without privilege the only identity that can be assumed is the
/// current one (used by the unprivileged test configuration); anything else is
/// an error rather than a silent skip.
pub fn assume_identity(pw: &Passwd) -> io::Result<()> {
    unsafe {
        if libc::geteuid() == 0 {
            let name = str_cstr(&pw.name)?;
            cvt(libc::initgroups(name.as_ptr(), pw.gid))?;
            cvt(libc::setresgid(pw.gid, pw.gid, pw.gid))?;
            cvt(libc::setresuid(pw.uid, pw.uid, pw.uid))?;
        } else if libc::geteuid() != pw.uid || libc::getegid() != pw.gid {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "cannot change identity without privilege",
            ));
        }
        let (mut r, mut e, mut s) = (0, 0, 0);
        libc::getresuid(&mut r, &mut e, &mut s);
        if r != pw.uid || e != pw.uid || s != pw.uid {
            return Err(io::Error::other("uid change incomplete"));
        }
        if pw.uid != 0 && libc::setuid(0) == 0 {
            return Err(io::Error::other("regained root after dropping privileges"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seqpacket_preserves_message_boundaries_and_detects_truncation() {
        let (a, b) = seqpacket_pair().unwrap();
        assert!(send(a.as_raw_fd(), b"one").unwrap());
        assert!(send(a.as_raw_fd(), b"second message").unwrap());
        let mut buf = [0u8; 64];
        assert_eq!(recv(b.as_raw_fd(), &mut buf).unwrap(), Recv::Data(3));
        assert_eq!(&buf[..3], b"one");
        let mut small = [0u8; 4];
        assert_eq!(recv(b.as_raw_fd(), &mut small).unwrap(), Recv::TooBig);
        drop(a);
        assert_eq!(recv(b.as_raw_fd(), &mut buf).unwrap(), Recv::Eof);
    }

    #[test]
    fn peer_cred_reports_this_process() {
        let (a, _b) = seqpacket_pair().unwrap();
        let (pid, uid, _) = peer_cred(a.as_raw_fd()).unwrap();
        assert_eq!(pid, std::process::id());
        assert_eq!(uid, unsafe { libc::getuid() });
    }

    #[test]
    fn random_values_differ() {
        let a = random_u64().unwrap();
        let b = random_u64().unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn passwd_lookup_round_trips_current_user() {
        let uid = unsafe { libc::getuid() };
        let by_uid = getpwuid(uid).unwrap().expect("current uid has a passwd entry");
        let by_name = getpwnam(&by_uid.name).unwrap().unwrap();
        assert_eq!(by_uid, by_name);
        assert!(getpwnam("no-such-account-bsm-test").unwrap().is_none());
    }

    #[test]
    fn listener_socket_has_requested_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("l.sock");
        let _l = listen_seqpacket(&path, 0o600).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let _c = connect_seqpacket(&path).unwrap();
    }

    #[test]
    fn identity_other_than_own_is_refused_without_privilege() {
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let me = getpwuid(unsafe { libc::getuid() }).unwrap().unwrap();
        let mut other = me.clone();
        other.uid += 1;
        assert!(assume_identity(&other).is_err());
        assert!(assume_identity(&me).is_ok());
    }

    #[test]
    fn retire_waits_for_the_group_leader_and_everything_in_its_group() {
        use std::os::unix::process::CommandExt;
        let mut cmd = std::process::Command::new("/bin/sh");
        cmd.arg("-c").arg("sleep 30 & sleep 30");
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
        let child = cmd.spawn().unwrap();
        let pid = child.id() as libc::pid_t;
        std::mem::forget(child);
        std::thread::sleep(Duration::from_millis(200));
        kill_group(pid, libc::SIGKILL);
        assert!(retire_user_processes(unsafe { libc::getuid() }, Some(pid), Duration::from_secs(5)));
        assert!(unsafe { libc::kill(-pid, 0) } != 0, "the group must be empty");
    }
}
