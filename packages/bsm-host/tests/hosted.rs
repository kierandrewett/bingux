//! End-to-end: the real bridge, a hosted compositor (Gnoblin via
//! `BSM_TEST_GNOBLIN`, otherwise stock headless Mutter plus a Hosted1 stub),
//! private D-Bus and PipeWire, and the stand-in compositor. Proves that real
//! frames arrive as wl_shm buffers and that input reaches a client.
//!
//! Evidence (PNGs) goes to `BSM_HOST_EVIDENCE_DIR` when set.

mod support;

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

use support::mutter::*;
use support::standin::*;

const T: Duration = Duration::from_secs(30);
const OUTPUT_CONFIGURED: u32 = 2;

fn evidence(name: &str, f: &Frame) {
    if let Some(dir) = std::env::var_os("BSM_HOST_EVIDENCE_DIR").map(PathBuf::from) {
        std::fs::create_dir_all(&dir).unwrap();
        save_png(&dir.join(name), f.width, f.height, &f.pixels);
    }
}

fn latest(s: &Standin, out: &str) -> Frame {
    s.shared.lock().unwrap().frames.iter().rev().find(|f| f.output == out).cloned().expect("frame")
}

fn px(f: &Frame, x: u32, y: u32) -> [u8; 3] {
    let i = ((y * f.width + x) * 4) as usize;
    [f.pixels[i + 2], f.pixels[i + 1], f.pixels[i]]
}

/// Bounding box of pixels that differ between two frames of the same size.
fn diff_bbox(a: &Frame, b: &Frame, threshold: i32) -> Option<(u32, u32, u32, u32, usize)> {
    assert_eq!((a.width, a.height), (b.width, b.height));
    let (mut x0, mut y0, mut x1, mut y1, mut n) = (u32::MAX, u32::MAX, 0, 0, 0usize);
    for y in 0..a.height {
        for x in 0..a.width {
            let (p, q) = (px(a, x, y), px(b, x, y));
            let d: i32 = p.iter().zip(q).map(|(a, b)| (*a as i32 - b as i32).abs()).sum();
            if d > threshold {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
                n += 1;
            }
        }
    }
    (n > 0).then_some((x0, y0, x1, y1, n))
}

/// Centroid of the cyan crosshair clickdot draws (R low, G and B high).
fn marker(f: &Frame) -> Option<(f64, f64, usize)> {
    let (mut sx, mut sy, mut n) = (0f64, 0f64, 0usize);
    for y in 0..f.height {
        for x in 0..f.width {
            let [r, g, b] = px(f, x, y);
            if r < 110 && g > 170 && b > 170 {
                sx += x as f64;
                sy += y as f64;
                n += 1;
            }
        }
    }
    (n > 20).then(|| (sx / n as f64, sy / n as f64, n))
}

fn spawn_bridge(env: &HostedEnv, standin: &Standin, generation: &str) -> std::process::Child {
    Command::new(env!("CARGO_BIN_EXE_bsm-host"))
        .env("BSM_HOST_SOCKET", &standin.socket)
        .env("BSM_SESSION_ID", "standin-session")
        .env("BSM_GENERATION", generation)
        .env("XDG_RUNTIME_DIR", &env.runtime)
        .env("DBUS_SESSION_BUS_ADDRESS", &env.bus)
        .env("GNOBLIN_COMPOSITOR_SOCKET", env.runtime.join("none.sock"))
        .env("RUST_LOG", "debug")
        .stdout(Stdio::null())
        .stderr(std::fs::File::create(env.dir.join("bridge.log")).unwrap())
        .spawn()
        .expect("spawn bridge")
}

fn with_logs<R>(env: &HostedEnv, bridge: &mut std::process::Child, f: impl FnOnce() -> R) -> R {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    let _ = bridge.kill();
    let _ = bridge.wait();
    match result {
        Ok(r) => r,
        Err(e) => {
            eprintln!("--- bridge.log ---\n{}", env.log("bridge.log"));
            eprintln!("--- compositor-err.log ---\n{}", env.log("compositor-err.log"));
            std::panic::resume_unwind(e)
        }
    }
}

/// Wait until the newest frame of `out` differs from `base` in at least `min` pixels.
fn wait_changed(s: &Standin, out: &str, base: &Frame, min: usize) {
    s.wait_for("the client window to appear", T, |sh| {
        sh.frames
            .iter()
            .rev()
            .find(|f| f.output == out)
            .is_some_and(|f| diff_bbox(base, f, 30).is_some_and(|b| b.4 >= min))
    });
}

fn wait_frame_after(s: &Standin, out: &str, n_before: usize) -> Frame {
    s.wait_for("a new frame", T, |sh| sh.frames.iter().filter(|f| f.output == out).count() > n_before);
    latest(s, out)
}

fn count(s: &Standin, out: &str) -> usize {
    s.shared.lock().unwrap().frames.iter().filter(|f| f.output == out).count()
}

#[test]
fn real_frames_arrive_and_pointer_input_reaches_a_client() {
    if let Err(why) = available() {
        eprintln!("SKIP: {why}");
        return;
    }
    let env = HostedEnv::start("pointer");
    let standin = Standin::start(&env.dir, "7");
    standin.add_output("HDMI-A-1", OutputSpec::new(0, 0, 800, 600));
    let mut bridge = spawn_bridge(&env, &standin, "7");

    with_logs(&env, &mut bridge, || {
        standin.wait_for("output_configured", T, |s| s.reports.iter().any(|r| r.kind == OUTPUT_CONFIGURED));
        standin.wait_for("a real frame", T, |s| !s.frames.is_empty());
        {
            let s = standin.shared.lock().unwrap();
            assert!(s.violations.is_empty(), "{:?}", s.violations);
            assert!(s.frames.iter().all(|f| f.after_ack));
            let f = &s.frames[0];
            assert_eq!((f.width, f.height), (800, 600));
        }
        std::thread::sleep(Duration::from_millis(500));
        let empty = latest(&standin, "HDMI-A-1");
        evidence("hosted-1-empty-desktop.png", &empty);

        // A client window appears in the hosted session (and settles).
        let mut clickdot = env.client("weston-clickdot", &[]);
        wait_changed(&standin, "HDMI-A-1", &empty, 5000);
        std::thread::sleep(Duration::from_millis(3000));
        let with_window = latest(&standin, "HDMI-A-1");
        let (x0, y0, x1, y1, _) = diff_bbox(&empty, &with_window, 30).expect("the client window must change the frame");
        evidence("hosted-2-client-window.png", &with_window);
        let (wx, wy, _) = marker(&with_window).expect("the client draws a crosshair at its centre");
        // Click well away from the initial crosshair, inside the window.
        let (cx, cy) = (x0 + (x1 - x0) * 3 / 4, y0 + (y1 - y0) * 3 / 4);
        assert!((wx - cx as f64).abs() > 40.0, "test setup: crosshair already near the click point");

        // Inactive: pointer and buttons never reach the session.
        standin.send(Control::PointerEnter("HDMI-A-1".into(), cx as f64, cy as f64));
        standin.send(Control::PointerButton(0x110, true));
        standin.send(Control::PointerButton(0x110, false));
        std::thread::sleep(Duration::from_millis(1000));
        let inactive = latest(&standin, "HDMI-A-1");
        let (ix, iy, _) = marker(&inactive).unwrap();
        assert!((ix - wx).abs() < 2.0 && (iy - wy).abs() < 2.0, "input leaked while inactive: marker moved to ({ix},{iy})");

        // Active: the click lands where the stand-in clicked.
        standin.send(Control::InputState(true));
        std::thread::sleep(Duration::from_millis(300));
        standin.send(Control::PointerMotion(cx as f64 - 3.0, cy as f64 - 3.0));
        standin.send(Control::PointerMotion(cx as f64, cy as f64));
        std::thread::sleep(Duration::from_millis(300));
        standin.send(Control::PointerButton(0x110, true));
        standin.send(Control::PointerButton(0x110, false));
        standin.wait_for("a frame with the marker at the click", T, |s| {
            s.frames.last().and_then(marker).is_some_and(|(mx, my, _)| (mx - cx as f64).abs() < 3.0 && (my - cy as f64).abs() < 3.0)
        });
        let clicked = latest(&standin, "HDMI-A-1");
        evidence("hosted-3-after-click.png", &clicked);
        let (mx, my, _) = marker(&clicked).unwrap();
        eprintln!("clicked at ({cx},{cy}); crosshair now at ({mx:.1},{my:.1}); it was at ({wx:.1},{wy:.1})");
        let _ = clickdot.kill();
        let _ = clickdot.wait();
    });
}

#[test]
fn keyboard_input_reaches_a_terminal_only_while_active() {
    if let Err(why) = available() {
        eprintln!("SKIP: {why}");
        return;
    }
    let env = HostedEnv::start("keyboard");
    let standin = Standin::start(&env.dir, "8");
    standin.add_output("DP-1", OutputSpec::new(0, 0, 800, 600));
    let mut bridge = spawn_bridge(&env, &standin, "8");
    with_logs(&env, &mut bridge, || {
        standin.wait_for("output_configured", T, |s| s.reports.iter().any(|r| r.kind == OUTPUT_CONFIGURED));
        standin.wait_for("a real frame", T, |s| !s.frames.is_empty());
        std::thread::sleep(Duration::from_millis(500));
        let empty = latest(&standin, "DP-1");
        let typed = env.dir.join("typed.txt");
        let script = format!("IFS= read -r line; printf %s \"$line\" > {}; sleep 30", typed.display());
        let mut term = env.client("foot", &["sh", "-c", &script]);
        wait_changed(&standin, "DP-1", &empty, 5000);
        std::thread::sleep(Duration::from_millis(2500));
        let with_term = latest(&standin, "DP-1");
        assert!(diff_bbox(&empty, &with_term, 30).is_some(), "foot did not draw");
        evidence("hosted-4-terminal.png", &with_term);

        // Inactive: keys go nowhere.
        standin.send(Control::KeyboardEnter("DP-1".into()));
        for k in [35u32, 23] {
            standin.send(Control::Key(k, true));
            standin.send(Control::Key(k, false));
        }
        std::thread::sleep(Duration::from_millis(800));
        assert!(!typed.exists(), "keys reached the session while inactive");

        // Active: "hi" and Enter arrive.
        standin.send(Control::InputState(true));
        std::thread::sleep(Duration::from_millis(300));
        for k in [35u32, 23, 28] {
            standin.send(Control::Key(k, true));
            std::thread::sleep(Duration::from_millis(40));
            standin.send(Control::Key(k, false));
            std::thread::sleep(Duration::from_millis(40));
        }
        support::mutter::wait_until(Duration::from_secs(10), || typed.exists(), "the typed line");
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(std::fs::read_to_string(&typed).unwrap(), "hi");
        let _ = term.kill();
        let _ = term.wait();
    });
}

#[test]
fn two_outputs_become_two_monitors_at_the_requested_positions() {
    if let Err(why) = available() {
        eprintln!("SKIP: {why}");
        return;
    }
    let env = HostedEnv::start("two");
    let standin = Standin::start(&env.dir, "9");
    standin.add_output("A", OutputSpec::new(0, 0, 800, 600));
    standin.add_output("B", OutputSpec { scale_120: 240, ..OutputSpec::new(800, 0, 960, 540) });
    let mut bridge = spawn_bridge(&env, &standin, "9");
    with_logs(&env, &mut bridge, || {
        standin.wait_for("both outputs configured", T, |s| s.reports.iter().filter(|r| r.kind == OUTPUT_CONFIGURED).count() == 2);
        standin.wait_for("frames on both", T, |s| {
            s.frames.iter().any(|f| f.output == "A") && s.frames.iter().any(|f| f.output == "B")
        });
        let b = latest(&standin, "B");
        assert_eq!((b.width, b.height), (1920, 1080), "scale 2: 960x540 logical is 1920x1080 pixels");
        // Ask Gnoblin: two logical monitors, B at x=800 with scale 2.
        let conn = zbus::blocking::connection::Builder::address(env.bus.as_str()).unwrap().build().unwrap();
        let reply = conn
            .call_method(
                Some("org.gnome.Mutter.DisplayConfig"),
                "/org/gnome/Mutter/DisplayConfig",
                Some("org.gnome.Mutter.DisplayConfig"),
                "GetCurrentState",
                &(),
            )
            .unwrap();
        let body = reply.body();
        let (_serial, _monitors, logical, _props): (
            u32,
            Vec<bsm_host::gnoblin::DcMonitor>,
            Vec<bsm_host::gnoblin::DcLogical>,
            std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
        ) = body.deserialize().unwrap();
        let mut places: Vec<(i32, i32, f64)> = logical.iter().map(|l| (l.0, l.1, l.2)).collect();
        places.sort_by_key(|p| p.0);
        assert_eq!(places.len(), 2, "{places:?}");
        assert_eq!((places[0].0, places[0].1), (0, 0));
        assert_eq!((places[1].0, places[1].1), (800, 0));
        assert!((places[1].2 - 2.0).abs() < 0.01, "scale of B: {places:?}");
    });
}
