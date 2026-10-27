//! Session start, PAM session lifetime, termination, adoption and the
//! greeter, with real processes and real libpam (test stack). Root-only
//! behaviour (setuid, pam_systemd, scopes) is in `tests/root-checks.sh`.

mod common;

use std::time::{Duration, Instant};

use bsm_proto::error::ErrorCode;
use bsm_proto::helper::{HelperEvent, HelperRequest};
use bsm_proto::state::AuthIntent;
use common::*;

struct Opened {
    host_pid: u32,
    leader_pid: u32,
    scope: String,
}

fn open(env: &Env, h: &mut HelperProc, c: &Client, session: &str, generation: u64) -> Opened {
    let t = c.login(env, 1000 + generation, generation, AuthIntent::Login);
    c.open_session(env, 2000 + generation, t, session, generation);
    match c.until(|e| matches!(e, HelperEvent::SessionOpened { .. } | HelperEvent::SessionFailed { .. })) {
        HelperEvent::SessionOpened { host_pid, leader_pid, scope, session: s, generation: g, .. } => {
            assert_eq!((s.as_str(), g), (session, generation));
            h.guard_session(leader_pid);
            Opened { host_pid, leader_pid, scope }
        }
        other => panic!("session did not open: {other:?}"),
    }
}

fn wait_for_file(path: &std::path::Path) -> String {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(s) = std::fs::read_to_string(path)
            && !s.is_empty()
        {
            return s;
        }
        assert!(Instant::now() < deadline, "{} never appeared", path.display());
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn env_of(text: &str) -> std::collections::BTreeMap<String, String> {
    text.lines().filter_map(|l| l.split_once('=')).map(|(k, v)| (k.to_owned(), v.to_owned())).collect()
}

#[test]
fn the_pam_session_stays_open_for_the_desktop_lifetime_and_closes_on_close_session() {
    if skip_as_root() {
        return;
    }
    let env = Env::new();
    let mut h = env.start();
    let c = env.connect();
    let o = open(&env, &mut h, &c, "s0000a001", 11);
    assert!(alive(o.leader_pid) && alive(o.host_pid));
    assert_ne!(o.leader_pid, o.host_pid);
    assert!(!o.scope.is_empty());

    // Authentication succeeded long ago; the PAM session must still be open.
    std::thread::sleep(Duration::from_millis(500));
    let log = env.pam.session_log_lines();
    assert_eq!(log.len(), 1, "exactly one open_session so far: {log:?}");
    assert!(log[0].starts_with("open_session "), "{log:?}");

    c.send(&HelperRequest::CloseSession { session: "s0000a001".into(), generation: 11, grace_ms: 500 });
    match c.until(|e| matches!(e, HelperEvent::SessionExited { .. })) {
        HelperEvent::SessionExited { session, generation, .. } => assert_eq!((session.as_str(), generation), ("s0000a001", 11)),
        _ => unreachable!(),
    }
    assert!(wait_dead(o.leader_pid, Duration::from_secs(5)));
    assert!(wait_dead(o.host_pid, Duration::from_secs(5)));
    let log = env.pam.session_log_lines();
    assert_eq!(log.len(), 2, "{log:?}");
    assert!(log[1].starts_with("close_session "), "{log:?}");
}

#[test]
fn pam_sees_a_seatless_wayland_user_session_and_the_processes_get_the_documented_environment() {
    if skip_as_root() {
        return;
    }
    let env = Env::new();
    let mut h = env.start();
    let c = env.connect();
    let o = open(&env, &mut h, &c, "s0000a002", 12);
    let line = &env.pam.session_log_lines()[0];
    assert!(line.contains("type=wayland") && line.contains("class=user") && line.contains("desktop=gnoblin"), "{line}");
    assert!(line.contains("seat=unset") && line.contains("vtnr=unset"), "{line}");
    assert!(line.contains("session=s0000a002") && line.contains("gen=12"), "{line}");

    let leader = env_of(&wait_for_file(&env.path("leader.env")));
    let bridge = env_of(&wait_for_file(&env.path("bridge.env")));
    for (name, e) in [("leader", &leader), ("bridge", &bridge)] {
        assert_eq!(e["USER"], env.pam.user, "{name}");
        assert_eq!(e["LOGNAME"], env.pam.user, "{name}");
        assert_eq!(e["XDG_SESSION_TYPE"], "wayland", "{name}");
        assert_eq!(e["XDG_SESSION_CLASS"], "user", "{name}");
        assert_eq!(e["XDG_SESSION_DESKTOP"], "gnoblin", "{name}");
        assert_eq!(e["DESKTOP_SESSION"], "gnoblin", "{name}");
        assert_eq!(e["BSM_SESSION_ID"], "s0000a002", "{name}");
        assert_eq!(e["BSM_GENERATION"], "12", "{name}");
        assert_eq!(e["XDG_RUNTIME_DIR"], format!("/run/user/{}", env.pam.uid), "{name}");
        assert_eq!(e["DBUS_SESSION_BUS_ADDRESS"], format!("unix:path=/run/user/{}/bus", env.pam.uid), "{name}");
        assert!(e.contains_key("HOME") && e.contains_key("SHELL") && e.contains_key("PATH"), "{name}");
        for forbidden in ["XDG_SEAT", "XDG_VTNR", "WAYLAND_DISPLAY", "DISPLAY", "LD_PRELOAD"] {
            assert!(!e.contains_key(forbidden), "{name} has {forbidden}");
        }
    }
    // The private socket reaches the bridge only.
    let expected = env.session_socket_dir().join("s0000a002-12.sock").display().to_string();
    assert_eq!(bridge["BSM_HOST_SOCKET"], expected);
    assert!(!leader.contains_key("BSM_HOST_SOCKET"));
    assert_eq!(bridge["BSM_OUTPUT_HINTS"], "1920x1080");
    c.send(&HelperRequest::CloseSession { session: "s0000a002".into(), generation: 12, grace_ms: 300 });
    c.until(|e| matches!(e, HelperEvent::SessionExited { .. }));
    let _ = o;
}

#[test]
fn the_session_ends_when_the_leader_exits_and_reports_its_status() {
    if skip_as_root() {
        return;
    }
    let env = Env::new();
    let cfg = env.config().replace(
        "gnoblin = [\"/bin/sh\", \"-c\", \"env > ",
        "gnoblin = [\"/bin/sh\", \"-c\", \"sleep 1; exit 3; env > ",
    );
    let mut h = env.start_with(&cfg);
    let c = env.connect();
    let o = open(&env, &mut h, &c, "s0000a003", 13);
    match c.until(|e| matches!(e, HelperEvent::SessionExited { .. })) {
        HelperEvent::SessionExited { status, .. } => assert_eq!(status, 3),
        _ => unreachable!(),
    }
    // The bridge was stopped with it and the PAM session closed after.
    assert!(wait_dead(o.host_pid, Duration::from_secs(5)));
    let log = env.pam.session_log_lines();
    assert!(log.last().unwrap().starts_with("close_session "), "{log:?}");
}

#[test]
fn the_session_ends_when_the_bridge_exits() {
    if skip_as_root() {
        return;
    }
    let env = Env::new();
    let cfg = env.config().replace(
        "bridge = [\"/bin/sh\", \"-c\", \"env > ",
        "bridge = [\"/bin/sh\", \"-c\", \"sleep 1; exit 4; env > ",
    );
    let mut h = env.start_with(&cfg);
    let c = env.connect();
    let o = open(&env, &mut h, &c, "s0000a004", 14);
    match c.until(|e| matches!(e, HelperEvent::SessionExited { .. })) {
        HelperEvent::SessionExited { status, .. } => assert_eq!(status, 4),
        _ => unreachable!(),
    }
    assert!(wait_dead(o.leader_pid, Duration::from_secs(5)), "leader outlived the bridge");
}

#[test]
fn a_missing_binary_fails_the_login_and_still_closes_the_pam_session() {
    if skip_as_root() {
        return;
    }
    let env = Env::new();
    let cfg = env.config().replace("gnoblin = [\"/bin/sh\"", "gnoblin = [\"/nonexistent/gnoblin\"");
    let _h = env.start_with(&cfg);
    let c = env.connect();
    let t = c.login(&env, 1, 15, AuthIntent::Login);
    c.open_session(&env, 2, t, "s0000a005", 15);
    match c.until(|e| matches!(e, HelperEvent::SessionOpened { .. } | HelperEvent::SessionFailed { .. })) {
        HelperEvent::SessionFailed { code: ErrorCode::HelperFailure, message, .. } => {
            assert!(message.contains("/nonexistent/gnoblin"), "{message}");
        }
        other => panic!("{other:?}"),
    }
    std::thread::sleep(Duration::from_millis(300));
    let log = env.pam.session_log_lines();
    assert_eq!(log.len(), 2, "opened and closed: {log:?}");
}

#[test]
fn session_scope_is_required_by_default_and_the_login_fails_without_one() {
    if skip_as_root() {
        return;
    }
    let env = Env::new();
    let cfg = env.config().replace("require_scope = false", "require_scope = true");
    let _h = env.start_with(&cfg);
    let c = env.connect();
    let t = c.login(&env, 1, 16, AuthIntent::Login);
    c.open_session(&env, 2, t, "s0000a006", 16);
    match c.until(|e| matches!(e, HelperEvent::SessionOpened { .. } | HelperEvent::SessionFailed { .. })) {
        HelperEvent::SessionFailed { code: ErrorCode::LogindFailure, message, .. } => {
            assert!(message.contains("scope"), "{message}");
        }
        other => panic!("{other:?}"),
    }
    std::thread::sleep(Duration::from_millis(300));
    let log = env.pam.session_log_lines();
    assert_eq!(log.len(), 2, "a failed start must close the PAM session: {log:?}");
}

#[test]
fn invalid_open_session_arguments_are_refused() {
    if skip_as_root() {
        return;
    }
    let env = Env::new();
    let _h = env.start();
    let c = env.connect();
    let t = c.login(&env, 1, 17, AuthIntent::Login);
    c.send(&HelperRequest::OpenSession {
        req_id: 2,
        ticket: t,
        session: "../evil".into(),
        generation: 17,
        socket_path: env.session_socket_dir().join("x.sock").display().to_string(),
        output_hints: vec![],
    });
    assert!(matches!(c.recv(), HelperEvent::SessionFailed { code: ErrorCode::InvalidArgument, .. }));
    let t = c.login(&env, 3, 17, AuthIntent::Login);
    c.send(&HelperRequest::OpenSession {
        req_id: 4,
        ticket: t,
        session: "s0000a007".into(),
        generation: 17,
        socket_path: "/tmp/elsewhere.sock".into(),
        output_hints: vec![],
    });
    assert!(matches!(c.recv(), HelperEvent::SessionFailed { code: ErrorCode::InvalidArgument, .. }));
    let t = c.login(&env, 5, 17, AuthIntent::Login);
    c.send(&HelperRequest::OpenSession {
        req_id: 6,
        ticket: t,
        session: "s0000a007".into(),
        generation: 17,
        socket_path: format!("{}/../x.sock", env.session_socket_dir().display()),
        output_hints: vec![],
    });
    assert!(matches!(c.recv(), HelperEvent::SessionFailed { code: ErrorCode::InvalidArgument, .. }));
    // None of that started a PAM session.
    assert!(env.pam.session_log_lines().is_empty());
}

#[test]
fn close_session_of_one_session_leaves_the_other_alone() {
    if skip_as_root() {
        return;
    }
    let env = Env::new();
    let mut h = env.start();
    let c = env.connect();
    let a = open(&env, &mut h, &c, "s0000a008", 21);
    let b = open(&env, &mut h, &c, "s0000a009", 22);
    // A stale generation changes nothing.
    c.send(&HelperRequest::CloseSession { session: "s0000a008".into(), generation: 99, grace_ms: 100 });
    c.assert_silent(Duration::from_millis(500));
    assert!(alive(a.leader_pid) && alive(b.leader_pid));

    c.send(&HelperRequest::CloseSession { session: "s0000a008".into(), generation: 21, grace_ms: 300 });
    match c.until(|e| matches!(e, HelperEvent::SessionExited { .. })) {
        HelperEvent::SessionExited { session, .. } => assert_eq!(session, "s0000a008"),
        _ => unreachable!(),
    }
    assert!(wait_dead(a.leader_pid, Duration::from_secs(5)));
    assert!(alive(b.leader_pid) && alive(b.host_pid), "the other session must be untouched");
    c.send(&HelperRequest::CloseSession { session: "s0000a009".into(), generation: 22, grace_ms: 300 });
    c.until(|e| matches!(e, HelperEvent::SessionExited { .. }));
}

#[test]
fn a_leader_that_ignores_sigterm_is_killed_after_the_grace_period() {
    if skip_as_root() {
        return;
    }
    let env = Env::new();
    let cfg = env.config().replace(
        "gnoblin = [\"/bin/sh\", \"-c\", \"env > ",
        &format!("gnoblin = [\"/bin/sh\", \"-c\", \"trap '' TERM; echo ready > {}/trap.ready; while :; do sleep 1; done; env > ", env.path("").display()),
    );
    let mut h = env.start_with(&cfg);
    let c = env.connect();
    let o = open(&env, &mut h, &c, "s0000a00a", 23);
    // SIGTERM must find the trap in place, or the leader simply dies.
    wait_for_file(&env.path("trap.ready"));
    let started = Instant::now();
    c.send(&HelperRequest::CloseSession { session: "s0000a00a".into(), generation: 23, grace_ms: 400 });
    c.until(|e| matches!(e, HelperEvent::SessionExited { .. }));
    assert!(started.elapsed() >= Duration::from_millis(350), "SIGKILL must wait for the grace period");
    assert!(wait_dead(o.leader_pid, Duration::from_secs(5)));
}

#[test]
fn a_restarted_helper_adopts_running_sessions_and_can_close_them() {
    if skip_as_root() {
        return;
    }
    let env = Env::new();
    let mut h = env.start();
    let c = env.connect();
    let o = open(&env, &mut h, &c, "s0000a00b", 31);
    h.kill9();
    drop(c);
    // The desktop and the PAM session survive the helper.
    std::thread::sleep(Duration::from_millis(300));
    assert!(alive(o.leader_pid) && alive(o.host_pid), "the session must survive a helper crash");
    assert_eq!(env.pam.session_log_lines().len(), 1);

    let mut h2 = env.start();
    let c2 = env.connect();
    c2.send(&HelperRequest::CloseSession { session: "s0000a00b".into(), generation: 31, grace_ms: 300 });
    match c2.until(|e| matches!(e, HelperEvent::SessionExited { .. })) {
        HelperEvent::SessionExited { session, generation, .. } => assert_eq!((session.as_str(), generation), ("s0000a00b", 31)),
        _ => unreachable!(),
    }
    assert!(wait_dead(o.leader_pid, Duration::from_secs(5)));
    let log = env.pam.session_log_lines();
    assert_eq!(log.len(), 2, "{log:?}");
    h2.guard_session(o.leader_pid);
}

#[test]
fn an_adopted_session_reports_its_own_exit() {
    if skip_as_root() {
        return;
    }
    let env = Env::new();
    let mut h = env.start();
    let c = env.connect();
    let o = open(&env, &mut h, &c, "s0000a00c", 32);
    h.kill9();
    drop(c);
    let _h2 = env.start();
    let c2 = env.connect();
    // Kill the leader behind the new helper's back.
    unsafe { libc::kill(o.leader_pid as i32, libc::SIGKILL) };
    match c2.until(|e| matches!(e, HelperEvent::SessionExited { .. })) {
        HelperEvent::SessionExited { session, status, .. } => {
            assert_eq!(session, "s0000a00c");
            assert_eq!(status, -libc::SIGKILL);
        }
        _ => unreachable!(),
    }
}

#[test]
fn greeter_runs_as_the_greeter_account_in_private_directories_with_a_clean_environment() {
    if skip_as_root() {
        return;
    }
    let env = Env::new();
    let _h = env.start();
    let c = env.connect();
    let run = env.path("run");
    std::fs::create_dir_all(&run).unwrap();
    let wl = run.join("greeter-wayland.sock").display().to_string();
    let ctl = run.join("greeter-control.sock").display().to_string();
    c.send(&HelperRequest::SpawnGreeter { req_id: 1, generation: 41, wayland_socket: wl.clone(), control_socket: ctl.clone() });
    let pid = match c.recv() {
        HelperEvent::GreeterSpawned { req_id: 1, generation: 41, pid } => pid,
        other => panic!("{other:?}"),
    };
    assert!(alive(pid));
    let dir = env.path("greeter").join("41");
    let text = wait_for_file(&dir.join("home/env.txt"));
    let e = env_of(&text);
    assert_eq!(e["WAYLAND_DISPLAY"], wl);
    assert_eq!(e["BSM_CONTROL_SOCKET"], ctl);
    assert_eq!(e["BSM_GENERATION"], "41");
    assert_eq!(e["HOME"], dir.join("home").display().to_string());
    assert_eq!(e["XDG_RUNTIME_DIR"], dir.join("rt").display().to_string());
    assert_eq!(e["QT_QPA_PLATFORM"], "wayland");
    for forbidden in ["BSM_HOST_SOCKET", "DBUS_SESSION_BUS_ADDRESS", "DISPLAY", "SSH_AUTH_SOCK", "LD_PRELOAD"] {
        assert!(!e.contains_key(forbidden), "greeter env has {forbidden}");
    }
    use std::os::unix::fs::PermissionsExt;
    for d in [dir.clone(), dir.join("rt"), dir.join("home")] {
        assert_eq!(std::fs::metadata(&d).unwrap().permissions().mode() & 0o777, 0o700, "{}", d.display());
    }

    c.send(&HelperRequest::KillGreeter { generation: 41 });
    match c.until(|e| matches!(e, HelperEvent::GreeterExited { .. })) {
        HelperEvent::GreeterExited { generation, status } => {
            assert_eq!(generation, 41);
            assert_eq!(status, -libc::SIGTERM);
        }
        _ => unreachable!(),
    }
    assert!(!dir.exists(), "the greeter's private directories must be removed");
}

#[test]
fn greeter_sockets_outside_the_socket_root_are_refused() {
    if skip_as_root() {
        return;
    }
    let env = Env::new();
    let _h = env.start();
    let c = env.connect();
    c.send(&HelperRequest::SpawnGreeter {
        req_id: 1,
        generation: 1,
        wayland_socket: "/tmp/evil.sock".into(),
        control_socket: env.path("run").join("c.sock").display().to_string(),
    });
    assert!(matches!(c.recv(), HelperEvent::GreeterExited { generation: 1, status: 127 }));
}

#[test]
fn a_new_greeter_replaces_the_old_one_and_the_greeter_dies_with_the_connection() {
    if skip_as_root() {
        return;
    }
    let env = Env::new();
    let _h = env.start();
    let c = env.connect();
    let run = env.path("run");
    let spawn = |req_id: u64, generation: u64| HelperRequest::SpawnGreeter {
        req_id,
        generation,
        wayland_socket: run.join("w.sock").display().to_string(),
        control_socket: run.join("c.sock").display().to_string(),
    };
    c.send(&spawn(1, 51));
    let first = match c.recv() {
        HelperEvent::GreeterSpawned { pid, .. } => pid,
        other => panic!("{other:?}"),
    };
    c.send(&spawn(2, 52));
    assert!(matches!(c.recv(), HelperEvent::GreeterExited { generation: 51, .. }));
    let second = match c.recv() {
        HelperEvent::GreeterSpawned { generation: 52, pid, .. } => pid,
        other => panic!("{other:?}"),
    };
    assert!(wait_dead(first, Duration::from_secs(5)));
    assert!(alive(second));
    drop(c);
    assert!(wait_dead(second, Duration::from_secs(8)), "a greeter must not outlive bsm's connection");
}

#[test]
fn sigterm_stops_the_helper_cleanly_and_leaves_sessions_running() {
    if skip_as_root() {
        return;
    }
    let env = Env::new();
    let mut h = env.start();
    let c = env.connect();
    let o = open(&env, &mut h, &c, "s0000a00d", 61);
    let status = h.terminate().expect("exit status");
    assert!(status.success(), "{status:?}");
    assert!(!env.socket().exists(), "the socket file must be removed");
    assert!(alive(o.leader_pid), "sessions outlive the helper");
    let _h2 = env.start();
    let c2 = env.connect();
    c2.send(&HelperRequest::CloseSession { session: "s0000a00d".into(), generation: 61, grace_ms: 300 });
    c2.until(|e| matches!(e, HelperEvent::SessionExited { .. }));
}

#[test]
fn a_session_that_ends_while_bsm_is_disconnected_is_reported_to_the_next_connection() {
    if skip_as_root() {
        return;
    }
    let env = Env::new();
    let mut h = env.start();
    let c = env.connect();
    let o = open(&env, &mut h, &c, "s0000a00e", 71);
    drop(c);
    std::thread::sleep(Duration::from_millis(300));
    unsafe { libc::kill(o.leader_pid as i32, libc::SIGKILL) };
    assert!(wait_dead(o.leader_pid, Duration::from_secs(5)));
    std::thread::sleep(Duration::from_millis(1500));
    // The queued exit arrives before anything else, so no Ping handshake here. A
    // lingering descriptor of the old peer can delay the helper's EOF: retry.
    let deadline = Instant::now() + Duration::from_secs(10);
    let c2 = loop {
        let c = env.connect_raw();
        if let Some(ev) = c.try_recv(Duration::from_millis(1500)) {
            break (c, ev);
        }
        assert!(Instant::now() < deadline, "no event on any connection");
    };
    match Some(c2.1) {
        Some(HelperEvent::SessionExited { session, generation, status }) => {
            assert_eq!((session.as_str(), generation, status), ("s0000a00e", 71, -libc::SIGKILL));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_loose_adoption_directory_means_nothing_is_adopted() {
    // Review F4: the directory is only trusted when root-only; a wrong umask must
    // not let another user's socket be adopted as a session.
    if skip_as_root() {
        return;
    }
    use std::os::unix::fs::PermissionsExt;
    let env = Env::new();
    let mut h = env.start();
    let c = env.connect();
    let o = open(&env, &mut h, &c, "s0000a00f", 81);
    let dir = env.state_dir().join("sessions");
    assert_eq!(std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700, "created private");
    h.kill9();
    drop(c);
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    let h2 = env.start();
    let c2 = env.connect();
    c2.send(&HelperRequest::CloseSession { session: "s0000a00f".into(), generation: 81, grace_ms: 200 });
    c2.assert_silent(Duration::from_millis(800));
    assert!(alive(o.leader_pid), "an untrusted directory must not be adopted");
    // Restoring the mode makes it adoptable again.
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    drop(c2);
    h2.kill9_boxed();
    let mut h3 = env.start();
    let c3 = env.connect();
    c3.send(&HelperRequest::CloseSession { session: "s0000a00f".into(), generation: 81, grace_ms: 200 });
    c3.until(|e| matches!(e, HelperEvent::SessionExited { .. }));
    h3.guard_session(o.leader_pid);
}

#[test]
fn user_processes_get_dev_null_and_no_inherited_descriptors() {
    // Review F5.
    if skip_as_root() {
        return;
    }
    let env = Env::new();
    let cfg = env.config().replace(
        "gnoblin = [\"/bin/sh\", \"-c\", \"env > ",
        &format!(
            "gnoblin = [\"/bin/sh\", \"-c\", \"for f in /proc/$$/fd/*; do echo $(basename $f) $(readlink $f); done > {}/fds.txt; env > ",
            env.path("").display()
        ),
    );
    let mut h = env.start_with(&cfg);
    let c = env.connect();
    let _o = open(&env, &mut h, &c, "s0000a010", 91);
    let text = wait_for_file(&env.path("fds.txt"));
    for line in text.lines() {
        let (fd, target) = line.split_once(' ').unwrap_or((line, ""));
        let fd: i32 = fd.parse().unwrap();
        if fd <= 2 {
            assert_eq!(target, "/dev/null", "fd {fd}");
        } else {
            assert!(!target.contains("socket:") && !target.contains("anon_inode") && !target.contains("pipe:"), "inherited fd {fd} -> {target}");
        }
    }
    c.send(&HelperRequest::CloseSession { session: "s0000a010".into(), generation: 91, grace_ms: 300 });
    c.until(|e| matches!(e, HelperEvent::SessionExited { .. }));
}
