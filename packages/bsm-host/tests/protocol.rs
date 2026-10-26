//! Protocol-level tests of the bridge against a stand-in compositor, using the
//! feature-gated `--test-pattern` mode (no Gnoblin). Run with
//! `cargo test --features test-pattern`.
#![cfg(feature = "test-pattern")]

mod support;

use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use support::standin::*;

const GEN: &str = "42";
const T: Duration = Duration::from_secs(10);

// report_kind values of the protocol
const GNOBLIN_READY: u32 = 1;
const OUTPUT_CONFIGURED: u32 = 2;
const BINGUX_LAUNCHED: u32 = 3;
const SURFACES_MAPPED: u32 = 4;
const DESKTOP_READY: u32 = 5;

struct Env {
    dir: PathBuf,
    standin: Standin,
}

struct Bridge {
    child: Child,
    input_log: PathBuf,
    runtime: PathBuf,
}

impl Env {
    fn new(name: &str) -> Env {
        let dir = std::env::temp_dir().join(format!("bsm-host-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("run")).unwrap();
        let standin = Standin::start(&dir, GEN);
        Env { dir, standin }
    }

    fn spawn(&self, extra: &[&str]) -> Bridge {
        let runtime = self.dir.join("run");
        let input_log = self.dir.join("input.jsonl");
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_bsm-host"));
        cmd.arg("--test-pattern")
            .arg("--test-input-log")
            .arg(&input_log)
            .args(extra)
            .env("BSM_HOST_SOCKET", &self.standin.socket)
            .env("BSM_SESSION_ID", "standin-session")
            .env("BSM_GENERATION", GEN)
            .env("XDG_RUNTIME_DIR", &runtime)
            .env("RUST_LOG", "debug")
            .stdin(Stdio::null());
        Bridge { child: cmd.spawn().expect("spawn bsm-host"), input_log, runtime }
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl Bridge {
    fn ready_sock(&self) -> PathBuf {
        self.runtime.join("bsm/ready.sock")
    }
    fn wait_exit(&mut self, timeout: Duration) -> Option<i32> {
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            if let Some(s) = self.child.try_wait().unwrap() {
                return s.code();
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        None
    }
    fn input_events(&self) -> Vec<serde_json::Value> {
        std::fs::read_to_string(&self.input_log)
            .unwrap_or_default()
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect()
    }
    fn send_line(&self, line: &str) {
        let mut s = UnixStream::connect(self.ready_sock()).expect("connect ready.sock");
        s.write_all(line.as_bytes()).unwrap();
        s.write_all(b"\n").unwrap();
        std::thread::sleep(Duration::from_millis(50));
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn kinds(s: &Shared) -> Vec<u32> {
    s.reports.iter().map(|r| r.kind).collect()
}

fn evidence_dir() -> Option<PathBuf> {
    std::env::var_os("BSM_HOST_EVIDENCE_DIR").map(PathBuf::from)
}

#[test]
fn presents_only_after_ack_and_reports_every_milestone_in_order() {
    let env = Env::new("full");
    env.standin.add_output("HDMI-A-1", OutputSpec::new(0, 0, 640, 360));
    env.standin.add_output("DP-1", OutputSpec { scale_120: 180, ..OutputSpec::new(640, 0, 320, 200) });
    let bridge = env.spawn(&["--test-auto-ready-ms", "150"]);
    env.standin.wait_for("desktop_ready", T, |s| kinds(s).contains(&DESKTOP_READY));
    // a frame must follow desktop_ready (the reveal gate counts it)
    let after = env.standin.shared.lock().unwrap().frames.len();
    env.standin.wait_for("a frame after desktop_ready", T, |s| s.frames.len() > after);
    let s = env.standin.shared.lock().unwrap();

    assert!(s.violations.is_empty(), "violations: {:?}", s.violations);
    assert!(s.frames.iter().all(|f| f.after_ack), "a buffer was committed before the ack");
    let k = kinds(&s);
    let pos = |kind: u32| k.iter().position(|x| *x == kind).unwrap();
    assert_eq!(k.iter().filter(|x| **x == GNOBLIN_READY).count(), 1);
    assert_eq!(k.iter().filter(|x| **x == OUTPUT_CONFIGURED).count(), 2);
    assert_eq!(k.iter().filter(|x| **x == BINGUX_LAUNCHED).count(), 1);
    assert_eq!(k.iter().filter(|x| **x == DESKTOP_READY).count(), 1);
    assert!(pos(GNOBLIN_READY) < pos(OUTPUT_CONFIGURED));
    assert!(pos(BINGUX_LAUNCHED) < pos(DESKTOP_READY));
    // surfaces_mapped ends at 3 of 3 on each output, before desktop_ready
    for out in ["HDMI-A-1", "DP-1"] {
        let last = s
            .reports
            .iter()
            .rposition(|r| r.kind == SURFACES_MAPPED && r.output.as_deref() == Some(out))
            .unwrap_or_else(|| panic!("no surfaces_mapped for {out}"));
        assert_eq!((s.reports[last].a, s.reports[last].b), (3, 3));
        assert!(last < pos(DESKTOP_READY));
    }
    // counted output progress
    let cfg: Vec<(u32, u32)> = s.reports.iter().filter(|r| r.kind == OUTPUT_CONFIGURED).map(|r| (r.a, r.b)).collect();
    assert_eq!(cfg, vec![(1, 2), (2, 2)]);
    // frame sizes follow the configure (fractional scale 1.5: 320x200 logical -> 480x300 pixels)
    let dp = s.frames.iter().find(|f| f.output == "DP-1").expect("frame on DP-1");
    assert_eq!((dp.width, dp.height), (480, 300));
    let hdmi = s.frames.iter().find(|f| f.output == "HDMI-A-1").unwrap();
    assert_eq!((hdmi.width, hdmi.height), (640, 360));
    // the pattern really arrived: opaque pixels with the white border
    assert_eq!(&hdmi.pixels[0..4], &[0xff, 0xff, 0xff, 0xff]);
    if let Some(dir) = evidence_dir() {
        std::fs::create_dir_all(&dir).unwrap();
        for out in ["HDMI-A-1", "DP-1"] {
            let f = s.frames.iter().rev().find(|f| f.output == out).unwrap();
            save_png(&dir.join(format!("testpattern-{out}.png")), f.width, f.height, &f.pixels);
        }
    }
    drop(s);
    drop(bridge);
}

#[test]
fn ready_socket_rejects_stale_generation_and_requires_all_components() {
    let env = Env::new("sock");
    env.standin.add_output("DP-1", OutputSpec::new(0, 0, 320, 200));
    let bridge = env.spawn(&[]);
    env.standin.wait_for("output_configured", T, |s| kinds(s).contains(&OUTPUT_CONFIGURED));
    assert!(bridge.ready_sock().exists());
    let mode = std::fs::metadata(bridge.ready_sock()).unwrap();
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(mode.permissions().mode() & 0o777, 0o600);

    // a line for another generation changes nothing
    bridge.send_line(r#"{"generation":"41","component":"wallpaper","event":"launched"}"#);
    bridge.send_line(r#"{"generation":"41","component":"desktop","event":"launched"}"#);
    std::thread::sleep(Duration::from_millis(200));
    assert!(!kinds(&env.standin.shared.lock().unwrap()).contains(&BINGUX_LAUNCHED));

    // one component is not enough
    bridge.send_line(&format!(r#"{{"generation":"{GEN}","component":"wallpaper","event":"launched"}}"#));
    std::thread::sleep(Duration::from_millis(200));
    assert!(!kinds(&env.standin.shared.lock().unwrap()).contains(&BINGUX_LAUNCHED));
    bridge.send_line(&format!(r#"{{"generation":"{GEN}","component":"desktop","event":"launched"}}"#));
    env.standin.wait_for("bingux_launched", T, |s| kinds(s).contains(&BINGUX_LAUNCHED));

    // ready without mapped surfaces is not desktop_ready
    bridge.send_line(&format!(r#"{{"generation":"{GEN}","component":"wallpaper","event":"ready"}}"#));
    bridge.send_line(&format!(r#"{{"generation":"{GEN}","component":"desktop","event":"ready"}}"#));
    std::thread::sleep(Duration::from_millis(300));
    assert!(!kinds(&env.standin.shared.lock().unwrap()).contains(&DESKTOP_READY));
    // all three surfaces on the monitor connector complete it
    for (c, r) in [("wallpaper", "wallpaper"), ("desktop", "bar"), ("desktop", "dock")] {
        bridge.send_line(&format!(
            r#"{{"generation":"{GEN}","component":"{c}","event":"surface","output":"Test-1","role":"{r}","mapped":true}}"#
        ));
    }
    env.standin.wait_for("desktop_ready", T, |s| kinds(s).contains(&DESKTOP_READY));
    assert!(env.standin.shared.lock().unwrap().violations.is_empty());
}

#[test]
fn oversized_ready_line_closes_the_connection_only() {
    let env = Env::new("big");
    env.standin.add_output("DP-1", OutputSpec::new(0, 0, 320, 200));
    let bridge = env.spawn(&[]);
    env.standin.wait_for("output_configured", T, |s| kinds(s).contains(&OUTPUT_CONFIGURED));
    let mut s = UnixStream::connect(bridge.ready_sock()).unwrap();
    let junk = vec![b'x'; 10_000];
    let _ = s.write_all(&junk);
    std::thread::sleep(Duration::from_millis(300));
    // the bridge is still alive and still accepts good connections
    bridge.send_line(&format!(r#"{{"generation":"{GEN}","component":"wallpaper","event":"launched"}}"#));
    let mut bridge = bridge;
    assert!(bridge.child.try_wait().unwrap().is_none(), "bridge died on a junk line");
}

#[test]
fn input_is_forwarded_only_while_active_and_released_on_revoke() {
    let env = Env::new("input");
    env.standin.add_output("DP-1", OutputSpec::new(0, 0, 320, 200));
    let bridge = env.spawn(&[]);
    env.standin.wait_for("output_configured", T, |s| kinds(s).contains(&OUTPUT_CONFIGURED));

    // inactive: nothing is forwarded
    env.standin.send(Control::PointerEnter("DP-1".into(), 10.0, 10.0));
    env.standin.send(Control::KeyboardEnter("DP-1".into()));
    env.standin.send(Control::Key(30, true));
    env.standin.send(Control::Key(30, false));
    env.standin.send(Control::PointerMotion(11.0, 12.0));
    env.standin.send(Control::PointerButton(0x110, true));
    std::thread::sleep(Duration::from_millis(300));
    assert!(bridge.input_events().iter().all(|e| e["ev"] == "release_all"), "{:?}", bridge.input_events());

    // active: keys, pointer and buttons arrive, mapped to the focused output
    env.standin.send(Control::InputState(true));
    std::thread::sleep(Duration::from_millis(100));
    env.standin.send(Control::PointerMotion(100.0, 50.0));
    env.standin.send(Control::Key(30, true));
    env.standin.send(Control::PointerButton(0x110, true));
    env.standin.send(Control::PointerAxisDiscrete(0, 120));
    std::thread::sleep(Duration::from_millis(300));
    let ev = bridge.input_events();
    assert!(ev.iter().any(|e| e["ev"] == "motion" && e["output"] == "DP-1" && e["x"] == 100.0 && e["y"] == 50.0), "{ev:?}");
    assert!(ev.iter().any(|e| e["ev"] == "key" && e["code"] == 30 && e["pressed"] == true), "{ev:?}");
    assert!(ev.iter().any(|e| e["ev"] == "button" && e["code"] == 0x110 && e["pressed"] == true), "{ev:?}");
    assert!(ev.iter().any(|e| e["ev"] == "scroll_discrete" && e["y"] == 120), "{ev:?}");

    // revoke: held input is released and later input is dropped
    let before = bridge.input_events().len();
    env.standin.send(Control::InputState(false));
    std::thread::sleep(Duration::from_millis(150));
    env.standin.send(Control::Key(31, true));
    env.standin.send(Control::PointerMotion(5.0, 5.0));
    std::thread::sleep(Duration::from_millis(300));
    let ev = bridge.input_events();
    assert_eq!(ev[before]["ev"], "release_all", "{ev:?}");
    assert_eq!(ev.len(), before + 1, "input leaked while inactive: {:?}", &ev[before..]);
}

#[test]
fn background_session_is_throttled_and_foreground_is_not() {
    let env = Env::new("throttle");
    env.standin.add_output("DP-1", OutputSpec::new(0, 0, 160, 100));
    let _bridge = env.spawn(&["--background-fps", "10"]);
    env.standin.wait_for("frames", T, |s| s.frames.len() > 3);
    let count = |secs: f64| {
        let start = env.standin.shared.lock().unwrap().frames.len();
        std::thread::sleep(Duration::from_secs_f64(secs));
        env.standin.shared.lock().unwrap().frames.len() - start
    };
    let background = count(1.0);
    env.standin.send(Control::InputState(true));
    std::thread::sleep(Duration::from_millis(200));
    let foreground = count(1.0);
    assert!(background <= 14, "background forwarded {background} frames in 1 s");
    assert!(background >= 5, "background starved: {background} frames in 1 s");
    assert!(foreground >= 40, "foreground only {foreground} frames in 1 s");
}

#[test]
fn resize_reconfigures_and_presents_the_new_size_only_after_ack() {
    let env = Env::new("resize");
    env.standin.add_output("DP-1", OutputSpec::new(0, 0, 320, 200));
    let _bridge = env.spawn(&[]);
    env.standin.wait_for("first frame", T, |s| !s.frames.is_empty());
    env.standin.send(Control::Reconfigure("DP-1".into(), OutputSpec::new(0, 0, 400, 250)));
    env.standin.wait_for("frame at the new size", T, |s| s.frames.iter().any(|f| (f.width, f.height) == (400, 250)));
    let s = env.standin.shared.lock().unwrap();
    assert!(s.violations.is_empty(), "{:?}", s.violations);
    assert_eq!(s.acks.len(), 2);
}

#[test]
fn closed_event_ends_the_bridge_cleanly() {
    let env = Env::new("closed");
    env.standin.add_output("DP-1", OutputSpec::new(0, 0, 160, 100));
    let mut bridge = env.spawn(&[]);
    env.standin.wait_for("frame", T, |s| !s.frames.is_empty());
    env.standin.send(Control::Close);
    assert_eq!(bridge.wait_exit(T), Some(0));
    assert!(!bridge.ready_sock().exists(), "ready.sock left behind");
}

#[test]
fn losing_the_bsm_connection_exits_with_status_3() {
    let mut env = Env::new("lost");
    env.standin.add_output("DP-1", OutputSpec::new(0, 0, 160, 100));
    let mut bridge = env.spawn(&[]);
    env.standin.wait_for("frame", T, |s| !s.frames.is_empty());
    env.standin.stop();
    assert_eq!(bridge.wait_exit(T), Some(3));
}

#[test]
fn missing_socket_env_is_a_usage_error() {
    let out = Command::new(env!("CARGO_BIN_EXE_bsm-host"))
        .env_remove("BSM_HOST_SOCKET")
        .env("XDG_RUNTIME_DIR", "/tmp")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("BSM_HOST_SOCKET"));
}

#[allow(dead_code)]
fn _unused(_: &Path) {}
