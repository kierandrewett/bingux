//! Harness: runs the real `bsm-helper` binary against a test PAM directory and
//! talks to it over its real socket.

#![allow(dead_code)]

#[path = "../../src/testpam.rs"]
pub mod testpam;

use std::os::fd::{AsRawFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use bsm_helper::sys::{self, Recv};
use bsm_proto::helper::{HelperEvent, HelperRequest, MAX_DATAGRAM};
use bsm_proto::state::AuthIntent;

pub use testpam::{PASSWORD, TestPam};

pub fn skip_as_root() -> bool {
    if unsafe { libc::geteuid() } == 0 {
        eprintln!("skipped: the unprivileged test configuration cannot run as root (uid 0 never signs in)");
        true
    } else {
        false
    }
}

pub struct Env {
    pub pam: TestPam,
    pub dir: tempfile::TempDir,
    pub extra_config: String,
    pub allowed_uid: u32,
    /// Peer process identity; empty means unchecked (see `SocketConfig`).
    pub peer_exe: String,
    pub peer_cgroup: String,
}

impl Env {
    pub fn new() -> Env {
        let pam = TestPam::new();
        let allowed_uid = pam.uid;
        Env { pam, dir: tempfile::tempdir().unwrap(), extra_config: String::new(), allowed_uid, peer_exe: String::new(), peer_cgroup: String::new() }
    }

    pub fn socket(&self) -> PathBuf {
        self.dir.path().join("helper.sock")
    }

    pub fn state_dir(&self) -> PathBuf {
        self.dir.path().join("state")
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    pub fn session_socket_dir(&self) -> PathBuf {
        self.dir.path().join("sessions")
    }

    /// Default test configuration: the helper's own account is the "user".
    pub fn config(&self) -> String {
        let d = self.dir.path();
        format!(
            r#"
state_dir = "{state}"

[socket]
path = "{sock}"
allowed_uid = {uid}
allowed_exe = "{peer_exe}"
allowed_cgroup = "{peer_cgroup}"

[pam]
login_service = "bsm-test-login"
unlock_service = "bsm-test-password"
confdir = "{pamdir}"
ticket_ttl_secs = 30

[session]
gnoblin = ["/bin/sh", "-c", "env > {d}/leader.env; exec sleep 120"]
bridge = ["/bin/sh", "-c", "env > {d}/bridge.env; exec sleep 120"]
require_scope = false
require_user_runtime = false
min_uid = 0
default_grace_ms = 500
socket_dir = "{sessions}"
locale_conf = "{d}/locale.conf"

[greeter]
user = "{user}"
command = ["/bin/sh", "-c", "env > \"$HOME/env.txt\"; exec sleep 120"]
runtime_root = "{d}/greeter"
socket_root = "{d}/run"
{extra}
"#,
            state = self.state_dir().display(),
            sock = self.socket().display(),
            uid = self.allowed_uid,
            peer_exe = self.peer_exe,
            peer_cgroup = self.peer_cgroup,
            pamdir = self.pam.path().display(),
            d = d.display(),
            sessions = self.session_socket_dir().display(),
            user = self.pam.user,
            extra = self.extra_config,
        )
    }

    pub fn start(&self) -> HelperProc {
        self.start_with(&self.config())
    }

    pub fn start_with(&self, config: &str) -> HelperProc {
        let cfg_path = self.dir.path().join("helper.toml");
        std::fs::write(&cfg_path, config).unwrap();
        std::fs::create_dir_all(self.session_socket_dir()).unwrap();
        // A killed helper leaves its socket file behind; wait for the new one.
        let _ = std::fs::remove_file(self.socket());
        let child = Command::new(env!("CARGO_BIN_EXE_bsm-helper"))
            .arg("--config")
            .arg(&cfg_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(std::fs::OpenOptions::new().create(true).append(true).open(self.dir.path().join("helper.log")).unwrap())
            .spawn()
            .expect("spawn bsm-helper");
        let proc = HelperProc { child: Some(child), supervised: Vec::new() };
        // The socket file appears at bind(), a moment before listen(): wait until
        // it accepts a connection.
        let deadline = Instant::now() + Duration::from_secs(10);
        while sys::connect_seqpacket(&self.socket()).is_err() {
            assert!(Instant::now() < deadline, "helper did not start listening");
            std::thread::sleep(Duration::from_millis(20));
        }
        proc
    }

    /// One raw connection attempt, for tests about connection policy.
    pub fn connect_raw(&self) -> Client {
        Client::connect(&self.socket())
    }

    /// A connection the helper has accepted, proven by a Ping/Pong. A descriptor
    /// that a test just dropped can stay open for a moment in the fork child of a
    /// concurrently spawned helper, so the helper may still see its previous peer
    /// and close the newcomer: retry, as a restarting bsm does.
    pub fn connect(&self) -> Client {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let c = Client::connect(&self.socket());
            let _ = c.send_raw(&serde_json::to_vec(&HelperRequest::Ping { nonce: u64::MAX }).unwrap());
            if let Some(HelperEvent::Pong { nonce: u64::MAX }) = c.try_recv(Duration::from_millis(700)) {
                return c;
            }
            assert!(Instant::now() < deadline, "the helper never accepted a connection");
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        // The helper's log is only interesting when the test failed.
        if std::thread::panicking() {
            let log = std::fs::read_to_string(self.dir.path().join("helper.log")).unwrap_or_default();
            eprintln!("--- helper log ---\n{log}--- end ---");
        }
    }
}

pub struct HelperProc {
    child: Option<Child>,
    /// Leader pids of sessions to clean up if a test fails half way.
    supervised: Vec<u32>,
}

impl HelperProc {
    pub fn kill9_boxed(mut self) {
        self.kill9();
    }

    pub fn pid(&self) -> u32 {
        self.child.as_ref().unwrap().id()
    }

    pub fn kill9(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }

    pub fn terminate(&mut self) -> Option<std::process::ExitStatus> {
        let mut c = self.child.take()?;
        unsafe { libc::kill(c.id() as i32, libc::SIGTERM) };
        c.wait().ok()
    }

    pub fn guard_session(&mut self, leader_pid: u32) {
        self.supervised.push(leader_pid);
    }

    /// Direct children of the helper (authentication children).
    pub fn children(&self) -> Vec<u32> {
        children_of(self.pid())
    }
}

pub fn children_of(pid: u32) -> Vec<u32> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/task/{pid}/children")).unwrap_or_default();
    text.split_whitespace().filter_map(|s| s.parse().ok()).collect()
}

pub fn parent_of(pid: u32) -> Option<u32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let after = stat.rsplit_once(')')?.1;
    after.split_whitespace().nth(1)?.parse().ok()
}

pub fn alive(pid: u32) -> bool {
    // A zombie still answers kill(0); read the state instead.
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(s) => s.rsplit_once(')').and_then(|(_, r)| r.split_whitespace().next().map(|st| st != "Z")).unwrap_or(false),
        Err(_) => false,
    }
}

pub fn wait_dead(pid: u32, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if !alive(pid) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    !alive(pid)
}

impl Drop for HelperProc {
    fn drop(&mut self) {
        for leader in &self.supervised {
            if let Some(sup) = parent_of(*leader).filter(|p| *p > 1) {
                unsafe { libc::kill(sup as i32, libc::SIGTERM) };
            }
            unsafe { libc::kill(*leader as i32, libc::SIGKILL) };
        }
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

pub struct Client {
    pub fd: OwnedFd,
}

impl Client {
    pub fn connect(path: &Path) -> Client {
        Client { fd: sys::connect_seqpacket(path).expect("connect") }
    }

    pub fn send(&self, req: &HelperRequest) {
        let bytes = serde_json::to_vec(req).unwrap();
        assert!(sys::send(self.fd.as_raw_fd(), &bytes).unwrap());
    }

    pub fn send_raw(&self, bytes: &[u8]) -> std::io::Result<bool> {
        sys::send(self.fd.as_raw_fd(), bytes)
    }

    /// Next event within `timeout`; `None` on timeout or when the helper closed the connection.
    pub fn try_recv(&self, timeout: Duration) -> Option<HelperEvent> {
        if !sys::wait_readable(self.fd.as_raw_fd(), timeout).ok()? {
            return None;
        }
        let mut buf = vec![0u8; MAX_DATAGRAM + 1];
        match sys::recv(self.fd.as_raw_fd(), &mut buf).ok()? {
            Recv::Data(n) => Some(serde_json::from_slice(&buf[..n]).expect("event decodes")),
            _ => None,
        }
    }

    pub fn closed_within(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut buf = vec![0u8; MAX_DATAGRAM + 1];
        while Instant::now() < deadline {
            if sys::wait_readable(self.fd.as_raw_fd(), Duration::from_millis(50)).unwrap_or(false) {
                match sys::recv(self.fd.as_raw_fd(), &mut buf) {
                    Ok(Recv::Eof) | Err(_) => return true,
                    _ => {}
                }
            }
        }
        false
    }

    pub fn recv(&self) -> HelperEvent {
        self.try_recv(Duration::from_secs(15)).expect("no event from the helper within 15 s")
    }

    /// Receive events until one matches; returns it.
    pub fn until(&self, mut pred: impl FnMut(&HelperEvent) -> bool) -> HelperEvent {
        loop {
            let ev = self.recv();
            if pred(&ev) {
                return ev;
            }
        }
    }

    pub fn assert_silent(&self, timeout: Duration) {
        if let Some(ev) = self.try_recv(timeout) {
            panic!("unexpected event: {ev:?}");
        }
    }

    pub fn auth_start(&self, env: &Env, req_id: u64, generation: u64, intent: AuthIntent) {
        let service = match intent {
            AuthIntent::Login | AuthIntent::Switch => "bsm-test-login",
            AuthIntent::Unlock => "bsm-test-password",
        };
        self.send(&HelperRequest::AuthStart {
            req_id,
            user: env.pam.user.clone(),
            uid: env.pam.uid,
            intent,
            generation,
            service: service.to_owned(),
        });
    }

    /// Run a whole successful conversation and return the ticket.
    pub fn login(&self, env: &Env, req_id: u64, generation: u64, intent: AuthIntent) -> u64 {
        self.auth_start(env, req_id, generation, intent);
        self.until(|e| matches!(e, HelperEvent::AuthPrompt { kind: bsm_proto::helper::PamPromptKind::EchoOff, .. }));
        self.send(&HelperRequest::AuthResponse { req_id, response: PASSWORD.to_owned() });
        match self.until(|e| matches!(e, HelperEvent::AuthDone { .. })) {
            HelperEvent::AuthDone { ok: true, ticket: Some(t), .. } => t,
            other => panic!("login failed: {other:?}"),
        }
    }

    pub fn open_session(&self, env: &Env, req_id: u64, ticket: u64, session: &str, generation: u64) {
        self.send(&HelperRequest::OpenSession {
            req_id,
            ticket,
            session: session.to_owned(),
            generation,
            socket_path: env.session_socket_dir().join(format!("{session}-{generation}.sock")).display().to_string(),
            output_hints: vec![(1920, 1080)],
        });
    }
}
