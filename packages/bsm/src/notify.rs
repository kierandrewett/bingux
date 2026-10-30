//! `sd_notify`: readiness and watchdog over `$NOTIFY_SOCKET`. Does nothing without it.

use std::os::unix::net::UnixDatagram;
use std::time::Duration;

/// Send one state string to systemd. Returns false when not running under systemd or on error.
pub fn notify(state: &str) -> bool {
    let Some(path) = std::env::var_os("NOTIFY_SOCKET") else { return false };
    notify_to(&path, state)
}

pub fn notify_to(path: &std::ffi::OsStr, state: &str) -> bool {
    let Ok(sock) = UnixDatagram::unbound() else { return false };
    let bytes = path.as_encoded_bytes();
    // A leading '@' is the abstract namespace.
    let r = if bytes.first() == Some(&b'@') {
        use std::os::linux::net::SocketAddrExt;
        match std::os::unix::net::SocketAddr::from_abstract_name(&bytes[1..]) {
            Ok(addr) => sock.send_to_addr(state.as_bytes(), &addr).map(|_| ()),
            Err(e) => Err(e),
        }
    } else {
        sock.send_to(state.as_bytes(), path).map(|_| ())
    };
    r.is_ok()
}

/// Interval at which `WATCHDOG=1` must be sent: half of `WATCHDOG_USEC`, if set for this process.
pub fn watchdog_interval() -> Option<Duration> {
    let usec: u64 = std::env::var("WATCHDOG_USEC").ok()?.parse().ok()?;
    if let Ok(pid) = std::env::var("WATCHDOG_PID") {
        if pid.parse::<u32>().ok()? != std::process::id() {
            return None;
        }
    }
    if usec == 0 {
        return None;
    }
    Some(Duration::from_micros(usec / 2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_reaches_a_notify_socket() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("n.sock");
        let server = UnixDatagram::bind(&p).unwrap();
        server.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        assert!(notify_to(p.as_os_str(), "READY=1"));
        let mut buf = [0u8; 32];
        let n = server.recv(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"READY=1");
    }

    #[test]
    fn no_socket_is_not_an_error() {
        assert!(!notify_to(std::ffi::OsStr::new("/nonexistent/notify.sock"), "READY=1"));
    }
}
