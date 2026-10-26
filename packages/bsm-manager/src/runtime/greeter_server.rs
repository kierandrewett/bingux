//! The greeter control socket (docs: `bsm_proto::control`).
//!
//! A `SOCK_STREAM` unix socket, one JSON object per line, at most 64 KiB per line. The
//! accept loop reads the peer credentials (`SO_PEERCRED`) and hands `(uid, pid)` to the
//! manager, which decides: only the greeter account's process that bsm itself started may
//! attach, and only one connection at a time. A connection that has not been accepted
//! by the manager within `VERDICT_TIMEOUT` is closed. Nothing read from a connection is
//! logged except the message type: `AuthResponse` carries secrets.
//!
//! Writes go through a bounded queue and a writer thread with a send timeout. A greeter
//! that cannot keep up is disconnected; it never stalls the manager.

use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{SyncSender, TrySendError, sync_channel};
use std::sync::Arc;

use parking_lot::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use bsm_proto::control::{ClientMessage, ServerMessage};
use bsm_proto::error::ErrorCode;

use super::api::{DriverTx, RtInput};
use crate::core::Input;

pub const MAX_LINE: usize = 64 * 1024;
pub const VERDICT_TIMEOUT: Duration = Duration::from_secs(5);
const WRITE_QUEUE: usize = 256;
const MAX_MALFORMED: u32 = 10;

#[derive(Debug, Clone)]
pub struct ControlConfig {
    pub path: PathBuf,
    /// File mode of the socket. The greeter account reaches it through the group.
    pub mode: u32,
    /// Group that owns the socket (the greeter's), if bsm may chown to it.
    pub gid: Option<u32>,
}

struct Active {
    id: u64,
    out: SyncSender<String>,
    stream: UnixStream,
}

struct Shared {
    pending: HashMap<u64, (UnixStream, Instant)>,
    active: Option<Active>,
}

pub struct GreeterServer {
    shared: Arc<Mutex<Shared>>,
    tx: DriverTx,
    path: PathBuf,
}

fn peer_cred(stream: &UnixStream) -> io::Result<(u32, u32)> {
    let mut cred = libc::ucred { pid: 0, uid: 0, gid: 0 };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: `cred` and `len` are valid for the call; the fd is a live socket.
    let rc = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            &mut cred as *mut _ as *mut libc::c_void,
            &mut len,
        )
    };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok((cred.uid, cred.pid as u32))
}

impl GreeterServer {
    /// Bind the socket and start accepting.
    pub fn spawn(cfg: ControlConfig, tx: DriverTx) -> io::Result<Self> {
        if let Some(dir) = cfg.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        match std::fs::remove_file(&cfg.path) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        let listener = UnixListener::bind(&cfg.path)?;
        std::fs::set_permissions(&cfg.path, std::fs::Permissions::from_mode(cfg.mode))?;
        if let Some(gid) = cfg.gid {
            std::os::unix::fs::chown(&cfg.path, None, Some(gid))?;
        }
        listener.set_nonblocking(true)?;
        let shared = Arc::new(Mutex::new(Shared { pending: HashMap::new(), active: None }));
        let server = Self { shared: shared.clone(), tx: tx.clone(), path: cfg.path.clone() };
        thread::Builder::new().name("bsm-control".into()).spawn(move || Self::accept_loop(listener, shared, tx))?;
        Ok(server)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn accept_loop(listener: UnixListener, shared: Arc<Mutex<Shared>>, tx: DriverTx) {
        let mut next_id = 1u64;
        loop {
            let mut pfd = libc::pollfd { fd: listener.as_raw_fd(), events: libc::POLLIN, revents: 0 };
            // SAFETY: one valid pollfd.
            unsafe { libc::poll(&mut pfd, 1, 500) };
            loop {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let _ = stream.set_nonblocking(false);
                        let Ok((uid, pid)) = peer_cred(&stream) else { continue };
                        let id = next_id;
                        next_id += 1;
                        shared.lock().pending.insert(id, (stream, Instant::now()));
                        if tx.send(RtInput::Core(Input::GreeterPeer { conn: id, uid, pid })).is_err() {
                            return;
                        }
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(e) => {
                        tracing::error!("control socket accept failed: {e}");
                        thread::sleep(Duration::from_millis(200));
                        break;
                    }
                }
            }
            // Drop connections the manager never answered.
            let mut s = shared.lock();
            s.pending.retain(|_, (_, at)| at.elapsed() < VERDICT_TIMEOUT);
        }
    }

    /// The manager's decision about a pending connection.
    pub fn verdict(&self, conn: u64, accept: bool) {
        let stream = {
            let mut s = self.shared.lock();
            s.pending.remove(&conn).map(|(st, _)| st)
        };
        let Some(stream) = stream else { return };
        if !accept {
            let _ = stream.shutdown(std::net::Shutdown::Both);
            return;
        }
        let (Ok(read_half), Ok(write_half), Ok(keep)) = (stream.try_clone(), stream.try_clone(), stream.try_clone()) else {
            return;
        };
        let _ = write_half.set_write_timeout(Some(Duration::from_secs(2)));
        let (out_tx, out_rx) = sync_channel::<String>(WRITE_QUEUE);
        self.shared.lock().active = Some(Active { id: conn, out: out_tx, stream: keep });
        // Writer.
        let shared = self.shared.clone();
        thread::Builder::new()
            .name("bsm-control-w".into())
            .spawn(move || {
                let mut w = write_half;
                while let Ok(line) = out_rx.recv() {
                    if w.write_all(line.as_bytes()).is_err() {
                        break;
                    }
                }
                let _ = w.shutdown(std::net::Shutdown::Both);
                let mut s = shared.lock();
                if s.active.as_ref().is_some_and(|a| a.id == conn) {
                    // The reader sees EOF and reports the disconnect.
                    s.active = None;
                }
            })
            .ok();
        // Reader.
        let tx = self.tx.clone();
        let shared = self.shared.clone();
        thread::Builder::new()
            .name("bsm-control-r".into())
            .spawn(move || {
                Self::read_loop(conn, read_half, &tx, &shared);
                {
                    let mut s = shared.lock();
                    if s.active.as_ref().is_some_and(|a| a.id == conn) {
                        // Dropping the queue ends the writer thread too.
                        s.active = None;
                    }
                }
                let _ = tx.send(RtInput::Core(Input::GreeterDisconnected { conn }));
            })
            .ok();
    }

    fn read_loop(conn: u64, mut stream: UnixStream, tx: &DriverTx, shared: &Arc<Mutex<Shared>>) {
        let mut buf: Vec<u8> = Vec::with_capacity(4096);
        let mut chunk = [0u8; 4096];
        let mut malformed = 0u32;
        loop {
            let n = match stream.read(&mut chunk) {
                Ok(0) => return,
                Ok(n) => n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => return,
            };
            buf.extend_from_slice(&chunk[..n]);
            while let Some(pos) = buf.iter().position(|b| *b == b'\n') {
                let line: Vec<u8> = buf.drain(..=pos).collect();
                let line = &line[..line.len() - 1];
                if line.is_empty() {
                    continue;
                }
                match serde_json::from_slice::<ClientMessage>(line) {
                    Ok(message) => {
                        // Only the type is ever logged.
                        tracing::trace!(conn, kind = message_kind(&message), "greeter message");
                        if tx.send(RtInput::Core(Input::Greeter { conn, message })).is_err() {
                            return;
                        }
                    }
                    Err(_) => {
                        malformed += 1;
                        Self::send_on(shared, conn, &ServerMessage::Error {
                            request_id: None,
                            code: ErrorCode::InvalidArgument,
                            message: "malformed message".into(),
                        });
                        if malformed >= MAX_MALFORMED {
                            return;
                        }
                    }
                }
            }
            if buf.len() > MAX_LINE {
                tracing::warn!(conn, "control line too long; disconnecting");
                return;
            }
        }
    }

    fn send_on(shared: &Arc<Mutex<Shared>>, conn: u64, msg: &ServerMessage) -> bool {
        let Ok(mut line) = serde_json::to_string(msg) else { return false };
        line.push('\n');
        let s = shared.lock();
        match s.active.as_ref().filter(|a| a.id == conn) {
            Some(a) => match a.out.try_send(line) {
                Ok(()) => true,
                Err(TrySendError::Full(_)) => {
                    // Too slow to keep up: disconnect rather than stall or grow without bound.
                    let _ = a.stream.shutdown(std::net::Shutdown::Both);
                    false
                }
                Err(TrySendError::Disconnected(_)) => false,
            },
            None => false,
        }
    }

    /// Send to the attached greeter, if any. False when nobody is attached or the queue is full.
    pub fn send(&self, msg: &ServerMessage) -> bool {
        let id = self.shared.lock().active.as_ref().map(|a| a.id);
        match id {
            Some(id) => Self::send_on(&self.shared, id, msg),
            None => false,
        }
    }

    /// Close the attached connection (protocol violation).
    pub fn drop_active(&self) {
        let mut s = self.shared.lock();
        if let Some(a) = s.active.take() {
            let _ = a.stream.shutdown(std::net::Shutdown::Both);
        }
    }
}

fn message_kind(m: &ClientMessage) -> &'static str {
    match m {
        ClientMessage::Hello { .. } => "hello",
        ClientMessage::SelectUser { .. } => "select_user",
        ClientMessage::BeginAuth { .. } => "begin_auth",
        ClientMessage::AuthResponse { .. } => "auth_response",
        ClientMessage::CancelAuth { .. } => "cancel_auth",
        ClientMessage::Power { .. } => "power",
        ClientMessage::CancelOperation { .. } => "cancel_operation",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::sync::mpsc::{Receiver, sync_channel};

    fn setup() -> (tempfile::TempDir, GreeterServer, Receiver<RtInput>) {
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = sync_channel(64);
        let server = GreeterServer::spawn(ControlConfig { path: dir.path().join("c.sock"), mode: 0o660, gid: None }, tx).unwrap();
        (dir, server, rx)
    }

    fn next(rx: &Receiver<RtInput>) -> Input {
        match rx.recv_timeout(Duration::from_secs(3)).expect("input") {
            RtInput::Core(i) => i,
            _ => panic!("kind"),
        }
    }

    #[test]
    fn the_peer_credentials_reach_the_manager_and_its_verdict_decides() {
        let (_d, server, rx) = setup();
        let mode = std::fs::metadata(server.path()).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o660);
        let mut c = UnixStream::connect(server.path()).unwrap();
        let Input::GreeterPeer { conn, uid, pid } = next(&rx) else { panic!() };
        assert_eq!(uid, unsafe { libc::getuid() });
        assert_eq!(pid, std::process::id());
        // Nothing is read before acceptance.
        c.write_all(b"{\"type\":\"hello\",\"version\":1}\n").unwrap();
        assert!(rx.recv_timeout(Duration::from_millis(200)).is_err());
        server.verdict(conn, true);
        let Input::Greeter { message: ClientMessage::Hello { version: 1 }, .. } = next(&rx) else { panic!() };
        // And the other direction.
        assert!(server.send(&ServerMessage::Error { request_id: Some(3), code: ErrorCode::Busy, message: "x".into() }));
        let mut line = String::new();
        BufReader::new(c.try_clone().unwrap()).read_line(&mut line).unwrap();
        let back: ServerMessage = serde_json::from_str(line.trim()).unwrap();
        assert!(matches!(back, ServerMessage::Error { request_id: Some(3), code: ErrorCode::Busy, .. }));
        // Closing is reported.
        drop(c);
        assert!(matches!(next(&rx), Input::GreeterDisconnected { .. }));
        std::thread::sleep(Duration::from_millis(100));
        assert!(!server.send(&ServerMessage::Error { request_id: None, code: ErrorCode::Busy, message: String::new() }));
    }

    #[test]
    fn a_refused_connection_is_closed_and_sends_nothing_reach_it() {
        let (_d, server, rx) = setup();
        let mut c = UnixStream::connect(server.path()).unwrap();
        let Input::GreeterPeer { conn, .. } = next(&rx) else { panic!() };
        server.verdict(conn, false);
        let mut b = [0u8; 8];
        assert_eq!(c.read(&mut b).unwrap(), 0);
        assert!(!server.send(&ServerMessage::Error { request_id: None, code: ErrorCode::Busy, message: String::new() }));
    }

    #[test]
    fn malformed_and_oversized_input_is_contained() {
        let (_d, server, rx) = setup();
        let mut c = UnixStream::connect(server.path()).unwrap();
        let Input::GreeterPeer { conn, .. } = next(&rx) else { panic!() };
        server.verdict(conn, true);
        c.write_all(b"garbage\n{\"type\":\"hello\",\"version\":1}\n").unwrap();
        // The garbage line is answered with an error and does not reach the manager.
        let Input::Greeter { message: ClientMessage::Hello { .. }, .. } = next(&rx) else { panic!() };
        let mut line = String::new();
        BufReader::new(c.try_clone().unwrap()).read_line(&mut line).unwrap();
        assert!(line.contains("invalid_argument"));
        // A line that never ends is cut off at the cap.
        let big = vec![b'a'; MAX_LINE + 4096];
        let _ = c.write_all(&big);
        assert!(matches!(next(&rx), Input::GreeterDisconnected { .. }));
    }

    #[test]
    fn an_unanswered_connection_times_out() {
        let (_d, server, rx) = setup();
        let mut c = UnixStream::connect(server.path()).unwrap();
        let _ = next(&rx);
        c.set_read_timeout(Some(Duration::from_secs(8))).unwrap();
        let mut b = [0u8; 8];
        assert_eq!(c.read(&mut b).unwrap_or(0), 0);
        let _ = server;
    }

    #[test]
    fn a_stale_socket_file_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("c.sock");
        std::fs::write(&p, b"stale").unwrap();
        let (tx, _rx) = sync_channel(4);
        GreeterServer::spawn(ControlConfig { path: p.clone(), mode: 0o600, gid: None }, tx).unwrap();
        assert!(UnixStream::connect(&p).is_ok());
    }
}
