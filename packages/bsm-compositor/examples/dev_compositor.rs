//! Manual smoke: bsm's compositor in a window of the running Wayland desktop, with a
//! coloured fake greeter and a fake hosted session driven through a scripted
//! sequence: greeter, new login fade, lock, unlock slide, power fade and cancel.
//!
//! ```text
//! cargo run --example dev_compositor            # default script, ~16 s
//! cargo run --example dev_compositor -- --hold  # greeter only, runs until closed
//! ```
//!
//! Nothing here is part of the product. The fake clients use this process's own
//! credentials, which is exactly what the registration commands check.

#[path = "../tests/common/mod.rs"]
mod common;

use std::sync::mpsc;
use std::time::Duration;

use bsm_compositor::calloop;
use bsm_compositor::{Backend, Config};
use bsm_proto::scene::{SceneCommand, SceneEvent, TransitionPhaseEvent};
use bsm_proto::state::{InputOwner, TransitionKind};
use common::{Client, Fill, bind_host_cmd, my_exe, my_scope, my_uid, solid};

const SID: &str = "s0dev0001";
const GEN: u64 = 1;

fn greeter_fill() -> Fill {
    Box::new(|x, y, w, h| {
        let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
        let band = y < h / 8;
        if band {
            [220, 80, 70, 255]
        } else {
            [(30.0 + 40.0 * fx) as u8, (50.0 + 40.0 * fy) as u8, 190, 255]
        }
    })
}

fn session_fill() -> Fill {
    Box::new(|x, y, w, h| {
        let cell = ((x * 8 / w) + (y * 6 / h)) % 2 == 0;
        if cell { [40, 170, 80, 255] } else { [30, 130, 70, 255] }
    })
}

fn wait(evt: &mpsc::Receiver<SceneEvent>, what: &str, pred: impl Fn(&SceneEvent) -> bool) -> SceneEvent {
    loop {
        match evt.recv_timeout(Duration::from_secs(20)) {
            Ok(e) if pred(&e) => return e,
            Ok(_) => {}
            Err(_) => panic!("timed out waiting for {what}"),
        }
    }
}

fn transition(
    cmd: &calloop::channel::Sender<SceneCommand>,
    op: u64,
    kind: TransitionKind,
    source: bool,
    target: bool,
    ms: u32,
) {
    cmd.send(SceneCommand::BeginTransition {
        operation: op,
        kind,
        source: source.then(|| (SID.to_string(), GEN)),
        target: target.then(|| (SID.to_string(), GEN)),
        duration_ms: ms,
        reduce_motion: false,
        power: None,
    })
    .unwrap();
}

fn script(cmd: calloop::channel::Sender<SceneCommand>, evt: mpsc::Receiver<SceneEvent>, greeter_sock: std::path::PathBuf, hold: bool) {
    wait(&evt, "Ready", |e| matches!(e, SceneEvent::Ready { .. }));
    cmd.send(SceneCommand::RegisterGreeter { generation: 1, uid: my_uid(), pid: std::process::id() }).unwrap();
    let mut greeter = Client::connect(&greeter_sock).expect("greeter connects");
    wait(&evt, "GreeterConnected", |e| matches!(e, SceneEvent::GreeterConnected { .. }));
    greeter.greeter_map(greeter_fill());
    cmd.send(SceneCommand::ShowGreeterOnly).unwrap();
    cmd.send(SceneCommand::SetInputOwner(InputOwner::Greeter)).unwrap();
    if hold {
        // Keep the greeter client alive until the window is closed.
        loop {
            greeter.pump();
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    std::thread::sleep(Duration::from_secs(2));

    // Bring the session up in the background.
    cmd.send(SceneCommand::RegisterSession {
        session: SID.into(),
        generation: GEN,
        uid: my_uid(),
        scope: String::new(),
        host_pid: 0,
        host_exe: my_exe(),
    })
    .unwrap();
    let SceneEvent::SessionSocketReady { path, .. } =
        wait(&evt, "SessionSocketReady", |e| matches!(e, SceneEvent::SessionSocketReady { .. }))
    else {
        unreachable!()
    };
    cmd.send(bind_host_cmd(SID, GEN, my_scope(), std::process::id())).unwrap();
    let mut host = Client::connect(std::path::Path::new(&path)).expect("host connects");
    wait(&evt, "SessionConnected", |e| matches!(e, SceneEvent::SessionConnected { .. }));
    host.host_bind();
    host.host_create_surfaces();
    host.host_paint(&session_fill());

    println!("new login fade");
    transition(&cmd, 1, TransitionKind::NewLoginFade, false, true, 420);
    wait(&evt, "Completed", |e| matches!(e, SceneEvent::TransitionPhase { operation: 1, phase: TransitionPhaseEvent::Completed }));
    cmd.send(SceneCommand::SetInputOwner(InputOwner::Session(SID.into()))).unwrap();
    std::thread::sleep(Duration::from_secs(2));

    println!("lock");
    cmd.send(SceneCommand::Protect { session: SID.into(), generation: GEN }).unwrap();
    wait(&evt, "Protected", |e| matches!(e, SceneEvent::Protected { .. }));
    transition(&cmd, 2, TransitionKind::Lock, true, false, 240);
    wait(&evt, "Completed", |e| matches!(e, SceneEvent::TransitionPhase { operation: 2, phase: TransitionPhaseEvent::Completed }));
    cmd.send(SceneCommand::SetInputOwner(InputOwner::Greeter)).unwrap();
    std::thread::sleep(Duration::from_secs(2));

    println!("unlock slide up");
    transition(&cmd, 3, TransitionKind::UnlockSlideUp, false, true, 520);
    wait(&evt, "Committed", |e| matches!(e, SceneEvent::TransitionPhase { operation: 3, phase: TransitionPhaseEvent::Committed }));
    cmd.send(SceneCommand::SetInputOwner(InputOwner::Session(SID.into()))).unwrap();
    std::thread::sleep(Duration::from_secs(2));

    println!("power fade, then refused");
    transition(&cmd, 4, TransitionKind::PowerFade, true, false, 360);
    wait(&evt, "Committed", |e| matches!(e, SceneEvent::TransitionPhase { operation: 4, phase: TransitionPhaseEvent::Committed }));
    std::thread::sleep(Duration::from_millis(1500));
    cmd.send(SceneCommand::CancelTransition { operation: 4 }).unwrap();
    wait(&evt, "Cancelled", |e| matches!(e, SceneEvent::TransitionPhase { operation: 4, phase: TransitionPhaseEvent::Cancelled }));
    std::thread::sleep(Duration::from_secs(2));
    println!("done");
    drop((greeter, host));
    // Dropping the command sender stops the compositor.
}

fn main() -> anyhow::Result<()> {
    let filter = tracing_subscriber::EnvFilter::try_from_env("RUST_LOG")
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).with_writer(std::io::stderr).try_init();
    let hold = std::env::args().any(|a| a == "--hold");
    let dir = std::env::temp_dir().join(format!("bsm-dev-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let greeter_sock = dir.join("greeter.sock");
    let cfg = Config {
        backend: Backend::Winit,
        runtime_dir: dir.clone(),
        greeter_socket: greeter_sock.clone(),
        session_socket_dir: dir.join("sessions"),
        ..Config::default()
    };
    let (cmd_tx, cmd_rx) = calloop::channel::channel::<SceneCommand>();
    let (evt_tx, evt_rx) = mpsc::channel();
    let driver = std::thread::spawn(move || script(cmd_tx, evt_rx, greeter_sock, hold));
    let _ = solid;
    let r = bsm_compositor::run(cfg, cmd_rx, evt_tx);
    let _ = driver.join();
    let _ = std::fs::remove_dir_all(&dir);
    r
}
