//! `LISTEN_FDS` handover as systemd's socket unit does it.

mod common;

use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::Duration;

use bsm_proto::helper::{HelperEvent, HelperRequest};
use common::*;

#[test]
fn a_socket_passed_by_systemd_is_served_and_never_unlinked() {
    if skip_as_root() {
        return;
    }
    let env = Env::new();
    let cfg_path = env.path("helper.toml");
    std::fs::write(&cfg_path, env.config()).unwrap();
    let listener = bsm_helper::sys::listen_seqpacket(&env.socket(), 0o600).unwrap();
    let fd = listener.as_raw_fd();
    // `exec` keeps the pid, so LISTEN_PID=$$ names the helper as systemd would.
    let mut cmd = Command::new("/bin/sh");
    cmd.arg("-c")
        .arg("LISTEN_PID=$$ LISTEN_FDS=1 exec \"$0\" --config \"$1\"")
        .arg(env!("CARGO_BIN_EXE_bsm-helper"))
        .arg(&cfg_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null());
    unsafe {
        cmd.pre_exec(move || {
            if fd != 3 && libc::dup2(fd, 3) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            let flags = libc::fcntl(3, libc::F_GETFD);
            libc::fcntl(3, libc::F_SETFD, flags & !libc::FD_CLOEXEC);
            Ok(())
        });
    }
    let mut child = cmd.spawn().unwrap();
    // The socket is already listening, so a request queues until the helper runs.
    let c = env.connect();
    c.send(&HelperRequest::Ping { nonce: 9 });
    assert_eq!(c.try_recv(Duration::from_secs(10)), Some(HelperEvent::Pong { nonce: 9 }));
    unsafe { libc::kill(child.id() as i32, libc::SIGTERM) };
    assert!(child.wait().unwrap().success());
    assert!(env.socket().exists(), "a socket owned by the socket unit must not be unlinked by the service");
    // It is still usable by the next instance.
    let again = env.connect();
    drop(again);
    drop(listener);
}
