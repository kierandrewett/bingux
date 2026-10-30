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
        self.spawn_with(extra, &[("BSM_HOST_READY_PEERS", "*")])
    }

    fn spawn_with(&self, extra: &[&str], envs: &[(&str, &str)]) -> Bridge {
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
            .envs(envs.iter().copied())
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
    env.standin.wait_for("a frame presented after desktop_ready", T, |s| {
        let ready_at = s.reports.iter().find(|r| r.kind == DESKTOP_READY).unwrap().at;
        s.frames.iter().any(|f| f.at >= ready_at && f.after_ack)
    });
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
fn losing_the_bsm_connection_for_longer_than_the_window_exits_with_status_3() {
    let mut env = Env::new("lost");
    env.standin.add_output("DP-1", OutputSpec::new(0, 0, 160, 100));
    let mut bridge = env.spawn(&["--recovery-window", "2"]);
    env.standin.wait_for("frame", T, |s| !s.frames.is_empty());
    env.standin.stop();
    // The bridge does not give up at once: it keeps the session and retries.
    assert_eq!(bridge.wait_exit(Duration::from_millis(1200)), None, "gave up before the recovery window");
    assert_eq!(bridge.wait_exit(T), Some(3));
}

#[test]
fn bsm_restart_is_survived_without_rebuilding_and_the_state_is_reported_again() {
    let mut env = Env::new("restart");
    env.standin.add_output("HDMI-A-1", OutputSpec::new(0, 0, 320, 200));
    env.standin.add_output("DP-1", OutputSpec::new(320, 0, 240, 150));
    let mut bridge = env.spawn(&["--test-auto-ready-ms", "150"]);
    env.standin.wait_for("desktop_ready", T, |s| kinds(s).contains(&DESKTOP_READY));
    env.standin.send(Control::InputState(true));
    std::thread::sleep(Duration::from_millis(200));

    // bsm goes away.
    env.standin.stop();
    std::thread::sleep(Duration::from_millis(600));
    assert!(bridge.child.try_wait().unwrap().is_none(), "the bridge exited when bsm went away");

    // A new bsm answers on the same path, same generation, same outputs.
    env.standin = Standin::start(&env.dir, GEN);
    env.standin.add_output("HDMI-A-1", OutputSpec::new(0, 0, 320, 200));
    env.standin.add_output("DP-1", OutputSpec::new(320, 0, 240, 150));
    env.standin.wait_for("desktop_ready again", T, |s| kinds(s).contains(&DESKTOP_READY));
    env.standin.wait_for("a frame presented after desktop_ready", T, |s| {
        let ready_at = s.reports.iter().find(|r| r.kind == DESKTOP_READY).unwrap().at;
        s.frames.iter().any(|f| f.at >= ready_at && f.after_ack)
    });
    let s = env.standin.shared.lock().unwrap();
    assert!(s.violations.is_empty(), "{:?}", s.violations);
    assert!(s.frames.iter().all(|f| f.after_ack), "a buffer before the ack on the new connection");
    let k = kinds(&s);
    // the whole state is reported again for the same generation
    assert_eq!(k.iter().filter(|x| **x == GNOBLIN_READY).count(), 1);
    assert_eq!(k.iter().filter(|x| **x == BINGUX_LAUNCHED).count(), 1);
    assert_eq!(k.iter().filter(|x| **x == OUTPUT_CONFIGURED).count(), 2);
    assert_eq!(k.iter().filter(|x| **x == DESKTOP_READY).count(), 1);
    assert!(s.reports.iter().all(|r| r.generation == GEN));
    // both outputs got a surface, a configure and an ack on the new connection
    assert_eq!(s.acks.len(), 2);
    drop(s);
    // Input is revoked until the new bsm says otherwise.
    let before = bridge.input_events().len();
    env.standin.send(Control::PointerEnter("DP-1".into(), 5.0, 5.0));
    env.standin.send(Control::PointerMotion(6.0, 6.0));
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(bridge.input_events().len(), before, "input leaked after the reconnect");
    env.standin.send(Control::InputState(true));
    std::thread::sleep(Duration::from_millis(200));
    env.standin.send(Control::PointerMotion(7.0, 7.0));
    std::thread::sleep(Duration::from_millis(300));
    assert!(bridge.input_events().iter().any(|e| e["ev"] == "motion" && e["x"] == 7.0));
    // and the bridge is still the same process
    assert!(bridge.child.try_wait().unwrap().is_none());
}

#[test]
fn outputs_that_changed_during_the_outage_are_reconciled() {
    let mut env = Env::new("reconcile");
    env.standin.add_output("A", OutputSpec::new(0, 0, 160, 100));
    env.standin.add_output("B", OutputSpec::new(160, 0, 160, 100));
    let bridge = env.spawn(&["--test-auto-ready-ms", "150"]);
    env.standin.wait_for("desktop_ready", T, |s| kinds(s).contains(&DESKTOP_READY));
    env.standin.stop();
    env.standin = Standin::start(&env.dir, GEN);
    // B is gone, C is new and A is bigger.
    env.standin.add_output("A", OutputSpec::new(0, 0, 200, 120));
    env.standin.add_output("C", OutputSpec::new(200, 0, 100, 60));
    env.standin.wait_for("frames on A and C", T, |s| {
        s.frames.iter().any(|f| f.output == "A" && (f.width, f.height) == (200, 120)) && s.frames.iter().any(|f| f.output == "C")
    });
    env.standin.wait_for("desktop_ready for the new set", T, |s| kinds(s).contains(&DESKTOP_READY));
    let s = env.standin.shared.lock().unwrap();
    assert!(s.violations.is_empty(), "{:?}", s.violations);
    assert!(s.frames.iter().all(|f| f.output != "B"), "a frame for the vanished output");
    let last_ready = s.reports.iter().rposition(|r| r.kind == DESKTOP_READY).unwrap();
    assert_eq!((s.reports[last_ready].a, s.reports[last_ready].b), (2, 2));
    drop(s);
    drop(bridge);
}

#[test]
fn a_different_generation_after_the_reconnect_is_a_protocol_error() {
    let mut env = Env::new("othergen");
    env.standin.add_output("DP-1", OutputSpec::new(0, 0, 160, 100));
    let mut bridge = env.spawn(&[]);
    env.standin.wait_for("frame", T, |s| !s.frames.is_empty());
    env.standin.stop();
    env.standin = Standin::start(&env.dir, "43");
    env.standin.add_output("DP-1", OutputSpec::new(0, 0, 160, 100));
    assert_eq!(bridge.wait_exit(T), Some(5));
}

#[test]
fn closed_after_a_reconnect_still_ends_the_bridge_cleanly() {
    let mut env = Env::new("closedafter");
    env.standin.add_output("DP-1", OutputSpec::new(0, 0, 160, 100));
    let mut bridge = env.spawn(&[]);
    env.standin.wait_for("frame", T, |s| !s.frames.is_empty());
    env.standin.stop();
    env.standin = Standin::start(&env.dir, GEN);
    env.standin.add_output("DP-1", OutputSpec::new(0, 0, 160, 100));
    env.standin.wait_for("frame on the new connection", T, |s| !s.frames.is_empty());
    env.standin.send(Control::Close);
    assert_eq!(bridge.wait_exit(T), Some(0));
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

#[test]
fn a_same_uid_process_that_is_not_the_bingux_shell_cannot_report() {
    let env = Env::new("peers");
    env.standin.add_output("DP-1", OutputSpec::new(0, 0, 160, 100));
    // Default peer policy: only quickshell, qs, bingux and bingux-wallpaper may report.
    let bridge = env.spawn_with(&[], &[]);
    env.standin.wait_for("output_configured", T, |s| kinds(s).contains(&OUTPUT_CONFIGURED));
    let mut s = UnixStream::connect(bridge.ready_sock()).expect("connect");
    let _ = s.write_all(format!("{{\"generation\":\"{GEN}\",\"component\":\"wallpaper\",\"event\":\"launched\"}}\n").as_bytes());
    let _ = s.write_all(format!("{{\"generation\":\"{GEN}\",\"component\":\"desktop\",\"event\":\"launched\"}}\n").as_bytes());
    std::thread::sleep(Duration::from_millis(500));
    assert!(!kinds(&env.standin.shared.lock().unwrap()).contains(&BINGUX_LAUNCHED), "an unknown peer reported");
    // The same lines from a process that carries an allowed name are accepted.
    let mut env2 = Env::new("peers2");
    env2.standin.add_output("DP-1", OutputSpec::new(0, 0, 160, 100));
    let name = std::env::current_exe().unwrap().file_name().unwrap().to_string_lossy().into_owned();
    let bridge2 = env2.spawn_with(&[], &[("BSM_HOST_READY_PEERS", &name)]);
    env2.standin.wait_for("output_configured", T, |s| kinds(s).contains(&OUTPUT_CONFIGURED));
    bridge2.send_line(&format!("{{\"generation\":\"{GEN}\",\"component\":\"wallpaper\",\"event\":\"launched\"}}"));
    bridge2.send_line(&format!("{{\"generation\":\"{GEN}\",\"component\":\"desktop\",\"event\":\"launched\"}}"));
    env2.standin.wait_for("bingux_launched", T, |s| kinds(s).contains(&BINGUX_LAUNCHED));
    env2.standin.stop();
    drop(bridge);
}

#[test]
fn a_connection_that_sends_nothing_is_closed_and_frees_its_slot() {
    use std::io::Read;
    let env = Env::new("idle");
    env.standin.add_output("DP-1", OutputSpec::new(0, 0, 160, 100));
    let bridge = env.spawn_with(&[], &[("BSM_HOST_READY_PEERS", "*"), ("BSM_HOST_READY_IDLE_SECS", "1")]);
    env.standin.wait_for("output_configured", T, |s| kinds(s).contains(&OUTPUT_CONFIGURED));
    // Sixteen idle connections take every slot...
    let mut idle: Vec<UnixStream> = (0..16).map(|_| UnixStream::connect(bridge.ready_sock()).unwrap()).collect();
    std::thread::sleep(Duration::from_millis(300));
    // ...and are closed after the idle time, so the real reporter still gets in.
    idle[0].set_read_timeout(Some(Duration::from_secs(4))).unwrap();
    let mut b = [0u8; 1];
    assert_eq!(idle[0].read(&mut b).unwrap_or(1), 0, "the idle connection was not closed");
    bridge.send_line(&format!("{{\"generation\":\"{GEN}\",\"component\":\"wallpaper\",\"event\":\"launched\"}}"));
    bridge.send_line(&format!("{{\"generation\":\"{GEN}\",\"component\":\"desktop\",\"event\":\"launched\"}}"));
    env.standin.wait_for("bingux_launched", T, |s| kinds(s).contains(&BINGUX_LAUNCHED));
    idle.clear();
}

#[test]
fn a_half_open_bsm_cannot_block_the_recovery_loop() {
    use std::os::unix::net::UnixListener;
    let mut env = Env::new("halfopen");
    env.standin.add_output("DP-1", OutputSpec::new(0, 0, 160, 100));
    let bridge = env.spawn_with(
        &["--recovery-window", "60"],
        &[("BSM_HOST_READY_PEERS", "*"), ("BSM_HOST_HANDSHAKE_SECS", "2")],
    );
    env.standin.wait_for("frame", T, |s| !s.frames.is_empty());
    let path = env.standin.socket.clone();
    env.standin.stop();
    // A peer that accepts and never answers the registry roundtrip.
    let _ = std::fs::remove_file(&path);
    let hole = UnixListener::bind(&path).unwrap();
    let held = std::thread::spawn(move || {
        let mut conns = Vec::new();
        hole.set_nonblocking(true).unwrap();
        let end = std::time::Instant::now() + Duration::from_secs(8);
        while std::time::Instant::now() < end {
            if let Ok((c, _)) = hole.accept() {
                conns.push(c);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        conns.len()
    });
    // Longer than one handshake timeout: without the timeout the bridge would sit in the roundtrip.
    std::thread::sleep(Duration::from_millis(4500));
    env.standin = Standin::start(&env.dir, GEN);
    env.standin.add_output("DP-1", OutputSpec::new(0, 0, 160, 100));
    env.standin.wait_for("a frame after the half-open peer was replaced", Duration::from_secs(15), |s| {
        s.frames.iter().any(|f| f.after_ack)
    });
    assert!(held.join().unwrap() >= 2, "the bridge did not retry after the first handshake timed out");
    drop(bridge);
}
