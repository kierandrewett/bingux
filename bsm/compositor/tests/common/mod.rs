//! Test clients for the bsm compositor: a Quickshell-like greeter (layer surfaces) and
//! a session bridge (`bsm_session_host_v1` surfaces), both with shm buffers of
//! solid or banded colour, plus helpers to register them with a [`Harness`].
#![allow(dead_code)]

use std::collections::HashMap;
use std::io::Write;
use std::os::fd::AsFd;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use bsm_compositor::{CapturedFrame, Harness};
use bsm_proto::scene::SceneCommand;
use wayland_client::globals::{GlobalList, GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_buffer::WlBuffer;
use wayland_client::protocol::wl_callback::WlCallback;
use wayland_client::protocol::wl_compositor::WlCompositor;
use wayland_client::protocol::wl_keyboard::{self, WlKeyboard};
use wayland_client::protocol::wl_output::{self, WlOutput};
use wayland_client::protocol::wl_pointer::{self, WlPointer};
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::protocol::wl_seat::{self, WlSeat};
use wayland_client::protocol::wl_shm::{self, WlShm};
use wayland_client::protocol::wl_shm_pool::WlShmPool;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle, WEnum};
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1::{self, ZwlrLayerShellV1};
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::{
    self, Anchor, KeyboardInteractivity, ZwlrLayerSurfaceV1,
};

use wayland_protocols::xdg::shell::client::xdg_surface::{self, XdgSurface};
use wayland_protocols::xdg::shell::client::xdg_toplevel::{self, XdgToplevel};
use wayland_protocols::xdg::shell::client::xdg_wm_base::{self, XdgWmBase};

impl Dispatch<XdgWmBase, ()> for App {
    fn event(_: &mut Self, base: &XdgWmBase, ev: xdg_wm_base::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let xdg_wm_base::Event::Ping { serial } = ev {
            base.pong(serial);
        }
    }
}
impl Dispatch<XdgSurface, ()> for App {
    fn event(app: &mut Self, s: &XdgSurface, ev: xdg_surface::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let xdg_surface::Event::Configure { serial } = ev {
            s.ack_configure(serial);
            app.xdg_configures += 1;
        }
    }
}
impl Dispatch<XdgToplevel, ()> for App {
    fn event(_: &mut Self, _: &XdgToplevel, _: xdg_toplevel::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

pub mod bsm {
    use wayland_client;
    use wayland_client::protocol::*;
    pub mod __interfaces {
        use wayland_client::backend as wayland_backend;
        use wayland_client::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("../core/protocol/bsm-session-host-v1.xml");
    }
    use self::__interfaces::*;
    wayland_scanner::generate_client_code!("../core/protocol/bsm-session-host-v1.xml");
}
use bsm::bsm_session_host_v1::{self, BsmSessionHostV1};
use bsm::bsm_session_surface_v1::{self, BsmSessionSurfaceV1};

pub type Rgba = [u8; 4];

#[derive(Debug, Clone, PartialEq)]
pub enum KbEvent {
    Enter,
    Leave,
    Key(u32, bool),
}

#[derive(Debug, Clone, PartialEq)]
pub enum PtrEvent {
    Enter,
    Leave,
    Motion,
    Button(u32, bool),
}

#[derive(Default)]
pub struct App {
    pub outputs: Vec<(WlOutput, String)>,
    pub kb: Vec<KbEvent>,
    pub ptr: Vec<PtrEvent>,
    pub layer_cfg: HashMap<u32, (u32, u32, u32)>, // surface proto id -> (serial, w, h)
    pub session_cfg: HashMap<u32, (u32, u32, u32, u32)>, // surface proto id -> (serial, w, h, scale_120)
    pub identity: Option<(String, String, u32)>,
    pub host_outputs: Vec<(String, u32, u32)>,
    pub removed_outputs: Vec<String>,
    pub input_state: Vec<u32>,
    pub presented: Vec<(u32, u32)>, // (surface proto id, seq)
    pub closed: bool,
    pub releases: u32,
    /// Keys listed by each wl_keyboard.enter, in order.
    pub enter_keys: Vec<Vec<u32>>,
    pub xdg_configures: u32,
    pub layer_closed: u32,
    seat: Option<WlSeat>,
    keyboard: Option<WlKeyboard>,
    pointer: Option<WlPointer>,
}

impl Dispatch<WlRegistry, GlobalListContents> for App {
    fn event(_: &mut Self, _: &WlRegistry, _: wayland_client::protocol::wl_registry::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}
macro_rules! ignore_events {
    ($($ty:ty),*) => { $(impl Dispatch<$ty, ()> for App {
        fn event(_: &mut Self, _: &$ty, _: <$ty as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
    })* };
}
ignore_events!(WlCompositor, WlShm, WlShmPool, WlSurface, WlCallback, ZwlrLayerShellV1);

impl Dispatch<WlBuffer, ()> for App {
    fn event(app: &mut Self, _: &WlBuffer, ev: wayland_client::protocol::wl_buffer::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let wayland_client::protocol::wl_buffer::Event::Release = ev {
            app.releases += 1;
        }
    }
}

impl Dispatch<WlOutput, usize> for App {
    fn event(app: &mut Self, _: &WlOutput, ev: wl_output::Event, idx: &usize, _: &Connection, _: &QueueHandle<Self>) {
        if let wl_output::Event::Name { name } = ev {
            if let Some(o) = app.outputs.get_mut(*idx) {
                o.1 = name;
            }
        }
    }
}

impl Dispatch<WlSeat, ()> for App {
    fn event(app: &mut Self, seat: &WlSeat, ev: wl_seat::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        if let wl_seat::Event::Capabilities { capabilities: WEnum::Value(c) } = ev {
            if c.contains(wl_seat::Capability::Keyboard) && app.keyboard.is_none() {
                app.keyboard = Some(seat.get_keyboard(qh, ()));
            }
            if c.contains(wl_seat::Capability::Pointer) && app.pointer.is_none() {
                app.pointer = Some(seat.get_pointer(qh, ()));
            }
        }
    }
}

impl Dispatch<WlKeyboard, ()> for App {
    fn event(app: &mut Self, _: &WlKeyboard, ev: wl_keyboard::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match ev {
            wl_keyboard::Event::Enter { keys, .. } => {
                app.enter_keys.push(keys.chunks_exact(4).map(|c| u32::from_ne_bytes([c[0], c[1], c[2], c[3]])).collect());
                app.kb.push(KbEvent::Enter)
            }
            wl_keyboard::Event::Leave { .. } => app.kb.push(KbEvent::Leave),
            wl_keyboard::Event::Key { key, state, .. } => {
                app.kb.push(KbEvent::Key(key, matches!(state, WEnum::Value(wl_keyboard::KeyState::Pressed))))
            }
            _ => {}
        }
    }
}

impl Dispatch<WlPointer, ()> for App {
    fn event(app: &mut Self, _: &WlPointer, ev: wl_pointer::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match ev {
            wl_pointer::Event::Enter { .. } => app.ptr.push(PtrEvent::Enter),
            wl_pointer::Event::Leave { .. } => app.ptr.push(PtrEvent::Leave),
            wl_pointer::Event::Motion { .. } => app.ptr.push(PtrEvent::Motion),
            wl_pointer::Event::Button { button, state, .. } => {
                app.ptr.push(PtrEvent::Button(button, matches!(state, WEnum::Value(wl_pointer::ButtonState::Pressed))))
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwlrLayerSurfaceV1, u32> for App {
    fn event(app: &mut Self, _: &ZwlrLayerSurfaceV1, ev: zwlr_layer_surface_v1::Event, id: &u32, _: &Connection, _: &QueueHandle<Self>) {
        match ev {
            zwlr_layer_surface_v1::Event::Configure { serial, width, height } => {
                app.layer_cfg.insert(*id, (serial, width, height));
            }
            zwlr_layer_surface_v1::Event::Closed => app.layer_closed += 1,
            _ => {}
        }
    }
}

impl Dispatch<BsmSessionHostV1, ()> for App {
    fn event(app: &mut Self, _: &BsmSessionHostV1, ev: bsm_session_host_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        use bsm_session_host_v1::Event::*;
        match ev {
            Identity { session_id, generation, uid } => app.identity = Some((session_id, generation, uid)),
            OutputAdded { name, width, height, .. } => app.host_outputs.push((name, width, height)),
            OutputChanged { name, width, height, .. } => {
                if let Some(o) = app.host_outputs.iter_mut().find(|o| o.0 == name) {
                    *o = (name, width, height);
                }
            }
            OutputRemoved { name } => {
                app.host_outputs.retain(|o| o.0 != name);
                app.removed_outputs.push(name);
            }
            InputState { active } => app.input_state.push(active),
            Closed => app.closed = true,
        }
    }
}

impl Dispatch<BsmSessionSurfaceV1, u32> for App {
    fn event(app: &mut Self, _: &BsmSessionSurfaceV1, ev: bsm_session_surface_v1::Event, id: &u32, _: &Connection, _: &QueueHandle<Self>) {
        match ev {
            bsm_session_surface_v1::Event::Configure { serial, width, height, scale_120, .. } => {
                app.session_cfg.insert(*id, (serial, width, height, scale_120));
            }
            bsm_session_surface_v1::Event::Presented { seq, .. } => app.presented.push((*id, seq)),
        }
    }
}

/// Fill function: `(x, y, width, height) -> RGBA`.
pub type Fill = Box<dyn Fn(u32, u32, u32, u32) -> Rgba>;

pub fn solid(c: Rgba) -> Fill {
    Box::new(move |_, _, _, _| c)
}

fn make_buffer(
    shm: &WlShm,
    qh: &QueueHandle<App>,
    w: u32,
    h: u32,
    fill: &Fill,
) -> WlBuffer {
    let mut bytes = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let [r, g, b, a] = fill(x, y, w, h);
            // Argb8888 is premultiplied by convention.
            let m = |c: u8| ((c as u16 * a as u16 + 127) / 255) as u8;
            bytes.extend_from_slice(&[m(b), m(g), m(r), a]);
        }
    }
    let mut f = tempfile::tempfile().expect("tempfile");
    f.write_all(&bytes).unwrap();
    let pool = shm.create_pool(f.as_fd(), bytes.len() as i32, qh, ());
    let buf = pool.create_buffer(0, w as i32, h as i32, (w * 4) as i32, wl_shm::Format::Argb8888, qh, ());
    pool.destroy();
    buf
}

pub struct Client {
    pub conn: Connection,
    pub queue: EventQueue<App>,
    pub app: App,
    pub globals: GlobalList,
    pub qh: QueueHandle<App>,
    pub compositor: Option<WlCompositor>,
    pub shm: Option<WlShm>,
    pub surfaces: Vec<(String, WlSurface)>,
    layer_surfaces: Vec<ZwlrLayerSurfaceV1>,
    session_surfaces: Vec<BsmSessionSurfaceV1>,
    pub host: Option<BsmSessionHostV1>,
    /// Keyboard interactivity the first greeter layer surface asks for.
    pub kb_mode: KeyboardInteractivity,
}

impl Client {
    pub fn connect(path: &Path) -> Result<Client, String> {
        let stream = UnixStream::connect(path).map_err(|e| format!("connect: {e}"))?;
        Self::from_stream(stream)
    }

    pub fn from_stream(stream: UnixStream) -> Result<Client, String> {
        let conn = Connection::from_socket(stream).map_err(|e| format!("{e}"))?;
        let (globals, queue) = registry_queue_init::<App>(&conn).map_err(|e| format!("registry: {e}"))?;
        let qh = queue.handle();
        Ok(Client {
            conn,
            queue,
            app: App::default(),
            globals,
            qh,
            compositor: None,
            shm: None,
            surfaces: Vec::new(),
            layer_surfaces: Vec::new(),
            session_surfaces: Vec::new(),
            host: None,
            kb_mode: KeyboardInteractivity::Exclusive,
        })
    }

    /// `(interface, count)` of every advertised global, sorted.
    pub fn interfaces(&self) -> Vec<(String, usize)> {
        let mut m: std::collections::BTreeMap<String, usize> = Default::default();
        self.globals.contents().with_list(|l| {
            for g in l {
                if g.interface != "wl_display" {
                    *m.entry(g.interface.clone()).or_default() += 1;
                }
            }
        });
        m.into_iter().collect()
    }

    pub fn roundtrip(&mut self) -> Result<(), String> {
        self.queue.roundtrip(&mut self.app).map(|_| ()).map_err(|e| format!("{e}"))
    }

    pub fn pump(&mut self) {
        let _ = self.queue.dispatch_pending(&mut self.app);
        let _ = self.queue.flush();
        if let Some(g) = self.conn.prepare_read() {
            let _ = g.read();
        }
        let _ = self.queue.dispatch_pending(&mut self.app);
    }

    /// Bind wl_compositor, wl_shm, the seat and every wl_output (v4, for names).
    pub fn bind_basics(&mut self) {
        let qh = self.qh.clone();
        self.compositor = self.globals.bind::<WlCompositor, _, _>(&qh, 1..=6, ()).ok();
        self.shm = self.globals.bind::<WlShm, _, _>(&qh, 1..=1, ()).ok();
        let _seat = self.globals.bind::<WlSeat, _, _>(&qh, 1..=7, ()).ok();
        let names: Vec<u32> = self.globals.contents().with_list(|l| {
            l.iter().filter(|g| g.interface == "wl_output").map(|g| g.name).collect()
        });
        for (idx, name) in names.iter().enumerate() {
            let reg = self.globals.registry();
            let o = reg.bind::<WlOutput, _, _>(*name, 4, &qh, idx);
            self.app.outputs.push((o, String::new()));
        }
        self.app.seat = _seat;
        self.roundtrip().expect("roundtrip after bind");
        self.roundtrip().expect("second roundtrip for seat caps");
        self.app.outputs.sort_by(|a, b| a.1.cmp(&b.1));
    }

    // ----- greeter behaviour -----

    /// One full-output background layer surface per output, painted with `fill`.
    /// The first one asks for exclusive keyboard interactivity.
    pub fn greeter_map(&mut self, fill: Fill) {
        self.bind_basics();
        let qh = self.qh.clone();
        let shell: ZwlrLayerShellV1 = self.globals.bind(&qh, 1..=4, ()).expect("layer shell");
        let compositor = self.compositor.clone().unwrap();
        let outs: Vec<_> = self.app.outputs.iter().map(|o| (o.0.clone(), o.1.clone())).collect();
        for (i, (out, name)) in outs.iter().enumerate() {
            let surface = compositor.create_surface(&qh, ());
            let id = wayland_client::Proxy::id(&surface).protocol_id();
            let ls = shell.get_layer_surface(
                &surface,
                Some(out),
                zwlr_layer_shell_v1::Layer::Background,
                "greeter".into(),
                &qh,
                id,
            );
            ls.set_anchor(Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right);
            ls.set_size(0, 0);
            ls.set_exclusive_zone(-1);
            ls.set_keyboard_interactivity(if i == 0 { self.kb_mode } else { KeyboardInteractivity::None });
            surface.commit();
            self.surfaces.push((name.clone(), surface));
            self.layer_surfaces.push(ls);
        }
        self.roundtrip().expect("layer configure");
        for (i, (_, surface)) in self.surfaces.clone().iter().enumerate() {
            let id = wayland_client::Proxy::id(surface).protocol_id();
            let (serial, w, h) = *self.app.layer_cfg.get(&id).expect("configure received");
            self.layer_surfaces[i].ack_configure(serial);
            let buf = make_buffer(self.shm.as_ref().unwrap(), &qh, w, h, &fill);
            surface.attach(Some(&buf), 0, 0);
            surface.damage_buffer(0, 0, w as i32, h as i32);
            surface.commit();
        }
        self.roundtrip().expect("greeter commit");
    }

    /// Repaint every greeter surface with a new fill (a new committed buffer).
    pub fn greeter_repaint(&mut self, fill: Fill) {
        let qh = self.qh.clone();
        for (_, surface) in self.surfaces.clone() {
            let id = wayland_client::Proxy::id(&surface).protocol_id();
            let (_, w, h) = self.app.layer_cfg[&id];
            let buf = make_buffer(self.shm.as_ref().unwrap(), &qh, w, h, &fill);
            surface.attach(Some(&buf), 0, 0);
            surface.damage_buffer(0, 0, w as i32, h as i32);
            surface.commit();
        }
        self.roundtrip().unwrap();
    }

    /// What Qt does at start-up: bind xdg_wm_base and create a toplevel, paint it.
    pub fn xdg_toplevel_paint(&mut self, fill: &Fill) {
        let qh = self.qh.clone();
        let base: XdgWmBase = self.globals.bind(&qh, 1..=6, ()).expect("xdg_wm_base");
        let surface = self.compositor.clone().unwrap().create_surface(&qh, ());
        let xs = base.get_xdg_surface(&surface, &qh, ());
        let _tl = xs.get_toplevel(&qh, ());
        surface.commit();
        self.roundtrip().expect("toplevel configure");
        let buf = make_buffer(self.shm.as_ref().unwrap(), &qh, 64, 48, fill);
        surface.attach(Some(&buf), 0, 0);
        surface.damage_buffer(0, 0, 64, 48);
        surface.commit();
        self.roundtrip().expect("toplevel commit");
    }

    // ----- session bridge behaviour -----

    /// Bind the host global, wait for identity and outputs.
    pub fn host_bind(&mut self) {
        self.bind_basics();
        let qh = self.qh.clone();
        self.host = Some(self.globals.bind::<BsmSessionHostV1, _, _>(&qh, 1..=1, ()).expect("bsm_session_host_v1"));
        self.roundtrip().expect("identity");
    }

    /// Create a session surface for every output the host announced.
    pub fn host_create_surfaces(&mut self) {
        let qh = self.qh.clone();
        let compositor = self.compositor.clone().unwrap();
        let host = self.host.clone().unwrap();
        let names: Vec<String> = self.app.host_outputs.iter().map(|o| o.0.clone()).collect();
        for name in names {
            let surface = compositor.create_surface(&qh, ());
            let id = wayland_client::Proxy::id(&surface).protocol_id();
            let ss = host.get_session_surface(&surface, name.clone(), &qh, id);
            self.surfaces.push((name, surface));
            self.session_surfaces.push(ss);
        }
        self.roundtrip().expect("session configure");
    }

    /// Create one more session surface (for an output that appeared later).
    pub fn host_add_surface(&mut self, name: &str) {
        let qh = self.qh.clone();
        let compositor = self.compositor.clone().unwrap();
        let host = self.host.clone().unwrap();
        let surface = compositor.create_surface(&qh, ());
        let id = wayland_client::Proxy::id(&surface).protocol_id();
        let ss = host.get_session_surface(&surface, name.to_string(), &qh, id);
        self.surfaces.push((name.to_string(), surface));
        self.session_surfaces.push(ss);
        self.roundtrip().expect("session configure");
    }

    /// Ack the latest configure of every session surface and commit a buffer of exactly
    /// that size painted with `fill`.
    pub fn host_paint(&mut self, fill: &Fill) {
        let qh = self.qh.clone();
        for (i, (_, surface)) in self.surfaces.clone().iter().enumerate() {
            let id = wayland_client::Proxy::id(surface).protocol_id();
            let (serial, w, h, _) = self.app.session_cfg[&id];
            self.session_surfaces[i].ack_configure(serial);
            let buf = make_buffer(self.shm.as_ref().unwrap(), &qh, w, h, fill);
            surface.attach(Some(&buf), 0, 0);
            surface.damage_buffer(0, 0, w as i32, h as i32);
            surface.commit();
        }
        self.roundtrip().expect("session commit");
    }

    /// Commit a buffer on output `idx` without acking (evidence must not count).
    pub fn host_commit_without_ack(&mut self, idx: usize, fill: &Fill) {
        let qh = self.qh.clone();
        let surface = self.surfaces[idx].1.clone();
        let id = wayland_client::Proxy::id(&surface).protocol_id();
        let (_, w, h, _) = self.app.session_cfg[&id];
        let buf = make_buffer(self.shm.as_ref().unwrap(), &qh, w, h, fill);
        surface.attach(Some(&buf), 0, 0);
        surface.damage_buffer(0, 0, w as i32, h as i32);
        surface.commit();
        self.roundtrip().unwrap();
    }

    pub fn host_ack_only(&mut self, idx: usize) {
        let surface = self.surfaces[idx].1.clone();
        let id = wayland_client::Proxy::id(&surface).protocol_id();
        let (serial, ..) = self.app.session_cfg[&id];
        self.session_surfaces[idx].ack_configure(serial);
        self.roundtrip().unwrap();
    }
}

// ----- harness side helpers -----

pub fn my_uid() -> u32 {
    rustix::process::getuid().as_raw()
}

pub fn my_scope() -> String {
    let cg = std::fs::read_to_string("/proc/self/cgroup").unwrap();
    let path = cg.lines().next().unwrap().rsplit(':').next().unwrap().to_string();
    if path.is_empty() { "/".into() } else { path }
}

pub fn my_exe() -> String {
    std::fs::read_link("/proc/self/exe").unwrap().display().to_string()
}

pub const WAIT: Duration = Duration::from_secs(5);

pub fn artifacts(test: &str) -> PathBuf {
    let base = std::env::var_os("BSM_TEST_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("bsm-compositor-artifacts"));
    let p = base.join(test);
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// Register the greeter with this process's credentials and connect a client to it.
pub fn connect_greeter(h: &mut Harness, generation: u64, fill: Fill) -> Client {
    h.send(SceneCommand::RegisterGreeter { generation, uid: my_uid(), pid: std::process::id() });
    let mut c = Client::connect(&h.greeter_socket).expect("greeter connects");
    h.wait_event(WAIT, |e| matches!(e, bsm_proto::scene::SceneEvent::GreeterConnected { generation: g, .. } if *g == generation))
        .expect("GreeterConnected");
    c.greeter_map(fill);
    c
}

pub struct SessionIds {
    pub id: String,
    pub generation: u64,
}

/// RegisterSession + BindHost with this process's credentials; returns the socket path.
pub fn register_session(h: &mut Harness, id: &str, generation: u64) -> PathBuf {
    h.send(SceneCommand::RegisterSession {
        session: id.into(),
        generation,
        uid: my_uid(),
        scope: String::new(),
        host_pid: 0,
        host_exe: my_exe(),
    });
    let ev = h
        .wait_event(WAIT, |e| matches!(e, bsm_proto::scene::SceneEvent::SessionSocketReady { session, .. } if session == id))
        .expect("SessionSocketReady");
    let bsm_proto::scene::SceneEvent::SessionSocketReady { path, .. } = ev else { unreachable!() };
    PathBuf::from(path)
}

pub fn bind_host(h: &Harness, id: &str, generation: u64) {
    h.send(SceneCommand::BindHost {
        session: id.into(),
        generation,
        scope: my_scope(),
        host_pid: std::process::id(),
    });
}

/// Full bring-up of one session: register, connect, bind, surfaces, ack, first frame.
pub fn bring_up_session(h: &mut Harness, id: &str, generation: u64, fill: Fill) -> Client {
    let path = register_session(h, id, generation);
    bind_host(h, id, generation);
    let mut c = Client::connect(&path).expect("host connects");
    h.wait_event(WAIT, |e| matches!(e, bsm_proto::scene::SceneEvent::SessionConnected { session, .. } if session == id))
        .expect("SessionConnected");
    c.host_bind();
    c.host_create_surfaces();
    c.host_paint(&fill);
    c
}

/// Hex-free approximate comparison of two pixels.
pub fn near(a: Rgba, b: Rgba, tol: i32) -> bool {
    (0..4).all(|i| (a[i] as i32 - b[i] as i32).abs() <= tol)
}

pub fn frame_of<'a>(frames: &'a [CapturedFrame], output: &str) -> &'a CapturedFrame {
    frames.iter().find(|f| f.output == output).expect("frame for output")
}

pub fn bind_host_cmd(id: &str, generation: u64, scope: String, host_pid: u32) -> SceneCommand {
    SceneCommand::BindHost { session: id.into(), generation, scope, host_pid }
}
