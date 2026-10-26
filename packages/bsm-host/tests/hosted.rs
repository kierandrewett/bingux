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
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bsm-host"));
    if !env.real_gnoblin {
        cmd.env("GNOBLIN_COMPOSITOR_SOCKET", env.runtime.join("none.sock"));
    }
    cmd.env("BSM_HOST_SOCKET", &standin.socket)
        .env("BSM_SESSION_ID", "standin-session")
        .env("BSM_GENERATION", generation)
        .env("XDG_RUNTIME_DIR", &env.runtime)
        .env("DBUS_SESSION_BUS_ADDRESS", &env.bus)
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
        // The cursor image comes from Mutter's stream metadata and is set as a client cursor.
        standin.wait_for("a client cursor", T, |s| s.cursor_sets.iter().any(|c| c.0) && s.cursor_image.is_some());
        {
            let s = standin.shared.lock().unwrap();
            let (w, h, px) = s.cursor_image.clone().unwrap();
            assert!(w > 4 && h > 4 && w <= 384, "cursor size {w}x{h}");
            assert!(px.chunks_exact(4).any(|p| p[3] > 0), "the cursor image is fully transparent");
            // Evidence: the cursor over a mid-grey backdrop, as ARGB8888 premultiplied.
            let mut flat = Vec::with_capacity(px.len());
            for p in px.chunks_exact(4) {
                let inv = 255 - p[3] as u32;
                for c in &p[..3] {
                    flat.push((*c as u32 + 128 * inv / 255).min(255) as u8);
                }
                flat.push(255);
            }
            evidence("hosted-3b-cursor.png", &Frame { output: "cursor".into(), seq: 0, width: w, height: h, pixels: flat, after_ack: true, damage: vec![], at: std::time::Instant::now() });
        }

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

#[test]
fn a_held_key_repeats_inside_the_session() {
    if let Err(why) = available() {
        eprintln!("SKIP: {why}");
        return;
    }
    let env = HostedEnv::start("repeat");
    let standin = Standin::start(&env.dir, "10");
    standin.add_output("DP-1", OutputSpec::new(0, 0, 800, 600));
    let mut bridge = spawn_bridge(&env, &standin, "10");
    with_logs(&env, &mut bridge, || {
        standin.wait_for("output_configured", T, |s| s.reports.iter().any(|r| r.kind == OUTPUT_CONFIGURED));
        standin.wait_for("a real frame", T, |s| !s.frames.is_empty());
        std::thread::sleep(Duration::from_millis(500));
        let empty = latest(&standin, "DP-1");
        let typed = env.dir.join("typed.txt");
        let script = format!("IFS= read -r line; printf %s \"$line\" > {}; sleep 30", typed.display());
        let mut term = env.client("foot", &["sh", "-c", &script]);
        wait_changed(&standin, "DP-1", &empty, 5000);
        std::thread::sleep(Duration::from_millis(2000));
        standin.send(Control::KeyboardEnter("DP-1".into()));
        standin.send(Control::InputState(true));
        std::thread::sleep(Duration::from_millis(300));
        standin.send(Control::Key(35, true)); // h, held
        std::thread::sleep(Duration::from_millis(1500));
        standin.send(Control::Key(35, false));
        standin.send(Control::Key(28, true));
        standin.send(Control::Key(28, false));
        support::mutter::wait_until(Duration::from_secs(10), || typed.exists(), "the typed line");
        std::thread::sleep(Duration::from_millis(200));
        let line = std::fs::read_to_string(&typed).unwrap();
        eprintln!("held h for 1.5 s, the terminal received {} characters", line.len());
        assert!(line.chars().all(|c| c == 'h'), "{line:?}");
        assert!(line.len() >= 5, "no key repeat: {line:?}");
        let _ = term.kill();
        let _ = term.wait();
    });
}

/// Build Gnoblin's layer-surface test client (needs the Gnoblin sources:
/// `BSM_TEST_GNOBLIN_SRC`). Returns the path of the client binary.
fn build_layer_client(dir: &std::path::Path) -> Option<PathBuf> {
    let src = PathBuf::from(std::env::var_os("BSM_TEST_GNOBLIN_SRC")?);
    let build = dir.join("layer-client-build");
    std::fs::create_dir_all(&build).unwrap();
    let layer_xml = src.join("src/protocols/layer-shell/wlr-layer-shell-unstable-v1.xml");
    let pkgdata = Command::new("pkg-config").args(["--variable=pkgdatadir", "wayland-protocols"]).output().ok()?;
    let xdg_xml = PathBuf::from(String::from_utf8(pkgdata.stdout).ok()?.trim()).join("stable/xdg-shell/xdg-shell.xml");
    for (mode, xml, name) in [
        ("client-header", &layer_xml, "wlr-layer-shell-unstable-v1-client-protocol.h"),
        ("private-code", &layer_xml, "layer.c"),
        ("private-code", &xdg_xml, "xdg.c"),
    ] {
        assert!(Command::new("wayland-scanner").args([mode]).arg(xml).arg(build.join(name)).status().ok()?.success());
    }
    let cflags = Command::new("pkg-config").args(["--cflags", "--libs", "wayland-client"]).output().ok()?;
    let mut cc = Command::new("cc");
    cc.arg(format!("-I{}", build.display()))
        .arg(src.join("tests/hosted-layer-client.c"))
        .arg(build.join("layer.c"))
        .arg(build.join("xdg.c"))
        .args(String::from_utf8(cflags.stdout).ok()?.split_whitespace())
        .arg("-o")
        .arg(build.join("client"));
    cc.status().ok()?.success().then(|| build.join("client"))
}

#[test]
fn desktop_ready_waits_for_gnoblins_own_layer_list() {
    if let Err(why) = available() {
        eprintln!("SKIP: {why}");
        return;
    }
    if std::env::var_os("BSM_TEST_GNOBLIN").is_none() {
        eprintln!("SKIP: needs the real hosted Gnoblin (BSM_TEST_GNOBLIN) for its layer list");
        return;
    }
    let env = HostedEnv::start("layers");
    let Some(client) = build_layer_client(&env.dir) else {
        eprintln!("SKIP: BSM_TEST_GNOBLIN_SRC not set or the layer client did not build");
        return;
    };
    let standin = Standin::start(&env.dir, "11");
    standin.add_output("DP-1", OutputSpec::new(0, 0, 800, 600));
    let mut bridge = spawn_bridge(&env, &standin, "11");
    let ready_sock = env.runtime.join("bsm/ready.sock");
    let say = |line: String| {
        use std::io::Write;
        let mut s = std::os::unix::net::UnixStream::connect(&ready_sock).expect("ready.sock");
        writeln!(s, "{line}").unwrap();
        std::thread::sleep(Duration::from_millis(100));
    };
    with_logs(&env, &mut bridge, || {
        standin.wait_for("output_configured", T, |s| s.reports.iter().any(|r| r.kind == OUTPUT_CONFIGURED));
        // The connector Gnoblin gave the virtual monitor.
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
        let (_s, monitors, _l, _p): (
            u32,
            Vec<bsm_host::gnoblin::DcMonitor>,
            Vec<bsm_host::gnoblin::DcLogical>,
            std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
        ) = body.deserialize().unwrap();
        let connector = monitors[0].0.0.clone();
        eprintln!("DisplayConfig connector: {connector}");

        let g = "11";
        for c in ["wallpaper", "desktop"] {
            say(format!(r#"{{"generation":"{g}","component":"{c}","event":"launched"}}"#));
        }
        // Bingux claims every surface mapped and says it is ready, but Gnoblin has none.
        for (c, r) in [("wallpaper", "wallpaper"), ("desktop", "bar"), ("desktop", "dock")] {
            say(format!(r#"{{"generation":"{g}","component":"{c}","event":"surface","output":"{connector}","role":"{r}","mapped":true}}"#));
        }
        for c in ["wallpaper", "desktop"] {
            say(format!(r#"{{"generation":"{g}","component":"{c}","event":"ready"}}"#));
        }
        std::thread::sleep(Duration::from_millis(1500));
        let has = |kind: u32| standin.shared.lock().unwrap().reports.iter().any(|r| r.kind == kind);
        assert!(!has(5), "desktop_ready on Bingux's word alone");
        let mapped = |s: &Shared| s.reports.iter().rev().find(|r| r.kind == 4).map(|r| (r.a, r.b));
        assert_eq!(mapped(&standin.shared.lock().unwrap()), Some((0, 3)));

        // Gnoblin maps two of the three.
        let mut kids = Vec::new();
        let spawn = |ns: &str, layer: &str, anchor: &str, size: &str, colour: &str| {
            Command::new(&client)
                .args([ns, layer, anchor, size, colour, &connector])
                .env("XDG_RUNTIME_DIR", &env.runtime)
                .env("WAYLAND_DISPLAY", &env.display)
                .stdout(Stdio::null())
                .spawn()
                .expect("layer client")
        };
        kids.push(spawn("bingux-wallpaper", "background", "all", "0", "0xFF204060"));
        kids.push(spawn("bingux-top-bar", "top", "top", "32", "0xFFD04020"));
        standin.wait_for("2 of 3 mapped", T, |s| mapped(s) == Some((2, 3)));
        assert!(!has(5), "desktop_ready with the dock missing from Gnoblin's list");

        // The third completes it, and a frame follows the report.
        kids.push(spawn("bingux-dock", "top", "bottom", "48", "0xFF20C040"));
        standin.wait_for("3 of 3 mapped", T, |s| mapped(s) == Some((3, 3)));
        standin.wait_for("desktop_ready", T, |s| s.reports.iter().any(|r| r.kind == 5));
        standin.wait_for("a frame presented after desktop_ready", T, |s| {
            let ready_at = s.reports.iter().find(|r| r.kind == 5).unwrap().at;
            s.frames.iter().any(|f| f.at >= ready_at && f.after_ack)
        });
        // The frames really show Gnoblin's layer surfaces: wallpaper, bar, dock colours.
        let near = |a: [u8; 3], b: [u8; 3]| a.iter().zip(b).all(|(x, y)| (*x as i32 - y as i32).abs() <= 3);
        standin.wait_for("a frame showing the layer surfaces", T, |s| {
            s.frames.last().is_some_and(|f| {
                near(px(f, 400, 300), [0x20, 0x40, 0x60]) && near(px(f, 400, 10), [0xD0, 0x40, 0x20]) && near(px(f, 400, 590), [0x20, 0xC0, 0x40])
            })
        });
        let f = latest(&standin, "DP-1");
        evidence("hosted-5-layers.png", &f);
        for mut k in kids {
            let _ = k.kill();
            let _ = k.wait();
        }
    });
}

#[test]
fn session_actions_from_gnoblin_are_forwarded_to_bsmctl() {
    if let Err(why) = available() {
        eprintln!("SKIP: {why}");
        return;
    }
    if std::env::var_os("BSM_TEST_GNOBLIN").is_some() {
        eprintln!("SKIP: stub signal test (stock Mutter only)");
        return;
    }
    let env = HostedEnv::start("actions");
    let standin = Standin::start(&env.dir, "12");
    standin.add_output("DP-1", OutputSpec::new(0, 0, 800, 600));
    let record = env.dir.join("bsmctl-calls");
    let fake = env.dir.join("bsmctl");
    std::fs::write(&fake, format!("#!/bin/sh\necho \"$1\" >> {}\n", record.display())).unwrap();
    std::fs::set_permissions(&fake, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let mut bridge = Command::new(env!("CARGO_BIN_EXE_bsm-host"))
        .env("BSM_HOST_SOCKET", &standin.socket)
        .env("BSM_GENERATION", "12")
        .env("XDG_RUNTIME_DIR", &env.runtime)
        .env("DBUS_SESSION_BUS_ADDRESS", &env.bus)
        .env("GNOBLIN_COMPOSITOR_SOCKET", env.runtime.join("none.sock"))
        .env("BSMCTL", &fake)
        .stdout(Stdio::null())
        .stderr(std::fs::File::create(env.dir.join("bridge.log")).unwrap())
        .spawn()
        .unwrap();
    with_logs(&env, &mut bridge, || {
        standin.wait_for("output_configured", T, |s| s.reports.iter().any(|r| r.kind == OUTPUT_CONFIGURED));
        let conn = zbus::blocking::connection::Builder::address(env.bus.as_str()).unwrap().build().unwrap();
        for action in ["logout", "poweroff", "hibernate"] {
            conn.emit_signal(None::<&str>, "/org/gnoblin/Hosted1", "org.gnoblin.Hosted1", "ActionRequested", &(action,))
                .unwrap();
        }
        wait_until(Duration::from_secs(10), || std::fs::read_to_string(&record).is_ok_and(|s| s.lines().count() >= 2), "bsmctl calls");
        std::thread::sleep(Duration::from_millis(300));
        let calls = std::fs::read_to_string(&record).unwrap();
        // The calls run concurrently, so their order is not defined.
        let mut calls: Vec<&str> = calls.lines().collect();
        calls.sort();
        assert_eq!(calls, vec!["logout", "poweroff"], "hibernate has no bsm equivalent and must be ignored");
    });
}

fn cpu_ticks(pid: u32) -> u64 {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
    let rest = &stat[stat.rfind(')').unwrap() + 2..];
    let f: Vec<&str> = rest.split_whitespace().collect();
    f[11].parse::<u64>().unwrap() + f[12].parse::<u64>().unwrap()
}

fn rss_kib(pid: u32) -> u64 {
    std::fs::read_to_string(format!("/proc/{pid}/status"))
        .unwrap()
        .lines()
        .find_map(|l| l.strip_prefix("VmRSS:"))
        .and_then(|v| v.split_whitespace().next()?.parse().ok())
        .unwrap_or(0)
}

/// Cost of the shared-memory path. Ignored by default: run with
/// `cargo test --test hosted measure_costs -- --ignored --nocapture`
/// (optionally BSM_BENCH_SIZE=3840x2160).
#[test]
#[ignore]
fn measure_costs() {
    if let Err(why) = available() {
        eprintln!("SKIP: {why}");
        return;
    }
    if !std::process::Command::new("sh").args(["-c", "command -v weston-simple-shm"]).status().unwrap().success() {
        eprintln!("SKIP: weston-simple-shm not installed");
        return;
    }
    let (w, h) = std::env::var("BSM_BENCH_SIZE")
        .ok()
        .and_then(|s| s.split_once('x').map(|(a, b)| (a.parse().unwrap(), b.parse().unwrap())))
        .unwrap_or((1920u32, 1080u32));
    let env = HostedEnv::start("bench");
    let standin = Standin::start(&env.dir, "13");
    standin.add_output("DP-1", OutputSpec::new(0, 0, w, h));
    let mut bridge = spawn_bridge(&env, &standin, "13");
    let pid = bridge.id();
    with_logs(&env, &mut bridge, || {
        standin.wait_for("output_configured", T, |s| s.reports.iter().any(|r| r.kind == OUTPUT_CONFIGURED));
        standin.wait_for("a frame", T, |s| !s.frames.is_empty());
        let mut client = env.client("weston-simple-shm", &[]);
        std::thread::sleep(Duration::from_secs(4));
        let tick = unsafe { libc::sysconf(libc::_SC_CLK_TCK) } as f64;
        let window = 10.0;
        let mut report = String::new();
        for (label, active) in [("background", false), ("foreground", true)] {
            standin.send(Control::InputState(active));
            std::thread::sleep(Duration::from_secs(2));
            let (b0, m0) = (cpu_ticks(pid), cpu_ticks(env.compositor_pid()));
            let f0 = count(&standin, "DP-1");
            std::thread::sleep(Duration::from_secs_f64(window));
            let (b1, m1) = (cpu_ticks(pid), cpu_ticks(env.compositor_pid()));
            let f1 = count(&standin, "DP-1");
            let frames = f1 - f0;
            let bridge_cpu = (b1 - b0) as f64 / tick / window * 100.0;
            let mutter_cpu = (m1 - m0) as f64 / tick / window * 100.0;
            let per_frame_ms = if frames > 0 { (b1 - b0) as f64 / tick * 1000.0 / frames as f64 } else { 0.0 };
            report += &format!(
                "{label:>10}: {w}x{h}: {:.1} fps forwarded, bridge {bridge_cpu:.1} % of one core ({per_frame_ms:.2} ms CPU per forwarded frame), compositor {mutter_cpu:.1} %\n",
                frames as f64 / window
            );
        }
        {
            // Baseline: the same copy between two plain 8 MB memory blocks.
            let n = w as usize * h as usize * 4;
            let src = memmap2::MmapMut::map_anon(n).unwrap();
            let mut dst = memmap2::MmapMut::map_anon(n).unwrap();
            dst.copy_from_slice(&src);
            let t = std::time::Instant::now();
            for _ in 0..100 {
                dst.copy_from_slice(&src);
            }
            report += &format!("baseline memcpy of one {w}x{h} frame: {:.2} ms\n", t.elapsed().as_secs_f64() * 1000.0 / 100.0);
        }
        report += &format!("bridge RSS {} KiB\n", rss_kib(pid));
        let _ = client.kill();
        let _ = client.wait();
        eprintln!("--- measured costs ---\n{report}");
        // SIGTERM prints the copy statistics.
        unsafe { libc::kill(pid as i32, libc::SIGTERM) };
        std::thread::sleep(Duration::from_millis(1500));
        let log = env.log("bridge.log");
        for l in log.lines().filter(|l| l.contains("output stats") || l.contains("committed=")) {
            eprintln!("{l}");
        }
    });
}

#[test]
fn resizing_and_removing_outputs_rebuilds_the_virtual_monitors() {
    if let Err(why) = available() {
        eprintln!("SKIP: {why}");
        return;
    }
    let env = HostedEnv::start("rebuild");
    let standin = Standin::start(&env.dir, "14");
    standin.add_output("A", OutputSpec::new(0, 0, 800, 600));
    standin.add_output("B", OutputSpec::new(800, 0, 800, 600));
    let mut bridge = spawn_bridge(&env, &standin, "14");
    let pid = bridge.id();
    with_logs(&env, &mut bridge, || {
        standin.wait_for("both outputs", T, |s| s.reports.iter().filter(|r| r.kind == OUTPUT_CONFIGURED).count() == 2);
        standin.wait_for("frames on both", T, |s| s.frames.iter().any(|f| f.output == "A") && s.frames.iter().any(|f| f.output == "B"));

        // A grows: both monitors are rebuilt, frames arrive at the new size, nothing before the ack.
        standin.send(Control::Reconfigure("A".into(), OutputSpec::new(0, 0, 1024, 768)));
        standin.wait_for("frames at the new size", T, |s| s.frames.iter().any(|f| f.output == "A" && (f.width, f.height) == (1024, 768)));
        standin.wait_for("B frames after the rebuild", T, |s| {
            let a = s.frames.iter().rposition(|f| f.output == "A" && f.width == 1024);
            let b = s.frames.iter().rposition(|f| f.output == "B");
            matches!((a, b), (Some(a), Some(b)) if b >= a) || s.reports.iter().filter(|r| r.kind == OUTPUT_CONFIGURED).count() >= 4
        });
        assert!(standin.shared.lock().unwrap().violations.is_empty(), "{:?}", standin.shared.lock().unwrap().violations);

        // B goes away: A keeps presenting.
        standin.send(Control::RemoveOutput("B".into()));
        std::thread::sleep(Duration::from_millis(1500));
        let n = count(&standin, "A");
        let mut client = env.client("weston-clickdot", &[]);
        standin.wait_for("A changes after B was removed", T, |s| s.frames.iter().filter(|f| f.output == "A").count() > n);
        let _ = client.kill();
        let _ = client.wait();
        assert!(standin.shared.lock().unwrap().violations.is_empty());
        assert!(std::path::Path::new(&format!("/proc/{pid}")).exists(), "bridge exited");
    });
}

#[test]
fn a_crashed_compositor_ends_the_bridge_with_status_4() {
    if let Err(why) = available() {
        eprintln!("SKIP: {why}");
        return;
    }
    let mut env = HostedEnv::start("crash");
    let standin = Standin::start(&env.dir, "15");
    standin.add_output("A", OutputSpec::new(0, 0, 800, 600));
    let mut bridge = spawn_bridge(&env, &standin, "15");
    standin.wait_for("output_configured", T, |s| s.reports.iter().any(|r| r.kind == OUTPUT_CONFIGURED));
    standin.wait_for("a frame", T, |s| !s.frames.is_empty());
    env.kill_compositor();
    let start = std::time::Instant::now();
    let code = loop {
        if let Some(s) = bridge.try_wait().unwrap() {
            break s.code();
        }
        if start.elapsed() > Duration::from_secs(15) {
            let _ = bridge.kill();
            panic!("the bridge did not notice the compositor dying\n{}", env.log("bridge.log"));
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(code, Some(4), "{}", env.log("bridge.log"));
}
