//! Client of `bsm-helper` over a `SOCK_SEQPACKET` socket (docs: `bsm_proto::helper`).
//!
//! One JSON message per datagram, at most `MAX_DATAGRAM` bytes. A reader thread turns
//! each datagram into an `Input::Helper`; losing the connection becomes `Input::HelperLost`
//! and the thread reconnects with backoff. Sending never blocks for long: the socket has a
//! send timeout and a failure is reported as a lost helper.

use std::io;
use std::mem::MaybeUninit;
use std::os::fd::OwnedFd;
use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use std::thread;
use std::time::Duration;

use bsm_proto::helper::{HelperEvent, HelperRequest, MAX_DATAGRAM};
use zeroize::Zeroize;
use socket2::{Domain, SockAddr, Socket, Type};

use super::api::{DriverTx, RtInput};
use crate::core::Input;

pub enum HelperEndpoint {
    /// Connect to `HELPER_SOCKET` (or a test path), reconnecting after a loss. With `peer_uid`
    /// set the connection is used only if the process that accepted it runs as that uid
    /// (root in production): anything else could answer `AuthDone` and hand out tickets (F16).
    Path { path: PathBuf, peer_uid: Option<u32> },
    /// An already connected socket (tests, socket activation). Not reconnected.
    Fd(OwnedFd),
}

pub struct HelperClient {
    writer: Arc<Mutex<Option<Socket>>>,
}

/// uid of the process on the other end (`SO_PEERCRED`, recorded at connect time).
fn peer_uid_of(s: &Socket) -> io::Result<u32> {
    use std::os::fd::AsRawFd;
    let mut cred = libc::ucred { pid: 0, uid: 0, gid: 0 };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: `cred` and `len` are valid for the call; the fd is a live socket.
    let rc = unsafe {
        libc::getsockopt(s.as_raw_fd(), libc::SOL_SOCKET, libc::SO_PEERCRED, &mut cred as *mut _ as *mut libc::c_void, &mut len)
    };
    if rc != 0 { Err(io::Error::last_os_error()) } else { Ok(cred.uid) }
}

fn seqpacket() -> io::Result<Socket> {
    Socket::new(Domain::UNIX, Type::SEQPACKET, None)
}

impl HelperClient {
    /// Start the reader thread. `HelperConnected` is sent once a connection is up.
    pub fn spawn(endpoint: HelperEndpoint, tx: DriverTx) -> io::Result<Self> {
        let writer: Arc<Mutex<Option<Socket>>> = Arc::new(Mutex::new(None));
        let w = writer.clone();
        thread::Builder::new().name("bsm-helper".into()).spawn(move || {
            let mut path = None;
            let mut peer_uid = None;
            let mut first_fd = None;
            match endpoint {
                HelperEndpoint::Path { path: p, peer_uid: u } => {
                    path = Some(p);
                    peer_uid = u;
                }
                HelperEndpoint::Fd(fd) => first_fd = Some(Socket::from(fd)),
            }
            let mut backoff = Duration::from_millis(200);
            loop {
                let sock = if let Some(s) = first_fd.take() {
                    Ok(s)
                } else if let Some(p) = &path {
                    seqpacket().and_then(|s| s.connect(&SockAddr::unix(p)?).map(|_| s)).and_then(|s| {
                        match (peer_uid, peer_uid_of(&s)) {
                            (Some(want), Ok(got)) if want != got => {
                                tracing::error!("the helper socket is served by uid {got}, not {want}; not used");
                                Err(io::Error::new(io::ErrorKind::PermissionDenied, "untrusted helper peer"))
                            }
                            (Some(_), Err(e)) => Err(e),
                            _ => Ok(s),
                        }
                    })
                } else {
                    return;
                };
                match sock {
                    Ok(sock) => {
                        backoff = Duration::from_millis(200);
                        if Self::serve(sock, &w, &tx).is_err() {
                            return;
                        }
                        if path.is_none() {
                            return;
                        }
                    }
                    Err(e) => {
                        tracing::debug!("helper not reachable yet: {e}");
                    }
                }
                thread::sleep(backoff);
                backoff = (backoff * 2).min(Duration::from_secs(5));
            }
        })?;
        Ok(Self { writer })
    }

    /// Run one connection until it ends. `Err` means the driver is gone.
    fn serve(sock: Socket, writer: &Arc<Mutex<Option<Socket>>>, tx: &DriverTx) -> Result<(), ()> {
        let _ = sock.set_write_timeout(Some(Duration::from_secs(2)));
        let Ok(reader) = sock.try_clone() else { return Ok(()) };
        *writer.lock() = Some(sock);
        tx.send(RtInput::Core(Input::HelperConnected)).map_err(|_| ())?;
        let mut buf = vec![MaybeUninit::<u8>::uninit(); MAX_DATAGRAM + 1];
        loop {
            match reader.recv(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if n > MAX_DATAGRAM {
                        tracing::warn!("oversized helper datagram dropped");
                        continue;
                    }
                    // SAFETY: `recv` initialised the first `n` bytes.
                    let bytes: &[u8] = unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const u8, n) };
                    match serde_json::from_slice::<HelperEvent>(bytes) {
                        Ok(ev) => tx.send(RtInput::Core(Input::Helper(ev))).map_err(|_| ())?,
                        Err(e) => tracing::warn!("malformed helper message dropped: {e}"),
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
        *writer.lock() = None;
        tx.send(RtInput::Core(Input::HelperLost)).map_err(|_| ())?;
        Ok(())
    }

    /// Send one request. False when there is no connection or the send failed; the caller
    /// treats that as a lost helper (the reader thread notices the same).
    pub fn send(&self, req: &HelperRequest) -> bool {
        let Ok(mut bytes) = serde_json::to_vec(req) else { return false };
        if bytes.len() > MAX_DATAGRAM {
            bytes.zeroize();
            tracing::error!("helper request too large; refused");
            return false;
        }
        let ok = match self.writer.lock().as_ref() {
            Some(s) => {
                let sent = s.send(&bytes).is_ok();
                if !sent {
                    // Not a lost helper until the connection says so: closing it makes the reader
                    // thread report the loss and reconnect, the one place that decides (F16).
                    let _ = s.shutdown(std::net::Shutdown::Both);
                }
                sent
            }
            None => false,
        };
        // F11: the serialised request may carry a password.
        bytes.zeroize();
        ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::sync_channel;

    fn pair() -> (Socket, OwnedFd) {
        let (a, b) = Socket::pair(Domain::UNIX, Type::SEQPACKET, None).unwrap();
        (a, OwnedFd::from(b))
    }

    fn recv_input(rx: &std::sync::mpsc::Receiver<RtInput>) -> Input {
        match rx.recv_timeout(Duration::from_secs(2)).expect("input") {
            RtInput::Core(i) => i,
            _ => panic!("unexpected input kind"),
        }
    }

    #[test]
    fn requests_and_events_cross_the_socket_as_one_datagram_each() {
        let (peer, fd) = pair();
        let (tx, rx) = sync_channel(16);
        let client = HelperClient::spawn(HelperEndpoint::Fd(fd), tx).unwrap();
        assert!(matches!(recv_input(&rx), Input::HelperConnected));
        assert!(client.send(&HelperRequest::Ping { nonce: 7 }));
        let mut buf = [MaybeUninit::<u8>::uninit(); 256];
        let n = peer.recv(&mut buf).unwrap();
        let bytes: Vec<u8> = buf[..n].iter().map(|b| unsafe { b.assume_init() }).collect();
        assert_eq!(serde_json::from_slice::<HelperRequest>(&bytes).unwrap(), HelperRequest::Ping { nonce: 7 });
        peer.send(&serde_json::to_vec(&HelperEvent::Pong { nonce: 7 }).unwrap()).unwrap();
        assert!(matches!(recv_input(&rx), Input::Helper(HelperEvent::Pong { nonce: 7 })));
        // Garbage is dropped, the connection survives.
        peer.send(b"not json").unwrap();
        peer.send(&serde_json::to_vec(&HelperEvent::Pong { nonce: 8 }).unwrap()).unwrap();
        assert!(matches!(recv_input(&rx), Input::Helper(HelperEvent::Pong { nonce: 8 })));
        // Closing the peer is a lost helper, and sends fail afterwards.
        drop(peer);
        assert!(matches!(recv_input(&rx), Input::HelperLost));
        assert!(!client.send(&HelperRequest::Ping { nonce: 9 }));
    }

    #[test]
    fn the_path_endpoint_reconnects_after_a_loss() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("h.sock");
        let listener = seqpacket().unwrap();
        listener.bind(&SockAddr::unix(&path).unwrap()).unwrap();
        listener.listen(4).unwrap();
        let (tx, rx) = sync_channel(16);
        let client = HelperClient::spawn(HelperEndpoint::Path { path: path.clone(), peer_uid: None }, tx).unwrap();
        let (first, _) = listener.accept().unwrap();
        assert!(matches!(recv_input(&rx), Input::HelperConnected));
        drop(first);
        assert!(matches!(recv_input(&rx), Input::HelperLost));
        let (second, _) = listener.accept().unwrap();
        assert!(matches!(recv_input(&rx), Input::HelperConnected));
        assert!(client.send(&HelperRequest::Ping { nonce: 1 }));
        let mut buf = [MaybeUninit::<u8>::uninit(); 64];
        assert!(second.recv(&mut buf).unwrap() > 0);
    }
}

#[cfg(test)]
mod f16_tests {
    use super::*;
    use std::sync::mpsc::sync_channel;

    #[test]
    fn a_peer_with_the_wrong_uid_is_never_used() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("h.sock");
        let listener = seqpacket().unwrap();
        listener.bind(&SockAddr::unix(&path).unwrap()).unwrap();
        listener.listen(4).unwrap();
        let me = unsafe { libc::getuid() };
        let (tx, rx) = sync_channel(16);
        // We are not uid `me + 1`: the connection is refused and no HelperConnected is sent.
        let _c = HelperClient::spawn(HelperEndpoint::Path { path: path.clone(), peer_uid: Some(me + 1) }, tx).unwrap();
        let _ = listener.accept().unwrap();
        assert!(rx.recv_timeout(Duration::from_millis(500)).is_err());
        // The right uid is accepted.
        let (tx, rx) = sync_channel(16);
        let _c = HelperClient::spawn(HelperEndpoint::Path { path, peer_uid: Some(me) }, tx).unwrap();
        let _ = listener.accept().unwrap();
        assert!(matches!(rx.recv_timeout(Duration::from_secs(2)), Ok(RtInput::Core(Input::HelperConnected))));
    }
}
