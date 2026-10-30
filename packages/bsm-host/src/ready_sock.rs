//! `$XDG_RUNTIME_DIR/bsm/ready.sock`: where the Bingux shell processes report
//! their startup progress to the bridge.
//!
//! The bridge creates the socket (directory mode 0700, socket mode 0600) and
//! accepts only peers with the session's own uid. A line is one JSON object
//! of at most 4 KiB. Connections that send an over-long line or a malformed
//! object are closed; a well-formed line for another generation is counted and
//! ignored by [`crate::readiness::Readiness`].

use std::io::{self, Read};
use std::os::fd::AsFd;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context as _, Result};

use crate::readiness::ReadyLine;

pub const MAX_LINE: usize = 4096;
pub const MAX_CONNECTIONS: usize = 16;

pub fn socket_path(runtime_dir: &Path) -> PathBuf {
    runtime_dir.join(bsm_proto::paths::SESSION_READY_SOCKET)
}

/// Create the listening socket. Any stale socket file is replaced.
pub fn bind(runtime_dir: &Path) -> Result<(UnixListener, PathBuf)> {
    let path = socket_path(runtime_dir);
    let dir = path.parent().context("ready socket has a parent")?;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .with_context(|| format!("create {}", dir.display()))?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    match std::fs::remove_file(&path) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e).with_context(|| format!("remove stale {}", path.display())),
    }
    let listener = UnixListener::bind(&path).with_context(|| format!("bind {}", path.display()))?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;
    Ok((listener, path))
}

/// Is the peer of `stream` the given uid?
pub fn peer_uid_is(stream: &UnixStream, uid: u32) -> bool {
    rustix::net::sockopt::socket_peercred(stream.as_fd()).is_ok_and(|c| c.uid.as_raw() == uid)
}

/// Processes that may report: the Bingux shell (Quickshell started as
/// `bingux` and `bingux-wallpaper`). Matched against the peer's executable
/// name and `comm`. Best effort: any process of the session uid can run a
/// binary with such a name, and when `/proc/<pid>` cannot be read the peer is
/// accepted. It keeps stray same-uid helpers from weakening the reveal gate by
/// accident; it is not a security boundary (see docs/bsm/bridge.md).
pub const DEFAULT_PEERS: &[&str] = &["quickshell", "qs", "bingux", "bingux-wallpaper"];

/// Connections that send no valid line within this time are closed, so idle
/// connections cannot occupy the slots the real reporters need.
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(10);

/// The pid of the peer of `stream` (SO_PEERCRED).
pub fn peer_pid(stream: &UnixStream) -> Option<u32> {
    rustix::net::sockopt::socket_peercred(stream.as_fd()).ok().map(|c| c.pid.as_raw_nonzero().get() as u32)
}

/// Names (executable file name, comm) of process `pid`, if readable.
pub fn peer_names(pid: u32) -> Option<Vec<String>> {
    let mut names = Vec::new();
    if let Ok(exe) = std::fs::read_link(format!("/proc/{pid}/exe")) {
        if let Some(n) = exe.file_name() {
            names.push(n.to_string_lossy().into_owned());
        }
    }
    if let Ok(comm) = std::fs::read_to_string(format!("/proc/{pid}/comm")) {
        names.push(comm.trim().to_owned());
    }
    (!names.is_empty()).then_some(names)
}

/// Is a peer with these names allowed? `None` (unreadable) is allowed;
/// a `*` entry allows everything (tests).
pub fn peer_allowed(names: Option<&[String]>, allow: &[String]) -> bool {
    if allow.iter().any(|a| a == "*") {
        return true;
    }
    match names {
        None => true,
        Some(names) => names.iter().any(|n| allow.iter().any(|a| a == n)),
    }
}

/// One accepted connection with its partial-line buffer.
pub struct LineConn {
    pub stream: UnixStream,
    buf: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Lines {
    /// Complete, parsed lines and whether the peer is still connected.
    Ok { lines: Vec<ReadyLine>, open: bool },
    /// The peer misbehaved and must be disconnected.
    Violation(&'static str),
}

impl LineConn {
    pub fn new(stream: UnixStream) -> io::Result<Self> {
        stream.set_nonblocking(true)?;
        Ok(Self { stream, buf: Vec::new() })
    }

    pub fn read(&mut self) -> Lines {
        let mut open = true;
        let mut chunk = [0u8; 2048];
        loop {
            match (&self.stream).read(&mut chunk) {
                Ok(0) => {
                    open = false;
                    break;
                }
                Ok(n) => self.buf.extend_from_slice(&chunk[..n]),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => {
                    open = false;
                    break;
                }
            }
            if self.buf.len() > MAX_LINE * 4 {
                break;
            }
        }
        match parse_lines(&mut self.buf) {
            Ok(lines) => Lines::Ok { lines, open },
            Err(why) => Lines::Violation(why),
        }
    }
}

/// Split complete lines off `buf`. Keeps a trailing partial line. A line over
/// [`MAX_LINE`] bytes (complete or still growing) or invalid JSON is a violation.
pub fn parse_lines(buf: &mut Vec<u8>) -> Result<Vec<ReadyLine>, &'static str> {
    let mut out = Vec::new();
    loop {
        let Some(nl) = buf.iter().position(|b| *b == b'\n') else {
            if buf.len() > MAX_LINE {
                return Err("line too long");
            }
            return Ok(out);
        };
        if nl > MAX_LINE {
            return Err("line too long");
        }
        let line: Vec<u8> = buf.drain(..=nl).collect();
        let line = &line[..nl];
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        match serde_json::from_slice::<ReadyLine>(line) {
            Ok(l) => out.push(l),
            Err(_) => return Err("malformed line"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::os::unix::fs::MetadataExt;

    #[test]
    fn parses_complete_lines_and_keeps_partial() {
        let mut buf = br#"{"generation":"1","event":"launched","component":"desktop"}
{"generation":"1","event":"re"#
            .to_vec();
        let lines = parse_lines(&mut buf).unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].event, "launched");
        assert!(buf.starts_with(b"{\"generation\":\"1\",\"event\":\"re"));
        buf.extend_from_slice(b"ady\"}\n");
        let lines = parse_lines(&mut buf).unwrap();
        assert_eq!(lines[0].event, "ready");
        assert!(buf.is_empty());
    }

    #[test]
    fn peer_policy_is_best_effort_and_configurable() {
        let allow: Vec<String> = DEFAULT_PEERS.iter().map(|s| s.to_string()).collect();
        let n = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(peer_allowed(Some(&n(&["quickshell", "bingux"])), &allow));
        assert!(peer_allowed(Some(&n(&["python3", "bingux-wallpaper"])), &allow));
        assert!(!peer_allowed(Some(&n(&["python3", "python3"])), &allow), "a stray helper is refused");
        assert!(peer_allowed(None, &allow), "unreadable /proc: accepted, documented");
        assert!(peer_allowed(Some(&n(&["anything"])), &n(&["*"])));
    }

    #[test]
    fn own_process_names_are_readable() {
        let pid = std::process::id();
        let names = peer_names(pid).expect("/proc is readable for our own pid");
        assert!(!names.is_empty());
        let (a, _b) = UnixStream::pair().unwrap();
        assert_eq!(peer_pid(&a), Some(pid));
    }

    #[test]
    fn overlong_and_malformed_lines_are_violations() {
        let mut buf = vec![b'x'; MAX_LINE + 1];
        assert_eq!(parse_lines(&mut buf).unwrap_err(), "line too long");
        let mut buf = b"not json\n".to_vec();
        assert_eq!(parse_lines(&mut buf).unwrap_err(), "malformed line");
        let mut buf = b"{\"event\":\"ready\"}\n".to_vec(); // generation missing
        assert!(parse_lines(&mut buf).is_err());
    }

    #[test]
    fn bound_socket_is_private_and_replaces_stale_file() {
        let dir = std::env::temp_dir().join(format!("bsm-host-ready-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::create_dir_all(dir.join("bsm")).unwrap();
        std::fs::write(socket_path(&dir), b"stale").unwrap();
        let (listener, path) = bind(&dir).unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
        assert_eq!(std::fs::metadata(path.parent().unwrap()).unwrap().mode() & 0o777, 0o700);
        let mut c = UnixStream::connect(&path).unwrap();
        c.write_all(b"{\"generation\":\"3\",\"event\":\"launched\"}\n").unwrap();
        let (s, _) = loop {
            match listener.accept() {
                Ok(x) => break x,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => std::thread::sleep(std::time::Duration::from_millis(5)),
                Err(e) => panic!("{e}"),
            }
        };
        let uid = unsafe { libc::getuid() };
        assert!(peer_uid_is(&s, uid));
        assert!(!peer_uid_is(&s, uid + 1));
        let mut lc = LineConn::new(s).unwrap();
        match lc.read() {
            Lines::Ok { lines, open } => {
                assert!(open);
                assert_eq!(lines[0].generation, "3");
            }
            other => panic!("{other:?}"),
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
