//! The session supervisor: one forked root process per hosted session.
//!
//! It holds the PAM handle for the whole life of the desktop, starts the
//! leader (`gnoblin --hosted`) and the bridge (`bsm-host`) as the user and
//! ends the session when either of them exits, when `CloseSession` arrives or
//! when it is signalled.
//!
//! ```text
//! helper --fork--> supervisor (root)
//!                    pam_start, pam_setcred, pam_open_session   (pam_systemd: logind session
//!                      and scope; the supervisor itself is moved into the scope)
//!                    fork+exec leader as user, fork+exec bridge as user
//!                    ... poll: parent socket, signals, adoption socket ...
//!                    stop everything in the scope, pam_close_session, pam_end
//! ```
//!
//! The supervisor outlives a helper crash or restart on purpose: the desktop
//! and the PAM session must not die because the helper did. It keeps a
//! listening socket so that a restarted helper can adopt it (see `server`).

use std::collections::BTreeMap;
use std::ffi::CString;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use bsm_proto::error::ErrorCode;
use bsm_proto::paths;
use bsm_proto::state::{Generation, SessionId};

use crate::cgroup;
use crate::config::{SessionConfig, Template};
use crate::env::{self, SessionEnvInputs};
use crate::ipc::{self, Incoming, SupCmd, SupEvent};
use crate::pam::Pam;
use crate::sys::{self, Passwd};

pub struct SessionParams {
    pub session: SessionId,
    pub generation: Generation,
    pub pw: Passwd,
    pub cfg: SessionConfig,
    pub state_dir: PathBuf,
    pub host_socket: String,
    pub output_hints: Vec<(u32, u32)>,
    /// cgroup of the helper, to refuse a "scope" that is the helper's own.
    pub helper_cgroup: Option<String>,
    /// Where the cgroup tree is mounted (tests may point elsewhere).
    pub cgroup_root: PathBuf,
}

/// Path of the adoption socket for a session.
pub fn adopt_socket_path(state_dir: &Path, session: &str, generation: Generation) -> PathBuf {
    state_dir.join("sessions").join(format!("{session}-{generation}.sock"))
}

/// Create (or verify) the adoption directory: owned by this process's euid, mode
/// 0700, not a symlink (review F4). The directory's parent is created normally.
pub fn ensure_private_dir(dir: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    if let Some(parent) = dir.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match std::fs::DirBuilder::new().mode(0o700).create(dir) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }
    let fd = sys::open_dir_nofollow(dir)?;
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    sys::cvt(unsafe { libc::fstat(fd.as_raw_fd(), &mut st) })?;
    if st.st_uid != sys::euid() {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "directory has another owner"));
    }
    sys::cvt(unsafe { libc::fchmod(fd.as_raw_fd(), 0o700) })?;
    Ok(())
}

/// Whether a directory and a socket file in it may be trusted as a supervisor's
/// adoption socket: both owned by `euid`, the directory closed to others, the
/// socket mode 0600 and a real socket, none of it reached through a symlink.
pub fn adoption_files_trusted(dir: &Path, sock: &Path, euid: u32) -> Result<(), String> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    let d = std::fs::symlink_metadata(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    if !d.file_type().is_dir() || d.uid() != euid || d.mode() & 0o077 != 0 {
        return Err(format!("{} must be a directory owned by uid {euid} with mode 0700", dir.display()));
    }
    let m = std::fs::symlink_metadata(sock).map_err(|e| format!("{}: {e}", sock.display()))?;
    if !m.file_type().is_socket() || m.uid() != euid || m.mode() & 0o777 != 0o600 {
        return Err(format!("{} is not a socket owned by uid {euid} with mode 0600", sock.display()));
    }
    Ok(())
}

/// Every `(name, pid)` must be in the cgroup `expected` (absolute path).
pub fn verify_in_cgroup(procs: &[(&str, u32)], expected: &str) -> Result<(), String> {
    for (name, pid) in procs {
        match cgroup::path_of(*pid) {
            Ok(p) if p == expected => {}
            Ok(p) => return Err(format!("the {name} (pid {pid}) is in cgroup {p}, not in the session's {expected}")),
            Err(e) => return Err(format!("cannot read the cgroup of the {name} (pid {pid}): {e}")),
        }
    }
    Ok(())
}

/// A session id is `s` plus hex in practice; everything that ends up in a
/// path or an environment variable is limited to a conservative alphabet.
pub fn valid_session_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 32 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// What a parked child needs besides the dynamic `StartParams`.
#[derive(Clone)]
pub struct SessionBase {
    pub pw: Passwd,
    pub cfg: SessionConfig,
    pub state_dir: PathBuf,
    pub helper_cgroup: Option<String>,
    pub cgroup_root: PathBuf,
}

impl SessionBase {
    pub fn with(self, s: crate::ipc::StartParams) -> SessionParams {
        SessionParams {
            session: s.session,
            generation: s.generation,
            pw: self.pw,
            cfg: self.cfg,
            state_dir: self.state_dir,
            host_socket: s.host_socket,
            output_hints: s.output_hints,
            helper_cgroup: self.helper_cgroup,
            cgroup_root: self.cgroup_root,
        }
    }
}

struct Spawned {
    pid: libc::pid_t,
    status: Option<i32>,
}

struct Children {
    leader: Spawned,
    bridge: Spawned,
}

impl Children {
    fn mark(&mut self, pid: libc::pid_t, status: i32) -> bool {
        if pid == self.leader.pid {
            self.leader.status = Some(status);
            true
        } else if pid == self.bridge.pid {
            self.bridge.status = Some(status);
            true
        } else {
            false
        }
    }

    fn reap(&mut self) {
        for (pid, status) in sys::reap_children() {
            self.mark(pid, status);
        }
    }

    /// Status of whichever primary process ended first (leader preferred on a tie).
    fn ended(&self) -> Option<i32> {
        self.leader.status.or(self.bridge.status)
    }
}

/// Fork and exec one process as the user. Failures to exec are reported through
/// a close-on-exec pipe, so a missing binary fails the login instead of
/// becoming a session that ends a moment later.
pub(crate) fn spawn_as_user(
    pw: &Passwd,
    argv: &[String],
    env: &BTreeMap<String, String>,
    cwd: &Path,
    die_signal: libc::c_int,
) -> Result<libc::pid_t, String> {
    let cargv: Vec<CString> = argv.iter().map(|a| sys::str_cstr(a).map_err(|e| e.to_string())).collect::<Result<_, _>>()?;
    let cenv: Vec<CString> =
        env.iter().map(|(k, v)| sys::str_cstr(&format!("{k}={v}")).map_err(|e| e.to_string())).collect::<Result<_, _>>()?;
    let mut ptrs: Vec<*const libc::c_char> = cargv.iter().map(|c| c.as_ptr()).collect();
    ptrs.push(std::ptr::null());
    let mut eptrs: Vec<*const libc::c_char> = cenv.iter().map(|c| c.as_ptr()).collect();
    eptrs.push(std::ptr::null());
    let home = sys::path_cstr(cwd).map_err(|e| e.to_string())?;

    let mut fds = [0 as libc::c_int; 2];
    sys::cvt(unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) }).map_err(|e| e.to_string())?;
    let (rd, wr) = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
    let parent = unsafe { libc::getpid() };
    match unsafe { sys::fork() }.map_err(|e| e.to_string())? {
        0 => {
            drop(rd);
            let fail = |what: &str, err: io::Error| -> ! {
                let msg = format!("{what}: {err}");
                unsafe { libc::write(wr.as_raw_fd(), msg.as_ptr() as *const libc::c_void, msg.len()) };
                unsafe { libc::_exit(127) }
            };
            sys::reset_signals();
            unsafe { libc::setsid() };
            if let Err(e) = sys::assume_identity(pw) {
                fail("cannot assume the user's identity", e);
            }
            unsafe { libc::umask(0o022) };
            // Identity changes clear the parent-death signal, so set it last.
            sys::die_with_parent(die_signal);
            if unsafe { libc::getppid() } != parent {
                unsafe { libc::_exit(127) };
            }
            unsafe {
                let devnull = libc::open(c"/dev/null".as_ptr(), libc::O_RDWR);
                if devnull >= 0 {
                    // Review F5: no inherited stdout or stderr (the helper's journal
                    // stream) for a user process.
                    libc::dup2(devnull, 0);
                    libc::dup2(devnull, 1);
                    libc::dup2(devnull, 2);
                    if devnull > 2 {
                        libc::close(devnull);
                    }
                }
                // Nothing else the helper or a PAM module left open either; the exec
                // error pipe is close-on-exec and must stay until then.
                sys::close_fds_except(&[wr.as_raw_fd()]);
                if libc::chdir(home.as_ptr()) != 0 {
                    libc::chdir(c"/".as_ptr());
                }
                libc::execve(ptrs[0], ptrs.as_ptr(), eptrs.as_ptr());
            }
            fail(&format!("cannot execute {}", argv[0]), io::Error::last_os_error());
        }
        pid => {
            drop(wr);
            let mut buf = [0u8; 512];
            let n = loop {
                let n = unsafe { libc::read(rd.as_raw_fd(), buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
                if n < 0 && io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                break n;
            };
            if n > 0 {
                let _ = sys::wait_pid_timeout(pid, Duration::from_secs(2));
                return Err(String::from_utf8_lossy(&buf[..n as usize]).into_owned());
            }
            Ok(pid)
        }
    }
}

fn runtime_ready(runtime: &Path, uid: u32) -> bool {
    use std::os::unix::fs::MetadataExt;
    let Ok(m) = std::fs::metadata(runtime) else { return false };
    m.is_dir() && m.uid() == uid && runtime.join("bus").exists()
}

/// Entry point of the supervisor: the parked authentication child that was
/// told to start the session. `pam` is the authenticated handle; `phase` makes
/// its conversation refuse prompts from now on. Returns the exit status.
pub fn supervisor_main(sock: OwnedFd, pam: Pam, phase: &std::cell::Cell<bool>, p: SessionParams) -> i32 {
    phase.set(true);
    // Unlike the authentication child, the supervisor must survive the helper.
    sys::die_with_parent(0);
    unsafe {
        libc::setsid();
        libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1 as libc::c_ulong, 0, 0, 0);
    }
    sys::set_process_name("bsm-session");
    let sigfd = match sys::signal_fd(&[libc::SIGCHLD, libc::SIGTERM, libc::SIGINT]) {
        Ok(f) => f,
        Err(e) => {
            let _ = ipc::send_msg(sock.as_raw_fd(), &SupEvent::Failed { code: ErrorCode::Internal, message: e.to_string() });
            return 1;
        }
    };
    let mut sup = Supervisor { parent: Some(sock), p, listener: None, adopt_path: None };
    sup.run(pam, sigfd)
}

struct Supervisor {
    parent: Option<OwnedFd>,
    p: SessionParams,
    listener: Option<OwnedFd>,
    adopt_path: Option<PathBuf>,
}

impl Supervisor {
    fn send_parent(&mut self, ev: &SupEvent) {
        if let Some(fd) = &self.parent {
            match ipc::send_msg(fd.as_raw_fd(), ev) {
                Ok(true) => {}
                _ => self.parent = None,
            }
        }
    }

    fn fail(&mut self, code: ErrorCode, message: impl Into<String>) -> i32 {
        let message = message.into();
        eprintln!("bsm-helper: session {} failed to start: {message}", self.p.session);
        self.send_parent(&SupEvent::Failed { code, message });
        1
    }

    fn run(&mut self, mut pam: Pam, sigfd: OwnedFd) -> i32 {
        let pw = self.p.pw.clone();
        let cfg = self.p.cfg.clone();
        let session_id = self.p.session.clone();
        let locale = std::fs::read_to_string(&cfg.locale_conf).map(|t| env::parse_locale_conf(&t)).unwrap_or_default();
        let inputs = SessionEnvInputs {
            pw: &pw,
            session_id: &session_id,
            generation: self.p.generation,
            path: &cfg.path,
            session_desktop: &cfg.session_desktop,
            current_desktop: &cfg.current_desktop,
            locale: &locale,
            extra: &cfg.extra_env,
        };
        // `pam` is the handle that authenticated this login: credentials and the
        // session are established on it, as PAM requires, and modules that kept
        // the authentication token (keyring unlock) can use it.
        for var in env::pam_session_vars(&inputs) {
            if let Err(e) = pam.put_env(&var) {
                return self.fail(ErrorCode::PamFailure, format!("pam_putenv: {e}"));
            }
        }
        if let Err(e) = pam.establish_credentials() {
            return self.fail(ErrorCode::PamFailure, format!("pam_setcred: {e}"));
        }
        if let Err(e) = pam.open_session() {
            let _ = pam.delete_credentials();
            return self.fail(ErrorCode::PamFailure, format!("pam_open_session: {e}"));
        }
        // Review F3: the session phase must still be for the account we run as.
        if let Err(m) = crate::auth::check_user(&pam, self.p.pw.uid) {
            let _ = pam.close_session();
            let _ = pam.delete_credentials();
            return self.fail(ErrorCode::PamFailure, m);
        }

        // From here on the PAM session is open and must be closed on every exit path.
        let outcome = self.after_open(&mut pam, &inputs, &sigfd);
        let _ = pam.close_session();
        let _ = pam.delete_credentials();
        drop(pam);
        if let Some(path) = &self.adopt_path {
            let _ = std::fs::remove_file(path);
        }
        match outcome {
            Ok(status) => {
                self.send_parent(&SupEvent::Exited { status });
                0
            }
            Err((code, message)) => self.fail(code, message),
        }
    }

    fn after_open(&mut self, pam: &mut Pam, inputs: &SessionEnvInputs<'_>, sigfd: &OwnedFd) -> Result<i32, (ErrorCode, String)> {
        let pam_env = pam.env_list();
        let logind_session = pam.get_env("XDG_SESSION_ID");
        let env_map = env::session_env(inputs, &pam_env);
        let runtime = PathBuf::from(&env_map["XDG_RUNTIME_DIR"]);

        let my_cgroup = cgroup::path_of(std::process::id())
            .map_err(|e| (ErrorCode::LogindFailure, format!("cannot read own cgroup: {e}")))?;
        let own_scope = cgroup::scope_unit(&my_cgroup).filter(|_| Some(&my_cgroup) != self.p.helper_cgroup.as_ref());
        if own_scope.is_none() && self.p.cfg.require_scope {
            return Err((
                ErrorCode::LogindFailure,
                format!("pam_open_session did not move the session into a systemd scope (cgroup {my_cgroup}); is pam_systemd in the stack?"),
            ));
        }
        let scope_cgroup = own_scope.map(|_| my_cgroup.clone());
        // Reported as the ABSOLUTE cgroup path (`/user.slice/user-1000.slice/session-5.scope`):
        // a bare unit name could be satisfied by any same-uid process that merely names it.
        let scope_name = my_cgroup.clone();

        if self.p.cfg.require_user_runtime {
            let deadline = Instant::now() + Duration::from_secs(self.p.cfg.runtime_wait_secs);
            while !runtime_ready(&runtime, self.p.pw.uid) {
                if Instant::now() >= deadline {
                    return Err((
                        ErrorCode::LogindFailure,
                        format!("{} did not appear (user manager or D-Bus socket missing)", runtime.display()),
                    ));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }

        let tpl = Template {
            wayland_display: &format!("bsm-{}", self.p.session),
            session_id: &self.p.session,
            generation: self.p.generation,
            runtime_dir: &env_map["XDG_RUNTIME_DIR"],
            uid: self.p.pw.uid,
            user: &self.p.pw.name,
        };
        let leader_argv = tpl.expand(&self.p.cfg.gnoblin);
        let bridge_argv = tpl.expand(&self.p.cfg.bridge);
        let mut bridge_env = env_map.clone();
        bridge_env.insert(paths::ENV_HOST_SOCKET.to_owned(), self.p.host_socket.clone());
        if !self.p.output_hints.is_empty() {
            let hints: Vec<String> = self.p.output_hints.iter().map(|(w, h)| format!("{w}x{h}")).collect();
            bridge_env.insert("BSM_OUTPUT_HINTS".to_owned(), hints.join(","));
        }

        let leader = spawn_as_user(&self.p.pw, &leader_argv, &env_map, &self.p.pw.home, libc::SIGKILL)
            .map_err(|e| (ErrorCode::HelperFailure, format!("leader: {e}")))?;
        let bridge = match spawn_as_user(&self.p.pw, &bridge_argv, &bridge_env, &self.p.pw.home, libc::SIGKILL) {
            Ok(p) => p,
            Err(e) => {
                let mut ch = Children { leader: Spawned { pid: leader, status: None }, bridge: Spawned { pid: 0, status: None } };
                self.terminate(&mut ch, scope_cgroup.as_deref(), 1000);
                return Err((ErrorCode::HelperFailure, format!("bridge: {e}")));
            }
        };
        let mut ch = Children { leader: Spawned { pid: leader, status: None }, bridge: Spawned { pid: bridge, status: None } };

        // Never report a host_pid that is not in the cgroup being reported: the
        // compositor trusts the pair when it checks the bridge's connection.
        if let Err(m) = verify_in_cgroup(&[("leader", leader as u32), ("bridge", bridge as u32)], &my_cgroup) {
            self.terminate(&mut ch, scope_cgroup.as_deref(), 1000);
            return Err((ErrorCode::LogindFailure, m));
        }

        // The adoption socket exists before the parent hears about the session,
        // so a helper that restarts right now can still find it.
        let path = adopt_socket_path(&self.p.state_dir, &self.p.session, self.p.generation);
        if let Some(dir) = path.parent()
            && let Err(e) = ensure_private_dir(dir)
        {
            eprintln!("bsm-helper: adoption directory unusable: {e}");
        }
        match sys::listen_seqpacket(&path, 0o600) {
            Ok(l) => {
                self.listener = Some(l);
                self.adopt_path = Some(path);
            }
            Err(e) => eprintln!("bsm-helper: adoption socket unavailable: {e}"),
        }

        let opened = SupEvent::Opened {
            session: self.p.session.clone(),
            generation: self.p.generation,
            supervisor_pid: std::process::id(),
            leader_pid: leader as u32,
            host_pid: bridge as u32,
            scope: scope_name,
            logind_session,
        };
        self.send_parent(&opened);

        let grace = self.p.cfg.default_grace_ms;
        let mut close_grace: Option<u32> = None;
        loop {
            ch.reap();
            if ch.ended().is_some() || close_grace.is_some() {
                break;
            }
            let mut fds = vec![libc::pollfd { fd: sigfd.as_raw_fd(), events: libc::POLLIN, revents: 0 }];
            let parent_idx = self.parent.as_ref().map(|f| {
                fds.push(libc::pollfd { fd: f.as_raw_fd(), events: libc::POLLIN, revents: 0 });
                fds.len() - 1
            });
            let listener_idx = self.listener.as_ref().map(|f| {
                fds.push(libc::pollfd { fd: f.as_raw_fd(), events: libc::POLLIN, revents: 0 });
                fds.len() - 1
            });
            let r = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, -1) };
            if r < 0 {
                continue;
            }
            if fds[0].revents != 0 {
                for sig in sys::read_signals(sigfd.as_raw_fd()) {
                    if sig == libc::SIGTERM || sig == libc::SIGINT {
                        close_grace = Some(grace);
                    }
                }
            }
            if let Some(i) = parent_idx
                && fds[i].revents != 0
                && let Some(fd) = self.parent.as_ref().map(|f| f.as_raw_fd())
            {
                loop {
                    match ipc::recv_msg::<SupCmd>(fd) {
                        Ok(Incoming::Msg(SupCmd::Close { grace_ms })) => close_grace = Some(grace_ms),
                        Ok(Incoming::WouldBlock) => break,
                        Ok(Incoming::Bad) => continue,
                        // Helper gone. The session keeps running and can be adopted.
                        Ok(Incoming::Eof) | Err(_) => {
                            self.parent = None;
                            break;
                        }
                    }
                    // Blocking socket: take one command per wakeup.
                    break;
                }
            }
            if let Some(i) = listener_idx
                && fds[i].revents != 0
            {
                self.accept_adopter(&opened);
            }
        }

        let status = ch.ended();
        self.terminate(&mut ch, scope_cgroup.as_deref(), close_grace.unwrap_or(grace));
        Ok(status.or(ch.leader.status).or(ch.bridge.status).unwrap_or(0))
    }

    /// A restarted helper connects to the adoption socket. Only root may.
    fn accept_adopter(&mut self, opened: &SupEvent) {
        let Some(l) = &self.listener else { return };
        let Ok(Some(fd)) = sys::accept(l.as_raw_fd()) else { return };
        let uid_ok = sys::peer_cred(fd.as_raw_fd()).map(|(_, uid, _)| uid == sys::euid()).unwrap_or(false);
        if !uid_ok {
            return;
        }
        self.parent = Some(fd);
        self.send_parent(opened);
    }

    /// Stop every process of the session, then wait until none is left.
    /// Only this session's scope is touched.
    ///
    /// SIGTERM to the members, a grace period, then `cgroup.kill` on the scope
    /// (review F6): the kernel kills the whole subtree atomically, so a fork loop
    /// cannot stay ahead of it and no pid is signalled from a stale listing. The
    /// supervisor lives in the scope itself (pam_systemd put it there), so it first
    /// moves into the helper's cgroup. Without `cgroup.kill` (kernel before 5.14) or
    /// without a scope the fallback is a tight signal sweep.
    fn terminate(&mut self, ch: &mut Children, scope_cgroup: Option<&str>, grace_ms: u32) {
        let me = std::process::id();
        let root = self.p.cgroup_root.clone();
        let members = |ch: &mut Children| -> Vec<u32> {
            ch.reap();
            match scope_cgroup {
                Some(cg) => cgroup::procs(&root, cg).unwrap_or_default().into_iter().filter(|p| *p != me).collect(),
                None => [&ch.leader, &ch.bridge]
                    .iter()
                    .filter(|s| s.pid > 0 && s.status.is_none())
                    .map(|s| s.pid as u32)
                    .collect(),
            }
        };
        // Signal pids, and in the development configuration (no scope) the process
        // groups of children that have NOT been reaped: a reaped pid may be reused.
        let signal = |ch: &Children, pids: &[u32], sig: libc::c_int| {
            for pid in pids {
                sys::kill(*pid as libc::pid_t, sig);
            }
            if scope_cgroup.is_none() {
                for s in [&ch.leader, &ch.bridge] {
                    if s.pid > 0 && s.status.is_none() {
                        sys::kill_group(s.pid, sig);
                    }
                }
            }
        };
        let pids = members(ch);
        if pids.is_empty() && scope_cgroup.is_none() {
            return;
        }
        signal(ch, &pids, libc::SIGTERM);
        let deadline = Instant::now() + Duration::from_millis(grace_ms as u64);
        while Instant::now() < deadline {
            if members(ch).is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        if let Some(cg) = scope_cgroup {
            let moved = self.p.helper_cgroup.as_deref().is_some_and(|dest| cgroup::move_pid(&root, dest, me).is_ok());
            if moved && cgroup::kill(&root, cg).is_ok() {
                let end = Instant::now() + Duration::from_secs(5);
                loop {
                    ch.reap();
                    // A scope that systemd already removed is as good as empty.
                    if cgroup::populated(&root, cg) != Some(true) {
                        return;
                    }
                    if Instant::now() >= end {
                        eprintln!("bsm-helper: scope {cg} still populated after cgroup.kill in session {}", self.p.session);
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
            if moved {
                eprintln!("bsm-helper: cgroup.kill unavailable for {cg}; falling back to a signal sweep");
            }
        }
        // Fallback: sweep without sleeping between listings.
        let end = Instant::now() + Duration::from_secs(5);
        loop {
            let pids = members(ch);
            if pids.is_empty() {
                return;
            }
            signal(ch, &pids, libc::SIGKILL);
            if Instant::now() >= end {
                eprintln!("bsm-helper: processes survived SIGKILL in session {}: {pids:?}", self.p.session);
                return;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

/// Helper-side adoption: connect to a supervisor's socket. `Ok(None)` if nothing listens.
pub fn connect_adopt(path: &Path) -> io::Result<Option<OwnedFd>> {
    match sys::connect_seqpacket(path) {
        Ok(fd) => Ok(Some(fd)),
        Err(e) if matches!(e.raw_os_error(), Some(libc::ECONNREFUSED) | Some(libc::ENOENT)) => Ok(None),
        Err(e) => Err(e),
    }
}

pub fn raw(fd: &OwnedFd) -> RawFd {
    fd.as_raw_fd()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_ids_are_restricted() {
        assert!(valid_session_id("s0000abcd"));
        assert!(!valid_session_id(""));
        assert!(!valid_session_id("a/b"));
        assert!(!valid_session_id("a b"));
        assert!(!valid_session_id(&"x".repeat(33)));
    }

    #[test]
    fn adoption_socket_path_is_inside_the_state_dir() {
        let p = adopt_socket_path(Path::new("/run/bsm-helper"), "s1", 7);
        assert_eq!(p, PathBuf::from("/run/bsm-helper/sessions/s1-7.sock"));
    }

    #[test]
    fn adoption_trusts_only_private_directories_and_own_sockets() {
        use std::os::unix::fs::PermissionsExt;
        let me = sys::euid();
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("sessions");
        ensure_private_dir(&dir).unwrap();
        assert_eq!(std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
        let sock = dir.join("s-1.sock");
        let _l = sys::listen_seqpacket(&sock, 0o600).unwrap();
        assert!(adoption_files_trusted(&dir, &sock, me).is_ok());
        // Another uid expected: owner mismatch.
        assert!(adoption_files_trusted(&dir, &sock, me + 1).is_err());
        // A directory others can write to (the wrong-umask case).
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(adoption_files_trusted(&dir, &sock, me).is_err());
        // ensure_private_dir repairs a loose mode it owns.
        ensure_private_dir(&dir).unwrap();
        assert!(adoption_files_trusted(&dir, &sock, me).is_ok());
        // A wider socket mode, a regular file and a symlink are refused.
        std::fs::set_permissions(&sock, std::fs::Permissions::from_mode(0o666)).unwrap();
        assert!(adoption_files_trusted(&dir, &sock, me).is_err());
        std::fs::set_permissions(&sock, std::fs::Permissions::from_mode(0o600)).unwrap();
        let file = dir.join("plain.sock");
        std::fs::write(&file, "x").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(adoption_files_trusted(&dir, &file, me).is_err());
        let link = dir.join("link.sock");
        std::os::unix::fs::symlink(&sock, &link).unwrap();
        assert!(adoption_files_trusted(&dir, &link, me).is_err());
        // The directory itself as a symlink is refused too.
        let linkdir = tmp.path().join("linkdir");
        std::os::unix::fs::symlink(&dir, &linkdir).unwrap();
        assert!(ensure_private_dir(&linkdir).is_err());
        assert!(adoption_files_trusted(&linkdir, &sock, me).is_err());
    }

    #[test]
    fn verify_in_cgroup_accepts_members_and_names_the_offender() {
        let me = std::process::id();
        let mine = cgroup::path_of(me).unwrap();
        assert!(verify_in_cgroup(&[("self", me)], &mine).is_ok());
        let e = verify_in_cgroup(&[("self", me)], "/user.slice/other.scope").unwrap_err();
        assert!(e.contains("self") && e.contains("other.scope"), "{e}");
        assert!(verify_in_cgroup(&[("gone", u32::MAX - 1)], &mine).is_err());
    }
}
