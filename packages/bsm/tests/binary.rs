//! The real `bsm` and `bsmctl` binaries, run as processes.
//!
//! * `bsm --backend headless` is a process named `bsm` (`/proc/<pid>/comm`, `ps -o comm=`),
//!   tells systemd it is ready, exposes `org.bingux.Session1` and stops cleanly on SIGTERM.
//! * `bsmctl` talks to it over a private bus, in JSON, and has no way to unlock anything.

use std::io::{BufRead, BufReader, Read};
use std::os::unix::net::UnixDatagram;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const BSM: &str = env!("CARGO_BIN_EXE_bsm");
const BSMCTL: &str = env!("CARGO_BIN_EXE_bsmctl");

fn wait_until(what: &str, timeout: Duration, mut f: impl FnMut() -> bool) {
    let end = Instant::now() + timeout;
    while !f() {
        assert!(Instant::now() < end, "timed out: {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

struct PrivateBus {
    child: Child,
    pub address: String,
}

impl PrivateBus {
    fn start() -> PrivateBus {
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("dbus-daemon (package dbus) is required for this test");
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap()).read_line(&mut line).unwrap();
        PrivateBus { child, address: line.trim().to_owned() }
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct Running {
    child: Child,
    dir: tempfile::TempDir,
    notify: UnixDatagram,
}

fn start_bsm(dbus: &str) -> Running {
    let dir = tempfile::tempdir().unwrap();
    let notify_path = dir.path().join("notify.sock");
    let notify = UnixDatagram::bind(&notify_path).unwrap();
    notify.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
    let child = Command::new(BSM)
        .args(["--backend", "headless", "--outputs", "320x200", "--dbus", dbus])
        .arg("--runtime-dir")
        .arg(dir.path().join("run"))
        .arg("--state-dir")
        .arg(dir.path().join("state"))
        .arg("--helper-socket")
        .arg(dir.path().join("no-helper.sock"))
        .env("NOTIFY_SOCKET", &notify_path)
        .env("BSM_LOG", "info")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("bsm starts");
    Running { child, dir, notify }
}

impl Running {
    fn pid(&self) -> u32 {
        self.child.id()
    }
    fn wait_ready(&self) -> String {
        let mut got = String::new();
        let mut buf = [0u8; 256];
        loop {
            let n = self.notify.recv(&mut buf).expect("bsm sent no sd_notify message in time");
            got.push_str(&String::from_utf8_lossy(&buf[..n]));
            if got.contains("READY=1") {
                return got;
            }
        }
    }
    fn control_socket(&self) -> PathBuf {
        self.dir.path().join("run/greeter-control.sock")
    }
    fn stop(mut self) -> (std::process::ExitStatus, String) {
        // SAFETY: plain kill(2) of our own child.
        unsafe { libc::kill(self.child.id() as i32, libc::SIGTERM) };
        let end = Instant::now() + Duration::from_secs(10);
        let status = loop {
            if let Some(s) = self.child.try_wait().unwrap() {
                break s;
            }
            assert!(Instant::now() < end, "bsm did not stop on SIGTERM");
            std::thread::sleep(Duration::from_millis(20));
        };
        let mut err = String::new();
        self.child.stderr.take().unwrap().read_to_string(&mut err).unwrap();
        (status, err)
    }
}

#[test]
fn version_help_and_bad_options() {
    let v = Command::new(BSM).arg("--version").output().unwrap();
    assert!(v.status.success());
    assert!(String::from_utf8_lossy(&v.stdout).starts_with("bsm 0.1.0"));
    let h = Command::new(BSM).arg("--help").output().unwrap();
    assert!(String::from_utf8_lossy(&h.stdout).contains("--backend"));
    let bad = Command::new(BSM).args(["--backend", "x11"]).output().unwrap();
    assert_eq!(bad.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&bad.stderr).contains("unknown backend"));
    let cfg = Command::new(BSM).args(["--config", "/nonexistent/bsm.toml"]).output().unwrap();
    assert_eq!(cfg.status.code(), Some(2));
    let c = Command::new(BSMCTL).arg("--version").output().unwrap();
    assert!(String::from_utf8_lossy(&c.stdout).starts_with("bsmctl 0.1.0"));
}

#[test]
fn the_process_is_named_bsm_reports_ready_and_stops_on_sigterm() {
    let bus = PrivateBus::start();
    let bsm = start_bsm(&bus.address);
    let pid = bsm.pid();
    let msg = bsm.wait_ready();
    assert!(msg.contains("READY=1"));
    wait_until("the control socket", Duration::from_secs(10), || bsm.control_socket().exists());

    // The acceptance check: the persistent process is named bsm.
    let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).unwrap();
    assert_eq!(comm.trim(), "bsm");
    let ps = Command::new("ps").args(["-o", "comm=", "-p", &pid.to_string()]).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&ps.stdout).trim(), "bsm");
    eprintln!("ps -o comm= -p {pid}: {}", String::from_utf8_lossy(&ps.stdout).trim());

    // bsmctl, as an ordinary user, over the private bus.
    let ctl = |args: &[&str]| {
        let mut full = vec!["--bus", bus.address.as_str(), "--json"];
        full.extend_from_slice(args);
        Command::new(BSMCTL).args(full).output().unwrap()
    };
    let status = ctl(&["status"]);
    assert!(status.status.success(), "{}", String::from_utf8_lossy(&status.stderr));
    let v: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(v["outputs"], 1);
    assert_eq!(v["helper_connected"], false);
    let outputs: serde_json::Value = serde_json::from_slice(&ctl(&["outputs"]).stdout).unwrap();
    assert_eq!(outputs[0]["width"], 320);
    assert!(ctl(&["sessions"]).status.success());
    assert!(ctl(&["users"]).status.success());
    assert!(ctl(&["transitions"]).status.success());

    // Requests that this caller may not make are refused with stable codes.
    // (In development the greeter account is the user running bsm, so `select-user`, which the
    // greeter may call, is not in this list; the core tests cover who may call it.)
    for args in [&["lock"][..], &["logout"][..], &["reboot"][..], &["poweroff"][..]] {
        let o = ctl(args);
        assert_eq!(o.status.code(), Some(1), "{args:?}");
        let e: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
        assert!(e["error"] == "unauthorised" || e["error"] == "no_session", "{args:?}: {e}");
    }
    // There is no command that could unlock or reveal anything.
    for word in ["unlock", "login", "activate"] {
        assert_eq!(ctl(&[word]).status.code(), Some(2));
    }

    let (status, log) = bsm.stop();
    assert!(status.success(), "bsm exited with {status:?}:\n{log}");
    assert!(log.contains("ready"), "{log}");
    assert!(!Path::new(&format!("/proc/{pid}")).exists() || std::fs::read_to_string(format!("/proc/{pid}/comm")).is_err());
}

#[test]
fn without_a_bus_the_service_still_runs_and_refuses_system_actions() {
    let bsm = start_bsm("none");
    bsm.wait_ready();
    wait_until("the control socket", Duration::from_secs(10), || bsm.control_socket().exists());
    let (status, log) = bsm.stop();
    assert!(status.success(), "{log}");
}
