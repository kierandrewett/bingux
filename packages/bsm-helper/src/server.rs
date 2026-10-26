//! The helper's single-threaded event loop.
//!
//! One connection (the `bsm` account) speaks `bsm_proto::helper`. Everything
//! that touches PAM or the user's identity runs in a forked child; this loop
//! only relays, keeps tickets and reaps. It makes no policy decisions beyond
//! refusing what could not be a legitimate request (unknown PAM service,
//! system accounts, paths outside the configured directories).

use std::collections::VecDeque;
use std::io;
use std::os::fd::{AsRawFd, OwnedFd, RawFd};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use bsm_proto::error::ErrorCode;
use bsm_proto::helper::{HelperEvent, HelperRequest, MAX_DATAGRAM};
use bsm_proto::state::{AuthIntent, Generation, SessionId};
use zeroize::Zeroize;

use crate::auth::{self, AuthParams};
use crate::config::Config;
use crate::greeter::{self, GreeterEnvInputs, GreeterRun};
use crate::ipc::{self, AuthChildEvent, AuthParentCmd, Incoming, StartParams, SupCmd, SupEvent};
use crate::session::{self, SessionBase};
use crate::sys::{self, Passwd, Recv};
use crate::ticket::{RedeemError, Tickets};
use crate::{cgroup, env as sessenv, logind};

struct Conn {
    fd: OwnedFd,
    out: VecDeque<Vec<u8>>,
}

struct AuthConv {
    req_id: u64,
    pid: libc::pid_t,
    sock: OwnedFd,
    user: String,
    uid: u32,
    intent: AuthIntent,
    generation: Generation,
    /// May become a session (Login or Switch): the child parks after success.
    startable: bool,
    deadline: Instant,
    /// A prompt is waiting for a response. Responses at any other time are dropped.
    awaiting: bool,
}

/// An authenticated child waiting for its ticket to be redeemed. It holds the
/// PAM handle that authenticated the login. Dropping it closes the socket, which
/// ends the child; there is no pid to signal, so a recycled pid cannot be hit.
struct Parked {
    sock: OwnedFd,
    pid: libc::pid_t,
}

/// An aborted authentication child that may still be finishing.
struct Dying {
    pid: libc::pid_t,
    kill_at: Instant,
}

struct SessionEntry {
    session: SessionId,
    generation: Generation,
    /// Request waiting for `SessionOpened`/`SessionFailed`; none for an adopted session.
    req_id: Option<u64>,
    sup_pid: Option<libc::pid_t>,
    sup_status: Option<i32>,
    sock: OwnedFd,
    opened: bool,
    logind_session: Option<String>,
    start_deadline: Option<Instant>,
}

struct GreeterEntry {
    generation: Generation,
    pid: libc::pid_t,
    run: GreeterRun,
    kill_at: Option<Instant>,
}

struct HintJob {
    pid: libc::pid_t,
    req_id: u64,
}

pub struct Server {
    cfg: Config,
    listener: OwnedFd,
    sigfd: OwnedFd,
    allowed_uid: u32,
    conn: Option<Conn>,
    auths: Vec<AuthConv>,
    dying: Vec<Dying>,
    tickets: Tickets<Option<Parked>>,
    sessions: Vec<SessionEntry>,
    greeter: Option<GreeterEntry>,
    hints: Vec<HintJob>,
    helper_cgroup: Option<String>,
    shutdown: bool,
}

#[derive(Clone, Copy)]
enum Src {
    Signal,
    Listener,
    Conn,
    Auth(usize),
    Session(usize),
}

/// Obtain the listening socket: the one systemd passes, or a fresh one at `path`.
pub fn acquire_listener(cfg: &Config, allowed_uid: u32) -> io::Result<OwnedFd> {
    use std::os::fd::FromRawFd;
    let pid_ok = std::env::var("LISTEN_PID").ok().and_then(|p| p.parse::<u32>().ok()) == Some(std::process::id());
    let fds = std::env::var("LISTEN_FDS").ok().and_then(|p| p.parse::<i32>().ok()).unwrap_or(0);
    if pid_ok && fds >= 1 {
        // SD_LISTEN_FDS_START. The descriptor is ours from here on.
        let fd = unsafe { OwnedFd::from_raw_fd(3) };
        let mut ty: libc::c_int = 0;
        let mut len = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
        sys::cvt(unsafe {
            libc::getsockopt(fd.as_raw_fd(), libc::SOL_SOCKET, libc::SO_TYPE, &mut ty as *mut _ as *mut libc::c_void, &mut len)
        })?;
        if ty != libc::SOCK_SEQPACKET {
            return Err(io::Error::other("inherited socket is not SOCK_SEQPACKET"));
        }
        sys::cvt(unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) })?;
        sys::set_nonblocking(fd.as_raw_fd(), true)?;
        unsafe {
            std::env::remove_var("LISTEN_PID");
            std::env::remove_var("LISTEN_FDS");
            std::env::remove_var("LISTEN_FDNAMES");
        }
        return Ok(fd);
    }
    if let Some(dir) = cfg.socket.path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let fd = sys::listen_seqpacket(&cfg.socket.path, 0o600)?;
    if sys::euid() == 0 {
        std::os::unix::fs::chown(&cfg.socket.path, Some(allowed_uid), None)?;
    }
    Ok(fd)
}

/// Resolve the uid allowed to connect.
pub fn resolve_allowed_uid(cfg: &Config) -> Result<u32, String> {
    if let Some(uid) = cfg.socket.allowed_uid {
        return Ok(uid);
    }
    match sys::getpwnam(&cfg.socket.allowed_user) {
        Ok(Some(pw)) => Ok(pw.uid),
        Ok(None) => Err(format!("account {:?} does not exist", cfg.socket.allowed_user)),
        Err(e) => Err(format!("looking up {:?}: {e}", cfg.socket.allowed_user)),
    }
}

impl Server {
    pub fn new(cfg: Config, listener: OwnedFd, allowed_uid: u32) -> io::Result<Server> {
        let sigfd = sys::signal_fd(&[libc::SIGCHLD, libc::SIGTERM, libc::SIGINT])?;
        unsafe { libc::signal(libc::SIGPIPE, libc::SIG_IGN) };
        let tickets = Tickets::new(cfg.ticket_ttl());
        let helper_cgroup = cgroup::path_of(std::process::id()).ok();
        let mut s = Server {
            cfg,
            listener,
            sigfd,
            allowed_uid,
            conn: None,
            auths: Vec::new(),
            dying: Vec::new(),
            tickets,
            sessions: Vec::new(),
            greeter: None,
            hints: Vec::new(),
            helper_cgroup,
            shutdown: false,
        };
        greeter::clean_stale(&s.cfg.greeter.runtime_root);
        s.adopt_running_sessions();
        Ok(s)
    }

    // ---- adoption -------------------------------------------------------

    /// Sessions whose supervisor survived a helper restart announce themselves
    /// on their adoption socket. Adopted sessions can be closed and report
    /// their exit like any other.
    fn adopt_running_sessions(&mut self) {
        let dir = self.cfg.state_dir.join("sessions");
        let Ok(rd) = std::fs::read_dir(&dir) else { return };
        for entry in rd.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("sock") {
                continue;
            }
            let fd = match session::connect_adopt(&path) {
                Ok(Some(fd)) => fd,
                Ok(None) => {
                    let _ = std::fs::remove_file(&path);
                    continue;
                }
                Err(e) => {
                    eprintln!("bsm-helper: adoption of {} failed: {e}", path.display());
                    continue;
                }
            };
            // The supervisor answers with its `Opened` description. Waiting here
            // keeps adoption complete before the first request can arrive.
            let hello = match sys::wait_readable(fd.as_raw_fd(), Duration::from_secs(2)) {
                Ok(true) => ipc::recv_msg::<SupEvent>(fd.as_raw_fd()),
                _ => continue,
            };
            let Ok(Incoming::Msg(SupEvent::Opened { session, generation, logind_session, .. })) = hello else {
                eprintln!("bsm-helper: {} did not announce a session", path.display());
                continue;
            };
            let _ = sys::set_nonblocking(fd.as_raw_fd(), true);
            eprintln!("bsm-helper: adopted session {session} generation {generation}");
            self.sessions.push(SessionEntry {
                session,
                generation,
                req_id: None,
                sup_pid: None,
                sup_status: None,
                sock: fd,
                opened: true,
                logind_session,
                start_deadline: None,
            });
        }
    }

    // ---- output ---------------------------------------------------------

    fn emit(&mut self, ev: HelperEvent) {
        let Some(conn) = &mut self.conn else { return };
        let Ok(bytes) = serde_json::to_vec(&ev) else { return };
        if !conn.out.is_empty() {
            conn.out.push_back(bytes);
        } else {
            match sys::send(conn.fd.as_raw_fd(), &bytes) {
                Ok(true) => {}
                Ok(false) => conn.out.push_back(bytes),
                Err(_) => {
                    self.drop_connection("send failed");
                    return;
                }
            }
        }
        if self.conn.as_ref().is_some_and(|c| c.out.len() > 1024) {
            self.drop_connection("peer does not read");
        }
    }

    fn flush(&mut self) {
        let Some(conn) = &mut self.conn else { return };
        while let Some(front) = conn.out.front() {
            match sys::send(conn.fd.as_raw_fd(), front) {
                Ok(true) => {
                    conn.out.pop_front();
                }
                Ok(false) => return,
                Err(_) => {
                    self.drop_connection("send failed");
                    return;
                }
            }
        }
    }

    // ---- connection -----------------------------------------------------

    fn accept_connections(&mut self) {
        loop {
            let fd = match sys::accept(self.listener.as_raw_fd()) {
                Ok(Some(fd)) => fd,
                Ok(None) => return,
                Err(e) => {
                    eprintln!("bsm-helper: accept: {e}");
                    return;
                }
            };
            match sys::peer_cred(fd.as_raw_fd()) {
                Ok((_, uid, _)) if uid == self.allowed_uid => {}
                Ok((pid, uid, _)) => {
                    eprintln!("bsm-helper: rejected connection from pid {pid} uid {uid}");
                    continue;
                }
                Err(e) => {
                    eprintln!("bsm-helper: peer credentials: {e}");
                    continue;
                }
            }
            // The old peer may have closed an instant ago without poll having
            // reported it yet: look before turning the newcomer away.
            if self.conn.is_some() {
                self.read_requests();
            }
            if self.conn.is_some() {
                eprintln!("bsm-helper: rejected a second connection while one is active");
                continue;
            }
            if sys::set_nonblocking(fd.as_raw_fd(), true).is_err() {
                continue;
            }
            self.conn = Some(Conn { fd, out: VecDeque::new() });
        }
    }

    /// The peer is gone (or misbehaving). Nothing it started may outlive its
    /// connection except desktop sessions: pending conversations and tickets
    /// are discarded and the greeter, which cannot work without bsm, is stopped.
    fn drop_connection(&mut self, why: &str) {
        eprintln!("bsm-helper: connection closed: {why}");
        self.conn = None;
        for conv in std::mem::take(&mut self.auths) {
            self.kill_auth_child(conv.pid);
        }
        self.tickets.clear();
        if let Some(g) = &mut self.greeter {
            sys::kill_group(g.pid, libc::SIGTERM);
            g.kill_at.get_or_insert(Instant::now() + Duration::from_secs(2));
        }
    }

    fn kill_auth_child(&mut self, pid: libc::pid_t) {
        sys::kill(pid, libc::SIGKILL);
        self.dying.push(Dying { pid, kill_at: Instant::now() });
    }

    fn read_requests(&mut self) {
        loop {
            let Some(conn) = &self.conn else { return };
            let fd = conn.fd.as_raw_fd();
            let mut buf = vec![0u8; MAX_DATAGRAM + 1];
            let got = sys::recv(fd, &mut buf);
            match got {
                Ok(Recv::Data(n)) if n > MAX_DATAGRAM => {
                    buf.zeroize();
                    self.drop_connection("oversized datagram");
                    return;
                }
                Ok(Recv::Data(n)) => {
                    let parsed = serde_json::from_slice::<HelperRequest>(&buf[..n]);
                    buf.zeroize();
                    match parsed {
                        Ok(req) => self.handle(req),
                        Err(e) => eprintln!("bsm-helper: undecodable request ignored: {}", e.classify_str()),
                    }
                }
                Ok(Recv::WouldBlock) => return,
                Ok(Recv::Eof) => {
                    self.drop_connection("peer closed");
                    return;
                }
                Ok(Recv::TooBig) => {
                    buf.zeroize();
                    self.drop_connection("oversized datagram");
                    return;
                }
                Err(e) => {
                    self.drop_connection(&format!("recv: {e}"));
                    return;
                }
            }
        }
    }

    // ---- requests -------------------------------------------------------

    fn handle(&mut self, req: HelperRequest) {
        match req {
            HelperRequest::Ping { nonce } => self.emit(HelperEvent::Pong { nonce }),
            HelperRequest::AuthStart { req_id, user, uid, intent, generation, service } => {
                self.auth_start(req_id, user, uid, intent, generation, service)
            }
            HelperRequest::AuthResponse { req_id, mut response } => {
                self.auth_response(req_id, &response);
                response.zeroize();
            }
            HelperRequest::AuthAbort { req_id } => {
                if let Some(i) = self.auths.iter().position(|a| a.req_id == req_id) {
                    self.abort_auth(i, ErrorCode::Cancelled, "authentication cancelled");
                }
            }
            HelperRequest::OpenSession { req_id, ticket, session, generation, socket_path, output_hints } => {
                self.open_session(req_id, ticket, session, generation, socket_path, output_hints)
            }
            HelperRequest::CloseSession { session, generation, grace_ms } => {
                match self.sessions.iter().find(|s| s.session == session && s.generation == generation) {
                    Some(s) => {
                        let _ = ipc::send_msg(s.sock.as_raw_fd(), &SupCmd::Close { grace_ms });
                    }
                    None => eprintln!("bsm-helper: CloseSession for unknown session {session} generation {generation}"),
                }
            }
            HelperRequest::SpawnGreeter { req_id, generation, wayland_socket, control_socket } => {
                self.spawn_greeter(req_id, generation, wayland_socket, control_socket)
            }
            HelperRequest::KillGreeter { generation } => {
                if let Some(g) = self.greeter.as_mut().filter(|g| g.generation == generation) {
                    sys::kill_group(g.pid, libc::SIGTERM);
                    g.kill_at.get_or_insert(Instant::now() + Duration::from_secs(2));
                }
            }
            HelperRequest::SetLockedHint { req_id, session, generation, locked } => {
                self.set_locked_hint(req_id, session, generation, locked)
            }
        }
    }

    /// Only accounts a person can sign in to: not root, not below `min_uid`,
    /// and never the compositor's or the greeter's own account.
    fn account_allowed(&self, pw: &Passwd) -> Result<(), String> {
        if pw.uid == 0 {
            return Err("the root account cannot sign in through bsm".into());
        }
        if pw.uid < self.cfg.session.min_uid {
            return Err(format!("uid {} is below the minimum {}", pw.uid, self.cfg.session.min_uid));
        }
        // Without root the helper can only ever start its own uid, which is
        // also the connecting account's: there is no boundary to protect and
        // the unprivileged test configuration depends on it.
        let service = pw.uid == self.allowed_uid || pw.name == self.cfg.greeter.user || pw.name == self.cfg.socket.allowed_user;
        if service && sys::euid() == 0 {
            return Err("service accounts cannot sign in".into());
        }
        Ok(())
    }

    fn auth_start(&mut self, req_id: u64, user: String, uid: u32, intent: AuthIntent, generation: Generation, service: String) {
        let done_err = |this: &mut Self, code: ErrorCode, message: &str| {
            this.emit(HelperEvent::AuthDone { req_id, ok: false, code: Some(code), message: message.to_owned(), ticket: None })
        };
        // Login and Switch authenticate on the login stack, because the same PAM
        // handle opens the session if the ticket is redeemed; the manager decides
        // afterwards whether that is a new login or a resume. Unlock has its own
        // stack with no session phase.
        let expected = match intent {
            AuthIntent::Login | AuthIntent::Switch => &self.cfg.pam.login_service,
            AuthIntent::Unlock => &self.cfg.pam.unlock_service,
        };
        if &service != expected {
            return done_err(self, ErrorCode::InvalidArgument, "PAM service not permitted for this intent");
        }
        if self.auths.iter().any(|a| a.req_id == req_id) {
            return done_err(self, ErrorCode::InvalidArgument, "duplicate request id");
        }
        let pw = match sys::getpwnam(&user) {
            Ok(Some(pw)) if pw.uid == uid => pw,
            Ok(_) => return done_err(self, ErrorCode::InvalidAccount, "unknown account"),
            Err(_) => return done_err(self, ErrorCode::Internal, "account lookup failed"),
        };
        if let Err(m) = self.account_allowed(&pw) {
            return done_err(self, ErrorCode::InvalidAccount, &m);
        }
        // One live conversation at a time: starting another supersedes the old one.
        while !self.auths.is_empty() {
            self.abort_auth(0, ErrorCode::Cancelled, "superseded by a new authentication");
        }
        let (parent_sock, child_sock) = match sys::seqpacket_pair() {
            Ok(p) => p,
            Err(e) => return done_err(self, ErrorCode::Internal, &format!("socketpair: {e}")),
        };
        let params = AuthParams {
            service,
            user: pw.name.clone(),
            confdir: self.cfg.pam.confdir.clone(),
            prompt_timeout: Duration::from_secs(self.cfg.pam.prompt_timeout_secs),
        };
        // Only logins can become sessions; an unlock child ends with its verdict.
        let startable = matches!(intent, AuthIntent::Login | AuthIntent::Switch);
        let base = startable.then(|| SessionBase {
            pw: pw.clone(),
            cfg: self.cfg.session.clone(),
            state_dir: self.cfg.state_dir.clone(),
            helper_cgroup: self.helper_cgroup.clone(),
            cgroup_root: PathBuf::from(cgroup::CGROUP_ROOT),
        });
        let parent_pid = unsafe { libc::getpid() };
        match unsafe { sys::fork() } {
            Ok(0) => {
                sys::reset_signals();
                sys::close_fds_except(&[child_sock.as_raw_fd()]);
                sys::die_with_parent(libc::SIGKILL);
                if unsafe { libc::getppid() } != parent_pid {
                    unsafe { libc::_exit(1) };
                }
                sys::set_process_name("bsm-auth");
                let code = auth::child_main(child_sock, &params, base);
                unsafe { libc::_exit(code) };
            }
            Ok(pid) => {
                drop(child_sock);
                let _ = sys::set_nonblocking(parent_sock.as_raw_fd(), true);
                self.auths.push(AuthConv {
                    req_id,
                    pid,
                    sock: parent_sock,
                    user: pw.name,
                    uid: pw.uid,
                    intent,
                    generation,
                    startable,
                    deadline: Instant::now() + Duration::from_secs(self.cfg.pam.conversation_timeout_secs),
                    awaiting: false,
                });
            }
            Err(e) => done_err(self, ErrorCode::Internal, &format!("fork: {e}")),
        }
    }

    fn auth_response(&mut self, req_id: u64, response: &str) {
        let Some(conv) = self.auths.iter_mut().find(|a| a.req_id == req_id) else {
            // Stale: the conversation was aborted, superseded or finished.
            return;
        };
        if !conv.awaiting {
            return;
        }
        conv.awaiting = false;
        let cmd = AuthParentCmd::Response { text: zeroize::Zeroizing::new(response.to_owned()) };
        let _ = ipc::send_msg(conv.sock.as_raw_fd(), &cmd);
    }

    /// End a live conversation: tell the child, report `Done`, and make sure
    /// the child cannot linger. Its result is never used.
    fn abort_auth(&mut self, index: usize, code: ErrorCode, message: &str) {
        let conv = self.auths.remove(index);
        let _ = ipc::send_msg(conv.sock.as_raw_fd(), &AuthParentCmd::Abort);
        drop(conv.sock);
        self.dying.push(Dying { pid: conv.pid, kill_at: Instant::now() + Duration::from_millis(1000) });
        self.emit(HelperEvent::AuthDone {
            req_id: conv.req_id,
            ok: false,
            code: Some(code),
            message: message.to_owned(),
            ticket: None,
        });
    }

    fn read_auth(&mut self, req_id: u64) {
        loop {
            let Some(i) = self.auths.iter().position(|a| a.req_id == req_id) else { return };
            let fd = self.auths[i].sock.as_raw_fd();
            match ipc::recv_msg::<AuthChildEvent>(fd) {
                Ok(Incoming::Msg(AuthChildEvent::Prompt { kind, text })) => {
                    if matches!(kind, bsm_proto::helper::PamPromptKind::EchoOff | bsm_proto::helper::PamPromptKind::EchoOn) {
                        self.auths[i].awaiting = true;
                    }
                    self.emit(HelperEvent::AuthPrompt { req_id, kind, text });
                }
                Ok(Incoming::Msg(AuthChildEvent::Done { ok, code, message })) => {
                    let conv = self.auths.remove(i);
                    if ok {
                        // A login keeps its child, which holds the authenticated PAM
                        // handle, as the ticket's payload. Any other child is done.
                        let parked = if conv.startable {
                            let _ = sys::set_nonblocking(conv.sock.as_raw_fd(), true);
                            Some(Parked { sock: conv.sock, pid: conv.pid })
                        } else {
                            self.dying.push(Dying { pid: conv.pid, kill_at: Instant::now() + Duration::from_millis(1000) });
                            None
                        };
                        let (uid, intent, generation) = (conv.uid, conv.intent, conv.generation);
                        match self.mint(uid, &conv.user, intent, generation, parked) {
                            Some(t) => self.emit(HelperEvent::AuthDone { req_id, ok: true, code: None, message: String::new(), ticket: Some(t) }),
                            None => self.emit(HelperEvent::AuthDone {
                                req_id,
                                ok: false,
                                code: Some(ErrorCode::Internal),
                                message: "no random source for the ticket".into(),
                                ticket: None,
                            }),
                        }
                    } else {
                        self.dying.push(Dying { pid: conv.pid, kill_at: Instant::now() + Duration::from_millis(1000) });
                        let code = code.or(Some(ErrorCode::PamFailure));
                        self.emit(HelperEvent::AuthDone { req_id, ok: false, code, message, ticket: None });
                    }
                    return;
                }
                Ok(Incoming::WouldBlock) => return,
                Ok(Incoming::Bad) | Ok(Incoming::Eof) | Err(_) => {
                    // The child ended without a result: crash or kill.
                    let conv = self.auths.remove(i);
                    self.kill_auth_child(conv.pid);
                    self.emit(HelperEvent::AuthDone {
                        req_id,
                        ok: false,
                        code: Some(ErrorCode::HelperFailure),
                        message: "authentication process ended unexpectedly".into(),
                        ticket: None,
                    });
                    return;
                }
            }
        }
    }

    fn mint(&mut self, uid: u32, user: &str, intent: AuthIntent, generation: Generation, parked: Option<Parked>) -> Option<u64> {
        sys::random_u64().ok()?;
        let mut failed = false;
        let id = self.tickets.mint(Instant::now(), uid, user, intent, generation, parked, || {
            sys::random_u64().unwrap_or_else(|_| {
                failed = true;
                1
            })
        });
        if failed {
            // A predictable value must never become a ticket.
            let _ = self.tickets.redeem(Instant::now(), id, generation, &[intent]);
            return None;
        }
        Some(id)
    }

    fn open_session(
        &mut self,
        req_id: u64,
        ticket: u64,
        session: SessionId,
        generation: Generation,
        socket_path: String,
        output_hints: Vec<(u32, u32)>,
    ) {
        let fail = |this: &mut Self, code: ErrorCode, message: &str| {
            this.emit(HelperEvent::SessionFailed {
                req_id,
                session: session.clone(),
                generation,
                code,
                message: message.to_owned(),
            })
        };
        let (t, parked) = match self.tickets.redeem(Instant::now(), ticket, generation, &[AuthIntent::Login, AuthIntent::Switch]) {
            Ok(t) => t,
            Err(RedeemError::WrongGeneration) => return fail(self, ErrorCode::StaleGeneration, "ticket belongs to another generation"),
            Err(RedeemError::Unknown | RedeemError::Expired | RedeemError::WrongIntent) => {
                return fail(self, ErrorCode::Unauthorised, "invalid, expired or already used ticket");
            }
        };
        if !session::valid_session_id(&session) {
            return fail(self, ErrorCode::InvalidArgument, "invalid session id");
        }
        if !greeter::path_within(&self.cfg.session.socket_dir, &socket_path) {
            return fail(self, ErrorCode::InvalidArgument, "socket path outside the session socket directory");
        }
        if output_hints.len() > 16 || output_hints.iter().any(|(w, h)| *w == 0 || *h == 0 || *w > 16384 || *h > 16384) {
            return fail(self, ErrorCode::InvalidArgument, "invalid output hints");
        }
        if self.sessions.iter().any(|s| s.session == session) {
            return fail(self, ErrorCode::Busy, "session id already in use");
        }
        let pw = match sys::getpwuid(t.uid) {
            Ok(Some(pw)) if pw.name == t.user => pw,
            _ => return fail(self, ErrorCode::InvalidAccount, "account changed since authentication"),
        };
        if let Err(m) = self.account_allowed(&pw) {
            return fail(self, ErrorCode::InvalidAccount, &m);
        }
        // `parked` is the child that holds the authenticated PAM handle. Anything
        // that went wrong above dropped it, which ended it.
        let Some(parked) = parked else {
            return fail(self, ErrorCode::Unauthorised, "ticket cannot open a session");
        };
        let start = StartParams { session: session.clone(), generation, host_socket: socket_path, output_hints };
        match ipc::send_msg(parked.sock.as_raw_fd(), &AuthParentCmd::StartSession(start)) {
            Ok(true) => {}
            _ => return fail(self, ErrorCode::HelperFailure, "the authenticated process is gone"),
        }
        let budget = Duration::from_secs(self.cfg.session.start_timeout_secs + self.cfg.session.runtime_wait_secs);
        self.sessions.push(SessionEntry {
            session,
            generation,
            req_id: Some(req_id),
            sup_pid: Some(parked.pid),
            sup_status: None,
            sock: parked.sock,
            opened: false,
            logind_session: None,
            start_deadline: Some(Instant::now() + budget),
        });
    }

    fn read_session(&mut self, index: usize) {
        let fd = self.sessions[index].sock.as_raw_fd();
        match ipc::recv_msg::<SupEvent>(fd) {
            Ok(Incoming::Msg(ev)) => self.session_event(index, ev),
            Ok(Incoming::WouldBlock) => {}
            Ok(Incoming::Bad) => eprintln!("bsm-helper: bad message from a session supervisor"),
            Ok(Incoming::Eof) | Err(_) => self.session_vanished(index),
        }
    }

    fn session_event(&mut self, index: usize, ev: SupEvent) {
        match ev {
            SupEvent::Opened { session, generation, supervisor_pid, leader_pid, host_pid, scope, logind_session } => {
                let e = &mut self.sessions[index];
                e.start_deadline = None;
                e.opened = true;
                e.logind_session = logind_session.clone();
                let adopted = e.req_id.is_none();
                if adopted {
                    e.session = session.clone();
                    e.generation = generation;
                    e.sup_pid = Some(supervisor_pid as libc::pid_t);
                    return;
                }
                let req_id = e.req_id.take().unwrap_or(0);
                self.emit(HelperEvent::SessionOpened { req_id, session, generation, host_pid, leader_pid, scope, logind_session });
            }
            SupEvent::Failed { code, message } => {
                let e = self.sessions.remove(index);
                self.reap_supervisor(e.sup_pid);
                self.emit(HelperEvent::SessionFailed {
                    req_id: e.req_id.unwrap_or(0),
                    session: e.session,
                    generation: e.generation,
                    code,
                    message,
                });
            }
            SupEvent::Exited { status } => {
                let e = self.sessions.remove(index);
                self.reap_supervisor(e.sup_pid);
                self.emit(HelperEvent::SessionExited { session: e.session, generation: e.generation, status });
            }
        }
    }

    /// The supervisor's socket closed without a final message: it crashed or was killed.
    fn session_vanished(&mut self, index: usize) {
        let e = self.sessions.remove(index);
        if e.session.is_empty() {
            return;
        }
        let status = match (e.sup_status, e.sup_pid) {
            (Some(s), _) => s,
            (None, Some(pid)) => sys::wait_pid_timeout(pid, Duration::from_millis(200)).unwrap_or(-libc::SIGKILL),
            (None, None) => -libc::SIGKILL,
        };
        if e.opened {
            self.emit(HelperEvent::SessionExited { session: e.session, generation: e.generation, status });
        } else {
            self.emit(HelperEvent::SessionFailed {
                req_id: e.req_id.unwrap_or(0),
                session: e.session,
                generation: e.generation,
                code: ErrorCode::HelperFailure,
                message: "session supervisor ended unexpectedly".into(),
            });
        }
    }

    fn reap_supervisor(&mut self, pid: Option<libc::pid_t>) {
        if let Some(pid) = pid {
            let _ = sys::wait_pid_timeout(pid, Duration::from_millis(500));
        }
    }

    fn spawn_greeter(&mut self, req_id: u64, generation: Generation, wayland_socket: String, control_socket: String) {
        let fail = |this: &mut Self, why: &str| {
            eprintln!("bsm-helper: cannot start the greeter: {why}");
            this.emit(HelperEvent::GreeterExited { generation, status: 127 });
        };
        let root = self.cfg.greeter.socket_root.clone();
        if !greeter::path_within(&root, &wayland_socket) || !greeter::path_within(&root, &control_socket) {
            return fail(self, "socket path outside the greeter socket directory");
        }
        let pw = match sys::getpwnam(&self.cfg.greeter.user) {
            Ok(Some(pw)) => pw,
            _ => return fail(self, "greeter account does not exist"),
        };
        if pw.uid == 0 {
            return fail(self, "refusing to run the greeter as root");
        }
        // A previous greeter is replaced, never run alongside.
        if let Some(old) = self.greeter.take() {
            sys::kill_group(old.pid, libc::SIGKILL);
            greeter::remove_dirs(&old.run.dir);
            self.emit(HelperEvent::GreeterExited { generation: old.generation, status: -libc::SIGKILL });
        }
        let run = match greeter::prepare_dirs(&self.cfg.greeter.runtime_root, generation, &pw) {
            Ok(r) => r,
            Err(e) => return fail(self, &format!("runtime directories: {e}")),
        };
        let locale = std::fs::read_to_string(&self.cfg.session.locale_conf)
            .map(|t| sessenv::parse_locale_conf(&t))
            .unwrap_or_default();
        let env = greeter::greeter_env(&GreeterEnvInputs {
            cfg: &self.cfg.greeter,
            pw: &pw,
            run: &run,
            generation,
            wayland_socket: &wayland_socket,
            control_socket: &control_socket,
            locale: &locale,
        });
        match session::spawn_as_user(&pw, &self.cfg.greeter.command, &env, &run.home, libc::SIGKILL) {
            Ok(pid) => {
                self.greeter = Some(GreeterEntry { generation, pid, run, kill_at: None });
                self.emit(HelperEvent::GreeterSpawned { req_id, generation, pid: pid as u32 });
            }
            Err(e) => {
                greeter::remove_dirs(&run.dir);
                fail(self, &e)
            }
        }
    }

    fn set_locked_hint(&mut self, req_id: u64, session: SessionId, generation: Generation, locked: bool) {
        let reply = |this: &mut Self, ok: bool, code: Option<ErrorCode>, message: &str| {
            this.emit(HelperEvent::LockedHintResult { req_id, ok, code, message: message.to_owned() })
        };
        let Some(entry) = self.sessions.iter().find(|s| s.session == session && s.generation == generation && s.opened) else {
            return reply(self, false, Some(ErrorCode::NoSession), "no such session");
        };
        let Some(id) = entry.logind_session.clone() else {
            return reply(self, false, Some(ErrorCode::NotSupported), "the session has no logind session");
        };
        if !logind::valid_session_id(&id) {
            return reply(self, false, Some(ErrorCode::LogindFailure), "invalid logind session id");
        }
        match unsafe { sys::fork() } {
            Ok(0) => {
                sys::reset_signals();
                sys::close_fds_except(&[]);
                sys::die_with_parent(libc::SIGKILL);
                let code = match logind::set_locked_hint(&id, locked) {
                    Ok(()) => 0,
                    Err(e) => {
                        eprintln!("bsm-helper: {e}");
                        1
                    }
                };
                unsafe { libc::_exit(code) };
            }
            Ok(pid) => self.hints.push(HintJob { pid, req_id }),
            Err(e) => reply(self, false, Some(ErrorCode::Internal), &format!("fork: {e}")),
        }
    }

    // ---- children -------------------------------------------------------

    fn child_exited(&mut self, pid: libc::pid_t, status: i32) {
        if let Some(i) = self.auths.iter().position(|a| a.pid == pid) {
            // Result messages sent before exit are still queued on the socket.
            let req_id = self.auths[i].req_id;
            self.read_auth(req_id);
            if let Some(i) = self.auths.iter().position(|a| a.pid == pid) {
                let conv = self.auths.remove(i);
                self.emit(HelperEvent::AuthDone {
                    req_id: conv.req_id,
                    ok: false,
                    code: Some(ErrorCode::HelperFailure),
                    message: "authentication process ended unexpectedly".into(),
                    ticket: None,
                });
            }
        }
        self.dying.retain(|d| d.pid != pid);
        // A parked child that died is no longer a valid payload; dropping it also
        // keeps its pid from being used after the kernel recycles it.
        for slot in self.tickets.payloads_mut() {
            if slot.as_ref().is_some_and(|p| p.pid == pid) {
                *slot = None;
            }
        }
        if let Some(i) = self.hints.iter().position(|h| h.pid == pid) {
            let job = self.hints.remove(i);
            if status == 0 {
                self.emit(HelperEvent::LockedHintResult { req_id: job.req_id, ok: true, code: None, message: String::new() });
            } else {
                self.emit(HelperEvent::LockedHintResult {
                    req_id: job.req_id,
                    ok: false,
                    code: Some(ErrorCode::LogindFailure),
                    message: "logind refused or is unreachable (see the helper journal)".into(),
                });
            }
        }
        if let Some(s) = self.sessions.iter_mut().find(|s| s.sup_pid == Some(pid)) {
            s.sup_status = Some(status);
        }
        if self.greeter.as_ref().is_some_and(|g| g.pid == pid)
            && let Some(g) = self.greeter.take()
        {
            greeter::remove_dirs(&g.run.dir);
            self.emit(HelperEvent::GreeterExited { generation: g.generation, status });
        }
    }

    fn timers(&mut self, now: Instant) {
        self.tickets.expire(now);
        for d in &self.dying {
            if now >= d.kill_at {
                sys::kill(d.pid, libc::SIGKILL);
            }
        }
        if let Some(g) = &self.greeter
            && g.kill_at.is_some_and(|k| now >= k)
        {
            sys::kill_group(g.pid, libc::SIGKILL);
        }
        let mut i = 0;
        while i < self.auths.len() {
            if now >= self.auths[i].deadline {
                let pid = self.auths[i].pid;
                self.abort_auth(i, ErrorCode::Timeout, "authentication timed out");
                sys::kill(pid, libc::SIGKILL);
            } else {
                i += 1;
            }
        }
        let mut i = 0;
        while i < self.sessions.len() {
            let overdue = self.sessions[i].start_deadline.is_some_and(|d| now >= d);
            if overdue {
                let e = self.sessions.remove(i);
                if let Some(pid) = e.sup_pid {
                    sys::kill(pid, libc::SIGKILL);
                }
                if e.session.is_empty() {
                    continue;
                }
                self.emit(HelperEvent::SessionFailed {
                    req_id: e.req_id.unwrap_or(0),
                    session: e.session,
                    generation: e.generation,
                    code: ErrorCode::Timeout,
                    message: "the session did not start in time".into(),
                });
            } else {
                i += 1;
            }
        }
    }

    fn next_deadline(&self) -> Option<Instant> {
        let mut d: Vec<Instant> = Vec::new();
        d.extend(self.tickets.next_expiry());
        d.extend(self.dying.iter().map(|x| x.kill_at));
        d.extend(self.auths.iter().map(|a| a.deadline));
        d.extend(self.sessions.iter().filter_map(|s| s.start_deadline));
        d.extend(self.greeter.as_ref().and_then(|g| g.kill_at));
        d.into_iter().min()
    }

    // ---- main loop ------------------------------------------------------

    /// Run until SIGTERM or SIGINT. Desktop sessions are left running.
    pub fn run(&mut self) -> io::Result<()> {
        while !self.shutdown {
            let mut fds: Vec<libc::pollfd> = Vec::new();
            let mut srcs: Vec<Src> = Vec::new();
            let add = |fd: RawFd, events: i16, src: Src, fds: &mut Vec<libc::pollfd>, srcs: &mut Vec<Src>| {
                fds.push(libc::pollfd { fd, events, revents: 0 });
                srcs.push(src);
            };
            add(self.sigfd.as_raw_fd(), libc::POLLIN, Src::Signal, &mut fds, &mut srcs);
            if let Some(c) = &self.conn {
                let ev = libc::POLLIN | if c.out.is_empty() { 0 } else { libc::POLLOUT };
                add(c.fd.as_raw_fd(), ev, Src::Conn, &mut fds, &mut srcs);
            }
            for (i, a) in self.auths.iter().enumerate() {
                add(a.sock.as_raw_fd(), libc::POLLIN, Src::Auth(i), &mut fds, &mut srcs);
            }
            for (i, s) in self.sessions.iter().enumerate() {
                add(s.sock.as_raw_fd(), libc::POLLIN, Src::Session(i), &mut fds, &mut srcs);
            }
            add(self.listener.as_raw_fd(), libc::POLLIN, Src::Listener, &mut fds, &mut srcs);

            let timeout = match self.next_deadline() {
                Some(d) => d.saturating_duration_since(Instant::now()).as_millis().min(i32::MAX as u128) as i32 + 1,
                None => -1,
            };
            let r = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, timeout) };
            if r < 0 {
                let e = io::Error::last_os_error();
                if e.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(e);
            }

            // Collect first: handling may add or remove entries, so indexes are
            // translated to stable keys before any handler runs.
            let mut ready_auth: Vec<u64> = Vec::new();
            let mut ready_sessions: Vec<RawFd> = Vec::new();
            let (mut conn_in, mut conn_out, mut listener, mut signals) = (false, false, false, false);
            for (pfd, src) in fds.iter().zip(&srcs) {
                if pfd.revents == 0 {
                    continue;
                }
                match *src {
                    Src::Signal => signals = true,
                    Src::Listener => listener = true,
                    Src::Conn => {
                        conn_in |= pfd.revents & (libc::POLLIN | libc::POLLHUP | libc::POLLERR) != 0;
                        conn_out |= pfd.revents & libc::POLLOUT != 0;
                    }
                    Src::Auth(i) => ready_auth.push(self.auths[i].req_id),
                    Src::Session(i) => ready_sessions.push(self.sessions[i].sock.as_raw_fd()),
                }
            }

            // Order matters: a closed connection is seen before a new one is
            // accepted, so a restarting bsm is not rejected as a second client.
            if conn_in {
                self.read_requests();
            }
            if conn_out {
                self.flush();
            }
            if listener {
                self.accept_connections();
            }
            for req_id in ready_auth {
                self.read_auth(req_id);
            }
            for fd in ready_sessions {
                while let Some(i) = self.sessions.iter().position(|s| s.sock.as_raw_fd() == fd) {
                    let before = self.sessions.len();
                    self.read_session(i);
                    // Stop after a terminal event removed the entry or nothing is queued.
                    if self.sessions.len() != before || !sys::wait_readable(fd, Duration::ZERO).unwrap_or(false) {
                        break;
                    }
                }
            }
            if signals {
                for sig in sys::read_signals(self.sigfd.as_raw_fd()) {
                    match sig {
                        libc::SIGTERM | libc::SIGINT => self.shutdown = true,
                        _ => {}
                    }
                }
                for (pid, status) in sys::reap_children() {
                    self.child_exited(pid, status);
                }
            }
            self.timers(Instant::now());
        }
        self.stop();
        Ok(())
    }

    /// Orderly shutdown. Authentication children and the greeter end with the
    /// helper; desktop sessions keep running and can be adopted.
    fn stop(&mut self) {
        for conv in std::mem::take(&mut self.auths) {
            sys::kill(conv.pid, libc::SIGKILL);
        }
        if let Some(g) = self.greeter.take() {
            sys::kill_group(g.pid, libc::SIGKILL);
            greeter::remove_dirs(&g.run.dir);
        }
        let _ = std::fs::remove_file(&self.cfg.socket.path);
    }

    /// Path of the socket the helper listens on when not socket-activated.
    pub fn socket_path(&self) -> &Path {
        &self.cfg.socket.path
    }
}

trait ClassifyErr {
    fn classify_str(&self) -> &'static str;
}

impl ClassifyErr for serde_json::Error {
    /// Error category without the message, which can quote the offending input.
    fn classify_str(&self) -> &'static str {
        match self.classify() {
            serde_json::error::Category::Io => "io",
            serde_json::error::Category::Syntax => "syntax",
            serde_json::error::Category::Data => "data",
            serde_json::error::Category::Eof => "eof",
        }
    }
}
