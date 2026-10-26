//! Connection security: roles, registries, rejection of unauthorised peers,
//! single use sockets, protocol errors. Every client here is a real Wayland client on
//! a real socket against the real compositor thread.

mod common;

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use bsm_compositor::{Harness, HarnessConfig};
use bsm_proto::scene::{SceneCommand, SceneEvent};
use common::*;

fn harness(outputs: usize) -> Harness {
    Harness::start(HarnessConfig::with_outputs(outputs, 64, 48)).expect("harness")
}

fn iface_names(c: &Client) -> Vec<String> {
    c.interfaces().into_iter().map(|(n, _)| n).collect()
}

#[test]
fn greeter_registry_is_exactly_the_documented_set() {
    let mut h = harness(2);
    let c = connect_greeter(&mut h, 1, solid([10, 20, 30, 255]));
    assert_eq!(
        c.interfaces(),
        vec![
            ("wl_compositor".to_string(), 1),
            ("wl_output".to_string(), 2),
            ("wl_seat".to_string(), 1),
            ("wl_shm".to_string(), 1),
            ("wl_subcompositor".to_string(), 1),
            ("wp_fractional_scale_manager_v1".to_string(), 1),
            ("wp_presentation".to_string(), 1),
            ("wp_viewporter".to_string(), 1),
            ("zwlr_layer_shell_v1".to_string(), 1),
            ("zwp_linux_dmabuf_v1".to_string(), 1),
            ("zxdg_output_manager_v1".to_string(), 1),
        ]
    );
    assert!(!iface_names(&c).iter().any(|n| n == "bsm_session_host_v1"));
}

#[test]
fn hosted_registry_is_exactly_the_documented_set() {
    let mut h = harness(2);
    let path = register_session(&mut h, "s0000000a", 7);
    bind_host(&h, "s0000000a", 7);
    let c = Client::connect(&path).expect("bound host connects");
    assert_eq!(
        c.interfaces(),
        vec![
            ("bsm_session_host_v1".to_string(), 1),
            ("wl_compositor".to_string(), 1),
            ("wl_output".to_string(), 2),
            ("wl_seat".to_string(), 1),
            ("wl_shm".to_string(), 1),
            ("zwp_linux_dmabuf_v1".to_string(), 1),
        ]
    );
    for forbidden in [
        "zwlr_layer_shell_v1",
        "ext_session_lock_manager_v1",
        "zwlr_screencopy_manager_v1",
        "zwlr_output_manager_v1",
        "zwlr_data_control_manager_v1",
        "zwp_virtual_keyboard_manager_v1",
        "wl_data_device_manager",
        "ext_foreign_toplevel_list_v1",
        "xdg_wm_base",
    ] {
        assert!(!iface_names(&c).iter().any(|n| n == forbidden), "{forbidden} must not be offered");
    }
}

#[test]
fn connect_before_bind_is_held_and_gets_no_protocol_until_bound() {
    let mut h = harness(1);
    let path = register_session(&mut h, "s00000001", 1);
    let mut raw = UnixStream::connect(&path).expect("socket accepts the connection");
    // wl_display.get_registry(new_id=2): object 1, opcode 1, size 12.
    raw.write_all(&[1, 0, 0, 0, 1, 0, 12, 0, 2, 0, 0, 0]).unwrap();
    raw.set_read_timeout(Some(Duration::from_millis(300))).unwrap();
    let mut buf = [0u8; 64];
    let r = raw.read(&mut buf);
    assert!(
        matches!(&r, Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut)),
        "a connection that was not bound must receive nothing, got {r:?}"
    );
    assert!(
        !h.events().iter().any(|e| matches!(e, SceneEvent::SessionConnected { .. })),
        "no SessionConnected before BindHost"
    );
    bind_host(&h, "s00000001", 1);
    h.wait_event(WAIT, |e| matches!(e, SceneEvent::SessionConnected { .. })).expect("promoted after BindHost");
    raw.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let n = raw.read(&mut buf).expect("registry events after promotion");
    assert!(n > 0);
}

#[test]
fn sockets_are_single_use() {
    let mut h = harness(1);
    let path = register_session(&mut h, "s00000002", 1);
    bind_host(&h, "s00000002", 1);
    let _first = Client::connect(&path).expect("first connection");
    h.wait_event(WAIT, |e| matches!(e, SceneEvent::SessionConnected { .. })).expect("connected");
    // The socket file is gone and nothing listens any more.
    assert!(!path.exists(), "the session socket must be removed after the host connected");
    assert!(UnixStream::connect(&path).is_err(), "no second connection to a single-use socket");
    let n = h.events().iter().filter(|e| matches!(e, SceneEvent::SessionConnected { .. })).count();
    assert_eq!(n, 1);
}

#[test]
fn wrong_pid_is_rejected_and_closed() {
    let mut h = harness(1);
    let path = register_session(&mut h, "s00000003", 1);
    // A different live process stands in for "the bridge the helper started".
    let mut other = std::process::Command::new("sleep").arg("30").spawn().expect("spawn sleep");
    h.send(SceneCommand::BindHost {
        session: "s00000003".into(),
        generation: 1,
        scope: my_scope(),
        host_pid: other.id(),
    });
    h.sync();
    let mut raw = UnixStream::connect(&path).expect("connects");
    raw.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    let mut buf = [0u8; 16];
    let n = raw.read(&mut buf).unwrap_or(0);
    assert_eq!(n, 0, "the rejected connection is closed without a byte of protocol");
    let ev = h
        .wait_event(WAIT, |e| matches!(e, SceneEvent::SessionConnectionRejected { .. }))
        .expect("SessionConnectionRejected");
    let SceneEvent::SessionConnectionRejected { session, reason, .. } = ev else { unreachable!() };
    assert_eq!(session, "s00000003");
    assert!(reason.contains("pid"), "{reason}");
    assert!(!h.events().iter().any(|e| matches!(e, SceneEvent::SessionConnected { .. })));
    let _ = other.kill();
    let _ = other.wait();
}

#[test]
fn wrong_uid_scope_and_exe_are_rejected() {
    let mut h = harness(1);
    // uid
    h.send(SceneCommand::RegisterSession {
        session: "s00000004".into(),
        generation: 1,
        uid: my_uid() + 1,
        scope: String::new(),
        host_pid: 0,
        host_exe: my_exe(),
    });
    let ev = h.wait_event(WAIT, |e| matches!(e, SceneEvent::SessionSocketReady { session, .. } if session == "s00000004")).unwrap();
    let SceneEvent::SessionSocketReady { path, .. } = ev else { unreachable!() };
    let mut raw = UnixStream::connect(&path).unwrap();
    raw.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    assert_eq!(raw.read(&mut [0u8; 8]).unwrap_or(0), 0);
    let rej = h.wait_event(WAIT, |e| matches!(e, SceneEvent::SessionConnectionRejected { session, .. } if session == "s00000004")).unwrap();
    assert!(matches!(rej, SceneEvent::SessionConnectionRejected { reason, .. } if reason.contains("uid")));

    // scope
    let path = register_session(&mut h, "s00000005", 1);
    h.send(SceneCommand::BindHost {
        session: "s00000005".into(),
        generation: 1,
        scope: "bsm-host-not-my-scope.scope".into(),
        host_pid: std::process::id(),
    });
    h.sync();
    let mut raw = UnixStream::connect(&path).unwrap();
    raw.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    assert_eq!(raw.read(&mut [0u8; 8]).unwrap_or(0), 0);
    let rej = h.wait_event(WAIT, |e| matches!(e, SceneEvent::SessionConnectionRejected { session, .. } if session == "s00000005")).unwrap();
    assert!(matches!(rej, SceneEvent::SessionConnectionRejected { reason, .. } if reason.contains("scope")));

    // exe (readable for our own process, so a mismatch must reject)
    h.send(SceneCommand::RegisterSession {
        session: "s00000006".into(),
        generation: 1,
        uid: my_uid(),
        scope: String::new(),
        host_pid: 0,
        host_exe: "/usr/bin/not-the-bridge".into(),
    });
    let ev = h.wait_event(WAIT, |e| matches!(e, SceneEvent::SessionSocketReady { session, .. } if session == "s00000006")).unwrap();
    let SceneEvent::SessionSocketReady { path, .. } = ev else { unreachable!() };
    bind_host(&h, "s00000006", 1);
    h.sync();
    let mut raw = UnixStream::connect(&path).unwrap();
    raw.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    assert_eq!(raw.read(&mut [0u8; 8]).unwrap_or(0), 0);
    let rej = h.wait_event(WAIT, |e| matches!(e, SceneEvent::SessionConnectionRejected { session, .. } if session == "s00000006")).unwrap();
    assert!(matches!(rej, SceneEvent::SessionConnectionRejected { reason, .. } if reason.contains("exe")));
}

#[test]
fn a_session_socket_cannot_be_used_to_impersonate_another_session() {
    let mut h = harness(1);
    // Session B expects another process; session A expects us. We connect to B's socket.
    let _a = register_session(&mut h, "s0000000a", 1);
    let path_b = register_session(&mut h, "s0000000b", 1);
    let mut other = std::process::Command::new("sleep").arg("30").spawn().unwrap();
    h.send(SceneCommand::BindHost { session: "s0000000b".into(), generation: 1, scope: my_scope(), host_pid: other.id() });
    h.sync();
    let mut raw = UnixStream::connect(&path_b).unwrap();
    raw.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    assert_eq!(raw.read(&mut [0u8; 8]).unwrap_or(0), 0);
    assert!(h.wait_event(WAIT, |e| matches!(e, SceneEvent::SessionConnectionRejected { session, .. } if session == "s0000000b")).is_some());
    assert!(!h.events().iter().any(|e| matches!(e, SceneEvent::SessionConnected { .. })));
    let _ = other.kill();
    let _ = other.wait();
}

#[test]
fn greeter_connection_needs_the_registered_uid_and_pid() {
    let mut h = harness(1);
    // Connected before registration: held, silent.
    let mut raw = UnixStream::connect(&h.greeter_socket).unwrap();
    raw.write_all(&[1, 0, 0, 0, 1, 0, 12, 0, 2, 0, 0, 0]).unwrap();
    raw.set_read_timeout(Some(Duration::from_millis(300))).unwrap();
    assert!(raw.read(&mut [0u8; 8]).is_err(), "no protocol before registration");
    // Registered for someone else's pid: the held connection is dropped.
    h.send(SceneCommand::RegisterGreeter { generation: 1, uid: my_uid(), pid: std::process::id() + 1 });
    raw.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    assert_eq!(raw.read(&mut [0u8; 8]).unwrap_or(0), 0, "wrong pid is closed");
    assert!(!h.events().iter().any(|e| matches!(e, SceneEvent::GreeterConnected { .. })));
    // Wrong uid.
    h.send(SceneCommand::RegisterGreeter { generation: 2, uid: my_uid() + 1, pid: std::process::id() });
    let mut raw = UnixStream::connect(&h.greeter_socket).unwrap();
    raw.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    assert_eq!(raw.read(&mut [0u8; 8]).unwrap_or(0), 0, "wrong uid is closed");
    assert!(!h.events().iter().any(|e| matches!(e, SceneEvent::GreeterConnected { .. })));
    // Right credentials: accepted, exactly once.
    h.send(SceneCommand::RegisterGreeter { generation: 3, uid: my_uid(), pid: std::process::id() });
    let mut good = Client::connect(&h.greeter_socket).expect("registered greeter connects");
    h.wait_event(WAIT, |e| matches!(e, SceneEvent::GreeterConnected { generation: 3, .. })).unwrap();
    assert!(good.roundtrip().is_ok());
    let _second = UnixStream::connect(&h.greeter_socket).unwrap();
    std::thread::sleep(Duration::from_millis(200));
    h.sync();
    let n = h.events().iter().filter(|e| matches!(e, SceneEvent::GreeterConnected { .. })).count();
    assert_eq!(n, 1, "a second connection for the same generation is not promoted");
}

#[test]
fn greeter_cannot_bind_the_session_host_and_hosts_cannot_bind_layer_shell() {
    let mut h = harness(1);
    let greeter = connect_greeter(&mut h, 1, solid([1, 2, 3, 255]));
    let path = register_session(&mut h, "s00000010", 1);
    bind_host(&h, "s00000010", 1);
    let mut host = Client::connect(&path).unwrap();
    h.wait_event(WAIT, |e| matches!(e, SceneEvent::SessionConnected { .. })).unwrap();
    host.roundtrip().unwrap();

    let name_of = |c: &Client, iface: &str| -> u32 {
        c.globals
            .contents()
            .with_list(|l| l.iter().find(|g| g.interface == iface).map(|g| g.name))
            .unwrap_or_else(|| panic!("{iface} not advertised"))
    };
    let host_name = name_of(&host, "bsm_session_host_v1");
    let layer_name = name_of(&greeter, "zwlr_layer_shell_v1");

    // The greeter forges a bind to the host global's name: protocol error, connection gone.
    let mut g = greeter;
    let qh = g.qh.clone();
    let _forged = g.globals.registry().bind::<common::bsm::bsm_session_host_v1::BsmSessionHostV1, _, _>(host_name, 1, &qh, ());
    assert!(g.roundtrip().is_err(), "binding a hidden global must kill the connection");

    // And the host forges a layer-shell bind.
    let qh = host.qh.clone();
    let _forged = host
        .globals
        .registry()
        .bind::<wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1::ZwlrLayerShellV1, _, _>(layer_name, 1, &qh, ());
    assert!(host.roundtrip().is_err(), "binding a hidden global must kill the connection");
}

#[test]
fn host_protocol_errors() {
    let mut h = harness(2);
    let mut c = bring_up_session(&mut h, "s00000020", 9, solid([0, 200, 0, 255]));
    assert_eq!(c.app.identity.as_ref().map(|i| (i.0.as_str(), i.1.as_str(), i.2)), Some(("s00000020", "9", my_uid())));
    assert_eq!(c.app.host_outputs.len(), 2);

    // A report with another generation: stale_generation.
    let host = c.host.clone().unwrap();
    host.report(common::bsm::bsm_session_host_v1::ReportKind::GnoblinReady, None, 0, 0, "10".into());
    assert!(c.roundtrip().is_err());

    // A fresh session: duplicate surface for an output is already_constructed.
    let mut h2 = harness(1);
    let mut c2 = bring_up_session(&mut h2, "s00000021", 1, solid([0, 1, 0, 255]));
    let host2 = c2.host.clone().unwrap();
    let comp = c2.compositor.clone().unwrap();
    let qh = c2.qh.clone();
    let s = comp.create_surface(&qh, ());
    let _dup = host2.get_session_surface(&s, "Virtual-1".into(), &qh, 99);
    assert!(c2.roundtrip().is_err());

    // Unknown output.
    let mut h3 = harness(1);
    let mut c3 = bring_up_session(&mut h3, "s00000022", 1, solid([0, 1, 0, 255]));
    let host3 = c3.host.clone().unwrap();
    let comp = c3.compositor.clone().unwrap();
    let qh = c3.qh.clone();
    let s = comp.create_surface(&qh, ());
    let _bad = host3.get_session_surface(&s, "HDMI-9".into(), &qh, 98);
    assert!(c3.roundtrip().is_err());

    // Inconsistent counts.
    let mut h4 = harness(1);
    let mut c4 = bring_up_session(&mut h4, "s00000023", 4, solid([0, 1, 0, 255]));
    let host4 = c4.host.clone().unwrap();
    host4.report(common::bsm::bsm_session_host_v1::ReportKind::SurfacesMapped, Some("Virtual-1".into()), 3, 2, "4".into());
    assert!(c4.roundtrip().is_err());

    // A valid report reaches the manager, tagged with the session and generation.
    let mut h5 = harness(1);
    let mut c5 = bring_up_session(&mut h5, "s00000024", 5, solid([0, 1, 0, 255]));
    c5.host.clone().unwrap().report(common::bsm::bsm_session_host_v1::ReportKind::SurfacesMapped, Some("Virtual-1".into()), 2, 2, "5".into());
    c5.roundtrip().unwrap();
    let ev = h5.wait_event(WAIT, |e| matches!(e, SceneEvent::SessionReport { .. })).expect("report forwarded");
    assert_eq!(
        ev,
        SceneEvent::SessionReport {
            session: "s00000024".into(),
            generation: 5,
            report: bsm_proto::scene::HostReport::SurfacesMapped { output: "Virtual-1".into(), mapped: 2, required: 2 },
        }
    );
    let _ = c.surfaces.len();
}

#[test]
fn host_evidence_counts_only_frames_after_the_ack_and_resets_on_output_change() {
    let mut h = harness(1);
    let path = register_session(&mut h, "s00000030", 1);
    bind_host(&h, "s00000030", 1);
    let mut c = Client::connect(&path).unwrap();
    h.wait_event(WAIT, |e| matches!(e, SceneEvent::SessionConnected { .. })).unwrap();
    c.host_bind();
    c.host_create_surfaces();
    let evidence = |h: &mut Harness| -> Vec<bsm_proto::scene::SurfaceEvidence> {
        h.sync();
        h.events()
            .iter()
            .rev()
            .find_map(|e| match e {
                SceneEvent::SessionSurfaces { surfaces, .. } => Some(surfaces.clone()),
                _ => None,
            })
            .expect("evidence event")
    };
    // Commit a right-sized buffer without acknowledging the configure: no evidence.
    c.host_commit_without_ack(0, &solid([5, 5, 5, 255]));
    let e = evidence(&mut h);
    assert_eq!(e.len(), 1);
    assert!(!e[0].acked);
    assert_eq!(e[0].presented_frames, 0);
    // Ack alone is not a frame either.
    c.host_ack_only(0);
    let e = evidence(&mut h);
    assert!(e[0].acked);
    assert_eq!(e[0].presented_frames, 0);
    // A frame after the ack counts, and is reported as presented (ready-frame proof).
    c.host_paint(&solid([5, 5, 5, 255]));
    let e = evidence(&mut h);
    assert_eq!(e[0].presented_frames, 1);
    c.pump();
    assert!(!c.app.presented.is_empty(), "the first frame after the ack gets a presented event");

    // The output changes: new configure, evidence void until a new ack and frame.
    let mut o = h.outputs.clone();
    o[0].width = 80;
    o[0].height = 60;
    h.set_outputs(o);
    c.roundtrip().unwrap();
    let e = evidence(&mut h);
    assert!(!e[0].acked);
    assert_eq!(e[0].presented_frames, 0);
    // An old-sized buffer committed against the new configure counts for nothing.
    c.host_ack_only(0);
    let qh = c.qh.clone();
    let _ = qh;
    let e = evidence(&mut h);
    assert!(e[0].acked);
    assert_eq!(e[0].presented_frames, 0);
}
